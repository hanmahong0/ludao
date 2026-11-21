import type {
  AgentType,
  DesensitizedContext,
  GapJudgeResult,
  TodayCase,
} from "../types";

export const CLOUD_EXPORT_ENABLED =
  (import.meta as unknown as { env?: Record<string, string | undefined> }).env
    ?.VITE_CLOUD_EXPORT_ENABLED === "1";

const CLOUD_URL: string =
  (import.meta as unknown as { env?: Record<string, string | undefined> }).env
    ?.VITE_CLOUD_URL ?? "http://localhost:8787";

export interface AgentResult {
  text?: string;
  error?: string;
}

/**
 * 调云端 `/agent`（仅传【已脱敏】上下文，由 rust-core 的 agent_context 产出）。
 * 云端不可达时返回友好错误而非抛出，便于 UI 提示"先启动云端服务"。
 * 出网内容仅脱敏结构；云端 assertDesensitized 再兜底（红线①③）。
 */
export async function runCloudAgent(input: {
  ctx: DesensitizedContext;
  agentType: AgentType;
  intent: string;
}): Promise<AgentResult> {
  if (!CLOUD_EXPORT_ENABLED) {
    return { error: "云端出网已默认关闭。设置 VITE_CLOUD_EXPORT_ENABLED=1 后方可使用。" };
  }
  try {
    const res = await fetch(`${CLOUD_URL}/agent`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    });
    const json = (await res.json().catch(() => ({}))) as AgentResult;
    if (!res.ok) return { error: json.error ?? `云端返回 HTTP ${res.status}` };
    return json;
  } catch (e) {
    return {
      error: `无法连接云端服务（${CLOUD_URL}）。请先启动：npm --prefix services/cloud run dev（${String(e)}）`,
    };
  }
}

/**
 * 调云端 `/today`：跨案"今日最该处理"简报。
 * ★只发 cases（脱敏可出网），绝不发 legend（caseId/标题本地映射）。
 * 云端 .strict() + assertDesensitized 再兜底（红线①③）。
 */
export async function runCloudToday(input: {
  cases: TodayCase[];
  intent: string;
}): Promise<AgentResult> {
  if (!CLOUD_EXPORT_ENABLED) {
    return { error: "云端出网已默认关闭。设置 VITE_CLOUD_EXPORT_ENABLED=1 后方可使用。" };
  }
  try {
    const res = await fetch(`${CLOUD_URL}/today`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ digest: { cases: input.cases }, intent: input.intent }),
    });
    const json = (await res.json().catch(() => ({}))) as AgentResult;
    if (!res.ok) return { error: json.error ?? `云端返回 HTTP ${res.status}` };
    return json;
  } catch (e) {
    return {
      error: `无法连接云端服务（${CLOUD_URL}）。请先启动：npm --prefix services/cloud run dev（${String(e)}）`,
    };
  }
}

export interface GapJudgeOutcome {
  result?: GapJudgeResult;
  error?: string;
}

/**
 * 调云端 `/gap-judge`：对单个本地引擎挑出的"需裁量"要件做实体判断。
 * 仅传【已脱敏】上下文 + 该要件的规则级理由（红线①③，云端再兜底）。
 * 云端不可达时返回友好错误而非抛出。
 */
export async function runCloudGapJudge(input: {
  ctx: DesensitizedContext;
  elementId: string;
  localReason?: string;
}): Promise<GapJudgeOutcome> {
  if (!CLOUD_EXPORT_ENABLED) {
    return { error: "云端出网已默认关闭。设置 VITE_CLOUD_EXPORT_ENABLED=1 后方可使用。" };
  }
  try {
    const res = await fetch(`${CLOUD_URL}/gap-judge`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    });
    const json = (await res.json().catch(() => ({}))) as GapJudgeResult & {
      error?: string;
    };
    if (!res.ok) return { error: json.error ?? `云端返回 HTTP ${res.status}` };
    return { result: json };
  } catch (e) {
    return {
      error: `无法连接云端服务（${CLOUD_URL}）。请先启动：npm --prefix services/cloud run dev（${String(e)}）`,
    };
  }
}
