import { describe, it, expect } from "vitest";
import {
  buildTxt,
  buildMarkdown,
  buildHtml,
  buildDocx,
  buildXlsx,
  buildExport,
  extractDocument,
  type ExportPage,
} from "./export";
import type { PdfDocument } from "./pdf";

const dec = new TextDecoder();

const samplePages: ExportPage[] = [
  { page: 1, paragraphs: ["Hello world.", "Second paragraph."] },
  { page: 2, paragraphs: ["Page two content."] },
];

// ---------------------------------------------------------------------------
// buildTxt
// ---------------------------------------------------------------------------

describe("buildTxt", () => {
  it("joins paragraphs with double newlines, pages with triple", () => {
    const result = dec.decode(buildTxt(samplePages));
    expect(result).toContain("Hello world.\n\nSecond paragraph.");
    expect(result).toContain("\n\n\nPage two content.");
    expect(result.endsWith("\n")).toBe(true);
  });

  it("handles empty pages", () => {
    const pages: ExportPage[] = [{ page: 1, paragraphs: [] }];
    const result = dec.decode(buildTxt(pages));
    expect(result).toBe("\n");
  });
});

// ---------------------------------------------------------------------------
// buildMarkdown
// ---------------------------------------------------------------------------

describe("buildMarkdown", () => {
  it("adds page headers", () => {
    const result = dec.decode(buildMarkdown(samplePages));
    expect(result).toContain("## Page 1");
    expect(result).toContain("## Page 2");
    expect(result).toContain("Hello world.");
    expect(result.endsWith("\n")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// buildHtml
// ---------------------------------------------------------------------------

describe("buildHtml", () => {
  it("produces valid HTML with title", () => {
    const result = dec.decode(buildHtml(samplePages, "Test Doc"));
    expect(result).toContain("<title>Test Doc</title>");
    expect(result).toContain("<h2>Page 1</h2>");
    expect(result).toContain("<p>Hello world.</p>");
  });

  it("escapes HTML entities in content", () => {
    const pages: ExportPage[] = [{ page: 1, paragraphs: ["<script>alert('xss')</script>"] }];
    const result = dec.decode(buildHtml(pages, "Test"));
    expect(result).not.toContain("<script>");
    expect(result).toContain("&lt;script&gt;");
  });

  it("escapes HTML entities in title", () => {
    const result = dec.decode(buildHtml([], "A & B <C>"));
    expect(result).toContain("<title>A &amp; B &lt;C&gt;</title>");
  });
});

// ---------------------------------------------------------------------------
// buildDocx
// ---------------------------------------------------------------------------

describe("buildDocx", () => {
  it("produces a valid ZIP file (PK magic bytes)", () => {
    const result = buildDocx(samplePages);
    expect(result[0]).toBe(0x50); // P
    expect(result[1]).toBe(0x4b); // K
    expect(result[2]).toBe(0x03);
    expect(result[3]).toBe(0x04);
  });

  it("contains [Content_Types].xml entry", () => {
    const result = buildDocx(samplePages);
    const str = dec.decode(result);
    expect(str).toContain("[Content_Types].xml");
  });

  it("contains document.xml with page content", () => {
    const result = buildDocx(samplePages);
    const str = dec.decode(result);
    expect(str).toContain("Hello world.");
    expect(str).toContain("Page two content.");
  });

  it("escapes XML entities", () => {
    const pages: ExportPage[] = [{ page: 1, paragraphs: ['AT&T "quotes" <tags>'] }];
    const result = buildDocx(pages);
    const str = dec.decode(result);
    expect(str).toContain("AT&amp;T");
    expect(str).toContain("&quot;quotes&quot;");
    expect(str).toContain("&lt;tags&gt;");
  });
});

// ---------------------------------------------------------------------------
// buildXlsx
// ---------------------------------------------------------------------------

describe("buildXlsx", () => {
  it("produces a valid ZIP file", () => {
    const result = buildXlsx(samplePages);
    expect(result[0]).toBe(0x50);
    expect(result[1]).toBe(0x4b);
  });

  it("contains sheet data with header row", () => {
    const result = buildXlsx(samplePages);
    const str = dec.decode(result);
    expect(str).toContain("Page");
    expect(str).toContain("Text");
  });

  it("includes paragraph text in cells", () => {
    const result = buildXlsx(samplePages);
    const str = dec.decode(result);
    expect(str).toContain("Hello world.");
    expect(str).toContain("Page two content.");
  });

  it("handles empty page paragraphs", () => {
    const pages: ExportPage[] = [{ page: 1, paragraphs: [] }];
    const result = buildXlsx(pages);
    const str = dec.decode(result);
    // Should still have a row with page number
    expect(str).toContain("1");
  });
});

// ---------------------------------------------------------------------------
// buildExport (dispatch)
// ---------------------------------------------------------------------------

describe("buildExport", () => {
  it("dispatches to buildTxt", () => {
    const result = dec.decode(buildExport("txt", samplePages, "Test"));
    expect(result).toContain("Hello world.");
  });

  it("dispatches to buildMarkdown", () => {
    const result = dec.decode(buildExport("md", samplePages, "Test"));
    expect(result).toContain("## Page 1");
  });

  it("dispatches to buildHtml", () => {
    const result = dec.decode(buildExport("html", samplePages, "Test"));
    expect(result).toContain("<title>Test</title>");
  });

  it("dispatches to buildDocx", () => {
    const result = buildExport("docx", samplePages, "Test");
    expect(result[0]).toBe(0x50);
  });

  it("dispatches to buildXlsx", () => {
    const result = buildExport("xlsx", samplePages, "Test");
    expect(result[0]).toBe(0x50);
  });
});

// ---------------------------------------------------------------------------
// ZIP integrity (deeper check via EOCD)
// ---------------------------------------------------------------------------

describe("ZIP structure", () => {
  it("has end-of-central-directory record", () => {
    const zip = buildDocx(samplePages);
    // EOCD signature is 0x06054b50 at the end of the file
    const eocdSig =
      zip[zip.length - 22] === 0x50 &&
      zip[zip.length - 21] === 0x4b &&
      zip[zip.length - 20] === 0x05 &&
      zip[zip.length - 19] === 0x06;
    expect(eocdSig).toBe(true);
  });

  it("entry count in EOCD matches input files", () => {
    // buildDocx creates 3 entries: [Content_Types].xml, _rels/.rels, word/document.xml
    const zip = buildDocx(samplePages);
    const view = new DataView(zip.buffer, zip.byteOffset, zip.byteLength);
    const count = view.getUint16(zip.length - 22 + 8, true);
    expect(count).toBe(3);
  });
});

// ---------------------------------------------------------------------------
// extractDocument
// ---------------------------------------------------------------------------

describe("extractDocument", () => {
  it("extracts lines and paragraphs from document text items", async () => {
    const mockPage1 = {
      getTextContent: async () => ({
        items: [
          // Line 1: two words on the same line (y=700)
          { str: "Hello", transform: [12, 0, 0, 12, 50, 700], width: 30, height: 12 },
          { str: "World", transform: [12, 0, 0, 12, 90, 700], width: 35, height: 12 },
          // Line 2: normal line break (y=684, gap=16 <= 12*1.6)
          {
            str: "Second line of first paragraph.",
            transform: [12, 0, 0, 12, 50, 684],
            width: 150,
            height: 12,
          },
          // Line 3: new paragraph (y=640, gap=44 > 12*1.6)
          {
            str: "New paragraph here.",
            transform: [12, 0, 0, 12, 50, 640],
            width: 100,
            height: 12,
          },
        ],
      }),
      cleanup: () => {},
    };

    const mockDoc = {
      getPage: async (n: number) => {
        if (n === 1) return mockPage1;
        return {
          getTextContent: async () => ({ items: [] }),
          cleanup: () => {},
        };
      },
    };

    const pageList = [
      { key: "p1", docId: "d1", srcPage: 1, rotation: 0 },
      { key: "p2", docId: "d1", srcPage: 2, rotation: 0 },
      { key: "p3", docId: "unknown", srcPage: 1, rotation: 0 },
    ];

    const result = await extractDocument(pageList, (id) =>
      id === "d1" ? (mockDoc as unknown as PdfDocument) : undefined,
    );

    expect(result).toHaveLength(3);
    // Page 1 should have 2 paragraphs
    expect(result[0].page).toBe(1);
    expect(result[0].paragraphs).toEqual([
      "Hello World Second line of first paragraph.",
      "New paragraph here.",
    ]);
    // Page 2 had empty items
    expect(result[1].page).toBe(2);
    expect(result[1].paragraphs).toEqual([]);
    // Page 3 had unknown docId
    expect(result[2].page).toBe(3);
    expect(result[2].paragraphs).toEqual([]);
  });
});
