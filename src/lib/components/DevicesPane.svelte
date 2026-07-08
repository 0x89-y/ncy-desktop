<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api, type PairAttempt } from "../ipc";
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
  import { X } from "@lucide/svelte";
  import { onMount } from "svelte";

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

  // --- Pair-attempts audit log (2026-07-07) ---
  let pairAttempts = $state<PairAttempt[]>([]);
  async function refreshPairAttempts() {
    try {
      const attempts = await api.listPairAttempts();
      pairAttempts = [...attempts].reverse(); // newest first for scannability
    } catch {
      /* non-fatal; leave previous list */
    }
  }
  function fmtOutcome(o: string): string {
    switch (o) {
      case "accepted": return "Accepted";
      case "rejected": return "Rejected";
      case "timeout": return "Timed out";
      case "cancelled": return "Cancelled";
      case "rate-limited": return "Rate-limited";
      case "discovery-off": return "Discovery off";
      case "bad-pubkey": return "Bad pubkey";
      case "server-error": return "Server error";
      default: return o;
    }
  }
  function fmtRelativeTime(ms: number): string {
    const delta = Date.now() - ms;
    if (delta < 60_000) return "just now";
    if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
    if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
    const days = Math.floor(delta / 86_400_000);
    return `${days}d ago`;
  }
  onMount(refreshPairAttempts);
</script>

<div class="devices">
  <section class="group">
    <h2>Server</h2>
    <div class="opt-list">
      <div class="opt-row">
        <span class="opt-label">Status</span>
        <div class="row" style="gap:.5rem; align-items:center;">
          <span class="muted tiny">
            {$serverStatus}{#if $serverAddr} · {$serverAddr}{/if}
          </span>
          <button class="ghost" onclick={refreshIps}>Refresh IPs</button>
        </div>
      </div>
      <div class="field">
        <span class="field-label">Advertise to phone as</span>
        <div class="row" style="flex-wrap:wrap;gap:.4rem">
          {#each $lanIps as ip (ip)}
            <button
              class="chip"
              class:active={$advertisedHost === ip}
              onclick={() => pickHost(ip)}
            >{ip}</button>
          {/each}
        </div>
      </div>
    </div>
  </section>

  <section class="group">
    <h2>New pairing</h2>
    <div class="opt-list">
      <div class="field">
        <span class="field-label">Label</span>
        <div class="row form">
          <input
            type="text"
            placeholder="e.g. My phone"
            bind:value={label}
            onkeydown={(e) => e.key === "Enter" && create()}
          />
          <button onclick={create} disabled={busy}>Create</button>
        </div>
      </div>
    </div>
    <p class="hint">
      Create a pairing slot, then on the phone open Ncy → Settings → Sync devices
      → paste the JSON or scan the QR. Each pairing has its own secret; removing
      one makes the phone re-pair.
    </p>
  </section>

  <section class="group">
    <h2>Paired devices</h2>
    <div class="col list">
      {#each $pairings as p (p.pairing.id)}
        {@const claimed = p.pairing.phoneId !== null}
        {@const showBox = !claimed || revealed[p.pairing.id]}
        <div class="panel pairing">
          <div class="row" style="justify-content:space-between">
            <div class="col">
              <div class="row" style="gap:.45rem">
                <span
                  class="dot"
                  class:paired={claimed}
                  title={claimed ? "Paired" : "Awaiting device"}
                ></span>
                <strong>{p.pairing.label}</strong>
              </div>
              <span class="muted tiny">
                Created {formatFullTime(p.pairing.createdAt)}
                {#if p.pairing.lastSeenAt} · last seen {formatFullTime(p.pairing.lastSeenAt)}{/if}
              </span>
            </div>
            <div class="row" style="gap:.3rem">
              {#if claimed}
                <button class="ghost" onclick={() => toggleReveal(p.pairing.id)}>
                  {showBox ? "Hide" : "Show more"}
                </button>
              {/if}
              <button
                class="icon-btn"
                onclick={() => remove(p.pairing.id)}
                title="Remove pairing"
                aria-label="Remove pairing"
              ><X size={15} strokeWidth={2.25} /></button>
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
        <p class="muted tiny">No pairings yet. Create one above to start syncing.</p>
      {/each}
    </div>
  </section>

  <section class="group">
    <h2>Sync with another desktop</h2>
    <p class="hint">
      On the <em>other</em> desktop, create a pairing above and copy its JSON.
      Paste it here — this desktop connects out to it.
    </p>
    <div class="opt-list">
      <div class="field">
        <span class="field-label">Peer pairing JSON</span>
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
      </div>
    </div>

    {#if $peers.length}
      <div class="opt-list" style="margin-top:.75rem;">
        {#each $peers as p (p.id)}
          {@const online = $connectedPeers.has(p.id)}
          <div class="peer">
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
            <button
              class="icon-btn"
              onclick={() => removePeer(p.id)}
              title="Stop syncing with this desktop"
              aria-label="Stop syncing with this desktop"
            ><X size={15} strokeWidth={2.25} /></button>
          </div>
        {/each}
      </div>
    {:else}
      <div class="muted tiny">No desktop peers yet.</div>
    {/if}
  </section>

  <section class="group">
    <h2>Recent pair attempts</h2>
    <p class="hint">
      Discovery-initiated pair requests hitting this desktop — accepted,
      rejected, timed out, or blocked by the security gates (rate limit,
      discoverability off, malformed handshake). Last 100 in memory; cleared
      on restart.
    </p>
    <div class="row" style="gap:.5rem;">
      <button class="ghost" onclick={refreshPairAttempts}>Refresh</button>
    </div>
    {#if pairAttempts.length === 0}
      <p class="hint">No pair attempts yet.</p>
    {:else}
      <div class="attempts">
        {#each pairAttempts as a}
          <div class="attempt-row row">
            <span
              class="attempt-outcome"
              class:accepted={a.outcome === "accepted"}
              class:rejected={a.outcome === "rejected" || a.outcome === "bad-pubkey" || a.outcome === "rate-limited" || a.outcome === "discovery-off"}
              class:warned={a.outcome === "timeout" || a.outcome === "cancelled" || a.outcome === "server-error"}
            >{fmtOutcome(a.outcome)}</span>
            <span class="tiny">{a.phoneName ?? "(no name)"}</span>
            <span class="tiny muted">from {a.sourceIp}</span>
            <span class="spacer"></span>
            <span class="tiny muted" title={new Date(a.timestampMs).toLocaleString()}>
              {fmtRelativeTime(a.timestampMs)}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .form { gap: .5rem; }
  .form input { flex: 1; }
  .list { gap: .75rem; }
  .pairing { gap: .75rem; }
  /* Pairing status dot — amber while awaiting the device, green once paired. */
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning, #d0a000);
    flex-shrink: 0;
  }
  .dot.paired { background: var(--success, #34a853); }
  /* Compact icon-only action button (the X remove). Overrides the global
     accent-pill button default. */
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    line-height: 0;
  }
  .icon-btn:hover { background: var(--surface-hover); color: var(--danger); filter: none; }
  /* --- Pair-attempts audit log --- */
  .attempts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 260px;
    overflow-y: auto;
    scrollbar-width: thin;
  }
  .attempt-row {
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .attempt-row:last-child { border-bottom: none; }
  .attempt-outcome {
    display: inline-flex;
    padding: 1px 8px;
    border-radius: var(--radius-pill, 999px);
    font-size: 0.72rem;
    font-weight: 600;
    background: var(--surface-2);
    color: var(--text-muted);
    min-width: 84px;
    justify-content: center;
  }
  .attempt-outcome.accepted {
    background: color-mix(in srgb, var(--success, #34a853) 20%, transparent);
    color: var(--success, #34a853);
  }
  .attempt-outcome.rejected {
    background: color-mix(in srgb, var(--danger, #b00) 18%, transparent);
    color: var(--danger, #b00);
  }
  .attempt-outcome.warned {
    background: color-mix(in srgb, var(--warning, #d0a000) 22%, transparent);
    color: var(--warning, #d0a000);
  }
  .spacer { flex: 1; }
  .qrbox {
    align-items: stretch;
    gap: 1rem;
    border-top: 1px solid var(--border);
    padding-top: .75rem;
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
  .peer-input {
    width: 100%;
    box-sizing: border-box;
    margin: 0;
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
    display: flex;
    align-items: center;
    gap: .6rem;
    padding: .6rem .8rem;
    border-top: 1px solid var(--border);
  }
  .peer:first-child { border-top: none; }
  .err { color: var(--error, #b00); }
</style>
