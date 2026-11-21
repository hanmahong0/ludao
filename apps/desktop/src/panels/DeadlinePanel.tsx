import { useEffect, useState } from "react";
import { ipc } from "../ipc";
import type { Case, Deadline, TriageItem } from "../types";
import { cn, dueText } from "../lib/ui";

// 民事一审常用法定期限（与 cn-civil-first-instance.json 的 deadlines[].id 对齐；
// 文案仅作选单提示，真实天数与到期日永远由 rust-core 引擎读签名配置求值）。
const RULE_OPTIONS: { value: string; label: string; trigger: string }[] = [
  { value: "answer_period", label: "答辩期", trigger: "收到起诉状副本之日" },
  { value: "jurisdiction_objection", label: "管辖权异议", trigger: "收到起诉状副本之日" },
  { value: "court_fee_payment", label: "预交诉讼费", trigger: "收到缴费通知之日" },
  { value: "evidence_period", label: "举证期限", trigger: "法院指定（受理后）" },
  { value: "appeal_period", label: "上诉期", trigger: "收到判决书/裁定书之日" },
  { value: "retrial_application", label: "申请再审", trigger: "判决/裁定生效之日" },
  {
    value: "enforcement_application",
    label: "申请强制执行",
    trigger: "履行期间最后一日起",
  },
  {
    value: "limitation_general",
    label: "诉讼时效（一般）",
    trigger: "知道权利受损害之日",
  },
];

const fieldCls =
  "rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none";

export function DeadlinePanel({
  activeCaseId,
  onSelectCase,
}: {
  activeCaseId: string;
  onSelectCase: (id: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [triage, setTriage] = useState<TriageItem[]>([]);
  const [deadlines, setDeadlines] = useState<Deadline[]>([]);
  const [loading, setLoading] = useState(true);
  const [showNew, setShowNew] = useState(false);
  const [govNote, setGovNote] = useState<string | null>(null);

  const reload = async () => {
    const [cs, tr, dl, cfg] = await Promise.all([
      ipc.caseList(),
      ipc.triageToday(),
      ipc.deadlineListByCase(activeCaseId),
      ipc.configStatus(),
    ]);
    setCases(cs);
    setTriage(tr);
    setDeadlines(dl);
    // 治理透明化：期限规则/节假日表未经法务终核时，明示到期日仅供参考。
    const pending = cfg.filter(
      (c) =>
        (c.kind === "deadline-rules" || c.kind === "holidays") &&
        c.reviewStatus === "pending_review",
    );
    setGovNote(
      pending.length > 0
        ? "期限规则与节假日表尚未经法务终核，到期日仅供参考，请以现行法条与官方节假日安排核对。"
        : null,
    );
    setLoading(false);
  };

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeCaseId]);

  const activeCase = cases.find((c) => c.id === activeCaseId);

  return (
    <div className="mx-auto max-w-5xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">期限管家</h1>
        <p className="text-sm text-slate-500">
          {loading
            ? "加载中…"
            : `今日分诊 ${triage.length} 项 · 本案期限 ${deadlines.length} 条`}
        </p>
      </header>

      {govNote && (
        <div className="mb-4 rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-[13px] text-amber-800">
          <span className="font-medium">⚠ 待核对：</span>
          {govNote}
        </div>
      )}

      {/* 今日分诊（跨案）：未确认锚点的可直接确认 */}
      <section className="mb-6 rounded-xl border border-slate-200 bg-white p-4">
        <h2 className="mb-3 text-sm font-semibold text-slate-700">
          今日分诊（triage.today）
        </h2>
        {triage.length === 0 ? (
          <p className="text-sm text-slate-400">
            暂无待办期限。到下方某个案件「+ 新建期限」即可纳入分诊。
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {triage.map((t) => (
              <DeadlineRow
                key={t.deadline.id}
                deadline={t.deadline}
                reason={t.reason}
                onConfirmed={reload}
              />
            ))}
          </ul>
        )}
      </section>

      {/* 案件选择 + 本案期限 */}
      <section className="rounded-xl border border-slate-200 bg-white p-4">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold text-slate-700">本案期限</h2>
            <select
              value={activeCaseId}
              onChange={(e) => onSelectCase(e.target.value)}
              className={fieldCls}
            >
              {cases.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
            </select>
          </div>
          <button
            onClick={() => setShowNew((v) => !v)}
            className="shrink-0 rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700"
          >
            {showNew ? "收起" : "+ 新建期限"}
          </button>
        </div>

        {showNew && activeCase && (
          <NewDeadlineForm
            caseId={activeCase.id}
            onCreated={() => {
              setShowNew(false);
              void reload();
            }}
            onCancel={() => setShowNew(false)}
          />
        )}

        {deadlines.length === 0 ? (
          <p className="text-sm text-slate-400">
            本案尚无期限。点「+ 新建期限」选择法定期限并确认起算日。
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {deadlines.map((d) => (
              <DeadlineRow key={d.id} deadline={d} onConfirmed={reload} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/** 单条期限行：已确认显示到期日（dueText），未确认显示「确认锚点」入口。 */
function DeadlineRow({
  deadline,
  reason,
  onConfirmed,
}: {
  deadline: Deadline;
  reason?: string;
  onConfirmed: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const d = dueText(deadline.dueDate);

  return (
    <li className="rounded-lg bg-slate-50 px-3 py-2 text-sm">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <span className="font-medium">{deadline.name}</span>
          <span className="ml-2 text-slate-400">{deadline.caseTitle}</span>
          <div className="text-[12px] text-slate-500">
            {reason ?? (deadline.anchorDate ? `起算 ${deadline.anchorDate}` : "待确认起算日")}
          </div>
        </div>
        {/* 护栏(R6)：锚点未经律师确认 → 不展示权威到期日，仅给确认入口 */}
        {deadline.anchorConfirmed ? (
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
          <button
            onClick={() => setConfirming((v) => !v)}
            className="shrink-0 rounded-full bg-amber-100 px-2.5 py-1 text-[12px] font-medium text-amber-700 transition hover:bg-amber-200"
            title="触发日（锚点）需律师确认，确认后引擎才给出到期日"
          >
            {confirming ? "取消" : "确认锚点"}
          </button>
        )}
      </div>

      {confirming && !deadline.anchorConfirmed && (
        <ConfirmAnchorForm
          deadline={deadline}
          onDone={() => {
            setConfirming(false);
            onConfirmed();
          }}
        />
      )}
    </li>
  );
}

/** 确认起算日 → 调本地引擎重算到期日（确定性，绝不交 LLM）。 */
function ConfirmAnchorForm({
  deadline,
  onDone,
}: {
  deadline: Deadline;
  onDone: () => void;
}) {
  const [anchorDate, setAnchorDate] = useState(deadline.anchorDate || "");
  const [docType, setDocType] = useState("judgment");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const needsDocType = deadline.ruleId === "appeal_period";

  const submit = async () => {
    if (!anchorDate || saving) return;
    setSaving(true);
    setErr(null);
    try {
      await ipc.deadlineConfirmAnchor({
        id: deadline.id,
        anchorDate,
        docType: needsDocType ? docType : undefined,
      });
      onDone();
    } catch (e) {
      setErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-2 flex flex-wrap items-end gap-2 border-t border-slate-200 pt-2">
      <label className="flex flex-col gap-1 text-[11px] text-slate-500">
        起算日（锚点）
        <input
          type="date"
          value={anchorDate}
          onChange={(e) => setAnchorDate(e.target.value)}
          className={fieldCls}
        />
      </label>
      {needsDocType && (
        <label className="flex flex-col gap-1 text-[11px] text-slate-500">
          文书类型
          <select
            value={docType}
            onChange={(e) => setDocType(e.target.value)}
            className={fieldCls}
          >
            <option value="judgment">判决（15日）</option>
            <option value="ruling">裁定（10日）</option>
          </select>
        </label>
      )}
      <button
        onClick={submit}
        disabled={saving || !anchorDate}
        className="rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700 disabled:opacity-40"
      >
        {saving ? "计算中…" : "确认并算到期日"}
      </button>
      {err && <span className="text-[12px] text-red-600">{err}</span>}
    </div>
  );
}

/** 新建期限：选法定期限 + 可选当场确认起算日。 */
function NewDeadlineForm({
  caseId,
  onCreated,
  onCancel,
}: {
  caseId: string;
  onCreated: () => void;
  onCancel: () => void;
}) {
  const [ruleId, setRuleId] = useState("answer_period");
  const [anchorDate, setAnchorDate] = useState("");
  const [confirmNow, setConfirmNow] = useState(false);
  const [docType, setDocType] = useState("judgment");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const needsDocType = ruleId === "appeal_period";
  const trigger = RULE_OPTIONS.find((o) => o.value === ruleId)?.trigger;

  const submit = async () => {
    if (saving) return;
    // 当场确认起算日须填日期；否则仅建草稿（到期日待确认）。
    if (confirmNow && !anchorDate) {
      setErr("勾选了当场确认，请填写起算日");
      return;
    }
    setSaving(true);
    setErr(null);
    try {
      await ipc.deadlineCreate({
        caseId,
        ruleId,
        anchorDate: anchorDate || undefined,
        anchorConfirmed: confirmNow,
        docType: needsDocType && confirmNow ? docType : undefined,
      });
      onCreated();
    } catch (e) {
      setErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="mb-3 rounded-lg border border-slate-200 bg-slate-50 p-3">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          法定期限
          <select
            value={ruleId}
            onChange={(e) => setRuleId(e.target.value)}
            className={fieldCls}
          >
            {RULE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          {trigger && (
            <span className="text-[11px] text-slate-400">起算：{trigger}</span>
          )}
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-500">
          起算日（可空，留空＝先建草稿）
          <input
            type="date"
            value={anchorDate}
            onChange={(e) => setAnchorDate(e.target.value)}
            className={fieldCls}
          />
        </label>
        {needsDocType && (
          <label className="flex flex-col gap-1 text-[12px] text-slate-500">
            文书类型（上诉期）
            <select
              value={docType}
              onChange={(e) => setDocType(e.target.value)}
              className={fieldCls}
            >
              <option value="judgment">判决（15日）</option>
              <option value="ruling">裁定（10日）</option>
            </select>
          </label>
        )}
        <label className="flex items-center gap-2 text-[13px] text-slate-600">
          <input
            type="checkbox"
            checked={confirmNow}
            onChange={(e) => setConfirmNow(e.target.checked)}
          />
          当场确认起算日并算到期日
        </label>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <button
          onClick={submit}
          disabled={saving}
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
        {err && <span className="text-[12px] text-red-600">{err}</span>}
      </div>
    </section>
  );
}
