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

export const filterText = writable<string>("");
export const showArchived = writable<boolean>(false);
export const activeView = writable<"chats" | "search" | "starred" | "reminders" | "pairing" | "settings">("chats");
export const activeTag = writable<string | null>(null);
/** Profile sync: which phone profile the chat list is scoped to.
 *  `null` = all profiles, `""` = the legacy/pre-profile-sync "Default" bucket,
 *  any other value = that profile's id. */
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
async function ensureNotificationPermission(): Promise<boolean> {
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
 * Selectable profile buckets derived from the synced chats. Named profiles come
 * from the phone (profileId + display name); the "Default" bucket collects any
 * legacy/pre-profile-sync chats that synced with an empty profile id. The
 * switcher UI only renders this when there's more than one bucket.
 */
export const profileBuckets = derived(chats, ($chats) => {
  const named = new Map<string, string>();
  let hasDefault = false;
  for (const c of $chats) {
    if (c.profileId) {
      if (!named.has(c.profileId)) {
        named.set(c.profileId, c.profileName || c.profileId.slice(0, 6));
      }
    } else {
      hasDefault = true;
    }
  }
  const list = Array.from(named, ([id, name]) => ({ id, name })).sort((a, b) =>
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
      if ($profile !== null && (c.profileId || "") !== $profile) return false;
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
