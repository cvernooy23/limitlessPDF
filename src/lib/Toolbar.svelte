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
    activeTool = "none",
    hasDoc = false,
  }: {
    onOpen?: () => void;
    onSearch?: () => void;
    onTool?: (mode: AnnoTool) => void;
    activeTool?: AnnoTool;
    hasDoc?: boolean;
  } = $props();

  const groups: ToolDef[][] = [
    [{ id: "open", label: "Open", icon: "M4 4h6l2 2h8v12H4z", enabled: true }],
    [
      { id: "select", label: "Select", icon: "M5 3l14 7-6 1-1 6z", enabled: true, mode: "none" },
      { id: "search", label: "Search", icon: "M10 4a6 6 0 104 10l5 5 1-1-5-5A6 6 0 0010 4z", enabled: true },
    ],
    [
      { id: "highlight", label: "Highlight", icon: "M4 16l8-8 4 4-8 8H4z", enabled: true, mode: "highlight" },
      { id: "note", label: "Note", icon: "M5 4h14v10l-4 4H5z", enabled: true, mode: "note" },
      { id: "draw", label: "Draw", icon: "M4 18l9-9 2 2-9 9H4z", enabled: true, mode: "draw" },
    ],
    [
      { id: "text", label: "Text", icon: "M5 5h14M12 5v14M9 19h6", enabled: true, mode: "text" },
      { id: "edit", label: "Edit text", icon: "M5 16l9-9 3 3-9 9H5z", enabled: true, mode: "edittext" },
    ],
  ];

  function isEnabled(t: ToolDef): boolean {
    if (t.enabled === undefined) return false;
    // Annotation/search tools require an open document.
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
    else if (t.mode) onTool?.(t.mode);
  }
</script>

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
          <path d={tool.icon} stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" />
        </svg>
        <span class="text-xs">{tool.label}</span>
      </button>
    {/each}
  {/each}
</div>

<style>
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
</style>
