<script lang="ts">
  import {
    checkOcrAvailable,
    detectScannedPages,
    runOcr,
    type OcrStatus,
    type OcrPageResult,
  } from "./api";

  const {
    filePath,
    sourcePassword,
    onDone,
    onClose,
  }: {
    filePath: string;
    sourcePassword?: string;
    onDone?: () => void;
    onClose?: () => void;
  } = $props();

  let status = $state<OcrStatus | null>(null);
  let scannedPages = $state<number[]>([]);
  let selectedPages = $state<Set<number>>(new Set());
  let language = $state("eng");
  let dpi = $state(300);
  let detecting = $state(false);
  let running = $state(false);
  let results = $state<OcrPageResult[] | null>(null);
  let error = $state<string | null>(null);

  // Check Tesseract availability on mount.
  $effect(() => {
    checkOcrAvailable().then((s) => {
      status = s;
      if (s.available && s.languages.length > 0 && !s.languages.includes(language)) {
        language = s.languages[0];
      }
    });
  });

  // Detect scanned pages when status becomes available.
  $effect(() => {
    if (!status?.available || detecting) return;
    detecting = true;
    detectScannedPages(filePath, sourcePassword)
      .then((pages) => {
        scannedPages = pages;
        selectedPages = new Set(pages);
      })
      .catch((e) => {
        error = `Detection failed: ${e}`;
      })
      .finally(() => {
        detecting = false;
      });
  });

  function togglePage(page: number) {
    const next = new Set(selectedPages);
    if (next.has(page)) next.delete(page);
    else next.add(page);
    selectedPages = next;
  }

  function selectAll() {
    selectedPages = new Set(scannedPages);
  }

  function selectNone() {
    selectedPages = new Set();
  }

  async function startOcr() {
    if (running || selectedPages.size === 0) return;
    running = true;
    error = null;
    results = null;
    try {
      const pages = Array.from(selectedPages).sort((a, b) => a - b);
      const res = await runOcr(filePath, pages, language, dpi, sourcePassword);
      results = res;
      onDone?.();
    } catch (e) {
      error = `OCR failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      running = false;
    }
  }

  const totalWords = $derived(
    results ? results.reduce((sum, r) => sum + r.wordCount, 0) : 0,
  );
</script>

<div class="ocr-panel glass">
  <div class="ocr-header">
    <span class="ocr-title">OCR — Scanned Pages</span>
    <button class="ocr-close glass-hover" onclick={() => onClose?.()} title="Close OCR panel">
      ✕
    </button>
  </div>

  {#if !status}
    <div class="ocr-body dim">Checking Tesseract…</div>
  {:else if !status.available}
    <div class="ocr-body">
      <div class="ocr-warn">Tesseract not found</div>
      <pre class="ocr-install">{status.message}</pre>
    </div>
  {:else if detecting}
    <div class="ocr-body dim">Scanning pages for text…</div>
  {:else if scannedPages.length === 0}
    <div class="ocr-body dim">
      No scanned pages detected — every page already has extractable text.
    </div>
  {:else}
    <div class="ocr-body">
      <div class="ocr-info">
        Found <strong>{scannedPages.length}</strong> scanned page{scannedPages.length === 1 ? "" : "s"}.
      </div>

      <div class="ocr-pages">
        <div class="ocr-pages-head">
          <span>Pages to OCR:</span>
          <button class="link" onclick={selectAll}>All</button>
          <button class="link" onclick={selectNone}>None</button>
        </div>
        <div class="ocr-page-list">
          {#each scannedPages as pg}
            <label class="ocr-page-item">
              <input
                type="checkbox"
                checked={selectedPages.has(pg)}
                onchange={() => togglePage(pg)}
              />
              <span>Page {pg}</span>
            </label>
          {/each}
        </div>
      </div>

      <div class="ocr-opts">
        <label class="ocr-opt">
          <span>Language</span>
          <select bind:value={language}>
            {#each status.languages as lang}
              <option value={lang}>{lang}</option>
            {/each}
          </select>
        </label>
        <label class="ocr-opt">
          <span>DPI</span>
          <select bind:value={dpi}>
            <option value={150}>150</option>
            <option value={300}>300</option>
            <option value={600}>600</option>
          </select>
        </label>
      </div>

      <button
        class="ocr-run"
        onclick={startOcr}
        disabled={running || selectedPages.size === 0}
      >
        {#if running}
          Running OCR…
        {:else}
          Run OCR on {selectedPages.size} page{selectedPages.size === 1 ? "" : "s"}
        {/if}
      </button>

      {#if error}
        <div class="ocr-error">{error}</div>
      {/if}

      {#if results}
        <div class="ocr-results">
          <div class="ocr-results-head">
            Done — {totalWords} word{totalWords === 1 ? "" : "s"} recognized.
            Reload the file to see searchable text.
          </div>
          {#each results as r}
            <div class="ocr-result-row">
              <span class="ocr-result-page">Page {r.page}</span>
              <span class="ocr-result-words">{r.wordCount} words</span>
            </div>
            {#if r.textPreview}
              <div class="ocr-result-preview">{r.textPreview}</div>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .ocr-panel {
    display: flex;
    flex-direction: column;
    width: 300px;
    max-height: 100%;
    border-radius: 16px;
    overflow: hidden;
  }
  .ocr-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .ocr-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .ocr-close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--color-ink-dim);
    border: 1px solid transparent;
  }
  .ocr-body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: 12px;
    color: var(--color-ink);
  }
  .ocr-body.dim {
    color: var(--color-ink-dim);
    justify-content: center;
    text-align: center;
    padding: 28px 14px;
  }
  .ocr-warn {
    font-weight: 600;
    color: #f0a73a;
  }
  .ocr-install {
    font-size: 11px;
    color: var(--color-ink-dim);
    white-space: pre-wrap;
    line-height: 1.5;
    background: rgba(255, 255, 255, 0.04);
    padding: 8px 10px;
    border-radius: 8px;
  }
  .ocr-info strong {
    color: var(--color-accent);
  }
  .ocr-pages-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--color-ink-dim);
  }
  .link {
    background: none;
    border: none;
    color: var(--color-accent);
    font-size: 11px;
    cursor: pointer;
    text-decoration: underline;
    padding: 0;
  }
  .ocr-page-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    max-height: 120px;
    overflow-y: auto;
    margin-top: 4px;
  }
  .ocr-page-item {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--color-ink);
    cursor: pointer;
  }
  .ocr-page-item input {
    accent-color: var(--color-accent);
  }
  .ocr-opts {
    display: flex;
    gap: 10px;
  }
  .ocr-opt {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    color: var(--color-ink-dim);
    flex: 1;
  }
  .ocr-opt select {
    padding: 5px 8px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: var(--color-ink);
    font-size: 12px;
    outline: none;
  }
  .ocr-opt select:focus {
    border-color: var(--color-accent);
  }
  .ocr-run {
    padding: 9px 0;
    border-radius: 10px;
    font-size: 13px;
    font-weight: 600;
    color: #0b0d14;
    background: linear-gradient(135deg, var(--color-accent), var(--color-accent-2));
    border: 1px solid rgba(255, 255, 255, 0.25);
    cursor: pointer;
  }
  .ocr-run:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .ocr-run:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .ocr-error {
    color: #f0a73a;
    font-size: 11px;
    padding: 6px 8px;
    border-radius: 8px;
    background: rgba(240, 167, 58, 0.08);
  }
  .ocr-results {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ocr-results-head {
    font-size: 12px;
    font-weight: 600;
    color: #46d39a;
  }
  .ocr-result-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
  }
  .ocr-result-page {
    color: var(--color-ink);
  }
  .ocr-result-words {
    color: var(--color-ink-dim);
  }
  .ocr-result-preview {
    font-size: 11px;
    color: var(--color-ink-dim);
    line-height: 1.4;
    max-height: 60px;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 4px 6px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 6px;
  }
</style>
