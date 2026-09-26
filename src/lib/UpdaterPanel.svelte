<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  const {
    currentVersion,
    onClose,
  }: {
    currentVersion: string;
    onClose?: () => void;
  } = $props();

  type Channel = "stable" | "beta" | "alpha";

  interface UpdateInfo {
    version: string;
    body: string | null;
    date: string | null;
  }

  let channel = $state<Channel>(
    (localStorage.getItem("update-channel") as Channel) || "stable",
  );
  let checking = $state(false);
  let installing = $state(false);
  let update = $state<UpdateInfo | null>(null);
  let error = $state<string | null>(null);
  let progress = $state<{ downloaded: number; total: number | null }>({
    downloaded: 0,
    total: null,
  });
  let installed = $state(false);

  const channelLabels: Record<Channel, string> = {
    stable: "Stable",
    beta: "Beta",
    alpha: "Alpha",
  };
  const channelDescriptions: Record<Channel, string> = {
    stable: "Production releases from main",
    beta: "Pre-release builds from dev",
    alpha: "Nightly builds from latest",
  };

  function setChannel(ch: Channel) {
    channel = ch;
    localStorage.setItem("update-channel", ch);
    update = null;
    error = null;
  }

  async function checkForUpdate() {
    checking = true;
    error = null;
    update = null;
    try {
      const result: UpdateInfo | null = await invoke("check_for_update", {
        channel,
      });
      update = result;
    } catch (e) {
      error = String(e);
    } finally {
      checking = false;
    }
  }

  async function installUpdate() {
    if (!update) return;
    installing = true;
    error = null;
    progress = { downloaded: 0, total: null };
    try {
      const unlisten = await listen<{ chunk: number; total: number | null }>(
        "update-progress",
        (event) => {
          progress.downloaded += event.payload.chunk;
          if (event.payload.total) progress.total = event.payload.total;
        },
      );
      await invoke("download_and_install_update", { channel });
      unlisten();
      installed = true;
    } catch (e) {
      error = String(e);
    } finally {
      installing = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(() => {
    checkForUpdate();
  });
</script>

<div class="updater-panel glass">
  <div class="updater-header">
    <span class="updater-title">Updates</span>
    <button
      class="updater-close glass-hover"
      onclick={() => onClose?.()}
      title="Close update panel"
    >
      &#10005;
    </button>
  </div>

  <div class="updater-body">
    <div class="channel-section">
      <div class="section-label">Update channel</div>
      <div class="channel-list">
        {#each (["stable", "beta", "alpha"] as Channel[]) as ch}
          <button
            class="channel-btn glass-hover"
            class:active={channel === ch}
            onclick={() => setChannel(ch)}
          >
            <span class="channel-name">{channelLabels[ch]}</span>
            <span class="channel-desc">{channelDescriptions[ch]}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="divider"></div>

    <div class="status-section">
      <div class="version-row">
        <span class="version-label">Current</span>
        <span class="version-value">v{currentVersion}</span>
      </div>

      {#if checking}
        <div class="status-msg">
          <span class="spinner-sm"></span>
          Checking for updates...
        </div>
      {:else if installed}
        <div class="status-msg success">
          Update installed. Restart the app to apply.
        </div>
      {:else if installing}
        <div class="status-msg">
          <span class="spinner-sm"></span>
          Downloading update...
          {#if progress.total}
            <div class="progress-bar">
              <div
                class="progress-fill"
                style="width: {(progress.downloaded / progress.total) * 100}%"
              ></div>
            </div>
            <span class="progress-text">
              {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
            </span>
          {:else if progress.downloaded > 0}
            <span class="progress-text">
              {formatBytes(progress.downloaded)}
            </span>
          {/if}
        </div>
      {:else if update}
        <div class="update-available">
          <div class="version-row">
            <span class="version-label">Available</span>
            <span class="version-value accent">v{update.version}</span>
          </div>
          {#if update.body}
            <div class="release-notes">{update.body}</div>
          {/if}
          <button class="update-btn glass-hover" onclick={installUpdate}>
            Download &amp; Install
          </button>
        </div>
      {:else if error}
        <div class="status-msg error">{error}</div>
      {:else}
        <div class="status-msg dim">
          You're on the latest {channelLabels[channel].toLowerCase()} version.
        </div>
      {/if}
    </div>

    {#if !checking && !installing && !installed}
      <button class="check-btn glass-hover" onclick={checkForUpdate}>
        Check now
      </button>
    {/if}
  </div>
</div>

<style>
  .updater-panel {
    display: flex;
    flex-direction: column;
    width: 300px;
    max-height: 100%;
    border-radius: 16px;
    overflow: hidden;
  }
  .updater-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .updater-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .updater-close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--color-ink-dim);
    border: 1px solid transparent;
  }
  .updater-body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 12px;
    color: var(--color-ink);
  }

  .section-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-ink-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 6px;
  }

  .channel-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .channel-btn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    text-align: left;
    width: 100%;
  }
  .channel-btn.active {
    background: rgba(255, 255, 255, 0.06);
    border-color: var(--color-accent);
  }
  .channel-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .channel-desc {
    font-size: 10px;
    color: var(--color-ink-dim);
  }

  .divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
  }

  .version-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
  }
  .version-label {
    color: var(--color-ink-dim);
  }
  .version-value {
    font-weight: 600;
    font-family: monospace;
  }
  .version-value.accent {
    color: var(--color-accent);
  }

  .status-msg {
    font-size: 12px;
    color: var(--color-ink-dim);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    text-align: center;
    padding: 8px 0;
  }
  .status-msg.dim {
    padding: 16px 0;
  }
  .status-msg.success {
    color: #57c785;
    font-weight: 600;
  }
  .status-msg.error {
    color: #f0a73a;
    word-break: break-word;
  }

  .spinner-sm {
    display: inline-block;
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.15);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .progress-bar {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--color-accent);
    transition: width 0.2s;
  }
  .progress-text {
    font-size: 10px;
    font-family: monospace;
    color: var(--color-ink-dim);
  }

  .update-available {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .release-notes {
    font-size: 11px;
    color: var(--color-ink-dim);
    line-height: 1.5;
    background: rgba(255, 255, 255, 0.04);
    padding: 8px 10px;
    border-radius: 8px;
    max-height: 120px;
    overflow-y: auto;
    white-space: pre-wrap;
  }

  .update-btn {
    width: 100%;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid var(--color-accent);
    background: rgba(var(--color-accent-rgb, 99, 102, 241), 0.15);
    color: var(--color-accent);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .check-btn {
    width: 100%;
    padding: 7px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: transparent;
    color: var(--color-ink-dim);
    font-size: 11px;
    cursor: pointer;
  }
</style>
