import {
  AlignmentType,
  Document,
  HeadingLevel,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
} from "docx";
import type { Case, Evidence } from "../types";
import { FORM_LABEL } from "./ui";

function cell(text: string, opts?: { bold?: boolean; width?: number }): TableCell {
  return new TableCell({
    width: opts?.width
      ? { size: opts.width, type: WidthType.PERCENTAGE }
      : undefined,
    children: [new Paragraph({ children: [new TextRun({ text, bold: opts?.bold })] })],
  });
}

/** 三性速记：真/合/关 → √ 或 ✗。 */
function natures(e: Evidence): string {
  const m = (ok: boolean) => (ok ? "√" : "✗");
  return `真${m(e.threeNatures.authenticity)} 合${m(
    e.threeNatures.legality,
  )} 关${m(e.threeNatures.relevance)}`;
}

/**
 * 生成「证据清单」.docx 并在本机触发下载。
 * 纯本地渲染：数据来自加密案卷库，文档在 webview 内构建，原件与数据不出本机。
 */
export async function exportEvidenceListDocx(
  c: Case,
  evidence: Evidence[],
): Promise<void> {
  const header = new TableRow({
    tableHeader: true,
    children: [
      cell("序号", { bold: true, width: 8 }),
      cell("证据名称", { bold: true, width: 24 }),
      cell("证据形式", { bold: true, width: 14 }),
      cell("来源", { bold: true, width: 18 }),
      cell("证明对象", { bold: true, width: 24 }),
      cell("三性", { bold: true, width: 12 }),
    ],
  });

  const rows = evidence.map(
    (e) =>
      new TableRow({
        children: [
          cell(String(e.seq)),
          cell(e.name),
          cell(FORM_LABEL[e.form] ?? e.form),
          cell(e.source || "—"),
          cell(e.proofTarget || "—"),
          cell(natures(e)),
        ],
      }),
  );

  const today = new Date().toISOString().slice(0, 10);

  const doc = new Document({
    sections: [
      {
        children: [
          new Paragraph({
            heading: HeadingLevel.HEADING_1,
            alignment: AlignmentType.CENTER,
            children: [new TextRun({ text: "证据清单", bold: true })],
          }),
          new Paragraph({
            alignment: AlignmentType.CENTER,
            children: [new TextRun({ text: c.title, size: 24 })],
          }),
          new Paragraph({
            children: [
              new TextRun({
                text: `案号：${c.caseNo || "（立案前未知）"}    案由：${
                  c.caseTypeLabel
                }    制表日期：${today}`,
                size: 18,
              }),
            ],
          }),
          new Paragraph({ text: "" }),
          new Table({
            width: { size: 100, type: WidthType.PERCENTAGE },
            rows: [header, ...rows],
          }),
          new Paragraph({ text: "" }),
          new Paragraph({
            children: [
              new TextRun({
                text: "本清单由本地生成，仅含已编目脱敏信息；原件留存于本机加密案卷库。",
                italics: true,
                size: 16,
                color: "888888",
              }),
            ],
          }),
        ],
      },
    ],
  });

  const blob = await Packer.toBlob(doc);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `证据清单-${c.title}-${today}.docx`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

/**
 * 生成文书 .docx（模板骨架 + 已填占位）。纯本地渲染：填充值在本机，文档不出网。
 * sections 为已填好的段落（{{placeholder}} 已替换；未填者保留标记供律师补全）。
 */
export async function exportDocumentDocx(opts: {
  docTitle: string;
  fileLabel: string;
  sections: { heading: string; body: string }[];
  pendingReview?: boolean;
}): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);

  const children: Paragraph[] = [
    new Paragraph({
      heading: HeadingLevel.HEADING_1,
      alignment: AlignmentType.CENTER,
      children: [new TextRun({ text: opts.docTitle, bold: true })],
    }),
    new Paragraph({ text: "" }),
  ];

  for (const s of opts.sections) {
    // "标题" 段已由上方居中大标题承载，正文不重复。
    if (s.heading === "标题") continue;
    for (const line of s.body.split("\n")) {
      children.push(new Paragraph({ children: [new TextRun({ text: line })] }));
    }
    children.push(new Paragraph({ text: "" }));
  }

  children.push(
    new Paragraph({
      children: [
        new TextRun({
          text: opts.pendingReview
            ? "本文书由本地模板骨架生成，正文措辞与诉讼请求【待执业律师/法务核定】，不得直接对外。原件数据留存本机。"
            : "本文书由本地模板生成，原件数据留存本机。",
          italics: true,
          size: 16,
          color: "888888",
        }),
      ],
    }),
  );

  const doc = new Document({ sections: [{ children }] });
  const blob = await Packer.toBlob(doc);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${opts.docTitle}-${opts.fileLabel}-${today}.docx`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
