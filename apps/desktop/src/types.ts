// 核心领域类型 —— 对照 skeleton/api/contract.md 的 5 张核心表
// case / evidence / fact / deadline / attestation
// 仅前端视图所需字段；与 rust-core 的 SQLCipher schema 对齐时以契约为准。

/** 案件阶段（诉讼流程） */
export type CaseStage =
  | "intake" // 接案
  | "evidence" // 举证
  | "filing" // 立案
  | "trial" // 庭审
  | "judgment" // 判决
  | "execution" // 执行
  | "closed"; // 结案

export type CaseStatus = "active" | "paused" | "archived";

/** 1) 案卷 case */
export interface Case {
  id: string;
  caseNo: string; // 案号
  title: string; // 当事人 / 简称
  caseType: string; // 案由 config_id，如 private_lending
  caseTypeLabel: string; // 案由中文，如 民间借贷纠纷
  stage: CaseStage;
  status: CaseStatus;
  amount?: number; // 标的额（元）
  includesInterest?: boolean;
  nextDeadlineAt?: string; // 最近到期（ISO 日期）
  updatedAt: string;
}

/** 证据三性 */
export interface ThreeNatures {
  authenticity: boolean; // 真实性
  legality: boolean; // 合法性
  relevance: boolean; // 关联性
}

/** 证据形式 */
export type EvidenceForm = "book" | "electronic" | "cash" | "physical" | "testimony";

/** 2) 证据 evidence */
export interface Evidence {
  id: string;
  caseId: string;
  seq: number; // 序号
  name: string; // 证据名称
  source: string; // 来源
  form: EvidenceForm;
  proofTarget: string; // 证明对象
  threeNatures: ThreeNatures;
  linkedElementIds: string[]; // 关联要件 id
}

/** 3) 事实 fact（时间轴） */
export interface Fact {
  id: string;
  caseId: string;
  factDate: string; // 事实发生日（ISO）
  summary: string; // 脱敏摘要
  evidenceIds: string[]; // 支撑证据
}

export type DeadlineKind = "statutory" | "court_set" | "internal";

/** 4) 期限 deadline */
export interface Deadline {
  id: string;
  caseId: string;
  caseTitle: string;
  ruleId: string; // 期限规则 id
  name: string; // 期限名称，如 举证期限
  kind: DeadlineKind;
  dueDate: string; // 到期日（ISO）
  anchorDate: string; // 起算锚点
  anchorConfirmed: boolean; // 锚点是否已确认（护栏）
  importance: number; // 重要性 0-100（分诊排序用）
}

export type AttestPlatform = "zhixin" | "antchain" | "yibaoquan";

/** 5) 存证 attestation（只上哈希，不上原件） */
export interface Attestation {
  id: string;
  caseId: string;
  evidenceId: string;
  contentHash: string; // 内容哈希
  platform: AttestPlatform;
  attestedAt: string;
}

// ---- gap.diagnose 返回结构（对照 private-lending.json 的 gapRules 形状）----

export type GapSeverity = "critical" | "warning" | "info";

export interface SatisfiedElement {
  elementId: string;
  name: string;
}

export interface Gap {
  elementRef: string; // 缺口指向的要件 id
  severity: GapSeverity;
  explain: string; // 解释
  suggest: string; // 补强建议
}

export interface NeedsCloudJudgment {
  elementId: string;
  reason: string; // 需云端裁量判断的理由
}

export interface GapDiagnosis {
  satisfied: SatisfiedElement[];
  gaps: Gap[];
  needsCloudJudgment: NeedsCloudJudgment[];
}

/** triage.today 单条 */
export interface TriageItem {
  deadline: Deadline;
  reason: string; // 入选今日分诊的原因
}

/** gapJudge 单要件裁量结论（对齐云端 services/cloud GapJudgeResult） */
export interface GapJudgeResult {
  elementId: string;
  verdict: "likely-satisfied" | "likely-gap" | "needs-human";
  rationale: string; // 裁量推理链（中文）
  suggestedEvidence: string[]; // 可补强证据类型
}

// ---- 写入命令入参（对齐 src-tauri/src/dto.rs 的 *Input；camelCase）----

/** 当事人（暂存 case.attributes.parties；represented=true 为我方委托人）。★含 PII，绝不出网。 */
export interface Party {
  name: string;
  role: "plaintiff" | "defendant" | "third_party";
  represented: boolean;
}

export type PartyInput = Party;

export interface CaseCreateInput {
  title: string;
  caseType: string;
  caseNo?: string;
  stage?: CaseStage;
  caseTypeLabel?: string;
  amount?: number;
  includesInterest?: boolean;
  // 案由特定 claim 标记（缺口诊断 CEL gapRules 据此判断）
  claimType?: string;
  includesDisability?: boolean;
  includesLostIncome?: boolean;
  hasQualityIssue?: boolean;
  // 当事人（暂存 attributes.parties，供利冲初筛比对）
  parties?: PartyInput[];
}

/** 利益冲突初筛（conflict.scan：本地跨案当事人比对；★含 PII，绝不出网/同步） */
export interface ConflictPartyRef {
  caseId: string;
  caseTitle: string;
  caseTypeLabel: string;
  role: string;
  represented: boolean;
}

export interface ConflictFinding {
  name: string;
  severity: GapSeverity; // critical | warning | info
  explain: string;
  hits: ConflictPartyRef[];
}

export interface CaseUpdateInput {
  id: string;
  title?: string;
  stage?: CaseStage;
  status?: CaseStatus;
  caseNo?: string;
  amount?: number;
  includesInterest?: boolean;
  caseTypeLabel?: string;
}

export interface EvidenceCreateInput {
  caseId: string;
  name: string;
  form?: EvidenceForm;
  source?: string;
  proofTarget?: string;
  linkedElementIds: string[];
}

export interface EvidenceUpdateInput {
  id: string;
  name?: string;
  form?: EvidenceForm;
  source?: string;
  proofTarget?: string;
  threeNatures?: ThreeNatures;
  linkedElementIds?: string[];
}

export interface FactUpsertInput {
  id?: string;
  caseId: string;
  summary: string;
  factDate?: string;
  evidenceIds: string[];
}

export interface DeadlineCreateInput {
  caseId: string;
  ruleId: string;
  anchorDate?: string;
  anchorConfirmed?: boolean;
  docType?: string;
  importance?: number;
  kind?: DeadlineKind;
}

export interface DeadlineConfirmAnchorInput {
  id: string;
  anchorDate: string;
  docType?: string;
}

/** config.status —— 当前生效 server-config 版本 + 复核状态 */
export interface ConfigStatus {
  kind: string;
  configId: string;
  version: string;
  schemaVersion: number;
  effectiveFrom?: string;
  fetchedAt: string;
  reviewStatus: "verified" | "pending_review";
  note?: string;
}

/** 今日全局精力简报：跨案脱敏摘要（对齐 rust-core TodayDigestDto / 云端 TodayDigestSchema） */
export interface TodayDeadline {
  name: string;
  kind: string;
  dueDate?: string; // 仅锚点已确认时给定（R6）
  anchorConfirmed: boolean;
  importance: number;
}

/** 单案脱敏摘要（可出网）：无当事人/案号/标的 */
export interface TodayCase {
  caseRef: string; // 序号占位（"1".."n"）
  caseLabel: string; // 案由中文
  stage: string;
  deadlines: TodayDeadline[];
  gapCritical: number;
  gapWarning: number;
  needsJudgment: number;
}

/** caseRef → caseId 本地映射（★不出网，仅用于把"案件N"还原为真实案件） */
export interface TodayLegend {
  caseRef: string;
  caseId: string;
}

export interface TodayDigest {
  cases: TodayCase[];
  legend: TodayLegend[];
}

// ---- 文书模板（config-kind=template；本地确定性渲染，PII 不出网）----

export interface TemplateSection {
  heading: string;
  body: string; // 含 {{placeholder}} 标记
}

export interface TemplatePlaceholder {
  key: string;
  label: string;
  source: string; // manual | today | case.*
}

/** 一个文书模板（骨架）。对齐 rust-core TemplateDto。 */
export interface DocTemplate {
  configId: string;
  docType: string;
  title: string;
  version: string;
  sections: TemplateSection[];
  placeholders: TemplatePlaceholder[];
  reviewStatus: "verified" | "pending_review";
  note?: string;
}

/** 云端 agent 类型（对齐 services/cloud prompts.ts AgentType） */
export type AgentType = "attention" | "evidence";

/** 公共案例检索命中（合成/已登记来源） */
export interface PublicCaseHit {
  caseVersion: string;
  title: string;
  court?: string;
  decidedAt?: string;
  passageId: string;
  excerpt: string;
  locator?: string;
  sourceId: string;
  licenseStatus: string;
  similarityNote?: string;
}

/** ZeroClaw sidecar 运行状态 */
export interface AssistantRuntimeStatus {
  enabled: boolean;
  configured: boolean;
  runtimeKind: "zeroclaw" | "disabled";
  pinnedVersion: string;
  binaryPresent: boolean;
  socketPath?: string;
  lastError?: string;
  handshakeOk: boolean;
  modelConfigured: boolean;
  mcpAttached: boolean;
  disclosure: string;
}

export interface CapabilityGrant {
  token: string;
  matterId: string;
  sessionId: string;
  expiresAt: string;
}

export interface AssistantCitation {
  kind: "private_excerpt" | "public_case";
  passageId?: string;
  caseVersion?: string;
  locator?: string;
  excerpt: string;
}

/** 本地助手分析草稿（非终局法律意见） */
export interface AssistantDraftResult {
  taskId: string;
  state: "succeeded" | "failed" | "cancelled" | "interrupted";
  text: string;
  citations: AssistantCitation[];
  coverageNote?: string;
  error?: string;
  runtimeEvidence:
    | "disabled"
    | "rules_summary_only"
    | "product_handshake_verified"
    | "product_mcp_enumerated"
    | "simulation_turn"
    | "product_tool_turn_verified";
}

/** 出网脱敏上下文（由 rust-core agent_context 产出；对齐云端 DesensitizedContext schema） */
export interface DesensitizedContext {
  caseType: string;
  jurisdiction?: string;
  claims: {
    id: string;
    type: string;
    includesInterest?: boolean;
    amountBand?: string;
  }[];
  factsSummary: string;
  evidenceMeta: { form: string; proofTarget: string; satisfiesElements: string[] }[];
  elementGaps: { elementId: string; severity: string }[];
}
