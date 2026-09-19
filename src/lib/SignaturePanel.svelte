<script lang="ts">
  import { checkSignatures, type SignatureInfo } from "./api";

  const {
    filePath,
    sourcePassword,
    onClose,
  }: {
    filePath: string;
    sourcePassword?: string;
    onClose?: () => void;
  } = $props();

  let signatures = $state<SignatureInfo[] | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  $effect(() => {
    loading = true;
    error = null;
    checkSignatures(filePath, sourcePassword)
      .then((sigs) => {
        signatures = sigs;
      })
      .catch((e) => {
        error = `Could not read signatures: ${e instanceof Error ? e.message : String(e)}`;
        signatures = [];
      })
      .finally(() => {
        loading = false;
      });
  });

  function statusIcon(sig: SignatureInfo): string {
    if (sig.status === "signed") return "✓";
    if (sig.status === "modified") return "⚠";
    return "?";
  }

  function statusColor(sig: SignatureInfo): string {
    if (sig.status === "signed") return "#46d39a";
    if (sig.status === "modified") return "#f0a73a";
    return "var(--color-ink-dim)";
  }

  function statusLabel(sig: SignatureInfo): string {
    if (sig.status === "signed") return "Signature intact";
    if (sig.status === "modified") return "Document modified after signing";
    return "Validity unknown";
  }
</script>

<div class="sig-panel glass">
  <div class="sig-header">
    <span class="sig-title">Digital Signatures</span>
    <button class="sig-close glass-hover" onclick={() => onClose?.()} title="Close signature panel">
      ✕
    </button>
  </div>

  {#if loading}
    <div class="sig-body dim">Checking signatures…</div>
  {:else if error}
    <div class="sig-body">
      <div class="sig-error">{error}</div>
    </div>
  {:else if !signatures || signatures.length === 0}
    <div class="sig-body dim">
      This document has no digital signatures.
    </div>
  {:else}
    <div class="sig-body">
      <div class="sig-count">
        <strong>{signatures.length}</strong> signature{signatures.length === 1 ? "" : "s"} found
      </div>

      {#each signatures as sig, i}
        <div class="sig-card">
          <div class="sig-status" style="color: {statusColor(sig)}">
            <span class="sig-status-icon">{statusIcon(sig)}</span>
            <span class="sig-status-label">{statusLabel(sig)}</span>
          </div>

          <div class="sig-details">
            {#if sig.signerName}
              <div class="sig-row">
                <span class="sig-label">Signed by</span>
                <span class="sig-value">{sig.signerName}</span>
              </div>
            {/if}

            {#if sig.signingTime}
              <div class="sig-row">
                <span class="sig-label">Date</span>
                <span class="sig-value">{sig.signingTime}</span>
              </div>
            {/if}

            {#if sig.reason}
              <div class="sig-row">
                <span class="sig-label">Reason</span>
                <span class="sig-value">{sig.reason}</span>
              </div>
            {/if}

            {#if sig.location}
              <div class="sig-row">
                <span class="sig-label">Location</span>
                <span class="sig-value">{sig.location}</span>
              </div>
            {/if}

            <div class="sig-row">
              <span class="sig-label">Field</span>
              <span class="sig-value mono">{sig.fieldName}</span>
            </div>

            {#if sig.subFilter}
              <div class="sig-row">
                <span class="sig-label">Type</span>
                <span class="sig-value mono">{sig.subFilter}</span>
              </div>
            {/if}
          </div>
        </div>
      {/each}

      <div class="sig-note">
        Byte-range coverage is checked to detect post-sign modifications.
        Full certificate chain validation is not performed.
      </div>
    </div>
  {/if}
</div>

<style>
  .sig-panel {
    display: flex;
    flex-direction: column;
    width: 300px;
    max-height: 100%;
    border-radius: 16px;
    overflow: hidden;
  }
  .sig-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .sig-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .sig-close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--color-ink-dim);
    border: 1px solid transparent;
  }
  .sig-body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: 12px;
    color: var(--color-ink);
  }
  .sig-body.dim {
    color: var(--color-ink-dim);
    justify-content: center;
    text-align: center;
    padding: 28px 14px;
  }
  .sig-error {
    color: #f0a73a;
    font-size: 11px;
    padding: 6px 8px;
    border-radius: 8px;
    background: rgba(240, 167, 58, 0.08);
  }
  .sig-count strong {
    color: var(--color-accent);
  }
  .sig-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--color-glass-stroke);
    background: rgba(255, 255, 255, 0.04);
  }
  .sig-status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
  }
  .sig-status-icon {
    font-size: 14px;
  }
  .sig-details {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sig-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .sig-label {
    font-size: 11px;
    color: var(--color-ink-dim);
    flex-shrink: 0;
  }
  .sig-value {
    font-size: 11px;
    color: var(--color-ink);
    text-align: right;
    word-break: break-word;
  }
  .sig-value.mono {
    font-family: monospace;
    font-size: 10px;
    opacity: 0.7;
  }
  .sig-note {
    font-size: 10px;
    color: var(--color-ink-dim);
    line-height: 1.5;
    padding: 6px 8px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    opacity: 0.7;
  }
</style>
