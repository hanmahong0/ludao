//! 启动并监管固定版本的 ZeroClaw daemon（禁止调用方注入 shell 片段）。

use super::zeroclaw_rpc::PINNED_TAG;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// MCP 子进程环境（由宿主签发，禁止调用方注入 command/args）。
#[derive(Clone, Debug)]
pub struct LudaoMcpEnv {
    pub db_path: PathBuf,
    pub capability_token: String,
    pub matter_scope: String,
}

pub fn resolve_legal_mcp_binary() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("LUDAO_LEGAL_MCP_BIN") {
        let pb = PathBuf::from(p);
        if pb.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("LUDAO_LEGAL_MCP_BIN 不得包含 ..".into());
        }
        if !pb.is_file() {
            return Err(format!("LUDAO_LEGAL_MCP_BIN 不存在: {}", pb.display()));
        }
        return Ok(pb.canonicalize().map_err(|e| e.to_string())?);
    }
    for c in [
        PathBuf::from("target/debug/ludao-legal-mcp"),
        PathBuf::from("/workspace/target/debug/ludao-legal-mcp"),
    ] {
        if c.is_file() {
            return Ok(c);
        }
    }
    Err("未找到 ludao-legal-mcp（先 cargo build --bin ludao-legal-mcp）".into())
}

pub fn write_zeroclaw_config(data_dir: &Path, mcp: Option<&LudaoMcpEnv>) -> Result<(), String> {
    let mut body = String::from("schema_version = 4\n\n[tools]\noptional = [\"*\"]\n\n");
    if let Some(m) = mcp {
        let mcp_bin = resolve_legal_mcp_binary()?;
        let db = toml_escape(&m.db_path.to_string_lossy());
        let cap = toml_escape(&m.capability_token);
        let matter = toml_escape(&m.matter_scope);
        let cmd = toml_escape(&mcp_bin.to_string_lossy());
        body.push_str("[mcp]\nenabled = true\n\n[[mcp.servers]]\n");
        body.push_str(&format!(
            "name = \"ludao-legal\"\ncommand = \"{cmd}\"\nargs = []\n"
        ));
        body.push_str(&format!(
            "env = {{ LUDAO_DB_PATH = \"{db}\", LUDAO_CAPABILITY_TOKEN = \"{cap}\", LUDAO_MATTER_SCOPE = \"{matter}\" }}\n"
        ));
    } else {
        body.push_str("[mcp]\nenabled = false\n");
    }
    std::fs::write(data_dir.join("config.toml"), body).map_err(|e| e.to_string())
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\"', "\\\"")
}

pub struct SidecarHandle {
    pub child: Child,
    pub socket_path: PathBuf,
    pub data_dir: PathBuf,
}

pub fn resolve_binary_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("LUDAO_ZEROCLAW_BIN") {
        let pb = PathBuf::from(p);
        if pb.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("LUDAO_ZEROCLAW_BIN 不得包含 ..".into());
        }
        if !pb.is_file() {
            return Err(format!("LUDAO_ZEROCLAW_BIN 不存在: {}", pb.display()));
        }
        return Ok(pb.canonicalize().map_err(|e| e.to_string())?);
    }
    for c in [
        PathBuf::from("vendor/zeroclaw/zeroclaw"),
        PathBuf::from("/workspace/vendor/zeroclaw/zeroclaw"),
    ] {
        if c.is_file() {
            return Ok(c);
        }
    }
    Err("未配置已验签 ZeroClaw 二进制（LUDAO_ZEROCLAW_BIN）".into())
}

pub fn spawn_daemon(
    data_dir: &Path,
    socket_path: &Path,
    mcp: Option<&LudaoMcpEnv>,
) -> Result<SidecarHandle, String> {
    let bin = resolve_binary_path()?;
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    write_zeroclaw_config(data_dir, mcp)?;
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }
    let mut child = Command::new(&bin)
        .arg("daemon")
        .arg("--ephemeral")
        .env("ZEROCLAW_DATA_DIR", data_dir)
        .env("ZEROCLAW_SOCKET", socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn zeroclaw {}: {e}", PINNED_TAG))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if socket_path.exists() {
            return Ok(SidecarHandle {
                child,
                socket_path: socket_path.to_path_buf(),
                data_dir: data_dir.to_path_buf(),
            });
        }
        if child
            .try_wait()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("zeroclaw daemon exited before socket ready".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    Err("zeroclaw socket timeout".into())
}

impl SidecarHandle {
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }

    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}