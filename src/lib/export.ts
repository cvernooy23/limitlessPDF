/**
 * Export the current document to other formats. Text is pulled from the PDF's
 * text layer via pdf.js, grouped into lines and paragraphs, then emitted as
 * plain text, Markdown, HTML, Word (.docx) or Excel (.xlsx). The Office formats
 * are real OOXML packages built with a tiny store-only ZIP writer below — no
 * external dependencies.
 *
 * Scanned/image-only PDFs have no text layer, so they export empty; that needs
 * OCR, which isn't included here.
 */
import type { PdfDocument, PageItem } from "./pdf";

export type ExportFormat = "txt" | "md" | "html" | "docx" | "xlsx";

export interface ExportPage {
  page: number; // 1-based output page number
  paragraphs: string[];
}

interface Line {
  text: string;
  y: number;
  height: number;
  gapBefore: number;
}

/** Extract paragraphs for every page in the current arrangement. */
export async function extractDocument(
  pageList: PageItem[],
  getDoc: (docId: string) => PdfDocument | undefined,
): Promise<ExportPage[]> {
  const out: ExportPage[] = [];
  for (let i = 0; i < pageList.length; i++) {
    const item = pageList[i];
    const doc = getDoc(item.docId);
    if (!doc) {
      out.push({ page: i + 1, paragraphs: [] });
      continue;
    }
    const lines = await pageLines(doc, item.srcPage);
    out.push({ page: i + 1, paragraphs: linesToParagraphs(lines) });
  }
  return out;
}

/** Build the lines of a single page from its text items. */
async function pageLines(doc: PdfDocument, pageNum: number): Promise<Line[]> {
  const page = await doc.getPage(pageNum);
  const tc = await page.getTextContent();

  const items = (tc.items as any[])
    .filter((it) => typeof it.str === "string" && it.str.length > 0)
    .map((it) => {
      const t = it.transform as number[]; // [a, b, c, d, e(x), f(y)]
      const h = Math.hypot(t[1] ?? 0, t[3] ?? 0) || it.height || 10;
      return { str: it.str as string, x: t[4] as number, y: t[5] as number, w: (it.width as number) || 0, h };
    });
  page.cleanup();

  if (items.length === 0) return [];

  // Group items into visual lines by y proximity.
  type Seg = { x: number; str: string; w: number };
  const groups: { y: number; h: number; segs: Seg[] }[] = [];
  for (const it of items.sort((a, b) => b.y - a.y || a.x - b.x)) {
    const tol = Math.max(3, it.h * 0.5);
    let g = groups.find((gr) => Math.abs(gr.y - it.y) <= tol);
    if (!g) {
      g = { y: it.y, h: it.h, segs: [] };
      groups.push(g);
    }
    g.segs.push({ x: it.x, str: it.str, w: it.w });
    g.h = Math.max(g.h, it.h);
  }

  groups.sort((a, b) => b.y - a.y); // top to bottom
  const lines: Line[] = [];
  let prevY: number | null = null;
  for (const g of groups) {
    g.segs.sort((a, b) => a.x - b.x);
    let text = "";
    let lastEnd: number | null = null;
    for (const s of g.segs) {
      if (lastEnd !== null && s.x - lastEnd > Math.max(1, g.h * 0.25)) text += " ";
      text += s.str;
      lastEnd = s.x + s.w;
    }
    text = text.replace(/\s+/g, " ").trim();
    if (!text) continue;
    const gap = prevY === null ? 0 : Math.max(0, prevY - g.y);
    lines.push({ text, y: g.y, height: g.h, gapBefore: gap });
    prevY = g.y;
  }
  return lines;
}

/** Merge wrapped lines into paragraphs; a larger-than-normal vertical gap (or a
 *  line that clearly ended a sentence) starts a new paragraph. */
function linesToParagraphs(lines: Line[]): string[] {
  if (lines.length === 0) return [];
  const heights = lines.map((l) => l.height).sort((a, b) => a - b);
  const med = heights[Math.floor(heights.length / 2)] || 12;

  const paras: string[] = [];
  let cur = "";
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    const newPara = i > 0 && l.gapBefore > med * 1.6;
    if (newPara && cur) {
      paras.push(cur.trim());
      cur = "";
    }
    cur += (cur ? " " : "") + l.text;
  }
  if (cur.trim()) paras.push(cur.trim());
  return paras;
}

// ---------------------------------------------------------------------------
// Text-based builders
// ---------------------------------------------------------------------------

export function buildTxt(pages: ExportPage[]): Uint8Array {
  const parts = pages.map((p) => p.paragraphs.join("\n\n"));
  return new TextEncoder().encode(parts.join("\n\n\n").trim() + "\n");
}

export function buildMarkdown(pages: ExportPage[]): Uint8Array {
  const blocks = pages.map((p) => {
    const body = p.paragraphs.join("\n\n");
    return `## Page ${p.page}\n\n${body}`.trimEnd();
  });
  return new TextEncoder().encode(blocks.join("\n\n") + "\n");
}

export function buildHtml(pages: ExportPage[], title: string): Uint8Array {
  const body = pages
    .map((p) => {
      const ps = p.paragraphs.map((t) => `    <p>${escapeHtml(t)}</p>`).join("\n");
      return `  <section class="page">\n    <h2>Page ${p.page}</h2>\n${ps}\n  </section>`;
    })
    .join("\n");
  const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${escapeHtml(title)}</title>
<style>
  body { font: 16px/1.55 -apple-system, Segoe UI, Roboto, sans-serif; max-width: 820px; margin: 40px auto; padding: 0 20px; color: #1a1a1a; }
  .page { margin-bottom: 40px; }
  h2 { color: #555; font-size: 14px; text-transform: uppercase; letter-spacing: .05em; border-bottom: 1px solid #eee; padding-bottom: 6px; }
  p { margin: 0 0 12px; }
</style>
</head>
<body>
${body}
</body>
</html>
`;
  return new TextEncoder().encode(html);
}

// ---------------------------------------------------------------------------
// Office Open XML (.docx / .xlsx) via a minimal store-only ZIP
// ---------------------------------------------------------------------------

export function buildDocx(pages: ExportPage[]): Uint8Array {
  const body: string[] = [];
  pages.forEach((p, idx) => {
    if (idx > 0) {
      // Page break before each page after the first.
      body.push('<w:p><w:r><w:br w:type="page"/></w:r></w:p>');
    }
    body.push(
      `<w:p><w:r><w:rPr><w:b/><w:color w:val="888888"/></w:rPr><w:t xml:space="preserve">Page ${p.page}</w:t></w:r></w:p>`,
    );
    for (const para of p.paragraphs) {
      body.push(`<w:p><w:r><w:t xml:space="preserve">${escapeXml(para)}</w:t></w:r></w:p>`);
    }
  });

  const documentXml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
${body.join("\n")}
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
</w:body>
</w:document>`;

  const contentTypes = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>`;

  const rels = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`;

  return zipStore([
    { name: "[Content_Types].xml", text: contentTypes },
    { name: "_rels/.rels", text: rels },
    { name: "word/document.xml", text: documentXml },
  ]);
}

export function buildXlsx(pages: ExportPage[]): Uint8Array {
  // One row per paragraph: column A = page number, column B = text.
  const rows: string[] = [];
  let r = 1;
  const addRow = (a: string, b: string) => {
    rows.push(
      `<row r="${r}">` +
        `<c r="A${r}" t="inlineStr"><is><t xml:space="preserve">${escapeXml(a)}</t></is></c>` +
        `<c r="B${r}" t="inlineStr"><is><t xml:space="preserve">${escapeXml(b)}</t></is></c>` +
        `</row>`,
    );
    r++;
  };
  addRow("Page", "Text");
  for (const p of pages) {
    if (p.paragraphs.length === 0) {
      addRow(String(p.page), "");
      continue;
    }
    for (const para of p.paragraphs) addRow(String(p.page), para);
  }

  const sheet = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<cols><col min="1" max="1" width="8" customWidth="1"/><col min="2" max="2" width="100" customWidth="1"/></cols>
<sheetData>
${rows.join("\n")}
</sheetData>
</worksheet>`;

  const workbook = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Text" sheetId="1" r:id="rId1"/></sheets>
</workbook>`;

  const workbookRels = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>`;

  const contentTypes = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>`;

  const rels = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>`;

  return zipStore([
    { name: "[Content_Types].xml", text: contentTypes },
    { name: "_rels/.rels", text: rels },
    { name: "xl/workbook.xml", text: workbook },
    { name: "xl/_rels/workbook.xml.rels", text: workbookRels },
    { name: "xl/worksheets/sheet1.xml", text: sheet },
  ]);
}

export function buildExport(format: ExportFormat, pages: ExportPage[], title: string): Uint8Array {
  switch (format) {
    case "txt":
      return buildTxt(pages);
    case "md":
      return buildMarkdown(pages);
    case "html":
      return buildHtml(pages, title);
    case "docx":
      return buildDocx(pages);
    case "xlsx":
      return buildXlsx(pages);
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

function escapeXml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    c === "&" ? "&amp;" : c === "<" ? "&lt;" : c === ">" ? "&gt;" : c === '"' ? "&quot;" : "&apos;",
  );
}

/** CRC-32 (IEEE) for ZIP entries. */
function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) {
    crc ^= bytes[i];
    for (let j = 0; j < 8; j++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}

interface ZipInput {
  name: string;
  text: string;
}

/** Build a ZIP archive with stored (uncompressed) entries — enough for the
 *  small XML parts that make up a .docx/.xlsx package. */
function zipStore(inputs: ZipInput[]): Uint8Array {
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;

  for (const input of inputs) {
    const nameBytes = enc.encode(input.name);
    const data = enc.encode(input.text);
    const crc = crc32(data);
    const size = data.length;

    const local = new Uint8Array(30 + nameBytes.length);
    const lv = new DataView(local.buffer);
    lv.setUint32(0, 0x04034b50, true);
    lv.setUint16(4, 20, true);
    lv.setUint16(6, 0, true);
    lv.setUint16(8, 0, true); // method 0 = store
    lv.setUint16(10, 0, true);
    lv.setUint16(12, 0x21, true); // arbitrary date
    lv.setUint32(14, crc, true);
    lv.setUint32(18, size, true);
    lv.setUint32(22, size, true);
    lv.setUint16(26, nameBytes.length, true);
    lv.setUint16(28, 0, true);
    local.set(nameBytes, 30);
    parts.push(local, data);

    const cd = new Uint8Array(46 + nameBytes.length);
    const cv = new DataView(cd.buffer);
    cv.setUint32(0, 0x02014b50, true);
    cv.setUint16(4, 20, true);
    cv.setUint16(6, 20, true);
    cv.setUint16(8, 0, true);
    cv.setUint16(10, 0, true);
    cv.setUint16(12, 0, true);
    cv.setUint16(14, 0x21, true);
    cv.setUint32(16, crc, true);
    cv.setUint32(20, size, true);
    cv.setUint32(24, size, true);
    cv.setUint16(28, nameBytes.length, true);
    cv.setUint32(42, offset, true);
    cd.set(nameBytes, 46);
    central.push(cd);

    offset += local.length + data.length;
  }

  const centralSize = central.reduce((s, c) => s + c.length, 0);
  const eocd = new Uint8Array(22);
  const ev = new DataView(eocd.buffer);
  ev.setUint32(0, 0x06054b50, true);
  ev.setUint16(8, inputs.length, true);
  ev.setUint16(10, inputs.length, true);
  ev.setUint32(12, centralSize, true);
  ev.setUint32(16, offset, true);

  const all = [...parts, ...central, eocd];
  const total = all.reduce((s, c) => s + c.length, 0);
  const result = new Uint8Array(total);
  let pos = 0;
  for (const c of all) {
    result.set(c, pos);
    pos += c.length;
  }
  return result;
}
