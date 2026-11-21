/// <reference types="vite/client" />
/**
 * Dev-only stand-in for the Tauri back end, so the UI can be run and reviewed in a
 * plain browser (`npm run dev`) with sample chats instead of anyone's real notes.
 * Inert inside Tauri, and stripped from production builds by the DEV guard.
 *
 * URL parameters: ?theme=dark|light, ?accent=<name>, ?chat=<id> (opens a chat),
 * ?view=settings&tab=<tab> (opens Settings), ?pair=1 (shows the "Allow this
 * device?" request after a moment).
 */
import type { AppSettings, ChatSummary, MaterializedChat, Message } from "../lib/ipc";
import { activeView, selectChat, settingsTab, type SettingsTab } from "../lib/stores/app";

type Handler = (args: any) => unknown;

export function installTauriMock(): void {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;

  const params = new URLSearchParams(location.search);
  const now = Date.now();
  const min = 60_000, hour = 60 * min, day = 24 * hour;
  let seq = 0;
  const id = () => `m${++seq}`;

  function text(chatId: string, at: number, parts: [string, boolean?][], extra: Partial<Message> = {}): Message {
    return {
      id: id(), chatId, createdAt: at, editedAt: null, type: "TEXT",
      content: [{ type: "PARAGRAPH", spans: parts.map(([t, b]) => ({ text: t, bold: !!b })) }],
      voiceFile: null, voiceDurationMs: null, isStarred: false, imageFiles: [], fileAttachments: [],
      tags: [], reminderAt: null, checklistItems: [], reactions: [], transcript: null,
      replyToId: null, replyToPreview: null, replyToType: null, replyToCreatedAt: null,
      authorDeviceId: "desk", ...extra,
    };
  }
  function list(chatId: string, at: number, items: [string, boolean][]): Message {
    return { ...text(chatId, at, []), type: "CHECKLIST", content: [],
      checklistItems: items.map(([t, c], i) => ({ id: `${chatId}-${i}`, text: t, checked: c })) };
  }

  const chats: Record<string, { meta: Partial<MaterializedChat["metadata"]>; messages: Message[] }> = {
    ideas: { meta: { name: "Ideas", isPinned: true, bubbleColor: "#B4471F", remoteMessagesOnLeft: true, tags: ["work", "ideas"] }, messages: [] },
    groceries: { meta: { name: "Groceries", isPinned: true, bubbleColor: "#2F855A" }, messages: [] },
    work: { meta: { name: "Work log", bubbleColor: "#2B6CB0" }, messages: [] },
    reading: { meta: { name: "Reading list", bubbleColor: "#805AD5" }, messages: [] },
    lisbon: { meta: { name: "Lisbon trip", bubbleColor: "#0F766E" }, messages: [] },
  };
  const ideaVoice = text("ideas", now - day, [["Maybe pinned chats become cards at the top, with their checklist progress showing."]]);
  chats.ideas.messages = [
    list("ideas", now - 2 * day, [["Audit current screens", true], ["Pick a type pairing", true], ["Mockups for home and chat", false], ["Composer states", false]]),
    ideaVoice,
    text("ideas", now - day + 25 * min, [["Yes, and show the "], ["next reminder", true], [" on the card too."]], {
      authorDeviceId: "pixel", isStarred: true, reactions: ["👍", "💡"],
      replyToId: ideaVoice.id, replyToPreview: "Maybe pinned chats become cards at the top…", replyToType: "TEXT", replyToCreatedAt: ideaVoice.createdAt,
    }),
    text("ideas", now - hour, [["Ask Sam about the sync edge-cases doc"]], { reminderAt: now + 6 * hour }),
  ];
  chats.groceries.messages = [list("groceries", now - 3 * hour, [["Oat milk", false], ["Eggs", true], ["Lemons", false], ["Bread", true], ["Olive oil", true], ["Coffee", false], ["Rice", true], ["Tomatoes", false], ["Basil", false]])];
  chats.work.messages = [text("work", now - day - 3 * hour, [["Shipped 0.9.1: bubbles and the new theme"]])];
  chats.reading.messages = [text("reading", now - 4 * day, [["The Overstory, chapter 12, the banyan bit"]])];
  chats.lisbon.messages = [text("lisbon", now - 8 * day, [["Tram 28 early, before the crowds."]])];

  const preview = (m: Message) => m.type === "CHECKLIST"
    ? `${m.checklistItems.filter((i) => i.checked).length}/${m.checklistItems.length} ${m.checklistItems[0]?.text ?? ""}`
    : m.content.flatMap((b) => b.spans.map((s) => s.text)).join("");
  const summary = (cid: string): ChatSummary => {
    const c = chats[cid], last = c.messages[c.messages.length - 1];
    return {
      id: cid, name: c.meta.name!, avatarFile: null, createdAt: now - 30 * day, isPinned: !!c.meta.isPinned,
      isArchived: false, isProtected: false, tags: [], lastMessageAt: last?.createdAt ?? null,
      lastMessagePreview: last ? preview(last) : null, messageCount: c.messages.length,
      bubbleColor: c.meta.bubbleColor ?? null, bubbleGradient: false, hideName: false,
      profileId: "personal", profileName: "Personal",
    };
  };

  let settings: AppSettings = {
    port: 8765, deviceName: "Desk-PC", theme: params.get("theme") ?? "light", discoverable: false,
    accent: params.get("accent") ?? "inkblue", transcriptionEnabled: false,
    automaticFileDownloadLimitBytes: 100 * 1024 * 1024, mediaBackfillCutoffMs: 0,
  };
  let prefs = {
    startMinimized: false, closeToTray: true, askCloseToTray: false, launchOnStartup: false,
    askBeforeDeleting: true, autoCheckUpdates: false, autoBackupEnabled: false, autoBackupIntervalHours: 24,
    autoBackupFolder: null, autoBackupRetention: 5, lastAutoBackupAt: null, lastAutoBackupError: null,
    syncServerEnabled: true,
  };
  const qr = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29"><rect width="29" height="29" fill="#fff"/>${
    Array.from({ length: 29 * 29 }, (_, i) => ((i * 7919) % 13) < 6 ? `<rect x="${i % 29}" y="${Math.floor(i / 29)}" width="1" height="1"/>` : "").join("")}</svg>`;

  const handlers: Record<string, Handler> = {
    list_chats: () => Object.keys(chats).map(summary),
    get_chat: ({ chatId }) => {
      const c = chats[chatId];
      if (!c) return null;
      const s = summary(chatId);
      return {
        metadata: { id: chatId, name: s.name, avatarFile: null, backgroundFile: null, createdAt: s.createdAt,
          isPinned: s.isPinned, bubbleColor: s.bubbleColor, bubbleGradient: false, isArchived: !!c.meta.isArchived, tags: c.meta.tags ?? [],
          isProtected: false, hideName: false, remoteMessagesOnLeft: !!c.meta.remoteMessagesOnLeft, retentionDays: c.meta.retentionDays ?? null },
        messages: c.messages, lastMessageAt: s.lastMessageAt, lastMessagePreview: s.lastMessagePreview,
        isUnlocked: true, profileId: "personal", profileName: "Personal",
      } satisfies MaterializedChat;
    },
    author_update_chat_meta: ({ chatId, ...patch }) => {
      const meta = chats[chatId]?.meta;
      if (!meta) return null;
      for (const [k, v] of Object.entries(patch)) {
        if (v === null) continue;
        (meta as any)[k] = k === "bubbleColor" ? v || null : k === "retentionDays" ? v || null : v;
      }
      return null;
    },
    get_settings: () => settings,
    update_settings: ({ settings: next }) => (settings = { ...settings, ...next }),
    get_desktop_prefs: () => prefs,
    update_desktop_prefs: ({ prefs: next }) => (prefs = { ...prefs, ...next }),
    list_pairings: () => [{ pairing: { id: "pr1", label: "Pixel 8", phoneId: "pixel", phoneName: "Pixel 8", createdAt: now - 40 * day, lastSeenAt: now - 2 * min }, payloadJson: "{}", qrSvg: qr }],
    create_pairing: ({ label }) => ({ pairing: { id: "pr2", label, phoneId: null, phoneName: null, createdAt: now, lastSeenAt: null }, payloadJson: "{}", qrSvg: qr }),
    list_peers: () => [],
    get_server_status: () => ({ status: "running", addr: "192.168.1.31:8765" }),
    get_device_identity: () => ({ deviceId: "desk", name: "Desk-PC" }),
    list_lan_ips: () => ["192.168.1.31"],
    get_advertised_host: () => "192.168.1.31",
    list_pair_attempts: () => [],
    transcription_status: () => ({ cliPresent: false, runtimeVersion: null, modelPresent: false, modelSizeBytes: null,
      modelId: "base", modelLabel: "Base", cachedModelIds: [], unusedModelBytes: 0, dir: "C:\\Ncy\\whisper", ffmpegPresent: false }),
    blob_exists: () => false,
    blob_evicted: () => false,
    read_blob: () => null,
    "plugin:app|version": () => "0.8.0",
    "plugin:app|name": () => "Ncy",
    "plugin:window|is_maximized": () => false,
    "plugin:window|is_visible": () => true,
  };

  const callbacks = new Map<number, (payload: unknown) => void>();
  const listeners = new Map<string, number[]>();
  let nextCb = 1;
  const emit = (event: string, payload: unknown) =>
    (listeners.get(event) ?? []).forEach((cb) => callbacks.get(cb)?.({ event, id: 0, payload }));

  (window as any).__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback: (cb: (p: unknown) => void) => { const n = nextCb++; callbacks.set(n, cb); return n; },
    invoke: async (cmd: string, args: any = {}) => {
      if (cmd === "plugin:event|listen") {
        listeners.set(args.event, [...(listeners.get(args.event) ?? []), args.handler]);
        return args.handler;
      }
      if (cmd.startsWith("plugin:event|")) return null;
      const h = handlers[cmd];
      return h ? h(args) : null;
    },
    convertFileSrc: (p: string) => p,
  };
  (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };

  setTimeout(() => emit("sync-event", { kind: "peer-connected", peer: "192.168.1.40", pairingId: "pr1", phoneId: "pixel" }), 300);
  const openChat = params.get("chat");
  if (openChat) setTimeout(() => void selectChat(openChat), 500);
  if (params.has("chatsettings")) {
    setTimeout(() => (document.querySelector('[aria-label="Chat settings"]') as HTMLElement | null)?.click(), 900);
  }
  if (params.get("view") === "settings") {
    setTimeout(() => {
      settingsTab.set((params.get("tab") ?? "general") as SettingsTab);
      activeView.set("settings");
    }, 500);
  }
  if (params.has("pair")) {
    setTimeout(() => emit("sync-event", { kind: "pair-requested", sessionId: "s1", peer: "192.168.1.40",
      phoneId: "pixel2", phoneName: "Pixel 8", safetyCode: "482917" }), 800);
  }
}
