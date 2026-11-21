import clsx, { type ClassValue } from "clsx";
import type {
  CaseStage,
  CaseStatus,
  EvidenceForm,
  GapSeverity,
} from "../types";

export const cn = (...inputs: ClassValue[]) => clsx(inputs);

export const STAGE_LABEL: Record<CaseStage, string> = {
  intake: "接案",
  evidence: "举证",
  filing: "立案",
  trial: "庭审",
  judgment: "判决",
  execution: "执行",
  closed: "结案",
};

export const STATUS_LABEL: Record<CaseStatus, string> = {
  active: "进行中",
  paused: "暂停",
  archived: "已归档",
};

export const STATUS_DOT: Record<CaseStatus, string> = {
  active: "bg-emerald-500",
  paused: "bg-amber-500",
  archived: "bg-slate-400",
};

export const FORM_LABEL: Record<EvidenceForm, string> = {
  book: "书证",
  electronic: "电子数据",
  cash: "现金/收据",
  physical: "物证",
  testimony: "证人证言",
};

/** 缺口严重度 → 配色（critical 红 / warning 橙 / info 蓝） */
export const SEVERITY_STYLE: Record<
  GapSeverity,
  { label: string; chip: string; bar: string; text: string }
> = {
  critical: {
    label: "致命缺口",
    chip: "bg-red-100 text-red-700 ring-1 ring-red-200",
    bar: "bg-red-500",
    text: "text-red-700",
  },
  warning: {
    label: "需补强",
    chip: "bg-amber-100 text-amber-700 ring-1 ring-amber-200",
    bar: "bg-amber-500",
    text: "text-amber-700",
  },
  info: {
    label: "提示",
    chip: "bg-sky-100 text-sky-700 ring-1 ring-sky-200",
    bar: "bg-sky-500",
    text: "text-sky-700",
  },
};

/** 距今天数（mock 以 2026-06-15 为今天） */
export function daysUntil(iso?: string): number | null {
  if (!iso) return null;
  const today = new Date("2026-06-15");
  const target = new Date(iso);
  return Math.round((target.getTime() - today.getTime()) / 86400000);
}

export function dueText(iso?: string): { text: string; urgent: boolean } {
  const d = daysUntil(iso);
  if (d === null) return { text: "无", urgent: false };
  if (d < 0) return { text: `已过期 ${-d} 天`, urgent: true };
  if (d === 0) return { text: "今天到期", urgent: true };
  return { text: `${d} 天后（${iso}）`, urgent: d <= 7 };
}
