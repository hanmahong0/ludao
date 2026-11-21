import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import type { CapabilityGrant } from "./types";
import { CaseBoard } from "./panels/CaseBoard";
import { EvidenceWorkbench } from "./panels/EvidenceWorkbench";
import { DeadlinePanel } from "./panels/DeadlinePanel";
import { ConfigPanel } from "./panels/ConfigPanel";
import { AgentPanel } from "./panels/AgentPanel";
import { DocumentPanel } from "./panels/DocumentPanel";
import { PublicCasePanel } from "./panels/PublicCasePanel";
import { BACKEND_IS_REAL } from "./ipc";
import { cn } from "./lib/ui";

type NavKey = "board" | "evidence" | "deadline" | "doc" | "public" | "agent" | "config";

const NAV: { key: NavKey; label: string; icon: string; desc: string }[] = [
  { key: "board", label: "案件看板", icon: "▤", desc: "跨案分诊与进度" },
  { key: "evidence", label: "证据工作台", icon: "◷", desc: "时间轴·证据·缺口" },
  { key: "deadline", label: "期限管家", icon: "⏱", desc: "法定期限·锚点确认" },
  { key: "doc", label: "文书", icon: "✎", desc: "模板骨架·本地生成" },
  { key: "public", label: "类案", icon: "⚖", desc: "授权来源·FTS 检索" },
  { key: "agent", label: "AI 助手", icon: "✦", desc: "ZeroClaw·默认关闭" },
  { key: "config", label: "配置中心", icon: "⚙", desc: "版本·验签·复核" },
];

export default function App() {
  const [nav, setNav] = useState<NavKey>("board");
  const [activeCaseId, setActiveCaseId] = useState<string>("case-001");
  const [capability, setCapability] = useState<CapabilityGrant | null>(null);

  useEffect(() => {
    void ipc.scopeGrantForMatter(activeCaseId).then(setCapability).catch(() => setCapability(null));
  }, [activeCaseId]);

  const openEvidence = (caseId: string) => {
    setActiveCaseId(caseId);
    setNav("evidence");
  };

  return (
    <div className="flex h-full flex-col md:flex-row text-slate-800">
      {/* 侧边导航（桌面）/ 顶栏（移动） */}
      <aside className="flex shrink-0 flex-row items-center gap-1 border-b border-slate-200 bg-white px-3 py-2 md:w-60 md:flex-col md:items-stretch md:gap-2 md:border-b-0 md:border-r md:px-3 md:py-5">
        <div className="mr-3 flex items-center gap-2 md:mb-4 md:mr-0 md:px-2">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-slate-900 text-sm font-bold text-white">
            律
          </div>
          <div className="hidden md:block">
            <div className="text-sm font-semibold leading-tight">律道工作台</div>
            <div className="text-[11px] text-slate-400">本地优先 · 可离线</div>
          </div>
        </div>
        {NAV.map((item) => (
          <button
            key={item.key}
            onClick={() => setNav(item.key)}
            className={cn(
              "flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition",
              nav === item.key
                ? "bg-slate-900 text-white"
                : "text-slate-600 hover:bg-slate-100",
            )}
          >
            <span className="text-base">{item.icon}</span>
            <span className="flex flex-col">
              <span className="font-medium">{item.label}</span>
              <span
                className={cn(
                  "hidden text-[11px] md:block",
                  nav === item.key ? "text-slate-300" : "text-slate-400",
                )}
              >
                {item.desc}
              </span>
            </span>
          </button>
        ))}
        <div className="mt-auto hidden px-2 text-[11px] text-slate-400 md:block">
          {BACKEND_IS_REAL ? "本地加密库 · Tauri 已接" : "mock IPC · 浏览器预览"}
        </div>
      </aside>

      {/* 主区 */}
      <main className="flex-1 overflow-y-auto">
        {nav === "board" ? (
          <CaseBoard onOpenCase={openEvidence} />
        ) : nav === "evidence" ? (
          <EvidenceWorkbench
            activeCaseId={activeCaseId}
            onSelectCase={setActiveCaseId}
          />
        ) : nav === "deadline" ? (
          <DeadlinePanel
            activeCaseId={activeCaseId}
            onSelectCase={setActiveCaseId}
          />
        ) : nav === "doc" ? (
          <DocumentPanel
            activeCaseId={activeCaseId}
            onSelectCase={setActiveCaseId}
          />
        ) : nav === "public" ? (
          <PublicCasePanel />
        ) : nav === "agent" ? (
          <AgentPanel
            activeCaseId={activeCaseId}
            capabilityToken={capability?.token ?? ""}
            onSelectCase={setActiveCaseId}
          />
        ) : (
          <ConfigPanel />
        )}
      </main>
    </div>
  );
}
