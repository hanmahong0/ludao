//! Tauri 2 入口：打开加密库 → 注入 State → 注册 invoke_handler。
//! 命令薄包装在此；纯逻辑在 commands.rs（集成测试直接调那层，不经 webview）。

pub mod assistant;
pub mod bootstrap;
pub mod commands;
pub mod config_map;
pub mod db;
pub mod dto;
pub mod query_sanitize;
pub mod scope;

use assistant::AssistantSupervisor;
use scope::ScopeRegistry;
use dto::*;
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::{Manager, State};

/// 全局 DB 句柄（SQLCipher 连接非 Sync，故 Mutex 包裹）。
pub struct DbState(pub Mutex<Connection>);

// ── #[tauri::command] 薄包装 ──────────────────────────────────────────────────

#[tauri::command]
fn case_list(state: State<DbState>) -> Result<Vec<CaseDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::case_list(&conn)
}

#[tauri::command]
fn case_get(state: State<DbState>, id: String) -> Result<Option<CaseDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::case_get(&conn, &id)
}

#[tauri::command]
fn evidence_list(state: State<DbState>, case_id: String) -> Result<Vec<EvidenceDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::evidence_list(&conn, &case_id)
}

#[tauri::command]
fn fact_list(state: State<DbState>, case_id: String) -> Result<Vec<FactDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::fact_list(&conn, &case_id)
}

#[tauri::command]
fn deadline_compute(
    state: State<DbState>,
    rule_id: String,
    anchor_date: String,
    anchor_confirmed: bool,
    doc_type: Option<String>,
) -> Result<DueDateDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::deadline_compute(
        &conn,
        &rule_id,
        &anchor_date,
        anchor_confirmed,
        doc_type.as_deref(),
    )
}

#[tauri::command]
fn gap_diagnose(state: State<DbState>, case_id: String) -> Result<GapDiagnosisDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::gap_diagnose(&conn, &case_id)
}

#[tauri::command]
fn case_create(state: State<DbState>, input: dto::CaseCreateInput) -> Result<CaseDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::case_create(&conn, input)
}

#[tauri::command]
fn case_update(state: State<DbState>, input: dto::CaseUpdateInput) -> Result<CaseDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::case_update(&conn, input)
}

#[tauri::command]
fn evidence_create(
    state: State<DbState>,
    input: dto::EvidenceCreateInput,
) -> Result<EvidenceDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::evidence_create(&conn, input)
}

#[tauri::command]
fn evidence_update(
    state: State<DbState>,
    input: dto::EvidenceUpdateInput,
) -> Result<EvidenceDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::evidence_update(&conn, input)
}

#[tauri::command]
fn fact_upsert(state: State<DbState>, input: dto::FactUpsertInput) -> Result<FactDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::fact_upsert(&conn, input)
}

#[tauri::command]
fn deadline_create(
    state: State<DbState>,
    input: dto::DeadlineCreateInput,
) -> Result<DeadlineDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::deadline_create(&conn, input)
}

#[tauri::command]
fn deadline_confirm_anchor(
    state: State<DbState>,
    input: dto::DeadlineConfirmAnchorInput,
) -> Result<DeadlineDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::deadline_confirm_anchor(&conn, input)
}

#[tauri::command]
fn deadline_list_by_case(
    state: State<DbState>,
    case_id: String,
) -> Result<Vec<DeadlineDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::deadline_list_by_case(&conn, &case_id)
}

#[tauri::command]
fn triage_today(state: State<DbState>) -> Result<Vec<TriageItemDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::triage_today(&conn)
}

#[tauri::command]
fn config_status(state: State<DbState>) -> Result<Vec<ConfigStatusDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::config_status(&conn)
}

#[tauri::command]
fn agent_context(state: State<DbState>, case_id: String) -> Result<DesensitizedContextDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::agent_context(&conn, &case_id)
}

#[tauri::command]
fn triage_context(state: State<DbState>) -> Result<TodayDigestDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::triage_context(&conn)
}

#[tauri::command]
fn template_list(state: State<DbState>) -> Result<Vec<TemplateDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::template_list(&conn)
}

#[tauri::command]
fn conflict_scan(state: State<DbState>) -> Result<Vec<ConflictFindingDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::conflict_scan(&conn)
}

#[tauri::command]
fn search_public_cases(
    state: State<DbState>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<PublicCaseHitDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::search_public_cases(&conn, &query, limit.unwrap_or(10))
}

#[tauri::command]
fn compare_public_cases(
    state: State<DbState>,
    case_version_a: String,
    case_version_b: String,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::compare_public_cases(&conn, &case_version_a, &case_version_b)
}

#[tauri::command]
fn assistant_status(sup: State<AssistantSupervisor>) -> Result<AssistantRuntimeStatusDto, String> {
    Ok(assistant::assistant_status(&sup))
}

#[tauri::command]
fn assistant_set_enabled(
    sup: State<AssistantSupervisor>,
    enabled: bool,
) -> Result<AssistantRuntimeStatusDto, String> {
    assistant::assistant_set_enabled(&sup, enabled)
}

#[tauri::command]
fn assistant_run_draft(
    sup: State<AssistantSupervisor>,
    scopes: State<ScopeRegistry>,
    state: State<DbState>,
    input: AssistantRunDraftInput,
) -> Result<AssistantDraftResultDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    assistant::assistant_run_draft(&conn, &sup, &scopes, input)
}

#[tauri::command]
fn scope_grant_for_matter(
    scopes: State<ScopeRegistry>,
    state: State<DbState>,
    matter_id: String,
) -> Result<CapabilityGrantDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    if commands::case_get(&conn, &matter_id)?.is_none() {
        return Err("scope_issue: unknown matter".into());
    }
    let g = scopes.issue_for_matter(&matter_id, 3600)?;
    Ok(CapabilityGrantDto {
        token: g.token,
        matter_id: g.matter_id,
        expires_at: g.expires_at.to_rfc3339(),
    })
}

#[tauri::command]
fn search_private_documents(
    state: State<DbState>,
    scopes: State<ScopeRegistry>,
    capability_token: String,
    matter_id: String,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<crate::dto::PrivatePassageHitDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    commands::search_private_documents(
        &conn,
        &scopes,
        &capability_token,
        &matter_id,
        &query,
        limit.unwrap_or(10),
    )
}

#[tauri::command]
fn assistant_cancel(sup: State<AssistantSupervisor>, task_id: String) -> Result<(), String> {
    assistant::assistant_cancel(&sup, &task_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // app-data 下的加密库；首启 apply_schema + seed demo。
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("lurus.db");
            if std::env::var("LUDAO_SYNTHETIC_DEV_BUNDLE")
                .map(|v| v == "1")
                .unwrap_or(false)
            {
                db::enable_dev_config_trust_for_tests();
            }
            let conn = db::open_and_init(&db_path.to_string_lossy())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            // 子进程 MCP / sidecar 只读继承，禁止前端改写。
            std::env::set_var("LUDAO_DB_PATH", &db_path);
            app.manage(DbState(Mutex::new(conn)));
            app.manage(ScopeRegistry::new());
            app.manage(AssistantSupervisor::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            case_list,
            case_get,
            evidence_list,
            fact_list,
            deadline_compute,
            gap_diagnose,
            case_create,
            case_update,
            evidence_create,
            evidence_update,
            fact_upsert,
            deadline_create,
            deadline_confirm_anchor,
            deadline_list_by_case,
            triage_today,
            config_status,
            agent_context,
            triage_context,
            template_list,
            conflict_scan,
            search_public_cases,
            search_private_documents,
            scope_grant_for_matter,
            compare_public_cases,
            assistant_status,
            assistant_set_enabled,
            assistant_run_draft,
            assistant_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
