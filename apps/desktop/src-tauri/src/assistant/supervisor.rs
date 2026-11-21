//! AgentSupervisor：子进程、RPC 会话、任务状态（默认关闭）。

use super::sidecar::{spawn_daemon, LudaoMcpEnv, SidecarHandle};
use super::zeroclaw_rpc::{self, RpcClient, PINNED_COMMIT, PINNED_TAG};
use crate::commands;
use crate::dto::{
    AssistantCitationDto, AssistantDraftResultDto, AssistantRunDraftInput, AssistantRuntimeStatusDto,
};
use crate::scope::ScopeRegistry;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

#[cfg(unix)]
struct ActiveUnixRpc {
    sidecar: SidecarHandle,
    client: RpcClient,
}

pub struct AssistantSupervisor {
    inner: Mutex<SupervisorInner>,
}

#[derive(Default)]
struct SupervisorInner {
    enabled: bool,
    last_error: Option<String>,
    handshake_ok: bool,
    product_session_ready: bool,
    mcp_attached: bool,
    model_configured: bool,
    last_mcp_fingerprint: Option<String>,
    socket_path: Option<PathBuf>,
    #[cfg(unix)]
    active: Option<ActiveUnixRpc>,
    active_session_id: Option<String>,
    active_task_id: Option<String>,
}

impl AssistantSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SupervisorInner::default()),
        }
    }

    #[cfg(unix)]
    fn mcp_fingerprint(cap: &str, matter: &str) -> Result<String, String> {
        let db = std::env::var("LUDAO_DB_PATH").map_err(|_| "LUDAO_DB_PATH 未设置".to_string())?;
        Ok(format!("{db}|{matter}|{}", &cap[..cap.len().min(24)]))
    }

    #[cfg(unix)]
    fn ensure_product_mcp(
        inner: &mut SupervisorInner,
        cap: &str,
        matter: &str,
    ) -> Result<(), String> {
        let fp = Self::mcp_fingerprint(cap, matter)?;
        if inner.mcp_attached && inner.last_mcp_fingerprint.as_deref() == Some(&fp) {
            return Ok(());
        }
        let db_path = std::env::var("LUDAO_DB_PATH").map_err(|_| "LUDAO_DB_PATH 未设置".to_string())?;
        let mcp = LudaoMcpEnv {
            db_path: PathBuf::from(db_path),
            capability_token: cap.to_string(),
            matter_scope: matter.to_string(),
        };
        let (data_dir, socket) = if let Some(a) = inner.active.as_ref() {
            (a.sidecar.data_dir.clone(), a.sidecar.socket_path.clone())
        } else {
            let data_dir = std::env::temp_dir().join(format!("ludao-zc-{}", Uuid::new_v4()));
            let socket = data_dir.join("daemon.sock");
            (data_dir, socket)
        };
        if let Some(mut a) = inner.active.take() {
            a.sidecar.stop();
        }
        let sidecar = spawn_daemon(&data_dir, &socket, Some(&mcp))?;
        let mut client = zeroclaw_rpc::RpcClient::connect(&socket, Duration::from_secs(12))?;
        client.initialize()?;
        inner.socket_path = Some(socket);
        inner.handshake_ok = true;
        inner.enabled = true;
        inner.product_session_ready = true;
        inner.mcp_attached = super::sidecar::resolve_legal_mcp_binary().is_ok();
        inner.last_mcp_fingerprint = Some(fp);
        inner.active = Some(ActiveUnixRpc { sidecar, client });
        Ok(())
    }

    fn status_locked(inner: &SupervisorInner) -> AssistantRuntimeStatusDto {
        AssistantRuntimeStatusDto {
            enabled: inner.enabled,
            configured: super::sidecar::resolve_binary_path().is_ok(),
            runtime_kind: if inner.enabled {
                "zeroclaw".into()
            } else {
                "disabled".into()
            },
            pinned_version: format!("{PINNED_TAG}@{PINNED_COMMIT}"),
            binary_present: super::sidecar::resolve_binary_path().is_ok(),
            socket_path: inner
                .socket_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            last_error: inner.last_error.clone(),
            handshake_ok: inner.handshake_ok,
            model_configured: inner.model_configured,
            disclosure: "Windows 无 OS 级沙箱。助手默认关闭；scope 由宿主 capability 签发。未配置本地模型时不发起 LLM turn。".into(),
            mcp_attached: inner.mcp_attached,
        }
    }

    pub fn status(&self) -> AssistantRuntimeStatusDto {
        let inner = self.inner.lock().expect("supervisor lock");
        Self::status_locked(&inner)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<AssistantRuntimeStatusDto, String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        if !enabled {
            inner.enabled = false;
            inner.handshake_ok = false;
            inner.product_session_ready = false;
            inner.mcp_attached = false;
            inner.last_mcp_fingerprint = None;
            #[cfg(unix)]
            if let Some(mut a) = inner.active.take() {
                a.sidecar.stop();
            }
            inner.active_session_id = None;
            return Ok(Self::status_locked(&inner));
        }

        #[cfg(unix)]
        {
            let data_dir = std::env::temp_dir().join(format!("ludao-zc-{}", Uuid::new_v4()));
            let socket = data_dir.join("daemon.sock");
            match spawn_daemon(&data_dir, &socket, None) {
                Ok(mut sidecar) => {
                    inner.socket_path = Some(socket.clone());
                    match RpcClient::connect(&socket, Duration::from_secs(8)) {
                        Ok(mut client) => match client.initialize() {
                            Ok(_) => {
                                inner.handshake_ok = true;
                                inner.enabled = true;
                                inner.last_error = None;
                                inner.product_session_ready = true;
                                inner.mcp_attached = false;
                                inner.active = Some(ActiveUnixRpc { sidecar, client });
                            }
                            Err(e) => {
                                sidecar.stop();
                                inner.enabled = false;
                                inner.handshake_ok = false;
                                inner.last_error = Some(format!("产品 RPC initialize 失败: {e}"));
                            }
                        },
                        Err(e) => {
                            sidecar.stop();
                            inner.enabled = false;
                            inner.last_error = Some(e);
                        }
                    }
                }
                Err(e) => {
                    inner.enabled = false;
                    inner.last_error = Some(e);
                }
            }
        }
        #[cfg(not(unix))]
        {
            inner.enabled = false;
            inner.last_error = Some("当前平台未实现 sidecar".into());
        }
        Ok(Self::status_locked(&inner))
    }

    fn rules_summary_draft(
        conn: &Connection,
        scope: &ScopeRegistry,
        input: &AssistantRunDraftInput,
        task_id: &str,
        evidence: &str,
    ) -> Result<AssistantDraftResultDto, String> {
        scope
            .validate(&input.capability_token, &input.case_id)
            .map_err(|e| e.to_string())?;
        let ctx = commands::agent_context(conn, &input.case_id)?;
        let private_hits = commands::search_private_documents(
            conn,
            scope,
            &input.capability_token,
            &input.case_id,
            &input.intent,
            3,
        )?;
        let public_hits = commands::search_public_cases(conn, &input.intent, 3)?;
        let mut citations = Vec::new();
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
        let mut text = format!(
            "【规则摘要·非模型输出】案由：{}\n事实摘要：{}\n",
            ctx.case_type, ctx.facts_summary
        );
        for (i, c) in citations.iter().enumerate() {
            text.push_str(&format!(
                "{}. [{}] {} …\n",
                i + 1,
                c.kind,
                &c.excerpt[..c.excerpt.len().min(80)]
            ));
        }
        text.push_str("\n（未发生 LLM turn；仅供检索对照。）\n");
        Ok(AssistantDraftResultDto {
            task_id: task_id.to_string(),
            state: "succeeded".into(),
            text,
            citations,
            coverage_note: Some("rules_summary_only".into()),
            error: None,
            runtime_evidence: evidence.into(),
        })
    }
}

pub fn assistant_status(sup: &AssistantSupervisor) -> AssistantRuntimeStatusDto {
    sup.status()
}

pub fn assistant_set_enabled(
    sup: &AssistantSupervisor,
    enabled: bool,
) -> Result<AssistantRuntimeStatusDto, String> {
    sup.set_enabled(enabled)
}

pub fn assistant_run_draft(
    conn: &Connection,
    sup: &AssistantSupervisor,
    scope: &ScopeRegistry,
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
            coverage_note: None,
            error: Some("assistant_disabled".into()),
            runtime_evidence: "disabled".into(),
        });
    }

    scope
        .validate(&input.capability_token, &input.case_id)
        .map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        let mut inner = sup.inner.lock().map_err(|e| e.to_string())?;
        inner.active_task_id = Some(task_id.clone());
        if let Err(e) = AssistantSupervisor::ensure_product_mcp(&mut inner, &input.capability_token, &input.case_id) {
            drop(inner);
            return Ok(AssistantDraftResultDto {
                task_id: task_id.clone(),
                state: "failed".into(),
                text: String::new(),
                citations: vec![],
                coverage_note: None,
                error: Some(e),
                runtime_evidence: "product_handshake_verified".into(),
            });
        }
        if let Some(active) = inner.active.as_mut() {
            if !active.sidecar.running() {
                inner.enabled = false;
                inner.handshake_ok = false;
                return Ok(AssistantDraftResultDto {
                    task_id: task_id.clone(),
                    state: "interrupted".into(),
                    text: String::new(),
                    citations: vec![],
                    coverage_note: None,
                    error: Some("sidecar_exited".into()),
                    runtime_evidence: "product_handshake_verified".into(),
                });
            } else {
                let session_id = match active.client.session_new("default") {
                    Ok(id) => id,
                    Err(e) => {
                        inner.active_session_id = None;
                        drop(inner);
                        return AssistantSupervisor::rules_summary_draft(
                            conn,
                            scope,
                            &input,
                            &task_id,
                            "product_handshake_verified",
                        )
                        .map(|mut r| {
                            r.error = Some(format!("session/new failed: {e}"));
                            r.state = "failed".into();
                            r
                        });
                    }
                };
                inner.active_session_id = Some(session_id.clone());
                drop(inner);
                let prompt = format!(
                    "仅基于授权案件工具结果回答（无工具权不得臆测）: {}",
                    input.intent
                );
                let mut inner = sup.inner.lock().map_err(|e| e.to_string())?;
                let Some(active) = inner.active.as_mut() else {
                    return AssistantSupervisor::rules_summary_draft(
                        conn,
                        scope,
                        &input,
                        &task_id,
                        "product_handshake_verified",
                    );
                };
                if active.client.session_prompt(&session_id, &prompt).is_err() {
                    drop(inner);
                    return AssistantSupervisor::rules_summary_draft(
                        conn,
                        scope,
                        &input,
                        &task_id,
                        "product_handshake_verified",
                    );
                }
                let turn = active.client.wait_turn(&session_id, Duration::from_secs(8));
                drop(inner);
                match turn {
                    Ok(t) if t.completed && !t.text_chunks.is_empty() => {
                        return Ok(AssistantDraftResultDto {
                            task_id,
                            state: "succeeded".into(),
                            text: t.text_chunks.join(""),
                            citations: vec![],
                            coverage_note: Some("product_tool_turn_verified".into()),
                            error: None,
                            runtime_evidence: "product_tool_turn_verified".into(),
                        });
                    }
                    Ok(_) => {
                        return AssistantSupervisor::rules_summary_draft(
                            conn,
                            scope,
                            &input,
                            &task_id,
                            "product_handshake_verified",
                        );
                    }
                    Err(e) => {
                        return Ok(AssistantDraftResultDto {
                            task_id,
                            state: "failed".into(),
                            text: String::new(),
                            citations: vec![],
                            coverage_note: None,
                            error: Some(e),
                            runtime_evidence: "product_handshake_verified".into(),
                        });
                    }
                }
            }
        }
    }

    AssistantSupervisor::rules_summary_draft(conn, scope, &input, &task_id, "rules_summary_only")
}

pub fn assistant_cancel(sup: &AssistantSupervisor, task_id: &str) -> Result<(), String> {
    let mut inner = sup.inner.lock().map_err(|e| e.to_string())?;
    if inner.active_task_id.as_deref() != Some(task_id) {
        return Err("cancel: unknown task".into());
    }
    #[cfg(unix)]
    {
        let sid = inner.active_session_id.clone();
        if let (Some(active), Some(sid)) = (inner.active.as_mut(), sid) {
            let _ = active.client.session_cancel(&sid);
            let _ = active.client.session_state(&sid);
        }
    }
    inner.active_task_id = None;
    Ok(())
}