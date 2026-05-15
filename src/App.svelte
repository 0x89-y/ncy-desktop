<script lang="ts">
  import { onMount } from "svelte";
  import { activeView, bootstrap, settings } from "./lib/stores/app";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ChatList from "./lib/components/ChatList.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import PairingView from "./lib/views/PairingView.svelte";
  import SearchView from "./lib/views/SearchView.svelte";
  import SettingsView from "./lib/views/SettingsView.svelte";
  import StarredView from "./lib/views/StarredView.svelte";
  import RemindersView from "./lib/views/RemindersView.svelte";

  let booted = $state(false);

  onMount(async () => {
    try {
      await bootstrap();
    } finally {
      booted = true;
    }
  });

  $effect(() => {
    const t = $settings?.theme ?? "system";
    document.documentElement.dataset.theme = t;
    if (t === "light" || t === "dark") {
      document.documentElement.style.colorScheme = t;
    } else {
      document.documentElement.style.colorScheme = "";
    }
  });
</script>

<div class="shell">
  <Sidebar />

  {#if !booted}
    <div class="boot muted">Loading…</div>
  {:else if $activeView === "chats"}
    <div class="chats-pane">
      <div class="chat-list-pane">
        <ChatList />
      </div>
      <div class="chat-view-pane">
        <ChatView />
      </div>
    </div>
  {:else if $activeView === "search"}
    <SearchView />
  {:else if $activeView === "starred"}
    <StarredView />
  {:else if $activeView === "reminders"}
    <RemindersView />
  {:else if $activeView === "pairing"}
    <PairingView />
  {:else if $activeView === "settings"}
    <SettingsView />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    height: 100%;
    width: 100%;
  }
  .boot {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .chats-pane {
    flex: 1;
    display: grid;
    grid-template-columns: 320px 1fr;
    min-width: 0;
  }
  .chat-list-pane {
    border-right: 1px solid var(--outline-variant);
    background: var(--surface);
    overflow: hidden;
  }
  .chat-view-pane {
    overflow: hidden;
    min-width: 0;
  }
</style>
