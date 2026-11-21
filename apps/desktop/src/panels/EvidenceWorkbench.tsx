import { useEffect, useState } from "react";
import { ipc } from "../ipc";
import { runCloudGapJudge } from "../lib/cloudAgent";
import type {
  Case,
  Evidence,
  EvidenceForm,
  Fact,
  GapDiagnosis,
  GapJudgeResult,
} from "../types";
import { FORM_LABEL, SEVERITY_STYLE, cn } from "../lib/ui";

export function EvidenceWorkbench({
  activeCaseId,
  onSelectCase,
}: {
  activeCaseId: string;
  onSelectCase: (id: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [facts, setFacts] = useState<Fact[]>([]);
  const [evidence, setEvidence] = useState<Evidence[]>([]);
  const [gap, setGap] = useState<GapDiagnosis | null>(null);
  const [showNewEv, setShowNewEv] = useState(false);
  const [exporting, setExporting] = useState(false);

  const reloadEvidence = () =>
    Promise.all([
      ipc.evidenceList(activeCaseId),
      ipc.gapDiagnose(activeCaseId),
    ]).then(([e, g]) => {
      setEvidence(e);
      setGap(g);
    });

  useEffect(() => {
    ipc.caseList().then(setCases);
  }, []);

  useEffect(() => {
    setGap(null);
    setShowNewEv(false);
    Promise.all([
      ipc.factList(activeCaseId),
      ipc.evidenceList(activeCaseId),
      ipc.gapDiagnose(activeCaseId),
    ]).then(([f, e, g]) => {
      setFacts(f);
      setEvidence(e);
      setGap(g);
    });
  }, [activeCaseId]);

  const activeCase = cases.find((c) => c.id === activeCaseId);

  const onExport = async () => {
    if (!activeCase || exporting) return;
    setExporting(true);
    try {
      // 动态加载：docx 渲染库仅在首次导出时拉取，不进首屏包。
      const { exportEvidenceListDocx } = await import("../lib/exportDocx");
      await exportEvidenceListDocx(activeCase, evidence);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="mx-auto max-w-6xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">证据工作台</h1>
        <div className="mt-2 flex flex-wrap gap-1.5">
          {cases.map((c) => (
            <button
              key={c.id}
              onClick={() => onSelectCase(c.id)}
              className={cn(
                "rounded-full px-3 py-1 text-[12px] transition",
                c.id === activeCaseId
                  ? "bg-slate-900 text-white"
                  : "bg-white text-slate-600 ring-1 ring-slate-200 hover:ring-slate-400",
              )}
            >
              {c.title}
            </button>
          ))}
        </div>
        {activeCase && (
          <p className="mt-2 text-sm text-slate-500">
            {activeCase.caseTypeLabel} · {activeCase.caseNo}
          </p>
        )}
      </header>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-3">
        {/* 事实时间轴 */}
        <section className="lg:col-span-1">
          <h2 className="mb-3 text-sm font-semibold text-slate-700">
            事实时间轴（按 fact_date）
          </h2>
          <ol className="relative border-l-2 border-slate-200 pl-4">
            {facts.length === 0 && (
              <li className="text-sm text-slate-400">暂无事实</li>
            )}
            {facts.map((f) => (
              <li key={f.id} className="mb-4 last:mb-0">
                <span className="absolute -left-[7px] mt-1 h-3 w-3 rounded-full border-2 border-white bg-indigo-500" />
                <time className="text-[12px] font-mono text-indigo-600">
                  {f.factDate}
                </time>
                <p className="text-sm text-slate-700">{f.summary}</p>
                <span className="text-[11px] text-slate-400">
                  证据 {f.evidenceIds.join("、") || "—"}
                </span>
              </li>
            ))}
          </ol>
        </section>

        {/* 证据表 */}
        <section className="lg:col-span-2">
          <div className="mb-3 flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold text-slate-700">证据清单</h2>
            <div className="flex items-center gap-2">
              <button
                onClick={onExport}
                disabled={exporting || evidence.length === 0}
                title="生成 .docx 证据清单（本地渲染，数据不出本机）"
                className="rounded-lg px-2.5 py-1 text-[12px] font-medium text-slate-600 ring-1 ring-slate-300 transition hover:bg-slate-100 disabled:opacity-40"
              >
                {exporting ? "导出中…" : "⬇ 导出证据清单"}
              </button>
              <button
                onClick={() => setShowNewEv((v) => !v)}
                className="rounded-lg bg-slate-900 px-2.5 py-1 text-[12px] font-medium text-white transition hover:bg-slate-700"
              >
                {showNewEv ? "收起" : "+ 新增证据"}
              </button>
            </div>
          </div>
          {showNewEv && (
            <NewEvidenceForm
              caseId={activeCaseId}
              onCreated={() => {
                setShowNewEv(false);
                void reloadEvidence();
              }}
              onCancel={() => setShowNewEv(false)}
            />
          )}
          <div className="overflow-x-auto rounded-xl border border-slate-200 bg-white">
            <table className="w-full text-left text-sm">
              <thead className="bg-slate-50 text-[12px] text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-medium">序号</th>
                  <th className="px-3 py-2 font-medium">名称</th>
                  <th className="px-3 py-2 font-medium">来源</th>
                  <th className="px-3 py-2 font-medium">证明对象</th>
                  <th className="px-3 py-2 font-medium">三性</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {evidence.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-3 py-4 text-slate-400">
                      暂无证据
                    </td>
                  </tr>
                )}
                {evidence.map((e) => (
                  <tr key={e.id} className="align-top hover:bg-slate-50">
                    <td className="px-3 py-2 text-slate-400">{e.seq}</td>
                    <td className="px-3 py-2">
                      <div className="font-medium">{e.name}</div>
                      <span className="text-[11px] text-slate-400">
                        {FORM_LABEL[e.form]}
                      </span>
                    </td>
                    <td className="px-3 py-2 text-slate-600">{e.source}</td>
                    <td className="px-3 py-2 text-slate-600">{e.proofTarget}</td>
                    <td className="px-3 py-2">
                      <div className="flex gap-1">
                        <NatureChip ok={e.threeNatures.authenticity} label="真" />
                        <NatureChip ok={e.threeNatures.legality} label="合" />
                        <NatureChip ok={e.threeNatures.relevance} label="关" />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* 缺口诊断面板 */}
          <h2 className="mb-3 mt-6 text-sm font-semibold text-slate-700">
            缺口诊断（gap.diagnose · 本地 CEL 引擎）
          </h2>
          <GapPanel gap={gap} activeCaseId={activeCaseId} />
        </section>
      </div>
    </div>
  );
}

function NatureChip({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span
      className={cn(
        "flex h-5 w-5 items-center justify-center rounded text-[11px] font-medium",
        ok ? "bg-emerald-100 text-emerald-700" : "bg-red-100 text-red-600",
      )}
      title={ok ? "无异议" : "有异议"}
    >
      {label}
    </span>
  );
}

function GapPanel({
  gap,
  activeCaseId,
}: {
  gap: GapDiagnosis | null;
  activeCaseId: string;
}) {
  if (!gap) return <p className="text-sm text-slate-400">诊断中…</p>;

  return (
    <div className="flex flex-col gap-3">
      {gap.satisfied.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 rounded-lg bg-emerald-50 px-3 py-2 text-[12px] text-emerald-700">
          <span className="font-medium">已满足要件：</span>
          {gap.satisfied.map((s) => (
            <span
              key={s.elementId}
              className="rounded bg-white px-2 py-0.5 ring-1 ring-emerald-200"
            >
              {s.name}
            </span>
          ))}
        </div>
      )}

      {gap.gaps.map((g) => {
        const sv = SEVERITY_STYLE[g.severity];
        return (
          <div
            key={g.elementRef + g.severity}
            className="flex gap-3 rounded-xl border border-slate-200 bg-white p-3"
          >
            <span className={cn("mt-0.5 w-1 shrink-0 rounded-full", sv.bar)} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span
                  className={cn(
                    "rounded-full px-2 py-0.5 text-[11px] font-medium",
                    sv.chip,
                  )}
                >
                  {sv.label}
                </span>
                <span className="font-mono text-[12px] text-slate-400">
                  {g.elementRef}
                </span>
              </div>
              <p className="mt-1.5 text-sm text-slate-700">{g.explain}</p>
              <p className={cn("mt-1 text-[13px]", sv.text)}>建议：{g.suggest}</p>
            </div>
          </div>
        );
      })}

      {gap.gaps.length === 0 && (
        <p className="rounded-lg bg-emerald-50 px-3 py-2 text-sm text-emerald-700">
          形式要件已齐备，未发现缺口。
        </p>
      )}

      {gap.needsCloudJudgment.length > 0 && (
        <div className="rounded-xl border border-dashed border-violet-300 bg-violet-50 p-3">
          <div className="text-[12px] font-medium text-violet-700">
            需云端裁量判断（gapJudge · 经脱敏后出网）
          </div>
          <ul className="mt-2 flex flex-col gap-2">
            {gap.needsCloudJudgment.map((n) => (
              <NeedsCloudJudgmentItem
                key={n.elementId}
                activeCaseId={activeCaseId}
                elementId={n.elementId}
                reason={n.reason}
              />
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

const VERDICT_META: Record<
  GapJudgeResult["verdict"],
  { label: string; chip: string }
> = {
  "likely-satisfied": {
    label: "倾向满足",
    chip: "bg-emerald-100 text-emerald-700",
  },
  "likely-gap": { label: "倾向缺口", chip: "bg-amber-100 text-amber-700" },
  "needs-human": { label: "需人工裁量", chip: "bg-violet-100 text-violet-700" },
};

/**
 * 单个"需裁量"要件：点按经本地脱敏上下文 → 云端 gapJudge → 渲染裁量结论。
 * 形式门槛由本地 CEL 已判；此处只补实体裁量。原件/原文不出本机。
 */
function NeedsCloudJudgmentItem({
  activeCaseId,
  elementId,
  reason,
}: {
  activeCaseId: string;
  elementId: string;
  reason: string;
}) {
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<GapJudgeResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const judge = async () => {
    if (running) return;
    setRunning(true);
    setError(null);
    try {
      // 本地 rust-core 产出脱敏上下文（原件/原文不出网）。
      const ctx = await ipc.agentContext(activeCaseId);
      const out = await runCloudGapJudge({
        ctx,
        elementId,
        localReason: reason,
      });
      if (out.error) setError(out.error);
      else setResult(out.result ?? null);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const vm = result ? VERDICT_META[result.verdict] : null;

  return (
    <li className="rounded-lg bg-white/70 p-2.5 ring-1 ring-violet-200">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 text-[13px] text-violet-800">
          <span className="font-mono">{elementId}</span>：{reason}
        </div>
        <button
          onClick={judge}
          disabled={running}
          className="shrink-0 rounded-md bg-violet-600 px-2 py-1 text-[11px] font-medium text-white transition hover:bg-violet-500 disabled:opacity-40"
        >
          {running ? "裁量中…" : result ? "重新裁量" : "云端裁量复核"}
        </button>
      </div>

      {error && (
        <div className="mt-2 text-[12px] leading-relaxed text-amber-700">
          {error}
        </div>
      )}

      {result && vm && (
        <div className="mt-2 border-t border-violet-100 pt-2">
          <span
            className={cn(
              "rounded-full px-2 py-0.5 text-[11px] font-medium",
              vm.chip,
            )}
          >
            {vm.label}
          </span>
          <p className="mt-1.5 whitespace-pre-wrap text-[13px] leading-relaxed text-slate-700">
            {result.rationale}
          </p>
          {result.suggestedEvidence.length > 0 && (
            <div className="mt-1.5 flex flex-wrap items-center gap-1 text-[12px] text-slate-500">
              <span>建议补强：</span>
              {result.suggestedEvidence.map((s, i) => (
                <span
                  key={i}
                  className="rounded bg-slate-100 px-1.5 py-0.5 text-slate-600"
                >
                  {s}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </li>
  );
}

const EV_FORMS: EvidenceForm[] = [
  "book",
  "electronic",
  "cash",
  "physical",
  "testimony",
];

const evFieldCls =
  "rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none";

function NewEvidenceForm({
  caseId,
  onCreated,
  onCancel,
}: {
  caseId: string;
  onCreated: () => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [form, setForm] = useState<EvidenceForm>("book");
  const [source, setSource] = useState("");
  const [proofTarget, setProofTarget] = useState("");
  const [linked, setLinked] = useState("");
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    try {
      await ipc.evidenceCreate({
        caseId,
        name: name.trim(),
        form,
        source: source.trim() || undefined,
        proofTarget: proofTarget.trim() || undefined,
        linkedElementIds: linked
          .split(/[,，\s]+/)
          .map((s) => s.trim())
          .filter(Boolean),
      });
      onCreated();
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="mb-3 rounded-xl border border-slate-200 bg-white p-4">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          证据名称
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="如 借条 / 银行转账回单"
            className={evFieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          形式
          <select
            value={form}
            onChange={(e) => setForm(e.target.value as EvidenceForm)}
            className={evFieldCls}
          >
            {EV_FORMS.map((f) => (
              <option key={f} value={f}>
                {FORM_LABEL[f]}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          来源（可空）
          <input
            value={source}
            onChange={(e) => setSource(e.target.value)}
            placeholder="如 原告提供 / 银行调取"
            className={evFieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          证明对象（可空）
          <input
            value={proofTarget}
            onChange={(e) => setProofTarget(e.target.value)}
            placeholder="如 款项实际交付"
            className={evFieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500 sm:col-span-2">
          关联要件 id（逗号分隔，缺口诊断据此对照）
          <input
            value={linked}
            onChange={(e) => setLinked(e.target.value)}
            placeholder="如 loan_relationship, actual_delivery"
            className={evFieldCls}
          />
        </label>
      </div>
      <div className="mt-3 flex gap-2">
        <button
          onClick={submit}
          disabled={saving || !name.trim()}
          className="rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700 disabled:opacity-40"
        >
          {saving ? "保存中…" : "添加证据"}
        </button>
        <button
          onClick={onCancel}
          className="rounded-lg px-3 py-1.5 text-sm text-slate-500 transition hover:bg-slate-100"
        >
          取消
        </button>
      </div>
    </section>
  );
}
