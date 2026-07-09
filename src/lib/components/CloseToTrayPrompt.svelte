<script lang="ts">
  import { tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { closePromptOpen, patchDesktopPrefs } from "../stores/tray";
  import { MinusCircle } from "@lucide/svelte";

  let dontAsk = $state(false);
  let dialog = $state<HTMLDivElement | null>(null);
  let busy = $state(false);
  let previousFocus: HTMLElement | null = null;

  /** Persist the choice, then act on it. A failed write shouldn't strand the
   *  user in a dialog they can't dismiss, so the window action runs regardless. */
  async function commit(patch: Parameters<typeof patchDesktopPrefs>[0]) {
    if (busy) return;
    busy = true;
    try {
      await patchDesktopPrefs({ ...patch, ...(dontAsk ? { askCloseToTray: false } : {}) });
    } catch (e) {
      console.error("could not save tray preference", e);
    } finally {
      busy = false;
      closePromptOpen.set(false);
    }
  }

  // Keep running in the tray, and make that the default for later closes.
  async function minimize() {
    await commit({ closeToTray: true });
    await getCurrentWindow().hide();
  }

  // `destroy` skips the CloseRequested handler that opened this dialog, so the
  // window actually goes away instead of re-prompting.
  async function quit() {
    await commit({});
    await getCurrentWindow().destroy();
  }

  function cancel() {
    if (busy) return;
    closePromptOpen.set(false);
  }

  $effect(() => {
    if (!$closePromptOpen) return;
    previousFocus = document.activeElement as HTMLElement | null;
    void tick().then(() => dialog?.querySelector<HTMLElement>("[data-cancel]")?.focus());
    return () => previousFocus?.focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (!$closePromptOpen) return;
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
      return;
    }
    if (e.key !== "Tab" || !dialog) return;
    const items = Array.from(dialog.querySelectorAll<HTMLElement>("button, input"));
    const first = items[0];
    const last = items.at(-1);
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last?.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first?.focus();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if $closePromptOpen}
  <div class="backdrop">
    <div
      class="modal card col"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ctp-title"
      bind:this={dialog}
    >
      <div class="row" style="align-items:center; gap:.5rem;">
        <MinusCircle size={20} strokeWidth={2.25} />
        <h2 id="ctp-title" style="margin:0; font-size:1.05rem;">Keep Ncy running in the tray?</h2>
      </div>
      <p class="muted tiny" style="margin:0;">
        Ncy stays in the notification area, so reminders still fire and paired
        devices keep syncing. Reopen it any time from the tray icon.
      </p>

      <label class="dontask tiny muted">
        <input type="checkbox" bind:checked={dontAsk} disabled={busy} />
        Don't ask again
      </label>

      <div class="row" style="justify-content:flex-end; gap:.5rem;">
        <button type="button" class="ghost" data-cancel onclick={cancel} disabled={busy}>
          Cancel
        </button>
        <button type="button" class="ghost" onclick={quit} disabled={busy}>Quit Ncy</button>
        <button type="button" onclick={minimize} disabled={busy}>Minimize to tray</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
  }
  .modal {
    width: min(420px, 100%);
    gap: 0.75rem;
    padding: 1.25rem;
    box-shadow: var(--card-shadow);
  }
  .dontask {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    user-select: none;
  }
  .dontask input {
    width: 15px;
    height: 15px;
    accent-color: var(--accent);
    cursor: pointer;
  }
</style>
