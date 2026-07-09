import { writable, derived, get } from "svelte/store";
import { api, type ChatSummary, type MaterializedChat, type AppSettings, type PairingDescriptor, type PeerView, type SyncEventPayload, type DeviceIdentityView } from "../ipc";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export const chats = writable<ChatSummary[]>([]);
export const activeChatId = writable<string | null>(null);
export const activeChat = writable<MaterializedChat | null>(null);
export const pairings = writable<PairingDescriptor[]>([]);
export const peers = writable<PeerView[]>([]);
export const settings = writable<AppSettings | null>(null);
export const identity = writable<DeviceIdentityView | null>(null);
export const lanIps = writable<string[]>([]);
export const advertisedHost = writable<string>("");
export const connectedPeers = writable<Set<string>>(new Set());
export const serverStatus = writable<"starting" | "running" | "error" | "stopped">("starting");
export const serverAddr = writable<string>("");
export const lastSyncEvent = writable<SyncEventPayload | null>(null);
/** Phase 2b — set of blob hashes that have finished downloading this session.
 * Lets blob-rendering components reactively re-fetch when the bytes land. */
export const arrivedBlobs = writable<Set<string>>(new Set());

/** In-flight voice-message transcriptions, keyed by messageId. MessageBubble
 *  reads this to show a "Transcribing…" indicator while the whisper subprocess
 *  runs; the trigger sites (auto after `authorSendVoice`, manual from the ctx
 *  menu) add/remove entries around their `authorTranscribeVoice` call. */
export const transcribing = writable<Set<string>>(new Set());
/** Last transcription error per message, keyed by messageId. MessageBubble
 *  renders it inline under the voice bubble so failures are visible without
 *  needing devtools open. Cleared on the next attempt. */
export const transcriptionErrors = writable<Record<string, string>>({});

export function markTranscribing(messageId: string) {
  transcribing.update((s) => {
    const next = new Set(s);
    next.add(messageId);
    return next;
  });
  transcriptionErrors.update((m) => {
    if (!(messageId in m)) return m;
    const next = { ...m };
    delete next[messageId];
    return next;
  });
}
export function unmarkTranscribing(messageId: string) {
  transcribing.update((s) => {
    if (!s.has(messageId)) return s;
    const next = new Set(s);
    next.delete(messageId);
    return next;
  });
}
export function setTranscriptionError(messageId: string, message: string) {
  transcriptionErrors.update((m) => ({ ...m, [messageId]: message }));
}

/** Discovery-initiated pair requests awaiting user confirmation. FIFO queue —
 *  the modal shows the head; approving / rejecting drops it. Backend times
 *  out after ~60s and drops its side, so entries that outlive that need to
 *  be dismissed by the modal itself. */
export type PendingPairRequest = {
  sessionId: string;
  peer: string;
  phoneId: string;
  phoneName: string;
  /** 6-digit ECDH safety code. Same value is displayed on the phone's
   *  waiting screen — visually compare before confirming. */
  safetyCode: string;
  arrivedAt: number;
};
export const pendingPairRequests = writable<PendingPairRequest[]>([]);
export function enqueuePairRequest(r: Omit<PendingPairRequest, "arrivedAt">) {
  pendingPairRequests.update((q) => [
    ...q.filter((x) => x.sessionId !== r.sessionId),
    { ...r, arrivedAt: Date.now() },
  ]);
}
export function dropPairRequest(sessionId: string) {
  pendingPairRequests.update((q) => q.filter((x) => x.sessionId !== sessionId));
}

export const filterText = writable<string>("");
export const showArchived = writable<boolean>(false);

/** Top-level view. Search/Starred/Reminders now live as tabs inside the chats
 *  view (see `chatPane`); device/sync controls live in settings. */
export type View = "chats" | "settings";
export const activeView = writable<View>("chats");

/** Which sub-view fills the left list pane inside the chats view. */
export type ChatPane = "chats" | "search" | "starred" | "reminders";
export const chatPane = writable<ChatPane>("chats");

export const DEFAULT_QUICK_REACTIONS = ["👍", "❤️", "😂", "🎉", "✅", "🔥"];
export const REACTION_CHOICES = [
  "👍", "❤️", "😂", "🎉", "✅", "🔥",
  "🙏", "👀", "🤔", "😮", "😢", "😡",
  "💯", "⭐", "🚀", "💡", "👏", "🤝",
  "😍", "🥳", "🤯", "😅", "🙌", "✨",
];
const QUICK_REACTIONS_KEY = "ncy:quick-reactions";

function normalizeQuickReactions(value: unknown): string[] {
  if (!Array.isArray(value)) return DEFAULT_QUICK_REACTIONS;
  const allowed = new Set(REACTION_CHOICES);
  const next: string[] = [];
  for (const item of value) {
    if (typeof item !== "string" || !allowed.has(item) || next.includes(item)) continue;
    next.push(item);
    if (next.length >= 8) break;
  }
  return next.length ? next : DEFAULT_QUICK_REACTIONS;
}

function loadQuickReactions(): string[] {
  if (typeof localStorage === "undefined") return DEFAULT_QUICK_REACTIONS;
  try {
    return normalizeQuickReactions(JSON.parse(localStorage.getItem(QUICK_REACTIONS_KEY) || "null"));
  } catch {
    return DEFAULT_QUICK_REACTIONS;
  }
}

const initialQuickReactions = loadQuickReactions();
export const quickReactions = writable<string[]>(initialQuickReactions);
quickReactions.subscribe((v) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(QUICK_REACTIONS_KEY, JSON.stringify(normalizeQuickReactions(v)));
  }
});

export function setQuickReactions(next: string[]) {
  quickReactions.set(normalizeQuickReactions(next));
}

export type MessageDensity = "comfortable" | "compact";
const MESSAGE_DENSITY_KEY = "ncy:message-density";
function normalizeMessageDensity(value: string | null): MessageDensity {
  return value === "compact" ? "compact" : "comfortable";
}
const initialMessageDensity =
  typeof localStorage !== "undefined"
    ? normalizeMessageDensity(localStorage.getItem(MESSAGE_DENSITY_KEY))
    : "comfortable";
export const messageDensity = writable<MessageDensity>(initialMessageDensity);
messageDensity.subscribe((v) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(MESSAGE_DENSITY_KEY, normalizeMessageDensity(v));
  }
});

/** Active section within the settings view. Persisted per-device so reopening
 *  Settings lands on the last-used section. */
export type SettingsTab = "general" | "sync" | "interface" | "transcription" | "backup" | "about";
const SETTINGS_TAB_KEY = "ncy:settings-tab";
const normalizeSettingsTab = (value: string | null): SettingsTab => {
  switch (value) {
    case "general":
    case "sync":
    case "interface":
    case "transcription":
    case "backup":
    case "about":
      return value;
    case "appearance":
      return "interface";
    case "device":
    case "devices":
      return "sync";
    case "voice":
      return "transcription";
    default:
      return "general";
  }
};
const initialSettingsTab =
  typeof localStorage !== "undefined"
    ? normalizeSettingsTab(localStorage.getItem(SETTINGS_TAB_KEY))
    : "general";
export const settingsTab = writable<SettingsTab>(initialSettingsTab);
settingsTab.subscribe((v) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(SETTINGS_TAB_KEY, v);
  }
});

export const activeTag = writable<string | null>(null);
/** Profile sync: which profile bucket the chat list is scoped to.
 *  `null` = all profiles, `""` = the legacy/pre-profile-sync "Default" bucket,
 *  `name:<normalized display name>` = all phone profiles with that display name.
 */
export const activeProfileId = writable<string | null>(null);

export async function refreshChats() {
  const list = await api.listChats();
  chats.set(list);
}

export async function refreshActiveChat() {
  const id = get(activeChatId);
  if (!id) {
    activeChat.set(null);
    return;
  }
  const c = await api.getChat(id);
  activeChat.set(c);
}

export async function selectChat(id: string | null) {
  activeChatId.set(id);
  await refreshActiveChat();
}

export async function refreshPairings() {
  const ps = await api.listPairings();
  pairings.set(ps);
}

export async function refreshPeers() {
  peers.set(await api.listPeers());
}

export async function refreshSettings() {
  settings.set(await api.getSettings());
}

export async function refreshIdentity() {
  identity.set(await api.getDeviceIdentity());
}

export async function refreshLanIps() {
  lanIps.set(await api.listLanIps());
  advertisedHost.set(await api.getAdvertisedHost());
}

/**
 * Permission state for system notifications. Probed lazily on first use so we don't
 * trigger the OS prompt on launch.
 */
let notifPermissionResolved: Promise<boolean> | null = null;
export async function ensureNotificationPermission(): Promise<boolean> {
  if (notifPermissionResolved) return notifPermissionResolved;
  notifPermissionResolved = (async () => {
    try {
      if (await isPermissionGranted()) return true;
      const next = await requestPermission();
      return next === "granted";
    } catch {
      return false;
    }
  })();
  return notifPermissionResolved;
}

async function maybeNotify(chatId: string) {
  // Don't fire if the user is already looking at the desktop.
  try {
    if (await getCurrentWindow().isFocused()) return;
  } catch {
    return;
  }
  // Skip notifications for the active chat anyway — they'd be redundant noise.
  if (get(activeChatId) === chatId) return;

  const chat = get(chats).find((c) => c.id === chatId);
  if (!chat) return;
  // Protected chats: don't leak content through the notification body.
  const body = chat.isProtected
    ? "New message"
    : (chat.lastMessagePreview ?? "New message");

  if (!(await ensureNotificationPermission())) return;
  try {
    sendNotification({ title: chat.name, body });
  } catch {
    /* swallow — notification failure shouldn't break the event handler */
  }
}

export async function bootstrap() {
  await Promise.all([
    refreshChats(),
    refreshPairings(),
    refreshPeers(),
    refreshSettings(),
    refreshIdentity(),
    refreshLanIps(),
  ]);

  api.onSyncEvent(async (ev) => {
    lastSyncEvent.set(ev);
    switch (ev.kind) {
      case "server-started":
        serverStatus.set("running");
        serverAddr.set(ev.addr);
        break;
      case "server-error":
        serverStatus.set("error");
        break;
      case "peer-connected":
        connectedPeers.update((s) => {
          const next = new Set(s);
          next.add(ev.peer);
          return next;
        });
        await refreshPairings();
        await refreshPeers();
        break;
      case "peer-disconnected":
        connectedPeers.update((s) => {
          const next = new Set(s);
          next.delete(ev.peer);
          return next;
        });
        await refreshPeers();
        break;
      case "event-received":
        await refreshChats();
        if (get(activeChatId) === ev.chatId) {
          await refreshActiveChat();
        }
        // Only inbound events trigger notifications; locally-authored ones never do.
        // `refreshChats` ran first so `maybeNotify` reads the fresh preview.
        void maybeNotify(ev.chatId);
        break;
      case "event-authored":
        await refreshChats();
        if (get(activeChatId) === ev.chatId) {
          await refreshActiveChat();
        }
        break;
      case "blob-arrived":
        arrivedBlobs.update((s) => {
          const next = new Set(s);
          next.add(ev.hash);
          return next;
        });
        break;
      case "pair-requested":
        enqueuePairRequest({
          sessionId: ev.sessionId,
          peer: ev.peer,
          phoneId: ev.phoneId,
          phoneName: ev.phoneName,
          safetyCode: ev.safetyCode,
        });
        break;
    }
  });

  // The `server-started` bus event almost always fires before this listener is
  // attached (Rust binds the socket during setup, the webview boots later, and
  // Tauri events aren't buffered for late listeners). So read the current status
  // directly once; the live events above keep it updated on later transitions.
  try {
    const s = await api.getServerStatus();
    if (s.status === "running") {
      serverStatus.set("running");
      if (s.addr) serverAddr.set(s.addr);
    } else if (s.status === "error") {
      serverStatus.set("error");
    }
  } catch {
    /* leave as "starting"; a later bus event may still correct it */
  }
}

/**
 * Selectable profile buckets derived from the synced chats. Named profiles are
 * grouped by display name so reinstall/test duplicates collapse into one chip.
 * The "Default" bucket collects legacy/pre-profile-sync chats with no profile id.
 */
function normalizeProfileName(name: string): string {
  return name.trim().replace(/\s+/g, " ").toLocaleLowerCase();
}

function profileBucketKey(profileId: string, profileName: string): string {
  const name = profileName.trim() || profileId.slice(0, 6);
  return `name:${normalizeProfileName(name)}`;
}

export const profileBuckets = derived(chats, ($chats) => {
  const named = new Map<string, { id: string; name: string }>();
  let hasDefault = false;
  for (const c of $chats) {
    if (c.profileId) {
      const name = c.profileName || c.profileId.slice(0, 6);
      const id = profileBucketKey(c.profileId, name);
      if (!named.has(id)) {
        named.set(id, { id, name });
      }
    } else {
      hasDefault = true;
    }
  }
  const list = Array.from(named.values()).sort((a, b) =>
    a.name.localeCompare(b.name)
  );
  if (hasDefault) list.push({ id: "", name: "Default" });
  return list;
});

export const visibleChats = derived(
  [chats, filterText, showArchived, activeTag, activeProfileId],
  ([$chats, $filter, $showArchived, $tag, $profile]) => {
    const q = $filter.trim().toLowerCase();
    return $chats.filter((c) => {
      if ($profile !== null) {
        if ($profile === "") {
          if (c.profileId) return false;
        } else if ($profile.startsWith("name:")) {
          if (!c.profileId || profileBucketKey(c.profileId, c.profileName) !== $profile) {
            return false;
          }
        } else if ((c.profileId || "") !== $profile) {
          return false;
        }
      }
      if (!$showArchived && c.isArchived) return false;
      if ($tag && !c.tags.includes($tag)) return false;
      if (q) {
        const inName = c.name.toLowerCase().includes(q);
        const inPreview = (c.lastMessagePreview ?? "").toLowerCase().includes(q);
        if (!inName && !inPreview) return false;
      }
      return true;
    });
  }
);

export const allTags = derived(chats, ($chats) => {
  const tagSet = new Set<string>();
  for (const c of $chats) for (const t of c.tags) tagSet.add(t);
  return Array.from(tagSet).sort();
});
