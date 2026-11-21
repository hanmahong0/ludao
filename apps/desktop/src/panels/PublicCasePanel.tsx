import { useState } from "react";
import { ipc } from "../ipc";
import type { PublicCaseHit } from "../types";
import { cn } from "../lib/ui";

export function PublicCasePanel() {
  const [query, setQuery] = useState("借贷");
  const [hits, setHits] = useState<PublicCaseHit[]>([]);
  const [compare, setCompare] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    setLoading(true);
    setError(null);
    try {
      const r = await ipc.searchPublicCases(query, 8);
      setHits(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const runCompare = async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await ipc.comparePublicCases("pub-p-v1", "pub-q-v1");
      setCompare(text);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4 p-4">
      <div className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
        <h2 className="text-base font-semibold">类案检索（合成 fixture）</h2>
        <p className="mt-1 text-xs text-slate-500">
          关键词 FTS 基线；来源许可见 licenseStatus。不采集外部法院站点。
        </p>
        <div className="mt-3 flex gap-2">
          <input
            className="flex-1 rounded-lg border border-slate-200 px-3 py-2 text-sm"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            type="button"
            className={cn(
              "rounded-lg bg-slate-900 px-4 py-2 text-sm text-white",
              loading && "opacity-60",
            )}
            disabled={loading}
            onClick={() => void search()}
          >
            检索
          </button>
          <button
            type="button"
            className="rounded-lg border border-slate-300 px-4 py-2 text-sm"
            disabled={loading}
            onClick={() => void runCompare()}
          >
            对比 P/Q
          </button>
        </div>
        {error && <p className="mt-2 text-sm text-red-600">{error}</p>}
      </div>
      <div className="grid flex-1 gap-3 overflow-auto md:grid-cols-2">
        {hits.map((h) => (
          <article key={h.passageId} className="rounded-xl border border-slate-200 bg-white p-3 text-sm">
            <h3 className="font-medium">{h.title}</h3>
            <div className="mt-1 text-xs text-slate-500">
              {h.caseVersion} · {h.licenseStatus}
              {h.locator
                ? ` · ${h.locator.startsWith("evidence_seq:") ? `证据序号 ${h.locator.slice("evidence_seq:".length)}` : h.locator.startsWith("passage:") ? `段落 ${h.locator.slice("passage:".length)}` : h.locator}`
                : ""}
            </div>
            <p className="mt-2 text-slate-700">{h.excerpt}</p>
            {h.similarityNote && (
              <p className="mt-2 text-xs text-amber-700">标签：{h.similarityNote}</p>
            )}
          </article>
        ))}
      </div>
      {compare && (
        <pre className="max-h-48 overflow-auto rounded-xl border bg-slate-50 p-3 text-xs whitespace-pre-wrap">
          {compare}
        </pre>
      )}
    </div>
  );
}