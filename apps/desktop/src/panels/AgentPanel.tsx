import { useEffect, useState } from "react";
import { ipc } from "../ipc";
import { CLOUD_EXPORT_ENABLED, runCloudAgent, runCloudToday } from "../lib/cloudAgent";
import type { AgentType, AssistantRuntimeStatus, Case } from "../types";
import { cn } from "../lib/ui";

type Preset = { label: string; agentType: AgentType; intent: string };

const TODAY_LABEL = "今日全局精力简报";
const TODAY_INTENT =
  "基于跨案脱敏摘要，按到期紧迫度 × 重要性 × 缺口严重度，给出今日全局最该处理的 1–3 件事；" +
  "逐条用 caseRef 序号点名是哪个案件的哪件事并说明理由，简洁可执行。锚点未确认的期限请提示先确认触发日。";

type RefTitle = { caseRef: string; title: string };

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
  onSelectCase,
}: {
  activeCaseId: string;
  capabilityToken: string;
  onSelectCase: (id: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [running, setRunning] = useState(false);
  const [activePreset, setActivePreset] = useState<string | null>(null);
  const [output, setOutput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [legend, setLegend] = useState<RefTitle[]>([]);
  const [runtime, setRuntime] = useState<AssistantRuntimeStatus | null>(null);
  const [localMode, setLocalMode] = useState(true);
  const [cloudOptIn, setCloudOptIn] = useState(false);

  useEffect(() => {
    void ipc.caseList().then(setCases);
    void ipc.assistantStatus().then(setRuntime).catch(() => undefined);
  }, []);

  const toggleAssistant = async () => {
    const next = !(runtime?.enabled ?? false);
    const st = await ipc.assistantSetEnabled(next);
    setRuntime(st);
  };

  const runLocalDraft = async (intent: string) => {
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
    if (!cloudOptIn || !CLOUD_EXPORT_ENABLED) {
      setError("云端通道默认关闭；需构建启用 VITE_CLOUD_EXPORT_ENABLED=1 且勾选明确授权。");
      return;
    }
    if (running) return;
    setRunning(true);
    setActivePreset(preset.label);
    setError(null);
    setOutput("");
    setLegend([]);
    try {
      // 本地 rust-core 产出脱敏上下文（原件/原文不出网）。
      const ctx = await ipc.agentContext(activeCaseId);
      const res = await runCloudAgent({
        ctx,
        agentType: preset.agentType,
        intent: preset.intent,
      });
      if (res.error) setError(res.error);
      else setOutput(res.text ?? "");
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  // 跨案：本地产出脱敏摘要（仅 cases 出网）；legend 仅本地把 caseRef 还原为真实案件。
  const runToday = async () => {
    if (running) return;
    setRunning(true);
    setActivePreset(TODAY_LABEL);
    setError(null);
    setOutput("");
    setLegend([]);
    try {
      const digest = await ipc.triageContext();
      const res = await runCloudToday({ cases: digest.cases, intent: TODAY_INTENT });
      if (res.error) {
        setError(res.error);
      } else {
        setOutput(res.text ?? "");
        setLegend(
          digest.legend.map((l) => ({
            caseRef: l.caseRef,
            title: cases.find((c) => c.id === l.caseId)?.title ?? l.caseId,
          })),
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="mx-auto max-w-3xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">AI 助手</h1>
        <p className="text-sm text-slate-500">
          本地 ZeroClaw sidecar（默认关闭）· 宿主只读法律工具桥 · 云端通道可选
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
              <label className="flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={cloudOptIn}
                  onChange={(e) => setCloudOptIn(e.target.checked)}
                  disabled={!CLOUD_EXPORT_ENABLED}
                />
                明确授权云端（默认关）
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
              本地跨案分诊（期限 × 重要性 × 缺口）→ 仅脱敏摘要出网
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

      {legend.length > 0 && (
        <div className="mt-3 rounded-lg bg-slate-50 px-3 py-2">
          <div className="mb-1 text-[11px] font-medium text-slate-500">
            案件对照（caseRef → 案件，仅本机，未出网）
          </div>
          <div className="flex flex-wrap gap-1.5">
            {legend.map((l) => (
              <span
                key={l.caseRef}
                className="rounded bg-white px-2 py-0.5 text-[12px] text-slate-600 ring-1 ring-slate-200"
              >
                <span className="font-mono text-indigo-600">{l.caseRef}</span> ·{" "}
                {l.title}
              </span>
            ))}
          </div>
        </div>
      )}

      <p className="mt-3 text-[12px] leading-relaxed text-slate-400">
        出网内容仅为脱敏结构（案由 / 标的档位 / 证据形式 / 缺口），由本地引擎组装、云端
        DeepSeek 推理；建议仅供参考，期限与形式要件判断以本地确定性结果为准。
      </p>
    </div>
  );
}
