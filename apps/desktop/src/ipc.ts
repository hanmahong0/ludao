import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AssistantDraftResult,
  AssistantRuntimeStatus,
  CapabilityGrant,
  Case,
  CaseCreateInput,
  ConflictFinding,
  ConfigStatus,
  Deadline,
  DeadlineConfirmAnchorInput,
  DeadlineCreateInput,
  DesensitizedContext,
  DocTemplate,
  Evidence,
  EvidenceCreateInput,
  Fact,
  GapDiagnosis,
  PublicCaseHit,
  TodayDigest,
  TriageItem,
} from "./types";

export const BACKEND_IS_REAL = isTauri();

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!BACKEND_IS_REAL) {
    throw new Error(`浏览器预览未实现 ${cmd}，请使用 Tauri 桌面构建。`);
  }
  return invoke<T>(cmd, args ?? {});
}

const syntheticCases: Case[] = [
  {
    id: "case-001",
    caseNo: "",
    title: "合成当事人A 诉 合成当事人B",
    caseType: "private_lending",
    caseTypeLabel: "民间借贷纠纷",
    stage: "evidence",
    status: "active",
    amount: 120000,
    includesInterest: true,
    updatedAt: new Date().toISOString(),
  },
  {
    id: "case-002",
    caseNo: "",
    title: "合成机构 劳动争议",
    caseType: "labor_dispute",
    caseTypeLabel: "劳动争议",
    stage: "evidence",
    status: "active",
    updatedAt: new Date().toISOString(),
  },
];

export const ipc = {
  caseList: (): Promise<Case[]> =>
    BACKEND_IS_REAL ? call<Case[]>("case_list") : Promise.resolve(syntheticCases),

  caseGet: (id: string): Promise<Case | null> =>
    BACKEND_IS_REAL ? call<Case | null>("case_get", { id }) : Promise.resolve(syntheticCases.find((c) => c.id === id) ?? null),

  caseCreate: (input: CaseCreateInput): Promise<Case> => call<Case>("case_create", { input }),

  caseUpdate: (input: unknown): Promise<Case> => call<Case>("case_update", { input }),

  evidenceList: (caseId: string): Promise<Evidence[]> =>
    BACKEND_IS_REAL ? call<Evidence[]>("evidence_list", { caseId }) : Promise.resolve([]),

  evidenceCreate: (input: EvidenceCreateInput): Promise<Evidence> =>
    call<Evidence>("evidence_create", { input }),

  evidenceUpdate: (input: unknown): Promise<Evidence> => call<Evidence>("evidence_update", { input }),

  factList: (caseId: string): Promise<Fact[]> => call<Fact[]>("fact_list", { caseId }),

  factUpsert: (input: unknown): Promise<unknown> => call("fact_upsert", { input }),

  deadlineListByCase: (caseId: string): Promise<Deadline[]> =>
    BACKEND_IS_REAL ? call<Deadline[]>("deadline_list_by_case", { caseId }) : Promise.resolve([]),

  deadlineCreate: (input: DeadlineCreateInput): Promise<Deadline> =>
    call<Deadline>("deadline_create", { input }),

  deadlineConfirmAnchor: (input: DeadlineConfirmAnchorInput): Promise<Deadline> =>
    call<Deadline>("deadline_confirm_anchor", { input }),

  triageToday: (): Promise<TriageItem[]> =>
    BACKEND_IS_REAL ? call<TriageItem[]>("triage_today") : Promise.resolve([]),

  conflictScan: (): Promise<ConflictFinding[]> =>
    BACKEND_IS_REAL ? call<ConflictFinding[]>("conflict_scan") : Promise.resolve([]),

  gapDiagnose: (caseId: string): Promise<GapDiagnosis> =>
    call<GapDiagnosis>("gap_diagnose", { caseId }),

  configStatus: (): Promise<ConfigStatus[]> =>
    BACKEND_IS_REAL ? call<ConfigStatus[]>("config_status") : Promise.resolve([]),

  agentContext: (caseId: string): Promise<DesensitizedContext> =>
    call<DesensitizedContext>("agent_context", { caseId }),

  triageContext: (): Promise<TodayDigest> => call<TodayDigest>("triage_context"),

  templateList: (): Promise<DocTemplate[]> =>
    BACKEND_IS_REAL ? call<DocTemplate[]>("template_list") : Promise.resolve([]),

  searchPublicCases: (query: string, limit = 10): Promise<PublicCaseHit[]> =>
    call<PublicCaseHit[]>("search_public_cases", { query, limit }),

  comparePublicCases: (caseVersionA: string, caseVersionB: string): Promise<string> =>
    call<string>("compare_public_cases", { caseVersionA, caseVersionB }),

  assistantStatus: (): Promise<AssistantRuntimeStatus> =>
    BACKEND_IS_REAL
      ? call<AssistantRuntimeStatus>("assistant_status")
      : Promise.resolve({
          enabled: false,
          configured: false,
          runtimeKind: "disabled",
          pinnedVersion: "",
          binaryPresent: false,
          handshakeOk: false,
          modelConfigured: false,
          mcpAttached: false,
          lastError: "合成演示模式：助手不可用",
          disclosure: "浏览器预览没有 ZeroClaw，也没有案件授权。",
        }),

  assistantSetEnabled: (enabled: boolean): Promise<AssistantRuntimeStatus> =>
    BACKEND_IS_REAL
      ? call<AssistantRuntimeStatus>("assistant_set_enabled", { enabled })
      : Promise.reject(new Error("合成演示模式：助手不可用")),

  scopeConfirmSelection: (matterId: string): Promise<void> =>
    BACKEND_IS_REAL
      ? call<void>("scope_confirm_selection", { matterId })
      : Promise.reject(new Error("合成演示模式：scope 不可用")),

  scopeGrantForMatter: (matterId: string): Promise<CapabilityGrant> =>
    BACKEND_IS_REAL
      ? call<CapabilityGrant>("scope_grant_for_matter", { matterId })
      : Promise.reject(new Error("合成演示模式：scope 不可用")),

  searchPrivateDocuments: (
    capabilityToken: string,
    sessionId: string,
    matterId: string,
    query: string,
    limit = 10,
  ): Promise<unknown[]> =>
    call("search_private_documents", { capabilityToken, sessionId, matterId, query, limit }),

  assistantRunDraft: (input: {
    caseId: string;
    intent: string;
    capabilityToken: string;
    sessionId: string;
  }): Promise<AssistantDraftResult> => call<AssistantDraftResult>("assistant_run_draft", { input }),

  assistantCancel: (taskId: string): Promise<void> => call<void>("assistant_cancel", { taskId }),
};