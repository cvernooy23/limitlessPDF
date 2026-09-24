import { describe, it, expect, vi } from "vitest";

vi.mock("pdfjs-dist", () => ({
  GlobalWorkerOptions: { workerSrc: "" },
  AnnotationMode: { ENABLE_FORMS: 1 },
  TextLayer: class {},
  getDocument: vi.fn(),
}));
vi.mock("pdfjs-dist/build/pdf.worker.min.mjs?url", () => ({ default: "worker-url" }));
vi.mock("pdfjs-dist/web/pdf_viewer.css", () => ({}));

import {
  sanitizeGlyphs,
  passwordError,
  formFieldsToValues,
  formFieldsToContentEdits,
  textBoxesToContentEdits,
  annotationsToPayload,
  detectFormFields,
  type FormField,
  type PageItem,
  type TextBox,
  type PdfDocument,
} from "./pdf";
import type {
  HighlightAnnotation,
  UnderlineAnnotation,
  StrikethroughAnnotation,
  DrawAnnotation,
  NoteAnnotation,
  RectShapeAnnotation,
  CircleAnnotation,
  ArrowAnnotation,
  RedactAnnotation,
} from "./annotations";

describe("pdf.ts", () => {
  // ── sanitizeGlyphs ─────────────────────────────────────────────────────────

  describe("sanitizeGlyphs", () => {
    it("converts non-breaking spaces to standard spaces", () => {
      expect(sanitizeGlyphs("Hello\u00A0world\u00A0test")).toBe("Hello world test");
    });

    it("strips zero-width spaces, joiners, and BOM", () => {
      expect(sanitizeGlyphs("A\u200BB\u200CC\u200DD\uFEFFE")).toBe("ABCDE");
    });

    it("converts curly single quotes to straight single quotes", () => {
      expect(sanitizeGlyphs("\u2018quote\u2019")).toBe("'quote'");
    });

    it("converts curly double quotes to straight double quotes", () => {
      expect(sanitizeGlyphs("\u201Cquoted\u201D")).toBe('"quoted"');
    });

    it("converts en-dashes and em-dashes to hyphens", () => {
      expect(sanitizeGlyphs("2020\u20132026\u2014present")).toBe("2020-2026-present");
    });

    it("leaves regular ASCII text unchanged", () => {
      const normal = "The quick brown fox jumps over the lazy dog 123!@#";
      expect(sanitizeGlyphs(normal)).toBe(normal);
    });
  });

  // ── passwordError ──────────────────────────────────────────────────────────

  describe("passwordError", () => {
    it("returns null for non-objects or null error", () => {
      expect(passwordError(null)).toBeNull();
      expect(passwordError(undefined)).toBeNull();
      expect(passwordError("string error")).toBeNull();
    });

    it("returns null for non-PasswordException error", () => {
      expect(passwordError(new Error("Generic error"))).toBeNull();
      expect(passwordError({ name: "UnknownException" })).toBeNull();
    });

    it("returns incorrect: false when password is required (code 1)", () => {
      const err = { name: "PasswordException", code: 1 };
      expect(passwordError(err)).toEqual({ incorrect: false });
    });

    it("returns incorrect: true when password was wrong (code 2)", () => {
      const err = { name: "PasswordException", code: 2 };
      expect(passwordError(err)).toEqual({ incorrect: true });
    });

    it("returns incorrect: false for other codes", () => {
      const err = { name: "PasswordException", code: 0 };
      expect(passwordError(err)).toEqual({ incorrect: false });
    });
  });

  // ── formFieldsToValues ─────────────────────────────────────────────────────

  describe("formFieldsToValues", () => {
    it("preserves checkbox and radio values without glyph sanitization", () => {
      const fields: FormField[] = [
        {
          id: "p1:f1",
          pageKey: "p1",
          fieldName: "agree",
          kind: "checkbox",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "Yes\u00A0Value",
          onState: "Yes",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 10,
        },
        {
          id: "p1:f2",
          pageKey: "p1",
          fieldName: "gender",
          kind: "radio",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "Option\u20141",
          onState: "Option-1",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 10,
        },
      ];

      const res = formFieldsToValues(fields);
      expect(res).toEqual([
        { fieldName: "agree", kind: "checkbox", value: "Yes\u00A0Value", onState: "Yes" },
        { fieldName: "gender", kind: "radio", value: "Option\u20141", onState: "Option-1" },
      ]);
    });

    it("sanitizes text, dropdown, and listbox field values", () => {
      const fields: FormField[] = [
        {
          id: "p1:f1",
          pageKey: "p1",
          fieldName: "fullName",
          kind: "text",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "John\u00A0\u2018Doe\u2019",
          onState: "",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
        {
          id: "p1:f2",
          pageKey: "p1",
          fieldName: "country",
          kind: "dropdown",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "US\u2013East",
          onState: "",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
      ];

      const res = formFieldsToValues(fields);
      expect(res).toEqual([
        { fieldName: "fullName", kind: "text", value: "John 'Doe'", onState: "" },
        { fieldName: "country", kind: "dropdown", value: "US-East", onState: "" },
      ]);
    });
  });

  // ── Mock helpers for PDF doc and viewports ──────────────────────────────────

  const mockViewport = {
    convertToPdfPoint: (x: number, y: number) => [x + 10, y + 20] as [number, number],
    convertToViewportPoint: (x: number, y: number) => [x * 2, y * 2] as [number, number],
  };

  const createMockPage = (annots: unknown[] = []) => ({
    rotate: 0,
    getViewport: vi.fn().mockReturnValue(mockViewport),
    getAnnotations: vi.fn().mockResolvedValue(annots),
    cleanup: vi.fn(),
  });

  const createMockDoc = (pages: Record<number, unknown> = {}) => ({
    getPage: vi.fn().mockImplementation((num: number) => {
      const p = pages[num] || createMockPage();
      return Promise.resolve(p);
    }),
  });

  // ── formFieldsToContentEdits ───────────────────────────────────────────────

  describe("formFieldsToContentEdits", () => {
    const pageList: PageItem[] = [
      { key: "p1", docId: "d1", srcPage: 1, rotation: 0 },
      { key: "p2", docId: "d1", srcPage: 2, rotation: 90 },
    ];
    const mockDoc = createMockDoc();
    const getDoc = () => mockDoc as unknown as PdfDocument;
    const sourceOf = (id: string) => (id === "d1" ? 0 : 1);

    it("stamps 'X' for checked checkbox and skips unchecked checkbox", async () => {
      const fields: FormField[] = [
        {
          id: "p1:f1",
          pageKey: "p1",
          fieldName: "cb_checked",
          kind: "checkbox",
          x: 10,
          y: 20,
          w: 20,
          h: 20,
          value: "Yes",
          onState: "Yes",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
        {
          id: "p1:f2",
          pageKey: "p1",
          fieldName: "cb_unchecked",
          kind: "checkbox",
          x: 10,
          y: 50,
          w: 20,
          h: 20,
          value: "",
          onState: "Yes",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
      ];

      const edits = await formFieldsToContentEdits(fields, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(1);
      expect(edits[0]).toEqual({
        kind: "text",
        source: 0,
        srcPage: 1,
        // tx = 10 + 20*0.18 = 13.6, ty = 20 + 20*0.82 = 36.4
        // convertToPdfPoint adds 10, 20
        x: 13.6 + 10,
        y: 36.4 + 20,
        size: 20 * 0.82,
        color: "#111111",
        text: "X",
      });
    });

    it("stamps text value for text field and resolves dropdown label", async () => {
      const fields: FormField[] = [
        {
          id: "p1:f1",
          pageKey: "p1",
          fieldName: "text_field",
          kind: "text",
          x: 10,
          y: 20,
          w: 100,
          h: 20,
          value: "Hello\u00A0World",
          onState: "",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
        {
          id: "p1:f2",
          pageKey: "p1",
          fieldName: "dropdown_field",
          kind: "dropdown",
          x: 10,
          y: 50,
          w: 100,
          h: 20,
          value: "opt2",
          onState: "",
          options: [
            { value: "opt1", label: "Option 1" },
            { value: "opt2", label: "Option Two" },
          ],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
      ];

      const edits = await formFieldsToContentEdits(fields, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(2);
      expect(edits[0].text).toBe("Hello World");
      expect(edits[1].text).toBe("Option Two");
    });

    it("skips empty text fields and fields on missing page", async () => {
      const fields: FormField[] = [
        {
          id: "p1:f1",
          pageKey: "p1",
          fieldName: "empty_text",
          kind: "text",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "   ",
          onState: "",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
        {
          id: "unknown:f2",
          pageKey: "unknown",
          fieldName: "no_page",
          kind: "text",
          x: 0,
          y: 0,
          w: 10,
          h: 10,
          value: "val",
          onState: "",
          options: [],
          multiline: false,
          maxLen: null,
          readOnly: false,
          fontSize: 12,
        },
      ];

      const edits = await formFieldsToContentEdits(fields, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(0);
    });
  });

  // ── textBoxesToContentEdits ────────────────────────────────────────────────

  describe("textBoxesToContentEdits", () => {
    const pageList: PageItem[] = [{ key: "p1", docId: "d1", srcPage: 1, rotation: 0 }];
    const mockDoc = createMockDoc();
    const getDoc = () => mockDoc as unknown as PdfDocument;
    const sourceOf = () => 0;

    it("converts new text box into kind 'text' edit", async () => {
      const boxes: TextBox[] = [
        {
          id: "tb1",
          pageKey: "p1",
          x: 10,
          y: 20,
          size: 14,
          color: "#ff0000",
          text: "Added\u00A0text",
        },
      ];

      const edits = await textBoxesToContentEdits(boxes, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(1);
      expect(edits[0]).toEqual({
        kind: "text",
        source: 0,
        srcPage: 1,
        // x: 10 + 10, y: (20 + 14 * 0.8) + 20 = 51.2
        x: 20,
        y: 51.2,
        size: 14,
        color: "#ff0000",
        text: "Added text",
      });
    });

    it("converts in-place edit into kind 'editText' edit", async () => {
      const boxes: TextBox[] = [
        {
          id: "tb2",
          pageKey: "p1",
          x: 10,
          y: 20,
          w: 40,
          h: 10,
          size: 12,
          color: "#000",
          text: "Updated text",
          origText: "Original text",
        },
      ];

      const edits = await textBoxesToContentEdits(boxes, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(1);
      expect(edits[0]).toEqual({
        kind: "editText",
        source: 0,
        srcPage: 1,
        // px = 10 + 40/2 + 10 = 40, py = 20 + 10/2 + 20 = 45
        x: 40,
        y: 45,
        origText: "Original text",
        text: "Updated text",
      });
    });

    it("skips unchanged editText, empty text, or missing page", async () => {
      const boxes: TextBox[] = [
        {
          id: "tb_unchanged",
          pageKey: "p1",
          x: 0,
          y: 0,
          size: 12,
          color: "#000",
          text: "Same",
          origText: "Same",
        },
        {
          id: "tb_empty",
          pageKey: "p1",
          x: 0,
          y: 0,
          size: 12,
          color: "#000",
          text: "   ",
        },
        {
          id: "tb_nopage",
          pageKey: "unknown",
          x: 0,
          y: 0,
          size: 12,
          color: "#000",
          text: "Lost",
        },
      ];

      const edits = await textBoxesToContentEdits(boxes, pageList, getDoc, sourceOf);
      expect(edits).toHaveLength(0);
    });
  });

  // ── annotationsToPayload ───────────────────────────────────────────────────

  describe("annotationsToPayload", () => {
    const pageList: PageItem[] = [{ key: "p1", docId: "d1", srcPage: 1, rotation: 0 }];
    const mockDoc = createMockDoc();
    const getDoc = () => mockDoc as unknown as PdfDocument;

    it("converts all annotation types to their PDF user space payload", async () => {
      const highlight: HighlightAnnotation = {
        id: "a1",
        pageKey: "p1",
        type: "highlight",
        color: "#ffd23f",
        rect: { x: 5, y: 10, w: 20, h: 10 },
      };

      const underline: UnderlineAnnotation = {
        id: "a2",
        pageKey: "p1",
        type: "underline",
        color: "#ff7a90",
        rect: { x: 5, y: 30, w: 20, h: 5 },
      };

      const strikethrough: StrikethroughAnnotation = {
        id: "a3",
        pageKey: "p1",
        type: "strikethrough",
        color: "#6ee7b7",
        rect: { x: 5, y: 40, w: 20, h: 5 },
      };

      const draw: DrawAnnotation = {
        id: "a4",
        pageKey: "p1",
        type: "draw",
        color: "#000",
        width: 2,
        paths: [
          [
            { x: 1, y: 2 },
            { x: 3, y: 4 },
          ],
        ],
      };

      const note: NoteAnnotation = {
        id: "a5",
        pageKey: "p1",
        type: "note",
        color: "#c4a3ff",
        x: 15,
        y: 25,
        text: "Test Note",
      };

      const rect: RectShapeAnnotation = {
        id: "a6",
        pageKey: "p1",
        type: "rect",
        color: "#6ea8ff",
        rect: { x: 50, y: 50, w: 40, h: 30 },
        borderWidth: 2,
      };

      const circle: CircleAnnotation = {
        id: "a7",
        pageKey: "p1",
        type: "circle",
        color: "#6ea8ff",
        rect: { x: 100, y: 100, w: 40, h: 40 },
        borderWidth: 1.5,
      };

      const arrow: ArrowAnnotation = {
        id: "a8",
        pageKey: "p1",
        type: "arrow",
        color: "#ffd23f",
        start: { x: 0, y: 0 },
        end: { x: 10, y: 10 },
        width: 3,
      };

      const redact: RedactAnnotation = {
        id: "a9",
        pageKey: "p1",
        type: "redact",
        color: "#000",
        rect: { x: 200, y: 200, w: 50, h: 20 },
      };

      const payloads = await annotationsToPayload(
        [highlight, underline, strikethrough, draw, note, rect, circle, arrow, redact],
        pageList,
        getDoc,
      );

      expect(payloads).toHaveLength(9);

      // highlight
      expect(payloads[0]).toEqual({
        type: "highlight",
        out_index: 0,
        color: "#ffd23f",
        rect: { x0: 15, y0: 30, x1: 35, y1: 40 },
      });

      // underline
      expect(payloads[1]).toEqual({
        type: "underline",
        out_index: 0,
        color: "#ff7a90",
        rect: { x0: 15, y0: 50, x1: 35, y1: 55 },
      });

      // strikethrough
      expect(payloads[2]).toEqual({
        type: "strikethrough",
        out_index: 0,
        color: "#6ee7b7",
        rect: { x0: 15, y0: 60, x1: 35, y1: 65 },
      });

      // draw (stroke points mapped through convertToPdfPoint)
      expect(payloads[3]).toEqual({
        type: "draw",
        out_index: 0,
        color: "#000",
        width: 2,
        paths: [[11, 22, 13, 24]],
      });

      // note
      expect(payloads[4]).toEqual({
        type: "note",
        out_index: 0,
        color: "#c4a3ff",
        x: 25,
        y: 45,
        text: "Test Note",
      });

      // rect
      expect(payloads[5]).toEqual({
        type: "rect",
        out_index: 0,
        color: "#6ea8ff",
        rect: { x0: 60, y0: 70, x1: 100, y1: 100 },
        borderWidth: 2,
      });

      // circle
      expect(payloads[6]).toEqual({
        type: "circle",
        out_index: 0,
        color: "#6ea8ff",
        rect: { x0: 110, y0: 120, x1: 150, y1: 160 },
        borderWidth: 1.5,
      });

      // arrow
      expect(payloads[7]).toEqual({
        type: "arrow",
        out_index: 0,
        color: "#ffd23f",
        start: { x: 10, y: 20 },
        end: { x: 20, y: 30 },
        width: 3,
      });

      // redact (black color fixed)
      expect(payloads[8]).toEqual({
        type: "redact",
        out_index: 0,
        color: "#000000",
        rect: { x0: 210, y0: 220, x1: 260, y1: 240 },
      });
    });

    it("skips annotations with unknown pageKey", async () => {
      const annot: NoteAnnotation = {
        id: "lost",
        pageKey: "unknown",
        type: "note",
        color: "#000",
        x: 0,
        y: 0,
        text: "",
      };
      const res = await annotationsToPayload([annot], pageList, getDoc);
      expect(res).toHaveLength(0);
    });
  });

  // ── detectFormFields ───────────────────────────────────────────────────────

  describe("detectFormFields", () => {
    it("detects text, checkbox, radio, and choice form fields and skips buttons/signatures", async () => {
      const mockAnnots = [
        {
          id: "w1",
          subtype: "Widget",
          fieldType: "Tx",
          fieldName: "user_name",
          fieldValue: "Alice",
          rect: [10, 10, 60, 30],
          multiLine: true,
          maxLen: 50,
          readOnly: false,
          defaultAppearanceData: { fontSize: 14 },
        },
        {
          id: "w2",
          subtype: "Widget",
          fieldType: "Btn",
          pushButton: false,
          radioButton: false,
          fieldName: "terms",
          exportValue: "Yes",
          fieldValue: "Yes",
          rect: [10, 40, 25, 55],
        },
        {
          id: "w3",
          subtype: "Widget",
          fieldType: "Btn",
          pushButton: false,
          radioButton: true,
          fieldName: "gender",
          buttonValue: "Female",
          fieldValue: "Female",
          rect: [10, 60, 25, 75],
        },
        {
          id: "w4",
          subtype: "Widget",
          fieldType: "Btn",
          pushButton: true, // Should be skipped
          rect: [0, 0, 10, 10],
        },
        {
          id: "w5",
          subtype: "Widget",
          fieldType: "Ch",
          combo: true,
          fieldName: "country",
          fieldValue: "US",
          options: [{ exportValue: "US", displayValue: "United States" }],
          rect: [10, 80, 80, 100],
        },
        {
          id: "w6",
          subtype: "Widget",
          fieldType: "Sig", // Should be skipped
          rect: [0, 0, 10, 10],
        },
        {
          id: "w7",
          subtype: "Highlight", // Non-widget, skipped
          rect: [0, 0, 10, 10],
        },
      ];

      const page = createMockPage(mockAnnots);
      const mockDoc = createMockDoc({ 1: page });
      const pageList: PageItem[] = [{ key: "p1", docId: "d1", srcPage: 1, rotation: 0 }];

      const fields = await detectFormFields(pageList, () => mockDoc as unknown as PdfDocument);
      expect(fields).toHaveLength(4);

      // Text field
      expect(fields[0]).toMatchObject({
        id: "p1:w1",
        fieldName: "user_name",
        kind: "text",
        value: "Alice",
        multiline: true,
        maxLen: 50,
        fontSize: 14,
      });

      // Checkbox
      expect(fields[1]).toMatchObject({
        id: "p1:w2",
        fieldName: "terms",
        kind: "checkbox",
        value: "Yes",
        onState: "Yes",
      });

      // Radio
      expect(fields[2]).toMatchObject({
        id: "p1:w3",
        fieldName: "gender",
        kind: "radio",
        value: "Female",
        onState: "Female",
      });

      // Dropdown
      expect(fields[3]).toMatchObject({
        id: "p1:w5",
        fieldName: "country",
        kind: "dropdown",
        value: "US",
        options: [{ value: "US", label: "United States" }],
      });
    });
  });
});
