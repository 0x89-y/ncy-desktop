<script lang="ts">
  import { activeView, serverStatus, connectedPeers, identity, pairings } from "../stores/app";

  type View = "chats" | "search" | "starred" | "reminders" | "pairing" | "settings";
  const items: { key: View; label: string; icon: string }[] = [
    { key: "chats", label: "Chats", icon: "💬" },
    { key: "search", label: "Search", icon: "🔎" },
    { key: "starred", label: "Starred", icon: "⭐" },
    { key: "reminders", label: "Reminders", icon: "⏰" },
    { key: "pairing", label: "Devices", icon: "🔗" },
    { key: "settings", label: "Settings", icon: "⚙️" },
  ];
</script>

<aside class="bar">
  <div class="brand">
    <div class="logo">N</div>
    <span class="name">Ncy</span>
  </div>

  <nav class="col">
    {#each items as it (it.key)}
      <button
        class="nav"
        class:active={$activeView === it.key}
        onclick={() => activeView.set(it.key)}
      >
        <span class="ico">{it.icon}</span>
        <span class="lbl">{it.label}</span>
        {#if it.key === "pairing" && $pairings.length > 0}
          <span class="badge">{$pairings.length}</span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="spacer"></div>

  <div class="status">
    <div class="row">
      <span class="dot" class:running={$serverStatus === "running"} class:err={$serverStatus === "error"}></span>
      <span class="tiny">{$serverStatus}</span>
    </div>
    {#if $connectedPeers.size > 0}
      <div class="tiny muted">{$connectedPeers.size} phone{$connectedPeers.size === 1 ? "" : "s"} online</div>
    {/if}
    {#if $identity}
      <div class="tiny muted devname" title={$identity.deviceId}>{$identity.name}</div>
    {/if}
  </div>
</aside>

<style>
  .bar {
    width: 200px;
    flex-shrink: 0;
    padding: 1rem .75rem;
    background: var(--surface);
    border-right: 1px solid var(--outline-variant);
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: 0 .25rem;
  }
  .logo {
    width: 32px; height: 32px;
    border-radius: 10px;
    background: var(--primary);
    color: var(--on-primary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 800;
  }
  .name { font-weight: 700; font-size: 1.1rem; }

  nav { gap: .15rem; }
  .nav {
    display: flex;
    align-items: center;
    gap: .65rem;
    background: transparent;
    color: var(--on-surface);
    text-align: left;
    padding: .55rem .75rem;
    border-radius: 10px;
    width: 100%;
  }
  .nav:hover { background: var(--surface-2); filter: none; }
  .nav.active {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }
  .ico { font-size: 1.05rem; }
  .lbl { flex: 1; }
  .status {
    display: flex; flex-direction: column; gap: .15rem;
    padding: .25rem;
  }
  .dot {
    display: inline-block;
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--outline);
  }
  .dot.running { background: #34a853; }
  .dot.err { background: var(--error); }
  .devname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
