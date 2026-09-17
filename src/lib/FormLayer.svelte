<script lang="ts">
  import type { FormField } from "./pdf";
  import type { Tool } from "./annotations";

  const {
    pageKey,
    scale,
    tool = "none",
    fields,
    onChange,
  }: {
    pageKey: string;
    scale: number;
    tool?: Tool;
    fields: FormField[];
    onChange: (field: FormField, value: string) => void;
  } = $props();

  // Only intercept pointer events while no annotation tool is active, so
  // highlighting/drawing/etc. still work over the page.
  const active = $derived(tool === "none");

  function box(f: FormField) {
    return `left:${f.x * scale}px; top:${f.y * scale}px; width:${f.w * scale}px; height:${f.h * scale}px;`;
  }
</script>

<div class="form-layer" class:active>
  {#each fields as f (f.id)}
    {#if f.kind === "text"}
      {#if f.multiline}
        <textarea
          class="ff ff-text"
          style={box(f)}
          style:font-size={`${Math.max(8, f.fontSize) * scale}px`}
          readonly={f.readOnly}
          maxlength={f.maxLen ?? undefined}
          value={f.value}
          oninput={(e) => onChange(f, (e.target as HTMLTextAreaElement).value)}
          onpointerdown={(e) => e.stopPropagation()}
        ></textarea>
      {:else}
        <input
          class="ff ff-text"
          style={box(f)}
          style:font-size={`${Math.max(8, f.fontSize) * scale}px`}
          readonly={f.readOnly}
          maxlength={f.maxLen ?? undefined}
          value={f.value}
          oninput={(e) => onChange(f, (e.target as HTMLInputElement).value)}
          onpointerdown={(e) => e.stopPropagation()}
        />
      {/if}
    {:else if f.kind === "checkbox"}
      <button
        class="ff ff-check"
        class:on={!!f.value}
        style={box(f)}
        title={f.fieldName}
        onpointerdown={(e) => e.stopPropagation()}
        onclick={() => onChange(f, f.value ? "" : f.onState)}
      >
        {#if f.value}✕{/if}
      </button>
    {:else if f.kind === "radio"}
      <button
        class="ff ff-radio"
        class:on={!!f.value}
        style={box(f)}
        title={f.fieldName}
        onpointerdown={(e) => e.stopPropagation()}
        onclick={() => onChange(f, f.value ? "" : f.onState)}
        aria-label={f.fieldName}
      ></button>
    {:else}
      <select
        class="ff ff-select"
        style={box(f)}
        style:font-size={`${Math.max(8, Math.min(f.fontSize, f.h * 0.7)) * scale}px`}
        disabled={f.readOnly}
        value={f.value}
        onpointerdown={(e) => e.stopPropagation()}
        onchange={(e) => onChange(f, (e.target as HTMLSelectElement).value)}
      >
        <option value=""></option>
        {#each f.options as o}
          <option value={o.value}>{o.label}</option>
        {/each}
      </select>
    {/if}
  {/each}
</div>

<style>
  .form-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 6;
  }
  .form-layer.active .ff {
    pointer-events: all;
  }
  .ff {
    position: absolute;
    margin: 0;
    box-sizing: border-box;
    /* A faint tint so users can see which areas are fillable. */
    background: rgba(110, 168, 255, 0.12);
    border: 1px solid rgba(110, 168, 255, 0.45);
    border-radius: 2px;
    color: #111;
    font-family: Helvetica, Arial, sans-serif;
    outline: none;
  }
  .ff:focus {
    background: rgba(110, 168, 255, 0.18);
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(110, 168, 255, 0.35);
  }
  .ff-text {
    padding: 0 3px;
    line-height: 1.05;
    resize: none;
    overflow: hidden;
  }
  textarea.ff-text {
    padding: 2px 3px;
    line-height: 1.15;
  }
  .ff-check,
  .ff-radio {
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-weight: 700;
    line-height: 1;
  }
  .ff-radio {
    border-radius: 50%;
  }
  .ff-radio.on::after {
    content: "";
    width: 58%;
    height: 58%;
    border-radius: 50%;
    background: #111;
  }
  .ff-check.on {
    color: #111;
  }
  .ff-select {
    padding: 0 2px;
  }
</style>
