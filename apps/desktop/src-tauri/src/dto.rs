//! 前端可序列化 DTO（对齐 apps/desktop/src/types.ts）。
//! 命令返回这些类型，serde 字段名用 camelCase 与 TS interface 完全一致。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDto {
    pub id: String,
    pub case_no: String,
    pub title: String,
    pub case_type: String,
    pub case_type_label: String,
    pub stage: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includes_interest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_deadline_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeNatures {
    pub authenticity: bool,
    pub legality: bool,
    pub relevance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDto {
    pub id: String,
    pub case_id: String,
    pub seq: i64,
    pub name: String,
    pub source: String,
    pub form: String,
    pub proof_target: String,
    pub three_natures: ThreeNatures,
    pub linked_element_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactDto {
    pub id: String,
    pub case_id: String,
    pub fact_date: String,
    pub summary: String,
    pub evidence_ids: Vec<String>,
}

/// deadline.compute 的返回（成功）。对齐 contract 的 DueDate 语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DueDateDto {
    pub due_date: String,
    pub used_ruleset_version: String,
    pub used_holiday_table: Vec<String>,
    pub trace: Vec<String>,
}

// ---- gap.diagnose 返回（对齐 types.ts 的 GapDiagnosis）----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SatisfiedElement {
    pub element_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GapDto {
    pub element_ref: String,
    pub severity: String,
    pub explain: String,
    pub suggest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsCloudJudgment {
    pub element_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GapDiagnosisDto {
    pub satisfied: Vec<SatisfiedElement>,
    pub gaps: Vec<GapDto>,
    pub needs_cloud_judgment: Vec<NeedsCloudJudgment>,
}

/// 当事人录入项（暂存 case.attributes.parties；represented=true 为我方委托人）。
/// 供利益冲突初筛（conflict_scan）跨案比对；姓名为 PII，仅本地，绝不进出网/同步通道。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyInput {
    pub name: String,
    #[serde(default = "default_party_role")]
    pub role: String,
    #[serde(default)]
    pub represented: bool,
}

fn default_party_role() -> String {
    "third_party".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseCreateInput {
    pub title: String,
    pub case_type: String,
    #[serde(default)]
    pub case_no: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub case_type_label: Option<String>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub includes_interest: Option<bool>,
    // 案由特定 claim 标记（供缺口诊断 CEL gapRules 读取）。
    #[serde(default)]
    pub claim_type: Option<String>,
    #[serde(default)]
    pub includes_disability: Option<bool>,
    #[serde(default)]
    pub includes_lost_income: Option<bool>,
    #[serde(default)]
    pub has_quality_issue: Option<bool>,
    // 当事人（暂存 attributes.parties，供利冲初筛比对）。
    #[serde(default)]
    pub parties: Vec<PartyInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseUpdateInput {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub case_no: Option<String>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub includes_interest: Option<bool>,
    #[serde(default)]
    pub case_type_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCreateInput {
    pub case_id: String,
    pub name: String,
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub proof_target: Option<String>,
    #[serde(default)]
    pub linked_element_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceUpdateInput {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub proof_target: Option<String>,
    #[serde(default)]
    pub three_natures: Option<ThreeNatures>,
    #[serde(default)]
    pub linked_element_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub case_id: String,
    pub summary: String,
    #[serde(default)]
    pub fact_date: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

// ---- 期限（deadline）DTO（对齐 types.ts Deadline / TriageItem）----

/// 期限行。dueDate/anchorDate 为空串表示尚未确定（锚点未确认即不算到期日，R6 护栏）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineDto {
    pub id: String,
    pub case_id: String,
    pub case_title: String,
    pub rule_id: String,
    pub name: String,
    pub kind: String,
    pub due_date: String,
    pub anchor_date: String,
    pub anchor_confirmed: bool,
    pub importance: i64,
}

/// triage.today 单条（跨案分诊）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageItemDto {
    pub deadline: DeadlineDto,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineCreateInput {
    pub case_id: String,
    pub rule_id: String,
    #[serde(default)]
    pub anchor_date: Option<String>,
    #[serde(default)]
    pub anchor_confirmed: Option<bool>,
    #[serde(default)]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub importance: Option<i64>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineConfirmAnchorInput {
    pub id: String,
    pub anchor_date: String,
    #[serde(default)]
    pub doc_type: Option<String>,
}

// ---- 配置治理状态（config.status：当前生效的 server-config 版本 + 复核状态）----

/// 单个生效配置的状态。reviewStatus: "verified" | "pending_review"。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigStatusDto {
    pub kind: String,
    pub config_id: String,
    pub version: String,
    pub schema_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_from: Option<String>,
    pub fetched_at: String,
    pub review_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---- 出网脱敏上下文（对齐云端 DesensitizedContext schema；rust-core 产出，原件/原文绝不入内）----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimDto {
    pub id: String,
    #[serde(rename = "type")]
    pub claim_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includes_interest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_band: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceMetaDto {
    pub form: String,
    pub proof_target: String,
    pub satisfies_elements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementGapDto {
    pub element_id: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesensitizedContextDto {
    pub case_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
    pub claims: Vec<ClaimDto>,
    pub facts_summary: String,
    pub evidence_meta: Vec<EvidenceMetaDto>,
    pub element_gaps: Vec<ElementGapDto>,
}

// ---- 今日全局精力简报（跨案脱敏摘要 → 云端 attention agent；cases 可出网，legend 仅本地）----

/// 单条期限（脱敏：仅名称/类型/到期日/锚点/重要性；无 case_title/case_id/原文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayDeadlineDto {
    pub name: String,
    pub kind: String,
    /// 仅 anchor_confirmed 时给定（R6：未确认锚点不呈现到期日）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub anchor_confirmed: bool,
    pub importance: i64,
}

/// 单案脱敏摘要（可出网）：案由中文 / 阶段 / 期限事实 / 缺口计数；无当事人/案号/标的。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayCaseDto {
    /// 序号占位（"1".."n"）；云端据此点名，不泄露身份。
    pub case_ref: String,
    pub case_label: String,
    pub stage: String,
    pub deadlines: Vec<TodayDeadlineDto>,
    pub gap_critical: i64,
    pub gap_warning: i64,
    pub needs_judgment: i64,
}

/// case_ref → case_id 的本地映射（仅供前端把"案件N"还原为真实案件；★不出网）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayLegendDto {
    pub case_ref: String,
    pub case_id: String,
}

/// 今日全局精力简报载体：cases 脱敏可出网；legend 仅本地（前端只把 cases 传云端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayDigestDto {
    pub cases: Vec<TodayCaseDto>,
    pub legend: Vec<TodayLegendDto>,
}

// ---- 文书模板（config-kind=template；本地确定性渲染，PII 留本机不出网）----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSectionDto {
    pub heading: String,
    /// 含 {{placeholder}} 标记的段落正文。
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePlaceholderDto {
    pub key: String,
    pub label: String,
    /// 取值来源：manual（律师手填）/ today（当日）/ case.* 等。
    pub source: String,
}

/// 一个文书模板（骨架）。reviewStatus 复用配置治理：含 _compliance_gate → pending_review。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDto {
    pub config_id: String,
    pub doc_type: String,
    pub title: String,
    pub version: String,
    pub sections: Vec<TemplateSectionDto>,
    pub placeholders: Vec<TemplatePlaceholderDto>,
    pub review_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---- 利益冲突初筛（conflict_scan：本地跨案当事人比对；★本地命令，含 PII，绝不出网/同步）----

/// 一条命中：某当事人在某案中的身份（含案件标题/姓名，仅本地 UI，绝不出网）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPartyRefDto {
    pub case_id: String,
    pub case_title: String,
    pub case_type_label: String,
    pub role: String,
    pub represented: bool,
}

/// 一个利冲线索：同一当事人跨 ≥2 在办案件出现。severity: critical|warning|info。
/// 机械初筛，非法律结论——最终利冲判断以律师审查为准（UI 标注）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFindingDto {
    pub name: String,
    pub severity: String,
    pub explain: String,
    pub hits: Vec<ConflictPartyRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicCaseHitDto {
    pub case_version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub court: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    pub passage_id: String,
    pub excerpt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    pub source_id: String,
    pub license_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivatePassageHitDto {
    pub passage_id: String,
    pub document_version: String,
    pub excerpt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRuntimeStatusDto {
    pub enabled: bool,
    pub configured: bool,
    pub runtime_kind: String,
    pub pinned_version: String,
    pub binary_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub handshake_ok: bool,
    pub model_configured: bool,
    pub disclosure: String,
    pub mcp_attached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGrantDto {
    pub token: String,
    pub matter_id: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantCitationDto {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passage_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantDraftResultDto {
    pub task_id: String,
    pub state: String,
    pub text: String,
    pub citations: Vec<AssistantCitationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub runtime_evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunDraftInput {
    pub case_id: String,
    pub intent: String,
    pub capability_token: String,
}
