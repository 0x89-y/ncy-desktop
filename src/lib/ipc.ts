import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ChatSummary = {
  id: string;
  name: string;
  avatarFile: string | null;
  createdAt: number;
  isPinned: boolean;
  isArchived: boolean;
  isProtected: boolean;
  tags: string[];
  lastMessageAt: number | null;
  lastMessagePreview: string | null;
  messageCount: number;
  bubbleColor: string | null;
  bubbleGradient: boolean;
  hideName: boolean;
  /** Profile sync: which phone profile this chat belongs to (+ display name).
   *  Empty string for pre-profile-sync chats (a single "default" bucket). */
  profileId: string;
  profileName: string;
};

export type RichTextSpan = {
  text: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikethrough?: boolean;
  color?: string | null;
  link?: string | null;
  inlineImage?: string | null;
};

export type RichTextBlock = {
  type:
    | "PARAGRAPH"
    | "HEADING1"
    | "HEADING2"
    | "HEADING3"
    | "BULLET_LIST"
    | "CODE_BLOCK";
  spans: RichTextSpan[];
};

export type ChecklistItem = {
  id: string;
  text: string;
  checked: boolean;
};

export type Message = {
  id: string;
  chatId: string;
  content: RichTextBlock[];
  voiceFile: string | null;
  voiceDurationMs: number | null;
  createdAt: number;
  editedAt: number | null;
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST";
  isStarred: boolean;
  imageFiles: string[];
  tags: string[];
  reminderAt: number | null;
  checklistItems: ChecklistItem[];
};

export type ChatMetadata = {
  id: string;
  name: string;
  avatarFile: string | null;
  backgroundFile: string | null;
  createdAt: number;
  isPinned: boolean;
  bubbleColor: string | null;
  bubbleGradient: boolean;
  isArchived: boolean;
  tags: string[];
  isProtected: boolean;
  hideName: boolean;
};

export type MaterializedChat = {
  metadata: ChatMetadata;
  messages: Message[];
  lastMessageAt: number | null;
  lastMessagePreview: string | null;
  /** Phase 3: false when the chat is protected and no key is cached on the desktop
   *  — message bodies are `🔒` placeholders and the UI should show the PIN prompt. */
  isUnlocked: boolean;
  /** Profile sync: which phone profile this chat belongs to (+ display name). */
  profileId: string;
  profileName: string;
};

export type Pairing = {
  id: string;
  label: string;
  secretB64: string;
  phoneId: string | null;
  phoneName: string | null;
  createdAt: number;
  lastSeenAt: number | null;
};

export type PairingDescriptor = {
  pairing: Pairing;
  payloadJson: string;
  qrSvg: string;
};

export type AppSettings = {
  port: number;
  deviceName: string;
  theme: string;
  /** When false (default), the desktop does not advertise over mDNS — it stays
   *  invisible to network scanners. Pairing still works via QR / pasted JSON. */
  discoverable: boolean;
};

export type DeviceIdentityView = {
  deviceId: string;
  name: string;
};

export type SyncEventPayload =
  | { kind: "peer-connected"; peer: string; pairingId: string | null; phoneId: string | null }
  | { kind: "peer-auth-failed"; peer: string }
  | { kind: "event-received"; chatId: string; eventId: string }
  | { kind: "event-authored"; chatId: string; eventId: string }
  | { kind: "blob-arrived"; hash: string }
  | { kind: "peer-disconnected"; peer: string }
  | { kind: "server-started"; addr: string }
  | { kind: "server-stopped" }
  | { kind: "server-error"; message: string };

export type MediaType = "voice" | "image";

export const api = {
  listChats: () => invoke<ChatSummary[]>("list_chats"),
  getChat: (chatId: string) =>
    invoke<MaterializedChat | null>("get_chat", { chatId }),
  listPairings: () => invoke<PairingDescriptor[]>("list_pairings"),
  createPairing: (label: string) =>
    invoke<PairingDescriptor>("create_pairing", { label }),
  deletePairing: (id: string) => invoke<void>("delete_pairing", { id }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) =>
    invoke<AppSettings>("update_settings", { settings }),
  listLanIps: () => invoke<string[]>("list_lan_ips"),
  setAdvertisedHost: (host: string) =>
    invoke<void>("set_advertised_host", { host }),
  getAdvertisedHost: () => invoke<string>("get_advertised_host"),
  getDeviceIdentity: () => invoke<DeviceIdentityView>("get_device_identity"),
  getServerStatus: () =>
    invoke<{ status: string; addr: string | null }>("get_server_status"),
  onSyncEvent: (cb: (ev: SyncEventPayload) => void): Promise<UnlistenFn> =>
    listen<SyncEventPayload>("sync-event", (e) => cb(e.payload)),

  // Phase 2a.2 authoring — desktop generates events that flow back to the phone.
  authorSendText: (chatId: string, text: string) =>
    invoke<void>("author_send_text", { chatId, text }),
  authorToggleStar: (chatId: string, messageId: string) =>
    invoke<void>("author_toggle_star", { chatId, messageId }),
  authorEditText: (chatId: string, messageId: string, text: string) =>
    invoke<void>("author_edit_text", { chatId, messageId, text }),
  authorDeleteMessage: (chatId: string, messageId: string) =>
    invoke<void>("author_delete_message", { chatId, messageId }),
  authorSendImage: (chatId: string, sourcePath: string) =>
    invoke<void>("author_send_image", { chatId, sourcePath }),
  authorSendChecklist: (chatId: string, items: string[]) =>
    invoke<void>("author_send_checklist", { chatId, items }),
  authorUpdateChatMeta: (
    chatId: string,
    patch: { name?: string; isPinned?: boolean; isArchived?: boolean; tags?: string[] },
  ) =>
    invoke<void>("author_update_chat_meta", {
      chatId,
      name: patch.name ?? null,
      isPinned: patch.isPinned ?? null,
      isArchived: patch.isArchived ?? null,
      tags: patch.tags ?? null,
    }),
  authorDeleteChat: (chatId: string) =>
    invoke<void>("author_delete_chat", { chatId }),
  authorSendMessage: (chatId: string, content: RichTextBlock[]) =>
    invoke<void>("author_send_message", { chatId, content }),

  // Phase 2b — content-addressed media blobs.
  readBlob: (hash: string, mediaType: MediaType) =>
    invoke<string | null>("read_blob", { hash, mediaType }),

  // Phase 3 — protected-chat unlock. `unlockChat` returns false on a wrong PIN.
  unlockChat: (chatId: string, pin: string) =>
    invoke<boolean>("unlock_chat", { chatId, pin }),
  lockChat: (chatId: string) => invoke<void>("lock_chat", { chatId }),
};

/**
 * Extract a content-addressed SHA-256 hash from a media reference produced by the
 * phone (e.g. "voice/<hash>.m4a" or "images/<hash>.jpg"). Returns null for legacy
 * random-named media that the phone authored before phase 2b.
 */
export function extractMediaHash(reference: string | null | undefined): string | null {
  if (!reference) return null;
  const lastSlash = reference.lastIndexOf("/");
  const name = lastSlash >= 0 ? reference.slice(lastSlash + 1) : reference;
  const dot = name.indexOf(".");
  const stem = dot >= 0 ? name.slice(0, dot) : name;
  return /^[0-9a-f]{64}$/.test(stem) ? stem : null;
}
