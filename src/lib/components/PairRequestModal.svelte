<script lang="ts">
  import {
    pendingPairRequests,
    dropPairRequest,
    refreshPairings,
    type PendingPairRequest,
  } from "../stores/app";
  import { api } from "../ipc";
  import { UserPlus, X } from "@lucide/svelte";

  // Show the oldest pending request as the "active" prompt. Additional
  // requests wait behind it (rare — you'd typically only pair one phone at
  // a time), so we don't build a full queue UI.
  const active = $derived<PendingPairRequest | null>($pendingPairRequests[0] ?? null);
  let label = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    // Reset the label input each time a different request becomes active.
    if (active) {
      label = active.phoneName || "Phone";
      error = null;
    }
  });

  async function accept() {
    if (!active || busy) return;
    busy = true;
    error = null;
    try {
      await api.pairAccept(active.sessionId, label.trim());
      dropPairRequest(active.sessionId);
      await refreshPairings();
    } catch (e: any) {
      error = typeof e === "string" ? e : (e?.message ?? "pair accept failed");
    } finally {
      busy = false;
    }
  }
  async function reject() {
    if (!active || busy) return;
    busy = true;
    error = null;
    try {
      await api.pairReject(active.sessionId);
      dropPairRequest(active.sessionId);
    } catch (e: any) {
      error = typeof e === "string" ? e : (e?.message ?? "pair reject failed");
    } finally {
      busy = false;
    }
  }

  // 8-char fingerprint of the phone's deviceId — same shape the pairing chip
  // shows once claimed. Gives you something to eyeball if two devices happen
  // to have the same friendly name.
  const fingerprint = $derived(active ? active.phoneId.slice(0, 8) : "");
</script>

{#if active}
  <div class="backdrop" role="dialog" aria-modal="true" aria-labelledby="prm-title">
    <div class="modal card col">
      <div class="row" style="align-items:center; gap:.5rem;">
        <UserPlus size={20} strokeWidth={2.25} />
        <h2 id="prm-title" style="margin:0; font-size:1.05rem;">Pair a device</h2>
      </div>
      <p class="muted tiny" style="margin:0;">
        A phone on your network wants to pair. Compare the safety code below
        with the one shown on the phone — if they don't match, someone else
        is on the wire between you. If anything looks off, reject and pair
        via QR instead.
      </p>

      <div class="col field" style="gap:.15rem;">
        <span class="muted tiny">Safety code (must match the phone's screen)</span>
        <code class="safety-code">{active.safetyCode.slice(0, 3)} {active.safetyCode.slice(3)}</code>
      </div>

      <div class="col field" style="gap:.15rem;">
        <span class="muted tiny">Phone reports itself as</span>
        <strong>{active.phoneName || "(no name)"}</strong>
      </div>

      <div class="col field" style="gap:.15rem;">
        <span class="muted tiny">Device fingerprint (first 8 hex)</span>
        <code class="fp">{fingerprint}</code>
      </div>

      <div class="col field" style="gap:.15rem;">
        <span class="muted tiny">Save this pairing as</span>
        <input
          type="text"
          bind:value={label}
          placeholder="Phone"
          disabled={busy}
        />
      </div>

      {#if error}
        <div class="tiny" style="color:var(--danger);">{error}</div>
      {/if}

      <div class="row" style="justify-content:flex-end; gap:.5rem;">
        <button type="button" class="ghost" onclick={reject} disabled={busy}>
          <X size={14} strokeWidth={2.25} /> Reject
        </button>
        <button type="button" onclick={accept} disabled={busy}>
          {busy ? "…" : "Confirm & pair"}
        </button>
      </div>

      {#if $pendingPairRequests.length > 1}
        <p class="muted tiny" style="margin:0;">
          {$pendingPairRequests.length - 1} more request{$pendingPairRequests.length - 1 === 1 ? "" : "s"} waiting.
        </p>
      {/if}
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
  .fp {
    font-family: "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: 0.9rem;
    background: var(--surface-2);
    padding: 0.2rem 0.4rem;
    border-radius: 6px;
    user-select: all;
    letter-spacing: 0.05em;
  }
  .safety-code {
    font-family: "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: 1.6rem;
    font-weight: 600;
    background: color-mix(in srgb, var(--accent) 12%, var(--surface-2));
    color: var(--accent);
    padding: 0.4rem 0.7rem;
    border-radius: 10px;
    letter-spacing: 0.35em;
    text-align: center;
    user-select: all;
  }
</style>
