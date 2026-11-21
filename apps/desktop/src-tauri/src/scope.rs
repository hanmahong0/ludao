//! 宿主签发的短期能力（principal + matter + 过期），不接受模型/前端自报 scope。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64U, Engine};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

const DEFAULT_PRINCIPAL: &str = "local-desktop-user";

#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    pub token: String,
    pub principal: String,
    pub matter_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct ScopeRegistry {
    inner: Mutex<HashMap<String, CapabilityGrant>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    MissingToken,
    UnknownToken,
    Expired,
    MatterMismatch { expected: String, got: String },
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
        }
    }
}

impl ScopeRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// UI 选择案件后由宿主签发；`ttl_secs` 默认 3600。
    fn hmac_key() -> Vec<u8> {
        if let Ok(h) = std::env::var("LUDAO_SCOPE_HMAC") {
            if let Ok(b) = hex::decode(h.trim()) {
                if !b.is_empty() {
                    return b;
                }
            }
        }
        b"ludao-scope-hmac-dev-only".to_vec()
    }

    fn sign_token(principal: &str, matter_id: &str, exp: i64) -> String {
        let payload = format!("{principal}|{matter_id}|{exp}");
        let mut mac = Sha256::new();
        mac.update(ScopeRegistry::hmac_key());
        mac.update(payload.as_bytes());
        let sig = hex::encode(mac.finalize());
        let blob = format!("{payload}|{sig}");
        format!("capv1.{}", B64U.encode(blob.as_bytes()))
    }

    fn verify_token(token: &str, matter_id: &str) -> Result<CapabilityGrant, ScopeError> {
        let token = token.trim();
        let Some(rest) = token.strip_prefix("capv1.") else {
            return Err(ScopeError::UnknownToken);
        };
        let bytes = B64U.decode(rest).map_err(|_| ScopeError::UnknownToken)?;
        let s = String::from_utf8(bytes).map_err(|_| ScopeError::UnknownToken)?;
        let mut parts = s.rsplitn(2, '|');
        let sig = parts.next().ok_or(ScopeError::UnknownToken)?;
        let payload = parts.next().ok_or(ScopeError::UnknownToken)?;
        let mut fields = payload.split('|');
        let principal = fields.next().ok_or(ScopeError::UnknownToken)?;
        let mid = fields.next().ok_or(ScopeError::UnknownToken)?;
        let exp: i64 = fields
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or(ScopeError::UnknownToken)?;
        let expected = ScopeRegistry::sign_token(principal, mid, exp);
        if expected != token {
            return Err(ScopeError::UnknownToken);
        }
        if Utc::now().timestamp() > exp {
            return Err(ScopeError::Expired);
        }
        if mid != matter_id {
            return Err(ScopeError::MatterMismatch {
                expected: mid.to_string(),
                got: matter_id.to_string(),
            });
        }
        Ok(CapabilityGrant {
            token: token.to_string(),
            principal: principal.to_string(),
            matter_id: mid.to_string(),
            issued_at: Utc::now(),
            expires_at: DateTime::from_timestamp(exp, 0).unwrap_or_else(Utc::now),
        })
    }

    pub fn issue_for_matter(
        &self,
        matter_id: &str,
        ttl_secs: i64,
    ) -> Result<CapabilityGrant, String> {
        if matter_id.trim().is_empty() {
            return Err("scope_issue: empty matter_id".into());
        }
        let now = Utc::now();
        let exp = (now + Duration::seconds(ttl_secs.max(60))).timestamp();
        let token = Self::sign_token(DEFAULT_PRINCIPAL, matter_id, exp);
        let grant = CapabilityGrant {
            token: token.clone(),
            principal: DEFAULT_PRINCIPAL.to_string(),
            matter_id: matter_id.to_string(),
            issued_at: now,
            expires_at: DateTime::from_timestamp(exp, 0).unwrap_or(now),
        };
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .insert(token.clone(), grant.clone());
        Ok(grant)
    }

    pub fn validate(&self, token: &str, matter_id: &str) -> Result<CapabilityGrant, ScopeError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(ScopeError::MissingToken);
        }
        if let Ok(g) = Self::verify_token(token, matter_id) {
            return Ok(g);
        }
        let map = self.inner.lock().expect("scope lock");
        let grant = map.get(token).ok_or(ScopeError::UnknownToken)?.clone();
        drop(map);
        if Utc::now() > grant.expires_at {
            return Err(ScopeError::Expired);
        }
        if grant.matter_id != matter_id {
            return Err(ScopeError::MatterMismatch {
                expected: grant.matter_id.clone(),
                got: matter_id.to_string(),
            });
        }
        Ok(grant)
    }

    pub fn revoke(&self, token: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(token.trim());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_matter_rejected() {
        let reg = ScopeRegistry::new();
        let g = reg.issue_for_matter("case-001", 600).unwrap();
        assert!(reg.validate(&g.token, "case-002").is_err());
        assert!(reg.validate(&g.token, "case-001").is_ok());
    }
}