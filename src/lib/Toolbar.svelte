<script lang="ts">
  import type { Tool as AnnoTool } from "./annotations";

  interface ToolDef {
    id: string;
    label: string;
    icon: string; // inline SVG path data
    enabled?: boolean;
    /** Annotation mode this button activates, if any. */
    mode?: AnnoTool;
  }

  const {
    onOpen,
    onSearch,
    onTool,
    onOcr,
    onSig,
    activeTool = "none",
    hasDoc = false,
  }: {
    onOpen?: () => void;
    onSearch?: () => void;
    onTool?: (mode: AnnoTool) => void;
    onOcr?: () => void;
    onSig?: () => void;
    activeTool?: AnnoTool;
    hasDoc?: boolean;
  } = $props();

  const groups: ToolDef[][] = [
    [{ id: "open", label: "Open", icon: "M4 4h6l2 2h8v12H4z", enabled: true }],
    [
      { id: "select", label: "Select", icon: "M5 3l14 7-6 1-1 6z", enabled: true, mode: "none" },
      {
        id: "search",
        label: "Search",
        icon: "M10 4a6 6 0 104 10l5 5 1-1-5-5A6 6 0 0010 4z",
        enabled: true,
      },
    ],
    [
      {
        id: "highlight",
        label: "Highlight",
        icon: "M4 16l8-8 4 4-8 8H4z",
        enabled: true,
        mode: "highlight",
      },
      { id: "note", label: "Note", icon: "M5 4h14v10l-4 4H5z", enabled: true, mode: "note" },
      { id: "draw", label: "Draw", icon: "M4 18l9-9 2 2-9 9H4z", enabled: true, mode: "draw" },
      {
        id: "underline",
        label: "Underline",
        icon: "M6 19h12M8 5v9a4 4 0 008 0V5",
        enabled: true,
        mode: "underline",
      },
      {
        id: "strikethrough",
        label: "Strikethrough",
        icon: "M5 12h14M12 5c-2.8 0-4 1.5-4 3 0 1 .5 1.8 1.5 2.3M12 19c2.8 0 4-1.5 4-3 0-1-.5-1.8-1.5-2.3",
        enabled: true,
        mode: "strikethrough",
      },
      {
        id: "rect",
        label: "Rectangle",
        icon: "M4 4h16v16H4z",
        enabled: true,
        mode: "rect",
      },
      {
        id: "circle",
        label: "Circle",
        icon: "M12 4a8 8 0 100 16 8 8 0 000-16z",
        enabled: true,
        mode: "circle",
      },
      {
        id: "arrow",
        label: "Arrow",
        icon: "M5 19L19 5M19 5v7M19 5h-7",
        enabled: true,
        mode: "arrow",
      },
      {
        id: "redact",
        label: "Redact",
        icon: "M3 5h18v14H3zM3 5l18 14M21 5L3 19",
        enabled: true,
        mode: "redact",
      },
    ],
    [
      { id: "text", label: "Text", icon: "M5 5h14M12 5v14M9 19h6", enabled: true, mode: "text" },
      {
        id: "edit",
        label: "Edit text",
        icon: "M5 16l9-9 3 3-9 9H5z",
        enabled: true,
        mode: "edittext",
      },
    ],
    [
      {
        id: "ocr",
        label: "OCR",
        icon: "M4 7V4h3M17 4h3v3M20 17v3h-3M7 20H4v-3M9 9h6M9 12h4M9 15h5",
        enabled: true,
      },
      {
        id: "sig",
        label: "Signatures",
        icon: "M4 20l3-3 2 2-3 3H4zM9 15l8-8 2 2-8 8M16 4l2-2 4 4-2 2",
        enabled: true,
      },
    ],
  ];

  function isEnabled(t: ToolDef): boolean {
    if (t.enabled === undefined) return false;
    if (t.id !== "open" && !hasDoc) return false;
    return true;
  }

  function isActive(t: ToolDef): boolean {
    return t.mode !== undefined && activeTool === t.mode;
  }

  function handle(t: ToolDef) {
    if (!isEnabled(t)) return;
    if (t.id === "open") onOpen?.();
    else if (t.id === "search") onSearch?.();
    else if (t.id === "ocr") onOcr?.();
    else if (t.id === "sig") onSig?.();
    else if (t.mode) onTool?.(t.mode);
  }

  // ── Scroll overflow detection ──────────────────────────────────────
  let trackEl = $state<HTMLElement | null>(null);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function checkScroll() {
    if (!trackEl) return;
    const { scrollLeft, scrollWidth, clientWidth } = trackEl;
    canScrollLeft = scrollLeft > 2;
    canScrollRight = scrollLeft + clientWidth < scrollWidth - 2;
  }

  function scroll(dx: number) {
    trackEl?.scrollBy({ left: dx, behavior: "smooth" });
  }

  $effect(() => {
    if (!trackEl) return;
    const ro = new ResizeObserver(() => checkScroll());
    ro.observe(trackEl);
    checkScroll();
    return () => ro.disconnect();
  });
</script>

<div class="toolbar-wrap">
  <button
    class="scroll-arrow left"
    class:visible={canScrollLeft}
    onclick={() => scroll(-200)}
    tabindex={-1}
    aria-label="Scroll toolbar left"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
      <path
        d="M15 6l-6 6 6 6"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>

  <div class="toolbar-track" bind:this={trackEl} onscroll={checkScroll}>
    <div class="glass sheen flex w-max items-center gap-1 rounded-2xl px-2 py-1.5">
      {#each groups as group, i}
        {#if i > 0}
          <div class="mx-1 h-6 w-px bg-white/10"></div>
        {/if}
        {#each group as tool}
          <button
            class="tool glass-hover"
            class:active={isActive(tool)}
            class:opacity-40={!isEnabled(tool)}
            disabled={!isEnabled(tool)}
            onclick={() => handle(tool)}
            title={tool.enabled === undefined ? `${tool.label} — coming soon` : tool.label}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
              <path
                d={tool.icon}
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linejoin="round"
                stroke-linecap="round"
              />
            </svg>
            <span class="tool-label">{tool.label}</span>
          </button>
        {/each}
      {/each}
    </div>
  </div>

  <button
    class="scroll-arrow right"
    class:visible={canScrollRight}
    onclick={() => scroll(200)}
    tabindex={-1}
    aria-label="Scroll toolbar right"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
      <path
        d="M9 6l6 6-6 6"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>
</div>

<style>
  /* ── Scroll wrapper ─────────────────────────────────────────────── */
  .toolbar-wrap {
    position: relative;
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .toolbar-track {
    overflow-x: auto;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  .toolbar-track::-webkit-scrollbar {
    display: none;
  }

  /* Gradient edge fades */
  .toolbar-wrap::before,
  .toolbar-wrap::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    width: 28px;
    pointer-events: none;
    z-index: 2;
    opacity: 0;
    transition: opacity 0.2s;
  }
  .toolbar-wrap:has(.scroll-arrow.left.visible)::before {
    opacity: 1;
    left: 24px;
    background: linear-gradient(to right, var(--color-bg, #1a1a2e), transparent);
  }
  .toolbar-wrap:has(.scroll-arrow.right.visible)::after {
    opacity: 1;
    right: 24px;
    background: linear-gradient(to left, var(--color-bg, #1a1a2e), transparent);
  }

  /* Arrow buttons */
  .scroll-arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    flex-shrink: 0;
    border-radius: 50%;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-ink);
    cursor: pointer;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.2s;
    z-index: 3;
  }
  .scroll-arrow.visible {
    opacity: 0.7;
    pointer-events: auto;
  }
  .scroll-arrow:hover {
    opacity: 1;
    background: rgba(255, 255, 255, 0.15);
  }
  .scroll-arrow.left {
    margin-right: 4px;
  }
  .scroll-arrow.right {
    margin-left: 4px;
  }

  /* ── Tool buttons ───────────────────────────────────────────────── */
  .tool {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: 10px;
    color: var(--color-ink);
    border: 1px solid transparent;
    white-space: nowrap;
  }
  .tool:disabled {
    cursor: default;
  }
  .tool.active {
    background: linear-gradient(135deg, rgba(110, 168, 255, 0.28), rgba(167, 139, 250, 0.28));
    border-color: rgba(110, 168, 255, 0.5);
    color: #fff;
  }

  /* Print layout: hide toolbar entirely */
  @media print {
    .toolbar-wrap {
      display: none;
    }
  }
</style>
