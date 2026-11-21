//! ZeroClaw v0.9.0 本地 JSON-RPC（NDJSON）最小客户端。
//! 固定 commit 6c78ba59bce6a3d5079cf0c0310488770e264ad4，见 vendor/zeroclaw.lock.json。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;

pub const PINNED_TAG: &str = "v0.9.0";
pub const PINNED_COMMIT: &str = "6c78ba59bce6a3d5079cf0c0310488770e264ad4";

#[derive(Debug)]
pub struct RpcLine {
    pub raw: String,
    pub value: Value,
}

pub fn send_line<W: Write>(w: &mut W, v: &Value) -> std::io::Result<()> {
    let line = serde_json::to_string(v).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
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
    #[serde(rename = "protocolVersion")]
    pub protocol_version: i64,
    #[serde(rename = "serverVersion", default)]
    pub server_version: Option<String>,
}

pub fn initialize_request(id: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": { "protocolVersion": 1 },
        "id": id
    })
}

pub fn status_request(id: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "status",
        "params": {},
        "id": id
    })
}

#[cfg(unix)]
pub fn handshake_unix(socket_path: &Path, timeout: Duration) -> Result<InitializeResult, String> {
    use std::os::unix::net::UnixStream;
    let stream = UnixStream::connect(socket_path).map_err(|e| format!("connect {socket_path:?}: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    send_line(&mut writer, &initialize_request(1)).map_err(|e| e.to_string())?;
    let line = read_line(&mut reader)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "initialize: empty response".to_string())?;
    let result = line
        .value
        .get("result")
        .ok_or_else(|| format!("initialize error: {}", line.raw))?;
    serde_json::from_value(result.clone()).map_err(|e| format!("parse initialize: {e}"))
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
        let v = initialize_request(1);
        assert_eq!(v["method"], "initialize");
        assert_eq!(v["params"]["protocolVersion"], 1);
    }
}