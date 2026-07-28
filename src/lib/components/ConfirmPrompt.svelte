<script lang="ts">
  import { confirmPrompt, resolveConfirmation } from "../stores/confirm";
  import { AlertTriangle, X } from "@lucide/svelte";

  function onKeydown(event: KeyboardEvent) {
    if (!$confirmPrompt) return;
    if (event.key === "Escape") {
      event.preventDefault();
      resolveConfirmation(false);
    } else if (event.key === "Enter") {
      event.preventDefault();
      resolveConfirmation(true);
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if $confirmPrompt}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title">
      <header>
        <AlertTriangle size={18} aria-hidden="true" />
        <strong id="confirm-title">{$confirmPrompt.title}</strong>
        <button class="icon-btn" aria-label="Cancel" onclick={() => resolveConfirmation(false)}><X size={16} /></button>
      </header>
      <p>{$confirmPrompt.message}</p>
      <footer>
        {#if !$confirmPrompt.noticeOnly}
          <button class="ghost" onclick={() => resolveConfirmation(false)}>Cancel</button>
        {/if}
        <button class:danger={$confirmPrompt.dangerous} onclick={() => resolveConfirmation(true)}>{$confirmPrompt.confirmLabel}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop { position:fixed; inset:32px 0 0; z-index:1000; display:grid; place-items:center; padding:1rem; background:color-mix(in srgb, var(--surface) 72%, transparent); }
  .dialog { width:min(26rem, 100%); border:1px solid var(--border-strong); border-radius:var(--radius); background:var(--surface); }
  header { display:flex; align-items:center; gap:.6rem; padding:.8rem 1rem; border-bottom:1px solid var(--border); }
  header strong { flex:1; }
  p { margin:0; padding:1rem; color:var(--text-muted); line-height:1.45; }
  footer { display:flex; justify-content:flex-end; gap:.5rem; padding:.75rem 1rem; border-top:1px solid var(--border); }
  .icon-btn { width:28px; height:28px; padding:0; display:grid; place-items:center; background:transparent; color:var(--text-muted); border:0; border-radius:var(--radius-sm); }
  .icon-btn:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  button.danger { background:var(--danger); color:white; }
</style>
