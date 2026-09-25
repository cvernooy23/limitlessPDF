<script lang="ts">
  import {
    listCertificates,
    signPdf,
    createStampPdf,
    pickSavePath,
    type CertInfo,
    type SignRect,
  } from "./api";

  interface Props {
    filePath: string;
    sourcePassword?: string;
    totalPages: number;
    onClose: () => void;
    /** Called after a stamp PDF is created so the parent can insert it. */
    onStampCreated: (tempPath: string) => void;
  }

  let { filePath, sourcePassword, totalPages, onClose, onStampCreated }: Props = $props();

  // ── Tab state ──────────────────────────────────────────────────────────

  let activeTab = $state<"certificate" | "typed">("certificate");

  // ── Certificate tab ────────────────────────────────────────────────────

  let certs = $state<CertInfo[]>([]);
  let certsLoading = $state(false);
  let certsError = $state("");
  let selectedThumbprint = $state("");

  let sigPage = $state(1);
  let sigPosition = $state<"bottom-left" | "bottom-center" | "bottom-right">("bottom-left");
  let sigReason = $state("");
  let sigLocation = $state("");
  let sigFieldName = $state("Signature1");
  let signing = $state(false);
  let signError = $state("");
  let signSuccess = $state("");

  async function loadCerts() {
    certsLoading = true;
    certsError = "";
    try {
      certs = await listCertificates();
      if (certs.length > 0 && !selectedThumbprint) {
        const withKey = certs.find((c) => c.hasPrivateKey);
        if (withKey) selectedThumbprint = withKey.thumbprint;
      }
    } catch (e) {
      certsError = String(e);
    } finally {
      certsLoading = false;
    }
  }

  function positionToRect(pos: "bottom-left" | "bottom-center" | "bottom-right"): SignRect {
    const w = 200;
    const h = 50;
    const margin = 36;
    const pageW = 612; // US Letter default
    switch (pos) {
      case "bottom-left":
        return { x: margin, y: margin, width: w, height: h };
      case "bottom-center":
        return { x: (pageW - w) / 2, y: margin, width: w, height: h };
      case "bottom-right":
        return { x: pageW - w - margin, y: margin, width: w, height: h };
    }
  }

  async function handleSign() {
    if (!selectedThumbprint) {
      signError = "Select a certificate first";
      return;
    }
    signing = true;
    signError = "";
    signSuccess = "";
    try {
      const dest = await pickSavePath("signed.pdf");
      if (!dest) {
        signing = false;
        return;
      }
      const cert = certs.find((c) => c.thumbprint === selectedThumbprint);
      const rect = positionToRect(sigPosition);
      await signPdf(
        filePath,
        dest,
        sigPage,
        rect,
        selectedThumbprint,
        sigFieldName,
        sigReason || undefined,
        sigLocation || undefined,
        cert?.subject || undefined,
        sourcePassword,
      );
      signSuccess = `Signed PDF saved successfully`;
    } catch (e) {
      signError = String(e);
    } finally {
      signing = false;
    }
  }

  // ── Typed signature tab ────────────────────────────────────────────────

  let typedName = $state("");
  let stampCreating = $state(false);
  let stampError = $state("");

  async function handleCreateStamp() {
    if (!typedName.trim()) {
      stampError = "Enter a name first";
      return;
    }
    stampCreating = true;
    stampError = "";
    try {
      // Render the name to a canvas and get PNG bytes
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d")!;
      const fontSize = 48;
      ctx.font = `italic ${fontSize}px "Segoe Script", "Brush Script MT", cursive`;
      const metrics = ctx.measureText(typedName);
      const textWidth = Math.ceil(metrics.width) + 40;
      const textHeight = fontSize + 30;

      canvas.width = textWidth;
      canvas.height = textHeight;

      // Transparent background
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      // Draw text
      ctx.font = `italic ${fontSize}px "Segoe Script", "Brush Script MT", cursive`;
      ctx.fillStyle = "#1a1a2e";
      ctx.textBaseline = "middle";
      ctx.fillText(typedName, 20, textHeight / 2);

      // Get PNG bytes
      const blob = await new Promise<Blob>((resolve) =>
        canvas.toBlob((b) => resolve(b!), "image/png"),
      );
      const arrayBuf = await blob.arrayBuffer();
      const pngBytes = Array.from(new Uint8Array(arrayBuf));

      const tempPath = await createStampPdf(pngBytes);
      onStampCreated(tempPath);
      onClose();
    } catch (e) {
      stampError = String(e);
    } finally {
      stampCreating = false;
    }
  }

  // Load certificates on mount
  $effect(() => {
    if (activeTab === "certificate") {
      loadCerts();
    }
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="sign-overlay"
  role="dialog"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="sign-dialog glass">
    <div class="sign-header">
      <h2>Digital Signature</h2>
      <button class="close-btn" onclick={onClose} title="Close">✕</button>
    </div>

    <!-- Tabs -->
    <div class="tab-bar">
      <button
        class="tab-btn"
        class:active={activeTab === "certificate"}
        onclick={() => (activeTab = "certificate")}
      >
        Certificate
      </button>
      <button
        class="tab-btn"
        class:active={activeTab === "typed"}
        onclick={() => (activeTab = "typed")}
      >
        Typed Signature
      </button>
    </div>

    <!-- Certificate tab -->
    {#if activeTab === "certificate"}
      <div class="tab-content">
        {#if certsLoading}
          <p class="loading">Loading certificates...</p>
        {:else if certsError}
          <p class="error">{certsError}</p>
        {:else if certs.length === 0}
          <p class="empty">No certificates found in the system store.</p>
        {:else}
          <div class="cert-list">
            {#each certs as cert}
              <label class="cert-item" class:selected={selectedThumbprint === cert.thumbprint}>
                <input
                  type="radio"
                  name="cert"
                  value={cert.thumbprint}
                  bind:group={selectedThumbprint}
                />
                <div class="cert-info">
                  <span class="cert-subject">{cert.subject}</span>
                  <span class="cert-issuer">Issued by: {cert.issuer}</span>
                  {#if !cert.hasPrivateKey}
                    <span class="cert-warn">No private key</span>
                  {/if}
                </div>
              </label>
            {/each}
          </div>
        {/if}

        <div class="form-grid">
          <label>
            Page
            <input type="number" bind:value={sigPage} min="1" max={totalPages} />
          </label>
          <label>
            Position
            <select bind:value={sigPosition}>
              <option value="bottom-left">Bottom Left</option>
              <option value="bottom-center">Bottom Center</option>
              <option value="bottom-right">Bottom Right</option>
            </select>
          </label>
          <label>
            Field Name
            <input type="text" bind:value={sigFieldName} />
          </label>
          <label>
            Reason
            <input type="text" bind:value={sigReason} placeholder="e.g. Approval" />
          </label>
          <label>
            Location
            <input type="text" bind:value={sigLocation} placeholder="e.g. Office" />
          </label>
        </div>

        {#if signError}
          <p class="error">{signError}</p>
        {/if}
        {#if signSuccess}
          <p class="success">{signSuccess}</p>
        {/if}

        <div class="actions">
          <button
            class="glass glass-hover sign-btn"
            onclick={handleSign}
            disabled={signing || !selectedThumbprint}
          >
            {signing ? "Signing..." : "Sign PDF"}
          </button>
        </div>
      </div>
    {/if}

    <!-- Typed signature tab -->
    {#if activeTab === "typed"}
      <div class="tab-content">
        <label class="name-label">
          Your Name
          <input
            type="text"
            bind:value={typedName}
            placeholder="Type your full name"
            class="name-input"
          />
        </label>

        {#if typedName.trim()}
          <div class="preview-area">
            <span class="preview-label">Preview</span>
            <div class="cursive-preview">
              {typedName}
            </div>
          </div>
        {/if}

        {#if stampError}
          <p class="error">{stampError}</p>
        {/if}

        <div class="actions">
          <button
            class="glass glass-hover sign-btn"
            onclick={handleCreateStamp}
            disabled={stampCreating || !typedName.trim()}
          >
            {stampCreating ? "Creating..." : "Insert Signature"}
          </button>
        </div>
        <p class="hint">
          Creates a cursive image of your name and inserts it as a new page. You can then rearrange
          it in the document.
        </p>
      </div>
    {/if}
  </div>
</div>

<style>
  .sign-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
  }
  .sign-dialog {
    width: min(520px, 90vw);
    max-height: 80vh;
    overflow-y: auto;
    border-radius: 16px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .sign-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sign-header h2 {
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--color-ink);
    margin: 0;
  }
  .close-btn {
    background: none;
    border: none;
    color: var(--color-ink-dim);
    font-size: 1.2rem;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 8px;
  }
  .close-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-ink);
  }

  /* Tabs */
  .tab-bar {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    padding-bottom: 8px;
  }
  .tab-btn {
    background: none;
    border: none;
    color: var(--color-ink-dim);
    padding: 6px 14px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    transition: all 0.15s;
  }
  .tab-btn:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .tab-btn.active {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-ink);
    font-weight: 500;
  }

  .tab-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* Cert list */
  .cert-list {
    max-height: 180px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cert-item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    cursor: pointer;
    transition: background 0.15s;
    background: rgba(255, 255, 255, 0.03);
  }
  .cert-item:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .cert-item.selected {
    background: rgba(70, 211, 154, 0.1);
    outline: 1px solid rgba(70, 211, 154, 0.3);
  }
  .cert-item input[type="radio"] {
    margin-top: 3px;
    accent-color: #46d39a;
  }
  .cert-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cert-subject {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--color-ink);
  }
  .cert-issuer {
    font-size: 0.75rem;
    color: var(--color-ink-dim);
  }
  .cert-warn {
    font-size: 0.7rem;
    color: #f0a73a;
  }

  /* Form */
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .form-grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.78rem;
    color: var(--color-ink-dim);
  }
  .form-grid input,
  .form-grid select {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 6px 10px;
    color: var(--color-ink);
    font-size: 0.82rem;
  }
  .form-grid input:focus,
  .form-grid select:focus {
    outline: none;
    border-color: rgba(70, 211, 154, 0.4);
  }

  /* Typed signature */
  .name-label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--color-ink-dim);
  }
  .name-input {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 10px;
    padding: 10px 14px;
    color: var(--color-ink);
    font-size: 0.95rem;
  }
  .name-input:focus {
    outline: none;
    border-color: rgba(70, 211, 154, 0.4);
  }
  .preview-area {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .preview-label {
    font-size: 0.75rem;
    color: var(--color-ink-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .cursive-preview {
    font-family: "Segoe Script", "Brush Script MT", cursive;
    font-size: 2rem;
    font-style: italic;
    color: var(--color-ink);
    padding: 16px 20px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 10px;
    border: 1px dashed rgba(255, 255, 255, 0.12);
    text-align: center;
    min-height: 60px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Messages */
  .loading,
  .empty {
    font-size: 0.85rem;
    color: var(--color-ink-dim);
    text-align: center;
    padding: 16px;
  }
  .error {
    font-size: 0.82rem;
    color: #e74c3c;
    margin: 0;
  }
  .success {
    font-size: 0.82rem;
    color: #46d39a;
    margin: 0;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--color-ink-dim);
    margin: 0;
    text-align: center;
  }

  /* Actions */
  .actions {
    display: flex;
    justify-content: flex-end;
    padding-top: 4px;
  }
  .sign-btn {
    padding: 8px 20px;
    border-radius: 10px;
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--color-ink);
    cursor: pointer;
    border: none;
  }
  .sign-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
