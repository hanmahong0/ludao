import { useEffect, useState } from "react";
import { BACKEND_IS_REAL, ipc } from "../ipc";
import type { AgentType, AssistantRuntimeStatus, Case } from "../types";
import { cn } from "../lib/ui";

type Preset = { label: string; agentType: AgentType; intent: string };

const TODAY_LABEL = "今日全局精力简报";
const PRESETS: Preset[] = [
  {
    label: "今日最该处理",
    agentType: "attention",
    intent:
      "基于本案脱敏上下文，给出今日最该处理的 1–3 件事及理由，按优先级排序，简洁可执行。",
  },
  {
    label: "案情秒级简报",
    agentType: "attention",
    intent: "用不超过 6 句话给出本案争点、当前进度与下一步建议（基于脱敏上下文）。",
  },
  {
    label: "证据缺口解读",
    agentType: "evidence",
    intent: "解读本案证据缺口：逐项说明缺什么、为何重要、如何补强；并提示需人工裁量之处。",
  },
];

export function AgentPanel({
  activeCaseId,
  capabilityToken,
  sessionId,
  onSelectCase,
}: {
  activeCaseId: string;
  capabilityToken: string;
  sessionId: string;
  onSelectCase: (id: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [running, setRunning] = useState(false);
  const [activePreset, setActivePreset] = useState<string | null>(null);
  const [output, setOutput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState<AssistantRuntimeStatus | null>(null);
  const [localMode, setLocalMode] = useState(true);

  useEffect(() => {
    void ipc.caseList().then(setCases);
    void ipc.assistantStatus().then(setRuntime).catch(() => undefined);
  }, []);

  const toggleAssistant = async () => {
    if (!BACKEND_IS_REAL) {
      setError("合成演示模式：助手不可用。请使用 Tauri 桌面。");
      return;
    }
    try {
      const next = !(runtime?.enabled ?? false);
      const st = await ipc.assistantSetEnabled(next);
      setRuntime(st);
    } catch (e) {
      setError(String(e));
    }
  };

  const runLocalDraft = async (intent: string) => {
    if (!BACKEND_IS_REAL) {
      setError("合成演示模式：助手与 scope 不可用。请使用 Tauri 桌面。");
      return;
    }
    if (running) return;
    setRunning(true);
    setError(null);
    setOutput("");
    try {
      if (!capabilityToken) {
        setError("宿主未签发案件 capability；请重新选择案件。");
        return;
      }
      const res = await ipc.assistantRunDraft({
        caseId: activeCaseId,
        intent,
        capabilityToken,
        sessionId,
      });
      if (res.error) setError(res.error);
      else setOutput(`${res.text}\n\n[runtime: ${res.runtimeEvidence}]`);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const run = async (preset: Preset) => {
    setActivePreset(preset.label);
    setError("云端外发已移除。请勾选本地助手；离线台账不依赖云。");
  };

  const runToday = async () => {
    setActivePreset(TODAY_LABEL);
    setError("云端外发已移除。今日简报不再出网；请使用本地期限与缺口面板。");
  };

  return (
    <div className="mx-auto max-w-3xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">AI 助手</h1>
        <p className="text-sm text-slate-500">
          本地 ZeroClaw sidecar（默认关闭）· 宿主只读法律工具 · 云端外发已移除
        </p>
        {runtime && (
          <div className="mt-2 rounded-lg border border-slate-200 bg-slate-50 p-3 text-xs text-slate-600">
            <div>
              运行状态：{runtime.enabled ? "已启用" : "已关闭"} · 握手：
              {runtime.handshakeOk ? "OK" : "未验证"} · 模型：
              {runtime.modelConfigured ? "已配置" : "未配置"}
            </div>
            <div className="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                className="rounded border px-2 py-1"
                onClick={() => void toggleAssistant()}
              >
                {runtime.enabled ? "关闭助手" : "尝试启用"}
              </button>
              <label className="flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={localMode}
                  onChange={(e) => setLocalMode(e.target.checked)}
                />
                优先本地宿主工具/ZeroClaw
              </label>
              <label className="flex items-center gap-1 text-slate-400">
                <input type="checkbox" checked={false} disabled readOnly />
                云端外发已移除
              </label>
            </div>
            <p className="mt-2 text-slate-500">{runtime.disclosure}</p>
            {runtime.lastError && (
              <p className="mt-1 text-amber-700">{runtime.lastError}</p>
            )}
          </div>
        )}
      </header>

      {/* 跨全部案件：精力管家的核心——一眼看清"今天全局最该处理什么" */}
      <div className="mb-5 rounded-xl border border-slate-200 bg-gradient-to-br from-slate-50 to-white p-4">
        <div className="mb-2 flex items-center justify-between gap-3">
          <div>
            <div className="text-sm font-semibold text-slate-800">跨全部案件</div>
            <div className="text-[12px] text-slate-500">
              本地跨案分诊仍在期限与缺口面板。此按钮不再出网。
            </div>
          </div>
          <button
            onClick={runToday}
            disabled={running}
            className={cn(
              "shrink-0 rounded-lg px-3 py-1.5 text-sm font-medium text-white transition disabled:opacity-40",
              running && activePreset === TODAY_LABEL
                ? "bg-indigo-400"
                : "bg-indigo-600 hover:bg-indigo-500",
            )}
          >
            {running && activePreset === TODAY_LABEL ? "汇总中…" : `✦ ${TODAY_LABEL}`}
          </button>
        </div>
      </div>

      <div className="mb-2 text-[12px] font-medium text-slate-500">针对当前案件</div>
      <div className="mb-4 flex items-center gap-2">
        <span className="text-[12px] text-slate-500">案件</span>
        <select
          value={activeCaseId}
          onChange={(e) => onSelectCase(e.target.value)}
          className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none"
        >
          {cases.map((c) => (
            <option key={c.id} value={c.id}>
              {c.title}
            </option>
          ))}
        </select>
      </div>

      <div className="mb-4 flex flex-wrap gap-2">
        {PRESETS.map((p) => (
          <button
            key={p.label}
            onClick={() => (localMode ? void runLocalDraft(p.intent) : void run(p))}
            disabled={running}
            className={cn(
              "rounded-lg px-3 py-1.5 text-sm font-medium text-white transition disabled:opacity-40",
              running && activePreset === p.label
                ? "bg-slate-500"
                : "bg-slate-900 hover:bg-slate-700",
            )}
          >
            {running && activePreset === p.label ? "生成中…" : p.label}
          </button>
        ))}
      </div>

      <div className="min-h-[180px] whitespace-pre-wrap rounded-xl border border-slate-200 bg-white p-4 text-sm leading-relaxed text-slate-700">
        {error ? (
          <div className="text-amber-700">
            <div className="font-medium">未能获取建议</div>
            <div className="mt-1 text-[13px] leading-relaxed">{error}</div>
          </div>
        ) : output ? (
          output
        ) : (
          <span className="text-slate-400">
            选上面一个动作，AI 将基于【脱敏上下文】给出建议。原件 / 原文不出本机。
          </span>
        )}
      </div>

      <p className="mt-3 text-[12px] leading-relaxed text-slate-400">
        没有模型密钥时，本地助手只给出规则摘要，并标明 runtime 证据。期限与形式要件以本地确定性结果为准。
      </p>
    </div>
  );
}
