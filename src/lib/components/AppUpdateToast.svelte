<script lang="ts">
  import { X } from "@lucide/svelte";
  import { updaterPhase, updaterVersion, installUpdate } from "../stores/updater";

  let dismissed = $state(false);
  let lastVersion = $state<string | null>(null);

  // Re-show if a different (newer) version turns up after a dismiss.
  $effect(() => {
    if ($updaterVersion && $updaterVersion !== lastVersion) {
      dismissed = false;
      lastVersion = $updaterVersion;
    }
  });

  let show = $derived(
    ($updaterPhase === "available" || $updaterPhase === "downloading") && !dismissed,
  );
</script>

{#if show}
  <div class="app-update">
    {#if $updaterPhase === "downloading"}
      <span class="txt">Downloading Ncy update…</span>
    {:else}
      <span class="txt">Ncy <strong>v{$updaterVersion}</strong> is available.</span>
      <button type="button" class="update-btn" onclick={installUpdate}>Update</button>
      <button type="button" class="x" onclick={() => (dismissed = true)} aria-label="Dismiss">
        <X size={15} />
      </button>
    {/if}
  </div>
{/if}

<style>
  .app-update {
    position: fixed;
    left: 18px;
    bottom: 18px;
    z-index: 45;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 10px 10px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    max-width: min(380px, calc(100vw - 36px));
  }
  .txt {
    flex: 1;
    min-width: 0;
    font-size: 0.88rem;
    line-height: 1.4;
  }
  .update-btn {
    flex-shrink: 0;
    padding: 6px 14px;
    font-size: 0.84rem;
    background: var(--accent);
    color: var(--accent-contrast);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .x {
    display: inline-flex;
    flex-shrink: 0;
    padding: 4px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    line-height: 0;
    cursor: pointer;
  }
  .x:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
</style>
