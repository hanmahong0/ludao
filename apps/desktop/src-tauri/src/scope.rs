//! 宿主内存中的一次性能力句柄。
//!
//! 票据是 CSPRNG 随机字节，不携带可重算的签名。空表拒绝一切票据。
//! 撤销从宿主表删除后立即失效。MCP 只吃宿主写下的授权快照文件，不接受调用方自报 case。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64U, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    pub token: String,
    pub principal: String,
    pub session_id: String,
    pub matter_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrantFile {
    session_id: String,
    matter_id: String,
    expires_at_unix: i64,
    /// 仅宿主与本进程快照之间传递。禁止写入日志或 argv。
    token: String,
    revoked: bool,
}

#[derive(Debug)]
struct Inner {
    host_session: String,
    confirmed_matter: Option<String>,
    grants: HashMap<String, CapabilityGrant>,
    generation: u64,
}

#[derive(Debug)]
pub struct ScopeRegistry {
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    MissingToken,
    UnknownToken,
    Expired,
    MatterMismatch { expected: String, got: String },
    SessionMismatch,
    NotConfirmed,
}

impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopeError::MissingToken => write!(f, "scope_denied: missing capability token"),
            ScopeError::UnknownToken => write!(f, "scope_denied: unknown capability token"),
            ScopeError::Expired => write!(f, "scope_denied: capability expired"),
            ScopeError::MatterMismatch { expected, got } => {
                write!(f, "scope_denied: matter mismatch (grant={expected}, request={got})")
            }
            ScopeError::SessionMismatch => write!(f, "scope_denied: session mismatch"),
            ScopeError::NotConfirmed => {
                write!(f, "scope_denied: matter is not the host-confirmed selection")
            }
        }
    }
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    format!("capv1.{}", B64U.encode(buf))
}

fn random_id() -> String {
    let mut buf = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

impl ScopeRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                host_session: random_id(),
                confirmed_matter: None,
                grants: HashMap::new(),
                generation: 0,
            }),
        }
    }

    pub fn host_session_id(&self) -> String {
        self.inner.lock().expect("scope").host_session.clone()
    }

    pub fn generation(&self) -> u64 {
        self.inner.lock().expect("scope").generation
    }

    /// 宿主 UI 的明确选择。模型工具链没有这个入口。
    pub fn confirm_selection(&self, session_id: &str, matter_id: &str) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        if session_id != g.host_session {
            return Err(ScopeError::SessionMismatch.to_string());
        }
        if matter_id.trim().is_empty() {
            return Err("scope_confirm: empty matter".into());
        }
        g.confirmed_matter = Some(matter_id.to_string());
        g.generation = g.generation.wrapping_add(1);
        Ok(())
    }

    /// 只为已经确认的案件签发。传入另一个案件 id 不会扩大授权。
    pub fn issue_for_confirmed(
        &self,
        session_id: &str,
        matter_id: &str,
        ttl_secs: i64,
    ) -> Result<CapabilityGrant, String> {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        if session_id != g.host_session {
            return Err(ScopeError::SessionMismatch.to_string());
        }
        if g.confirmed_matter.as_deref() != Some(matter_id) {
            return Err(ScopeError::NotConfirmed.to_string());
        }
        let now = Utc::now();
        let expires_at = if ttl_secs < 0 {
            now - Duration::seconds(5)
        } else {
            now + Duration::seconds(ttl_secs.max(60))
        };
        let grant = CapabilityGrant {
            token: random_token(),
            principal: "local-desktop-user".into(),
            session_id: session_id.to_string(),
            matter_id: matter_id.to_string(),
            issued_at: now,
            expires_at,
        };
        g.grants.insert(grant.token.clone(), grant.clone());
        g.generation = g.generation.wrapping_add(1);
        Ok(grant)
    }

    pub fn validate(
        &self,
        token: &str,
        matter_id: &str,
        session_id: &str,
    ) -> Result<CapabilityGrant, ScopeError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(ScopeError::MissingToken);
        }
        let g = self.inner.lock().expect("scope");
        let grant = g.grants.get(token).ok_or(ScopeError::UnknownToken)?.clone();
        drop(g);
        if Utc::now() > grant.expires_at {
            return Err(ScopeError::Expired);
        }
        if grant.session_id != session_id {
            return Err(ScopeError::SessionMismatch);
        }
        if grant.matter_id != matter_id {
            return Err(ScopeError::MatterMismatch {
                expected: grant.matter_id,
                got: matter_id.to_string(),
            });
        }
        Ok(grant)
    }

    pub fn revoke(&self, token: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.grants.remove(token.trim());
            g.generation = g.generation.wrapping_add(1);
        }
    }

    /// 宿主把当前票据写成 0600 快照。文件内容供 MCP 启动时读取，不进 argv。
    pub fn write_grant_file(&self, token: &str, path: &Path) -> Result<(), String> {
        let g = self.inner.lock().map_err(|e| e.to_string())?;
        let grant = g
            .grants
            .get(token)
            .ok_or_else(|| "grant file: unknown token".to_string())?;
        let body = GrantFile {
            session_id: grant.session_id.clone(),
            matter_id: grant.matter_id.clone(),
            expires_at_unix: grant.expires_at.timestamp(),
            token: grant.token.clone(),
            revoked: false,
        };
        drop(g);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        let text = serde_json::to_string(&body).map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn write_revoked_file(path: &Path) -> Result<(), String> {
        let body = serde_json::json!({
            "revoked": true,
            "session_id": "",
            "matter_id": "",
            "expires_at_unix": 0,
            "token": ""
        });
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        f.write_all(body.to_string().as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// MCP 进程从快照建立只含这一条授权的表。空文件、撤销、过期、会话不一致都拒绝。
    pub fn from_grant_file(path: &Path, expect_session: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("grant_file: {e}"))?;
        if text.trim().is_empty() || text.trim() == "{}" {
            return Err("scope_denied: empty grant registry".into());
        }
        let file: GrantFile = serde_json::from_str(&text).map_err(|e| format!("grant_file: {e}"))?;
        if file.revoked || file.token.is_empty() {
            return Err("scope_denied: grant revoked".into());
        }
        let expires_at = DateTime::from_timestamp(file.expires_at_unix, 0)
            .ok_or_else(|| "grant_file: bad expiry".to_string())?;
        if Utc::now() > expires_at {
            return Err("scope_denied: capability expired".into());
        }
        if file.session_id != expect_session {
            return Err(ScopeError::SessionMismatch.to_string());
        }
        let grant = CapabilityGrant {
            token: file.token,
            principal: "snapshot".into(),
            session_id: file.session_id.clone(),
            matter_id: file.matter_id,
            issued_at: Utc::now(),
            expires_at,
        };
        let mut grants = HashMap::new();
        grants.insert(grant.token.clone(), grant.clone());
        Ok(Self {
            inner: Mutex::new(Inner {
                host_session: file.session_id,
                confirmed_matter: Some(grant.matter_id.clone()),
                grants,
                generation: 1,
            }),
        })
    }
}

#[cfg(unix)]
trait OpenMode {
    fn mode(&mut self, mode: u32) -> &mut Self;
}
#[cfg(unix)]
impl OpenMode for OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptionsExt::mode(self, mode)
    }
}
#[cfg(not(unix))]
trait OpenMode {
    fn mode(&mut self, _mode: u32) -> &mut Self;
}
#[cfg(not(unix))]
impl OpenMode for OpenOptions {
    fn mode(&mut self, _mode: u32) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn confirmed(matter: &str) -> (ScopeRegistry, String, CapabilityGrant) {
        let reg = ScopeRegistry::new();
        let sid = reg.host_session_id();
        reg.confirm_selection(&sid, matter).unwrap();
        let g = reg.issue_for_confirmed(&sid, matter, 600).unwrap();
        (reg, sid, g)
    }

    #[test]
    fn empty_registry_rejects_forged_token() {
        let reg = ScopeRegistry::new();
        let err = reg
            .validate("capv1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "case-001", "nope")
            .unwrap_err();
        assert!(matches!(err, ScopeError::UnknownToken));
    }

    #[test]
    fn other_existing_matter_is_not_self_granted() {
        let reg = ScopeRegistry::new();
        let sid = reg.host_session_id();
        reg.confirm_selection(&sid, "case-001").unwrap();
        let err = reg.issue_for_confirmed(&sid, "case-002", 600).unwrap_err();
        assert!(err.contains("not the host-confirmed"));
    }

    #[test]
    fn revoke_replay_and_session_and_expiry() {
        let (reg, sid, g) = confirmed("case-001");
        assert!(reg.validate(&g.token, "case-002", &sid).is_err());
        assert!(reg.validate(&g.token, "case-001", "other-session").is_err());
        reg.revoke(&g.token);
        assert!(matches!(
            reg.validate(&g.token, "case-001", &sid).unwrap_err(),
            ScopeError::UnknownToken
        ));
        reg.confirm_selection(&sid, "case-001").unwrap();
        let expired = reg.issue_for_confirmed(&sid, "case-001", -1).unwrap();
        assert!(matches!(
            reg.validate(&expired.token, "case-001", &sid).unwrap_err(),
            ScopeError::Expired
        ));
    }

    #[test]
    fn grant_file_roundtrip_and_revoked_restart() {
        let (reg, sid, g) = confirmed("case-001");
        let path = std::env::temp_dir().join(format!("ludao-grant-{}-{}.json", std::process::id(), random_id()));
        reg.write_grant_file(&g.token, &path).unwrap();
        let loaded = ScopeRegistry::from_grant_file(&path, &sid).unwrap();
        assert!(loaded.validate(&g.token, "case-001", &sid).is_ok());
        ScopeRegistry::write_revoked_file(&path).unwrap();
        let err = ScopeRegistry::from_grant_file(&path, &sid).unwrap_err();
        assert!(err.contains("revoked"));
        let _ = std::fs::remove_file(&path);
    }
}
