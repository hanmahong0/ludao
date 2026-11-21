import { useEffect, useState } from "react";
import { ipc } from "../ipc";
import type { ConfigStatus } from "../types";
import { cn } from "../lib/ui";

const KIND_LABEL: Record<string, string> = {
  "deadline-rules": "期限规则",
  holidays: "节假日表",
  "claim-elements": "案由要件",
  template: "文书模板",
  catalog: "字段目录",
  "sync-shape": "同步通道",
  "ui-layout": "界面布局",
  flag: "开关",
};

export function ConfigPanel() {
  const [configs, setConfigs] = useState<ConfigStatus[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    ipc.configStatus().then((c) => {
      setConfigs(c);
      setLoading(false);
    });
  }, []);

  const pending = configs.filter(
    (c) => c.reviewStatus === "pending_review",
  ).length;

  return (
    <div className="mx-auto max-w-4xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">配置中心</h1>
        <p className="text-sm text-slate-500">
          {loading
            ? "加载中…"
            : `${configs.length} 个生效配置 · ${pending} 个待复核`}
        </p>
      </header>

      <div className="mb-5 rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 text-[13px] text-slate-600">
        配置经 Ed25519 验签后本地加密缓存，断网用最后已知有效版本。标「待复核」者为新风险源——
        未经法务 / 资深律师终核前对用户仅作「参考 / 草稿」，绝不冒充权威。
      </div>

      {loading ? (
        <p className="text-sm text-slate-400">加载中…</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {configs.map((c) => {
            const isPending = c.reviewStatus === "pending_review";
            return (
              <li
                key={`${c.kind}/${c.configId}`}
                className="rounded-xl border border-slate-200 bg-white p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-medium">
                        {KIND_LABEL[c.kind] ?? c.kind}
                      </span>
                      <span className="font-mono text-[12px] text-slate-400">
                        {c.configId}
                      </span>
                    </div>
                    <div className="mt-1 text-[12px] text-slate-500">
                      版本 {c.version} · schema v{c.schemaVersion}
                      {c.effectiveFrom ? ` · 生效 ${c.effectiveFrom}` : ""}
                    </div>
                    {c.note && (
                      <p className="mt-2 text-[12px] leading-relaxed text-slate-600">
                        {c.note}
                      </p>
                    )}
                  </div>
                  <span
                    className={cn(
                      "shrink-0 rounded-full px-2.5 py-1 text-[12px] font-medium",
                      isPending
                        ? "bg-amber-100 text-amber-700"
                        : "bg-emerald-100 text-emerald-700",
                    )}
                  >
                    {isPending ? "待复核" : "已核验"}
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
