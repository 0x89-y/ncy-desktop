<script lang="ts">
  import { onMount } from "svelte";
  import { activeView, bootstrap, settings } from "./lib/stores/app";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ChatList from "./lib/components/ChatList.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import WindowControls from "./lib/components/WindowControls.svelte";
  import ResizeHandles from "./lib/components/ResizeHandles.svelte";
  import PairingView from "./lib/views/PairingView.svelte";
  import SearchView from "./lib/views/SearchView.svelte";
  import SettingsView from "./lib/views/SettingsView.svelte";
  import StarredView from "./lib/views/StarredView.svelte";
  import RemindersView from "./lib/views/RemindersView.svelte";

  let booted = $state(false);

  // System "prefers-color-scheme" state, kept reactive so `theme: 'system'`
  // resolves to a real `light`/`dark` value the [data-theme='dark'] CSS
  // selector can match (Acy's `appliedTheme` pattern).
  let systemDark = $state(
    typeof matchMedia !== "undefined" &&
      matchMedia("(prefers-color-scheme: dark)").matches,
  );
  onMount(() => {
    if (typeof matchMedia === "undefined") return;
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => { systemDark = e.matches; };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  });

  onMount(async () => {
    try {
      await bootstrap();
    } finally {
      booted = true;
    }
  });

  $effect(() => {
    const setting = $settings?.theme ?? "system";
    const applied = setting === "system" ? (systemDark ? "dark" : "light") : setting;
    document.documentElement.dataset.theme = applied;
    document.documentElement.style.colorScheme = applied;
    document.documentElement.dataset.accent = $settings?.accent ?? "purple";
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  <span class="brand mono" data-tauri-drag-region>ncy</span>
  <div class="drag" data-tauri-drag-region></div>
  <WindowControls />
</header>

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

<ResizeHandles />

<style>
  .titlebar {
    height: 32px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 4px 0 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .brand {
    font-size: 0.95rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text);
  }
  .brand::before {
    content: "0x";
    color: var(--accent);
  }
  .drag { flex: 1; height: 100%; }

  .shell {
    display: flex;
    height: calc(100% - 32px);
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
    border-right: 1px solid var(--border);
    background: var(--surface);
    overflow: hidden;
  }
  .chat-view-pane {
    overflow: hidden;
    min-width: 0;
  }
</style>
