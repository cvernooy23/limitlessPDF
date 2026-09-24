import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  inTauri,
  baseName,
  getAppInfo,
  getEngineStatus,
  pickPdf,
  readPdf,
  snapshotPdf,
  pickSavePath,
  pickExportPath,
  exportFile,
  savePdf,
  checkOcrAvailable,
  detectScannedPages,
  runOcr,
  checkSignatures,
  pickFolder,
  splitPdf,
} from "./api";

// Mock Tauri core and plugins
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

const mockInvoke = vi.mocked(invoke);
const mockOpen = vi.mocked(open);
const mockSave = vi.mocked(save);

describe("api.ts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("inTauri", () => {
    const originalWindow = globalThis.window;

    afterEach(() => {
      // Restore window
      globalThis.window = originalWindow;
    });

    it("returns false when window does not have __TAURI_INTERNALS__", () => {
      // @ts-expect-error Mocking window without __TAURI_INTERNALS__
      globalThis.window = {};
      expect(inTauri()).toBe(false);
    });

    it("returns true when window has __TAURI_INTERNALS__", () => {
      // @ts-expect-error Mocking window with __TAURI_INTERNALS__
      globalThis.window = { __TAURI_INTERNALS__: {} };
      expect(inTauri()).toBe(true);
    });
  });

  describe("baseName", () => {
    it("extracts filename from Unix-style path", () => {
      expect(baseName("/home/user/documents/report.pdf")).toBe("report.pdf");
    });

    it("extracts filename from Windows-style path", () => {
      expect(baseName("C:\\Users\\User\\Documents\\presentation.pdf")).toBe("presentation.pdf");
    });

    it("extracts filename from mixed-slash path", () => {
      expect(baseName("C:/Users/User\\Folder/file.pdf")).toBe("file.pdf");
    });

    it("returns bare filename when no slash is present", () => {
      expect(baseName("standalone.pdf")).toBe("standalone.pdf");
    });

    it("handles trailing slash by falling back to full path", () => {
      expect(baseName("/folder/subfolder/")).toBe("/folder/subfolder/");
    });
  });

  describe("pickPdf", () => {
    it("calls open dialog with PDF filter and returns path string", async () => {
      mockOpen.mockResolvedValueOnce("/path/to/doc.pdf");
      const res = await pickPdf();
      expect(mockOpen).toHaveBeenCalledWith({
        multiple: false,
        directory: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      expect(res).toBe("/path/to/doc.pdf");
    });

    it("returns null when user cancels dialog", async () => {
      mockOpen.mockResolvedValueOnce(null);
      const res = await pickPdf();
      expect(res).toBeNull();
    });

    it("returns null if open returns an array", async () => {
      mockOpen.mockResolvedValueOnce(["/file1.pdf", "/file2.pdf"]);
      const res = await pickPdf();
      expect(res).toBeNull();
    });
  });

  describe("readPdf", () => {
    it("invokes read_pdf with path parameter", async () => {
      const buffer = new ArrayBuffer(8);
      mockInvoke.mockResolvedValueOnce(buffer);
      const res = await readPdf("/path/to/file.pdf");
      expect(mockInvoke).toHaveBeenCalledWith("read_pdf", { path: "/path/to/file.pdf" });
      expect(res).toBe(buffer);
    });
  });

  describe("snapshotPdf", () => {
    it("invokes snapshot_pdf with srcPath", async () => {
      mockInvoke.mockResolvedValueOnce("/tmp/snapshot-123.pdf");
      const res = await snapshotPdf("/original.pdf");
      expect(mockInvoke).toHaveBeenCalledWith("snapshot_pdf", { srcPath: "/original.pdf" });
      expect(res).toBe("/tmp/snapshot-123.pdf");
    });
  });

  describe("getAppInfo", () => {
    it("invokes app_info", async () => {
      const info = { name: "limitlessPDF", version: "0.0.3" };
      mockInvoke.mockResolvedValueOnce(info);
      const res = await getAppInfo();
      expect(mockInvoke).toHaveBeenCalledWith("app_info");
      expect(res).toEqual(info);
    });
  });

  describe("getEngineStatus", () => {
    it("invokes engine_status", async () => {
      const status = { available: true, version: "1.0", message: "Engine loaded" };
      mockInvoke.mockResolvedValueOnce(status);
      const res = await getEngineStatus();
      expect(mockInvoke).toHaveBeenCalledWith("engine_status");
      expect(res).toEqual(status);
    });
  });

  describe("pickSavePath", () => {
    it("calls save dialog with default path and PDF filter", async () => {
      mockSave.mockResolvedValueOnce("/saved/output.pdf");
      const res = await pickSavePath("output.pdf");
      expect(mockSave).toHaveBeenCalledWith({
        defaultPath: "output.pdf",
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      expect(res).toBe("/saved/output.pdf");
    });

    it("returns null if cancelled", async () => {
      mockSave.mockResolvedValueOnce(null);
      const res = await pickSavePath("output.pdf");
      expect(res).toBeNull();
    });
  });

  describe("pickExportPath", () => {
    it("calls save dialog with extension uppercase name and ext filter", async () => {
      mockSave.mockResolvedValueOnce("/saved/output.docx");
      const res = await pickExportPath("output.docx", "docx");
      expect(mockSave).toHaveBeenCalledWith({
        defaultPath: "output.docx",
        filters: [{ name: "DOCX", extensions: ["docx"] }],
      });
      expect(res).toBe("/saved/output.docx");
    });

    it("returns null if cancelled", async () => {
      mockSave.mockResolvedValueOnce(null);
      const res = await pickExportPath("output.txt", "txt");
      expect(res).toBeNull();
    });
  });

  describe("exportFile", () => {
    it("invokes export_file with path and bytes array", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);
      const bytes = new Uint8Array([1, 2, 3, 4]);
      await exportFile("/out.bin", bytes);
      expect(mockInvoke).toHaveBeenCalledWith("export_file", {
        path: "/out.bin",
        bytes: [1, 2, 3, 4],
      });
    });
  });

  describe("savePdf", () => {
    it("invokes save_pdf with all provided and default parameters", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);
      const plan = [{ source: 0, srcPage: 1, rotation: 0 }];
      const annotations = [
        {
          type: "highlight" as const,
          out_index: 0,
          color: "#ffff00",
          rect: { x0: 0, y0: 0, x1: 10, y1: 10 },
        },
      ];
      const contentEdits = [
        {
          kind: "text" as const,
          source: 0,
          srcPage: 1,
          x: 10,
          y: 20,
          size: 12,
          color: "#000",
          text: "Hi",
        },
      ];

      await savePdf(["/src.pdf"], "/dest.pdf", plan, annotations, contentEdits);

      expect(mockInvoke).toHaveBeenCalledWith("save_pdf", {
        sources: ["/src.pdf"],
        destPath: "/dest.pdf",
        plan,
        annotations,
        contentEdits,
        formValues: [],
        formMode: "editable",
        password: null,
        sourcePassword: null,
      });
    });

    it("passes custom formValues, formMode, password, and sourcePassword", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);
      const formValues = [
        { fieldName: "fname", kind: "text" as const, value: "Alice", onState: "" },
      ];
      await savePdf(
        ["/src.pdf"],
        "/dest.pdf",
        [],
        [],
        [],
        formValues,
        "flatten",
        "savePass",
        "srcPass",
      );

      expect(mockInvoke).toHaveBeenCalledWith("save_pdf", {
        sources: ["/src.pdf"],
        destPath: "/dest.pdf",
        plan: [],
        annotations: [],
        contentEdits: [],
        formValues,
        formMode: "flatten",
        password: "savePass",
        sourcePassword: "srcPass",
      });
    });
  });

  describe("OCR methods", () => {
    it("checkOcrAvailable invokes check_ocr_available", async () => {
      const status = { available: true, version: "5.0", languages: ["eng"], message: "ready" };
      mockInvoke.mockResolvedValueOnce(status);
      const res = await checkOcrAvailable();
      expect(mockInvoke).toHaveBeenCalledWith("check_ocr_available");
      expect(res).toEqual(status);
    });

    it("detectScannedPages invokes detect_scanned_pages with password", async () => {
      mockInvoke.mockResolvedValueOnce([1, 3]);
      const res = await detectScannedPages("/scanned.pdf", "pass123");
      expect(mockInvoke).toHaveBeenCalledWith("detect_scanned_pages", {
        path: "/scanned.pdf",
        sourcePassword: "pass123",
      });
      expect(res).toEqual([1, 3]);
    });

    it("detectScannedPages passes null password when omitted", async () => {
      mockInvoke.mockResolvedValueOnce([]);
      await detectScannedPages("/scanned.pdf");
      expect(mockInvoke).toHaveBeenCalledWith("detect_scanned_pages", {
        path: "/scanned.pdf",
        sourcePassword: null,
      });
    });

    it("runOcr invokes run_ocr with null defaults", async () => {
      mockInvoke.mockResolvedValueOnce([{ page: 1, wordCount: 42, textPreview: "Preview" }]);
      const res = await runOcr("/doc.pdf", [1]);
      expect(mockInvoke).toHaveBeenCalledWith("run_ocr", {
        srcPath: "/doc.pdf",
        pages: [1],
        language: null,
        dpi: null,
        sourcePassword: null,
      });
      expect(res).toEqual([{ page: 1, wordCount: 42, textPreview: "Preview" }]);
    });

    it("runOcr passes explicit options when provided", async () => {
      mockInvoke.mockResolvedValueOnce([]);
      await runOcr("/doc.pdf", [1, 2], "deu", 150, "pw");
      expect(mockInvoke).toHaveBeenCalledWith("run_ocr", {
        srcPath: "/doc.pdf",
        pages: [1, 2],
        language: "deu",
        dpi: 150,
        sourcePassword: "pw",
      });
    });
  });

  describe("checkSignatures", () => {
    it("invokes check_signatures with path and sourcePassword", async () => {
      const sigs = [
        {
          fieldName: "Signature1",
          signerName: "John Doe",
          signingTime: "2026-01-01",
          reason: "Approval",
          location: "Earth",
          subFilter: "adbe.pkcs7.detached",
          coversWholeDoc: true,
          status: "Valid",
        },
      ];
      mockInvoke.mockResolvedValueOnce(sigs);
      const res = await checkSignatures("/doc.pdf", "secret");
      expect(mockInvoke).toHaveBeenCalledWith("check_signatures", {
        path: "/doc.pdf",
        sourcePassword: "secret",
      });
      expect(res).toEqual(sigs);
    });

    it("passes null when sourcePassword is omitted", async () => {
      mockInvoke.mockResolvedValueOnce([]);
      await checkSignatures("/doc.pdf");
      expect(mockInvoke).toHaveBeenCalledWith("check_signatures", {
        path: "/doc.pdf",
        sourcePassword: null,
      });
    });
  });

  describe("pickFolder", () => {
    it("calls open with directory: true", async () => {
      mockOpen.mockResolvedValueOnce("/selected/folder");
      const res = await pickFolder();
      expect(mockOpen).toHaveBeenCalledWith({
        multiple: false,
        directory: true,
      });
      expect(res).toBe("/selected/folder");
    });

    it("returns null if cancelled", async () => {
      mockOpen.mockResolvedValueOnce(null);
      const res = await pickFolder();
      expect(res).toBeNull();
    });
  });

  describe("splitPdf", () => {
    it("invokes split_pdf with mapped ranges and default label", async () => {
      mockInvoke.mockResolvedValueOnce(["/out/doc_p1.pdf", "/out/doc_cover.pdf"]);
      const ranges = [
        { from: 1, to: 1 },
        { from: 2, to: 4, label: "cover" },
      ];
      const res = await splitPdf("/doc.pdf", "/out", "doc", ranges, "pw");
      expect(mockInvoke).toHaveBeenCalledWith("split_pdf", {
        source: "/doc.pdf",
        outDir: "/out",
        stem: "doc",
        ranges: [
          { from: 1, to: 1, label: "" },
          { from: 2, to: 4, label: "cover" },
        ],
        sourcePassword: "pw",
      });
      expect(res).toEqual(["/out/doc_p1.pdf", "/out/doc_cover.pdf"]);
    });
  });
});
