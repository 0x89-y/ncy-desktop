<script lang="ts">
  import { onMount } from "svelte";
  import { activeView, bootstrap, settings, serverStatus, connectedPeers, pairings } from "./lib/stores/app";
  import ChatListPane from "./lib/components/ChatListPane.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import WindowControls from "./lib/components/WindowControls.svelte";
  import ResizeHandles from "./lib/components/ResizeHandles.svelte";
  import PairRequestModal from "./lib/components/PairRequestModal.svelte";
  import AppUpdateToast from "./lib/components/AppUpdateToast.svelte";
  import CloseToTrayPrompt from "./lib/components/CloseToTrayPrompt.svelte";
  import ImageViewer from "./lib/components/ImageViewer.svelte";
  import SettingsView from "./lib/views/SettingsView.svelte";
  import { initAppUpdater } from "./lib/stores/updater";
  import { initClosePrompt } from "./lib/stores/tray";
  import { initAutomaticBackups } from "./lib/stores/backups";
  import { Settings as SettingsIcon } from "@lucide/svelte";

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
    <div class="chats-pane">
      <div class="chat-list-pane">
        <ChatListPane />
      </div>
      <div class="chat-view-pane">
        <ChatView />
      </div>
    </div>
  {:else if $activeView === "settings"}
    <SettingsView />
  {/if}
</div>

<PairRequestModal />

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
  .dot.running { background: #34a853; }
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
