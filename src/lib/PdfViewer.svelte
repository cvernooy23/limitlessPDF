<script lang="ts">
  import {
    pageRender,
    textLayerRender,
    type PdfDocument,
    type PageItem,
    type TextBox,
    type FormField,
  } from "./pdf";
  import AnnotationLayer from "./AnnotationLayer.svelte";
  import TextBoxLayer from "./TextBoxLayer.svelte";
  import FormLayer from "./FormLayer.svelte";
  import { hitTest, newId, type Annotation, type Tool } from "./annotations";

  const {
    getDoc,
    pageList = [],
    scale = 1.2,
    query = "",
    activeIndex = 0,
    onMatches,
    tool = "none",
    color = "#ffd23f",
    inkWidth = 2.5,
    annotations = [],
    selectedId = null,
    onAddAnnotation,
    onSelectAnnotation,
    textBoxes = [],
    fontSize = 16,
    onAddTextBox,
    onEditTextBox,
    formFields = [],
    onFormChange,
  }: {
    getDoc: (docId: string) => PdfDocument | undefined;
    pageList?: PageItem[];
    scale?: number;
    query?: string;
    activeIndex?: number;
    onMatches?: (count: number) => void;
    tool?: Tool;
    color?: string;
    inkWidth?: number;
    annotations?: Annotation[];
    selectedId?: string | null;
    onAddAnnotation?: (a: Annotation) => void;
    onSelectAnnotation?: (id: string | null) => void;
    textBoxes?: TextBox[];
    fontSize?: number;
    onAddTextBox?: (b: TextBox) => void;
    onEditTextBox?: (id: string, text: string) => void;
    formFields?: FormField[];
    onFormChange?: (field: FormField, value: string) => void;
  } = $props();

  // Select mode: hit-test the click against this page's annotations. Runs on the
  // HTML page container (reliable), not on SVG elements. Does not block text
  // selection — empty clicks just clear the selection.
  function selectAt(e: PointerEvent, key: string) {
    if (tool !== "none") return;
    const wrap = document.getElementById(`page-${key}`);
    if (!wrap) return;
    const r = wrap.getBoundingClientRect();
    const id = hitTest(
      annotations.filter((a) => a.pageKey === key),
      e.clientX - r.left,
      e.clientY - r.top,
      scale,
    );
    onSelectAnnotation?.(id);
  }

  function onPagePointerDown(e: PointerEvent, key: string) {
    if (tool === "none") {
      selectAt(e, key);
      return;
    }
    if (tool === "edittext") {
      // Identify the clicked text run: a <span> that is a direct child of the
      // pdf.js text layer. Anything else (empty space, the endOfContent helper,
      // the layer itself) is ignored so we never cover the whole page.
      const node = e.target as HTMLElement | null;
      const layer = node?.closest?.(".textLayer") as HTMLElement | null;
      if (!node || !layer || node === layer) return;
      let run: HTMLElement | null = node;
      while (run && run !== layer && run.parentElement !== layer) {
        run = run.parentElement;
      }
      if (!run || run === layer || run.parentElement !== layer || run.tagName !== "SPAN") return;
      const orig = run.textContent ?? "";
      if (!orig.trim()) return;
      const wrap = document.getElementById(`page-${key}`);
      if (!wrap) return;
      const wr = wrap.getBoundingClientRect();
      const sr = run.getBoundingClientRect();
      onAddTextBox?.({
        id: newId(),
        pageKey: key,
        x: (sr.left - wr.left) / scale,
        y: (sr.top - wr.top) / scale,
        w: sr.width / scale,
        h: sr.height / scale,
        size: (sr.height / scale) * 0.82,
        color: "#111111",
        text: orig,
        origText: orig,
      });
    }
  }

  // Per-page selectable text divs (keyed by page key), populated as each text
  // layer renders. Insertion order tracks render order ≈ visual order.
  const pageDivs = new Map<string, HTMLElement[]>();
  // Each match is its own <span> wrapping one occurrence of the query.
  let matches: HTMLElement[] = [];
  // Divs we've wrapped with match spans, so we can restore them cheaply.
  const markedDivs = new Set<HTMLElement>();
  // Tracks the active match element so we only scroll/flash when it changes.
  let activeEl: HTMLElement | null = null;

  function handleTextReady(key: string, divs: HTMLElement[]) {
    pageDivs.set(key, divs);
    recompute();
  }

  /** Restore a previously-marked div back to plain text. */
  function clearMarks(div: HTMLElement) {
    // textContent concatenates across the wrapper spans, giving the plain text.
    div.textContent = div.textContent;
  }

  /** Wrap every occurrence of `q` inside `div`, returning the created spans. */
  function markOccurrences(div: HTMLElement, q: string): HTMLElement[] {
    const text = div.textContent ?? "";
    const lower = text.toLowerCase();
    if (!lower.includes(q)) return [];

    const frag = document.createDocumentFragment();
    const created: HTMLElement[] = [];
    let from = 0;
    let idx = lower.indexOf(q, from);
    while (idx !== -1) {
      if (idx > from) frag.appendChild(document.createTextNode(text.slice(from, idx)));
      const mark = document.createElement("span");
      mark.className = "lp-hit";
      mark.textContent = text.slice(idx, idx + q.length);
      frag.appendChild(mark);
      created.push(mark);
      from = idx + q.length;
      idx = lower.indexOf(q, from);
    }
    if (from < text.length) frag.appendChild(document.createTextNode(text.slice(from)));

    div.replaceChildren(frag);
    markedDivs.add(div);
    return created;
  }

  function recompute() {
    // Restore any divs we marked previously.
    for (const div of markedDivs) clearMarks(div);
    markedDivs.clear();
    matches = [];
    activeEl = null;

    const q = query.trim().toLowerCase();
    if (q.length > 0) {
      for (const divs of pageDivs.values()) {
        for (const div of divs) {
          matches.push(...markOccurrences(div, q));
        }
      }
    }

    onMatches?.(matches.length);
    syncActive();
  }

  function syncActive() {
    matches.forEach((el, i) => el.classList.toggle("lp-hit-active", i === activeIndex));
    const el = matches[activeIndex] ?? null;
    if (el && el !== activeEl) {
      activeEl = el;
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      flashPage(el);
    } else if (!el) {
      activeEl = null;
    }
  }

  // Briefly pulse the page containing the active match.
  function flashPage(el: HTMLElement) {
    const wrap = el.closest(".page-wrap") as HTMLElement | null;
    if (!wrap) return;
    wrap.classList.remove("lp-flash");
    void wrap.offsetWidth; // force reflow so the animation restarts
    wrap.classList.add("lp-flash");
  }

  // Recompute highlights when the search query changes.
  $effect(() => {
    void query;
    recompute();
  });

  // Scroll/flash when the user steps through matches.
  $effect(() => {
    void activeIndex;
    syncActive();
  });
</script>

<div class="viewer">
  {#each pageList as pg, i (pg.key)}
    {@const d = getDoc(pg.docId)}
    <div
      class="page-wrap"
      id={`page-${pg.key}`}
      onpointerdown={(e) => onPagePointerDown(e, pg.key)}
    >
      {#if d}
        <canvas use:pageRender={{ doc: d, page: pg.srcPage, scale, rotation: pg.rotation }}
        ></canvas>
        <div
          class="textLayer"
          use:textLayerRender={{
            doc: d,
            page: pg.srcPage,
            scale,
            rotation: pg.rotation,
            onReady: (_p, divs) => handleTextReady(pg.key, divs),
          }}
        ></div>
      {/if}
      <AnnotationLayer
        pageKey={pg.key}
        {scale}
        {tool}
        {color}
        {inkWidth}
        annotations={annotations.filter((a) => a.pageKey === pg.key)}
        {selectedId}
        onAdd={(a) => onAddAnnotation?.(a)}
        onSelect={(id) => onSelectAnnotation?.(id)}
      />
      <TextBoxLayer
        pageKey={pg.key}
        {scale}
        {tool}
        {color}
        {fontSize}
        boxes={textBoxes.filter((b) => b.pageKey === pg.key)}
        {selectedId}
        onAdd={(b) => onAddTextBox?.(b)}
        onEdit={(id, text) => onEditTextBox?.(id, text)}
        onSelect={(id) => onSelectAnnotation?.(id)}
      />
      <FormLayer
        pageKey={pg.key}
        {scale}
        {tool}
        fields={formFields.filter((f) => f.pageKey === pg.key)}
        onChange={(field, value) => onFormChange?.(field, value)}
      />
      <span class="page-num">{i + 1}</span>
    </div>
  {/each}
</div>

<style>
  .viewer {
    height: 100%;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 22px;
    padding: 26px 26px 60px;
    scroll-behavior: smooth;
  }
  .page-wrap {
    position: relative;
    border-radius: 6px;
    background: #fff;
    box-shadow:
      0 10px 30px rgba(0, 0, 0, 0.45),
      0 2px 8px rgba(0, 0, 0, 0.3);
  }
  .page-wrap canvas {
    display: block;
    border-radius: 6px;
  }
  /* Navigation pulse — a glowing ring around the page holding the active match.
     The class is toggled from JS, so mark it :global to keep svelte-check quiet. */
  .page-wrap:global(.lp-flash) {
    animation: lp-flash 0.65s ease;
  }
  @keyframes -global-lp-flash {
    0% {
      box-shadow:
        0 0 0 0 rgba(110, 168, 255, 0),
        0 10px 30px rgba(0, 0, 0, 0.45);
    }
    28% {
      box-shadow:
        0 0 0 4px rgba(110, 168, 255, 0.65),
        0 0 22px 4px rgba(110, 168, 255, 0.4),
        0 10px 30px rgba(0, 0, 0, 0.45);
    }
    100% {
      box-shadow:
        0 0 0 0 rgba(110, 168, 255, 0),
        0 10px 30px rgba(0, 0, 0, 0.45);
    }
  }
  .page-num {
    position: absolute;
    bottom: -22px;
    left: 50%;
    transform: translateX(-50%);
    font-size: 11px;
    color: var(--color-ink-dim);
  }

  /* Print layout: stack pages, one per sheet, fit to paper via --print-zoom
     (set on <html> before printing). zoom scales the page + overlays together
     and is layout-aware so page breaks land correctly. */
  @media print {
    .viewer {
      display: block;
      height: auto;
      overflow: visible;
      padding: 0;
      gap: 0;
      scroll-behavior: auto;
      zoom: var(--print-zoom, 1);
    }
    .page-wrap {
      break-after: page;
      page-break-after: always;
      box-shadow: none !important;
      border-radius: 0;
      margin: 0 auto !important;
    }
    .page-wrap:last-child {
      break-after: auto;
      page-break-after: auto;
    }
    .page-num {
      display: none;
    }
  }
</style>
