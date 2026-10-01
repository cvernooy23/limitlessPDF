<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { inTauri } from "./api";
  import { theme } from "./theme.svelte";

  const appWindow = inTauri() ? getCurrentWindow() : null;

  function minimize() {
    appWindow?.minimize();
  }
  function toggleMaximize() {
    appWindow?.toggleMaximize();
  }
  function close() {
    appWindow?.close();
  }
</script>

<!-- data-tauri-drag-region makes the empty bar draggable to move the window -->
<header
  data-tauri-drag-region
  class="flex h-11 shrink-0 items-center justify-between px-3 select-none"
>
  <div data-tauri-drag-region class="flex items-center gap-2 pointer-events-none">
    <div
      class="grid h-6 w-6 place-items-center rounded-md text-[11px] font-bold"
      style="background: linear-gradient(135deg, var(--color-accent), var(--color-accent-2)); color: #0b0d14;"
    >
      lP
    </div>
    <span class="text-sm font-medium tracking-wide text-[var(--color-ink)]">limitlessPDF</span>
    <span class="text-xs text-[var(--color-ink-dim)]">— Preview</span>
  </div>

  <div class="flex items-center gap-1.5">
    <button
      class="win-btn glass-hover"
      onclick={() => theme.toggle()}
      aria-label={theme.resolved === "dark" ? "Switch to light mode" : "Switch to dark mode"}
      title={theme.resolved === "dark" ? "Switch to light mode" : "Switch to dark mode"}
    >
      {#if theme.resolved === "dark"}
        <!-- sun: click to go light -->
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
          stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
        </svg>
      {:else}
        <!-- moon: click to go dark -->
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
          stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      {/if}
    </button>
    <button class="win-btn glass-hover" onclick={minimize} aria-label="Minimize" title="Minimize">
      <svg width="11" height="11" viewBox="0 0 11 11"
        ><rect y="5" width="11" height="1" fill="currentColor" /></svg
      >
    </button>
    <button
      class="win-btn glass-hover"
      onclick={toggleMaximize}
      aria-label="Maximize"
      title="Maximize"
    >
      <svg width="11" height="11" viewBox="0 0 11 11"
        ><rect x="0.5" y="0.5" width="10" height="10" fill="none" stroke="currentColor" /></svg
      >
    </button>
    <button class="win-btn win-close" onclick={close} aria-label="Close" title="Close">
      <svg width="11" height="11" viewBox="0 0 11 11"
        ><path d="M1 1l9 9M10 1l-9 9" stroke="currentColor" stroke-width="1.2" /></svg
      >
    </button>
  </div>
</header>

<style>
  .win-btn {
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    border-radius: 8px;
    color: var(--color-ink-dim);
    border: 1px solid transparent;
  }
  .win-btn:hover {
    color: var(--color-ink);
  }
  .win-close:hover {
    background: #e5484d;
    color: white;
  }
</style>
