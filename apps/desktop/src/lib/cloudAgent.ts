import type {
  AgentType,
  DesensitizedContext,
  GapJudgeResult,
  TodayCase,
} from "../types";

/** 外发通道已删除。常量保留为 false，避免旧构建开关重新打开 fetch。 */
export const CLOUD_EXPORT_ENABLED = false;

const CLOSED =
  "云端外发已移除。离线台账与本地助手仍可用；没有逐次脱敏预览就不会出网。";

export interface AgentResult {
  text?: string;
  error?: string;
}

export async function runCloudAgent(_input: {
  ctx: DesensitizedContext;
  agentType: AgentType;
  intent: string;
}): Promise<AgentResult> {
  return { error: CLOSED };
}

export async function runCloudToday(_input: {
  cases: TodayCase[];
  intent: string;
}): Promise<AgentResult> {
  return { error: CLOSED };
}

export interface GapJudgeOutcome {
  result?: GapJudgeResult;
  error?: string;
}

export async function runCloudGapJudge(_input: {
  ctx: DesensitizedContext;
  elementId: string;
  localReason?: string;
}): Promise<GapJudgeOutcome> {
  return { error: CLOSED };
}
