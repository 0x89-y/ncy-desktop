<script lang="ts">
  import { api } from "../ipc";
  import { activeTag, activeView, allTags, upcomingReminders, chatListCollapsed, chatPane, chats, connectedPeers, lastSyncEvent, newChatRequest, pairings, pinnedOnly, selectChat, settingsTab, showArchived, type ChatPane } from "../stores/app";
  import { Bell, Check, Filter, Search, Star, X, type Icon as IconType } from "@lucide/svelte";
  import type { Message } from "../ipc";
  import ChatList from "./ChatList.svelte";
  import ProfileMenu from "./ProfileMenu.svelte";
  import StarredPane from "./chatpane/StarredPane.svelte";
  import RemindersPane from "./chatpane/RemindersPane.svelte";

  const tabs: { key: ChatPane; label: string; title: string; icon: typeof IconType | null }[] = [
    { key: "chats", label: "All", title: "Chats", icon: null },
    { key: "starred", label: "Starred", title: "Starred", icon: Star },
    { key: "reminders", label: "Reminders", title: "Reminders", icon: Bell },
  ];
  const paneTitle = $derived(tabs.find((t) => t.key === $chatPane)?.title ?? "Chats");

  let reminderCount = $state(0);
  /** The next reminder to ring, shown above the chats. */
  let upNext = $state<{ chatId: string; chatName: string; text: string; at: number; more: number } | null>(null);

  function noteText(m: Message): string {
    if (m.type === "CHECKLIST") return m.checklistItems.map((i) => i.text).join(", ");
    if (m.type === "VOICE") return m.transcript ?? "Voice note";
    const text = m.content.flatMap((b) => b.spans.map((s) => s.text)).join("").trim();
    return text || (m.type === "IMAGE" ? "Photo" : m.type === "FILE" ? "File" : "Note");
  }

  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  function ago(ms: number): string {
    const minutes = Math.round((ms - Date.now()) / 60_000);
    if (Math.abs(minutes) < 60) return rtf.format(minutes, "minute");
    const hours = Math.round(minutes / 60);
    if (Math.abs(hours) < 24) return rtf.format(hours, "hour");
    return rtf.format(Math.round(hours / 24), "day");
  }
  function clock(ms: number): string {
    const d = new Date(ms), now = new Date();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === now.toDateString()) return time;
    return `${d.toLocaleDateString([], { weekday: "short" })} ${time}`;
  }

  /** One line under the title: who this computer is syncing with. Hidden when nothing is paired. */
  const syncLine = $derived.by(() => {
    const paired = $pairings.map((p) => p.pairing).filter((p) => p.phoneId || p.lastSeenAt);
    if (!paired.length) return null;
    const latest = [...paired].sort((a, b) => (b.lastSeenAt ?? 0) - (a.lastSeenAt ?? 0))[0];
    const name = latest.phoneName || latest.label;
    const when = latest.lastSeenAt ? ` · ${ago(latest.lastSeenAt)}` : "";
    if ($connectedPeers.size > 0) {
      const more = $connectedPeers.size - 1;
      return { online: true, text: `Synced with ${name}${more > 0 ? ` and ${more} more` : ""}${when}` };
    }
    // Offline, no one device is named: with several paired, naming the most recent
    // one suggested it was the one that mattered. Devices & sync lists them all.
    return { online: false, text: latest.lastSeenAt ? `Not connected · last synced ${ago(latest.lastSeenAt)}` : "Paired · not synced yet" };
  });
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
    const upcoming: { chatId: string; chatName: string; text: string; at: number }[] = [];
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        if (!m.reminderAt || m.reminderAt < now) continue;
        upcoming.push({ chatId: c.id, chatName: c.name, text: noteText(m), at: m.reminderAt });
      }
    }
    if (run !== reminderCountRun) return;
    upcoming.sort((a, b) => a.at - b.at);
    reminderCount = upcoming.length;
    upNext = upcoming.length ? { ...upcoming[0], more: upcoming.length - 1 } : null;
    upcomingReminders.set(upcoming);
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
    <span class="spacer"></span>
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

  <h1 class="pane-title">{paneTitle}</h1>
  {#if syncLine}
    <button type="button" class="sync-line" title="Devices & sync" onclick={() => { settingsTab.set("sync"); activeView.set("settings"); }}>
      <span class="dot" class:online={syncLine.online}></span>{syncLine.text}
    </button>
  {/if}
  <div class="chips" role="tablist" aria-label="Chat views">
    {#each tabs as t (t.key)}
      {@const Icon = t.icon}
      <button
        type="button"
        role="tab"
        class="chip-tab"
        class:on={$chatPane === t.key}
        aria-selected={$chatPane === t.key}
        onclick={() => chatPane.set(t.key)}
      >
        {#if Icon}<Icon size={13} strokeWidth={2.25} aria-hidden="true" />{/if}
        {t.label}
        {#if t.key === "chats"}<span class="count">{$chats.filter((c) => !c.isArchived).length}</span>{/if}
        {#if t.key === "reminders" && reminderCount > 0}<span class="count">{reminderCount}</span>{/if}
      </button>
    {/each}
  </div>

  {#if upNext && $chatPane === "chats" && !searchOpen}
    <button type="button" class="up-next" onclick={() => selectChat(upNext!.chatId)}>
      <Bell size={16} strokeWidth={2.25} aria-hidden="true" />
      <span class="up-body">
        <span class="up-kicker">Up next · {clock(upNext.at)}</span>
        <span class="up-text">{upNext.text}</span>
        <span class="up-chat">in {upNext.chatName}</span>
      </span>
      {#if upNext.more > 0}<span class="up-more">+{upNext.more}</span>{/if}
    </button>
  {/if}

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
    gap: .25rem;
    padding: .5rem .5rem 0 .85rem;
    flex-shrink: 0;
  }
  .pane-title {
    padding: .1rem 1.1rem 0;
    font-size: 1.85rem;
    font-weight: 600;
    letter-spacing: -.03em;
    line-height: 1.15;
  }
  .sync-line {
    display: flex;
    align-items: center;
    gap: .45rem;
    margin: .1rem .6rem 0;
    padding: .2rem .5rem;
    font-size: .8rem;
    color: var(--text-muted);
    background: transparent;
    border-radius: var(--radius-sm);
    text-align: left;
    align-self: flex-start;
  }
  .sync-line:hover { background: var(--surface-hover); color: var(--text); }
  .sync-line .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--radius-pill);
    border: 1.5px solid currentColor;
    flex: none;
  }
  .sync-line .dot.online { background: var(--online); border-color: var(--online); }
  .chips {
    display: flex;
    gap: .4rem;
    padding: .7rem 1.1rem .75rem;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .chip-tab {
    display: inline-flex;
    align-items: center;
    gap: .3rem;
    padding: .26rem .7rem;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text);
    font-size: .8rem;
    white-space: nowrap;
  }
  .chip-tab:hover:not(.on) { background: var(--surface-hover); }
  /* Selected: the accent tint and bolder text, so it reads without colour alone. */
  .chip-tab.on {
    background: var(--accent-selected);
    border-color: var(--accent);
    color: var(--on-accent-soft);
    font-weight: 600;
  }
  .chip-tab .count { font-family: var(--font-mono); font-size: .7rem; color: var(--text-muted); font-weight: 400; }
  .chip-tab.on .count { color: var(--on-accent-soft); }
  .up-next {
    display: flex;
    align-items: flex-start;
    gap: .65rem;
    margin: .75rem .75rem .1rem;
    padding: .6rem .75rem;
    border-radius: 12px;
    background: var(--accent-soft);
    border: 1px solid var(--accent-edge);
    color: var(--on-accent-soft);
    text-align: left;
    flex-shrink: 0;
  }
  .up-next:hover { background: var(--accent-soft); filter: brightness(.98); }
  .up-next :global(svg) { margin-top: .15rem; flex: none; }
  .up-body { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .up-kicker { font-family: var(--font-mono); font-size: .66rem; letter-spacing: .07em; text-transform: uppercase; }
  .up-text { color: var(--text); font-weight: 500; font-size: .88rem; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .up-chat { font-size: .78rem; }
  .up-more { font-family: var(--font-mono); font-size: .72rem; align-self: center; }
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
