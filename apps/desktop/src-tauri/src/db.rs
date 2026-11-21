//! 打开加密库 → schema → 内置配置 → 合成演示数据。

use rusqlite::{params, Connection};

const SYNTHETIC_DB_KEY: &str = "ludao-synthetic-dev-only-v1";

const SUPPLEMENTAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS source_registry (
  source_id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  license_status TEXT NOT NULL DEFAULT 'synthetic_fixture',
  allowed_uses TEXT NOT NULL DEFAULT 'local_retrieval_demo',
  robots_snapshot TEXT,
  registered_at TEXT NOT NULL,
  withdrawn INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS public_case (
  case_version TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  title TEXT NOT NULL,
  court TEXT,
  decided_at TEXT,
  legal_topics TEXT NOT NULL DEFAULT '[]',
  similarity_tags TEXT NOT NULL DEFAULT '[]',
  withdrawn INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY (source_id) REFERENCES source_registry(source_id)
);

CREATE TABLE IF NOT EXISTS public_passage (
  passage_id TEXT PRIMARY KEY,
  case_version TEXT NOT NULL,
  section_type TEXT NOT NULL,
  text_excerpt TEXT NOT NULL,
  locator TEXT,
  content_hash TEXT NOT NULL,
  FOREIGN KEY (case_version) REFERENCES public_case(case_version)
);

CREATE VIRTUAL TABLE IF NOT EXISTS public_passage_fts USING fts5(
  passage_id UNINDEXED,
  case_version UNINDEXED,
  text_excerpt,
  tokenize = 'unicode61'
);
"#;

/// 测试/本地开发：在打开库前设置 `LUDAO_CONFIG_DEV_TRUST=1` 与 `LUDAO_CONFIG_DEV_SEED`（32 字节 hex）。
pub fn open_and_init(db_path: &str) -> Result<Connection, String> {
    let key = std::env::var("LUDAO_DB_KEY").unwrap_or_else(|_| SYNTHETIC_DB_KEY.to_string());
    if key.trim().is_empty() {
        return Err("LUDAO_DB_KEY 不能为空".into());
    }
    let conn = vault::open_encrypted(db_path, &key).map_err(|e| e.to_string())?;
    vault::check_user_version(&conn, vault::SUPPORTED_USER_VERSION).map_err(|e| e.to_string())?;
    let user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if user_version == 0 {
        vault::apply_schema(&conn).map_err(|e| e.to_string())?;
    }
    conn.execute_batch(SUPPLEMENTAL_SCHEMA)
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    crate::bootstrap::install_builtin_configs(&conn, &now)?;

    seed_if_empty(&conn, &now)?;
    Ok(conn)
}

fn seed_if_empty(conn: &Connection, now: &str) -> Result<(), String> {
    let count: i64 = conn
        .query_row(r#"SELECT COUNT(*) FROM "case""#, [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if count > 0 {
        return Ok(());
    }

    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let r = (|| -> Result<(), String> {
        insert_case(
            conn,
            "case-001",
            None,
            "合成当事人A 诉 合成当事人B",
            "private_lending",
            r#"{"parties":[{"role":"plaintiff","name":"合成当事人A"},{"role":"defendant","name":"合成当事人B"}],"amount":120000,"includes_interest":true,"jurisdiction_code":"CN-GD"}"#,
            now,
        )?;
        insert_case(
            conn,
            "case-002",
            None,
            "合成机构 劳动争议",
            "labor_dispute",
            r#"{"parties":[{"role":"plaintiff","name":"合成劳动者"},{"role":"defendant","name":"合成用人单位"}]}"#,
            now,
        )?;
        insert_evidence(
            conn,
            "ev-a1",
            "case-001",
            1,
            "借条扫描件（合成）",
            "electronic",
            "借贷合意",
            now,
        )?;
        insert_evidence(
            conn,
            "ev-b1",
            "case-002",
            1,
            "合成劳动合同（劳动争议）",
            "book",
            "劳动关系",
            now,
        )?;
        insert_fact(
            conn,
            "fact-a1",
            "case-001",
            "2025-06-01",
            "双方签署借款协议（合成事实）",
            now,
        )?;
        seed_public_fixtures(conn, now)?;
        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        Ok(())
    })();
    if r.is_err() {
        let _ = conn.execute_batch("ROLLBACK");
        return r;
    }
    Ok(())
}

fn insert_case(
    conn: &Connection,
    id: &str,
    case_no: Option<&str>,
    title: &str,
    case_type: &str,
    attributes: &str,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        r#"INSERT INTO "case" (id, case_no, title, case_type, stage, status, attributes, created_at, _updated_at)
           VALUES (?1,?2,?3,?4,'evidence','active',?5,?6,?6)"#,
        params![id, case_no, title, case_type, attributes, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_evidence(
    conn: &Connection,
    id: &str,
    case_id: &str,
    seq: i32,
    name: &str,
    form: &str,
    proof_target: &str,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO evidence (id, case_id, seq, name, form, proof_target, captured_at, _updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
        params![id, case_id, seq, name, form, proof_target, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_fact(
    conn: &Connection,
    id: &str,
    case_id: &str,
    fact_date: &str,
    description: &str,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO fact (id, case_id, description, fact_date, _updated_at)
         VALUES (?1,?2,?3,?4,?5)",
        params![id, case_id, description, fact_date, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn seed_public_fixtures(conn: &Connection, now: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO source_registry (source_id, title, license_status, allowed_uses, registered_at) \
         VALUES ('fixture-synthetic-2026','合成裁判片段库（非真实法院文书）','synthetic_fixture','local_demo_only',?1)",
        [now],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO public_case (case_version, source_id, title, court, decided_at, legal_topics, similarity_tags) \
         VALUES ('pub-p-v1','fixture-synthetic-2026','合成案例P：支持借贷关系','合成法院','2024-01-10','[\"民间借贷\"]','[\"转账\",\"借条\"]')",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO public_passage (passage_id, case_version, section_type, text_excerpt, locator, content_hash) \
         VALUES ('pub-p-p1','pub-p-v1','holding','【合成案例，非法院文书】认定存在借贷合意且已交付借款。','第3页第2段',?1)",
        [vault::sha256_hex(b"synthetic-passage-p1")],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO public_passage_fts (passage_id, case_version, text_excerpt) \
         VALUES ('pub-p-p1','pub-p-v1','合成案例 借贷合意 借款交付 民间借贷')",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO public_case (case_version, source_id, title, court, decided_at, legal_topics, similarity_tags) \
         VALUES ('pub-q-v1','fixture-synthetic-2026','合成案例Q：反向——欠缺交付证据','合成法院','2024-06-01','[\"民间借贷\"]','[\"交付\",\"证据不足\"]')",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO public_passage (passage_id, case_version, section_type, text_excerpt, locator, content_hash) \
         VALUES ('pub-q-p1','pub-q-v1','holding','【合成案例，非法院文书】仅凭聊天记录无法证明款项已实际交付。','第2页第1段',?1)",
        [vault::sha256_hex(b"synthetic-passage-q1")],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO public_passage_fts (passage_id, case_version, text_excerpt) \
         VALUES ('pub-q-p1','pub-q-v1','合成案例 聊天记录 无法证明 实际交付 反向')",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn enable_dev_config_trust_for_tests() {
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};
    std::env::set_var("LUDAO_CONFIG_DEV_TRUST", "1");
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let seed_hex = hex::encode(SigningKey::from_bytes(&seed).to_bytes());
    std::env::set_var("LUDAO_CONFIG_DEV_SEED", seed_hex);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_init_seeds_synthetic_cases() {
        enable_dev_config_trust_for_tests();
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("t.db");
        let path_str = path.to_string_lossy();
        let conn = open_and_init(&path_str).expect("open");
        let n: i64 = conn
            .query_row(r#"SELECT COUNT(*) FROM "case""#, [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        let p: i64 = conn
            .query_row("SELECT COUNT(*) FROM public_case", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, 2);
    }
}