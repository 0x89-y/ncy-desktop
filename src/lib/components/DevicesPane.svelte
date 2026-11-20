<script lang="ts">
  import { requestConfirmation } from "../stores/confirm";
  import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api, type PairAttempt, type PairingDescriptor } from "../ipc";
  import {
    pairings,
    refreshPairings,
    peers,
    refreshPeers,
    connectedPeers,
    peerErrors,
  } from "../stores/app";
  import { formatFullTime } from "../format";
  import { X } from "@lucide/svelte";
  import { onMount } from "svelte";

  let busy = $state(false);
  let copyHint = $state<string | null>(null);
  let showManualPairing = $state(false);
  let showRecentRequests = $state(false);
  const duplicateDeviceNames = $derived.by(() => {
    const counts = new Map<string, number>();
    const add = (name: string | null) => {
      const key = deviceNameKey(name);
      if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
    };
    for (const descriptor of $pairings) {
      if (descriptor.pairing.phoneId) add(descriptor.pairing.phoneName);
    }
    for (const peer of $peers) add(peer.authenticatedName || peer.label);
    return new Set([...counts].filter(([, count]) => count > 1).map(([key]) => key));
  });

  function deviceNameKey(name: string | null): string {
    return (name ?? "").trim().toLowerCase();
  }

  function duplicateDeviceName(name: string | null): boolean {
    return duplicateDeviceNames.has(deviceNameKey(name));
  }

  function shortDeviceFingerprint(deviceId: string): string {
    return deviceId.toLowerCase().replace(/[^a-z0-9]/g, "").slice(0, 8) || "unknown";
  }
  let diagnostics = $state<Record<string, boolean>>({});
  function toggleDiagnostics(id: string) {
    diagnostics = { ...diagnostics, [id]: !diagnostics[id] };
  }

  async function create() {
    if (busy) return;
    busy = true;
    try {
      const placeholder = `New device ${crypto.randomUUID().slice(0, 4).toUpperCase()}`;
      await api.createPairing(placeholder);
      await refreshPairings();
    } finally {
      busy = false;
    }
  }

  async function remove(id: string) {
    if (!await requestConfirmation("The affected device will disconnect and must pair again. Chats remain unchanged.", { title: "Remove paired device", confirmLabel: "Remove", dangerous: true })) return;
    await api.deletePairing(id);
    await refreshPairings();
  }

  async function replaceInvitation(pairing: PairingDescriptor["pairing"]) {
    if (pairing.phoneId) return;
    if (!await requestConfirmation("Any previously copied QR or JSON will stop working.", { title: "Replace invitation", confirmLabel: "Replace", dangerous: true })) return;
    await api.replacePairing(pairing.id);
    await refreshPairings();
  }

  async function copy(text: string, hint: string) {
    await writeText(text);
    copyHint = hint;
    setTimeout(() => (copyHint = null), 1200);
    setTimeout(async () => {
      try {
        if ((await readText()) === text) await writeText("");
      } catch {
        // Clipboard permission may change while the timer is pending.
      }
    }, 60_000);
  }


  // --- Add another desktop as an outbound sync peer. ---
  let peerJson = $state("");
  let peerBusy = $state(false);
  let peerError = $state<string | null>(null);

  async function addDesktop() {
    if (peerBusy || !peerJson.trim()) return;
    const submitted = peerJson.trim();
    peerBusy = true;
    peerError = null;
    try {
      await api.addPeer(submitted);
      peerJson = "";
      try {
        if ((await readText()).trim() === submitted) await writeText("");
      } catch {
        // Pairing succeeded; clipboard cleanup is best-effort.
      }
      await refreshPeers();
    } catch (e: any) {
      peerError = typeof e === "string" ? e : (e?.message ?? "could not add peer");
    } finally {
      peerBusy = false;
    }
  }

  async function removePeer(id: string) {
    if (!await requestConfirmation("Stop syncing with this desktop?", { title: "Remove desktop", confirmLabel: "Remove", dangerous: true })) return;
    await api.deletePeer(id);
    await refreshPeers();
  }

  async function setPeerEnabled(id: string, enabled: boolean) {
    await api.setPeerEnabled(id, enabled);
    if (!enabled) {
      connectedPeers.update((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
    await refreshPeers();
  }

  // --- Discovery request audit log (2026-07-07) ---
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
  function isPairingRepairError(message: string | undefined): boolean {
    return message?.includes("Pairing credential was revoked or changed") ?? false;
  }
  onMount(refreshPairAttempts);
</script>

<div class="devices">
  <section class="group">
    <div class="subsection-title">Add or share device</div>
    <div class="opt-list">
      <div class="field">
        <span class="field-label">Share this desktop</span>
        <div class="row form">
          <button onclick={create} disabled={busy}>{busy ? "Creating..." : "Create pairing code"}</button>
          <button type="button" class="ghost" onclick={() => (showManualPairing = !showManualPairing)}>
            {showManualPairing ? "Hide manual" : "Manual"}
          </button>
        </div>
        {#if showManualPairing}
          <div class="manual-pairing">
            <textarea
              class="peer-input"
              placeholder={'{"v":1,"deviceId":"...","name":"...","host":"...","port":8765,"secret":"..."}'}
              bind:value={peerJson}
              rows="3"
              aria-label="Pairing JSON"
            ></textarea>
            <span class="tiny danger">
              Pairing JSON is a credential; do not save or share copied values.
            </span>
            {#if peerError}
              <div class="tiny err">{peerError}</div>
            {/if}
            <div class="row">
              <button onclick={addDesktop} disabled={peerBusy || !peerJson.trim()}>
                {peerBusy ? "Adding..." : "Add desktop"}
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </section>

  {#if $pairings.length || $peers.length}
  <section class="group">
    <div class="subsection-title">Paired devices</div>
    <div class="col list">
        {#each $pairings as p (p.pairing.id)}
          {@const claimed = p.pairing.phoneId !== null}
          <div class="panel pairing">
            <div class="row" style="justify-content:space-between">
              <div class="col">
                <div class="row" style="gap:.45rem">
                  <span
                    class="dot"
                    class:paired={claimed}
                    title={claimed ? "Paired" : "Awaiting device"}
                  ></span>
                  <strong>{claimed ? (p.pairing.phoneName || "Unnamed device") : p.pairing.label}</strong>
                </div>
                <span class="muted tiny">
                  {#if claimed && duplicateDeviceName(p.pairing.phoneName)}
                    ID {shortDeviceFingerprint(p.pairing.phoneId!)} ·
                  {/if}
                  Created {formatFullTime(p.pairing.createdAt)}
                  {#if p.pairing.lastSeenAt} · last synchronized {formatFullTime(p.pairing.lastSeenAt)}{/if}
                </span>
              </div>
              <div class="row" style="gap:.3rem">
                {#if !claimed}
                  <button class="ghost" onclick={() => replaceInvitation(p.pairing)}>
                    Replace code
                  </button>
                {/if}
                <button
                  class="icon-btn"
                  onclick={() => remove(p.pairing.id)}
                  title="Revoke and remove pairing"
                  aria-label="Revoke and remove pairing"
                ><X size={15} strokeWidth={2.25} /></button>
              </div>
            </div>

            {#if !claimed}
              <p class="tiny muted">Replacing this invitation invalidates every previously copied version of this QR/JSON.</p>
              <div class="row qrbox">
                <div class="qr" aria-label="Pairing QR">
                  {@html p.qrSvg}
                </div>
                <div class="col json-box" style="flex:1; min-width:0;">
                  <code class="json-pre">{p.payloadJson}</code>
                  <span class="tiny danger">
                    Contains a pairing credential. Copied JSON clears after 60 seconds if unchanged.
                  </span>
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
        {/each}

        {#each $peers as p (p.id)}
          {@const online = p.enabled && $connectedPeers.has(p.id)}
          {@const deviceName = p.authenticatedName || p.label}
          <div class="panel peer-card">
            <div class="col" style="flex:1; min-width:0;">
              <div class="row" style="gap:.45rem">
                <span
                  class="dot"
                  class:paired={online}
                  title={online ? "Connected" : "Waiting to connect"}
                ></span>
                <strong>{p.label}</strong>
              </div>
              <span class="muted tiny">
                {#if p.authenticatedName && p.authenticatedName !== p.label}
                  Device {p.authenticatedName} ·
                {/if}
                {#if duplicateDeviceName(deviceName)}
                  Device ID {shortDeviceFingerprint(p.id)} ·
                {/if}
                {p.host}:{p.port}
                {#if p.lastSeenAt} · last synchronized {formatFullTime(p.lastSeenAt)}{/if}
                {#if !p.enabled} · automatic sync paused
                {:else if !online && p.foregroundOnly} · open Ncy and enable phone sync there{/if}
                {#if p.enabled && $peerErrors[p.id]} · {$peerErrors[p.id]}{/if}
              </span>
              {#if p.enabled && !online && isPairingRepairError($peerErrors[p.id])}
                <span class="tiny err">
                  Automatic retries are stopped because this is a trust failure. Remove the peer and pair again.
                </span>
              {:else if p.enabled && !online && $peerErrors[p.id]}
                <span class="muted tiny">
                  If both devices are running, the network may block direct device traffic. Try a private LAN or hotspot.
                </span>
              {/if}
              <button class="ghost compact" onclick={() => toggleDiagnostics(p.id)}>
                {diagnostics[p.id] ? "Hide diagnostics" : "Show diagnostics"}
              </button>
              {#if diagnostics[p.id]}
                <div class="col tiny muted" style="gap:.15rem">
                  <span>Last discovery: {p.lastDiscoveredAt ? formatFullTime(p.lastDiscoveredAt) : "never"}</span>
                  <span>
                    Last authenticated: {p.lastAuthenticatedAt ? formatFullTime(p.lastAuthenticatedAt) : "never"}
                    {#if p.lastAuthenticatedHost && p.lastAuthenticatedPort}
                      at {p.lastAuthenticatedHost}:{p.lastAuthenticatedPort}
                    {/if}
                  </span>
                  <span>
                    Retry: {!p.enabled ? "paused by user" : online ? "not retrying" : p.nextRetryAt ? `scheduled ${formatFullTime(p.nextRetryAt)}` : isPairingRepairError($peerErrors[p.id]) ? "stopped until re-pairing" : "not scheduled"}
                  </span>
                  <span>Direct LAN: {online ? "reachable" : ($peerErrors[p.id] || p.lastError) ? "possibly blocked; an offline peer looks the same" : "not currently tested"}</span>
                  {#if p.lastError}<span class="err">Last error: {p.lastError}</span>{/if}
                </div>
              {/if}
            </div>
            <div class="row" style="gap:.3rem">
              <span class="chip" class:active={online}>
                {!p.enabled ? "Paused" : online ? "Connected" : $peerErrors[p.id] ? "Connection error" : "Waiting to connect"}
              </span>
              <button
                class="ghost compact"
                onclick={() => setPeerEnabled(p.id, !p.enabled)}
                title={p.enabled ? "Pause automatic sync" : "Resume automatic sync"}
              >{p.enabled ? "Pause" : "Resume"}</button>
              <button
                class="icon-btn"
                onclick={() => removePeer(p.id)}
                title="Stop syncing with this desktop"
                aria-label="Stop syncing with this desktop"
              ><X size={15} strokeWidth={2.25} /></button>
            </div>
          </div>
        {/each}
    </div>
  </section>
  {:else}
    <p class="muted tiny inline-empty">Create a pairing code to add your first device.</p>
  {/if}

  {#if pairAttempts.length}
  <section class="group">
    <div class="subsection-title">Recent requests</div>
    <div class="opt-list">
      <div class="opt-row">
        <span class="opt-label">
          Pairing request history
          <span class="inline-hint">Accepted, rejected, and timed-out requests.</span>
        </span>
        <button type="button" class="subtle compact-btn" onclick={() => (showRecentRequests = !showRecentRequests)}>
          {showRecentRequests ? "Hide" : "Show"}
        </button>
      </div>
    </div>
    {#if showRecentRequests}
      <div class="row attempts-head">
        <button class="ghost" onclick={refreshPairAttempts}>Refresh</button>
      </div>
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
  {/if}
</div>

<style>
  .devices {
    border-top: 1px solid var(--border);
    padding-top: .75rem;
  }
  .form { gap: .5rem; }
  .form .ghost {
    flex-shrink: 0;
  }
  .manual-pairing { display:flex; flex-direction:column; gap:.5rem; margin-top:.5rem; }
  .compact-btn {
    padding: .42rem .7rem;
    min-width: 4.75rem;
    flex-shrink: 0;
    font-size: .85rem;
  }
  .subsection-title { padding: .45rem var(--settings-pad); color: var(--text-muted); font-size: .78rem; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .inline-empty {
    margin: 0;
    padding: .75rem var(--settings-pad);
    border-top: 1px solid var(--border);
  }
  /* Cards run edge-to-edge, divided by the panel border-top (no gap). */
  .list { gap: 0; }
  .pairing { gap: .75rem; }
  /* Pairing status dot — amber while awaiting the device, green once paired. */
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning, var(--warning));
    flex-shrink: 0;
  }
  .dot.paired { background: var(--success, var(--online)); }
  .danger { color: var(--danger); }
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
  /* --- Discovery request audit log --- */
  .attempts-head { padding: 0 var(--settings-pad) .6rem; }
  .attempts {
    display: flex;
    flex-direction: column;
    max-height: 260px;
    overflow-y: auto;
    scrollbar-width: thin;
  }
  .attempt-row {
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem var(--settings-pad);
    border-top: 1px solid var(--border);
  }
  .attempt-row:first-child { border-top: none; }
  .attempt-outcome {
    display: inline-flex;
    padding: 1px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.72rem;
    font-weight: 600;
    background: var(--surface-2);
    color: var(--text-muted);
    min-width: 84px;
    justify-content: center;
  }
  .attempt-outcome.accepted {
    background: color-mix(in srgb, var(--success, var(--online)) 20%, transparent);
    color: var(--success, var(--online));
  }
  .attempt-outcome.rejected {
    background: color-mix(in srgb, var(--danger, #b00) 18%, transparent);
    color: var(--danger, #b00);
  }
  .attempt-outcome.warned {
    background: color-mix(in srgb, var(--warning, var(--warning)) 22%, transparent);
    color: var(--warning, var(--warning));
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
    font-family: var(--font-mono);
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
    font-family: var(--font-mono);
    font-size: .78rem;
    resize: vertical;
  }
  .err { color: var(--error, #b00); }
</style>
