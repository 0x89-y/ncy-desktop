<script lang="ts">
  import {
    activeView, serverStatus, connectedPeers, pairings,
    sidebarCollapsed,
  } from "../stores/app";
  import {
    MessageSquare,
    Search,
    Star,
    Clock,
    Link2,
    Settings,
    PanelLeftClose,
    PanelLeftOpen,
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

  function toggle() {
    sidebarCollapsed.update((v) => !v);
  }
</script>

<aside class="bar" class:collapsed={$sidebarCollapsed}>
  <nav class="col">
    {#each items as it (it.key)}
      {@const Icon = it.icon}
      <button
        class="nav"
        class:active={$activeView === it.key}
        title={$sidebarCollapsed ? it.label : undefined}
        aria-label={it.label}
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
    {#if $connectedPeers.size > 0}
      <div class="tiny muted status-extra">{$connectedPeers.size} phone{$connectedPeers.size === 1 ? "" : "s"} online</div>
    {/if}
  </div>

  <div class="footbar">
    <button
      type="button"
      class="toggle"
      title={$sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
      aria-label={$sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
      onclick={toggle}
    >
      {#if $sidebarCollapsed}
        <PanelLeftOpen size={16} strokeWidth={2.25} />
      {:else}
        <PanelLeftClose size={16} strokeWidth={2.25} />
      {/if}
    </button>
    <div class="status-row" title={$sidebarCollapsed ? $serverStatus : undefined}>
      <span class="dot" class:running={$serverStatus === "running"} class:err={$serverStatus === "error"}></span>
      <span class="tiny status-label">{$serverStatus}</span>
    </div>
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
    transition: width .18s ease, padding .18s ease;
    overflow: hidden;
  }
  .bar.collapsed { width: 56px; padding: 1rem .3rem; }

  /* Bottom bar: collapse button + status pill. Expanded → row (toggle then
     dot + "running"); collapsed → stack, dot on top, toggle underneath, both
     centred to match the icon-only rail. */
  .footbar {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .25rem;
  }
  .bar.collapsed .footbar {
    flex-direction: column-reverse;
    align-items: center;
    padding: .25rem 0;
    gap: .35rem;
  }
  .toggle {
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
    cursor: pointer;
    line-height: 0;
    flex-shrink: 0;
  }
  .toggle:hover { background: var(--surface-hover); color: var(--text); }
  .status-row {
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    min-width: 0;
  }
  .status-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar.collapsed .status-label { display: none; }

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
    position: relative;
  }
  .nav:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .nav.active {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text);
  }
  .ico { display: inline-flex; align-items: center; line-height: 0; color: inherit; }
  .lbl { flex: 1; white-space: nowrap; overflow: hidden; }

  /* Collapsed variant: icon-only rail. Labels + status text are hidden but the
     dot indicator + a badge indicator remain visible. Tooltips (`title`) on the
     nav buttons let the user still identify each item on hover. */
  .bar.collapsed .nav { justify-content: center; padding: .5rem 0; gap: 0; }
  .bar.collapsed .lbl { display: none; }
  .bar.collapsed .badge {
    position: absolute;
    top: 2px;
    right: 4px;
    min-width: .9rem;
    height: .9rem;
    padding: 0 .3rem;
    font-size: .65rem;
  }

  .status {
    display: flex; flex-direction: column; gap: .15rem;
    padding: .25rem;
    min-width: 0;
  }
  .bar.collapsed .status { padding: .25rem 0; align-items: center; }
  .bar.collapsed .status-extra { display: none; }

  .dot {
    display: inline-block;
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--outline);
    flex-shrink: 0;
  }
  .dot.running { background: #34a853; }
  .dot.err { background: var(--error); }
</style>
