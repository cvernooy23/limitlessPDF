<script lang="ts">
  import { onMount } from "svelte";
  import TitleBar from "./lib/TitleBar.svelte";
  import Toolbar from "./lib/Toolbar.svelte";
  import ThumbRail from "./lib/ThumbRail.svelte";
  import EmptyState from "./lib/EmptyState.svelte";
  import PdfViewer from "./lib/PdfViewer.svelte";
  import CommentsPanel from "./lib/CommentsPanel.svelte";
  import OcrPanel from "./lib/OcrPanel.svelte";
  import SignaturePanel from "./lib/SignaturePanel.svelte";
  import {
    ANNOTATION_COLORS,
    type Annotation,
    type NoteAnnotation,
    type Tool,
  } from "./lib/annotations";
  import {
    loadPdf,
    passwordError,
    annotationsToPayload,
    textBoxesToContentEdits,
    detectFormFields,
    formFieldsToValues,
    formFieldsToContentEdits,
    type PdfDocument,
    type PageItem,
    type TextBox,
    type FormField,
  } from "./lib/pdf";
  import {
    getAppInfo,
    getEngineStatus,
    pickPdf,
    readPdf,
    snapshotPdf,
    pickSavePath,
    savePdf,
    pickExportPath,
    exportFile,
    baseName,
    inTauri,
    type EngineStatus,
  } from "./lib/api";
  import { extractDocument, buildExport, type ExportFormat } from "./lib/export";

  let version = $state("0.1.0");
  let engine = $state<EngineStatus | null>(null);
  let bridgeError = $state<string | null>(null);

  // Document state
  let doc = $state<PdfDocument | null>(null);
  let fileName = $state<string | null>(null);
  let filePath = $state<string | null>(null);
  // Multi-document model. Each loaded PDF gets a docId; pages reference one.
  const docs = new Map<string, PdfDocument>(); // docId -> pdf.js document
  const srcPaths = new Map<string, string>(); // docId -> pristine snapshot path
  let idCounter = 0;
  const newDocId = () => `d${idCounter++}`;
  const newPageKey = () => `k${idCounter++}`;
  const getDoc = (docId: string) => docs.get(docId);

  // Page model: the current arrangement of pages (order, rotation, deletions).
  let pageList = $state<PageItem[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let scale = $state(1.2);

  // Save state
  let saving = $state(false);
  let saveMsg = $state<string | null>(null);
  let dirty = $state(false); // annotations changed since last save/open

  let mainEl = $state<HTMLElement | null>(null);

  // Find-in-document state
  let searchOpen = $state(false);
  let query = $state("");
  let matchCount = $state(0);
  let activeIndex = $state(0);
  let searchInput = $state<HTMLInputElement | null>(null);

  // Annotation state
  let tool = $state<Tool>("none");
  let color = $state(ANNOTATION_COLORS[0]);
  let inkWidth = $state(2.5);
  let annotations = $state<Annotation[]>([]);
  let selectedId = $state<string | null>(null);
  let commentsOpen = $state(false);
  let ocrOpen = $state(false);
  let sigOpen = $state(false);

  // Content edits (PDFium): new text objects added to pages.
  let textBoxes = $state<TextBox[]>([]);
  let fontSize = $state(16);

  // Interactive form fields (AcroForm). Geometry is re-detected when the page
  // arrangement changes; entered values persist in `formValues` (keyed by the
  // stable widget id) and are overlaid on top.
  let formFields = $state<FormField[]>([]);
  const formValues = new Map<string, string>();
  let formSaveMode = $state<"editable" | "flatten">("editable");
  const hasForm = $derived(formFields.length > 0);

  // Encryption (AES-256). When enabled, the saved file requires this password
  // to open. The password is never persisted — it's only passed to the save call.
  let encryptOn = $state(false);
  let password = $state("");
  let showPassword = $state(false);

  // Password prompt for opening encrypted PDFs. `openedPassword` is the password
  // the current document was opened with (if any) — passed back to the save
  // pipeline so the encrypted source can be decrypted before re-assembly.
  let pwPrompt = $state<{ incorrect: boolean } | null>(null);
  let pwEntry = $state("");
  let pwResolve: ((value: string | null) => void) | null = null;
  let openedPassword: string | null = null;

  function promptPassword(incorrect: boolean): Promise<string | null> {
    pwEntry = "";
    pwPrompt = { incorrect };
    setTimeout(() => document.getElementById("pw-entry")?.focus(), 30);
    return new Promise((resolve) => {
      pwResolve = resolve;
    });
  }

  function resolvePassword(value: string | null) {
    pwPrompt = null;
    const r = pwResolve;
    pwResolve = null;
    r?.(value);
  }

  // Export to other formats.
  let exportOpen = $state(false);
  let exporting = $state(false);
  const EXPORT_FORMATS: { id: ExportFormat; label: string }[] = [
    { id: "txt", label: "Plain text (.txt)" },
    { id: "md", label: "Markdown (.md)" },
    { id: "html", label: "HTML (.html)" },
    { id: "docx", label: "Word (.docx)" },
    { id: "xlsx", label: "Excel (.xlsx)" },
  ];

  async function doExport(format: ExportFormat) {
    if (!doc || exporting) return;
    exportOpen = false;
    exporting = true;
    saveMsg = null;
    try {
      const stem = (fileName ?? "document").replace(/\.pdf$/i, "");
      const dest = await pickExportPath(`${stem}.${format}`, format);
      if (!dest) return;
      const pages = await extractDocument(pageList, getDoc);
      const empty = pages.every((p) => p.paragraphs.length === 0);
      const bytes = buildExport(format, pages, stem);
      await exportFile(dest, bytes);
      saveMsg = empty
        ? `Exported ${baseName(dest)} — but no text was found (scanned PDF?)`
        : `Exported ${baseName(dest)}`;
    } catch (e) {
      saveMsg = `Export failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      exporting = false;
    }
  }

  $effect(() => {
    const list = pageList;
    if (!list.length) {
      formFields = [];
      return;
    }
    let cancelled = false;
    detectFormFields(list, getDoc)
      .then((fields) => {
        if (cancelled) return;
        for (const f of fields) if (!formValues.has(f.id)) formValues.set(f.id, f.value);
        formFields = fields.map((f) => ({ ...f, value: formValues.get(f.id) ?? f.value }));
      })
      .catch(() => {
        /* a document without forms is fine */
      });
    return () => {
      cancelled = true;
    };
  });

  // Update a field value, enforcing single-selection within a radio group.
  function onFormChange(field: FormField, value: string) {
    const updates = new Map<string, string>();
    if (field.kind === "radio" && value) {
      for (const f of formFields) {
        if (f.kind === "radio" && f.fieldName === field.fieldName && f.id !== field.id) {
          updates.set(f.id, "");
        }
      }
    }
    updates.set(field.id, value);
    for (const [id, v] of updates) formValues.set(id, v);
    formFields = formFields.map((f) => (updates.has(f.id) ? { ...f, value: updates.get(f.id)! } : f));
    dirty = true;
  }

  const notes = $derived(
    annotations.filter((a): a is NoteAnnotation => a.type === "note"),
  );

  onMount(async () => {
    if (!inTauri()) {
      bridgeError = "Running outside the Tauri shell — open a PDF from the desktop app.";
      return;
    }
    try {
      const [info, status] = await Promise.all([getAppInfo(), getEngineStatus()]);
      version = info.version;
      engine = status;
    } catch (e) {
      bridgeError = String(e);
    }

    // Drag-and-drop support.
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const path = event.payload.paths.find((p) => p.toLowerCase().endsWith(".pdf"));
          if (path) openPath(path);
        }
      });
    } catch {
      /* drag-drop is best-effort */
    }
  });

  async function openPath(path: string) {
    loading = true;
    loadError = null;
    try {
      const bytes = await readPdf(path);

      // Load, prompting for a password if the file is encrypted. Each attempt
      // needs a fresh buffer copy since pdf.js detaches the one it's given.
      let loaded: Awaited<ReturnType<typeof loadPdf>> | null = null;
      let pw: string | undefined;
      while (!loaded) {
        try {
          loaded = await loadPdf(bytes.slice(0), pw);
        } catch (e) {
          const perr = passwordError(e);
          if (!perr) throw e;
          const entered = await promptPassword(perr.incorrect);
          if (entered === null) {
            loading = false;
            return; // user cancelled
          }
          pw = entered;
        }
      }
      openedPassword = pw ?? null;

      // Release any previously-loaded documents.
      for (const d of docs.values()) d.destroy();
      docs.clear();
      srcPaths.clear();

      const id = newDocId();
      docs.set(id, loaded.doc);
      try {
        srcPaths.set(id, await snapshotPdf(path));
      } catch {
        srcPaths.set(id, path);
      }

      doc = loaded.doc;
      fileName = baseName(path);
      filePath = path;
      saveMsg = null;
      dirty = false;
      pageList = Array.from({ length: loaded.numPages }, (_, i) => ({
        key: newPageKey(),
        docId: id,
        srcPage: i + 1,
        rotation: 0,
      }));
      // Fresh document — clear annotations, text edits, form values, and search.
      annotations = [];
      textBoxes = [];
      formValues.clear();
      formFields = [];
      selectedId = null;
      query = "";
      searchOpen = false;
    } catch (e) {
      loadError = `Couldn't open this PDF: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      loading = false;
    }
  }

  async function insertPdf() {
    if (!inTauri() || !doc) return;
    const path = await pickPdf();
    if (!path) return;
    loading = true;
    try {
      const bytes = await readPdf(path);
      const loaded = await loadPdf(bytes);
      const id = newDocId();
      docs.set(id, loaded.doc);
      try {
        srcPaths.set(id, await snapshotPdf(path));
      } catch {
        srcPaths.set(id, path);
      }
      const added: PageItem[] = Array.from({ length: loaded.numPages }, (_, i) => ({
        key: newPageKey(),
        docId: id,
        srcPage: i + 1,
        rotation: 0,
      }));
      pageList = [...pageList, ...added];
      dirty = true;
      saveMsg = `Inserted ${loaded.numPages} page${loaded.numPages === 1 ? "" : "s"} from ${baseName(path)}`;
    } catch (e) {
      saveMsg = `Insert failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      loading = false;
    }
  }

  async function handleOpen() {
    if (!inTauri()) {
      loadError = "File picker is only available in the desktop app.";
      return;
    }
    const path = await pickPdf();
    if (path) await openPath(path);
  }

  function scrollToKey(key: string) {
    document.getElementById(`page-${key}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  // 1-based display position of a page given its key (for the comments list).
  function pageIndexOf(key: string): number {
    return pageList.findIndex((p) => p.key === key) + 1;
  }

  // --- Page operations ---
  function deletePage(key: string) {
    if (pageList.length <= 1) return; // keep at least one page
    pageList = pageList.filter((p) => p.key !== key);
    dirty = true;
  }

  function movePage(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= pageList.length || to >= pageList.length) return;
    const next = pageList.slice();
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    pageList = next;
    dirty = true;
  }

  async function rotatePage(key: string, dir: 1 | -1) {
    const item = pageList.find((p) => p.key === key);
    if (!item) return;
    const pageDoc = docs.get(item.docId);
    const oldDelta = item.rotation;
    const newDelta = (((oldDelta + dir * 90) % 360) + 360) % 360;

    // Re-map this page's annotations from the old rotation to the new one so
    // they stay anchored to the same content (coords are in viewport space).
    const onPage = annotations.filter((a) => a.pageKey === item.key);
    if (onPage.length && pageDoc) {
      const page = await pageDoc.getPage(item.srcPage);
      const base = page.rotate;
      const oldVp = page.getViewport({ scale: 1, rotation: (((base + oldDelta) % 360) + 360) % 360 });
      const newVp = page.getViewport({ scale: 1, rotation: (((base + newDelta) % 360) + 360) % 360 });
      const remap = (x: number, y: number): [number, number] => {
        const [px, py] = oldVp.convertToPdfPoint(x, y);
        const [nx, ny] = newVp.convertToViewportPoint(px, py);
        return [nx, ny];
      };
      annotations = annotations.map((a) => {
        if (a.pageKey !== item.key) return a;
        if (a.type === "highlight") {
          const [x0, y0] = remap(a.rect.x, a.rect.y);
          const [x1, y1] = remap(a.rect.x + a.rect.w, a.rect.y + a.rect.h);
          return { ...a, rect: { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) } };
        }
        if (a.type === "draw") {
          return { ...a, paths: a.paths.map((s) => s.map((p) => { const [nx, ny] = remap(p.x, p.y); return { x: nx, y: ny }; })) };
        }
        const [nx, ny] = remap(a.x, a.y);
        return { ...a, x: nx, y: ny };
      });
    }

    pageList = pageList.map((p) => (p.key === key ? { ...p, rotation: newDelta } : p));
    dirty = true;
  }

  function zoom(delta: number) {
    scale = Math.min(4, Math.max(0.3, Math.round((scale + delta) * 100) / 100));
  }

  // Ctrl/Cmd + mouse-wheel zoom, anchored to the viewer. Attached manually with
  // { passive: false } so we can preventDefault and stop the webview's own zoom.
  $effect(() => {
    const el = mainEl;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      zoom(e.deltaY < 0 ? 0.12 : -0.12);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  async function fitWidth() {
    if (!doc || !mainEl) return;
    const page = await doc.getPage(1);
    const vp = page.getViewport({ scale: 1 });
    const avail = mainEl.clientWidth - 80;
    scale = Math.min(4, Math.max(0.3, avail / vp.width));
  }

  // Open the OS print dialog. The @media print stylesheet isolates the rendered
  // page stack (canvas + annotation/text/form overlays all print as-is) and
  // hides the app chrome. We pre-compute a zoom so the widest page fits the
  // paper regardless of the current on-screen zoom.
  function printDoc() {
    if (!doc) return;
    const wraps = Array.from(document.querySelectorAll<HTMLElement>(".page-wrap"));
    const maxW = wraps.reduce((m, el) => Math.max(m, el.getBoundingClientRect().width), 0);
    const target = 720; // ≈ 7.5in of printable width at 96dpi
    const z = maxW > target ? target / maxW : 1;
    document.documentElement.style.setProperty("--print-zoom", String(z));
    // Let the style flush before the (synchronous) print dialog opens.
    requestAnimationFrame(() => window.print());
  }

  function toggleSearch() {
    if (!doc) return;
    searchOpen = !searchOpen;
    if (searchOpen) {
      // Focus the field on the next tick, once it's in the DOM.
      queueMicrotask(() => searchInput?.focus());
    }
  }

  function closeSearch() {
    searchOpen = false;
    query = "";
  }

  function onQueryInput() {
    activeIndex = 0;
  }

  function stepMatch(dir: number) {
    if (matchCount === 0) return;
    activeIndex = (activeIndex + dir + matchCount) % matchCount;
  }

  function handleMatches(count: number) {
    matchCount = count;
    if (activeIndex >= count) activeIndex = 0;
  }

  // --- Annotations ---
  function setTool(t: Tool) {
    tool = t;
    if (t !== "none") selectedId = null;
    if (t === "note") commentsOpen = true;
  }

  function addAnnotation(a: Annotation) {
    annotations = [...annotations, a];
    selectedId = a.id;
    dirty = true;
    if (a.type === "note") commentsOpen = true;
  }

  function selectAnnotation(id: string | null) {
    selectedId = id;
  }

  function editNote(id: string, text: string) {
    annotations = annotations.map((a) =>
      a.id === id && a.type === "note" ? { ...a, text } : a,
    );
    dirty = true;
  }

  function deleteAnnotation(id: string) {
    annotations = annotations.filter((a) => a.id !== id);
    if (selectedId === id) selectedId = null;
    dirty = true;
  }

  function addTextBox(b: TextBox) {
    textBoxes = [...textBoxes, b];
    selectedId = b.id;
    dirty = true;
  }

  function editTextBox(id: string, text: string) {
    textBoxes = textBoxes.map((b) => (b.id === id ? { ...b, text } : b));
    dirty = true;
  }

  // Delete whichever selected item exists (annotation or text box).
  function deleteSelected() {
    if (!selectedId) return;
    const before = annotations.length + textBoxes.length;
    annotations = annotations.filter((a) => a.id !== selectedId);
    textBoxes = textBoxes.filter((b) => b.id !== selectedId);
    if (annotations.length + textBoxes.length !== before) dirty = true;
    selectedId = null;
  }

  async function handleSave(mode: "save" | "saveas") {
    if (!doc || !filePath || saving) return;
    if (encryptOn && !password.trim()) {
      saveMsg = "Enter a password to encrypt, or turn off the lock.";
      return;
    }
    let dest: string | null;
    if (mode === "save") {
      dest = filePath; // overwrite the open file
    } else {
      const stem = (fileName ?? "document").replace(/\.pdf$/i, "");
      dest = await pickSavePath(`${stem} (annotated).pdf`);
      if (!dest) return;
    }
    saving = true;
    saveMsg = null;
    try {
      // Order the source documents as first-seen in the page list.
      const docOrder: string[] = [];
      const sourceIndex = new Map<string, number>();
      for (const p of pageList) {
        if (!sourceIndex.has(p.docId)) {
          sourceIndex.set(p.docId, docOrder.length);
          docOrder.push(p.docId);
        }
      }
      const sources = docOrder.map((id) => srcPaths.get(id) ?? "");
      const plan = pageList.map((p) => ({
        source: sourceIndex.get(p.docId) ?? 0,
        srcPage: p.srcPage,
        rotation: p.rotation,
      }));
      const payload = await annotationsToPayload(annotations, pageList, getDoc);
      const sourceFor = (docId: string) => sourceIndex.get(docId) ?? 0;
      const contentEdits = await textBoxesToContentEdits(textBoxes, pageList, getDoc, sourceFor);
      // Flatten bakes field values into the page as stamped text; editable
      // writes them back into the live AcroForm.
      const flatten = hasForm && formSaveMode === "flatten";
      if (flatten) {
        contentEdits.push(...(await formFieldsToContentEdits(formFields, pageList, getDoc, sourceFor)));
      }
      const formValuesOut = hasForm && !flatten ? formFieldsToValues(formFields) : [];
      const pw = encryptOn && password.trim() ? password : undefined;
      await savePdf(
        sources,
        dest,
        plan,
        payload,
        contentEdits,
        formValuesOut,
        flatten ? "flatten" : "editable",
        pw,
        openedPassword ?? undefined,
      );
      dirty = false;
      const lock = pw ? " 🔒" : "";
      saveMsg = (mode === "save" ? "Saved" : `Saved ${baseName(dest)}`) + lock;
    } catch (e) {
      saveMsg = `Save failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      saving = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName;
    const typing = tag === "INPUT" || tag === "TEXTAREA";

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      handleSave(e.shiftKey ? "saveas" : "save");
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      toggleSearch();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "p") {
      e.preventDefault();
      printDoc();
    } else if ((e.ctrlKey || e.metaKey) && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      zoom(0.1);
    } else if ((e.ctrlKey || e.metaKey) && e.key === "-") {
      e.preventDefault();
      zoom(-0.1);
    } else if ((e.ctrlKey || e.metaKey) && e.key === "0") {
      e.preventDefault();
      scale = 1.2;
    } else if (e.key === "Escape") {
      if (searchOpen) closeSearch();
      else if (selectedId) selectedId = null;
      else if (tool !== "none") tool = "none";
    } else if ((e.key === "Delete" || e.key === "Backspace") && selectedId && !typing) {
      e.preventDefault();
      deleteSelected();
    }
  }

  const engineLabel = $derived(
    engine === null
      ? bridgeError
        ? "engine: offline"
        : "engine: checking…"
      : engine.available
        ? "PDFium ready"
        : "PDFium not found",
  );
  const engineOk = $derived(engine?.available === true);
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app-root flex h-full flex-col p-2.5 gap-2.5">
  <div class="no-print"><TitleBar /></div>

  <div class="no-print flex flex-wrap items-center gap-2 px-1">
    <div class="min-w-[280px] flex-1 overflow-x-auto scrollbar-none">
      <Toolbar onOpen={handleOpen} onSearch={toggleSearch} onOcr={() => (ocrOpen = !ocrOpen)} onSig={() => (sigOpen = !sigOpen)} onTool={setTool} activeTool={tool} hasDoc={!!doc} />
    </div>
    <div class="flex flex-wrap items-center gap-2">
      {#if doc}
        {#if hasForm}
          <div class="form-toggle glass" title="How form fields are written when you save">
            <button class:on={formSaveMode === "editable"} onclick={() => (formSaveMode = "editable")}>
              Editable
            </button>
            <button class:on={formSaveMode === "flatten"} onclick={() => (formSaveMode = "flatten")}>
              Flatten
            </button>
          </div>
        {/if}
        <div class="flex items-center gap-1">
          <button
            class="lock-btn glass glass-hover"
            class:on={encryptOn}
            onclick={() => (encryptOn = !encryptOn)}
            title={encryptOn ? "Encryption on — saved file needs a password (AES-128)" : "Encrypt saved file with a password (AES-128)"}
            aria-label="Toggle encryption"
          >
            {encryptOn ? "🔒" : "🔓"}
          </button>
          {#if encryptOn}
            <div class="pw-field glass">
              {#if showPassword}
                <input
                  type="text"
                  class="pw-input"
                  placeholder="Password"
                  bind:value={password}
                  spellcheck="false"
                />
              {:else}
                <input
                  type="password"
                  class="pw-input"
                  placeholder="Password"
                  bind:value={password}
                />
              {/if}
              <button class="pw-eye" onclick={() => (showPassword = !showPassword)} title={showPassword ? "Hide" : "Show"} aria-label="Toggle password visibility">
                {showPassword ? "🙈" : "👁"}
              </button>
            </div>
          {/if}
        </div>
        <div class="flex items-center">
          <button
            class="save-btn save-btn-main"
            onclick={() => handleSave("save")}
            disabled={saving}
            title={dirty ? "Unsaved changes — Save (Ctrl+S)" : "Save, overwriting the open file (Ctrl+S)"}
          >
            {#if dirty && !saving}<span class="dirty-dot"></span>{/if}
            {saving ? "Saving…" : "Save"}
          </button>
          <button
            class="save-btn save-btn-as"
            onclick={() => handleSave("saveas")}
            disabled={saving}
            title="Save As — write a new file (Ctrl+Shift+S)"
          >
            As…
          </button>
        </div>
        <button
          class="glass glass-hover flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs text-[var(--color-ink)]"
          onclick={printDoc}
          disabled={loading}
          title="Print — opens your system print dialog (Ctrl+P)"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
            <path d="M7 8V4h10v4M7 18H5a2 2 0 01-2-2v-3a2 2 0 012-2h14a2 2 0 012 2v3a2 2 0 01-2 2h-2M7 14h10v6H7z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" />
          </svg>
          Print
        </button>
        <div class="export-wrap">
          <button
            class="glass glass-hover flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs text-[var(--color-ink)]"
            class:export-on={exportOpen}
            onclick={() => (exportOpen = !exportOpen)}
            disabled={exporting}
            title="Export the document to another format"
          >
            {exporting ? "Exporting…" : "Export"}
            <span class="text-[10px] opacity-70">▾</span>
          </button>
          {#if exportOpen}
            <button class="export-backdrop" aria-label="Close export menu" onclick={() => (exportOpen = false)}></button>
            <div class="export-menu glass">
              {#each EXPORT_FORMATS as f}
                <button class="export-item glass-hover" onclick={() => doExport(f.id)}>{f.label}</button>
              {/each}
            </div>
          {/if}
        </div>
        <button
          class="glass glass-hover rounded-full px-3 py-1.5 text-xs text-[var(--color-ink)]"
          onclick={insertPdf}
          disabled={saving || loading}
          title="Insert pages from another PDF (appended; drag to reposition)"
        >
          Insert PDF
        </button>
        <button
          class="glass glass-hover rounded-full px-3 py-1.5 text-xs text-[var(--color-ink)]"
          class:comments-on={commentsOpen}
          onclick={() => (commentsOpen = !commentsOpen)}
          title="Toggle comments panel"
        >
          Comments{notes.length ? ` (${notes.length})` : ""}
        </button>
      {/if}
      {#if fileName}
        <div class="glass max-w-[280px] truncate rounded-full px-3 py-1.5 text-xs text-[var(--color-ink)]" title={fileName}>
          {fileName}
        </div>
      {/if}
      <div
        class="glass flex items-center gap-2 rounded-full px-3 py-1.5 text-xs"
        title={engine?.message ?? bridgeError ?? "Connecting to backend"}
      >
        <span
          class="h-2 w-2 rounded-full"
          style="background: {engineOk ? '#46d39a' : '#f0a73a'}; box-shadow: 0 0 8px {engineOk ? '#46d39a' : '#f0a73a'};"
        ></span>
        <span class="text-[var(--color-ink-dim)]">{engineLabel}</span>
      </div>
    </div>
  </div>

  <main class="app-main flex min-h-0 flex-1 gap-2.5">
    <div class="no-print contents">
      <ThumbRail {getDoc} {pageList} onSelect={scrollToKey} onDelete={deletePage} onMove={movePage} onRotate={rotatePage} />
    </div>
    <section bind:this={mainEl} class="viewer-shell glass relative min-h-0 flex-1 overflow-hidden rounded-2xl">
      {#if doc}
        <PdfViewer
          {getDoc}
          {pageList}
          {scale}
          {query}
          {activeIndex}
          onMatches={handleMatches}
          {tool}
          {color}
          {inkWidth}
          {annotations}
          {selectedId}
          onAddAnnotation={addAnnotation}
          onSelectAnnotation={selectAnnotation}
          {textBoxes}
          {fontSize}
          onAddTextBox={addTextBox}
          onEditTextBox={editTextBox}
          {formFields}
          {onFormChange}
        />

        <!-- Tool options (color / pen size / font size) -->
        {#if tool !== "none"}
          <div class="optbar glass no-print">
            <span class="text-[11px] uppercase tracking-wide text-[var(--color-ink-dim)]">
              {tool === "highlight" ? "Highlight" : tool === "draw" ? "Draw" : tool === "text" ? "Add text" : tool === "edittext" ? "Edit text — click a line" : "Note"}
            </span>
            <div class="mx-1 h-5 w-px bg-white/10"></div>
            {#each ANNOTATION_COLORS as c}
              <button
                class="swatch"
                class:on={color === c}
                style={`background:${c}`}
                onclick={() => (color = c)}
                aria-label={`Color ${c}`}
              ></button>
            {/each}
            {#if tool === "text"}
              <div class="mx-1 h-5 w-px bg-white/10"></div>
              <span class="text-[11px] text-[var(--color-ink-dim)]">A</span>
              <input
                type="range"
                min="8"
                max="48"
                step="1"
                bind:value={fontSize}
                class="penrange"
                title="Font size"
              />
              <span class="w-7 text-[11px] text-[var(--color-ink-dim)]">{fontSize}</span>
            {/if}
            {#if tool === "draw"}
              <div class="mx-1 h-5 w-px bg-white/10"></div>
              <input
                type="range"
                min="1"
                max="8"
                step="0.5"
                bind:value={inkWidth}
                class="penrange"
                title="Pen size"
              />
            {/if}
            <div class="mx-1 h-5 w-px bg-white/10"></div>
            <button class="zbtn glass-hover" onclick={() => (tool = "none")} title="Done">Done</button>
          </div>
        {/if}

        <!-- Find bar -->
        {#if searchOpen}
          <div class="findbar glass no-print">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" class="opacity-60">
              <path d="M10 4a6 6 0 104 10l5 5 1-1-5-5A6 6 0 0010 4z" stroke="currentColor" stroke-width="1.6" />
            </svg>
            <input
              bind:this={searchInput}
              bind:value={query}
              oninput={onQueryInput}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  stepMatch(e.shiftKey ? -1 : 1);
                }
              }}
              placeholder="Find in document"
              class="findinput"
              spellcheck="false"
            />
            <span class="findcount">
              {query.trim() ? (matchCount ? `${activeIndex + 1}/${matchCount}` : "0/0") : ""}
            </span>
            <button class="zbtn glass-hover" onclick={() => stepMatch(-1)} title="Previous (Shift+Enter)" aria-label="Previous match">‹</button>
            <button class="zbtn glass-hover" onclick={() => stepMatch(1)} title="Next (Enter)" aria-label="Next match">›</button>
            <button class="zbtn glass-hover" onclick={closeSearch} title="Close (Esc)" aria-label="Close search">✕</button>
          </div>
        {/if}

        <!-- Floating zoom controls -->
        <div class="zoombar glass no-print">
          <button class="zbtn glass-hover" onclick={() => zoom(-0.15)} title="Zoom out" aria-label="Zoom out">−</button>
          <button class="zlabel" onclick={() => (scale = 1.2)} title="Reset zoom">{Math.round(scale * 100)}%</button>
          <button class="zbtn glass-hover" onclick={() => zoom(0.15)} title="Zoom in" aria-label="Zoom in">+</button>
          <div class="mx-1 h-5 w-px bg-white/10"></div>
          <button class="zbtn wide glass-hover" onclick={fitWidth} title="Fit width">Fit</button>
        </div>
      {:else}
        <EmptyState onOpen={handleOpen} />
      {/if}

      {#if loading}
        <div class="overlay no-print">
          <div class="spinner"></div>
          <span class="text-sm text-[var(--color-ink-dim)]">Opening…</span>
        </div>
      {/if}
    </section>

    {#if doc && commentsOpen}
      <div class="no-print contents">
        <CommentsPanel
          notes={notes}
          {selectedId}
          pageIndexOf={pageIndexOf}
          onSelect={(id) => {
            const note = notes.find((n) => n.id === id);
            selectAnnotation(id);
            if (note) scrollToKey(note.pageKey);
          }}
          onEdit={editNote}
          onDelete={deleteAnnotation}
          onClose={() => (commentsOpen = false)}
        />
      </div>
    {/if}
      {#if doc && ocrOpen && filePath}
      <div class="no-print contents">
        <OcrPanel
          {filePath}
          sourcePassword={openedPassword ?? undefined}
          onDone={() => {
            saveMsg = "OCR complete — reload the file to see searchable text.";
          }}
          onClose={() => (ocrOpen = false)}
        />
      </div>
    {/if}
    {#if doc && sigOpen && filePath}
      <div class="no-print contents">
        <SignaturePanel
          {filePath}
          sourcePassword={openedPassword ?? undefined}
          onClose={() => (sigOpen = false)}
        />
      </div>
    {/if}
  </main>

  <footer class="flex items-center justify-between px-2 text-[11px] text-[var(--color-ink-dim)]">
    <span>
      limitlessPDF v{version}{#if loadError} · <span class="text-[#f0a73a]">{loadError}</span>{/if}{#if saveMsg} · <span class="text-[var(--color-accent)]">{saveMsg}</span>{/if}
    </span>
    <span>{doc ? `${doc.numPages} pages` : "M1 viewer — Tauri · Svelte · pdf.js"}</span>
  </footer>

  {#if pwPrompt}
    <div class="pw-modal-backdrop no-print">
      <form
        class="pw-modal glass"
        onsubmit={(e) => {
          e.preventDefault();
          if (pwEntry) resolvePassword(pwEntry);
        }}
      >
        <div class="text-lg">🔒</div>
        <div class="pw-modal-title">This PDF is password-protected</div>
        <div class="pw-modal-sub">
          {pwPrompt.incorrect ? "Incorrect password — try again." : "Enter the password to open it."}
        </div>
        <input
          id="pw-entry"
          type="password"
          class="pw-modal-input"
          placeholder="Password"
          bind:value={pwEntry}
          autocomplete="off"
        />
        <div class="pw-modal-actions">
          <button type="button" class="pw-modal-btn" onclick={() => resolvePassword(null)}>Cancel</button>
          <button type="submit" class="pw-modal-btn primary" disabled={!pwEntry}>Open</button>
        </div>
      </form>
    </div>
  {/if}
</div>

<style>
  .zoombar {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px;
    border-radius: 999px;
    z-index: 5;
  }
  .findbar {
    position: absolute;
    top: 14px;
    right: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 14px;
    z-index: 8;
    color: var(--color-ink);
  }
  .optbar {
    position: absolute;
    top: 14px;
    left: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: 14px;
    z-index: 8;
    color: var(--color-ink);
  }
  .swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid transparent;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
    cursor: pointer;
  }
  .swatch.on {
    border-color: #fff;
    transform: scale(1.12);
  }
  .penrange {
    width: 90px;
    accent-color: var(--color-accent);
  }
  .comments-on {
    border-color: var(--color-accent);
    color: #fff;
  }
  .save-btn {
    font-size: 12px;
    font-weight: 600;
    color: #0b0d14;
    background: linear-gradient(135deg, var(--color-accent), var(--color-accent-2));
    border: 1px solid rgba(255, 255, 255, 0.25);
    padding: 6px 12px;
    cursor: pointer;
  }
  .save-btn:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .save-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .save-btn-main {
    border-radius: 999px 0 0 999px;
    border-right: 1px solid rgba(11, 13, 20, 0.25);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .dirty-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #0b0d14;
    box-shadow: 0 0 0 2px rgba(11, 13, 20, 0.25);
  }
  .save-btn-as {
    border-radius: 0 999px 999px 0;
    border-left: none;
    padding-left: 9px;
    padding-right: 12px;
  }
  .form-toggle {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 999px;
  }
  .form-toggle button {
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 11px;
    color: var(--color-ink-dim);
    white-space: nowrap;
  }
  .form-toggle button.on {
    background: linear-gradient(135deg, rgba(110, 168, 255, 0.3), rgba(167, 139, 250, 0.3));
    color: #fff;
  }
  .lock-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 30px;
    width: 32px;
    border-radius: 999px;
    font-size: 14px;
    line-height: 1;
  }
  .export-wrap {
    position: relative;
  }
  .export-on {
    background: linear-gradient(135deg, rgba(110, 168, 255, 0.28), rgba(167, 139, 250, 0.28)) !important;
  }
  .export-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
    border: none;
    cursor: default;
  }
  .export-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 41;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px;
    border-radius: 12px;
    min-width: 170px;
  }
  .export-item {
    text-align: left;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 12px;
    color: var(--color-ink);
    white-space: nowrap;
  }
  .pw-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(8, 8, 14, 0.55);
    backdrop-filter: blur(3px);
  }
  .pw-modal {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 320px;
    padding: 22px;
    border-radius: 18px;
    text-align: center;
  }
  .pw-modal-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .pw-modal-sub {
    font-size: 12px;
    color: var(--color-ink-dim);
  }
  .pw-modal-input {
    width: 100%;
    margin-top: 6px;
    padding: 9px 12px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-ink);
    outline: none;
    font-size: 13px;
  }
  .pw-modal-input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(110, 168, 255, 0.3);
  }
  .pw-modal-actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
    width: 100%;
  }
  .pw-modal-btn {
    flex: 1;
    padding: 8px 0;
    border-radius: 10px;
    font-size: 13px;
    color: var(--color-ink);
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .pw-modal-btn.primary {
    background: linear-gradient(135deg, rgba(110, 168, 255, 0.5), rgba(167, 139, 250, 0.5));
    border-color: rgba(110, 168, 255, 0.6);
    color: #fff;
  }
  .pw-modal-btn:disabled {
    opacity: 0.45;
  }
  .lock-btn.on {
    background: linear-gradient(135deg, rgba(110, 168, 255, 0.35), rgba(167, 139, 250, 0.35));
    border: 1px solid rgba(110, 168, 255, 0.5);
  }
  .pw-field {
    display: flex;
    align-items: center;
    height: 30px;
    border-radius: 999px;
    padding: 0 4px 0 10px;
  }
  .pw-input {
    width: 120px;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-ink);
    font-size: 12px;
  }
  .pw-eye {
    padding: 2px 4px;
    font-size: 12px;
    opacity: 0.8;
  }
  .findinput {
    width: 200px;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-ink);
    font-size: 13px;
  }
  .findinput::placeholder {
    color: var(--color-ink-dim);
  }
  .findcount {
    min-width: 42px;
    text-align: right;
    font-size: 12px;
    color: var(--color-ink-dim);
    font-variant-numeric: tabular-nums;
  }
  .zbtn {
    display: grid;
    place-items: center;
    min-width: 30px;
    height: 28px;
    padding: 0 8px;
    border-radius: 999px;
    color: var(--color-ink);
    border: 1px solid transparent;
    font-size: 15px;
    line-height: 1;
  }
  .zbtn.wide {
    font-size: 12px;
  }
  .zlabel {
    min-width: 52px;
    text-align: center;
    font-size: 12px;
    color: var(--color-ink-dim);
    background: transparent;
    border: none;
    cursor: pointer;
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: rgba(12, 13, 20, 0.45);
    backdrop-filter: blur(4px);
    z-index: 10;
  }
  .spinner {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: 3px solid rgba(255, 255, 255, 0.15);
    border-top-color: var(--color-accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .scrollbar-none::-webkit-scrollbar { display: none; }
  .scrollbar-none { -ms-overflow-style: none; scrollbar-width: none; }

  /* Print: show only the page stack, flowed one page per sheet. The chrome is
     marked .no-print; the layout wrappers are neutralized so pages can flow. */
  @media print {
    :global(.no-print) {
      display: none !important;
    }
    .app-root {
      display: block !important;
      height: auto !important;
      padding: 0 !important;
      gap: 0 !important;
    }
    .app-main {
      display: block !important;
      height: auto !important;
      gap: 0 !important;
    }
    .viewer-shell {
      display: block !important;
      height: auto !important;
      overflow: visible !important;
      border-radius: 0 !important;
      background: #fff !important;
      box-shadow: none !important;
    }
  }
  @media print {
    :global(html),
    :global(body) {
      height: auto !important;
      overflow: visible !important;
      background: #fff !important;
    }
  }
</style>
