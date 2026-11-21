//! AgentSupervisor：固定 ZeroClaw 二进制路径、健康检查、任务状态（默认关闭）。

use super::zeroclaw_rpc::{self, PINNED_COMMIT, PINNED_TAG};
use crate::commands;
use crate::dto::{
    AssistantCitationDto, AssistantDraftResultDto, AssistantRunDraftInput, AssistantRuntimeStatusDto,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_SOCKET: &str = "/tmp/ludao-zeroclaw-test.sock";

pub struct AssistantSupervisor {
    inner: Mutex<SupervisorInner>,
}

#[derive(Debug, Clone)]
struct SupervisorInner {
    enabled: bool,
    last_error: Option<String>,
    handshake_ok: bool,
    socket_path: PathBuf,
    binary_path: Option<PathBuf>,
}

impl Default for SupervisorInner {
    fn default() -> Self {
        Self {
            enabled: false,
            last_error: None,
            handshake_ok: false,
            socket_path: PathBuf::from(
                std::env::var("LUDAO_ZEROCLAW_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET.to_string()),
            ),
            binary_path: std::env::var("LUDAO_ZEROCLAW_BIN")
                .ok()
                .map(PathBuf::from)
                .or_else(default_binary_path),
        }
    }
}

fn default_binary_path() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("vendor/zeroclaw/zeroclaw"),
        PathBuf::from("/workspace/vendor/zeroclaw/zeroclaw"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

impl AssistantSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SupervisorInner::default()),
        }
    }

    fn status_locked(inner: &SupervisorInner) -> AssistantRuntimeStatusDto {
        let binary_present = inner
            .binary_path
            .as_ref()
            .map(|p| p.is_file())
            .unwrap_or(false);
        AssistantRuntimeStatusDto {
            enabled: inner.enabled,
            configured: binary_present,
            runtime_kind: if inner.enabled { "zeroclaw".into() } else { "disabled".into() },
            pinned_version: format!("{PINNED_TAG}@{PINNED_COMMIT}"),
            binary_present,
            socket_path: Some(inner.socket_path.to_string_lossy().into_owned()),
            last_error: inner.last_error.clone(),
            handshake_ok: inner.handshake_ok,
            model_configured: false,
            disclosure: "Windows 上 ZeroClaw 无 OS 级沙箱；助手默认关闭。私卷 scope 由宿主签发并逐次校验。".into(),
        }
    }

    pub fn status(&self) -> AssistantRuntimeStatusDto {
        let inner = self.inner.lock().expect("supervisor lock");
        Self::status_locked(&inner)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<AssistantRuntimeStatusDto, String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        inner.enabled = enabled;
        inner.last_error = None;
        inner.handshake_ok = false;
        if enabled {
            if let Some(bin) = inner.binary_path.clone() {
                if !bin.is_file() {
                    inner.last_error = Some(format!("未找到固定 ZeroClaw 二进制：{}", bin.display()));
                    inner.enabled = false;
                    return Ok(Self::status_locked(&inner));
                }
            } else {
                inner.last_error = Some("未配置 LUDAO_ZEROCLAW_BIN 或 vendor/zeroclaw/zeroclaw".into());
                inner.enabled = false;
                return Ok(Self::status_locked(&inner));
            }
            match zeroclaw_rpc::handshake_unix(&inner.socket_path, Duration::from_secs(5)) {
                Ok(res) => {
                    inner.handshake_ok = true;
                    if res.server_version.as_deref() != Some("0.9.0") {
                        inner.last_error = Some(format!(
                            "服务端版本 {:?} 与固定 {} 不一致",
                            res.server_version, PINNED_TAG
                        ));
                    }
                }
                Err(e) => {
                    inner.handshake_ok = false;
                    inner.last_error = Some(format!("RPC 握手失败：{e}"));
                    inner.enabled = false;
                }
            }
        }
        Ok(Self::status_locked(&inner))
    }
}

pub fn assistant_status(sup: &AssistantSupervisor) -> AssistantRuntimeStatusDto {
    sup.status()
}

pub fn assistant_set_enabled(sup: &AssistantSupervisor, enabled: bool) -> Result<AssistantRuntimeStatusDto, String> {
    sup.set_enabled(enabled)
}

/// 无本地模型时仍可用宿主法律工具检索 + 规则化草稿（MOCK/REAL_RPC_STUB 分级）。
pub fn assistant_run_draft(
    conn: &Connection,
    sup: &AssistantSupervisor,
    input: AssistantRunDraftInput,
) -> Result<AssistantDraftResultDto, String> {
    let task_id = Uuid::new_v4().to_string();
    let st = sup.status();
    if !st.enabled {
        return Ok(AssistantDraftResultDto {
            task_id,
            state: "failed".into(),
            text: String::new(),
            citations: vec![],
            coverage_note: Some("助手未启用。请在设置中启用并配置 ZeroClaw 与本地模型。".into()),
            error: Some("assistant_disabled".into()),
            runtime_evidence: "mock".into(),
        });
    }

    let ctx = commands::agent_context(conn, &input.case_id)?;
    let private_hits = commands::search_private_documents(conn, &input.case_id, &input.intent, 3)?;
    let public_hits = commands::search_public_cases(conn, &input.intent, 3)?;

    let mut citations: Vec<AssistantCitationDto> = Vec::new();
    for h in &private_hits {
        citations.push(AssistantCitationDto {
            kind: "private_excerpt".into(),
            passage_id: Some(h.passage_id.clone()),
            case_version: None,
            locator: h.locator.clone(),
            excerpt: h.excerpt.clone(),
        });
    }
    for h in &public_hits {
        citations.push(AssistantCitationDto {
            kind: "public_case".into(),
            passage_id: Some(h.passage_id.clone()),
            case_version: Some(h.case_version.clone()),
            locator: h.locator.clone(),
            excerpt: h.excerpt.clone(),
        });
    }

    let runtime_evidence = if st.handshake_ok {
        "real_rpc_stub_model"
    } else {
        "mock"
    };

    let mut text = format!(
        "【分析草稿·非法律意见】案由：{}\n争点摘要：{}\n",
        ctx.case_type, ctx.facts_summary
    );
    text.push_str("检索到的授权材料摘录：\n");
    for (i, c) in citations.iter().enumerate() {
        text.push_str(&format!("{}. [{}] {} …\n", i + 1, c.kind, &c.excerpt[..c.excerpt.len().min(80)]));
    }
    if public_hits.is_empty() && private_hits.is_empty() {
        text.push_str("\n资料不足以形成判断；未编造案号或页码。\n");
    }
  text.push_str("\n（未配置本地 LLM：本结果为宿主工具检索 + 规则化汇总，不含模型推理。）\n");

    Ok(AssistantDraftResultDto {
        task_id,
        state: "succeeded".into(),
        text,
        citations,
        coverage_note: Some("仅覆盖授权 scope 内已索引页段；未索引页不会出现在摘录中。".into()),
        error: None,
        runtime_evidence: runtime_evidence.into(),
    })
}

pub fn assistant_cancel(_sup: &AssistantSupervisor, _task_id: &str) -> Result<(), String> {
    Ok(())
}