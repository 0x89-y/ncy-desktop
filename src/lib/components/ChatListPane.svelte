<script lang="ts">
  import { api } from "../ipc";
  import { chatPane, chats, lastSyncEvent, type ChatPane } from "../stores/app";
  import { MessageSquare, Star, Bell, type Icon as IconType } from "@lucide/svelte";
  import ChatList from "./ChatList.svelte";
  import StarredPane from "./chatpane/StarredPane.svelte";
  import RemindersPane from "./chatpane/RemindersPane.svelte";

  const tabs: { key: ChatPane; label: string; icon: typeof IconType }[] = [
    { key: "chats", label: "Chats", icon: MessageSquare },
    { key: "starred", label: "Favorites", icon: Star },
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
  <div class="tabs-row">
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
  </div>

  <div class="body">
    {#if $chatPane === "chats"}
      <ChatList />
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
    border-radius: 5px;
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
  .body { flex: 1; min-height: 0; }
</style>
