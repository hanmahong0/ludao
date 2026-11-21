//! server-config 信封：Ed25519 验签、schema 门禁、config_cache 读写。
//! 生产构建设置 `LUDAO_PINNED_CONFIG_PUBKEY`（base64）后禁用 dev 自签。

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// 当前生效配置行（payload 为完整 JSON 文本）。
#[derive(Debug, Clone)]
pub struct ActiveConfig {
    pub kind: String,
    pub config_id: String,
    pub version: String,
    pub schema_version: i64,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Applied { kind: String, config_id: String, version: String },
    StoredInvalidSchema { kind: String, config_id: String, version: String },
    RejectedSignature,
    RejectedMetaMismatch,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigEnvelope {
    kind: String,
    config_id: String,
    version: String,
    schema_version: i64,
    effective_from: Option<String>,
    payload: String,
    signature: String,
}

/// DEV 信任根（仅开发/验收用；公钥写入仓库是刻意的非生产路径）。
const DEV_SIGNING_SEED: [u8; 32] = [
    0x6c, 0x75, 0x64, 0x61, 0x6f, 0x2d, 0x64, 0x65, 0x76, 0x2d, 0x63, 0x6f, 0x6e, 0x66, 0x69, 0x67,
    0x2d, 0x73, 0x69, 0x67, 0x6e, 0x2d, 0x72, 0x6f, 0x6f, 0x74, 0x2d, 0x30, 0x30, 0x00, 0x00, 0x01,
];

fn dev_signing_key() -> SigningKey {
    SigningKey::from_bytes(&DEV_SIGNING_SEED)
}

fn pinned_verify_key() -> Option<VerifyingKey> {
    static CACHED: OnceLock<Option<VerifyingKey>> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            std::env::var("LUDAO_PINNED_CONFIG_PUBKEY")
                .ok()
                .and_then(|b64| {
                    let bytes = B64.decode(b64.trim()).ok()?;
                    let arr: [u8; 32] = bytes.try_into().ok()?;
                    VerifyingKey::from_bytes(&arr).ok()
                })
        })
        .clone()
}

fn supported_schema_versions() -> &'static HashMap<&'static str, i64> {
    static MAP: OnceLock<HashMap<&'static str, i64>> = OnceLock::new();
    MAP.get_or_init(|| {
        HashMap::from([
            ("deadline-rules", 3),
            ("holidays", 1),
            ("claim-elements", 2),
            ("template", 1),
        ])
    })
}

pub fn supported_schema_version(kind: &str) -> Option<i64> {
    supported_schema_versions().get(kind).copied()
}

fn signing_message(
    kind: &str,
    config_id: &str,
    version: &str,
    schema_version: i64,
    effective_from: Option<&str>,
    payload: &str,
) -> String {
    let payload_hash = crate::sha256_hex(payload.as_bytes());
    let eff = effective_from.unwrap_or("");
    format!(
        "ludao-config-v1|{kind}|{config_id}|{version}|{schema_version}|{eff}|{payload_hash}"
    )
}

fn sign_envelope(
    kind: &str,
    config_id: &str,
    version: &str,
    schema_version: i64,
    effective_from: Option<&str>,
    payload: &str,
    key: &SigningKey,
) -> String {
    let msg = signing_message(kind, config_id, version, schema_version, effective_from, payload);
    let sig = key.sign(msg.as_bytes());
    B64.encode(sig.to_bytes())
}

fn verify_envelope(env: &ConfigEnvelope) -> bool {
    let msg = signing_message(
        &env.kind,
        &env.config_id,
        &env.version,
        env.schema_version,
        env.effective_from.as_deref(),
        &env.payload,
    );
    let sig_bytes = match B64.decode(env.signature.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig_arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let signature = Signature::from_bytes(&sig_arr);

    if let Some(pinned) = pinned_verify_key() {
        return pinned.verify(msg.as_bytes(), &signature).is_ok();
    }
    let dev_pub = dev_signing_key().verifying_key();
    dev_pub.verify(msg.as_bytes(), &signature).is_ok()
}

fn check_payload_meta_consistency(env: &ConfigEnvelope) -> Result<(), String> {
    let v: serde_json::Value =
        serde_json::from_str(&env.payload).map_err(|e| format!("payload json: {e}"))?;
    let obj = v.as_object().ok_or("payload must be object")?;
    let pk = obj
        .get("kind")
        .and_then(|x| x.as_str())
        .ok_or("payload.kind missing")?;
    let pid = obj
        .get("config_id")
        .and_then(|x| x.as_str())
        .ok_or("payload.config_id missing")?;
    let pver = obj
        .get("version")
        .and_then(|x| x.as_str())
        .ok_or("payload.version missing")?;
  let psv = obj
        .get("schemaVersion")
        .and_then(|x| x.as_i64())
        .ok_or("payload.schemaVersion missing")?;
    if pk != env.kind || pid != env.config_id || pver != env.version {
        return Err("envelope meta does not match payload meta".into());
    }
    if psv != env.schema_version {
        return Err("envelope schema_version does not match payload.schemaVersion".into());
    }
    Ok(())
}

/// 开发构建自签信封；生产（已 PINNED 公钥）返回 None。
pub fn dev_seal_envelope(
    kind: &str,
    config_id: &str,
    version: &str,
    schema_version: i64,
    effective_from: Option<&str>,
    payload: &str,
) -> Option<String> {
    if pinned_verify_key().is_some() {
        return None;
    }
    let signature = sign_envelope(
        kind,
        config_id,
        version,
        schema_version,
        effective_from,
        payload,
        &dev_signing_key(),
    );
    let env = ConfigEnvelope {
        kind: kind.to_string(),
        config_id: config_id.to_string(),
        version: version.to_string(),
        schema_version,
        effective_from: effective_from.map(String::from),
        payload: payload.to_string(),
        signature,
    };
    serde_json::to_string(&env).ok()
}

pub fn install_config(conn: &Connection, envelope_json: &str, fetched_at: &str) -> rusqlite::Result<InstallOutcome> {
    let env: ConfigEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

    if !verify_envelope(&env) {
        return Ok(InstallOutcome::RejectedSignature);
    }
    if check_payload_meta_consistency(&env).is_err() {
        return Ok(InstallOutcome::RejectedMetaMismatch);
    }

    let supported = supported_schema_version(&env.kind).unwrap_or(0);
    let valid = if env.schema_version <= supported { 1 } else { 0 };

    conn.execute(
        "INSERT OR REPLACE INTO config_cache \
         (kind, config_id, version, schema_version, effective_from, signature, payload, fetched_at, valid) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            env.kind,
            env.config_id,
            env.version,
            env.schema_version,
            env.effective_from,
            env.signature,
            env.payload,
            fetched_at,
            valid,
        ],
    )?;

    if valid == 1 {
        Ok(InstallOutcome::Applied {
            kind: env.kind,
            config_id: env.config_id,
            version: env.version,
        })
    } else {
        Ok(InstallOutcome::StoredInvalidSchema {
            kind: env.kind,
            config_id: env.config_id,
            version: env.version,
        })
    }
}

pub fn active_config(
    conn: &Connection,
    kind: &str,
    config_id: &str,
) -> rusqlite::Result<Option<ActiveConfig>> {
    match conn.query_row(
        "SELECT kind, config_id, version, schema_version, payload \
         FROM config_cache \
         WHERE kind = ?1 AND config_id = ?2 AND valid = 1 \
         ORDER BY fetched_at DESC LIMIT 1",
        params![kind, config_id],
        |r| {
            Ok(ActiveConfig {
                kind: r.get(0)?,
                config_id: r.get(1)?,
                version: r.get(2)?,
                schema_version: r.get(3)?,
                payload: r.get(4)?,
            })
        },
    ) {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_schema, open_encrypted};

    fn mem_conn() -> Connection {
        let conn = open_encrypted(":memory:", "test-key").expect("open");
        apply_schema(&conn).expect("schema");
        conn
    }

    #[test]
    fn dev_seal_install_and_active_roundtrip() {
        let conn = mem_conn();
        let payload = r#"{"kind":"holidays","config_id":"cn-statutory-2026","schemaVersion":1,"version":"2026.1","effectiveFrom":"2026-01-01","holidays":[]}"#;
        let env = dev_seal_envelope(
            "holidays",
            "cn-statutory-2026",
            "2026.1",
            1,
            Some("2026-01-01"),
            payload,
        )
        .expect("dev seal");
        let out = install_config(&conn, &env, "2026-01-01T00:00:00Z").expect("install");
        assert_eq!(
            out,
            InstallOutcome::Applied {
                kind: "holidays".into(),
                config_id: "cn-statutory-2026".into(),
                version: "2026.1".into(),
            }
        );
        let active = active_config(&conn, "holidays", "cn-statutory-2026")
            .expect("query")
            .expect("some");
        assert_eq!(active.payload, payload);
    }

}