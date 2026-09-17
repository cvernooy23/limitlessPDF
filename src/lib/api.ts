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
