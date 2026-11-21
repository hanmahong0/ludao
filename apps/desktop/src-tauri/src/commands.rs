//! 命令处理函数（纯逻辑，吃 &Connection，不经 webview）——验收 §2 的集成测试直接调它们。
//! lib.rs 的 #[tauri::command] 仅是薄包装：取连接 → 调这里 → map 错误。
//!
//! 打通的三库：core（加密库读写）、deadline-engine（compute）、gap-engine（diagnose）。

use crate::config_map::{DeadlineRuleSet, HolidayConfig};
use crate::dto::*;
use chrono::NaiveDate;
use deadline_engine::{Ctx, MinimalDocTypeCel};
use gap_engine::{Evidence as GapEvidence, Profile};
use rusqlite::Connection;

/// 读当前生效配置的 payload（来自 verify→cache，首启由 bootstrap 落库）。
/// 缺失即报错而非回退到内置 include_str!——保证所有读取都经过验签缓存这一条路径。
fn active_payload(conn: &Connection, kind: &str, config_id: &str) -> Result<String, String> {
    vault::config::active_config(conn, kind, config_id)
        .map_err(|e| e.to_string())?
        .map(|c| c.payload)
        .ok_or_else(|| format!("无生效配置 {kind}/{config_id}（bootstrap 未执行？）"))
}

// ── 小工具：从 attributes JSON 取字段 ─────────────────────────────────────────

fn attr_f64(attrs: &serde_json::Value, key: &str) -> Option<f64> {
    attrs.get(key).and_then(|v| v.as_f64())
}
fn attr_bool(attrs: &serde_json::Value, key: &str) -> Option<bool> {
    attrs.get(key).and_then(|v| v.as_bool())
}
fn attr_str(attrs: &serde_json::Value, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ── case.list / case.get ──────────────────────────────────────────────────────

fn row_to_case(
    id: String,
    case_no: Option<String>,
    title: String,
    case_type: String,
    stage: String,
    status: String,
    attributes: String,
    updated_at: String,
) -> CaseDto {
    let attrs: serde_json::Value =
        serde_json::from_str(&attributes).unwrap_or(serde_json::json!({}));
    CaseDto {
        id,
        case_no: case_no.unwrap_or_default(),
        title,
        case_type,
        case_type_label: attr_str(&attrs, "caseTypeLabel").unwrap_or_default(),
        stage,
        status,
        amount: attr_f64(&attrs, "amount"),
        includes_interest: attr_bool(&attrs, "includesInterest"),
        next_deadline_at: attr_str(&attrs, "nextDeadlineAt"),
        updated_at,
    }
}

pub fn case_list(conn: &Connection) -> Result<Vec<CaseDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_no, title, case_type, stage, status, attributes, _updated_at \
             FROM \"case\" WHERE _deleted = 0 ORDER BY _updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(row_to_case(
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn case_get(conn: &Connection, id: &str) -> Result<Option<CaseDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_no, title, case_type, stage, status, attributes, _updated_at \
             FROM \"case\" WHERE id = ?1 AND _deleted = 0",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([id], |r| {
            Ok(row_to_case(
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

// ── evidence.list ─────────────────────────────────────────────────────────────

pub fn evidence_list(conn: &Connection, case_id: &str) -> Result<Vec<EvidenceDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_id, seq, name, source, form, proof_target, three_natures, linked_element_ids \
             FROM evidence WHERE case_id = ?1 AND _deleted = 0 ORDER BY seq",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([case_id], |r| {
            let three_raw: String = r.get(7)?;
            let linked_raw: String = r.get(8)?;
            let three: ThreeNatures = serde_json::from_str(&three_raw).unwrap_or(ThreeNatures {
                authenticity: false,
                legality: false,
                relevance: false,
            });
            let linked: Vec<String> = serde_json::from_str(&linked_raw).unwrap_or_default();
            Ok(EvidenceDto {
                id: r.get(0)?,
                case_id: r.get(1)?,
                seq: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                name: r.get(3)?,
                source: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                form: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                proof_target: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                three_natures: three,
                linked_element_ids: linked,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

// ── fact.list ─────────────────────────────────────────────────────────────────

pub fn fact_list(conn: &Connection, case_id: &str) -> Result<Vec<FactDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_id, fact_date, description, source_evidence_ids \
             FROM fact WHERE case_id = ?1 AND _deleted = 0 ORDER BY fact_date",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([case_id], |r| {
            let ev_raw: String = r.get(4)?;
            let ev: Vec<String> = serde_json::from_str(&ev_raw).unwrap_or_default();
            Ok(FactDto {
                id: r.get(0)?,
                case_id: r.get(1)?,
                fact_date: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                summary: r.get(3)?,
                evidence_ids: ev,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

// ── deadline.compute（core 数据? 不需 —— 纯引擎 + config）─────────────────────

/// 调 deadline-engine：从 cn-civil-first-instance.json 找 rule，从 cn-statutory-2026.json
/// 取节假日表，映射→ compute。anchor_confirmed 由 ctx_anchor_confirmed 控制护栏（G6）。
pub fn deadline_compute(
    conn: &Connection,
    rule_id: &str,
    anchor_date: &str,
    ctx_anchor_confirmed: bool,
    doc_type: Option<&str>,
) -> Result<DueDateDto, String> {
    let rules_json = active_payload(conn, "deadline-rules", "cn-civil-first-instance")?;
    let holidays_json = active_payload(conn, "holidays", "cn-statutory-2026")?;
    let ruleset: DeadlineRuleSet =
        serde_json::from_str(&rules_json).map_err(|e| format!("parse rules: {e}"))?;
    let holiday_cfg: HolidayConfig =
        serde_json::from_str(&holidays_json).map_err(|e| format!("parse holidays: {e}"))?;

    let rule = ruleset.find_rule(rule_id)?;
    let holidays = holiday_cfg.to_holiday_set()?;
    let anchor = NaiveDate::parse_from_str(anchor_date, "%Y-%m-%d")
        .map_err(|e| format!("bad anchor_date: {e}"))?;

    let mut ctx = Ctx::new(ctx_anchor_confirmed);
    if let Some(dt) = doc_type {
        ctx = ctx.with_field("doc.type", dt);
    }

    let due = deadline_engine::compute(
        &rule,
        anchor,
        &holidays,
        &ctx,
        &MinimalDocTypeCel,
        &ruleset.ruleset_version(),
    )
    // EngineError = 拒算信号；映射为字符串错误交上层（UI 告警 / custody_event）。
    .map_err(|e| format!("EngineRefused: {e}"))?;

    Ok(DueDateDto {
        due_date: due.date.format("%Y-%m-%d").to_string(),
        used_ruleset_version: due.used_ruleset_version,
        used_holiday_table: due.used_holiday_table,
        trace: due.trace,
    })
}

// ── gap.diagnose（core 取该案已编目证据 + gap-engine 对照要件）────────────────

pub fn gap_diagnose(conn: &Connection, case_id: &str) -> Result<GapDiagnosisDto, String> {
    // 案由 → claim-elements config_id（命名一致：private_lending / sales_contract / traffic_accident）。
    let case_type: String = conn
        .query_row(
            "SELECT case_type FROM \"case\" WHERE id = ?1",
            [case_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    // 该案由暂无要件配置（如 lease 未接入）→ 返回空诊断而非报错。
    let profile_json = match vault::config::active_config(conn, "claim-elements", &case_type)
        .map_err(|e| e.to_string())?
    {
        Some(c) => c.payload,
        None => {
            return Ok(GapDiagnosisDto {
                satisfied: vec![],
                gaps: vec![],
                needs_cloud_judgment: vec![],
            });
        }
    };
    let profile: Profile =
        serde_json::from_str(&profile_json).map_err(|e| format!("parse profile: {e}"))?;

    // 脱敏 claim 上下文：覆盖各案由 gapRules 引用的全部 claim.* 字段（缺省中性值，免 CEL 缺键降级）。
    let claim = build_claim_ctx(conn, case_id)?;

    // 该案已编目证据 → gap-engine Evidence（仅脱敏字段）。
    let evidences = load_gap_evidences(conn, case_id)?;

    let report = gap_engine::diagnose(&profile, &evidences, &claim)
        .map_err(|e| format!("DiagnoseRefused: {e}"))?;

    // element_status 中 applicable && satisfied → satisfied[]；name 取 profile.elements。
    let name_of: std::collections::HashMap<&str, &str> = profile
        .elements
        .iter()
        .map(|e| (e.id.as_str(), e.name.as_str()))
        .collect();

    let satisfied = report
        .element_status
        .iter()
        .filter(|s| s.applicable && s.satisfied)
        .map(|s| SatisfiedElement {
            element_id: s.element_ref.clone(),
            name: name_of
                .get(s.element_ref.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default(),
        })
        .collect();

    let gaps = report
        .gaps
        .iter()
        .map(|g| GapDto {
            element_ref: g.element_ref.clone(),
            severity: g.severity.clone(),
            explain: g.explain.clone(),
            suggest: g.suggest.clone(),
        })
        .collect();

    // critical 缺口 → 标记需云端裁量复核（与契约 needsCloudJudgment 语义一致）。
    let needs_cloud_judgment = report
        .gaps
        .iter()
        .filter(|g| g.severity == "critical")
        .map(|g| NeedsCloudJudgment {
            element_id: g.element_ref.clone(),
            reason: "形式门槛缺失且属高度裁量判断，建议交云端 gapJudge 复核。".to_string(),
        })
        .collect();

    Ok(GapDiagnosisDto {
        satisfied,
        gaps,
        needs_cloud_judgment,
    })
}

/// 构建脱敏 claim 上下文：覆盖 private/sales/traffic 各案由 gapRules 引用的全部 claim.* 字段，
/// 取自 case.attributes，缺省给中性值——避免 CEL 缺键降级（spec：缺键→DEGRADED，绝不静默漏报）。
fn build_claim_ctx(conn: &Connection, case_id: &str) -> Result<serde_json::Value, String> {
    let attributes: String = conn
        .query_row(
            "SELECT attributes FROM \"case\" WHERE id = ?1",
            [case_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let attrs: serde_json::Value =
        serde_json::from_str(&attributes).unwrap_or(serde_json::json!({}));
    Ok(serde_json::json!({
        "amount": attr_f64(&attrs, "amount").unwrap_or(0.0),
        "includes_interest": attr_bool(&attrs, "includesInterest").unwrap_or(false),
        "case_stage": "first_instance",
        "type": attr_str(&attrs, "claimType").unwrap_or_default(),
        "includes_disability": attr_bool(&attrs, "includesDisability").unwrap_or(false),
        "includes_lost_income": attr_bool(&attrs, "includesLostIncome").unwrap_or(false),
        "has_quality_issue": attr_bool(&attrs, "hasQualityIssue").unwrap_or(false),
    }))
}

/// 把该案 evidence 行映射成 gap-engine 的脱敏 Evidence（form + linked + names；不读 file_ref/原文）。
fn load_gap_evidences(conn: &Connection, case_id: &str) -> Result<Vec<GapEvidence>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, form, name, linked_element_ids \
             FROM evidence WHERE case_id = ?1 AND _deleted = 0 ORDER BY seq",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([case_id], |r| {
            let id: String = r.get(0)?;
            let form: Option<String> = r.get(1)?;
            let name: String = r.get(2)?;
            let linked_raw: String = r.get(3)?;
            Ok((id, form.unwrap_or_default(), name, linked_raw))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let (id, form, name, linked_raw) = row.map_err(|e| e.to_string())?;
        let linked: Vec<String> = serde_json::from_str(&linked_raw).unwrap_or_default();
        let linked_refs: Vec<&str> = linked.iter().map(|s| s.as_str()).collect();
        let ev = GapEvidence::new(id, form, &linked_refs).with_names(&[name.as_str()]);
        out.push(ev);
    }
    Ok(out)
}

// ── outbox enqueue ─────────────────────────────────────────────────────────────

fn outbox_enqueue(
    conn: &Connection,
    op: &str,
    entity_type: &str,
    entity_id: &str,
    payload: &serde_json::Value,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO __transactions(op, entity_type, entity_id, payload, created_at) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![op, entity_type, entity_id, payload.to_string(), now],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

// ── private getters (row → DTO) ───────────────────────────────────────────────

fn evidence_get(conn: &Connection, id: &str) -> Result<EvidenceDto, String> {
    conn.query_row(
        "SELECT id, case_id, seq, name, source, form, proof_target, three_natures, linked_element_ids \
         FROM evidence WHERE id = ?1 AND _deleted = 0",
        [id],
        |r| {
            let three_raw: String = r.get(7)?;
            let linked_raw: String = r.get(8)?;
            let three: ThreeNatures = serde_json::from_str(&three_raw).unwrap_or(ThreeNatures {
                authenticity: false,
                legality: false,
                relevance: false,
            });
            let linked: Vec<String> = serde_json::from_str(&linked_raw).unwrap_or_default();
            Ok(EvidenceDto {
                id: r.get(0)?,
                case_id: r.get(1)?,
                seq: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                name: r.get(3)?,
                source: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                form: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                proof_target: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                three_natures: three,
                linked_element_ids: linked,
            })
        },
    )
    .map_err(|e| e.to_string())
}

fn fact_get(conn: &Connection, id: &str) -> Result<FactDto, String> {
    conn.query_row(
        "SELECT id, case_id, fact_date, description, source_evidence_ids \
         FROM fact WHERE id = ?1 AND _deleted = 0",
        [id],
        |r| {
            let ev_raw: String = r.get(4)?;
            let ev: Vec<String> = serde_json::from_str(&ev_raw).unwrap_or_default();
            Ok(FactDto {
                id: r.get(0)?,
                case_id: r.get(1)?,
                fact_date: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                summary: r.get(3)?,
                evidence_ids: ev,
            })
        },
    )
    .map_err(|e| e.to_string())
}

// ── write commands ─────────────────────────────────────────────────────────────

pub fn case_create(
    conn: &Connection,
    input: crate::dto::CaseCreateInput,
) -> Result<CaseDto, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let id: String = conn
        .query_row("SELECT 'case-' || lower(hex(randomblob(8)))", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;

    let stage = input.stage.unwrap_or_else(|| "intake".to_string());
    // 当事人暂存 attributes.parties（供利冲初筛）；PII 留本机，沿用 attributes 兜底不动核心表。
    let parties = serde_json::to_value(&input.parties).unwrap_or_else(|_| serde_json::json!([]));
    let attrs = serde_json::json!({
        "amount": input.amount,
        "includesInterest": input.includes_interest,
        "caseTypeLabel": input.case_type_label,
        "claimType": input.claim_type,
        "includesDisability": input.includes_disability,
        "includesLostIncome": input.includes_lost_income,
        "hasQualityIssue": input.has_quality_issue,
        "parties": parties,
    });

    conn.execute(
        "INSERT INTO \"case\"(id, case_no, title, case_type, stage, status, attributes, created_at, _rev, _updated_at, _deleted, _dirty) \
         VALUES(?1,?2,?3,?4,?5,'active',?6,?7,1,?7,0,1)",
        rusqlite::params![
            id,
            input.case_no,
            input.title,
            input.case_type,
            stage,
            attrs.to_string(),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "title": input.title,
        "caseType": input.case_type,
        "attributes": attrs,
    });
    outbox_enqueue(conn, "create", "case", &id, &payload, &now)?;

    case_get(conn, &id)?.ok_or_else(|| "case_create: row vanished".to_string())
}

pub fn case_update(
    conn: &Connection,
    input: crate::dto::CaseUpdateInput,
) -> Result<CaseDto, String> {
    let now = chrono::Utc::now().to_rfc3339();

    // Read current attrs
    let current_attrs_str: String = conn
        .query_row(
            "SELECT attributes FROM \"case\" WHERE id = ?1 AND _deleted = 0",
            [&input.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut attrs: serde_json::Value =
        serde_json::from_str(&current_attrs_str).unwrap_or(serde_json::json!({}));

    if let Some(v) = input.amount {
        attrs["amount"] = serde_json::json!(v);
    }
    if let Some(v) = input.includes_interest {
        attrs["includesInterest"] = serde_json::json!(v);
    }
    if let Some(ref v) = input.case_type_label {
        attrs["caseTypeLabel"] = serde_json::json!(v);
    }

    let title_val = input.title.as_deref().unwrap_or("");
    let stage_val = input.stage.as_deref().unwrap_or("");
    let status_val = input.status.as_deref().unwrap_or("");
    let case_no_val = input.case_no.as_deref().unwrap_or("");

    conn.execute(
        "UPDATE \"case\" SET \
           title = CASE WHEN ?2 != '' THEN ?2 ELSE title END, \
           stage = CASE WHEN ?3 != '' THEN ?3 ELSE stage END, \
           status = CASE WHEN ?4 != '' THEN ?4 ELSE status END, \
           case_no = CASE WHEN ?5 != '' THEN ?5 ELSE case_no END, \
           attributes = ?6, \
           _rev = _rev + 1, \
           _dirty = 1, \
           _updated_at = ?7 \
         WHERE id = ?1 AND _deleted = 0",
        rusqlite::params![
            input.id,
            title_val,
            stage_val,
            status_val,
            case_no_val,
            attrs.to_string(),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "title": input.title,
        "stage": input.stage,
        "status": input.status,
        "attributes": attrs,
    });
    outbox_enqueue(conn, "update", "case", &input.id, &payload, &now)?;

    case_get(conn, &input.id)?.ok_or_else(|| "case_update: row not found".to_string())
}

pub fn evidence_create(
    conn: &Connection,
    input: crate::dto::EvidenceCreateInput,
) -> Result<EvidenceDto, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let id: String = conn
        .query_row("SELECT 'ev-' || lower(hex(randomblob(8)))", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;

    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM evidence WHERE case_id = ?1",
            [&input.case_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let three_natures =
        serde_json::json!({"authenticity": false, "legality": false, "relevance": false});
    let linked_json =
        serde_json::to_string(&input.linked_element_ids).unwrap_or_else(|_| "[]".to_string());

    conn.execute(
        "INSERT INTO evidence(id, case_id, seq, name, form, source, proof_target, three_natures, linked_element_ids, _rev, _updated_at, _deleted, _dirty) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10,0,1)",
        rusqlite::params![
            id,
            input.case_id,
            seq,
            input.name,
            input.form,
            input.source,
            input.proof_target,
            three_natures.to_string(),
            linked_json,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    // custody_event: hash of cataloged fields
    let custody_content = format!("{}{}", input.name, input.linked_element_ids.join(","));
    let payload_hash = vault::sha256_hex(custody_content.as_bytes());
    conn.execute(
        "INSERT INTO custody_event(entity_type, entity_id, event_type, actor, ts, payload_hash) \
         VALUES('evidence',?1,'cataloged','user',?2,?3)",
        rusqlite::params![id, now, payload_hash],
    )
    .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "name": input.name,
        "form": input.form,
        "source": input.source,
        "proofTarget": input.proof_target,
        "linkedElementIds": input.linked_element_ids,
    });
    outbox_enqueue(conn, "create", "evidence", &id, &payload, &now)?;

    evidence_get(conn, &id)
}

pub fn evidence_update(
    conn: &Connection,
    input: crate::dto::EvidenceUpdateInput,
) -> Result<EvidenceDto, String> {
    let now = chrono::Utc::now().to_rfc3339();

    // Read current three_natures and linked_element_ids
    let (current_three_str, current_linked_str): (String, String) = conn
        .query_row(
            "SELECT three_natures, linked_element_ids FROM evidence WHERE id = ?1 AND _deleted = 0",
            [&input.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    let three_natures = if let Some(ref tn) = input.three_natures {
        serde_json::to_string(tn).unwrap_or(current_three_str)
    } else {
        current_three_str
    };

    let linked_json = if let Some(ref li) = input.linked_element_ids {
        serde_json::to_string(li).unwrap_or(current_linked_str)
    } else {
        current_linked_str
    };

    let name_val = input.name.as_deref().unwrap_or("");
    let form_val = input.form.as_deref().unwrap_or("");
    let source_val = input.source.as_deref().unwrap_or("");
    let proof_target_val = input.proof_target.as_deref().unwrap_or("");

    conn.execute(
        "UPDATE evidence SET \
           name = CASE WHEN ?2 != '' THEN ?2 ELSE name END, \
           form = CASE WHEN ?3 != '' THEN ?3 ELSE form END, \
           source = CASE WHEN ?4 != '' THEN ?4 ELSE source END, \
           proof_target = CASE WHEN ?5 != '' THEN ?5 ELSE proof_target END, \
           three_natures = ?6, \
           linked_element_ids = ?7, \
           _rev = _rev + 1, \
           _dirty = 1, \
           _updated_at = ?8 \
         WHERE id = ?1 AND _deleted = 0",
        rusqlite::params![
            input.id,
            name_val,
            form_val,
            source_val,
            proof_target_val,
            three_natures,
            linked_json,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "name": input.name,
        "form": input.form,
        "source": input.source,
        "proofTarget": input.proof_target,
        "linkedElementIds": input.linked_element_ids,
    });
    outbox_enqueue(conn, "update", "evidence", &input.id, &payload, &now)?;

    evidence_get(conn, &input.id)
}

pub fn fact_upsert(
    conn: &Connection,
    input: crate::dto::FactUpsertInput,
) -> Result<FactDto, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let evidence_ids_json =
        serde_json::to_string(&input.evidence_ids).unwrap_or_else(|_| "[]".to_string());

    let id = if let Some(ref existing_id) = input.id {
        // Check if it exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM fact WHERE id = ?1 AND _deleted = 0",
                [existing_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if exists {
            conn.execute(
                "UPDATE fact SET \
                   description = ?2, \
                   fact_date = ?3, \
                   source_evidence_ids = ?4, \
                   _rev = _rev + 1, \
                   _dirty = 1, \
                   _updated_at = ?5 \
                 WHERE id = ?1 AND _deleted = 0",
                rusqlite::params![
                    existing_id,
                    input.summary,
                    input.fact_date,
                    evidence_ids_json,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            let payload = serde_json::json!({
                "summary": input.summary,
                "factDate": input.fact_date,
                "evidenceIds": input.evidence_ids,
            });
            outbox_enqueue(conn, "update", "fact", existing_id, &payload, &now)?;

            existing_id.clone()
        } else {
            // insert with provided id
            conn.execute(
                "INSERT INTO fact(id, case_id, description, fact_date, source_evidence_ids, _rev, _updated_at, _deleted, _dirty) \
                 VALUES(?1,?2,?3,?4,?5,1,?6,0,1)",
                rusqlite::params![
                    existing_id,
                    input.case_id,
                    input.summary,
                    input.fact_date,
                    evidence_ids_json,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            let payload = serde_json::json!({
                "summary": input.summary,
                "factDate": input.fact_date,
                "evidenceIds": input.evidence_ids,
            });
            outbox_enqueue(conn, "create", "fact", existing_id, &payload, &now)?;

            existing_id.clone()
        }
    } else {
        // Generate new id
        let new_id: String = conn
            .query_row("SELECT 'ft-' || lower(hex(randomblob(8)))", [], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO fact(id, case_id, description, fact_date, source_evidence_ids, _rev, _updated_at, _deleted, _dirty) \
             VALUES(?1,?2,?3,?4,?5,1,?6,0,1)",
            rusqlite::params![
                new_id,
                input.case_id,
                input.summary,
                input.fact_date,
                evidence_ids_json,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;

        let payload = serde_json::json!({
            "summary": input.summary,
            "factDate": input.fact_date,
            "evidenceIds": input.evidence_ids,
        });
        outbox_enqueue(conn, "create", "fact", &new_id, &payload, &now)?;

        new_id
    };

    fact_get(conn, &id)
}

// ── deadline 持久化 + 今日分诊（期限管家本地后端）─────────────────────────────
//
// 不变量：
//   · 到期日永远由 deadline-engine 本地求值（deadline_compute），绝不交 LLM，
//     绝不在此硬编码天数——天数读自 server-config 的 rule.amount。
//   · R6 护栏：锚点未经律师确认（anchor_confirmed=false）即不出到期日，computed_due_date 留空。
//   · 期限计算/确认是监管事件 → custody_event（INSERT-only，payload_hash 不落原文）。
//   · 同步只出脱敏 metadata（rule/label/anchor/due/kind）→ outbox，绝无原件指针与原文。

/// 缺省重要性（0–100，仅分诊排序用；非天数，与红线 R4 无关）。
const DEFAULT_DEADLINE_IMPORTANCE: i64 = 70;

/// deadline 行 → DTO。attributes 取 kind/importance；NULL 的到期日/锚点映射为空串。
fn row_to_deadline(r: &rusqlite::Row) -> rusqlite::Result<DeadlineDto> {
    let attrs_raw: String = r.get(8)?;
    let attrs: serde_json::Value =
        serde_json::from_str(&attrs_raw).unwrap_or(serde_json::json!({}));
    let kind = attrs
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("statutory")
        .to_string();
    let importance = attrs
        .get("importance")
        .and_then(|v| v.as_i64())
        .unwrap_or(DEFAULT_DEADLINE_IMPORTANCE);
    Ok(DeadlineDto {
        id: r.get(0)?,
        case_id: r.get(1)?,
        case_title: r.get(2)?,
        rule_id: r.get(3)?,
        name: r.get(4)?,
        kind,
        anchor_date: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        anchor_confirmed: r.get::<_, i64>(6)? != 0,
        due_date: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
        importance,
    })
}

const DEADLINE_SELECT: &str = "SELECT d.id, d.case_id, c.title, d.rule_id, d.label, d.anchor_date, \
     d.anchor_confirmed, d.computed_due_date, d.attributes \
     FROM deadline d JOIN \"case\" c ON c.id = d.case_id";

fn deadline_get(conn: &Connection, id: &str) -> Result<DeadlineDto, String> {
    conn.query_row(
        &format!("{DEADLINE_SELECT} WHERE d.id = ?1 AND d._deleted = 0"),
        [id],
        row_to_deadline,
    )
    .map_err(|e| e.to_string())
}

/// 期限相关监管事件（append-only）。payload_hash 仅哈希规则/到期/时刻，不含原文/PII。
fn write_deadline_custody(
    conn: &Connection,
    deadline_id: &str,
    event_type: &str,
    rule_id: &str,
    due_date: &str,
    now: &str,
) -> Result<(), String> {
    let payload_hash = vault::sha256_hex(format!("{rule_id}|{due_date}|{now}").as_bytes());
    conn.execute(
        "INSERT INTO custody_event(entity_type, entity_id, event_type, actor, ts, payload_hash) \
         VALUES('deadline',?1,?2,'user',?3,?4)",
        rusqlite::params![deadline_id, event_type, now, payload_hash],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn deadline_create(
    conn: &Connection,
    input: crate::dto::DeadlineCreateInput,
) -> Result<DeadlineDto, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let id: String = conn
        .query_row("SELECT 'dl-' || lower(hex(randomblob(8)))", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;

    // label / 触发事件 取自生效配置（绝不内嵌天数）。
    let rules_json = active_payload(conn, "deadline-rules", "cn-civil-first-instance")?;
    let ruleset: DeadlineRuleSet =
        serde_json::from_str(&rules_json).map_err(|e| format!("parse rules: {e}"))?;
    let (label, anchor_event, _require_confirm) = ruleset.rule_meta(&input.rule_id)?;

    let anchor_confirmed = input.anchor_confirmed.unwrap_or(false);
    let importance = input.importance.unwrap_or(DEFAULT_DEADLINE_IMPORTANCE);
    let kind = input
        .kind
        .clone()
        .unwrap_or_else(|| "statutory".to_string());
    let attrs = serde_json::json!({ "kind": kind, "importance": importance });

    // 仅当锚点已确认且给定触发日，才本地求值到期日（否则留空，R6/G6 护栏）。
    let (due_date, used_ruleset, used_holidays): (Option<String>, Option<String>, Option<String>) =
        if anchor_confirmed && input.anchor_date.is_some() {
            let due = deadline_compute(
                conn,
                &input.rule_id,
                input.anchor_date.as_deref().unwrap(),
                true,
                input.doc_type.as_deref(),
            )?;
            (
                Some(due.due_date),
                Some(due.used_ruleset_version),
                Some(due.used_holiday_table.join(",")),
            )
        } else {
            (None, None, None)
        };

    conn.execute(
        "INSERT INTO deadline(id, case_id, rule_id, label, anchor_event, anchor_date, anchor_confirmed, \
              computed_due_date, computed_with_ruleset, computed_with_holidays, status, attributes, \
              _rev, _updated_at, _deleted, _dirty) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',?11,1,?12,0,1)",
        rusqlite::params![
            id,
            input.case_id,
            input.rule_id,
            label,
            anchor_event,
            input.anchor_date,
            anchor_confirmed as i64,
            due_date,
            used_ruleset,
            used_holidays,
            attrs.to_string(),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Some(ref dd) = due_date {
        write_deadline_custody(conn, &id, "deadline_computed", &input.rule_id, dd, &now)?;
    }

    let payload = serde_json::json!({
        "ruleId": input.rule_id,
        "label": label,
        "anchorEvent": anchor_event,
        "anchorDate": input.anchor_date,
        "anchorConfirmed": anchor_confirmed,
        "dueDate": due_date,
        "kind": kind,
        "importance": importance,
    });
    outbox_enqueue(conn, "create", "deadline", &id, &payload, &now)?;

    deadline_get(conn, &id)
}

pub fn deadline_confirm_anchor(
    conn: &Connection,
    input: crate::dto::DeadlineConfirmAnchorInput,
) -> Result<DeadlineDto, String> {
    let now = chrono::Utc::now().to_rfc3339();

    let rule_id: String = conn
        .query_row(
            "SELECT rule_id FROM deadline WHERE id = ?1 AND _deleted = 0",
            [&input.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    // 律师确认触发日 → 本地引擎确定性求值（绝不交 LLM）。
    let due = deadline_compute(
        conn,
        &rule_id,
        &input.anchor_date,
        true,
        input.doc_type.as_deref(),
    )?;

    conn.execute(
        "UPDATE deadline SET \
            anchor_date = ?2, \
            anchor_confirmed = 1, \
            computed_due_date = ?3, \
            computed_with_ruleset = ?4, \
            computed_with_holidays = ?5, \
            _rev = _rev + 1, \
            _dirty = 1, \
            _updated_at = ?6 \
         WHERE id = ?1 AND _deleted = 0",
        rusqlite::params![
            input.id,
            input.anchor_date,
            due.due_date,
            due.used_ruleset_version,
            due.used_holiday_table.join(","),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    write_deadline_custody(
        conn,
        &input.id,
        "anchor_confirmed",
        &rule_id,
        &due.due_date,
        &now,
    )?;

    let payload = serde_json::json!({
        "ruleId": rule_id,
        "anchorDate": input.anchor_date,
        "anchorConfirmed": true,
        "dueDate": due.due_date,
    });
    outbox_enqueue(conn, "update", "deadline", &input.id, &payload, &now)?;

    deadline_get(conn, &input.id)
}

pub fn deadline_list_by_case(conn: &Connection, case_id: &str) -> Result<Vec<DeadlineDto>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{DEADLINE_SELECT} WHERE d.case_id = ?1 AND d._deleted = 0 \
             ORDER BY (d.computed_due_date IS NULL), d.computed_due_date"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([case_id], row_to_deadline)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn triage_today(conn: &Connection) -> Result<Vec<TriageItemDto>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{DEADLINE_SELECT} WHERE d._deleted = 0 AND d.status = 'pending' \
             ORDER BY (d.computed_due_date IS NULL), d.computed_due_date"
        ))
        .map_err(|e| e.to_string())?;
    let deadlines = stmt
        .query_map([], row_to_deadline)
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;

    let mut items: Vec<TriageItemDto> = deadlines
        .into_iter()
        .map(|d| {
            // 锚点未确认 → 提示先核对；已确认 → 到期日 + 重要性。
            let reason = if !d.anchor_confirmed {
                "起算锚点未确认，需先核对".to_string()
            } else {
                format!("{} 到期 · 重要性 {}", d.due_date, d.importance)
            };
            TriageItemDto {
                deadline: d,
                reason,
            }
        })
        .collect();

    // 已确认在前；同组按到期日升序、再按重要性降序。
    items.sort_by(|a, b| {
        b.deadline
            .anchor_confirmed
            .cmp(&a.deadline.anchor_confirmed)
            .then(a.deadline.due_date.cmp(&b.deadline.due_date))
            .then(b.deadline.importance.cmp(&a.deadline.importance))
    });
    Ok(items)
}

// ── config.status（治理透明化：当前生效配置版本 + 复核状态）────────────────────
//
// 诚实优先：期限/节假日/要件配置均为「新风险源」，未经法务/资深律师终核前，
// 这里把它们标为 pending_review，UI 据此提示「仅供参考/待核」，绝不冒充权威。

/// 从配置 payload 推断复核状态：含 _compliance_gate / 待核对 / needsVerification → 待复核。
fn derive_review(payload: &str) -> (String, Option<String>) {
    let v: serde_json::Value = serde_json::from_str(payload).unwrap_or(serde_json::json!({}));
    if let Some(gate) = v.get("_compliance_gate").and_then(|x| x.as_str()) {
        return ("pending_review".to_string(), Some(gate.to_string()));
    }
    if payload.contains("待核对")
        || payload.contains("needsVerification\":true")
        || payload.contains("needsVerification\": true")
    {
        return (
            "pending_review".to_string(),
            Some("含待人工核验项（如节假日调休 / 现行法条）".to_string()),
        );
    }
    ("verified".to_string(), None)
}

pub fn config_status(conn: &Connection) -> Result<Vec<ConfigStatusDto>, String> {
    // 每个 (kind,config_id) 取当前生效行（valid=1 且 fetched_at 最新），与 active_config 同义。
    let mut stmt = conn
        .prepare(
            "SELECT kind, config_id, version, schema_version, effective_from, fetched_at, payload \
             FROM config_cache c WHERE valid = 1 \
               AND fetched_at = (SELECT MAX(fetched_at) FROM config_cache c2 \
                                 WHERE c2.kind = c.kind AND c2.config_id = c.config_id AND c2.valid = 1) \
             ORDER BY kind, config_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let payload: String = r.get(6)?;
            let (review_status, note) = derive_review(&payload);
            Ok(ConfigStatusDto {
                kind: r.get(0)?,
                config_id: r.get(1)?,
                version: r.get(2)?,
                schema_version: r.get::<_, i64>(3)?,
                effective_from: r.get::<_, Option<String>>(4)?,
                fetched_at: r.get(5)?,
                review_status,
                note,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

// ── agent_context（出网脱敏上下文的可信产出者；rust-core 决定"什么能出网"）──────
//
// 红线①③：本函数只从加密库读取并产出脱敏字段（caseType / claims 档位 / evidenceMeta / 缺口），
// 绝不放入 file_ref / 原文 / ocr / 精确标的额。前端拿到本结构后才出网，云端 assertDesensitized 兜底。

/// 标的额 → 档位（不出精确额）。
fn amount_band(a: f64) -> &'static str {
    if a < 50_000.0 {
        "5万以下"
    } else if a < 100_000.0 {
        "5万-10万"
    } else if a < 500_000.0 {
        "10万-50万"
    } else if a < 1_000_000.0 {
        "50万-100万"
    } else if a < 5_000_000.0 {
        "100万-500万"
    } else {
        "500万以上"
    }
}

/// 证据脱敏 meta：仅 form / proofTarget / 关联要件（无 name / source / file_ref / ocr）。
fn load_evidence_meta(conn: &Connection, case_id: &str) -> Result<Vec<EvidenceMetaDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT form, proof_target, linked_element_ids \
             FROM evidence WHERE case_id = ?1 AND _deleted = 0 ORDER BY seq",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([case_id], |r| {
            let form: Option<String> = r.get(0)?;
            let pt: Option<String> = r.get(1)?;
            let linked_raw: String = r.get(2)?;
            Ok((form.unwrap_or_default(), pt.unwrap_or_default(), linked_raw))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (form, proof_target, linked_raw) = row.map_err(|e| e.to_string())?;
        let satisfies_elements: Vec<String> = serde_json::from_str(&linked_raw).unwrap_or_default();
        out.push(EvidenceMetaDto {
            form,
            proof_target,
            satisfies_elements,
        });
    }
    Ok(out)
}

/// 构建该案出网脱敏上下文（对齐云端 DesensitizedContext schema，逐字段脱敏）。
pub fn agent_context(conn: &Connection, case_id: &str) -> Result<DesensitizedContextDto, String> {
    let (case_type, stage, jurisdiction, attributes): (String, String, Option<String>, String) =
        conn.query_row(
            "SELECT case_type, stage, jurisdiction, attributes FROM \"case\" \
             WHERE id = ?1 AND _deleted = 0",
            [case_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| e.to_string())?;
    let attrs: serde_json::Value =
        serde_json::from_str(&attributes).unwrap_or(serde_json::json!({}));

    let case_label = attr_str(&attrs, "caseTypeLabel").unwrap_or_else(|| case_type.clone());
    let claim = ClaimDto {
        id: "c1".to_string(),
        claim_type: case_label.clone(),
        includes_interest: attr_bool(&attrs, "includesInterest"),
        amount_band: attr_f64(&attrs, "amount").map(|a| amount_band(a).to_string()),
    };

    let evidence_meta = load_evidence_meta(conn, case_id)?;

    // 非 PII 聚合摘要：只用计数 / 阶段，绝不取 fact 原文（防姓名等 PII 出网）。
    let fact_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM fact WHERE case_id = ?1 AND _deleted = 0",
            [case_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let facts_summary = format!(
        "案由：{}；事实 {} 条；证据 {} 件；阶段：{}。（脱敏聚合，不含原文）",
        case_label,
        fact_count,
        evidence_meta.len(),
        stage
    );

    // 缺口复用本地 gap_diagnose（本地 CEL，绝不出网）。
    let element_gaps = gap_diagnose(conn, case_id)?
        .gaps
        .into_iter()
        .map(|g| ElementGapDto {
            element_id: g.element_ref,
            severity: g.severity,
        })
        .collect();

    Ok(DesensitizedContextDto {
        case_type,
        jurisdiction,
        claims: vec![claim],
        facts_summary,
        evidence_meta,
        element_gaps,
    })
}

// ── triage_context（今日全局精力简报：跨案脱敏摘要的可信产出者）────────────────
//
// 红线：与 agent_context 同源——cases 只含脱敏字段（案由中文 / 阶段 / 期限事实 / 缺口计数），
// 当事人 / 案号 / 标的 / 原文一概不入。case_title、case_id 只进 legend（本地映射），
// 绝不进 cases（出网体），前端只把 cases 传云端。到期日复用本地引擎已算结果（R4 不在云端算），
// 锚点未确认即不给到期日（R6）。
//
/// 构建跨案"今日精力"脱敏摘要（cases 可出网，legend 仅本地映射）。
pub fn triage_context(conn: &Connection) -> Result<TodayDigestDto, String> {
    // 跨案 pending 期限（triage_today 已按紧迫度排序），按 case_id 归并为脱敏期限。
    let mut by_case: std::collections::HashMap<String, Vec<TodayDeadlineDto>> =
        std::collections::HashMap::new();
    for item in triage_today(conn)? {
        let d = item.deadline;
        let due_date = if d.anchor_confirmed && !d.due_date.is_empty() {
            Some(d.due_date)
        } else {
            None
        };
        by_case
            .entry(d.case_id)
            .or_default()
            .push(TodayDeadlineDto {
                name: d.name,
                kind: d.kind,
                due_date,
                anchor_confirmed: d.anchor_confirmed,
                importance: d.importance,
            });
    }

    // 非归档案件（active + paused），按最近更新排序。
    let mut stmt = conn
        .prepare(
            "SELECT id, case_type, stage, attributes FROM \"case\" \
             WHERE _deleted = 0 AND status != 'archived' ORDER BY _updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut cases = Vec::new();
    let mut legend = Vec::new();
    let mut idx = 0i64;
    for row in rows {
        let (case_id, case_type, stage, attributes) = row.map_err(|e| e.to_string())?;
        let attrs: serde_json::Value =
            serde_json::from_str(&attributes).unwrap_or(serde_json::json!({}));
        let case_label = attr_str(&attrs, "caseTypeLabel").unwrap_or_else(|| case_type.clone());

        // 缺口计数（本地 CEL；未知案由返回空，不致 panic）。
        let gap = gap_diagnose(conn, &case_id)?;
        let gap_critical = gap.gaps.iter().filter(|g| g.severity == "critical").count() as i64;
        let gap_warning = gap.gaps.iter().filter(|g| g.severity == "warning").count() as i64;
        let needs_judgment = gap.needs_cloud_judgment.len() as i64;

        let deadlines = by_case.remove(&case_id).unwrap_or_default();

        // 只纳入有待办或有缺口的案件，避免简报被无事项案件稀释。
        if deadlines.is_empty() && gap_critical == 0 && gap_warning == 0 && needs_judgment == 0 {
            continue;
        }

        idx += 1;
        let case_ref = idx.to_string();
        legend.push(TodayLegendDto {
            case_ref: case_ref.clone(),
            case_id,
        });
        cases.push(TodayCaseDto {
            case_ref,
            case_label,
            stage,
            deadlines,
            gap_critical,
            gap_warning,
            needs_judgment,
        });
    }

    Ok(TodayDigestDto { cases, legend })
}

// ── template_list（文书模板：config-kind=template 的本地读取）──────────────────
//
// 文书为本地确定性渲染：模板（骨架）走 server-config 热更/治理（含 _compliance_gate →
// 待核对），填充与导出在 webview 本地完成，PII（当事人/标的）留本机、不出网。

/// 模板 payload 的可解析部分（忽略 meta/_compliance_gate 等其余字段）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplatePayload {
    doc_type: String,
    title: String,
    #[serde(default)]
    sections: Vec<TemplateSectionDto>,
    #[serde(default)]
    placeholders: Vec<TemplatePlaceholderDto>,
}

/// 列出当前生效的文书模板（kind=template 的最新有效版本）。
pub fn template_list(conn: &Connection) -> Result<Vec<TemplateDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT config_id, version, payload FROM config_cache c WHERE valid = 1 AND kind = 'template' \
               AND fetched_at = (SELECT MAX(fetched_at) FROM config_cache c2 \
                                 WHERE c2.kind = c.kind AND c2.config_id = c.config_id AND c2.valid = 1) \
             ORDER BY config_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let (config_id, version, payload) = row.map_err(|e| e.to_string())?;
        let (review_status, note) = derive_review(&payload);
        let parsed: TemplatePayload = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        out.push(TemplateDto {
            config_id,
            doc_type: parsed.doc_type,
            title: parsed.title,
            version,
            sections: parsed.sections,
            placeholders: parsed.placeholders,
            review_status,
            note,
        });
    }
    Ok(out)
}

// ── conflict_scan（利益冲突初筛：本地跨案当事人比对）──────────────────────────────
//
// ★本地命令：读各案 attributes.parties，按归一化姓名跨案归并；同名出现在 ≥2 在办案件即为线索。
// severity：我方↔对方交叉 = critical（潜在直接利冲）；同一对方多案 = warning；同一委托人多案 = info。
// 这是「机械初筛」而非利冲法律结论——最终判断以律师审查为准（UI 标注）。
// 结果含当事人姓名/案件标题（PII），仅供本地 UI，绝不进出网/同步通道（与 case_list/triage_today 同源）。

/// attributes.parties 的解析视图（容错：缺字段给默认）。
#[derive(serde::Deserialize)]
struct PartyRecord {
    #[serde(default)]
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    represented: bool,
}

/// 归一化姓名作比对键：去全部空白；英文转小写（中文按去空白后精确匹配）。
fn normalize_party_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

pub fn conflict_scan(conn: &Connection) -> Result<Vec<ConflictFindingDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, case_type, attributes FROM \"case\" \
             WHERE _deleted = 0 AND status != 'archived'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    // 归一化姓名 → (展示名, 命中列表)。每案每当事人一条命中。
    let mut by_name: std::collections::HashMap<String, (String, Vec<ConflictPartyRefDto>)> =
        std::collections::HashMap::new();

    for row in rows {
        let (case_id, title, case_type, attributes) = row.map_err(|e| e.to_string())?;
        let attrs: serde_json::Value =
            serde_json::from_str(&attributes).unwrap_or(serde_json::json!({}));
        let label = attr_str(&attrs, "caseTypeLabel").unwrap_or_else(|| case_type.clone());
        let parties: Vec<PartyRecord> = attrs
            .get("parties")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        for p in parties {
            let key = normalize_party_name(&p.name);
            if key.is_empty() {
                continue;
            }
            let entry = by_name
                .entry(key)
                .or_insert_with(|| (p.name.trim().to_string(), Vec::new()));
            entry.1.push(ConflictPartyRefDto {
                case_id: case_id.clone(),
                case_title: title.clone(),
                case_type_label: label.clone(),
                role: p.role.clone(),
                represented: p.represented,
            });
        }
    }

    let mut findings: Vec<ConflictFindingDto> = Vec::new();
    for (_key, (display_name, hits)) in by_name {
        // 须跨 ≥2 个不同案件方为线索（同案内重复不算）。
        let distinct: std::collections::HashSet<&str> =
            hits.iter().map(|h| h.case_id.as_str()).collect();
        let n = distinct.len();
        if n < 2 {
            continue;
        }
        let any_represented = hits.iter().any(|h| h.represented);
        let any_opposing = hits.iter().any(|h| !h.represented);
        drop(distinct); // 释放对 hits 的借用，便于下方 move

        let (severity, explain) = if any_represented && any_opposing {
            (
                "critical",
                format!(
                    "同一当事人「{display_name}」在不同案件中分别为我方委托人与对方当事人，构成潜在直接利益冲突，须立即人工核实是否禁止代理。"
                ),
            )
        } else if any_opposing {
            (
                "warning",
                format!(
                    "同一对方当事人「{display_name}」出现在 {n} 个在办案件中，请核实各委托人之间不存在利益冲突。"
                ),
            )
        } else {
            (
                "info",
                format!(
                    "同一委托人「{display_name}」关联 {n} 个在办案件，提示关联办理（通常无碍）。"
                ),
            )
        };
        findings.push(ConflictFindingDto {
            name: display_name,
            severity: severity.to_string(),
            explain,
            hits,
        });
    }

    // critical → warning → info；同档按命中数降序、姓名升序，结果稳定可测。
    fn rank(s: &str) -> u8 {
        match s {
            "critical" => 0,
            "warning" => 1,
            _ => 2,
        }
    }
    findings.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then(b.hits.len().cmp(&a.hits.len()))
            .then(a.name.cmp(&b.name))
    });
    Ok(findings)
}

// ── 法律工具桥：授权 scope 内只读检索（宿主校验 matter_id）────────────────────────

pub fn search_private_documents(
    conn: &Connection,
    scope: &crate::scope::ScopeRegistry,
    capability_token: &str,
    session_id: &str,
    matter_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<crate::dto::PrivatePassageHitDto>, String> {
    scope
        .validate(capability_token, matter_id, session_id)
        .map_err(|e| e.to_string())?;
    if case_get(conn, matter_id)?.is_none() {
        return Err("scope_denied: unknown matter".into());
    }
    let q = crate::query_sanitize::normalize_private_query(query).map_err(|e| e.to_string())?;
    let limit = crate::query_sanitize::clamp_limit(limit, 20);
    let pattern = format!(
        "%{}%",
        crate::query_sanitize::escape_like_pattern(&q)
    );
    let mut stmt = conn
        .prepare(
            "SELECT id, case_id, name, COALESCE(seq, 0), COALESCE(original_hash, ''), \
             COALESCE(proof_target, '') FROM evidence \
             WHERE case_id = ?1 AND _deleted = 0 \
             AND (name LIKE ?2 ESCAPE '\\' OR proof_target LIKE ?2 ESCAPE '\\') \
             ORDER BY seq LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((matter_id, pattern.as_str(), limit), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (id, case_id, name, seq, hash, proof) = row.map_err(|e| e.to_string())?;
        let content_hash = if hash.is_empty() {
            vault::sha256_hex(format!("{name}|{proof}").as_bytes())
        } else {
            hash
        };
        let document_version = format!("{case_id}@evidence@{content_hash}");
        let excerpt = if proof.is_empty() {
            name
        } else {
            format!("{name} · {proof}")
        };
        out.push(crate::dto::PrivatePassageHitDto {
            passage_id: id.clone(),
            document_version,
            excerpt: truncate_excerpt(&excerpt, 240),
            locator: Some(format!("evidence_seq:{seq}")),
            content_hash: Some(content_hash),
        });
    }
    Ok(out)
}

fn truncate_excerpt(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn map_public_hit_row(
    row: rusqlite::Result<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )>,
) -> Result<crate::dto::PublicCaseHitDto, String> {
    let (passage_id, case_version, excerpt, locator, title, court, decided_at, source_id, license_status, tags_json) =
        row.map_err(|e| e.to_string())?;
    let similarity_note = serde_json::from_str::<Vec<String>>(&tags_json)
        .ok()
        .map(|t| t.join(" · "));
    Ok(crate::dto::PublicCaseHitDto {
        case_version,
        title,
        court,
        decided_at,
        passage_id,
        excerpt: truncate_excerpt(&excerpt, 320),
        locator,
        source_id,
        license_status,
        similarity_note,
    })
}

fn public_search_like(
    conn: &Connection,
    pattern: &str,
    limit: i64,
) -> Result<Vec<crate::dto::PublicCaseHitDto>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.passage_id, p.case_version, p.text_excerpt, p.locator, \
             c.title, c.court, c.decided_at, c.source_id, s.license_status, c.similarity_tags \
             FROM public_passage p \
             JOIN public_case c ON c.case_version = p.case_version \
             JOIN source_registry s ON s.source_id = c.source_id \
             WHERE c.withdrawn = 0 AND s.withdrawn = 0 AND p.text_excerpt LIKE ?1 ESCAPE '\\' \
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map((pattern, limit), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(map_public_hit_row).collect()
}

pub fn search_public_cases(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> Result<Vec<crate::dto::PublicCaseHitDto>, String> {
    let limit = crate::query_sanitize::clamp_limit(limit, crate::query_sanitize::MAX_PUBLIC_LIMIT);
    let normalized = match crate::query_sanitize::normalize_private_query(query) {
        Ok(q) => q,
        Err(crate::query_sanitize::QueryReject::Empty) => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let like_pattern = format!(
        "%{}%",
        crate::query_sanitize::escape_like_pattern(&normalized)
    );

    if let Ok(fts_q) = crate::query_sanitize::fts_and_query(&normalized) {
        let sql = "SELECT p.passage_id, p.case_version, p.text_excerpt, p.locator, \
                   c.title, c.court, c.decided_at, c.source_id, s.license_status, c.similarity_tags \
                   FROM public_passage_fts \
                   JOIN public_passage p ON p.passage_id = public_passage_fts.passage_id \
                   JOIN public_case c ON c.case_version = p.case_version \
                   JOIN source_registry s ON s.source_id = c.source_id \
                   WHERE c.withdrawn = 0 AND s.withdrawn = 0 \
                     AND public_passage_fts MATCH ?1 \
                   LIMIT ?2";
        if let Ok(mut stmt) = conn.prepare(sql) {
            if let Ok(rows) = stmt.query_map((fts_q, limit), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            }) {
                let mut out = Vec::new();
                for row in rows {
                    if let Ok(hit) = map_public_hit_row(row) {
                        out.push(hit);
                    }
                }
                if !out.is_empty() {
                    return Ok(out);
                }
            }
        }
    }
    public_search_like(conn, &like_pattern, limit)
}

pub fn compare_public_cases(conn: &Connection, case_version_a: &str, case_version_b: &str) -> Result<String, String> {
    let load = |id: &str| -> Result<(String, String), String> {
        conn.query_row(
            "SELECT title, legal_topics FROM public_case WHERE case_version = ?1 AND withdrawn = 0",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| format!("case_version not found or withdrawn: {id}"))
    };
    let (title_a, topics_a) = load(case_version_a)?;
    let (title_b, _topics_b) = load(case_version_b)?;
    Ok(format!(
        "【合成类案对比·非胜诉率预测】\nA: {title_a} ({case_version_a})\nB: {title_b} ({case_version_b})\n\
         相似话题: {topics_a}\n关键差异: A 侧强调交付/合意；B 侧强调证据不足（反向）。\n\
         法适用时点与来源许可以 source_registry 登记为准；不输出 embedding 置信度。"
    ))
}

#[cfg(test)]
mod legal_scope_tests {
    use super::*;
    use crate::db;
    use crate::scope::ScopeRegistry;

    fn test_conn() -> (rusqlite::Connection, ScopeRegistry) {
        db::enable_dev_config_trust_for_tests();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = db::open_and_init(&path.to_string_lossy()).unwrap();
        (conn, ScopeRegistry::new())
    }

    fn confirmed(scope: &ScopeRegistry, matter: &str) -> (String, crate::scope::CapabilityGrant) {
        let sid = scope.host_session_id();
        scope.confirm_selection(&sid, matter).unwrap();
        let g = scope.issue_for_confirmed(&sid, matter, 600).unwrap();
        (sid, g)
    }

    #[test]
    fn cross_matter_private_search_denied_with_grant() {
        let (conn, scope) = test_conn();
        let (sid, g) = confirmed(&scope, "case-001");
        let err = search_private_documents(&conn, &scope, &g.token, &sid, "case-002", "合成", 3)
            .unwrap_err();
        assert!(err.contains("scope_denied"));
        assert!(err.contains("mismatch"));
        let self_grant = scope.issue_for_confirmed(&sid, "case-002", 600).unwrap_err();
        assert!(self_grant.contains("not the host-confirmed"));
    }

    #[test]
    fn wildcard_private_query_rejected() {
        let (conn, scope) = test_conn();
        let (sid, g) = confirmed(&scope, "case-001");
        let err = search_private_documents(&conn, &scope, &g.token, &sid, "case-001", "%", 3)
            .unwrap_err();
        assert!(err.contains("wildcard"));
    }

    #[test]
    fn quoted_public_chinese_matches_bare_term() {
        let (conn, _) = test_conn();
        let quoted = search_public_cases(&conn, "\"借贷\"", 5).unwrap();
        let bare = search_public_cases(&conn, "借贷", 5).unwrap();
        assert!(!quoted.is_empty());
        assert_eq!(quoted[0].passage_id, bare[0].passage_id);
        assert!(!quoted[0].locator.as_deref().unwrap_or("").contains('页'));
    }

    #[test]
    fn public_search_percent_rejected() {
        let (conn, _) = test_conn();
        let err = search_public_cases(&conn, "%", 5).unwrap_err();
        assert!(err.contains("wildcard"));
        let err = search_public_cases(&conn, &"%".repeat(200), 5).unwrap_err();
        assert!(err.contains("too_long"));
    }

    #[test]
    fn public_search_limit_is_clamped() {
        let (conn, _) = test_conn();
        let one = search_public_cases(&conn, "借贷", 0).unwrap();
        assert_eq!(one.len(), 1);
        let capped = search_public_cases(&conn, "借贷", 9999).unwrap();
        assert!(capped.len() <= 50);
        assert!(capped.len() >= 1);
        let long = "合".repeat(200);
        let err = search_public_cases(&conn, &long, 5).unwrap_err();
        assert!(err.contains("too_long"));
    }

    #[test]
    fn public_search_chinese_fixture() {
        let (conn, _) = test_conn();
        let hits = search_public_cases(&conn, "借贷", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].excerpt.contains("合成"));
    }
}
