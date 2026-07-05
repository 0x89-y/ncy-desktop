<script lang="ts">
  import { activeView, serverStatus, connectedPeers, identity, pairings } from "../stores/app";
  import {
    MessageSquare,
    Search,
    Star,
    Clock,
    Link2,
    Settings,
    type Icon as IconType,
  } from "@lucide/svelte";

  type View = "chats" | "search" | "starred" | "reminders" | "pairing" | "settings";
  const items: { key: View; label: string; icon: typeof IconType }[] = [
    { key: "chats", label: "Chats", icon: MessageSquare },
    { key: "search", label: "Search", icon: Search },
    { key: "starred", label: "Starred", icon: Star },
    { key: "reminders", label: "Reminders", icon: Clock },
    { key: "pairing", label: "Devices", icon: Link2 },
    { key: "settings", label: "Settings", icon: Settings },
  ];
</script>

<aside class="bar">
  <nav class="col">
    {#each items as it (it.key)}
      {@const Icon = it.icon}
      <button
        class="nav"
        class:active={$activeView === it.key}
        onclick={() => activeView.set(it.key)}
      >
        <span class="ico"><Icon size={17} /></span>
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
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  nav { gap: .15rem; }
  .nav {
    display: flex;
    align-items: center;
    gap: .65rem;
    background: transparent;
    color: var(--text-muted);
    text-align: left;
    padding: .5rem .7rem;
    border-radius: var(--radius-sm);
    width: 100%;
    border: 1px solid transparent;
    font-weight: 500;
    font-size: .92rem;
  }
  .nav:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .nav.active {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text);
  }
  .ico { display: inline-flex; align-items: center; line-height: 0; color: inherit; }
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
