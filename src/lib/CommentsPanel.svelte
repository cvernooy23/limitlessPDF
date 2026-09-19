<script lang="ts">
  import type { NoteAnnotation } from "./annotations";

  const {
    notes,
    selectedId = null,
    pageIndexOf,
    onSelect,
    onEdit,
    onDelete,
    onClose,
  }: {
    notes: NoteAnnotation[];
    selectedId?: string | null;
    pageIndexOf: (pageKey: string) => number;
    onSelect: (id: string) => void;
    onEdit: (id: string, text: string) => void;
    onDelete: (id: string) => void;
    onClose: () => void;
  } = $props();
</script>

<aside class="glass flex w-64 shrink-0 flex-col gap-2 rounded-2xl p-3">
  <div class="flex items-center justify-between px-1">
    <span class="text-xs font-medium uppercase tracking-wider text-[var(--color-ink-dim)]">
      Comments{#if notes.length}<span class="ml-1 normal-case opacity-70">({notes.length})</span
        >{/if}
    </span>
    <button class="closebtn glass-hover" onclick={onClose} aria-label="Close comments" title="Close"
      >✕</button
    >
  </div>

  <div class="flex flex-col gap-2 overflow-y-auto pr-1">
    {#if notes.length === 0}
      <p class="px-1 py-6 text-center text-xs text-[var(--color-ink-dim)]">
        No comments yet. Pick the Note tool and click on a page to add one.
      </p>
    {/if}

    {#each notes as note (note.id)}
      <div
        class="card"
        class:selected={note.id === selectedId}
        role="button"
        tabindex="0"
        onclick={() => onSelect(note.id)}
        onkeydown={(e) => e.key === "Enter" && onSelect(note.id)}
      >
        <div class="flex items-center justify-between">
          <span class="flex items-center gap-1.5 text-[11px] text-[var(--color-ink-dim)]">
            <span class="dot" style={`background:${note.color}`}></span>
            Page {pageIndexOf(note.pageKey)}
          </span>
          <button
            class="del glass-hover"
            onclick={(e) => {
              e.stopPropagation();
              onDelete(note.id);
            }}
            aria-label="Delete comment"
            title="Delete">🗑</button
          >
        </div>
        <textarea
          class="note-text"
          rows="2"
          placeholder="Write a comment…"
          value={note.text}
          oninput={(e) => onEdit(note.id, (e.target as HTMLTextAreaElement).value)}
          onclick={(e) => e.stopPropagation()}></textarea>
      </div>
    {/each}
  </div>
</aside>

<style>
  .closebtn,
  .del {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    color: var(--color-ink-dim);
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    font-size: 12px;
  }
  .closebtn:hover,
  .del:hover {
    color: var(--color-ink);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-radius: 12px;
    border: 1px solid var(--color-glass-stroke);
    background: rgba(255, 255, 255, 0.04);
    cursor: pointer;
  }
  .card.selected {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 1px var(--color-accent) inset;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    display: inline-block;
  }
  .note-text {
    width: 100%;
    resize: vertical;
    min-height: 36px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--color-glass-stroke);
    border-radius: 8px;
    padding: 6px 8px;
    color: var(--color-ink);
    font-size: 12px;
    outline: none;
  }
  .note-text:focus {
    border-color: var(--color-accent);
  }
</style>
