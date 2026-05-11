<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api } from "../ipc";
  import {
    pairings,
    refreshPairings,
    lanIps,
    advertisedHost,
    refreshLanIps,
    serverAddr,
    serverStatus,
  } from "../stores/app";
  import { formatFullTime } from "../format";

  let label = $state("");
  let busy = $state(false);
  let copyHint = $state<string | null>(null);

  async function create() {
    if (busy) return;
    busy = true;
    try {
      const name = label.trim() || "Phone";
      await api.createPairing(name);
      label = "";
      await refreshPairings();
    } finally {
      busy = false;
    }
  }

  async function remove(id: string) {
    if (!confirm("Remove this pairing? The phone will need to re-pair.")) return;
    await api.deletePairing(id);
    await refreshPairings();
  }

  async function copy(text: string, hint: string) {
    await writeText(text);
    copyHint = hint;
    setTimeout(() => (copyHint = null), 1200);
  }

  async function pickHost(ip: string) {
    await api.setAdvertisedHost(ip);
    await refreshLanIps();
    await refreshPairings();
  }

  async function refreshIps() {
    await refreshLanIps();
  }
</script>

<div class="page scroll">
  <header class="row top">
    <div class="col" style="flex:1">
      <h1>Pair a phone</h1>
      <p class="muted">
        Create a pairing slot, then on the phone open Ncy → Settings → Sync devices →
        Paste pairing JSON or scan the QR.
      </p>
    </div>
  </header>

  <div class="card host">
    <div class="row" style="justify-content:space-between">
      <div class="col">
        <strong>Server</strong>
        <span class="muted tiny">
          Status: {$serverStatus}
          {#if $serverAddr}· bound to {$serverAddr}{/if}
        </span>
      </div>
      <button class="ghost" onclick={refreshIps}>Refresh IPs</button>
    </div>
    <hr class="divider" />
    <div class="row" style="flex-wrap:wrap;gap:.4rem">
      <span class="muted tiny">Advertise to phone as:</span>
      {#each $lanIps as ip (ip)}
        <button
          class="chip"
          class:active={$advertisedHost === ip}
          onclick={() => pickHost(ip)}
        >{ip}</button>
      {/each}
    </div>
  </div>

  <div class="card">
    <h2>New pairing</h2>
    <div class="row form">
      <input
        type="text"
        placeholder="Label (e.g. My phone)"
        bind:value={label}
        onkeydown={(e) => e.key === "Enter" && create()}
      />
      <button onclick={create} disabled={busy}>Create</button>
    </div>
    <p class="muted tiny">
      Each pairing has its own pre-shared secret. Removing one makes the phone re-pair.
    </p>
  </div>

  <div class="col list">
    {#each $pairings as p (p.pairing.id)}
      {@const claimed = p.pairing.phoneId !== null}
      <div class="card pairing">
        <div class="row" style="justify-content:space-between">
          <div class="col">
            <strong>{p.pairing.label}</strong>
            <span class="muted tiny">
              Created {formatFullTime(p.pairing.createdAt)}
              {#if p.pairing.lastSeenAt} · last seen {formatFullTime(p.pairing.lastSeenAt)}{/if}
            </span>
          </div>
          <div class="row">
            <span class="chip" class:active={claimed}>
              {claimed ? `Paired${p.pairing.phoneId ? ` · ${p.pairing.phoneId.slice(0, 8)}` : ""}` : "Awaiting phone"}
            </span>
            <button class="danger ghost" onclick={() => remove(p.pairing.id)}>Remove</button>
          </div>
        </div>

        <div class="row qrbox">
          <div class="qr" aria-label="Pairing QR">
            {@html p.qrSvg}
          </div>
          <div class="col json-box" style="flex:1; min-width:0;">
            <code class="json-pre">{p.payloadJson}</code>
            <div class="row">
              <button class="subtle" onclick={() => copy(p.payloadJson, p.pairing.id)}>
                Copy JSON
              </button>
              {#if copyHint === p.pairing.id}<span class="tiny muted">copied</span>{/if}
            </div>
          </div>
        </div>
      </div>
    {:else}
      <div class="card muted center">
        No pairings yet. Create one above to start syncing.
      </div>
    {/each}
  </div>
</div>

<style>
  .page {
    height: 100%;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 920px;
    margin: 0 auto;
  }
  .top h1 { margin: 0 0 .25rem 0; font-size: 1.4rem; }
  .top p { margin: 0; }
  .form { gap: .5rem; }
  .form input { flex: 1; }
  .list { gap: .75rem; }
  .pairing { display: flex; flex-direction: column; gap: .75rem; }
  .qrbox {
    align-items: stretch;
    gap: 1rem;
  }
  .qr {
    width: 200px;
    height: 200px;
    background: white;
    border-radius: 12px;
    padding: .35rem;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .json-box { gap: .5rem; }
  .json-pre {
    display: block;
    background: var(--surface-2);
    border-radius: 10px;
    padding: .55rem .7rem;
    font-family: "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: .8rem;
    white-space: pre-wrap;
    word-break: break-all;
    overflow: auto;
    max-height: 9rem;
  }
  .chip.active {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }
  .center { text-align: center; padding: 2rem; }
  .host { gap: .5rem; }
</style>
