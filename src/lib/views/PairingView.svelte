<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api } from "../ipc";
  import {
    pairings,
    refreshPairings,
    peers,
    refreshPeers,
    connectedPeers,
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
  // Per-pairing "Show pairing info" toggle. Claimed pairings hide the QR + JSON
  // by default (they're only needed for the first pairing handshake); the user
  // can reveal them here if they need to re-pair a lost phone from scratch.
  let revealed = $state<Record<string, boolean>>({});
  function toggleReveal(id: string) {
    revealed = { ...revealed, [id]: !revealed[id] };
  }

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

  // --- Sync with another desktop (this device is the client). ---
  let peerJson = $state("");
  let peerBusy = $state(false);
  let peerError = $state<string | null>(null);

  async function addDesktop() {
    if (peerBusy || !peerJson.trim()) return;
    peerBusy = true;
    peerError = null;
    try {
      await api.addPeer(peerJson.trim());
      peerJson = "";
      await refreshPeers();
    } catch (e: any) {
      peerError = typeof e === "string" ? e : (e?.message ?? "could not add peer");
    } finally {
      peerBusy = false;
    }
  }

  async function removePeer(id: string) {
    if (!confirm("Stop syncing with this desktop?")) return;
    await api.deletePeer(id);
    await refreshPeers();
  }
</script>

<div class="page-scroll scroll">
 <div class="page">
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
      {@const showBox = !claimed || revealed[p.pairing.id]}
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
            {#if claimed}
              <button class="ghost" onclick={() => toggleReveal(p.pairing.id)}>
                {showBox ? "Hide pairing info" : "Show pairing info"}
              </button>
            {/if}
            <button class="danger ghost" onclick={() => remove(p.pairing.id)}>Remove</button>
          </div>
        </div>

        {#if showBox}
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
        {/if}
      </div>
    {:else}
      <div class="card muted center">
        No pairings yet. Create one above to start syncing.
      </div>
    {/each}
  </div>

  <div class="card">
    <h2>Sync with another desktop</h2>
    <p class="muted tiny">
      On the <em>other</em> desktop, create a pairing above and copy its JSON.
      Paste it here — this desktop connects out to it. Either side can run the
      pairing; the one you paste into is the client.
    </p>
    <textarea
      class="peer-input"
      placeholder={'{"v":1,"deviceId":"…","name":"…","host":"…","port":8765,"secret":"…"}'}
      bind:value={peerJson}
      rows="3"
    ></textarea>
    {#if peerError}
      <div class="tiny err">{peerError}</div>
    {/if}
    <div class="row">
      <button onclick={addDesktop} disabled={peerBusy || !peerJson.trim()}>
        {peerBusy ? "Adding…" : "Add desktop"}
      </button>
    </div>

    <div class="col list">
      {#each $peers as p (p.id)}
        {@const online = $connectedPeers.has(p.id)}
        <div class="row peer">
          <div class="col" style="flex:1; min-width:0;">
            <strong>{p.label}</strong>
            <span class="muted tiny">
              {p.host}:{p.port}
              {#if p.lastSeenAt} · last seen {formatFullTime(p.lastSeenAt)}{/if}
            </span>
          </div>
          <span class="chip" class:active={online}>
            {online ? "Connected" : "Offline"}
          </span>
          <button class="danger ghost" onclick={() => removePeer(p.id)}>Remove</button>
        </div>
      {:else}
        <div class="muted tiny">No desktop peers yet.</div>
      {/each}
    </div>
  </div>
 </div>
</div>

<style>
  /* Full-width scroller so the scrollbar hugs the window's right edge, while
     the content itself stays centered inside a narrower column. */
  .page-scroll {
    height: 100%;
    width: 100%;
  }
  .page {
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
  .peer-input {
    width: 100%;
    box-sizing: border-box;
    margin: .5rem 0;
    padding: .55rem .7rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2);
    color: inherit;
    font-family: "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: .78rem;
    resize: vertical;
  }
  .peer {
    align-items: center;
    gap: .6rem;
    padding: .6rem .15rem;
    border-top: 1px solid var(--outline-variant);
  }
  .err { color: var(--error, #b00); }
</style>
