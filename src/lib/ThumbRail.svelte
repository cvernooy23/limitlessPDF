<script lang="ts">
  import { pageRender, type PdfDocument, type PageItem } from "./pdf";

  const {
    getDoc,
    pageList = [],
    onSelect,
    onDelete,
    onMove,
    onRotate,
  }: {
    getDoc?: (docId: string) => PdfDocument | undefined;
    pageList?: PageItem[];
    onSelect?: (key: string) => void;
    onDelete?: (key: string) => void;
    onMove?: (from: number, to: number) => void;
    onRotate?: (key: string, dir: 1 | -1) => void;
  } = $props();

  const ready = $derived(pageList.length > 0 && !!getDoc);

  // Pointer-capture drag reorder. Capturing the pointer guarantees move/up
  // events reach us even as the cursor moves over other thumbnails — and it
  // avoids the webview's flaky HTML5 drag-and-drop / Tauri file-drop conflict.
  let dragIndex = $state<number | null>(null);
  let overIndex = $state<number | null>(null);
  let dragging = $state(false);
  let startY = 0;
  let captureEl: HTMLElement | null = null;

  function thumbDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    const el = (e.target as HTMLElement).closest("[data-thumb-index]") as HTMLElement | null;
    if (!el) return;
    captureEl = el;
    el.setPointerCapture(e.pointerId);
    dragIndex = i;
    overIndex = i;
    dragging = false;
    startY = e.clientY;
  }

  function thumbMove(e: PointerEvent) {
    if (dragIndex === null) return;
    if (!dragging && Math.abs(e.clientY - startY) > 4) dragging = true;
    if (!dragging) return;
    const t = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest("[data-thumb-index]") as HTMLElement | null;
    if (t) overIndex = Number(t.dataset.thumbIndex);
  }

  function thumbUp(e: PointerEvent) {
    try {
      captureEl?.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    captureEl = null;
    if (dragIndex !== null) {
      if (dragging && overIndex !== null && overIndex !== dragIndex) {
        onMove?.(dragIndex, overIndex);
      } else if (!dragging) {
        onSelect?.(pageList[dragIndex]?.key);
      }
    }
    dragIndex = null;
    overIndex = null;
    dragging = false;
  }
</script>

<aside class="glass flex w-44 shrink-0 flex-col gap-3 rounded-2xl p-3">
  <div class="px-1 text-xs font-medium uppercase tracking-wider text-[var(--color-ink-dim)]">
    Pages{#if ready}<span class="ml-1 normal-case opacity-70">({pageList.length})</span>{/if}
  </div>

  <div class="flex flex-col gap-3 overflow-y-auto pr-1">
    {#if ready}
      {#each pageList as pg, i (pg.key)}
        {@const d = getDoc?.(pg.docId)}
        <div
          class="thumb"
          class:drop-above={dragging && overIndex === i && dragIndex !== null && i < dragIndex}
          class:drop-below={dragging && overIndex === i && dragIndex !== null && i > dragIndex}
          class:lifting={dragging && dragIndex === i}
          data-thumb-index={i}
          role="button"
          tabindex="0"
          onpointerdown={(e) => thumbDown(e, i)}
          onpointermove={thumbMove}
          onpointerup={thumbUp}
          onkeydown={(e) => e.key === "Enter" && onSelect?.(pg.key)}
        >
          <div class="page-frame">
            {#if d}
              <canvas class="thumb-canvas" use:pageRender={{ doc: d, page: pg.srcPage, scale: 0.22, rotation: pg.rotation }}></canvas>
            {:else}
              <div class="page-skeleton"></div>
            {/if}
            <div class="thumb-actions">
              <button
                class="act"
                title="Rotate left"
                aria-label="Rotate left"
                onpointerdown={(e) => e.stopPropagation()}
                onclick={(e) => { e.stopPropagation(); onRotate?.(pg.key, -1); }}
              >↺</button>
              <button
                class="act"
                title="Rotate right"
                aria-label="Rotate right"
                onpointerdown={(e) => e.stopPropagation()}
                onclick={(e) => { e.stopPropagation(); onRotate?.(pg.key, 1); }}
              >↻</button>
              <button
                class="act act-del"
                title="Delete page"
                aria-label="Delete page"
                onpointerdown={(e) => e.stopPropagation()}
                onclick={(e) => { e.stopPropagation(); onDelete?.(pg.key); }}
              >✕</button>
            </div>
          </div>
          <span class="text-[11px] text-[var(--color-ink-dim)]">{i + 1}</span>
        </div>
      {/each}
    {:else}
      {#each [1, 2, 3, 4] as n}
        <div class="thumb">
          <div class="page-skeleton"></div>
          <span class="text-[11px] text-[var(--color-ink-dim)]">{n}</span>
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .thumb {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px;
    border-radius: 12px;
    border: 1px solid transparent;
    background: transparent;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .thumb:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .thumb.lifting {
    opacity: 0.5;
    cursor: grabbing;
  }
  .thumb.drop-above::before,
  .thumb.drop-below::after {
    content: "";
    position: absolute;
    left: 6px;
    right: 6px;
    height: 3px;
    border-radius: 999px;
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
  }
  .thumb.drop-above::before {
    top: -2px;
  }
  .thumb.drop-below::after {
    bottom: -2px;
  }
  .page-frame {
    position: relative;
    width: 100%;
  }
  .thumb-canvas {
    width: 100%;
    height: auto;
    border-radius: 4px;
    background: #fff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
  .thumb-actions {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    gap: 3px;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .page-frame:hover .thumb-actions {
    opacity: 1;
  }
  .act {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: none;
    background: rgba(12, 13, 20, 0.72);
    color: #fff;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
  }
  .act:hover {
    background: var(--color-accent);
    color: #0b0d14;
  }
  .act-del:hover {
    background: #e5484d;
    color: #fff;
  }
  .page-skeleton {
    width: 100%;
    aspect-ratio: 3 / 4;
    border-radius: 6px;
    background: linear-gradient(160deg, rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.03));
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
</style>
