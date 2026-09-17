<script lang="ts">
  const { onOpen }: { onOpen?: () => void } = $props();
</script>

<div class="grid h-full place-items-center">
  <div class="glass sheen flex max-w-md flex-col items-center gap-5 rounded-3xl px-12 py-14 text-center">
    <div
      class="grid h-20 w-20 place-items-center rounded-2xl text-2xl font-bold"
      style="background: linear-gradient(135deg, var(--color-accent), var(--color-accent-2)); color: #0b0d14;"
    >
      lP
    </div>
    <div class="space-y-1">
      <h1 class="text-xl font-semibold">Open a PDF to begin</h1>
      <p class="text-sm text-[var(--color-ink-dim)]">
        Drag a file here, or browse to get started. View, annotate, fill forms, and export — all in one app.
      </p>
    </div>
    <button class="cta glass-hover" onclick={() => onOpen?.()}>Browse files</button>
  </div>
</div>

<style>
  .cta {
    padding: 10px 22px;
    border-radius: 12px;
    font-size: 0.9rem;
    font-weight: 600;
    color: #0b0d14;
    background: linear-gradient(135deg, var(--color-accent), var(--color-accent-2));
    border: 1px solid rgba(255, 255, 255, 0.25);
    box-shadow: 0 8px 24px rgba(110, 168, 255, 0.35);
  }
  .cta:hover {
    filter: brightness(1.06);
  }
</style>
