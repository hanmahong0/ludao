//! 真实 MCP 进程 + 真实 ZeroClaw daemon。临时合成库，不碰现用数据。
//! 票据与数据库密钥只出现在 0600 文件里，失败信息会打码。

#![cfg(unix)]

use lurus_desktop_lib::assistant::sidecar::{mcp_registered, spawn_daemon, LudaoMcpEnv};
use lurus_desktop_lib::assistant::zeroclaw_rpc::RpcClient;
use lurus_desktop_lib::db::{self, snapshot_db_key};
use lurus_desktop_lib::scope::ScopeRegistry;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.len() >= 8 {
            out = out.replace(secret, "[redacted]");
        }
    }
    out
}

fn mcp_bin() -> PathBuf {
    PathBuf::from(
        std::env::var("CARGO_BIN_EXE_ludao-legal-mcp").expect("cargo MCP bin path"),
    )
}

fn file_fp(path: &Path) -> (u64, SystemTime, [u8; 32]) {
    use sha2::{Digest, Sha256};
    let meta = std::fs::metadata(path).unwrap();
    let bytes = std::fs::read(path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest: [u8; 32] = hasher.finalize().into();
    (meta.len(), meta.modified().unwrap(), digest)
}

fn run_mcp(db: &Path, grant: &Path, session: &str, lines: &[&str]) -> (bool, String, String) {
    let mut child = Command::new(mcp_bin())
        .env("LUDAO_SNAPSHOT_DB", db)
        .env("LUDAO_GRANT_FILE", grant)
        .env("LUDAO_HOST_SESSION", session)
        .env("LUDAO_DB_KEY_FILE", grant.parent().unwrap().join("db.key"))
        .env_remove("LUDAO_DB_KEY")
        .env("LUDAO_CAPABILITY_TOKEN", "capv1.forged-public-constant")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp");
    {
        let mut stdin = child.stdin.take().unwrap();
        for line in lines {
            writeln!(stdin, "{line}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn write_key(dir: &Path) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("db.key"))
        .unwrap();
    write!(f, "{}", snapshot_db_key()).unwrap();
}

#[test]
fn readonly_mcp_and_daemon_enumeration() {
    db::enable_dev_config_trust_for_tests();
    let root = std::env::temp_dir().join(format!("ludao-loop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("snap.db");
    {
        let conn = db::open_and_init(db_path.to_str().unwrap()).unwrap();
        conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
    }
    assert!(!root.join("snap.db-wal").exists());
    write_key(&root);
    let scope = ScopeRegistry::new();
    let session = scope.host_session_id();
    scope.confirm_selection(&session, "case-001").unwrap();
    let grant = scope.issue_for_confirmed(&session, "case-001", 600).unwrap();
    let grant_path = root.join("grant.json");
    scope.write_grant_file(&grant.token, &grant_path).unwrap();
    let secrets = vec![grant.token.clone(), snapshot_db_key()];
    let before = file_fp(&db_path);

    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
    let private_q = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_private_documents","arguments":{"query":"借贷","limit":5}}}"#;
    let public_q = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search_public_cases","arguments":{"query":"\"借贷\"","limit":5}}}"#;
    let other = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"search_private_documents","arguments":{"query":"劳动","case_id":"case-002"}}}"#;
    let unknown = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"shell","arguments":{"query":"id"}}}"#;
    let wild = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"search_public_cases","arguments":{"query":"%"}}}"#;
    let (ok, stdout, stderr) = run_mcp(
        &db_path,
        &grant_path,
        &session,
        &[init, list, private_q, public_q, other, unknown, wild],
    );
    assert!(ok, "mcp stderr {}", redact(&stderr, &secrets));
    let lines: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(lines.iter().any(|v| v["id"] == 1 && v.get("result").is_some()));
    let listed = lines.iter().find(|v| v["id"] == 2).unwrap();
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["search_private_documents", "search_public_cases"]
    );
    let priv_hit = lines.iter().find(|v| v["id"] == 3).unwrap();
    let priv_text = priv_hit["result"]["content"][0]["text"].as_str().unwrap();
    assert!(priv_text.contains("借贷") || priv_text.contains("case-001"), "{priv_text}");
    let pub_hit = lines.iter().find(|v| v["id"] == 4).unwrap();
    let pub_text = pub_hit["result"]["content"][0]["text"].as_str().unwrap();
    assert!(pub_text.contains("借贷"), "{pub_text}");
    assert!(!pub_text.contains('页'));
    let denied = lines.iter().find(|v| v["id"] == 5).unwrap();
    assert!(denied["error"]["message"].as_str().unwrap().contains("scope_denied"));
    let bad_tool = lines.iter().find(|v| v["id"] == 6).unwrap();
    assert!(bad_tool["error"]["message"].as_str().unwrap().contains("tool_not_allowed"));
    let wild_hit = lines.iter().find(|v| v["id"] == 7).unwrap();
    assert!(wild_hit["error"]["message"].as_str().unwrap().contains("wildcard"));
    let after = file_fp(&db_path);
    assert_eq!(before, after, "只读 MCP 不得改快照字节");
    assert!(!root.join("snap.db-wal").exists());
    assert!(!root.join("snap.db-shm").exists());
    assert!(!stdout.contains(&grant.token));
    assert!(!stderr.contains(&grant.token));

    let missing = root.join("no-such.db");
    let (ok, _, stderr) = run_mcp(&missing, &grant_path, &session, &[init]);
    assert!(!ok);
    assert!(stderr.contains("snapshot_missing"), "{}", redact(&stderr, &secrets));
    assert!(!missing.exists());

    let revoked = root.join("revoked.json");
    ScopeRegistry::write_revoked_file(&revoked).unwrap();
    let (ok, _, stderr) = run_mcp(&db_path, &revoked, &session, &[init]);
    assert!(!ok);
    assert!(stderr.contains("revoked"), "{}", redact(&stderr, &secrets));

    let (ok, _, stderr) = run_mcp(&db_path, &grant_path, "other-session", &[init]);
    assert!(!ok);
    assert!(stderr.contains("session") || stderr.contains("mismatch"), "{}", redact(&stderr, &secrets));

    let expired = root.join("expired.json");
    std::fs::write(
        &expired,
        r#"{"revoked":false,"session_id":"SESS","matter_id":"case-001","expires_at_unix":1,"token":"capv1.expired"}"#,
    )
    .unwrap();
    let text = std::fs::read_to_string(&expired).unwrap().replace("SESS", &session);
    std::fs::write(&expired, text).unwrap();
    let (ok, _, stderr) = run_mcp(&db_path, &expired, &session, &[init]);
    assert!(!ok);
    assert!(stderr.contains("expired"), "{}", redact(&stderr, &secrets));

    std::fs::write(root.join("empty.json"), "{}").unwrap();
    let (ok, _, stderr) = run_mcp(&db_path, &root.join("empty.json"), &session, &[init]);
    assert!(!ok);
    assert!(stderr.contains("empty"), "{}", redact(&stderr, &secrets));

    let data_dir = root.join("zc");
    let socket = data_dir.join("daemon.sock");
    let mcp = LudaoMcpEnv {
        db_path: db_path.clone(),
        grant_file: grant_path.clone(),
        host_session: session.clone(),
        db_key: snapshot_db_key(),
    };
    let mut handle = spawn_daemon(&data_dir, &socket, Some(&mcp)).expect("daemon");
    let cfg = std::fs::read_to_string(data_dir.join("config.toml")).unwrap();
    assert!(!cfg.contains(&snapshot_db_key()), "config 不得写入数据库密钥");
    assert!(!cfg.contains(&grant.token), "config 不得写入票据");
    let mut client = RpcClient::connect(&socket, Duration::from_secs(15)).unwrap();
    let init = client.initialize().unwrap();
    assert_eq!(init.server_version.as_deref(), Some("0.9.0"));
    let zc_session = client.session_new("legal").expect("session/new");
    let mut attached = false;
    for _ in 0..30 {
        if mcp_registered(&handle.log_text()) {
            attached = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let log = redact(&handle.log_text(), &secrets);
    assert!(attached, "daemon 未登记法律工具:\n{log}");
    let cancel = client.session_cancel(&zc_session);
    assert!(cancel.is_err(), "空闲会话取消必须失败");
    let state = client.session_state(&zc_session).expect("session/state");
    assert!(state.to_string().contains("idle"), "{state}");
    let raw_log = handle.log_text();
    let leaked = raw_log.contains(&grant.token) || raw_log.contains(&snapshot_db_key());
    assert!(!leaked, "daemon log leaked a secret");
    handle.stop();
    assert!(!socket.exists());
    let _ = std::fs::remove_dir_all(&root);
}
