//! ZeroClaw v0.9.0 本地 JSON-RPC（NDJSON）客户端（产品端）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub const PINNED_TAG: &str = "v0.9.0";
pub const PINNED_COMMIT: &str = "6c78ba59bce6a3d5079cf0c0310488770e264ad4";
pub const PINNED_LINUX_SHA256: &str =
    "ce9cdfd3d419ff2ee668b0741d4cea41eaee9c637c6f590c3faf3e37b6507db9";

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
            initialized: false,
            server_version: None,
        })
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
            let line = read_line(&mut self.reader)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "rpc eof".to_string())?;
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
            // notifications (session/update) ignored here; turn waiter reads separately
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
        let result = self.request(
            "session/new",
            json!({"agentAlias": agent_alias, "keep_siblings": true}),
        )?;
        result
            .get("sessionId")
            .or_else(|| result.get("session_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("session/new missing id: {result}"))
    }

    pub fn session_prompt(&mut self, session_id: &str, text: &str) -> Result<(), String> {
        let _ack = self.request(
            "session/prompt",
            json!({"sessionId": session_id, "prompt": [{"type":"text","text": text}]}),
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
            let line = match read_line(&mut self.reader) {
                Ok(Some(l)) => l,
                Ok(None) => return Err("turn stream eof".into()),
                Err(e) => return Err(e.to_string()),
            };
            raw_events += 1;
            let method = line.value.get("method").and_then(|m| m.as_str());
            if method == Some("session/update") {
                if let Some(params) = line.value.get("params") {
                    let sid = params
                        .get("sessionId")
                        .or_else(|| params.get("session_id"))
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
        let _ = self.request("session/cancel", json!({"sessionId": session_id}))?;
        Ok(())
    }

    pub fn session_state(&mut self, session_id: &str) -> Result<Value, String> {
        self.request("session/state", json!({"sessionId": session_id}))
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