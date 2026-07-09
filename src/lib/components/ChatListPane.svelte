<script lang="ts">
  import { api } from "../ipc";
  import { chatPane, chats, lastSyncEvent, showArchived, type ChatPane } from "../stores/app";
  import { MessageSquare, Search, Star, Bell, Archive, type Icon as IconType } from "@lucide/svelte";
  import ChatList from "./ChatList.svelte";
  import SearchPane from "./chatpane/SearchPane.svelte";
  import StarredPane from "./chatpane/StarredPane.svelte";
  import RemindersPane from "./chatpane/RemindersPane.svelte";

  const tabs: { key: ChatPane; label: string; icon: typeof IconType }[] = [
    { key: "chats", label: "Chats", icon: MessageSquare },
    { key: "search", label: "Search", icon: Search },
    { key: "starred", label: "Starred", icon: Star },
    { key: "reminders", label: "Reminders", icon: Bell },
  ];

  let reminderCount = $state(0);
  let reminderCountRun = 0;

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

<div class="pane col">
  <div class="tabs">
    {#each tabs as t (t.key)}
      {@const Icon = t.icon}
      <button
        type="button"
        class="tab"
        class:on={$chatPane === t.key}
        title={t.label}
        aria-label={t.label}
        onclick={() => chatPane.set(t.key)}
      >
        <span class="icon-wrap">
          <Icon size={17} strokeWidth={2.25} />
          {#if t.key === "reminders" && reminderCount > 0}
            <span class="tab-badge" aria-label={`${reminderCount} reminders`}>
              {reminderCount}
            </span>
          {/if}
        </span>
      </button>
    {/each}
    <button
      type="button"
      class="tab"
      class:on={$showArchived}
      title={$showArchived ? "Hide archived chats" : "Show archived chats"}
      aria-label={$showArchived ? "Hide archived chats" : "Show archived chats"}
      onclick={() => showArchived.update((v) => !v)}
    >
      <span class="icon-wrap">
        <Archive size={17} strokeWidth={2.25} />
      </span>
    </button>
  </div>

  <div class="body">
    {#if $chatPane === "chats"}
      <ChatList />
    {:else if $chatPane === "search"}
      <SearchPane />
    {:else if $chatPane === "starred"}
      <StarredPane />
    {:else if $chatPane === "reminders"}
      <RemindersPane />
    {/if}
  </div>
</div>

<style>
  .pane { height: calc(100% - 1rem); min-height: 0; }
  .pane {
    margin: .5rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tab {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: .5rem .2rem;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-left: 1px solid var(--border);
    border-radius: 0;
    font-size: .68rem;
    font-weight: 500;
    cursor: pointer;
    line-height: 1;
  }
  .tab:first-child { border-left: none; }
  .tab:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .tab.on { background: var(--accent); color: var(--accent-contrast); }
  .icon-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.25rem;
  }
  .tab-badge {
    position: absolute;
    top: -.35rem;
    right: -.55rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1rem;
    height: 1rem;
    padding: 0 .3rem;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--accent);
    font-size: .65rem;
    font-weight: 600;
    line-height: 1;
  }
  .tab.on .tab-badge {
    background: var(--accent-contrast);
    color: var(--accent);
  }
  .body { flex: 1; min-height: 0; }
</style>
