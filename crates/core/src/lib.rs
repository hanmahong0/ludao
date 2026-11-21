//! 本地加密案卷库 core：用 SQLCipher 打开加密库、按单一事实源 DDL 建表、做版本门禁。
//!
//! 关键顺序约束（SQLCipher）：连接打开后**第一件事必须是 `PRAGMA key`**，
//! 在此之前不得有任何读写/PRAGMA（否则 SQLCipher 用空密钥初始化页头，后续 key 无效）。

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

/// server-config 信封：验签 + schemaVersion 兼容 + 本地加密缓存 + 离线回退。
pub mod config;

/// 单一事实源 DDL：直接内联 skeleton/schema/core.sql，避免拷贝漂移。
const SCHEMA_SQL: &str = include_str!("../../../skeleton/schema/core.sql");

/// SHA-256 → 小写 hex。用于监管链 payload_hash（不落原文）及后续证据原件哈希。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// 本 crate 支持的最高 schema 版本（与 core.sql 中 `PRAGMA user_version` 对齐）。
pub const SUPPORTED_USER_VERSION: i64 = 1;

/// 打开（或创建）一个 SQLCipher 加密库。
///
/// 顺序：open → `PRAGMA key`（必须最先）→ `PRAGMA foreign_keys=ON`。
pub fn open_encrypted(path: &str, key: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    set_key(&conn, key)?;
    Ok(conn)
}

/// 在已打开的连接上设置 SQLCipher 密钥并开启外键。
/// 抽出来是为了让内存库测试（`Connection::open_in_memory`）复用同样的 key 顺序。
fn set_key(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    // PRAGMA key 必须先于任何其它操作；用 execute_batch 发原生 PRAGMA。
    // 单引号转义防注入/破坏 SQL。
    let escaped = key.replace('\'', "''");
    conn.execute_batch(&format!("PRAGMA key = '{escaped}';"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(())
}

/// 按 core.sql 建表（含触发器与索引）。须在 `PRAGMA key` 之后调用。
pub fn apply_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

/// 版本门禁：读 db 的 `PRAGMA user_version`。
/// - db 版本 > supported  → 返回 Err（旧客户端见到更高版本，强制升级，绝不静默脏写）。
/// - db 版本 <= supported → Ok。
pub fn check_user_version(conn: &Connection, supported: i64) -> rusqlite::Result<()> {
    let db_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if db_version > supported {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
            Some(format!(
                "db user_version {db_version} > supported {supported}: client too old, upgrade required"
            )),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用 key 打开内存库、建表，并复用与 open_encrypted 相同的 key 顺序。
    fn open_encrypted_memory(key: &str) -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        set_key(&conn, key).expect("set key");
        conn
    }

    #[test]
    fn cipher_is_real_sqlcipher_and_schema_applies() {
        let conn = open_encrypted_memory("test-passphrase");
        apply_schema(&conn).expect("apply schema");
        // cipher_version 非空 → 链接的确实是 SQLCipher（普通 SQLite 此 PRAGMA 返回空）。
        let cipher_version: String = conn
            .query_row("PRAGMA cipher_version", [], |r| r.get(0))
            .expect("cipher_version present");
        assert!(
            !cipher_version.trim().is_empty(),
            "cipher_version should be non-empty for real SQLCipher"
        );
    }

    #[test]
    fn all_expected_tables_exist() {
        let conn = open_encrypted_memory("k");
        apply_schema(&conn).expect("apply schema");
        let expected = [
            "case",
            "evidence",
            "fact",
            "deadline",
            "attestation",
            "custody_event",
            "__transactions",
            "config_cache",
        ];
        for t in expected {
            let cnt: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .expect("query sqlite_master");
            assert_eq!(cnt, 1, "expected table `{t}` to exist exactly once");
        }
    }

    #[test]
    fn custody_event_is_append_only() {
        let conn = open_encrypted_memory("k");
        apply_schema(&conn).expect("apply schema");
        conn.execute(
            "INSERT INTO custody_event (entity_type, entity_id, event_type, actor, ts, payload_hash) \
             VALUES ('evidence','e1','captured','system','2026-06-15T00:00:00Z','deadbeef')",
            [],
        )
        .expect("insert custody_event row");

        // UPDATE 应被触发器 RAISE(ABORT) 拒绝。
        let upd = conn.execute("UPDATE custody_event SET actor='hacker' WHERE id=1", []);
        assert!(upd.is_err(), "UPDATE on custody_event must be rejected");

        // DELETE 同样应被拒绝。
        let del = conn.execute("DELETE FROM custody_event WHERE id=1", []);
        assert!(del.is_err(), "DELETE on custody_event must be rejected");

        // 行仍在。
        let cnt: i64 = conn
            .query_row("SELECT count(*) FROM custody_event", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 1, "row must survive rejected UPDATE/DELETE");
    }

    #[test]
    fn version_gate_rejects_newer_db() {
        let conn = open_encrypted_memory("k");
        apply_schema(&conn).expect("apply schema");

        // 建表后 user_version 应为 1（来自 core.sql 的 PRAGMA），不高于 supported → Ok。
        check_user_version(&conn, SUPPORTED_USER_VERSION).expect("v1 <= supported should pass");

        // 把库版本抬到 99（模拟更新的客户端写过的库），门禁必须拒绝。
        conn.execute_batch("PRAGMA user_version = 99;").unwrap();
        assert!(
            check_user_version(&conn, 1).is_err(),
            "user_version 99 > supported 1 must be rejected"
        );
    }
}
