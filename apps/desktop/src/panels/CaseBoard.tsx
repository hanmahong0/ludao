import { useEffect, useState } from "react";
import { ipc } from "../ipc";
import type { Case, ConflictFinding, GapSeverity, PartyInput, TriageItem } from "../types";
import {
  STAGE_LABEL,
  STATUS_DOT,
  STATUS_LABEL,
  cn,
  dueText,
} from "../lib/ui";

// 利冲初筛严重度 → 徽标样式 / 中文标签。
const CONFLICT_BADGE: Record<GapSeverity, string> = {
  critical: "bg-red-100 text-red-700",
  warning: "bg-amber-100 text-amber-700",
  info: "bg-slate-200 text-slate-600",
};
const CONFLICT_LABEL: Record<GapSeverity, string> = {
  critical: "直接利冲",
  warning: "需复核",
  info: "关联",
};

export function CaseBoard({
  onOpenCase,
}: {
  onOpenCase: (caseId: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [triage, setTriage] = useState<TriageItem[]>([]);
  const [conflicts, setConflicts] = useState<ConflictFinding[]>([]);
  const [loading, setLoading] = useState(true);
  const [showNew, setShowNew] = useState(false);

  const load = () =>
    Promise.all([ipc.caseList(), ipc.triageToday(), ipc.conflictScan()]).then(
      ([cs, tr, cf]) => {
        setCases(cs);
        setTriage(tr);
        setConflicts(cf);
        setLoading(false);
      },
    );

  useEffect(() => {
    void load();
  }, []);

  return (
    <div className="mx-auto max-w-5xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5 flex items-start justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold md:text-xl">案件看板</h1>
          <p className="text-sm text-slate-500">
            {loading ? "加载中…" : `共 ${cases.length} 件 · 今日分诊 ${triage.length} 项`}
          </p>
        </div>
        <button
          onClick={() => setShowNew((v) => !v)}
          className="shrink-0 rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700"
        >
          {showNew ? "收起" : "+ 新建案件"}
        </button>
      </header>

      {showNew && (
        <NewCaseForm
          onCreated={() => {
            setShowNew(false);
            void load();
          }}
          onCancel={() => setShowNew(false)}
        />
      )}

      {/* 利冲初筛（conflict.scan）：同一当事人跨案出现的线索 */}
      {conflicts.length > 0 && (
        <section className="mb-6 rounded-xl border border-amber-200 bg-amber-50/50 p-4">
          <h2 className="text-sm font-semibold text-amber-800">
            利益冲突初筛（conflict.scan）
          </h2>
          <p className="mb-3 mt-0.5 text-[12px] text-amber-700/80">
            机械初筛：同一当事人在 ≥2 在办案件中出现的线索。最终利益冲突判断以律师审查为准。
          </p>
          <ul className="flex flex-col gap-2">
            {conflicts.map((f) => (
              <li
                key={f.name + f.severity}
                className="rounded-lg bg-white px-3 py-2 text-sm"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium">{f.name}</span>
                  <span
                    className={cn(
                      "shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium",
                      CONFLICT_BADGE[f.severity],
                    )}
                  >
                    {CONFLICT_LABEL[f.severity]}
                  </span>
                </div>
                <div className="mt-0.5 text-[12px] text-slate-600">
                  {f.explain}
                </div>
                <div className="mt-1.5 flex flex-wrap gap-1.5">
                  {f.hits.map((h, i) => (
                    <button
                      key={i}
                      onClick={() => onOpenCase(h.caseId)}
                      title={`${h.caseTypeLabel}（${h.represented ? "我方委托人" : "对方当事人"}）`}
                      className="rounded bg-slate-100 px-2 py-0.5 text-[11px] text-slate-600 transition hover:bg-slate-200"
                    >
                      <span
                        className={
                          h.represented ? "text-emerald-700" : "text-rose-700"
                        }
                      >
                        {h.represented ? "我方" : "对方"}
                      </span>
                      {" · "}
                      {h.caseTitle}
                    </button>
                  ))}
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}

      {/* 今日分诊条 */}
      {triage.length > 0 && (
        <section className="mb-6 rounded-xl border border-slate-200 bg-white p-4">
          <h2 className="mb-3 text-sm font-semibold text-slate-700">
            今日分诊（triage.today）
          </h2>
          <ul className="flex flex-col gap-2">
            {triage.map((t) => {
              const d = dueText(t.deadline.dueDate);
              return (
                <li
                  key={t.deadline.id}
                  className="flex items-center justify-between gap-3 rounded-lg bg-slate-50 px-3 py-2 text-sm"
                >
                  <div className="min-w-0">
                    <span className="font-medium">{t.deadline.name}</span>
                    <span className="ml-2 text-slate-400">
                      {t.deadline.caseTitle}
                    </span>
                    <div className="text-[12px] text-slate-500">{t.reason}</div>
                  </div>
                  {/* 护栏(R6)：锚点未经律师确认 → 到期日仅草稿，不以权威样式展示 */}
                  {t.deadline.anchorConfirmed ? (
                    <span
                      className={cn(
                        "shrink-0 rounded-full px-2.5 py-1 text-[12px] font-medium",
                        d.urgent
                          ? "bg-red-100 text-red-700"
                          : "bg-slate-200 text-slate-600",
                      )}
                    >
                      {d.text}
                    </span>
                  ) : (
                    <span
                      className="shrink-0 rounded-full bg-amber-100 px-2.5 py-1 text-[12px] font-medium text-amber-700"
                      title="触发日（锚点）未经律师确认，到期日仅为草稿，不作准"
                    >
                      待确认锚点
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {/* 案件卡片 */}
      <section className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {cases.map((c) => {
          const d = dueText(c.nextDeadlineAt);
          return (
            <button
              key={c.id}
              onClick={() => onOpenCase(c.id)}
              className="flex flex-col rounded-xl border border-slate-200 bg-white p-4 text-left transition hover:border-slate-400 hover:shadow-sm"
            >
              <div className="flex items-start justify-between gap-2">
                <span className="text-[12px] font-mono text-slate-400">
                  {c.caseNo}
                </span>
                <span className="flex items-center gap-1.5 text-[12px] text-slate-500">
                  <span
                    className={cn("h-2 w-2 rounded-full", STATUS_DOT[c.status])}
                  />
                  {STATUS_LABEL[c.status]}
                </span>
              </div>
              <div className="mt-1 text-[15px] font-semibold">{c.title}</div>
              <div className="mt-2 flex flex-wrap items-center gap-1.5">
                <span className="rounded bg-slate-100 px-2 py-0.5 text-[12px] text-slate-600">
                  {c.caseTypeLabel}
                </span>
                <span className="rounded bg-indigo-50 px-2 py-0.5 text-[12px] text-indigo-600">
                  {STAGE_LABEL[c.stage]}
                </span>
                {c.amount != null && (
                  <span className="text-[12px] text-slate-500">
                    标的 ¥{c.amount.toLocaleString()}
                  </span>
                )}
              </div>
              <div className="mt-3 flex items-center justify-between border-t border-slate-100 pt-2 text-[12px]">
                <span className="text-slate-400">最近到期</span>
                <span
                  className={cn(
                    "font-medium",
                    d.urgent ? "text-red-600" : "text-slate-600",
                  )}
                >
                  {d.text}
                </span>
              </div>
            </button>
          );
        })}
      </section>
    </div>
  );
}

const CASE_TYPE_OPTIONS: { value: string; label: string }[] = [
  { value: "private_lending", label: "民间借贷纠纷" },
  { value: "sales_contract", label: "买卖合同纠纷" },
  { value: "lease", label: "房屋租赁合同纠纷" },
  { value: "traffic_accident", label: "交通事故责任纠纷" },
  { value: "property_management", label: "物业服务合同纠纷" },
  { value: "labor_dispute", label: "劳动争议" },
  { value: "construction_contract", label: "建设工程施工合同纠纷" },
  { value: "financial_loan", label: "金融借款合同纠纷" },
  { value: "credit_card", label: "信用卡纠纷" },
  { value: "labor_service_injury", label: "提供劳务者受害责任纠纷" },
  { value: "house_sale", label: "房屋买卖合同纠纷" },
];

const fieldCls =
  "rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none";

function NewCaseForm({
  onCreated,
  onCancel,
}: {
  onCreated: () => void;
  onCancel: () => void;
}) {
  const [title, setTitle] = useState("");
  const [caseType, setCaseType] = useState("private_lending");
  const [caseNo, setCaseNo] = useState("");
  const [amount, setAmount] = useState("");
  const [ourParty, setOurParty] = useState("");
  const [ourPosition, setOurPosition] = useState<"plaintiff" | "defendant">(
    "plaintiff",
  );
  const [opposing, setOpposing] = useState("");
  const [includesInterest, setIncludesInterest] = useState(false);
  const [claimRecoverPayment, setClaimRecoverPayment] = useState(false);
  const [hasQualityIssue, setHasQualityIssue] = useState(false);
  const [includesDisability, setIncludesDisability] = useState(false);
  const [includesLostIncome, setIncludesLostIncome] = useState(false);
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    if (!title.trim() || saving) return;
    setSaving(true);
    try {
      const label = CASE_TYPE_OPTIONS.find((o) => o.value === caseType)?.label;
      // 当事人 → parties（我方 represented=true；对方取相反诉讼地位）。供利冲初筛比对。
      const parties: PartyInput[] = [];
      if (ourParty.trim())
        parties.push({
          name: ourParty.trim(),
          role: ourPosition,
          represented: true,
        });
      if (opposing.trim())
        parties.push({
          name: opposing.trim(),
          role: ourPosition === "plaintiff" ? "defendant" : "plaintiff",
          represented: false,
        });
      await ipc.caseCreate({
        title: title.trim(),
        caseType,
        caseTypeLabel: label,
        caseNo: caseNo.trim() || undefined,
        amount: amount ? Number(amount) : undefined,
        includesInterest,
        claimType: claimRecoverPayment ? "recover_payment" : undefined,
        hasQualityIssue,
        includesDisability,
        includesLostIncome,
        stage: "intake",
        parties: parties.length ? parties : undefined,
      });
      onCreated();
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="mb-6 rounded-xl border border-slate-200 bg-white p-4">
      <h2 className="mb-3 text-sm font-semibold text-slate-700">新建案件</h2>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          当事人 / 标题
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="如 张某 诉 李某 民间借贷"
            className={fieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          案由
          <select
            value={caseType}
            onChange={(e) => setCaseType(e.target.value)}
            className={fieldCls}
          >
            {CASE_TYPE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          案号（可空）
          <input
            value={caseNo}
            onChange={(e) => setCaseNo(e.target.value)}
            placeholder="(2026)京…民初…号"
            className={fieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          标的额（元，可空）
          <input
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            inputMode="numeric"
            placeholder="如 580000"
            className={fieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          我方当事人（委托人，可空）
          <input
            value={ourParty}
            onChange={(e) => setOurParty(e.target.value)}
            placeholder="本方委托人姓名/名称"
            className={fieldCls}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          我方诉讼地位
          <select
            value={ourPosition}
            onChange={(e) =>
              setOurPosition(e.target.value as "plaintiff" | "defendant")
            }
            className={fieldCls}
          >
            <option value="plaintiff">原告（起诉方）</option>
            <option value="defendant">被告（应诉方）</option>
          </select>
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          对方当事人（可空）
          <input
            value={opposing}
            onChange={(e) => setOpposing(e.target.value)}
            placeholder="对方当事人姓名/名称（供利冲初筛）"
            className={fieldCls}
          />
        </label>
        {caseType === "private_lending" && (
          <label className="flex items-center gap-2 text-[13px] text-slate-600">
            <input
              type="checkbox"
              checked={includesInterest}
              onChange={(e) => setIncludesInterest(e.target.checked)}
            />
            含利息主张
          </label>
        )}
        {caseType === "sales_contract" && (
          <>
            <label className="flex items-center gap-2 text-[13px] text-slate-600">
              <input
                type="checkbox"
                checked={claimRecoverPayment}
                onChange={(e) => setClaimRecoverPayment(e.target.checked)}
              />
              追收货款（须证明已交付）
            </label>
            <label className="flex items-center gap-2 text-[13px] text-slate-600">
              <input
                type="checkbox"
                checked={hasQualityIssue}
                onChange={(e) => setHasQualityIssue(e.target.checked)}
              />
              涉质量争议
            </label>
          </>
        )}
        {caseType === "traffic_accident" && (
          <>
            <label className="flex items-center gap-2 text-[13px] text-slate-600">
              <input
                type="checkbox"
                checked={includesDisability}
                onChange={(e) => setIncludesDisability(e.target.checked)}
              />
              含伤残赔偿主张
            </label>
            <label className="flex items-center gap-2 text-[13px] text-slate-600">
              <input
                type="checkbox"
                checked={includesLostIncome}
                onChange={(e) => setIncludesLostIncome(e.target.checked)}
              />
              含误工费主张
            </label>
          </>
        )}
      </div>
      <div className="mt-3 flex gap-2">
        <button
          onClick={submit}
          disabled={saving || !title.trim()}
          className="rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700 disabled:opacity-40"
        >
          {saving ? "保存中…" : "创建"}
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
