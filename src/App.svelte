<script lang="ts">
  import { normalizeAccent } from "./lib/accent";
  import { onMount } from "svelte";
  import { activeView, bootstrap, chatListCollapsed, settings, serverStatus, connectedPeers, pairings } from "./lib/stores/app";
  import ChatListPane from "./lib/components/ChatListPane.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import ChatRail from "./lib/components/ChatRail.svelte";
  import WindowControls from "./lib/components/WindowControls.svelte";
  import ResizeHandles from "./lib/components/ResizeHandles.svelte";
  import PairRequestModal from "./lib/components/PairRequestModal.svelte";
  import AppUpdateToast from "./lib/components/AppUpdateToast.svelte";
  import CloseToTrayPrompt from "./lib/components/CloseToTrayPrompt.svelte";
  import ConfirmPrompt from "./lib/components/ConfirmPrompt.svelte";
  import ImageViewer from "./lib/components/ImageViewer.svelte";
  import SettingsView from "./lib/views/SettingsView.svelte";
  import { initAppUpdater } from "./lib/stores/updater";
  import { initClosePrompt } from "./lib/stores/tray";
  import { initAutomaticBackups } from "./lib/stores/backups";
  import { initChatRetention } from "./lib/stores/retention";
  import { PanelLeft, Settings as SettingsIcon } from "@lucide/svelte";

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
    // Attach before `bootstrap` — Rust holds the ✕ and emits `close-requested`
    // once, with no buffering for late listeners. Miss it and a close during
    // startup would be swallowed, leaving a window that won't shut.
    await initClosePrompt();
    try {
      await bootstrap();
    } finally {
      booted = true;
    }
    initAppUpdater();
    initAutomaticBackups();
    initChatRetention();
  });

  $effect(() => {
    const setting = $settings?.theme ?? "system";
    const applied = setting === "system" ? (systemDark ? "dark" : "light") : setting;
    document.documentElement.dataset.theme = applied;
    document.documentElement.style.colorScheme = applied;
    document.documentElement.dataset.accent = normalizeAccent($settings?.accent);
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  {#if $activeView === "chats"}
    <button
      type="button"
      class="tb-btn"
      title={$chatListCollapsed ? "Expand chat list" : "Collapse chat list"}
      aria-label={$chatListCollapsed ? "Expand chat list" : "Collapse chat list"}
      aria-expanded={!$chatListCollapsed}
      onclick={() => chatListCollapsed.update((v) => !v)}
    >
      <PanelLeft size={16} strokeWidth={2.25} />
    </button>
  {/if}
  <div class="drag" data-tauri-drag-region></div>

  <div
    class="status-dot"
    title={$connectedPeers.size > 0
      ? `${$serverStatus} · ${$connectedPeers.size} phone${$connectedPeers.size === 1 ? "" : "s"} online`
      : $serverStatus}
  >
    <span class="dot" class:running={$serverStatus === "running"} class:err={$serverStatus === "error"}></span>
    {#if $connectedPeers.size > 0}<span class="tiny count">{$connectedPeers.size}</span>{/if}
  </div>

  <button
    type="button"
    class="tb-btn"
    class:active={$activeView === "settings"}
    title="Settings"
    aria-label="Settings"
    onclick={() => activeView.set("settings")}
  >
    <SettingsIcon size={16} strokeWidth={2.25} />
    {#if $pairings.length > 0}<span class="badge">{$pairings.length}</span>{/if}
  </button>

  <WindowControls />
</header>

<div class="shell">
  {#if !booted}
    <div class="boot muted">Loading…</div>
  {:else if $activeView === "chats"}
    <div class="chats-pane" class:list-collapsed={$chatListCollapsed}>
      <!-- Hidden with CSS rather than unmounted, so search text, filters and
           the list's keyboard shortcuts survive collapsing. -->
      <div class="chat-list-pane">
        <ChatListPane />
      </div>
      {#if $chatListCollapsed}
        <div class="chat-rail-pane"><ChatRail /></div>
      {/if}
      <div class="chat-view-pane">
        <ChatView />
      </div>
    </div>
  {:else if $activeView === "settings"}
    <SettingsView />
  {/if}
</div>

<PairRequestModal />
<ConfirmPrompt />

<ImageViewer />

<CloseToTrayPrompt />

<AppUpdateToast />

<ResizeHandles />

<style>
  .titlebar {
    height: 32px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 4px 0 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .drag { flex: 1; height: 100%; }

  /* Titlebar action button (Settings gear / back arrow). Non-draggable so it
     stays clickable inside the drag region. */
  .tb-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    line-height: 0;
  }
  .tb-btn:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .tb-btn.active { background: var(--surface-2); color: var(--text); }
  .tb-btn .badge {
    position: absolute;
    top: -3px;
    right: -3px;
    min-width: .9rem;
    height: .9rem;
    padding: 0 .25rem;
    font-size: .6rem;
  }

  .status-dot {
    display: inline-flex;
    align-items: center;
    gap: .25rem;
  }
  .status-dot .count { color: var(--text-muted); }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--outline);
    flex-shrink: 0;
  }
  .dot.running { background: var(--online); }
  .dot.err { background: var(--error); }

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
  .chats-pane.list-collapsed { grid-template-columns: 56px 1fr; }
  .chats-pane.list-collapsed .chat-list-pane { display: none; }
  .chat-rail-pane {
    border-right: 1px solid var(--border);
    background: var(--bg);
    overflow: hidden;
    min-height: 0;
  }
  .chat-list-pane {
    border-right: 1px solid var(--border);
    background: var(--bg);
    overflow: hidden;
  }
  .chat-view-pane {
    overflow: hidden;
    min-width: 0;
  }
</style>
