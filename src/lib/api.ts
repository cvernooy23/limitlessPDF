/**
 * Typed wrappers around the Rust core. Every backend capability is exposed
 * here so the rest of the UI never touches `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { SavePayload, ContentEditPayload, FormValuePayload } from "./pdf";

export interface AppInfo {
  name: string;
  version: string;
}

export interface EngineStatus {
  /** True when the PDFium native library was found and bound. */
  available: boolean;
  /** Reserved for the PDFium build/version string once available. */
  version: string | null;
  /** Human-readable status shown in the UI status badge. */
  message: string;
}

/** Detect whether we're running inside the Tauri shell (vs. a plain browser). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export async function getEngineStatus(): Promise<EngineStatus> {
  return invoke<EngineStatus>("engine_status");
}

/**
 * Open a native file picker for PDFs. Returns the chosen path, or null if the
 * user cancelled.
 */
export async function pickPdf(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  return typeof selected === "string" ? selected : null;
}

/** Read a file from disk and return its raw bytes (as an ArrayBuffer). */
export async function readPdf(path: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("read_pdf", { path });
}

/** Snapshot the opened PDF to a temp file; returns the pristine source path. */
export async function snapshotPdf(src: string): Promise<string> {
  return invoke<string>("snapshot_pdf", { srcPath: src });
}

/** Pull a friendly file name out of a full path, for the title bar. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Open a native "Save As" dialog for a PDF. Returns the path, or null. */
export async function pickSavePath(defaultName: string): Promise<string | null> {
  const selected = await save({
    defaultPath: defaultName,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  return selected ?? null;
}

/** "Save As" dialog for an exported file of the given extension. */
export async function pickExportPath(defaultName: string, ext: string): Promise<string | null> {
  const selected = await save({
    defaultPath: defaultName,
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
  });
  return selected ?? null;
}

/** Write arbitrary bytes (a built export file) to `dest`. */
export async function exportFile(dest: string, bytes: Uint8Array): Promise<void> {
  await invoke("export_file", { path: dest, bytes: Array.from(bytes) });
}

/** One output page: which source (index into `sources`), page, and rotation. */
export interface PlanEntry {
  source: number;
  srcPage: number;
  rotation: number;
}

/** Assemble `sources` per `plan`, apply content edits, add annotations, save to `dest`. */
export async function savePdf(
  sources: string[],
  dest: string,
  plan: PlanEntry[],
  annotations: SavePayload[],
  contentEdits: ContentEditPayload[],
  formValues: FormValuePayload[] = [],
  formMode: "editable" | "flatten" = "editable",
  password?: string,
  sourcePassword?: string,
): Promise<void> {
  await invoke("save_pdf", {
    sources,
    destPath: dest,
    plan,
    annotations,
    contentEdits,
    formValues,
    formMode,
    password: password ?? null,
    sourcePassword: sourcePassword ?? null,
  });
}

// ── OCR ───────────────────────────────────────────────────────────────────

export interface OcrStatus {
  available: boolean;
  version: string | null;
  languages: string[];
  message: string;
}

export interface OcrPageResult {
  page: number;
  wordCount: number;
  textPreview: string;
}

/** Check whether Tesseract is installed and which languages are available. */
export async function checkOcrAvailable(): Promise<OcrStatus> {
  return invoke<OcrStatus>("check_ocr_available");
}

/** Detect pages that are likely scanned images (little/no extractable text). */
export async function detectScannedPages(path: string, sourcePassword?: string): Promise<number[]> {
  return invoke<number[]>("detect_scanned_pages", {
    path,
    sourcePassword: sourcePassword ?? null,
  });
}

/** Run OCR on the given pages. The PDF is updated in-place with an invisible text layer. */
export async function runOcr(
  srcPath: string,
  pages: number[],
  language?: string,
  dpi?: number,
  sourcePassword?: string,
): Promise<OcrPageResult[]> {
  return invoke<OcrPageResult[]>("run_ocr", {
    srcPath,
    pages,
    language: language ?? null,
    dpi: dpi ?? null,
    sourcePassword: sourcePassword ?? null,
  });
}

// ── Signatures ───────────────────────────────────────────────────────────

export interface SignatureInfo {
  fieldName: string;
  signerName: string | null;
  signingTime: string | null;
  reason: string | null;
  location: string | null;
  subFilter: string | null;
  coversWholeDoc: boolean;
  status: string;
}

/** Extract digital signature information from a PDF. */
export async function checkSignatures(
  path: string,
  sourcePassword?: string,
): Promise<SignatureInfo[]> {
  return invoke<SignatureInfo[]>("check_signatures", {
    path,
    sourcePassword: sourcePassword ?? null,
  });
}

// ── Split PDF ──────────────────────────────────────────────────────────

/** A page range for the split command. Pages are 1-based, inclusive. */
export interface SplitRange {
  from: number;
  to: number;
  label?: string;
}

/** Open a native folder picker. Returns the chosen directory path, or null. */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: true,
  });
  return typeof selected === "string" ? selected : null;
}

/**
 * Split a PDF into separate files, one per range.
 * Returns the list of output file paths that were written.
 */
export async function splitPdf(
  source: string,
  outDir: string,
  stem: string,
  ranges: SplitRange[],
  sourcePassword?: string,
): Promise<string[]> {
  return invoke<string[]>("split_pdf", {
    source,
    outDir,
    stem,
    ranges: ranges.map((r) => ({
      from: r.from,
      to: r.to,
      label: r.label ?? "",
    })),
    sourcePassword: sourcePassword ?? null,
  });
}

// -- Insert pages/images -------------------------------------------------

/** Open a native file picker for images. Returns the chosen path, or null. */
export async function pickImage(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Images",
        extensions: ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"],
      },
    ],
  });
  return typeof selected === "string" ? selected : null;
}

/** Create a blank single-page PDF (US Letter by default). Returns the temp path. */
export async function createBlankPdf(width: number = 612, height: number = 792): Promise<string> {
  return invoke<string>("create_blank_pdf", { width, height });
}

/** Create a single-page PDF wrapping the given image. Returns the temp path. */
export async function createImagePdf(imagePath: string): Promise<string> {
  return invoke<string>("create_image_pdf", { imagePath });
}

// ── Digital Signatures ──────────────────────────────────────────────────

/** Certificate info returned from the system store. */
export interface CertInfo {
  thumbprint: string;
  subject: string;
  issuer: string;
  hasPrivateKey: boolean;
}

/** Rectangle for signature placement (PDF points, origin bottom-left). */
export interface SignRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** List certificates from the system certificate store (Windows only). */
export async function listCertificates(): Promise<CertInfo[]> {
  return invoke<CertInfo[]>("list_certificates");
}

/** Digitally sign a PDF with a certificate. */
export async function signPdf(
  srcPath: string,
  destPath: string,
  page: number,
  rect: SignRect,
  thumbprint: string,
  fieldName: string,
  reason?: string,
  location?: string,
  signerName?: string,
  sourcePassword?: string,
): Promise<void> {
  await invoke("sign_pdf", {
    srcPath,
    destPath,
    page,
    rect,
    thumbprint,
    fieldName,
    reason: reason ?? null,
    location: location ?? null,
    signerName: signerName ?? null,
    sourcePassword: sourcePassword ?? null,
  });
}

/** Create a single-page image PDF from raw PNG bytes (typed/cursive stamp). */
export async function createStampPdf(pngBytes: number[]): Promise<string> {
  return invoke<string>("create_stamp_pdf", { pngBytes });
}
