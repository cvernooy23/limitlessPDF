/**
 * Thin wrapper around pdf.js (Mozilla). Handles worker setup and document
 * loading; the components handle rendering. This is the display engine — the
 * Rust/PDFium side remains the authority for editing/export .
 */
import * as pdfjsLib from "pdfjs-dist";
import { TextLayer } from "pdfjs-dist";
// Vite resolves this to a hashed URL for the bundled worker file.
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
// pdf.js's official styles for the selectable text layer.
import "pdfjs-dist/web/pdf_viewer.css";

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

export type PdfDocument = pdfjsLib.PDFDocumentProxy;
export type PdfPage = pdfjsLib.PDFPageProxy;

/** Annotation payload in PDF user space (origin bottom-left), addressed by
 *  output-page index, sent to Rust. */
export type SavePayload =
  | {
      type: "highlight";
      out_index: number;
      color: string;
      rect: { x0: number; y0: number; x1: number; y1: number };
    }
  | { type: "draw"; out_index: number; color: string; width: number; paths: number[][] }
  | { type: "note"; out_index: number; color: string; x: number; y: number; text: string }
  | {
      type: "underline";
      out_index: number;
      color: string;
      rect: { x0: number; y0: number; x1: number; y1: number };
    }
  | {
      type: "strikethrough";
      out_index: number;
      color: string;
      rect: { x0: number; y0: number; x1: number; y1: number };
    };

export interface LoadedPdf {
  doc: PdfDocument;
  numPages: number;
}

/**
 * Load a PDF from raw bytes, optionally with a password for encrypted files.
 * On an encrypted file with a missing/incorrect password, the returned promise
 * rejects with a pdf.js PasswordException — use `passwordError()` to detect it.
 */
export async function loadPdf(
  data: ArrayBuffer | Uint8Array,
  password?: string,
): Promise<LoadedPdf> {
  // pdf.js takes ownership of the buffer, so hand it a fresh Uint8Array.
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  const task = pdfjsLib.getDocument({ data: bytes, password });
  const doc = await task.promise;
  return { doc, numPages: doc.numPages };
}

/** If `err` is a pdf.js password error, return whether a password was supplied
 *  but wrong (`incorrect: true`) vs simply missing; otherwise null. */
export function passwordError(err: unknown): { incorrect: boolean } | null {
  const e = err as { name?: string; code?: number } | null;
  if (e && e.name === "PasswordException") {
    // PasswordResponses: NEED_PASSWORD = 1, INCORRECT_PASSWORD = 2
    return { incorrect: e.code === 2 };
  }
  return null;
}

/**
 * Render a single page onto a canvas at the given scale and return the canvas.
 * Used for both the main view and thumbnails.
 */
export async function renderPageToCanvas(
  doc: PdfDocument,
  pageNumber: number,
  scale: number,
  canvas: HTMLCanvasElement,
  rotation = 0,
): Promise<{ width: number; height: number }> {
  const page = await doc.getPage(pageNumber);
  // Account for high-DPI screens so text stays crisp.
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const rot = (((page.rotate + rotation) % 360) + 360) % 360;
  const viewport = page.getViewport({ scale: scale * dpr, rotation: rot });
  const cssViewport = page.getViewport({ scale, rotation: rot });

  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas 2D context unavailable");

  canvas.width = Math.floor(viewport.width);
  canvas.height = Math.floor(viewport.height);
  canvas.style.width = `${Math.floor(cssViewport.width)}px`;
  canvas.style.height = `${Math.floor(cssViewport.height)}px`;

  // ENABLE_FORMS keeps non-form annotations on the canvas but skips interactive
  // form widgets — we render those ourselves as an HTML overlay (FormLayer), so
  // painting their baked appearance here too would double-render filled fields.
  await page.render({
    canvasContext: ctx,
    canvas,
    viewport,
    annotationMode: pdfjsLib.AnnotationMode.ENABLE_FORMS,
  }).promise;
  page.cleanup();

  return { width: cssViewport.width, height: cssViewport.height };
}

/**
 * Svelte action: render a PDF page into the bound <canvas>, re-rendering when
 * the doc/page/scale change. A token guards against overlapping renders.
 */
export interface PageItem {
  key: string;
  docId: string;
  srcPage: number;
  rotation: number;
}

/** A text content edit. Without `origText` it's a new text object (Add Text);
 *  with `origText` it's an in-place edit of an existing run (the original is
 *  covered in the preview and the run is rewritten via PDFium on save). Coords
 *  in scale-1 viewport space, top-left origin. */
export interface TextBox {
  id: string;
  pageKey: string;
  x: number;
  y: number;
  size: number;
  color: string;
  text: string;
  origText?: string;
  w?: number;
  h?: number;
}

/** Content-edit payload sent to Rust/PDFium, addressed by source + page. */
export type ContentEditPayload =
  | {
      kind: "text";
      source: number;
      srcPage: number;
      x: number;
      y: number;
      size: number;
      color: string;
      text: string;
    }
  | {
      kind: "editText";
      source: number;
      srcPage: number;
      x: number;
      y: number;
      origText: string;
      text: string;
    };

/** An interactive AcroForm field discovered in the document. Geometry is in
 *  scale-1 viewport space (top-left origin), like annotations/text boxes, so the
 *  form layer can position inputs by multiplying by the current scale. */
export interface FormField {
  id: string; // unique per widget: `${pageKey}:${annotationId}`
  pageKey: string;
  fieldName: string; // fully-qualified PDF field name (/T chain)
  kind: "text" | "checkbox" | "radio" | "dropdown" | "listbox";
  x: number;
  y: number;
  w: number;
  h: number;
  /** Text value, selected option value, or the on-state name when checked. */
  value: string;
  /** Export ("on") value for a checkbox/radio widget, e.g. "Yes". */
  onState: string;
  options: { value: string; label: string }[];
  multiline: boolean;
  maxLen: number | null;
  readOnly: boolean;
  fontSize: number;
}

/** Read interactive form widgets from the document via pdf.js, one entry per
 *  widget, addressed to the matching output page. Push buttons and signature
 *  fields are skipped (nothing to fill). */
export async function detectFormFields(
  pageList: PageItem[],
  getDoc: (docId: string) => PdfDocument | undefined,
): Promise<FormField[]> {
  const out: FormField[] = [];
  for (const item of pageList) {
    const doc = getDoc(item.docId);
    if (!doc) continue;
    const page = await doc.getPage(item.srcPage);
    let annots: any[];
    try {
      annots = await page.getAnnotations();
    } catch {
      continue;
    }
    const rot = (((page.rotate + item.rotation) % 360) + 360) % 360;
    const vp = page.getViewport({ scale: 1, rotation: rot });
    for (const a of annots) {
      if (a.subtype !== "Widget") continue;
      const ft = a.fieldType as string;
      let kind: FormField["kind"] | null = null;
      let onState = "";
      let options: { value: string; label: string }[] = [];
      if (ft === "Tx") {
        kind = "text";
      } else if (ft === "Btn") {
        if (a.pushButton) continue;
        if (a.radioButton) {
          kind = "radio";
          onState = String(a.buttonValue ?? "");
        } else {
          kind = "checkbox";
          onState = String(a.exportValue ?? a.buttonValue ?? "Yes");
        }
      } else if (ft === "Ch") {
        kind = a.combo ? "dropdown" : "listbox";
        options = (a.options ?? []).map((o: any) => ({
          value: String(o.exportValue ?? o.displayValue ?? ""),
          label: String(o.displayValue ?? o.exportValue ?? ""),
        }));
      } else {
        continue; // Sig or unknown
      }

      // PDF rect -> viewport rect. In pdfjs v6 convertToViewportRectangle
      // was removed; use convertToViewportPoint for each corner instead.
      const [vx1, vy1] = vp.convertToViewportPoint(a.rect[0], a.rect[1]);
      const [vx2, vy2] = vp.convertToViewportPoint(a.rect[2], a.rect[3]);
      const x = Math.min(vx1, vx2);
      const y = Math.min(vy1, vy2);
      const w = Math.abs(vx2 - vx1);
      const h = Math.abs(vy2 - vy1);

      const fv = Array.isArray(a.fieldValue) ? (a.fieldValue[0] ?? "") : (a.fieldValue ?? "");
      let value = "";
      if (kind === "checkbox" || kind === "radio") {
        value = String(fv) === onState && onState !== "" ? onState : "";
      } else {
        value = String(fv ?? "");
      }

      out.push({
        id: `${item.key}:${a.id}`,
        pageKey: item.key,
        fieldName: String(a.fieldName ?? a.id),
        kind,
        x,
        y,
        w,
        h,
        value,
        onState,
        options,
        multiline: !!a.multiLine,
        maxLen: typeof a.maxLen === "number" && a.maxLen > 0 ? a.maxLen : null,
        readOnly: !!a.readOnly,
        fontSize: a.defaultAppearanceData?.fontSize || Math.max(8, h * 0.62),
      });
    }
  }
  return out;
}

/** A filled field value sent to Rust for the keep-editable save path. */
export interface FormValuePayload {
  fieldName: string;
  kind: FormField["kind"];
  value: string;
  onState: string;
}

/** Values to write back into the live AcroForm (editable save). Unfilled fields
 *  are still sent so an existing value can be cleared. */
export function formFieldsToValues(fields: FormField[]): FormValuePayload[] {
  return fields.map((f) => ({
    fieldName: f.fieldName,
    kind: f.kind,
    value: f.kind === "checkbox" || f.kind === "radio" ? f.value : sanitizeGlyphs(f.value),
    onState: f.onState,
  }));
}

/** Convert filled fields into PDFium text stamps (PDF user space) for the
 *  flatten save path — reusing the same content-edit channel as text boxes.
 *  Checked boxes/radios stamp an "X"; choice fields stamp the selected label. */
export async function formFieldsToContentEdits(
  fields: FormField[],
  pageList: PageItem[],
  getDoc: (docId: string) => PdfDocument | undefined,
  sourceOf: (docId: string) => number,
): Promise<ContentEditPayload[]> {
  const itemByKey = new Map(pageList.map((p) => [p.key, p]));
  const vpCache = new Map<string, pdfjsLib.PageViewport>();
  const out: ContentEditPayload[] = [];
  for (const f of fields) {
    const item = itemByKey.get(f.pageKey);
    if (!item) continue;
    let vp = vpCache.get(item.key);
    if (!vp) {
      const doc = getDoc(item.docId);
      if (!doc) continue;
      const p = await doc.getPage(item.srcPage);
      const rot = (((p.rotate + item.rotation) % 360) + 360) % 360;
      vp = p.getViewport({ scale: 1, rotation: rot });
      vpCache.set(item.key, vp);
    }

    let text = "";
    let size = f.fontSize;
    let tx = f.x + 2;
    let ty = f.y + f.h * 0.72;
    if (f.kind === "checkbox" || f.kind === "radio") {
      if (!f.value) continue; // unchecked — nothing to stamp
      text = "X";
      size = f.h * 0.82;
      tx = f.x + f.w * 0.18;
      ty = f.y + f.h * 0.82;
    } else {
      const raw =
        f.kind === "dropdown" || f.kind === "listbox"
          ? (f.options.find((o) => o.value === f.value)?.label ?? f.value)
          : f.value;
      text = sanitizeGlyphs(raw).trim();
      if (!text) continue;
      size = Math.min(f.fontSize || 12, f.h * 0.72);
    }

    const [px, py] = vp.convertToPdfPoint(tx, ty);
    out.push({
      kind: "text",
      source: sourceOf(item.docId),
      srcPage: item.srcPage,
      x: px,
      y: py,
      size,
      color: "#111111",
      text,
    });
  }
  return out;
}

/** Map characters that have no glyph in PDFium's base-14 Helvetica (and would
 *  render as a .notdef box — the little square) onto WinAnsi equivalents. The
 *  usual culprit is contenteditable turning a typed space into a non-breaking
 *  space (U+00A0). */
export function sanitizeGlyphs(s: string): string {
  return s
    .replace(/\u00A0/g, " ") // non-breaking space -> normal space
    .replace(/[\u200B-\u200D\uFEFF]/g, "") // zero-width spaces / BOM
    .replace(/[\u2018\u2019]/g, "'") // curly single quotes
    .replace(/[\u201C\u201D]/g, '"') // curly double quotes
    .replace(/[\u2013\u2014]/g, "-"); // en / em dash
}

/** Convert pending text boxes into PDFium content edits (PDF user space). */
export async function textBoxesToContentEdits(
  boxes: TextBox[],
  pageList: PageItem[],
  getDoc: (docId: string) => PdfDocument | undefined,
  sourceOf: (docId: string) => number,
): Promise<ContentEditPayload[]> {
  const itemByKey = new Map(pageList.map((p) => [p.key, p]));
  const vpCache = new Map<string, pdfjsLib.PageViewport>();
  const out: ContentEditPayload[] = [];
  for (const b of boxes) {
    if (!b.text.trim()) continue;
    const item = itemByKey.get(b.pageKey);
    if (!item) continue;
    let vp = vpCache.get(item.key);
    if (!vp) {
      const doc = getDoc(item.docId);
      if (!doc) continue;
      const p = await doc.getPage(item.srcPage);
      const rot = (((p.rotate + item.rotation) % 360) + 360) % 360;
      vp = p.getViewport({ scale: 1, rotation: rot });
      vpCache.set(item.key, vp);
    }
    if (b.origText !== undefined) {
      // Edit of an existing run — skip if unchanged. Point at the run's center
      // so Rust can locate the underlying text object.
      const text = sanitizeGlyphs(b.text);
      if (text === b.origText) continue;
      const [px, py] = vp.convertToPdfPoint(b.x + (b.w ?? 0) / 2, b.y + (b.h ?? b.size) / 2);
      out.push({
        kind: "editText",
        source: sourceOf(item.docId),
        srcPage: item.srcPage,
        x: px,
        y: py,
        origText: b.origText,
        text,
      });
    } else {
      // New text — place the baseline near the box's text baseline.
      const [px, py] = vp.convertToPdfPoint(b.x, b.y + b.size * 0.8);
      out.push({
        kind: "text",
        source: sourceOf(item.docId),
        srcPage: item.srcPage,
        x: px,
        y: py,
        size: b.size,
        color: b.color,
        text: sanitizeGlyphs(b.text),
      });
    }
  }
  return out;
}

/**
 * Convert in-app annotations (stored in scale-1 viewport space, top-left
 * origin) into PDF user space via pdf.js's viewport transform, addressing each
 * by its output-page index. Handles the y-flip, page offset, and rotation, and
 * resolves each annotation's source document by its page key.
 */
export async function annotationsToPayload(
  annotations: import("./annotations").Annotation[],
  pageList: PageItem[],
  getDoc: (docId: string) => PdfDocument | undefined,
): Promise<SavePayload[]> {
  const indexByKey = new Map(pageList.map((p, i) => [p.key, i]));
  const viewports = new Map<string, pdfjsLib.PageViewport>();
  const vpFor = async (item: PageItem) => {
    let v = viewports.get(item.key);
    if (!v) {
      const doc = getDoc(item.docId);
      if (!doc) return null;
      const p = await doc.getPage(item.srcPage);
      const rot = (((p.rotate + item.rotation) % 360) + 360) % 360;
      v = p.getViewport({ scale: 1, rotation: rot });
      viewports.set(item.key, v);
    }
    return v;
  };

  const out: SavePayload[] = [];
  for (const a of annotations) {
    const idx = indexByKey.get(a.pageKey);
    if (idx === undefined) continue;
    const item = pageList[idx];
    const v = await vpFor(item);
    if (!v) continue;
    if (a.type === "highlight") {
      const [x0, y0] = v.convertToPdfPoint(a.rect.x, a.rect.y);
      const [x1, y1] = v.convertToPdfPoint(a.rect.x + a.rect.w, a.rect.y + a.rect.h);
      out.push({ type: "highlight", out_index: idx, color: a.color, rect: { x0, y0, x1, y1 } });
    } else if (a.type === "draw") {
      const paths = a.paths.map((stroke) =>
        stroke.flatMap((p) => v!.convertToPdfPoint(p.x, p.y) as number[]),
      );
      out.push({ type: "draw", out_index: idx, color: a.color, width: a.width, paths });
    } else if (a.type === "underline" || a.type === "strikethrough") {
      const [x0, y0] = v.convertToPdfPoint(a.rect.x, a.rect.y);
      const [x1, y1] = v.convertToPdfPoint(a.rect.x + a.rect.w, a.rect.y + a.rect.h);
      out.push({ type: a.type, out_index: idx, color: a.color, rect: { x0, y0, x1, y1 } });
    } else {
      const [x, y] = v.convertToPdfPoint(a.x, a.y);
      out.push({ type: "note", out_index: idx, color: a.color, x, y, text: a.text });
    }
  }
  return out;
}

interface RenderParams {
  doc: PdfDocument;
  page: number;
  scale: number;
  rotation?: number;
}

export function pageRender(canvas: HTMLCanvasElement, params: RenderParams) {
  let token = 0;

  async function run(p: RenderParams) {
    const current = ++token;
    try {
      await renderPageToCanvas(p.doc, p.page, p.scale, canvas, p.rotation ?? 0);
    } catch (err) {
      if (current === token) console.error(`Failed to render page ${p.page}`, err);
    }
  }

  run(params);

  return {
    update(p: RenderParams) {
      run(p);
    },
    destroy() {
      token++; // invalidate any in-flight render
    },
  };
}

export interface TextLayerParams {
  doc: PdfDocument;
  page: number;
  scale: number;
  rotation?: number;
  /** Called once the selectable text divs for this page are in the DOM. */
  onReady?: (page: number, divs: HTMLElement[]) => void;
}

/**
 * Svelte action: build pdf.js's selectable text layer over a page. Powers text
 * selection and find-in-document highlighting. Re-renders on scale change.
 */
export function textLayerRender(container: HTMLElement, params: TextLayerParams) {
  let token = 0;
  let current: { cancel?: () => void } | null = null;

  async function run(p: TextLayerParams) {
    const mine = ++token;
    try {
      current?.cancel?.();
    } catch {
      /* ignore */
    }
    container.replaceChildren();
    // pdf.js positions spans relative to this CSS variable.
    container.style.setProperty("--scale-factor", String(p.scale));

    try {
      const page = await p.doc.getPage(p.page);
      if (mine !== token) return;
      const textContentSource = await page.getTextContent();
      if (mine !== token) return;

      const rot = (((page.rotate + (p.rotation ?? 0)) % 360) + 360) % 360;
      const viewport = page.getViewport({ scale: p.scale, rotation: rot });
      const layer = new TextLayer({ textContentSource, container, viewport });
      current = layer;
      await layer.render();
      if (mine !== token) return;
      p.onReady?.(p.page, (layer.textDivs ?? []) as HTMLElement[]);
    } catch (err) {
      if (mine === token) console.error(`Text layer failed on page ${p.page}`, err);
    }
  }

  run(params);

  return {
    update(p: TextLayerParams) {
      run(p);
    },
    destroy() {
      token++;
      try {
        current?.cancel?.();
      } catch {
        /* ignore */
      }
    },
  };
}
