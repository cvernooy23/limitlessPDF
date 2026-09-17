<script lang="ts">
  import { newId, type Tool } from "./annotations";
  import type { TextBox } from "./pdf";

  const {
    pageKey,
    scale,
    tool,
    color,
    fontSize = 16,
    boxes,
    selectedId = null,
    onAdd,
    onEdit,
    onSelect,
  }: {
    pageKey: string;
    scale: number;
    tool: Tool;
    color: string;
    fontSize?: number;
    boxes: TextBox[];
    selectedId?: string | null;
    onAdd: (b: TextBox) => void;
    onEdit: (id: string, text: string) => void;
    onSelect: (id: string | null) => void;
  } = $props();

  function place(e: PointerEvent) {
    const wrap = document.getElementById(`page-${pageKey}`);
    if (!wrap) return;
    const r = wrap.getBoundingClientRect();
    const x = (e.clientX - r.left) / scale;
    const y = (e.clientY - r.top) / scale;
    const b: TextBox = { id: newId(), pageKey, x, y, size: fontSize, color, text: "" };
    onAdd(b);
    onSelect(b.id);
  }

  // Set the editable text once on mount (uncontrolled), and focus new boxes so
  // typing starts immediately without Svelte re-rendering the node mid-edit.
  function initBox(node: HTMLElement, text: string) {
    node.textContent = text;
    queueMicrotask(() => {
      node.focus();
      if (text) {
        // Pre-filled (edit) box — select all so typing replaces it.
        const range = document.createRange();
        range.selectNodeContents(node);
        const sel = window.getSelection();
        sel?.removeAllRanges();
        sel?.addRange(range);
      }
    });
    return {};
  }
</script>

<div class="tb-layer">
  {#if tool === "text"}
    <div class="tb-capture" onpointerdown={place}></div>
  {/if}

  {#each boxes as b (b.id)}
    <div
      class="tb-box"
      class:sel={b.id === selectedId}
      class:cover={b.origText !== undefined}
      style={`left:${b.x * scale}px; top:${b.y * scale}px; font-size:${b.size * scale}px; color:${b.color};${
        b.origText !== undefined
          ? ` min-width:${(b.w ?? 0) * scale}px; min-height:${(b.h ?? b.size) * scale}px;`
          : ""
      }`}
      contenteditable="true"
      spellcheck="false"
      role="textbox"
      tabindex="0"
      use:initBox={b.text}
      onpointerdown={(e) => {
        e.stopPropagation();
        onSelect(b.id);
      }}
      oninput={(e) => onEdit(b.id, (e.target as HTMLElement).textContent ?? "")}
    ></div>
  {/each}
</div>

<style>
  .tb-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 6;
  }
  .tb-capture {
    position: absolute;
    inset: 0;
    pointer-events: all;
    cursor: text;
  }
  .tb-box {
    position: absolute;
    pointer-events: all;
    min-width: 8px;
    outline: none;
    line-height: 1.15;
    white-space: pre-wrap;
    font-family: Helvetica, Arial, sans-serif;
    cursor: text;
  }
  .tb-box:focus,
  .tb-box.sel {
    box-shadow: 0 0 0 1.5px var(--color-accent);
    border-radius: 2px;
  }
  /* Edit-in-place boxes cover the original run so the preview reads cleanly. */
  .tb-box.cover {
    background: #fff;
    display: inline-flex;
    align-items: center;
  }
</style>
