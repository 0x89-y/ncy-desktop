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
  import { EllipsisVertical, Laptop, Smartphone } from "@lucide/svelte";
  import { onMount } from "svelte";

  /** "main": paired devices and pairing. "requests": the pairing request history,
   *  which Settings shows under Advanced. */
  let { part = "main" }: { part?: "main" | "requests" } = $props();

  let busy = $state(false);
  let menuFor = $state<string | null>(null);
  const claimedPairings = $derived($pairings.filter((p) => p.pairing.phoneId !== null));
  const pendingPairings = $derived($pairings.filter((p) => p.pairing.phoneId === null));
  /** Phones are not reported per connection, only by address, so a phone counts as
   *  online when something is connected and it synced in the last five minutes. */
  function phoneLooksOnline(lastSeenAt: number | null): boolean {
    return $connectedPeers.size > 0 && lastSeenAt !== null && Date.now() - lastSeenAt < 5 * 60_000;
  }
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
  {#if part === "main"}
  <section class="group">
    <div class="subsection-title">Paired devices</div>
    {#if claimedPairings.length || $peers.length}
      <div class="dev-list">
        {#each claimedPairings as p (p.pairing.id)}
          {@const online = phoneLooksOnline(p.pairing.lastSeenAt)}
          <div class="dev-row">
            <span class="dev-ico"><Smartphone size={18} strokeWidth={2} /></span>
            <div class="dev-body">
              <span class="dev-name">{p.pairing.phoneName || "Unnamed device"}</span>
              <span class="dev-sub">
                {#if duplicateDeviceName(p.pairing.phoneName)}ID {shortDeviceFingerprint(p.pairing.phoneId!)} · {/if}
                {p.pairing.lastSeenAt ? `Synced ${fmtRelativeTime(p.pairing.lastSeenAt)}` : `Paired ${formatFullTime(p.pairing.createdAt)}`}
              </span>
            </div>
            <span class="presence" class:online>{online ? "online" : "away"}</span>
            <div class="menu-anchor">
              <button type="button" class="icon-btn" aria-label={`Options for ${p.pairing.phoneName || "device"}`} aria-expanded={menuFor === p.pairing.id} onclick={() => (menuFor = menuFor === p.pairing.id ? null : p.pairing.id)}><EllipsisVertical size={16} /></button>
              {#if menuFor === p.pairing.id}
                <button type="button" class="menu-backdrop" aria-label="Close menu" onclick={() => (menuFor = null)}></button>
                <div class="row-menu" role="menu">
                  <button type="button" role="menuitem" class="danger-item" onclick={() => { menuFor = null; void remove(p.pairing.id); }}>Remove device</button>
                </div>
              {/if}
            </div>
          </div>
        {/each}

        {#each $peers as p (p.id)}
          {@const online = p.enabled && $connectedPeers.has(p.id)}
          {@const deviceName = p.authenticatedName || p.label}
          <div class="dev-row">
            <span class="dev-ico"><Laptop size={18} strokeWidth={2} /></span>
            <div class="dev-body">
              <span class="dev-name">{p.label}</span>
              <span class="dev-sub">
                {#if p.authenticatedName && p.authenticatedName !== p.label}Device {p.authenticatedName} · {/if}
                {#if duplicateDeviceName(deviceName)}ID {shortDeviceFingerprint(p.id)} · {/if}
                {#if !p.enabled}Automatic sync paused
                {:else if p.lastSeenAt}Synced {fmtRelativeTime(p.lastSeenAt)}
                {:else}{p.host}:{p.port}{/if}
                {#if p.enabled && !online && p.foregroundOnly} · open Ncy and enable phone sync there{/if}
                {#if p.enabled && $peerErrors[p.id]} · {$peerErrors[p.id]}{/if}
              </span>
              {#if p.enabled && !online && isPairingRepairError($peerErrors[p.id])}
                <span class="tiny err">Automatic retries are stopped because this is a trust failure. Remove the peer and pair again.</span>
              {:else if p.enabled && !online && $peerErrors[p.id]}
                <span class="tiny muted">If both devices are running, the network may block direct device traffic. Try a private LAN or hotspot.</span>
              {/if}
              {#if diagnostics[p.id]}
                <div class="col tiny muted diag">
                  <span>Address: {p.host}:{p.port}</span>
                  <span>Last discovery: {p.lastDiscoveredAt ? formatFullTime(p.lastDiscoveredAt) : "never"}</span>
                  <span>
                    Last authenticated: {p.lastAuthenticatedAt ? formatFullTime(p.lastAuthenticatedAt) : "never"}
                    {#if p.lastAuthenticatedHost && p.lastAuthenticatedPort} at {p.lastAuthenticatedHost}:{p.lastAuthenticatedPort}{/if}
                  </span>
                  <span>Retry: {!p.enabled ? "paused by user" : online ? "not retrying" : p.nextRetryAt ? `scheduled ${formatFullTime(p.nextRetryAt)}` : isPairingRepairError($peerErrors[p.id]) ? "stopped until re-pairing" : "not scheduled"}</span>
                  <span>Direct LAN: {online ? "reachable" : ($peerErrors[p.id] || p.lastError) ? "possibly blocked; an offline peer looks the same" : "not currently tested"}</span>
                  {#if p.lastError}<span class="err">Last error: {p.lastError}</span>{/if}
                </div>
              {/if}
            </div>
            <span class="presence" class:online class:error={p.enabled && !online && !!$peerErrors[p.id]}>
              {!p.enabled ? "paused" : online ? "online" : $peerErrors[p.id] ? "error" : "away"}
            </span>
            <div class="menu-anchor">
              <button type="button" class="icon-btn" aria-label={`Options for ${p.label}`} aria-expanded={menuFor === p.id} onclick={() => (menuFor = menuFor === p.id ? null : p.id)}><EllipsisVertical size={16} /></button>
              {#if menuFor === p.id}
                <button type="button" class="menu-backdrop" aria-label="Close menu" onclick={() => (menuFor = null)}></button>
                <div class="row-menu" role="menu">
                  <button type="button" role="menuitem" onclick={() => { menuFor = null; void setPeerEnabled(p.id, !p.enabled); }}>{p.enabled ? "Pause automatic sync" : "Resume automatic sync"}</button>
                  <button type="button" role="menuitem" onclick={() => { menuFor = null; toggleDiagnostics(p.id); }}>{diagnostics[p.id] ? "Hide diagnostics" : "Show diagnostics"}</button>
                  <button type="button" role="menuitem" class="danger-item" onclick={() => { menuFor = null; void removePeer(p.id); }}>Stop syncing with this desktop</button>
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted tiny inline-empty">No devices paired yet. Add one below.</p>
    {/if}
  </section>

  <section class="group">
    <div class="subsection-title">Add a device</div>
    {#each pendingPairings as p (p.pairing.id)}
      <div class="pair-box">
        <div class="qr" aria-label="Pairing QR">{@html p.qrSvg}</div>
        <div class="pair-body">
          <ol class="steps">
            <li>In Ncy on the phone, open Devices and scan this code.</li>
            <li>Check that both screens show the same 6 digits, then confirm.</li>
          </ol>
          <div class="row pair-actions">
            <button type="button" class="ghost" onclick={() => replaceInvitation(p.pairing)}>New code</button>
            <button type="button" class="ghost" onclick={() => copy(p.payloadJson, p.pairing.id)}>Copy pairing text</button>
            {#if copyHint === p.pairing.id}<span class="tiny muted">copied</span>{/if}
            <button type="button" class="text-link" onclick={() => remove(p.pairing.id)}>Cancel</button>
          </div>
          <span class="tiny muted">A new code makes every earlier copy of this one stop working. Copied pairing text clears from the clipboard after 60 seconds.</span>
        </div>
      </div>
    {:else}
      <div class="pair-box empty">
        <div class="pair-body">
          <span>Pair a phone by scanning a code shown here.</span>
          <div class="row pair-actions">
            <button onclick={create} disabled={busy}>{busy ? "Creating…" : "Show pairing code"}</button>
          </div>
        </div>
      </div>
    {/each}
    <button type="button" class="text-link manual-toggle" onclick={() => (showManualPairing = !showManualPairing)}>
      {showManualPairing ? "Hide pairing text" : "Pair another desktop with pairing text"}
    </button>
    {#if showManualPairing}
      <div class="manual-pairing">
        <textarea
          class="peer-input"
          placeholder={'{"v":1,"deviceId":"...","name":"...","host":"...","port":8765,"secret":"..."}'}
          bind:value={peerJson}
          rows="3"
          aria-label="Pairing JSON"
        ></textarea>
        <span class="tiny danger">Pairing text is a credential; do not save or share copied values.</span>
        {#if peerError}<div class="tiny err">{peerError}</div>{/if}
        <div class="row">
          <button onclick={addDesktop} disabled={peerBusy || !peerJson.trim()}>{peerBusy ? "Adding…" : "Add desktop"}</button>
        </div>
      </div>
    {/if}
  </section>
  {/if}

  {#if part === "requests" && pairAttempts.length}
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
  .dev-list { display: flex; flex-direction: column; }
  .dev-row {
    display: flex;
    align-items: center;
    gap: .8rem;
    padding: .7rem var(--settings-pad);
    border-top: 1px solid var(--border);
  }
  .dev-ico { color: var(--text-muted); display: inline-flex; flex: none; }
  .dev-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .1rem; }
  .dev-name { font-size: .92rem; font-weight: 500; }
  .dev-sub { font-size: .8rem; color: var(--text-muted); }
  .diag { margin-top: .3rem; gap: .15rem; }
  .presence {
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    font-family: var(--font-mono);
    font-size: .72rem;
    color: var(--text-muted);
  }
  .presence::before {
    content: "";
    width: 7px;
    height: 7px;
    border-radius: var(--radius-pill);
    border: 1.5px solid currentColor;
  }
  .presence.online { color: var(--online); }
  .presence.online::before { background: currentColor; }
  .presence.error { color: var(--danger); }
  .menu-anchor { position: relative; flex: none; }
  .menu-backdrop, .menu-backdrop:hover {
    position: fixed; inset: 0; z-index: 20; padding: 0; border: 0; border-radius: 0;
    background: transparent; box-shadow: none; filter: none;
  }
  .row-menu {
    position: absolute; z-index: 21; top: calc(100% + .25rem); right: 0; min-width: 14rem;
    display: flex; flex-direction: column; padding: .3rem;
    background: var(--surface); border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
  }
  .row-menu button {
    text-align: left; padding: .45rem .6rem; border: 0; border-radius: var(--radius-sm);
    background: transparent; color: var(--text); font-size: .88rem;
  }
  .row-menu button:hover { background: var(--surface-hover); }
  .row-menu .danger-item { color: var(--danger); }
  .pair-box {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin: .2rem var(--settings-pad) 0;
    padding: .9rem;
    border: 1px dashed var(--border-strong);
    border-radius: 14px;
  }
  .pair-box.empty { align-items: flex-start; }
  .pair-box .qr { width: 132px; height: 132px; flex: none; background: #fff; border-radius: 10px; padding: 6px; border: 1px solid var(--border-strong); }
  .pair-box .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .pair-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .55rem; font-size: .88rem; }
  .steps { margin: 0; padding-left: 1.2rem; display: flex; flex-direction: column; gap: .3rem; }
  .pair-actions { gap: .5rem; flex-wrap: wrap; align-items: center; }
  .text-link, .text-link:hover {
    background: transparent; border: 0; padding: .3rem .2rem; color: var(--accent); font-weight: 600; font-size: .86rem;
  }
  .manual-toggle { margin: .5rem var(--settings-pad) 0; align-self: flex-start; }
  .devices {
    border-top: 1px solid var(--border);
    padding: .75rem 0 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }
  .manual-pairing { display:flex; flex-direction:column; gap:.5rem; margin-top:.5rem; }
  .compact-btn {
    padding: .42rem .7rem;
    min-width: 4.75rem;
    flex-shrink: 0;
    font-size: .85rem;
  }
  .inline-empty {
    margin: 0;
    padding: .75rem var(--settings-pad);
    border-top: 1px solid var(--border);
  }
  /* Cards run edge-to-edge, divided by the panel border-top (no gap). */
  /* Pairing status dot — amber while awaiting the device, green once paired. */
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
