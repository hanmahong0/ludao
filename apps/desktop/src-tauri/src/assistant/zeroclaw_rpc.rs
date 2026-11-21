//! ZeroClaw v0.9.0 本地 JSON-RPC（NDJSON）客户端（产品端）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub const PINNED_TAG: &str = "v0.9.0";
pub const PINNED_COMMIT: &str = "6c78ba59bce6a3d5079cf0c0310488770e264ad4";
pub const PINNED_LINUX_SHA256: &str =
    "ce9cdfd3d419ff2ee668b0741d4cea41eaee9c637c6f590c3faf3e37b6507db9";
/// 解压后的 linux 可执行文件，不是 tar.gz。Windows 解压二进制未核。
pub const PINNED_BINARY_SHA256: &str =
    "6fe61f4c0288e355081a0a93619b51e7ce572412be99c2e8fb0a59244e0b496b";

#[derive(Debug)]
pub struct RpcLine {
    pub raw: String,
    pub value: Value,
}

pub fn send_line<W: Write>(w: &mut W, v: &Value) -> std::io::Result<()> {
    let line =
        serde_json::to_string(v).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writeln!(w, "{line}")?;
    w.flush()?;
    Ok(())
}

pub fn read_line<R: BufRead>(r: &mut R) -> std::io::Result<Option<RpcLine>> {
    let mut buf = String::new();
    let n = r.read_line(&mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(RpcLine {
        raw: trimmed.to_string(),
        value,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocol_version", alias = "protocolVersion")]
    pub protocol_version: i64,
    #[serde(rename = "server_version", alias = "serverVersion", default)]
    pub server_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub completed: bool,
    pub text_chunks: Vec<String>,
    pub raw_events: usize,
}

#[cfg(unix)]
pub struct RpcClient {
    writer: std::os::unix::net::UnixStream,
    reader: BufReader<std::os::unix::net::UnixStream>,
    next_id: i64,
    pending: VecDeque<RpcLine>,
    pub initialized: bool,
    pub server_version: Option<String>,
}

#[cfg(unix)]
impl RpcClient {
    pub fn connect(socket_path: &Path, timeout: Duration) -> Result<Self, String> {
        use std::os::unix::net::UnixStream;
        let stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("connect {socket_path:?}: {e}"))?;
        stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let reader = BufReader::new(stream);
        Ok(Self {
            writer,
            reader,
            next_id: 1,
            pending: VecDeque::new(),
            initialized: false,
            server_version: None,
        })
    }

    fn read_socket(&mut self) -> Result<RpcLine, String> {
        read_line(&mut self.reader)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "rpc eof".to_string())
    }

    fn read_next(&mut self) -> Result<RpcLine, String> {
        if let Some(line) = self.pending.pop_front() {
            return Ok(line);
        }
        self.read_socket()
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({"jsonrpc":"2.0","method":method,"params":params,"id":id});
        send_line(&mut self.writer, &req).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err("rpc timeout waiting response".into());
            }
            let line = self.read_socket()?;
            if line.value.get("id") == Some(&json!(id)) {
                if let Some(err) = line.value.get("error") {
                    return Err(err.to_string());
                }
                return line
                    .value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| format!("missing result: {}", line.raw));
            }
            if line.value.get("method").and_then(|m| m.as_str()) == Some("session/update") {
                self.pending.push_back(line);
            }
        }
    }

    pub fn initialize(&mut self) -> Result<InitializeResult, String> {
        let result = self.request("initialize", json!({"protocolVersion": 1}))?;
        let parsed: InitializeResult =
            serde_json::from_value(result).map_err(|e| format!("parse initialize: {e}"))?;
        if parsed.protocol_version != 1 {
            return Err(format!("unsupported protocol {}", parsed.protocol_version));
        }
        if parsed.server_version.as_deref() != Some("0.9.0") {
            return Err(format!(
                "unexpected server_version {:?}",
                parsed.server_version
            ));
        }
        self.initialized = true;
        self.server_version = parsed.server_version.clone();
        Ok(parsed)
    }

    pub fn session_new(&mut self, agent_alias: &str) -> Result<String, String> {
        let result = self.request("session/new", json!({"agent_alias": agent_alias}))?;
        result
            .get("session_id")
            .or_else(|| result.get("sessionId"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("session/new missing id: {result}"))
    }

    pub fn session_prompt(&mut self, session_id: &str, text: &str) -> Result<(), String> {
        let _ack = self.request(
            "session/prompt",
            json!({"session_id": session_id, "prompt": text}),
        )?;
        Ok(())
    }

    pub fn wait_turn(
        &mut self,
        session_id: &str,
        timeout: Duration,
    ) -> Result<TurnOutcome, String> {
        let deadline = Instant::now() + timeout;
        let mut text_chunks = Vec::new();
        let mut raw_events = 0usize;
        let mut completed = false;
        while Instant::now() < deadline {
            let line = self.read_next()?;
            raw_events += 1;
            let method = line.value.get("method").and_then(|m| m.as_str());
            if method == Some("session/update") {
                if let Some(params) = line.value.get("params") {
                    let sid = params
                        .get("session_id")
                        .or_else(|| params.get("sessionId"))
                        .and_then(|s| s.as_str());
                    if sid != Some(session_id) {
                        continue;
                    }
                    let ty = params.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if ty == "agent_message_chunk" {
                        if let Some(t) = params.get("text").and_then(|x| x.as_str()) {
                            text_chunks.push(t.to_string());
                        }
                    }
                    if ty == "turn_complete" {
                        let outcome = params.get("outcome").and_then(|o| o.as_str()).unwrap_or("");
                        if outcome == "failed" {
                            let msg = params
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("turn failed");
                            return Err(msg.to_string());
                        }
                        if text_chunks.is_empty() {
                            if let Some(t) = params.get("content").and_then(|c| c.as_str()) {
                                text_chunks.push(t.to_string());
                            }
                        }
                        completed = true;
                        break;
                    }
                }
            }
        }
        Ok(TurnOutcome {
            completed,
            text_chunks,
            raw_events,
        })
    }

    pub fn session_cancel(&mut self, session_id: &str) -> Result<(), String> {
        self.request("session/cancel", json!({"session_id": session_id}))?;
        Ok(())
    }

    pub fn session_state(&mut self, session_id: &str) -> Result<Value, String> {
        self.request("session/state", json!({"session_id": session_id}))
    }
}

#[cfg(unix)]
pub fn handshake_unix(socket_path: &Path, timeout: Duration) -> Result<InitializeResult, String> {
    let mut c = RpcClient::connect(socket_path, timeout)?;
    c.initialize()
}

#[cfg(not(unix))]
pub fn handshake_unix(_socket_path: &Path, _timeout: Duration) -> Result<InitializeResult, String> {
    Err("Windows named pipe RPC 需在本机验证；当前构建目标非 Unix。".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_payload_shape() {
        let v = json!({"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":1},"id":1});
        assert_eq!(v["method"], "initialize");
    }

    #[test]
    fn parses_snake_case_initialize_result() {
        let raw = json!({"protocol_version":1,"server_version":"0.9.0"});
        let p: InitializeResult = serde_json::from_value(raw).unwrap();
        assert_eq!(p.protocol_version, 1);
        assert_eq!(p.server_version.as_deref(), Some("0.9.0"));
    }
}