<script lang="ts">
  import { api } from "../ipc";
  import { activeTag, allTags, chatListCollapsed, chatPane, chats, lastSyncEvent, newChatRequest, pinnedOnly, showArchived, type ChatPane } from "../stores/app";
  import { Bell, Check, Filter, MessageSquare, Search, Star, X, type Icon as IconType } from "@lucide/svelte";
  import ChatList from "./ChatList.svelte";
  import ProfileMenu from "./ProfileMenu.svelte";
  import StarredPane from "./chatpane/StarredPane.svelte";
  import RemindersPane from "./chatpane/RemindersPane.svelte";

  const tabs: { key: ChatPane; label: string; icon: typeof IconType }[] = [
    { key: "chats", label: "Chats", icon: MessageSquare },
    { key: "starred", label: "Favorites", icon: Star },
    { key: "reminders", label: "Reminders", icon: Bell },
  ];

  let reminderCount = $state(0);
  let searchOpen = $state(false);
  let filtersOpen = $state(false);
  let query = $state("");
  let searchInput = $state<HTMLInputElement>();
  let reminderCountRun = 0;
  const activeFilterCount = $derived(($activeTag ? 1 : 0) + ($pinnedOnly ? 1 : 0) + ($showArchived ? 1 : 0));

  function toggleSearch() {
    if (!searchOpen) {
      chatPane.set("chats");
      filtersOpen = false;
      searchOpen = true;
      requestAnimationFrame(() => searchInput?.focus());
    } else {
      searchOpen = false;
      query = "";
    }
  }

  function setTag(tag: string | null) {
    activeTag.set(tag);
    filtersOpen = false;
  }

  function onWindowKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    // Both act on the list, so they bring a hidden list back first.
    if (mod && (e.key === "k" || e.key === "K")) {
      e.preventDefault();
      chatListCollapsed.set(false);
      if (!searchOpen) toggleSearch();
      else requestAnimationFrame(() => searchInput?.focus());
      return;
    }
    if (mod && (e.key === "n" || e.key === "N")) {
      e.preventDefault();
      chatListCollapsed.set(false);
      chatPane.set("chats");
      newChatRequest.set(true);
      return;
    }
    if (e.key !== "Escape") return;
    // Consume Escape only when this pane actually has a layer to close. These are
    // all `window` listeners, so stopping propagation unconditionally would swallow
    // the key for everything mounted after us (ConfirmPrompt, ImageViewer, the chat
    // context menus) — see the ordering in App.svelte.
    if (filtersOpen) {
      e.stopImmediatePropagation();
      filtersOpen = false;
    } else if (searchOpen) {
      e.stopImmediatePropagation();
      searchOpen = false;
      query = "";
    }
  }

  async function refreshReminderCount(run: number) {
    const now = Date.now();
    let count = 0;
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        if (m.reminderAt && m.reminderAt >= now) count += 1;
      }
    }
    if (run === reminderCountRun) reminderCount = count;
  }

  $effect(() => {
    $chats;
    $lastSyncEvent;
    const run = ++reminderCountRun;
    void refreshReminderCount(run);
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="pane col">
  <div class="tabs-row">
    <ProfileMenu />
    <div class="tabs" aria-label="Chat views">
      {#each tabs as t (t.key)}
        {@const Icon = t.icon}
        <button
          type="button"
          class="tab"
          class:on={$chatPane === t.key}
          title={t.label}
          aria-label={t.label}
          aria-pressed={$chatPane === t.key}
          onclick={() => chatPane.set(t.key)}
        >
          <Icon size={16} strokeWidth={2.25} aria-hidden="true" />
          {#if t.key === "reminders" && reminderCount > 0}
            <span class="tab-badge" aria-label={`${reminderCount} reminders`}>
              {reminderCount}
            </span>
          {/if}
        </button>
      {/each}
    </div>
    <button
      class="header-action"
      class:active={searchOpen}
      type="button"
      title={searchOpen ? "Close search" : "Search"}
      aria-label={searchOpen ? "Close search" : "Search"}
      aria-pressed={searchOpen}
      onclick={toggleSearch}
    >
      {#if searchOpen}<X size={16} />{:else}<Search size={16} strokeWidth={2.25} />{/if}
    </button>
    {#if $chatPane === "chats"}
      <div class="filter-control">
        <button
          class="header-action"
          class:active={$activeTag !== null || $pinnedOnly || $showArchived}
          type="button"
          title="Filter chats"
          aria-label="Filter chats"
          aria-expanded={filtersOpen}
          onclick={() => { filtersOpen = !filtersOpen; searchOpen = false; query = ""; }}
        >
          <Filter size={15} strokeWidth={2.25} />
          {#if activeFilterCount}<span class="filter-count">{activeFilterCount}</span>{/if}
        </button>
        {#if filtersOpen}
          <button class="menu-backdrop" aria-label="Close filters" onclick={() => filtersOpen = false}></button>
          <div class="filter-menu">
            {#if $allTags.length}
              <strong class="tiny muted">Tag</strong>
              <button onclick={() => setTag(null)}><span>Any tag</span>{#if $activeTag === null}<Check size={14} />{/if}</button>
              {#each $allTags as tag (tag)}
                <button onclick={() => setTag(tag)}><span>#{tag}</span>{#if $activeTag === tag}<Check size={14} />{/if}</button>
              {/each}
              <div class="separator"></div>
            {/if}
            <strong class="tiny muted">Chat state</strong>
            <button onclick={() => pinnedOnly.update((value) => !value)}><span>Pinned chats only</span>{#if $pinnedOnly}<Check size={14} />{/if}</button>
            <button onclick={() => showArchived.update((value) => !value)}><span>Show archived chats</span>{#if $showArchived}<Check size={14} />{/if}</button>
            {#if activeFilterCount}
              <div class="separator"></div>
              <button class="clear-filter" onclick={() => { activeTag.set(null); pinnedOnly.set(false); showArchived.set(false); filtersOpen = false; }}>Clear filters</button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if searchOpen}
    <div class="search-row">
      <Search size={15} strokeWidth={2.25} aria-hidden="true" />
      <input bind:this={searchInput} type="search" placeholder="Search chats & messages…" bind:value={query} aria-label="Search chats and messages" />
    </div>
  {/if}

  <div class="body">
    {#if $chatPane === "chats"}
      <ChatList {query} />
    {:else if $chatPane === "starred"}
      <StarredPane />
    {:else if $chatPane === "reminders"}
      <RemindersPane />
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; min-height: 0; gap: 0; }
  .pane {
    overflow: hidden;
  }
  .tabs-row {
    display: flex;
    align-items: center;
    gap: .35rem;
    padding: .45rem;
    flex-shrink: 0;
  }
  .tabs {
    display: flex;
    flex: 1;
    gap: 2px;
    min-width: 0;
    padding: 2px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .tab {
    flex: 1;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: .36rem .3rem;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 500;
    cursor: pointer;
    line-height: 1;
    transition: background-color .15s ease, color .15s ease;
  }
  .tab:hover:not(.on) { background: var(--surface-hover); color: var(--text); filter: none; }
  .tab.on { background: var(--accent); color: var(--accent-contrast); }
  .tab-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: .9rem;
    height: .9rem;
    padding: 0 .22rem;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--accent);
    font-size: .6rem;
    font-weight: 600;
    line-height: 1;
  }
  .tab.on .tab-badge {
    background: var(--accent-contrast);
    color: var(--accent);
  }
  .header-action {
    width: 2rem;
    height: 2rem;
    flex: 0 0 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
  }
  .header-action:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .header-action.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .filter-control { position: relative; flex: 0 0 auto; }
  .filter-count {
    position: absolute;
    top: -.15rem;
    right: -.15rem;
    min-width: .85rem;
    height: .85rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 .16rem;
    border-radius: var(--radius-sm);
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: .58rem;
    font-weight: 700;
  }
  .menu-backdrop,
  .menu-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 20;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
    filter: none;
  }
  .filter-menu {
    position: absolute;
    z-index: 21;
    top: calc(100% + .35rem);
    right: 0;
    width: 14rem;
    display: flex;
    flex-direction: column;
    padding: .3rem;
    max-height: 20rem;
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
  }
  .filter-menu strong { padding: .45rem .55rem .2rem; }
  .filter-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: .45rem .55rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .filter-menu button:hover { background: var(--surface-hover); filter: none; }
  .filter-menu .clear-filter { color: var(--accent); font-weight: 600; }
  .separator { height: 1px; margin: .35rem .25rem; background: var(--border); }
  .search-row {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: .4rem;
    margin: 0 .45rem .45rem;
    padding: 0 .55rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
  }
  .search-row:focus-within { border-color: var(--accent); color: var(--accent); }
  .search-row input {
    width: 100%;
    min-width: 0;
    padding: .42rem 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .body { flex: 1; min-height: 0; }
</style>
