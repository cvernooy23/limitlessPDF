<script lang="ts">
  import {
    listCertificates,
    signPdf,
    pickSavePath,
    isMobile,
    stageOutPath,
    copyOut,
    type CertInfo,
    type SignRect,
  } from "./api";

  interface Props {
    filePath: string;
    sourcePassword?: string;
    totalPages: number;
    onClose: () => void;
    /** Called with the signature PNG (data URL) + aspect ratio so the parent
     *  can enter placement mode and drop it onto a page. */
    onSignatureReady: (image: string, aspect: number) => void;
  }

  let { filePath, sourcePassword, totalPages, onClose, onSignatureReady }: Props = $props();

  // ── Tab state ──────────────────────────────────────────────────────────

  let activeTab = $state<"certificate" | "typed">("certificate");

  // Platform: on mobile we don't auto-launch the credential chooser, and we
  // offer an NFC security-key option alongside the device KeyChain.
  let mobile = $state(false);
  let platformKnown = $state(false);
  isMobile().then((m) => {
    mobile = m;
    platformKnown = true;
  });

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

  async function signNfc() {
    signing = true;
    signError = "";
    signSuccess = "";
    try {
      const userDest = await pickSavePath("signed.pdf");
      if (!userDest) {
        signing = false;
        return;
      }
      const dest = await stageOutPath("signed.pdf");
      const rect = positionToRect(sigPosition);
      await signPdf(
        filePath,
        dest,
        sigPage,
        rect,
        "nfc",
        sigFieldName,
        sigReason || undefined,
        sigLocation || undefined,
        undefined,
        sourcePassword,
      );
      await copyOut(dest, userDest);
      signSuccess = `Signed PDF saved successfully`;
    } catch (e) {
      signError = String(e);
    } finally {
      signing = false;
    }
  }

  async function signUsb() {
    signing = true;
    signError = "";
    signSuccess = "";
    try {
      const userDest = await pickSavePath("signed.pdf");
      if (!userDest) {
        signing = false;
        return;
      }
      const dest = await stageOutPath("signed.pdf");
      const rect = positionToRect(sigPosition);
      await signPdf(
        filePath,
        dest,
        sigPage,
        rect,
        "usb",
        sigFieldName,
        sigReason || undefined,
        sigLocation || undefined,
        undefined,
        sourcePassword,
      );
      await copyOut(dest, userDest);
      signSuccess = `Signed PDF saved successfully`;
    } catch (e) {
      signError = String(e);
    } finally {
      signing = false;
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
      const userDest = await pickSavePath("signed.pdf");
      if (!userDest) {
        signing = false;
        return;
      }
      // On mobile the picked destination is a content:// URI the Rust core
      // can't write to directly, so sign to a staged path and copy it out.
      const mobile = await isMobile();
      const dest = mobile ? await stageOutPath("signed.pdf") : userDest;
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
      if (mobile) await copyOut(dest, userDest);
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
      ctx.fillStyle =
        getComputedStyle(document.documentElement).getPropertyValue("--signature-ink").trim() ||
        "#1a1a2e";
      ctx.textBaseline = "middle";
      ctx.fillText(typedName, 20, textHeight / 2);

      // Hand the transparent PNG to the parent for tap-to-place on a page.
      const dataUrl = canvas.toDataURL("image/png");
      const aspect = canvas.width / canvas.height;
      onSignatureReady(dataUrl, aspect);
      onClose();
    } catch (e) {
      stampError = String(e);
    } finally {
      stampCreating = false;
    }
  }

  // Load certificates on mount — desktop only. On mobile the user explicitly
  // picks a credential source (device KeyChain or an NFC security key).
  $effect(() => {
    if (activeTab === "certificate" && platformKnown && !mobile) {
      loadCerts();
    }
  });
</script>

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
        {#if mobile}
          <button class="glass glass-hover sign-btn" onclick={loadCerts} disabled={certsLoading}>
            {certsLoading ? "Opening chooser..." : "Choose device credential"}
          </button>
          {#if certsError}
            <p class="error">{certsError}</p>
          {/if}
          {#if certs.length > 0}
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
                  </div>
                </label>
              {/each}
            </div>
          {/if}
        {:else if certsLoading}
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
            {signing ? "Signing..." : mobile ? "Sign with device credential" : "Sign PDF"}
          </button>
          {#if mobile}
            <button class="glass glass-hover sign-btn" onclick={signNfc} disabled={signing}>
              {signing ? "Signing..." : "Sign with NFC security key"}
            </button>
            <button class="glass glass-hover sign-btn" onclick={signUsb} disabled={signing}>
              {signing ? "Signing..." : "Sign with USB card (CAC)"}
            </button>
          {/if}
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
            {stampCreating ? "Preparing..." : "Place Signature"}
          </button>
        </div>
        <p class="hint">
          Creates a cursive image of your name. Tap a page to drop it, then drag to reposition or
          use the corner handle to resize.
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
    background: var(--surface-overlay);
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
    background: var(--surface-sunken);
    color: var(--color-ink);
  }

  /* Tabs */
  .tab-bar {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--divider);
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
    background: var(--surface-sunken);
  }
  .tab-btn.active {
    background: var(--glass-hover-bg);
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
    background: var(--surface-sunken);
  }
  .cert-item:hover {
    background: var(--surface-sunken);
  }
  .cert-item.selected {
    background: rgba(70, 211, 154, 0.1);
    outline: 1px solid rgba(70, 211, 154, 0.3);
  }
  .cert-item input[type="radio"] {
    margin-top: 3px;
    accent-color: var(--ok);
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
    color: var(--warn);
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
    background: var(--surface-sunken);
    border: 1px solid var(--divider);
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
    background: var(--surface-sunken);
    border: 1px solid var(--divider);
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
    color: var(--signature-ink);
    padding: 16px 20px;
    background: var(--signature-paper);
    border-radius: 10px;
    border: 1px solid rgba(15, 23, 42, 0.15);
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
    color: var(--danger);
    margin: 0;
  }
  .success {
    font-size: 0.82rem;
    color: var(--ok);
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
