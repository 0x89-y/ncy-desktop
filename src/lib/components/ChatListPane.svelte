<script lang="ts">
  import { chatPane, type ChatPane } from "../stores/app";
  import { MessageSquare, Search, Star, Clock, type Icon as IconType } from "@lucide/svelte";
  import ChatList from "./ChatList.svelte";
  import SearchPane from "./chatpane/SearchPane.svelte";
  import StarredPane from "./chatpane/StarredPane.svelte";
  import RemindersPane from "./chatpane/RemindersPane.svelte";

  const tabs: { key: ChatPane; label: string; icon: typeof IconType }[] = [
    { key: "chats", label: "Chats", icon: MessageSquare },
    { key: "search", label: "Search", icon: Search },
    { key: "starred", label: "Starred", icon: Star },
    { key: "reminders", label: "Reminders", icon: Clock },
  ];
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
        <Icon size={16} strokeWidth={2.25} />
        <span class="lbl">{t.label}</span>
      </button>
    {/each}
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
  .pane { height: 100%; min-height: 0; }
  .tabs {
    display: flex;
    gap: 2px;
    padding: .5rem .5rem 0;
    flex-shrink: 0;
  }
  .tab {
    flex: 1;
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: .15rem;
    padding: .4rem .2rem;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    font-size: .68rem;
    font-weight: 500;
    cursor: pointer;
    line-height: 1;
  }
  .tab:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .tab.on { color: var(--text); border-bottom-color: var(--accent); }
  .lbl { white-space: nowrap; }
  .body { flex: 1; min-height: 0; }
</style>
