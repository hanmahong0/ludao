//! 启动受管目录里、hash 已核对的 ZeroClaw。不接受调用方传入的任意可执行路径。

use super::zeroclaw_rpc::{PINNED_BINARY_SHA256, PINNED_TAG};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct LudaoMcpEnv {
    pub db_path: PathBuf,
    pub grant_file: PathBuf,
    pub host_session: String,
    pub db_key: String,
}

pub struct SidecarHandle {
    pub child: Child,
    pub socket_path: PathBuf,
    pub data_dir: PathBuf,
    pub log_path: PathBuf,
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn managed_runtime_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("LUDAO_RUNTIME_DIR") {
        let p = PathBuf::from(&dir);
        if !p.is_dir() {
            return Err("LUDAO_RUNTIME_DIR 必须是已存在的目录".into());
        }
        return Ok(p);
    }
    for c in [
        PathBuf::from("vendor/zeroclaw"),
        PathBuf::from("/workspace/vendor/zeroclaw"),
    ] {
        if c.join("zeroclaw").is_file() {
            return Ok(c);
        }
    }
    Err("未找到受管 ZeroClaw 目录（vendor/zeroclaw 或 LUDAO_RUNTIME_DIR）".into())
}

fn verify_regular_binary(path: &Path, expect_name: &str, expect_sha: Option<&str>) -> Result<PathBuf, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!("拒绝符号链接: {}", path.display()));
    }
    if !meta.is_file() {
        return Err(format!("不是普通文件: {}", path.display()));
    }
    if path.file_name().and_then(|s| s.to_str()) != Some(expect_name) {
        return Err(format!("文件名必须是 {expect_name}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(format!("runtime 权限允许其他用户写入: {}", path.display()));
        }
    }
    if let Some(expect) = expect_sha {
        let got = sha256_file(path)?;
        if got != expect {
            return Err(format!("binary sha256 mismatch for {}", path.display()));
        }
    }
    let canon = path.canonicalize().map_err(|e| e.to_string())?;
    let root = path.parent().unwrap().canonicalize().map_err(|e| e.to_string())?;
    if !canon.starts_with(&root) {
        return Err("canonical path 逃出受管目录".into());
    }
    Ok(canon)
}

/// 只接受受管目录中的 `zeroclaw`，并核对解压后的二进制 hash。
/// `LUDAO_ZEROCLAW_BIN` 不能把 `/bin/true` 变成 runtime。debug 下的显式 override 仍要过同一 hash。
pub fn resolve_binary_path() -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    if std::env::var("LUDAO_ZEROCLAW_DEV_OVERRIDE").ok().as_deref() == Some("1") {
        if let Ok(p) = std::env::var("LUDAO_ZEROCLAW_BIN") {
            return verify_regular_binary(Path::new(&p), "zeroclaw", Some(PINNED_BINARY_SHA256));
        }
    }
    let path = managed_runtime_dir()?.join("zeroclaw");
    verify_regular_binary(&path, "zeroclaw", Some(PINNED_BINARY_SHA256))
}

pub fn resolve_legal_mcp_binary() -> Result<PathBuf, String> {
    let name = "ludao-legal-mcp";
    let mut cands = Vec::new();
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_ludao-legal-mcp") {
        cands.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join(name));
            if let Some(parent) = dir.parent() {
                cands.push(parent.join(name));
            }
        }
    }
    cands.push(PathBuf::from("target/debug").join(name));
    cands.push(PathBuf::from("/workspace/target/debug").join(name));
    for c in cands {
        if c.is_file() {
            return verify_regular_binary(&c, name, None);
        }
    }
    Err("未找到 ludao-legal-mcp（先 cargo build --bin ludao-legal-mcp）".into())
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\"', "\\\"")
}

pub fn write_zeroclaw_config(data_dir: &Path, mcp: Option<&LudaoMcpEnv>) -> Result<(), String> {
    let sim = std::env::var("LUDAO_SIM_LLM_URL").unwrap_or_else(|_| "http://127.0.0.1:9/v1".into());
    let mut body = String::from(
        "schema_version = 4\n\n[tools]\noptional = []\n\n[secrets]\nencrypt = false\n\n",
    );
    body.push_str("[providers.models.custom.sim]\nkind = \"openai-compatible\"\n");
    body.push_str(&format!(
        "uri = \"{}\"\nmodel = \"sim-fixture\"\napi_key = \"simulation\"\n\n",
        toml_escape(&sim)
    ));
    body.push_str(
        "[risk_profiles.legal_readonly]\nallowed_commands = []\nallowed_roots = []\n",
    );
    body.push_str(
        "allowed_tools = [\"ludao-legal__search_private_documents\", \"ludao-legal__search_public_cases\"]\n",
    );
    body.push_str("block_high_risk_commands = true\n\n");
    if let Some(m) = mcp {
        let mcp_bin = resolve_legal_mcp_binary()?;
        body.push_str("[mcp]\nenabled = true\n\n[[mcp.servers]]\n");
        body.push_str("name = \"ludao-legal\"\n");
        body.push_str(&format!("command = \"{}\"\nargs = []\n", toml_escape(&mcp_bin.to_string_lossy())));
        let key_file = data_dir.join("db.key");
        {
            use std::io::Write;
            #[cfg(unix)]
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.create(true).write(true).truncate(true);
            #[cfg(unix)]
            opts.mode(0o600);
            let mut f = opts.open(&key_file).map_err(|e| e.to_string())?;
            f.write_all(m.db_key.as_bytes()).map_err(|e| e.to_string())?;
        }
        body.push_str(&format!(
            "env = {{ LUDAO_SNAPSHOT_DB = \"{}\", LUDAO_GRANT_FILE = \"{}\", LUDAO_HOST_SESSION = \"{}\", LUDAO_DB_KEY_FILE = \"{}\" }}\n\n",
            toml_escape(&m.db_path.to_string_lossy()),
            toml_escape(&m.grant_file.to_string_lossy()),
            toml_escape(&m.host_session),
            toml_escape(&key_file.to_string_lossy()),
        ));
        body.push_str("[mcp_bundles.legal]\nservers = [\"ludao-legal\"]\n\n");
        body.push_str("[agents.legal]\nenabled = true\nmodel_provider = \"custom.sim\"\n");
        body.push_str("mcp_bundles = [\"legal\"]\nacp_enable_mcp = true\nrisk_profile = \"legal_readonly\"\n\n");
    } else {
        body.push_str("[mcp]\nenabled = false\n\n");
        body.push_str("[agents.legal]\nenabled = true\nmodel_provider = \"custom.sim\"\n");
        body.push_str("mcp_bundles = []\nacp_enable_mcp = false\nrisk_profile = \"legal_readonly\"\n\n");
    }
    body.push_str("[acp]\ndefault_agent = \"legal\"\n");
    let cfg = data_dir.join("config.toml");
    std::fs::write(&cfg, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn mcp_registered(log: &str) -> bool {
    log.contains("MCP server `ludao-legal` connected")
        && log.contains("tool(s) registered")
        && !log.contains("MCP: 0 tool(s) registered")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_path_is_not_a_runtime() {
        std::env::set_var("LUDAO_ZEROCLAW_BIN", "/bin/true");
        std::env::remove_var("LUDAO_ZEROCLAW_DEV_OVERRIDE");
        let resolved = resolve_binary_path().expect("managed zeroclaw");
        assert!(resolved.ends_with("zeroclaw"));
        assert!(!resolved.ends_with("true"));
        std::env::set_var("LUDAO_ZEROCLAW_DEV_OVERRIDE", "1");
        let err = resolve_binary_path().unwrap_err();
        assert!(err.contains("文件名") || err.contains("sha256") || err.contains("符号"));
        std::env::remove_var("LUDAO_ZEROCLAW_DEV_OVERRIDE");
        std::env::remove_var("LUDAO_ZEROCLAW_BIN");
    }
}

pub fn spawn_daemon(
    data_dir: &Path,
    socket_path: &Path,
    mcp: Option<&LudaoMcpEnv>,
) -> Result<SidecarHandle, String> {
    let bin = resolve_binary_path()?;
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700));
    }
    write_zeroclaw_config(data_dir, mcp)?;
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }
    let log_path = data_dir.join("daemon.log");
    let log_file = File::create(&log_path).map_err(|e| e.to_string())?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(&bin)
        .arg("daemon")
        .arg("--ephemeral")
        .arg("-p")
        .arg("0")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("-v")
        .arg("--log-level")
        .arg("info")
        .env("ZEROCLAW_DATA_DIR", data_dir)
        .env("ZEROCLAW_SOCKET", socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("spawn zeroclaw {}: {e}", PINNED_TAG))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if socket_path.exists() {
            return Ok(SidecarHandle {
                child,
                socket_path: socket_path.to_path_buf(),
                data_dir: data_dir.to_path_buf(),
                log_path,
            });
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            let tail = std::fs::read_to_string(&log_path).unwrap_or_default();
            return Err(format!(
                "zeroclaw daemon exited before socket ready: {}",
                tail.chars().rev().take(400).collect::<String>().chars().rev().collect::<String>()
            ));
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

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log_path).unwrap_or_default()
    }
}
