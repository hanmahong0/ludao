import { useEffect, useMemo, useState } from "react";
import { ipc } from "../ipc";
import type { Case, DocTemplate } from "../types";
import { cn } from "../lib/ui";

const today = () => new Date().toISOString().slice(0, 10);

/** 用当前取值替换段落里的 {{key}}；未填者保留标记，供律师在 Word 里补全。 */
function fillBody(body: string, values: Record<string, string>): string {
  return body.replace(/\{\{(\w+)\}\}/g, (m, key: string) =>
    values[key]?.trim() ? values[key] : m,
  );
}

export function DocumentPanel({
  activeCaseId,
  onSelectCase,
}: {
  activeCaseId: string;
  onSelectCase: (id: string) => void;
}) {
  const [cases, setCases] = useState<Case[]>([]);
  const [templates, setTemplates] = useState<DocTemplate[]>([]);
  const [templateId, setTemplateId] = useState<string>("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    void ipc.caseList().then(setCases);
    void ipc.templateList().then((t) => {
      setTemplates(t);
      if (t.length > 0) setTemplateId(t[0].configId);
    });
  }, []);

  const template = templates.find((t) => t.configId === templateId);

  // 切换模板：初始化取值（today 自动填当日，其余留空待填）。
  useEffect(() => {
    if (!template) return;
    const init: Record<string, string> = {};
    for (const p of template.placeholders) {
      init[p.key] = p.source === "today" ? today() : "";
    }
    setValues(init);
  }, [templateId]); // eslint-disable-line react-hooks/exhaustive-deps

  const filledSections = useMemo(
    () =>
      (template?.sections ?? []).map((s) => ({
        heading: s.heading,
        body: fillBody(s.body, values),
      })),
    [template, values],
  );

  const activeCase = cases.find((c) => c.id === activeCaseId);

  const onExport = async () => {
    if (!template || exporting) return;
    setExporting(true);
    try {
      const { exportDocumentDocx } = await import("../lib/exportDocx");
      await exportDocumentDocx({
        docTitle: template.docType,
        fileLabel: activeCase?.title ?? "未命名",
        sections: filledSections,
        pendingReview: template.reviewStatus === "pending_review",
      });
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="mx-auto max-w-5xl px-4 py-5 md:px-8 md:py-7">
      <header className="mb-5">
        <h1 className="text-lg font-semibold md:text-xl">文书</h1>
        <p className="text-sm text-slate-500">
          本地模板骨架 → 填充 → 导出 .docx（纯本机渲染，数据不出网）
        </p>
      </header>

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-[12px] text-slate-500">
          模板
          <select
            value={templateId}
            onChange={(e) => setTemplateId(e.target.value)}
            className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none"
          >
            {templates.map((t) => (
              <option key={t.configId} value={t.configId}>
                {t.title}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2 text-[12px] text-slate-500">
          案件
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
        </label>
        <button
          onClick={onExport}
          disabled={!template || exporting}
          className="ml-auto rounded-lg bg-slate-900 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-slate-700 disabled:opacity-40"
        >
          {exporting ? "导出中…" : "⬇ 导出 .docx"}
        </button>
      </div>

      {template?.reviewStatus === "pending_review" && (
        <div className="mb-4 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-[13px] text-amber-800">
          <span className="font-medium">待核对：</span>
          {template.note ??
            "文书骨架仅为结构占位，正文措辞与诉讼请求须由执业律师/法务核定后方可使用。"}
        </div>
      )}

      {template && (
        <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
          {/* 填充表单 */}
          <section>
            <h2 className="mb-3 text-sm font-semibold text-slate-700">填充字段</h2>
            <div className="flex flex-col gap-3">
              {template.placeholders.map((p) => (
                <label
                  key={p.key}
                  className="flex flex-col gap-1 text-[12px] text-slate-500"
                >
                  {p.label}
                  {p.key === "facts_and_reasons" ? (
                    <textarea
                      rows={4}
                      value={values[p.key] ?? ""}
                      onChange={(e) =>
                        setValues((v) => ({ ...v, [p.key]: e.target.value }))
                      }
                      placeholder={`由律师据案情填写（${p.label}）`}
                      className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none"
                    />
                  ) : (
                    <input
                      value={values[p.key] ?? ""}
                      onChange={(e) =>
                        setValues((v) => ({ ...v, [p.key]: e.target.value }))
                      }
                      placeholder={
                        p.source === "today" ? today() : `填写${p.label}`
                      }
                      className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm text-slate-800 focus:border-slate-400 focus:outline-none"
                    />
                  )}
                </label>
              ))}
            </div>
          </section>

          {/* 预览 */}
          <section>
            <h2 className="mb-3 text-sm font-semibold text-slate-700">预览</h2>
            <div className="rounded-xl border border-slate-200 bg-white p-4 text-sm leading-relaxed text-slate-700">
              <div className="mb-3 text-center text-base font-semibold">
                {template.docType}
              </div>
              {filledSections
                .filter((s) => s.heading !== "标题")
                .map((s, i) => (
                  <p
                    key={i}
                    className={cn(
                      "mb-2 whitespace-pre-wrap",
                      /\{\{\w+\}\}/.test(s.body) && "text-slate-500",
                    )}
                  >
                    {s.body}
                  </p>
                ))}
            </div>
            <p className="mt-2 text-[12px] text-slate-400">
              仍显示 <span className="font-mono">{"{{…}}"}</span> 的为未填字段，导出后可在
              Word 中补全。
            </p>
          </section>
        </div>
      )}

      {templates.length === 0 && (
        <p className="text-sm text-slate-400">暂无可用模板。</p>
      )}
    </div>
  );
}
