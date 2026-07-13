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

export type FileAttachment = {
  file: string;
  name: string;
  mimeType: string | null;
  sizeBytes: number | null;
};

export type Message = {
  id: string;
  chatId: string;
  content: RichTextBlock[];
  voiceFile: string | null;
  voiceDurationMs: number | null;
  createdAt: number;
  editedAt: number | null;
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST" | "FILE";
  isStarred: boolean;
  imageFiles: string[];
  fileAttachments: FileAttachment[];
  tags: string[];
  reminderAt: number | null;
  checklistItems: ChecklistItem[];
  /** Emoji reactions attached to this message (ordered, no duplicates). */
  reactions: string[];
  /** Whisper transcript for voice messages (null until transcription runs). */
  transcript: string | null;
  /** Id of the message this one replies to, always within the same chat. */
  replyToId: string | null;
  /**
   * Preview of the replied-to message, frozen when the reply was authored. A
   * tombstone hard-removes its target from the materialized view, so a bare
   * replyToId would dangle; this snapshot keeps the quote renderable after the
   * original is deleted. Empty for types with no text — the UI supplies a label.
   */
  replyToPreview: string | null;
  replyToType: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST" | null;
  replyToCreatedAt: number | null;
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

/** An outbound peer this desktop syncs to (e.g. another desktop). */
export type PeerView = {
  id: string;
  label: string;
  authenticatedName: string | null;
  host: string;
  port: number;
  foregroundOnly: boolean;
  enabled: boolean;
  createdAt: number;
  lastSeenAt: number | null;
  lastDiscoveredAt: number | null;
  lastAuthenticatedAt: number | null;
  lastAuthenticatedHost: string | null;
  lastAuthenticatedPort: number | null;
  lastError: string | null;
  nextRetryAt: number | null;
};

export type AppSettings = {
  port: number;
  deviceName: string;
  theme: string;
  /** When false (default), the desktop does not advertise over mDNS. Pairing still
   *  works via QR / pasted JSON, but paired clients cannot recover automatically
   *  after this desktop's LAN address changes. */
  discoverable: boolean;
  /** One of "purple" / "blue" / "green" / "pink" / "orange" / "teal" — drives
   *  the `[data-accent='…']` CSS preset applied to `document.documentElement`. */
  accent: string;
  /** When true, voice messages are auto-transcribed via a local whisper.cpp
   *  install. Off by default (opt-in — needs the model + CLI installed first). */
  transcriptionEnabled: boolean;
};

export type DesktopPrefs = {
  startMinimized: boolean;
  /** Closing the window hides to the tray instead of quitting. */
  closeToTray: boolean;
  /** Offer "keep Ncy in the tray?" on close while `closeToTray` is off. */
  askCloseToTray: boolean;
  launchOnStartup: boolean;
  askBeforeDeleting: boolean;
  autoCheckUpdates: boolean;
  /** Run the LAN sync stack (WS listener, mDNS, outbound peers). Off by default
   *  for fresh installs — Ncy works standalone and sync is opt-in. */
  syncServerEnabled: boolean;
};

/** An image blob plus its sniffed format. See the `image_view` command. */
export type ImageView = {
  dataB64: string;
  /** Real MIME type of the bytes, e.g. "image/png" even for a `.jpg` reference. */
  mime: string;
  /** Real extension, used as the save dialog's default. */
  ext: string;
};

export type WhisperStatus = {
  cliPresent: boolean;
  modelPresent: boolean;
  modelSizeBytes: number | null;
  /** Absolute path to the folder where the CLI + model live. Surfaced so the
   *  Settings UI can print it verbatim and offer an "Open folder" button. */
  dir: string;
  /** Whether ffmpeg is available (bundled next to whisper-cli or on PATH).
   *  Needed to decode webm/opus voice recorded on the desktop; phone-recorded
   *  AAC/MP4 transcribes without it. */
  ffmpegPresent: boolean;
};

export type TranscriptionEventPayload =
  | { kind: "download-progress"; written: number; total: number }
  | { kind: "download-complete" }
  | { kind: "download-error"; message: string };

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
  | { kind: "peer-error"; peer: string; message: string }
  | { kind: "server-started"; addr: string }
  | { kind: "server-stopped" }
  | { kind: "server-error"; message: string }
  /** A phone discovered us over mDNS and is asking to pair. The UI shows a
   *  confirmation modal; the user's decision routes back through
   *  `pairAccept(sessionId, label)` or `pairReject(sessionId)`. Expires
   *  after ~60s on the server side. `safetyCode` is a 6-digit fingerprint of
   *  the ECDH handshake — the SAME code is shown on the phone's waiting
   *  screen, and the user should visually verify they match before
   *  confirming (defeats active MITM). */
  | {
      kind: "pair-requested";
      sessionId: string;
      peer: string;
      phoneId: string;
      phoneName: string;
      safetyCode: string;
    };

/** Row in the discovery request audit log (Settings → Device → Recent requests). */
export type PairAttempt = {
  timestampMs: number;
  sourceIp: string;
  phoneId: string | null;
  phoneName: string | null;
  /** `"accepted"`, `"rejected"`, `"timeout"`, `"cancelled"`, `"rate-limited"`,
   *  `"discovery-off"`, `"bad-pubkey"`, `"server-error"`. */
  outcome: string;
};

export type MediaType = "voice" | "image" | "file";

export const api = {
  listChats: () => invoke<ChatSummary[]>("list_chats"),
  getChat: (chatId: string) =>
    invoke<MaterializedChat | null>("get_chat", { chatId }),
  listPairings: () => invoke<PairingDescriptor[]>("list_pairings"),
  createPairing: (label: string) =>
    invoke<PairingDescriptor>("create_pairing", { label }),
  deletePairing: (id: string) => invoke<void>("delete_pairing", { id }),
  rotatePairing: (id: string) => invoke<PairingDescriptor>("rotate_pairing", { id }),
  addPeer: (payloadJson: string) =>
    invoke<void>("add_peer", { payloadJson }),
  listPeers: () => invoke<PeerView[]>("list_peers"),
  setPeerEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_peer_enabled", { id, enabled }),
  deletePeer: (id: string) => invoke<void>("delete_peer", { id }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) =>
    invoke<AppSettings>("update_settings", { settings }),
  getDesktopPrefs: () => invoke<DesktopPrefs>("get_desktop_prefs"),
  updateDesktopPrefs: (prefs: DesktopPrefs) =>
    invoke<DesktopPrefs>("update_desktop_prefs", { prefs }),
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
  authorSendText: (chatId: string, text: string, replyToId: string | null = null) =>
    invoke<void>("author_send_text", { chatId, text, replyToId }),
  authorToggleStar: (chatId: string, messageId: string) =>
    invoke<void>("author_toggle_star", { chatId, messageId }),
  authorToggleReaction: (chatId: string, messageId: string, emoji: string) =>
    invoke<void>("author_toggle_reaction", { chatId, messageId, emoji }),
  authorEditText: (chatId: string, messageId: string, text: string) =>
    invoke<void>("author_edit_text", { chatId, messageId, text }),
  authorDeleteMessage: (chatId: string, messageId: string) =>
    invoke<void>("author_delete_message", { chatId, messageId }),
  authorSendImage: (chatId: string, sourcePath: string, replyToId: string | null = null) =>
    invoke<void>("author_send_image", { chatId, sourcePath, replyToId }),
  authorSendFile: (
    chatId: string,
    sourcePath: string,
    displayName: string,
    replyToId: string | null = null,
  ) => invoke<void>("author_send_file", { chatId, sourcePath, displayName, replyToId }),
  authorSendVoice: (
    chatId: string,
    bytesB64: string,
    durationMs: number | null,
    replyToId: string | null = null,
  ) => invoke<string>("author_send_voice", { chatId, bytesB64, durationMs, replyToId }),
  authorTranscribeVoice: (chatId: string, messageId: string) =>
    invoke<void>("author_transcribe_voice", { chatId, messageId }),
  authorSendChecklist: (chatId: string, items: string[], replyToId: string | null = null) =>
    invoke<void>("author_send_checklist", { chatId, items, replyToId }),
  authorSetReminder: (chatId: string, messageId: string, reminderAt: number | null) =>
    invoke<void>("author_set_reminder", { chatId, messageId, reminderAt }),
  authorSetChatAvatar: (chatId: string, sourcePath: string) =>
    invoke<void>("author_set_chat_avatar", { chatId, sourcePath }),
  authorSetChatBackground: (chatId: string, sourcePath: string) =>
    invoke<void>("author_set_chat_background", { chatId, sourcePath }),
  authorClearChatAvatar: (chatId: string) =>
    invoke<void>("author_clear_chat_avatar", { chatId }),
  authorClearChatBackground: (chatId: string) =>
    invoke<void>("author_clear_chat_background", { chatId }),
  authorToggleChecklistItem: (chatId: string, messageId: string, itemId: string) =>
    invoke<void>("author_toggle_checklist_item", { chatId, messageId, itemId }),
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
  authorCreateChat: (name: string) =>
    invoke<string>("author_create_chat", { name }),
  authorDeleteChat: (chatId: string) =>
    invoke<void>("author_delete_chat", { chatId }),
  authorSendMessage: (
    chatId: string,
    content: RichTextBlock[],
    replyToId: string | null = null,
  ) => invoke<void>("author_send_message", { chatId, content, replyToId }),

  // Phase 2b — content-addressed media blobs.
  readBlob: (hash: string, mediaType: MediaType) =>
    invoke<string | null>("read_blob", { hash, mediaType }),
  blobExists: (hash: string, mediaType: MediaType) =>
    invoke<boolean>("blob_exists", { hash, mediaType }),
  saveBlob: (hash: string, mediaType: MediaType, dest: string) =>
    invoke<void>("save_blob", { hash, mediaType, dest }),

  // Image viewer. `imageView` sniffs the real format — nothing records it, and
  // every stored image is labelled `.jpg` whatever it actually is.
  imageView: (hash: string) => invoke<ImageView>("image_view", { hash }),
  saveImage: (hash: string, dest: string) =>
    invoke<void>("save_image", { hash, dest }),
  copyImage: (hash: string) => invoke<void>("copy_image", { hash }),

  // Standalone backup / restore (desktop's own data — events + blobs).
  exportBackup: (destPath: string) =>
    invoke<void>("export_backup", { destPath }),
  importBackup: (srcPath: string) =>
    invoke<{ chats: number; events: number; blobs: number }>("import_backup", {
      srcPath,
    }),

  // Phase 3 — protected-chat unlock. `unlockChat` returns false on a wrong PIN.
  unlockChat: (chatId: string, pin: string) =>
    invoke<boolean>("unlock_chat", { chatId, pin }),
  lockChat: (chatId: string) => invoke<void>("lock_chat", { chatId }),
  // #3 — protect a chat / change its PIN from the desktop.
  protectChat: (chatId: string, pin: string) =>
    invoke<void>("protect_chat", { chatId, pin }),
  changeChatPin: (chatId: string, oldPin: string, newPin: string) =>
    invoke<void>("change_chat_pin", { chatId, oldPin, newPin }),

  // Voice transcription (local whisper.cpp via subprocess).
  transcriptionStatus: () => invoke<WhisperStatus>("transcription_status"),
  transcriptionDownloadModel: () =>
    invoke<void>("transcription_download_model"),
  transcriptionDeleteModel: () =>
    invoke<void>("transcription_delete_model"),
  transcriptionOpenFolder: () =>
    invoke<void>("transcription_open_folder"),

  // Discovery-initiated pairing (2026-07-07): the phone opens a WebSocket,
  // sends `PairRequest`, and this desktop's UI decides whether to accept.
  pairAccept: (sessionId: string, label: string) =>
    invoke<void>("pair_accept", { sessionId, label }),
  pairReject: (sessionId: string) =>
    invoke<void>("pair_reject", { sessionId }),
  listPairAttempts: () => invoke<PairAttempt[]>("list_pair_attempts"),
  onTranscriptionEvent: (
    cb: (ev: TranscriptionEventPayload) => void,
  ): Promise<UnlistenFn> =>
    listen<TranscriptionEventPayload>("transcription-event", (e) =>
      cb(e.payload),
    ),
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
