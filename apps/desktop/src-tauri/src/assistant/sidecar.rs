//! 启动并监管固定版本的 ZeroClaw daemon（禁止调用方注入 shell 片段）。

use super::zeroclaw_rpc::PINNED_TAG;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

pub fn spawn_daemon(data_dir: &Path, socket_path: &Path) -> Result<SidecarHandle, String> {
    let bin = resolve_binary_path()?;
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
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