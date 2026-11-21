<script lang="ts">
  import { requestConfirmation } from "../stores/confirm";
  import {
    activeChat,
    arrivedBlobs,
    chats,
    upcomingReminders,
    refreshActiveChat,
    refreshChats,
    selectChat,
    settings,
    identity,
    markTranscribing,
    unmarkTranscribing,
    setTranscriptionError,
  } from "../stores/app";
  import { api, extractMediaHash, type Message } from "../ipc";
  import { formatTime, messagePreview, replyPreviewText } from "../format";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { writeText, readText } from "@tauri-apps/plugin-clipboard-manager";
  import { parseMarkdown, prefixSelectedLines, wrapSelection } from "../markdown";
  import {
    Bold, Code2, Heading1, Heading2, Heading3, Italic, List, ListChecks,
    Plus, Pin, Lock, Strikethrough, Unlock, X,
    ArrowDown, ArrowUp, Bell, Check, MessageSquare, Mic, Settings,
  } from "@lucide/svelte";
  import Avatar from "./Avatar.svelte";
  import ChatSettingsPanel from "./ChatSettingsPanel.svelte";
  import MessageBubble from "./MessageBubble.svelte";

  let scroller: HTMLDivElement | undefined = $state();
  let composeTextarea: HTMLTextAreaElement | undefined = $state();
  let lastCount = $state(0);
  let lastChatId = $state<string | null>(null);
  let atBottom = $state(true);
  let newMessageCount = $state(0);
  const drafts = new Map<string, string>();
  let composeText = $state("");
  let sending = $state(false);
  let sendError = $state<string | null>(null);
  let editRequestedId = $state<string | null>(null);

  let composeActionsOpen = $state(false);
  let formattingOpen = $state(false);

  function resizeComposer() {
    if (!composeTextarea) return;
    composeTextarea.style.height = "auto";
    const styles = getComputedStyle(composeTextarea);
    const borderHeight = parseFloat(styles.borderTopWidth) + parseFloat(styles.borderBottomWidth);
    const maxHeight = parseFloat(styles.maxHeight);
    const contentHeight = composeTextarea.scrollHeight + borderHeight;
    const targetHeight = Number.isFinite(maxHeight) ? Math.min(contentHeight, maxHeight) : contentHeight;
    composeTextarea.style.height = `${targetHeight}px`;
    composeTextarea.style.overflowY = Number.isFinite(maxHeight) && contentHeight > maxHeight ? "auto" : "hidden";
  }

  $effect(() => {
    composeText;
    queueMicrotask(resizeComposer);
  });

  // Phase 2b polish: pull the chat's wallpaper blob if backgroundFile is content-addressed.
  // Legacy `background.jpg` references (pre-content-addressing) extract to null and are
  // silently skipped — older builds rendered them locally; they can't sync.
  let backgroundDataUrl = $state<string | null>(null);
  const backgroundHash = $derived($activeChat ? extractMediaHash($activeChat.metadata.backgroundFile) : null);
  // Load again when a background set on another device lands after its chat.
  const backgroundArrived = $derived(backgroundHash ? $arrivedBlobs.has(backgroundHash) : false);
  $effect(() => {
    const hash = backgroundHash;
    void backgroundArrived;
    backgroundDataUrl = null;
    if (!hash) return;
    api
      .readBlob(hash, "image")
      .then((b64) => {
        if (
          !b64 ||
          // Active chat may have changed while the request was in flight.
          extractMediaHash($activeChat?.metadata.backgroundFile) !== hash
        ) {
          return;
        }
        backgroundDataUrl = `data:image/jpeg;base64,${b64}`;
      })
      .catch(() => {});
  });

  // The message the next send will quote. Cleared as soon as a send starts, so a
  // reply attaches to exactly one outgoing message.
  let replyTarget = $state<Message | null>(null);
  let highlightedId = $state<string | null>(null);
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;

  const messageIds = $derived(new Set(($activeChat?.messages ?? []).map((m) => m.id)));
  const recentChats = $derived(
    [...$chats].filter((c) => !c.isArchived && c.lastMessageAt).sort((a, b) => (b.lastMessageAt ?? 0) - (a.lastMessageAt ?? 0)).slice(0, 4),
  );
  function reminderWhen(ms: number): string {
    const d = new Date(ms), now = new Date();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === now.toDateString()) return time;
    const tomorrow = new Date(now); tomorrow.setDate(now.getDate() + 1);
    if (d.toDateString() === tomorrow.toDateString()) return `Tomorrow ${time}`;
    return d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  }
  const starredCount = $derived(($activeChat?.messages ?? []).filter((m) => m.isStarred).length);

  function startReply(m: Message) {
    replyTarget = m;
    composeTextarea?.focus();
  }
  function cancelReply() {
    replyTarget = null;
  }
  function consumeReplyId(): string | null {
    const id = replyTarget?.id ?? null;
    replyTarget = null;
    return id;
  }

  /** Scroll a quoted original into view and flash it. No-op if it was deleted. */
  function jumpToMessage(id: string) {
    const el = scroller?.querySelector(`[data-mid="${CSS.escape(id)}"]`);
    if (!el) return;
    el.scrollIntoView({ behavior: "smooth", block: "center" });
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightedId = id;
    highlightTimer = setTimeout(() => {
      highlightedId = null;
      highlightTimer = null;
    }, 1400);
  }

  let scrollFrame: number | null = null;
  function updateScrollState() {
    if (scrollFrame !== null) return;
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null;
      if (!scroller) return;
      atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 36;
      if (atBottom) newMessageCount = 0;
    });
  }

  function jumpToLatest() {
    if (!scroller) return;
    scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
    atBottom = true;
    newMessageCount = 0;
  }

  $effect(() => {
    const c = $activeChat;
    if (!c || !scroller) return;
    const chatId = c.metadata.id;
    const count = c.messages.length;
    const changedChat = chatId !== lastChatId;
    const added = changedChat ? 0 : Math.max(0, count - lastCount);
    if (changedChat) {
      if (lastChatId) drafts.set(lastChatId, composeText);
      composeText = drafts.get(chatId) ?? "";
      lastChatId = chatId;
      newMessageCount = 0;
      atBottom = true;
      queueMicrotask(() => {
        if (scroller) scroller.scrollTop = scroller.scrollHeight;
      });
    } else if (added > 0) {
      if (atBottom) {
        queueMicrotask(() => {
          if (scroller) scroller.scrollTop = scroller.scrollHeight;
        });
      } else {
        newMessageCount += added;
      }
    }
    lastCount = count;
  });

  $effect(() => {
    const id = $activeChat?.metadata.id;
    const draft = composeText;
    if (id) drafts.set(id, draft);
  });

  async function send() {
    const c = $activeChat;
    if (!c) return;
    if (!composeText.trim() || sending) return;
    const blocks = parseMarkdown(composeText);
    if (blocks.length === 0) return;
    sending = true;
    sendError = null;
    let sent = false;
    try {
      await api.authorSendMessage(c.metadata.id, blocks, consumeReplyId());
      composeText = "";
      sent = true;
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "send failed");
    } finally {
      sending = false;
    }
    if (sent) queueMicrotask(() => composeTextarea?.focus());
  }

  function applyWrap(prefix: string, suffix: string = prefix) {
    if (!composeTextarea) return;
    const next = wrapSelection(composeTextarea, prefix, suffix);
    composeText = next.value;
    // Restore selection after Svelte re-renders the textarea.
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  function applyLinePrefix(prefix: string) {
    if (!composeTextarea) return;
    const next = prefixSelectedLines(composeTextarea, prefix);
    composeText = next.value;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  function insertCodeBlock() {
    if (!composeTextarea) return;
    // Multi-line wrap: inject ``` on its own lines around the selection.
    const next = wrapSelection(composeTextarea, "```\n", "\n```");
    composeText = next.value;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  // ----- Compose right-click menu: formatting + clipboard -----
  // Replaces the native menu in the compose box. Selection is snapshotted on
  // open (it survives the right-click but the menu would steal focus).
  let composeMenu = $state<{ x: number; y: number; selStart: number; selEnd: number } | null>(null);
  let composeMenuEl: HTMLDivElement | undefined = $state();

  function openComposeMenu(e: MouseEvent) {
    if (sending || !composeTextarea) return; // disabled → fall back to native
    e.preventDefault();
    const ta = composeTextarea;
    const selStart = ta.selectionStart ?? 0;
    const selEnd = ta.selectionEnd ?? selStart;
    // Raw click point; the $effect below measures the rendered menu and clamps
    // it fully on-screen (the compose box sits at the bottom, so it usually
    // needs to open upward).
    composeMenu = { x: e.clientX, y: e.clientY, selStart, selEnd };
  }

  // Keep the menu inside the viewport once it has a real size.
  $effect(() => {
    const m = composeMenu;
    const el = composeMenuEl;
    if (!m || !el) return;
    const pad = 8;
    const r = el.getBoundingClientRect();
    const left = Math.max(pad, Math.min(m.x, window.innerWidth - r.width - pad));
    const top = Math.max(pad, Math.min(m.y, window.innerHeight - r.height - pad));
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  });
  function closeComposeMenu() {
    composeMenu = null;
  }

  function restoreSelection() {
    const m = composeMenu;
    if (!m || !composeTextarea) return;
    composeTextarea.focus();
    composeTextarea.setSelectionRange(m.selStart, m.selEnd);
  }

  async function menuCopy() {
    const m = composeMenu;
    if (!m) return;
    const text = composeText.slice(m.selStart, m.selEnd);
    closeComposeMenu();
    if (!text) return;
    try {
      await writeText(text);
    } catch (e) {
      console.error("copy failed", e);
    }
  }

  async function menuCut() {
    const m = composeMenu;
    if (!m) return;
    const { selStart, selEnd } = m;
    const text = composeText.slice(selStart, selEnd);
    closeComposeMenu();
    if (!text) return;
    try {
      await writeText(text);
    } catch (e) {
      console.error("cut failed", e);
      return;
    }
    composeText = composeText.slice(0, selStart) + composeText.slice(selEnd);
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(selStart, selStart);
    });
  }

  async function menuPaste() {
    const m = composeMenu;
    if (!m) return;
    const { selStart, selEnd } = m;
    closeComposeMenu();
    let clip = "";
    try {
      clip = (await readText()) ?? "";
    } catch (e) {
      console.error("paste failed", e);
      return;
    }
    if (!clip) return;
    composeText = composeText.slice(0, selStart) + clip + composeText.slice(selEnd);
    const caret = selStart + clip.length;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(caret, caret);
    });
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape" && composeActionsOpen) {
      e.preventDefault();
      composeActionsOpen = false;
      return;
    }
    if (e.key === "Escape" && composeMenu) {
      e.preventDefault();
      closeComposeMenu();
      return;
    }
    if (e.key === "Escape" && chatSettingsOpen) {
      e.preventDefault();
      chatSettingsOpen = false;
      return;
    }
    // Ctrl/Cmd+Shift+R — reply to the newest message in the open chat.
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "r" || e.key === "R")) {
      const msgs = $activeChat?.messages ?? [];
      const last = msgs[msgs.length - 1];
      if (last) {
        e.preventDefault();
        startReply(last);
      }
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    // Escape backs out of a pending reply before anything else claims the key.
    if (e.key === "Escape" && replyTarget) {
      e.preventDefault();
      cancelReply();
      return;
    }
    // Ctrl/Cmd-B / -I — standard formatting shortcuts.
    const mod = e.ctrlKey || e.metaKey;
    if (mod && (e.key === "b" || e.key === "B")) {
      e.preventDefault();
      applyWrap("**");
      return;
    }
    if (mod && (e.key === "i" || e.key === "I")) {
      e.preventDefault();
      applyWrap("*");
      return;
    }
    if (e.key === "ArrowUp" && !composeText && !composeActionsOpen && !composeMenu) {
      const ownText = [...($activeChat?.messages ?? [])]
        .reverse()
        .find((message) =>
          message.type === "TEXT" &&
          (!message.authorDeviceId || message.authorDeviceId === $identity?.deviceId)
        );
      if (ownText) {
        e.preventDefault();
        editRequestedId = ownText.id;
        composeTextarea?.blur();
      }
      return;
    }
    // Enter sends; Shift+Enter inserts a newline.
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  }

  // ----- Phase 3: protected-chat unlock -----
  let pinInput = $state("");
  let unlocking = $state(false);
  let unlockError = $state<string | null>(null);

  async function unlock() {
    const c = $activeChat;
    if (!c || unlocking || !pinInput.trim()) return;
    unlocking = true;
    unlockError = null;
    try {
      const ok = await api.unlockChat(c.metadata.id, pinInput);
      if (ok) {
        pinInput = "";
        await refreshActiveChat();
        await refreshChats();
      } else {
        unlockError = "Wrong PIN";
        pinInput = "";
      }
    } catch (e: any) {
      unlockError = typeof e === "string" ? e : (e?.message ?? "unlock failed");
    } finally {
      unlocking = false;
    }
  }

  async function lock() {
    const c = $activeChat;
    if (!c) return;
    await api.lockChat(c.metadata.id);
    await refreshActiveChat();
    await refreshChats();
  }

  // ----- Chat management (rename / pin / archive / tags / delete) -----
  let chatSettingsOpen = $state(false);
  let metaBusy = $state(false);

  /** Retention windows offered in the chat menu. 0 is the "keep everything" case. */
  const RETENTION_CHOICES = [
    { days: 0, label: "Keep forever" },
    { days: 7, label: "7 days" },
    { days: 14, label: "14 days" },
    { days: 30, label: "30 days" },
    { days: 90, label: "90 days" },
    { days: 365, label: "1 year" },
  ];

  /**
   * Turning this on deletes everywhere, immediately, for anything already past the
   * window — so say how many that is before doing it rather than after.
   */
  async function setRetention(days: number) {
    const c = $activeChat;
    if (!c || metaBusy) return;
    if ((c.metadata.retentionDays ?? 0) === days) return;
    if (days > 0) {
      const cutoff = Date.now() - days * 24 * 60 * 60 * 1000;
      const doomed = c.messages.filter(
        (m) => m.createdAt < cutoff && !m.isStarred && !(m.reminderAt && m.reminderAt > Date.now()),
      ).length;
      const detail =
        doomed > 0
          ? `${doomed} existing ${doomed === 1 ? "message is" : "messages are"} already older than that and will be deleted on every device.`
          : "Nothing in this chat is older than that yet.";
      if (
        !(await requestConfirmation(
          `Delete messages in this chat older than ${days} days? ${detail} Starred messages and ones with an upcoming reminder are kept.`,
          { title: "Auto-delete old messages", confirmLabel: "Turn on", dangerous: doomed > 0 },
        ))
      ) {
        return;
      }
    }
    await runMeta({ retentionDays: days });
    try {
      await api.applyChatRetention();
    } catch (e) {
      console.error("apply retention failed", e);
    }
  }

  async function runMeta(patch: {
    name?: string;
    isPinned?: boolean;
    isArchived?: boolean;
    tags?: string[];
    remoteMessagesOnLeft?: boolean;
    retentionDays?: number;
    bubbleColor?: string;
  }) {
    const c = $activeChat;
    if (!c || metaBusy) return;
    metaBusy = true;
    try {
      await api.authorUpdateChatMeta(c.metadata.id, patch);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("update chat meta failed", e);
    } finally {
      metaBusy = false;
    }
  }

  // ----- Protect chat / change PIN (#3) -----
  let pinMode = $state<null | "protect" | "change">(null);
  let pinOld = $state("");
  let pin1 = $state("");
  let pin2 = $state("");
  let pinBusy = $state(false);
  let pinError = $state<string | null>(null);

  function openPin(mode: "protect" | "change") {
    pinMode = mode;
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
    chatSettingsOpen = false;
  }
  function cancelPin() {
    pinMode = null;
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
  }
  async function savePin() {
    const c = $activeChat;
    if (!c || pinBusy) return;
    if (!pin1) { pinError = "PIN cannot be empty"; return; }
    if (pin1 !== pin2) { pinError = "PINs don't match"; return; }
    if (pinMode === "change" && !pinOld) { pinError = "Enter the current PIN"; return; }
    pinBusy = true;
    pinError = null;
    try {
      if (pinMode === "protect") {
        await api.protectChat(c.metadata.id, pin1);
      } else {
        await api.changeChatPin(c.metadata.id, pinOld, pin1);
      }
      await refreshActiveChat();
      await refreshChats();
      cancelPin();
    } catch (e: any) {
      pinError = typeof e === "string" ? e : (e?.message ?? "failed");
    } finally {
      pinBusy = false;
    }
  }

  async function deleteChat() {
    const c = $activeChat;
    if (!c) return;
    chatSettingsOpen = false;
    const prefs = await api.getDesktopPrefs().catch(() => ({ askBeforeDeleting: true }));
    if (
      prefs.askBeforeDeleting &&
      !await requestConfirmation(
        `Delete "${c.metadata.name}"? This removes it on every paired device and can't be undone.`,
      )
    )
      return;
    try {
      await api.authorDeleteChat(c.metadata.id);
      await selectChat(null);
      await refreshChats();
    } catch (e) {
      console.error("delete chat failed", e);
    }
  }

  // ----- Chat avatar + background (set / clear) -----
  async function pickImageFile(): Promise<string | null> {
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          { name: "Images", extensions: ["jpg", "jpeg", "png", "gif", "webp", "bmp"] },
        ],
      });
      return Array.isArray(picked) ? picked[0] ?? null : picked;
    } catch {
      return null;
    }
  }
  async function setAvatar() {
    const c = $activeChat;
    if (!c) return;
    const path = await pickImageFile();
    if (!path) return;
    try {
      await api.authorSetChatAvatar(c.metadata.id, path);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("set avatar failed", e);
    }
  }
  async function clearAvatar() {
    const c = $activeChat;
    if (!c) return;
    try {
      await api.authorClearChatAvatar(c.metadata.id);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("clear avatar failed", e);
    }
  }
  async function setBackground() {
    const c = $activeChat;
    if (!c) return;
    const path = await pickImageFile();
    if (!path) return;
    try {
      await api.authorSetChatBackground(c.metadata.id, path);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("set background failed", e);
    }
  }
  async function clearBackground() {
    const c = $activeChat;
    if (!c) return;
    try {
      await api.authorClearChatBackground(c.metadata.id);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("clear background failed", e);
    }
  }

  // ----- Voice recording (mic → MediaRecorder → blob) -----
  let recording = $state(false);
  let recordStart = $state(0);
  let recordElapsed = $state(0);
  let mediaRecorder: MediaRecorder | null = null;
  let recordedChunks: Blob[] = [];
  let recordStream: MediaStream | null = null;
  let recordTimer: ReturnType<typeof setInterval> | null = null;
  /** Live input level while recording: one bar per tick, newest on the right. */
  const REC_BARS = 32;
  let recordLevels = $state<number[]>([]);
  let levelContext: AudioContext | null = null;
  let levelAnalyser: AnalyserNode | null = null;
  function sampleLevel() {
    if (!levelAnalyser) return;
    const data = new Uint8Array(levelAnalyser.fftSize);
    levelAnalyser.getByteTimeDomainData(data);
    let peak = 0;
    for (const v of data) peak = Math.max(peak, Math.abs(v - 128) / 128);
    recordLevels = [...recordLevels, Math.min(1, peak * 1.6)].slice(-REC_BARS);
  }
  function stopLevelMeter() {
    levelAnalyser = null;
    void levelContext?.close();
    levelContext = null;
  }

  async function startRecording() {
    const c = $activeChat;
    if (!c || sending || recording) return;
    sendError = null;
    try {
      recordStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch (e: any) {
      sendError = "microphone permission denied";
      return;
    }
    recordedChunks = [];
    recordLevels = [];
    try {
      levelContext = new AudioContext();
      levelAnalyser = levelContext.createAnalyser();
      levelAnalyser.fftSize = 256;
      levelContext.createMediaStreamSource(recordStream).connect(levelAnalyser);
    } catch {
      stopLevelMeter(); // the meter is decoration; recording works without it
    }
    // Prefer AAC-in-MP4 so the blob is a real .m4a that any phone (and
    // symphonia, in the transcription pipeline) can decode without ffmpeg.
    // Recent Edge / WebView2 on Windows supports this via Media Foundation.
    // Falls back to webm/opus on older browsers — those blobs still play back
    // via the browser's lenient content sniffing but need ffmpeg to transcribe.
    const candidates = [
      "audio/mp4;codecs=mp4a.40.2",
      "audio/mp4",
      "audio/webm;codecs=opus",
      "audio/webm",
    ];
    const mimeType =
      candidates.find((m) => MediaRecorder.isTypeSupported(m)) ?? "";
    console.log("voice recording mimeType:", mimeType || "(browser default)");
    mediaRecorder = new MediaRecorder(recordStream, mimeType ? { mimeType } : undefined);
    mediaRecorder.ondataavailable = (ev) => {
      if (ev.data && ev.data.size > 0) recordedChunks.push(ev.data);
    };
    mediaRecorder.onstop = async () => {
      // Tear down the mic stream regardless of what happens next.
      recordStream?.getTracks().forEach((t) => t.stop());
      recordStream = null;
      stopLevelMeter();
      const duration = Date.now() - recordStart;
      const blob = new Blob(recordedChunks, {
        type: mediaRecorder?.mimeType ?? mimeType ?? "audio/mp4",
      });
      recordedChunks = [];
      mediaRecorder = null;
      if (recordTimer) { clearInterval(recordTimer); recordTimer = null; }
      recording = false;
      if (blob.size === 0) return;
      sending = true;
      try {
        const bytes = new Uint8Array(await blob.arrayBuffer());
        // btoa can't take a Uint8Array; assemble a binary string and encode.
        let bin = "";
        for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
        const b64 = btoa(bin);
        const messageId = await api.authorSendVoice(c.metadata.id, b64, duration, consumeReplyId());
        // Auto-transcribe: fire-and-forget so the send returns quickly while
        // whisper grinds in the background. MessageBubble reads the
        // `transcribing` store to show the spinner.
        if ($settings?.transcriptionEnabled) {
          console.log("auto-transcribe: starting for", messageId);
          markTranscribing(messageId);
          api
            .authorTranscribeVoice(c.metadata.id, messageId)
            .then(() => console.log("auto-transcribe: done for", messageId))
            .catch((e: any) => {
              const msg = typeof e === "string" ? e : (e?.message ?? String(e));
              console.warn("auto-transcribe failed:", msg, e);
              setTranscriptionError(messageId, msg);
            })
            .finally(() => unmarkTranscribing(messageId));
        } else {
          console.log("auto-transcribe: disabled in settings, skipping");
        }
      } catch (e: any) {
        sendError = typeof e === "string" ? e : (e?.message ?? "voice send failed");
      } finally {
        sending = false;
      }
    };
    recordStart = Date.now();
    recordElapsed = 0;
    recordTimer = setInterval(() => {
      recordElapsed = Date.now() - recordStart;
      sampleLevel();
    }, 120);
    mediaRecorder.start();
    recording = true;
  }
  function stopRecording() {
    if (!recording || !mediaRecorder) return;
    mediaRecorder.stop(); // triggers onstop above
  }
  function cancelRecording() {
    if (!recording || !mediaRecorder) return;
    // Discard by clearing chunks before stopping.
    recordedChunks = [];
    mediaRecorder.stop();
  }
  function fmtElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  }

  async function attach() {
    const c = $activeChat;
    if (!c || sending) return;
    composeActionsOpen = false;
    let picked: string | string[] | null;
    try {
      picked = await openDialog({
        multiple: false,
        directory: false,
      });
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "could not open picker");
      return;
    }
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const displayName = path.split(/[\\/]/).pop() || "File";
    const extension = displayName.includes(".") ? displayName.split(".").pop()!.toLowerCase() : "";
    const isImage = ["jpg", "jpeg", "png", "gif", "webp", "bmp"].includes(extension);
    sending = true;
    sendError = null;
    try {
      if (isImage) {
        await api.authorSendImage(c.metadata.id, path, consumeReplyId());
      } else {
        await api.authorSendFile(c.metadata.id, path, displayName, consumeReplyId());
      }
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "attachment send failed");
    } finally {
      sending = false;
    }
  }
</script>

{#if $activeChat}
  {@const chat = $activeChat}
  <div class="wrap col">
    <header class="row header">
      <Avatar
        name={chat.metadata.name}
        size={34}
        avatarFile={chat.metadata.avatarFile}
        hue={chat.metadata.bubbleColor}
      />
      <div class="col title-block">
        <div class="title">
          {#if chat.metadata.isPinned}<span class="title-ico"><Pin size={13} strokeWidth={2.25} /></span>{/if}
          {#if chat.metadata.isProtected}<span class="title-ico"><Lock size={13} strokeWidth={2.25} /></span>{/if}
          {chat.metadata.name}
        </div>
        <div class="subtitle">
          {`${chat.messages.length} ${chat.messages.length === 1 ? "note" : "notes"}${starredCount ? ` · ${starredCount} starred` : ""}`}
        </div>
      </div>
      {#if chat.metadata.isProtected && chat.isUnlocked}
        <button type="button" class="lock-btn" title="Lock this chat" aria-label="Lock this chat" onclick={lock}>
          <Unlock size={14} strokeWidth={2.25} />
          <span>Lock</span>
        </button>
      {/if}
      <div class="menu-wrap">
        <button
          type="button"
          class="menu-btn"
          title="Chat settings"
          aria-label="Chat settings"
          onclick={() => { chatSettingsOpen = true; }}
        ><Settings size={17} strokeWidth={2.25} /></button>
      </div>
    </header>

    {#if chatSettingsOpen}
      <ChatSettingsPanel
        {chat}
        busy={metaBusy}
        retentionChoices={RETENTION_CHOICES}
        onClose={() => (chatSettingsOpen = false)}
        onMeta={runMeta}
        onRetention={setRetention}
        onSetAvatar={setAvatar}
        onClearAvatar={clearAvatar}
        onSetBackground={setBackground}
        onClearBackground={clearBackground}
        onPin={openPin}
        onDelete={deleteChat}
      />
    {/if}


    {#if pinMode}
      <div class="meta-editor pin-editor">
        {#if pinMode === "change"}
          <input
            class="meta-input"
            type="password"
            placeholder="Current PIN"
            bind:value={pinOld}
            disabled={pinBusy}
            onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
          />
        {/if}
        <input
          class="meta-input"
          type="password"
          placeholder="New PIN"
          bind:value={pin1}
          disabled={pinBusy}
          onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
        />
        <input
          class="meta-input"
          type="password"
          placeholder="Confirm PIN"
          bind:value={pin2}
          disabled={pinBusy}
          onkeydown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); void savePin(); }
            if (e.key === "Escape") { e.preventDefault(); cancelPin(); }
          }}
        />
        <button type="button" class="send" onclick={savePin} disabled={pinBusy}>
          {pinBusy ? "…" : (pinMode === "protect" ? "Protect" : "Change")}
        </button>
        <button type="button" class="text-btn" onclick={cancelPin}>Cancel</button>
      </div>
      {#if pinError}
        <div class="meta-editor"><span class="compose-error tiny">{pinError}</span></div>
      {/if}
    {/if}

    {#if chat.metadata.isProtected && !chat.isUnlocked}
      <div class="lock-screen">
        <div class="lock-card">
          <div class="lock-icon"><Lock size={40} strokeWidth={2} /></div>
          <h2>{chat.metadata.hideName ? "Protected chat" : chat.metadata.name}</h2>
          <p class="muted">
            Enter the chat PIN to decrypt and view its messages on this device.
          </p>
          <input
            type="password"
            class="pin-input"
            placeholder="PIN"
            bind:value={pinInput}
            onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); void unlock(); } }}
            disabled={unlocking}
          />
          {#if unlockError}
            <div class="compose-error tiny">{unlockError}</div>
          {/if}
          <button
            type="button"
            class="send"
            onclick={unlock}
            disabled={unlocking || !pinInput.trim()}
          >
            {unlocking ? "…" : "Unlock"}
          </button>
        </div>
      </div>
    {:else}
    <div
      class="messages scroll"
      class:has-background={!!backgroundDataUrl}
      style={backgroundDataUrl ? `background-image:url(${backgroundDataUrl});` : ""}
      bind:this={scroller}
      onscroll={updateScrollState}
    >
      {#each chat.messages as m (m.id)}
        <MessageBubble
          message={m}
          bubbleColor={chat.metadata.bubbleColor}
          bubbleGradient={chat.metadata.bubbleGradient}
          isRemote={chat.metadata.remoteMessagesOnLeft && !!m.authorDeviceId && m.authorDeviceId !== $identity?.deviceId}
          onReply={startReply}
          onJumpToReplyTarget={jumpToMessage}
          replyTargetExists={!!m.replyToId && messageIds.has(m.replyToId)}
          highlighted={m.id === highlightedId}
          requestEdit={m.id === editRequestedId}
          onEditStarted={() => editRequestedId = null}
        />
      {:else}
        <div class="empty muted">No messages yet.</div>
      {/each}
    </div>
    {#if !atBottom}
      <button class="jump-latest" type="button" onclick={jumpToLatest} aria-label="Jump to latest messages">
        <ArrowDown size={16} />
        {#if newMessageCount > 0}<span>{newMessageCount} new</span>{/if}
      </button>
    {/if}

    <footer class="compose">
      {#if replyTarget}
        <!-- Reads the live target, not a snapshot: the snapshot is frozen only when
             the reply is actually sent, so edits made while this bar is open show. -->
        <div class="reply-bar">
          <div class="reply-bar-body">
            <span class="reply-bar-label tiny">Replying to</span>
            <span class="reply-bar-text tiny">
              {replyPreviewText(messagePreview(replyTarget), replyTarget.type)}
            </span>
          </div>
          <button type="button" class="text-btn" onclick={cancelReply} aria-label="Cancel reply">
            Cancel
          </button>
        </div>
      {/if}
      {#if sendError}
        <div class="compose-error tiny">{sendError}</div>
      {/if}
      {#if formattingOpen}
        <div class="format-row">
          <button type="button" class="format-close" title="Close formatting" aria-label="Close formatting" onclick={() => formattingOpen = false}><X size={15} /></button>
          <div class="format-toolbar" aria-label="Message formatting">
            <button type="button" title="Bold" aria-label="Bold" onclick={() => applyWrap("**")}><Bold size={16} /></button>
            <button type="button" title="Italic" aria-label="Italic" onclick={() => applyWrap("*")}><Italic size={16} /></button>
            <button type="button" title="Strikethrough" aria-label="Strikethrough" onclick={() => applyWrap("~~")}><Strikethrough size={16} /></button>
            <span class="format-separator"></span>
            <button type="button" title="Heading 1" aria-label="Heading 1" onclick={() => applyLinePrefix("# ")}><Heading1 size={17} /></button>
            <button type="button" title="Heading 2" aria-label="Heading 2" onclick={() => applyLinePrefix("## ")}><Heading2 size={17} /></button>
            <button type="button" title="Heading 3" aria-label="Heading 3" onclick={() => applyLinePrefix("### ")}><Heading3 size={17} /></button>
            <span class="format-separator"></span>
            <button type="button" title="Bullet list" aria-label="Bullet list" onclick={() => applyLinePrefix("- ")}><List size={17} /></button>
            <button type="button" title="Checklist" aria-label="Checklist" onclick={() => applyLinePrefix("- [ ] ")}><ListChecks size={17} /></button>
            <button type="button" title="Code block" aria-label="Code block" onclick={insertCodeBlock}><Code2 size={17} /></button>
          </div>
        </div>
      {/if}
      <div class="compose-row">
        {#if recording}
          <!-- Recording takes over the row: Discard, the time and level, then Save. -->
          <button type="button" class="attach-solo" title="Discard recording" aria-label="Discard recording" onclick={cancelRecording} disabled={sending}>
            <X size={18} strokeWidth={2.25} />
          </button>
          <div class="rec-bar" role="status" aria-label={`Recording, ${fmtElapsed(recordElapsed)}`}>
            <span class="rec-dot"></span>
            <span class="mono tiny">{fmtElapsed(recordElapsed)}</span>
            <span class="rec-level" aria-hidden="true">
              {#each Array(REC_BARS) as _, i}
                {@const level = recordLevels[i - (REC_BARS - recordLevels.length)]}
                <i class:off={level === undefined} style={`height:${Math.max(3, Math.round((level ?? 0) * 22))}px`}></i>
              {/each}
            </span>
          </div>
          <button type="button" class="send save" title="Save voice note" aria-label="Save voice note" onclick={stopRecording} disabled={sending}>
            <Check size={18} strokeWidth={2.5} />
          </button>
        {:else}
          <div class="attach-wrap">
            <button
              type="button"
              class="attach-solo"
              title="Attach a file, image or checklist"
              aria-label="Attach a file, image or checklist"
              aria-expanded={composeActionsOpen}
              onclick={() => { composeActionsOpen = !composeActionsOpen; }}
              disabled={sending}
            >
              <Plus size={18} strokeWidth={2.25} />
            </button>
            {#if composeActionsOpen}
              <button
                type="button"
                class="actions-backdrop"
                aria-label="Close add menu"
                onclick={() => { composeActionsOpen = false; }}
              ></button>
              <div class="actions-menu" role="menu">
                <button type="button" role="menuitem" onclick={attach}>Attach file or image…</button>
                <button type="button" role="menuitem" onclick={() => {
                  composeActionsOpen = false;
                  applyLinePrefix("- [ ] ");
                }}>Checklist</button>
              </div>
            {/if}
          </div>
          <div class="compose-field">
            <textarea
              class="compose-input"
              placeholder="Write a note…"
              bind:this={composeTextarea}
              bind:value={composeText}
              oninput={resizeComposer}
              onkeydown={onKeyDown}
              oncontextmenu={openComposeMenu}
              disabled={sending}
              rows="1"
            ></textarea>
            <button
              type="button"
              class="aa-toggle"
              class:on={formattingOpen}
              title={formattingOpen ? "Hide formatting" : "Formatting"}
              aria-label="Formatting"
              aria-pressed={formattingOpen}
              onclick={() => { formattingOpen = !formattingOpen; }}
            >Aa</button>
          </div>
          {#if composeText.trim()}
            <button type="button" class="send" title="Send (Enter)" aria-label="Send" onclick={send} disabled={sending}>
              {#if sending}…{:else}<ArrowUp size={18} strokeWidth={2.5} />{/if}
            </button>
          {:else}
            <!-- An empty composer offers a voice note where Send will be, as on the phone. -->
            <button type="button" class="send" title="Record a voice note" aria-label="Record a voice note" onclick={() => void startRecording()} disabled={sending}>
              <Mic size={18} strokeWidth={2.25} />
            </button>
          {/if}
        {/if}
      </div>
    </footer>
    {/if}
  </div>
{:else}
  <!-- No chat open: the next reminders and recent chats, one click away. -->
  <div class="placeholder">
    <div class="home-card">
      {#if $upcomingReminders.length}
        <div class="home-label">Up next</div>
        {#each $upcomingReminders.slice(0, 3) as r (r.chatId + r.at)}
          <button type="button" class="home-row" onclick={() => selectChat(r.chatId)}>
            <Bell size={16} strokeWidth={2.25} />
            <span class="home-main"><span class="home-text">{r.text}</span><span class="home-sub">{r.chatName}</span></span>
            <span class="home-time">{reminderWhen(r.at)}</span>
          </button>
        {/each}
      {/if}
      {#if recentChats.length}
        <div class="home-label">Recent</div>
        {#each recentChats as c (c.id)}
          <button type="button" class="home-row" onclick={() => selectChat(c.id)}>
            <Avatar name={c.name} size={28} avatarFile={c.avatarFile} hue={c.bubbleColor} />
            <span class="home-main"><span class="home-text">{c.name}</span></span>
            <span class="home-time">{formatTime(c.lastMessageAt)}</span>
          </button>
        {/each}
      {:else}
        <div class="muted home-empty"><MessageSquare size={40} strokeWidth={1.5} /><p>No chats yet.</p></div>
      {/if}
      <div class="home-keys">Ctrl+N new chat · Ctrl+K search</div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={onWindowKey} />

{#if composeMenu}
  {@const hasSel = composeMenu.selStart !== composeMenu.selEnd}
  <button
    type="button"
    class="ctx-backdrop"
    aria-label="Close menu"
    onclick={closeComposeMenu}
    oncontextmenu={(e) => { e.preventDefault(); closeComposeMenu(); }}
  ></button>
  <div
    class="ctx-menu"
    role="menu"
    bind:this={composeMenuEl}
    style={`left:${composeMenu.x}px; top:${composeMenu.y}px;`}
  >
    <button type="button" disabled={!hasSel} onclick={menuCut}>Cut</button>
    <button type="button" disabled={!hasSel} onclick={menuCopy}>Copy</button>
    <button type="button" onclick={menuPaste}>Paste</button>
  </div>
{/if}

<style>
  .wrap {
    position: relative;
    height: 100%;
    /* Transparent so the body dot-grid shows behind bubbles, mirroring Acy's
       "nav over grid, content shows grid" composition. */
    background: transparent;
  }
  .header {
    padding: .55rem .8rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    gap: .75rem;
  }
  .title-block {
    flex: 1;
    min-width: 0;
    gap: .15rem;
  }
  .title {
    font-weight: 600;
    letter-spacing: -.01em;
    font-size: 1.05rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subtitle { font-size: .78rem; color: var(--text-muted); }
  .messages {
    flex: 1;
    /* A centred column up to 720px, so lines stay readable on a wide window. */
    padding: 1rem max(1.5rem, calc((100% - 720px) / 2));
    display: flex;
    flex-direction: column;
    background-size: cover;
    background-position: center;
    background-repeat: no-repeat;
  }
  /* Dim the wallpaper slightly so bubbles stay readable. The bubble's own
     box-shadow + colored background does most of the work; this is belt-and-suspenders. */
  .messages.has-background::before {
    content: "";
    position: absolute;
    inset: 0;
    background: color-mix(in oklab, var(--bg) 35%, transparent);
    pointer-events: none;
    z-index: 0;
  }
  .messages.has-background { position: relative; }
  .messages.has-background > :global(*) { position: relative; z-index: 1; }
  .jump-latest {
    position: absolute;
    right: 1rem;
    bottom: 5.25rem;
    z-index: 8;
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    padding: .45rem .65rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .jump-latest:hover { background: var(--surface-hover); filter: none; }
  .jump-latest span { font-size: .75rem; font-weight: 600; }
  .empty {
    margin: auto;
    text-align: center;
    padding: 2rem;
  }
  .placeholder {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .compose {
    border-top: 1px solid var(--border);
    background: var(--surface);
    padding: .55rem .75rem .65rem;
    display: flex;
    flex-direction: column;
    gap: .35rem;
  }
  .compose-row {
    display: flex;
    align-items: center;
    gap: .5rem;
  }
  .compose-input {
    flex: 1;
    min-height: 2.25rem;
    max-height: 9rem;
    resize: vertical;
    padding: .5rem .65rem;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
    line-height: 1.3;
  }
  .compose-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
  .send {
    width: 2.5rem;
    height: 2.5rem;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--radius);
    border: none;
    background: var(--accent);
    color: var(--accent-contrast);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .send.save { background: var(--text); color: var(--bg); }
  .attach-wrap { position: relative; flex: none; }
  .attach-solo {
    width: 2.5rem;
    height: 2.5rem;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
  }
  .attach-solo:hover:not(:disabled) { background: var(--surface-hover); }
  .compose-field { position: relative; flex: 1; min-width: 0; display: flex; }
  .compose-field .compose-input { padding-right: 2.9rem; resize: none; }
  .home-card { width: min(380px, 100%); text-align: left; }
  .home-label {
    margin: 1rem 0 .35rem;
    font-family: var(--font-mono);
    font-size: .68rem;
    letter-spacing: .08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .home-label:first-child { margin-top: 0; }
  .home-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: .7rem;
    padding: .5rem .4rem;
    border: none;
    border-top: 1px solid var(--border);
    border-radius: 0;
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .home-row:hover { background: var(--surface-hover); }
  .home-row :global(svg) { color: var(--accent); flex: none; }
  .home-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .home-text { font-size: .9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .home-sub { font-size: .76rem; color: var(--text-muted); }
  .home-time { font-family: var(--font-mono); font-size: .7rem; color: var(--text-muted); white-space: nowrap; }
  .home-keys { margin-top: 1.2rem; font-family: var(--font-mono); font-size: .68rem; color: var(--text-muted); }
  .home-empty { text-align: center; }
  .aa-toggle {
    position: absolute;
    right: .35rem;
    bottom: .32rem;
    padding: .2rem .4rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font-weight: 600;
    font-size: .82rem;
    line-height: 1.2;
  }
  .aa-toggle:hover { background: var(--surface-hover); }
  .aa-toggle.on { background: var(--accent-soft); color: var(--on-accent-soft); }
  .rec-bar {
    flex: 1;
    min-width: 0;
    min-height: 2.5rem;
    display: flex;
    align-items: center;
    gap: .6rem;
    padding: 0 .8rem;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--surface);
  }
  .rec-level { flex: 1; display: flex; align-items: center; gap: 2px; height: 24px; overflow: hidden; }
  .rec-level i { display: block; flex: 1; max-width: 4px; min-width: 2px; border-radius: 2px; background: var(--danger); }
  .rec-level i.off { opacity: .25; }
  .send:disabled {
    opacity: .45;
    cursor: default;
  }
  .actions-backdrop,
  .actions-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 30;
    border: none;
    padding: 0;
    margin: 0;
    background: transparent;
    border-radius: 0;
    box-shadow: none;
    cursor: default;
  }
  .actions-menu {
    position: absolute;
    left: 0;
    bottom: calc(100% + .45rem);
    z-index: 31;
    min-width: 190px;
    display: flex;
    flex-direction: column;
    padding: .25rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow, 0 6px 24px rgba(0, 0, 0, .25));
  }
  .actions-menu button {
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: .5rem .7rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .actions-menu button:hover {
    background: var(--surface-2, var(--surface));
  }
  .format-row { display:flex; align-items:center; gap:calc(.5rem + 2px); margin-bottom:.25rem; }
  .format-close { display:inline-flex; align-items:center; justify-content:center; flex:0 0 2.5rem; width:2.5rem; height:2rem; padding:0; border:1px solid var(--border); border-radius:var(--radius-sm); background:transparent; color:var(--text-muted); }
  .format-close:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  .format-toolbar { display:flex; align-items:center; gap:2px; width:max-content; padding:2px; border:1px solid var(--border); border-radius:var(--radius-sm); background:var(--surface-2, var(--surface)); }
  .format-toolbar button { display:inline-flex; align-items:center; justify-content:center; width:2rem; height:1.8rem; padding:0; border:0; border-radius:5px; background:transparent; color:var(--text-muted); }
  .format-toolbar button:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  .format-separator { width:1px; height:1.15rem; margin:0 2px; background:var(--border); }
  .rec-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--danger);
    animation: rec-pulse 1s ease-in-out infinite;
  }
  @keyframes rec-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: .3; }
  }
  /* See ChatList.svelte — must beat `button:hover` from app.css. */
  .ctx-backdrop,
  .ctx-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 40;
    border: none;
    padding: 0;
    margin: 0;
    background: transparent;
    border-radius: 0;
    box-shadow: none;
    transition: none;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 170px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow, 0 6px 24px rgba(0, 0, 0, .25));
    padding: .25rem;
    box-sizing: border-box;
    /* Safety net if the menu is ever taller than the window. */
    max-height: calc(100vh - 1rem);
    overflow-y: auto;
  }
  .ctx-menu button {
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: .45rem .7rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .ctx-menu button:hover:not(:disabled) {
    background: var(--surface-2, var(--surface));
  }
  .ctx-menu button:disabled {
    opacity: .4;
    cursor: default;
  }
  .compose-error {
    color: var(--danger);
  }
  .reply-bar {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .35rem .5rem;
    margin-bottom: .35rem;
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .reply-bar-body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .reply-bar-label {
    color: var(--accent);
    font-weight: 600;
  }
  .reply-bar-text {
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .text-btn {
    background: transparent;
    border: none;
    color: var(--primary, currentColor);
    font: inherit;
    cursor: pointer;
    padding: 0;
  }
  .lock-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .lock-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: .65rem;
    max-width: 320px;
    text-align: center;
    padding: 1.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-dialog);
    background: var(--surface);
  }
  .lock-card h2 { margin: 0; font-size: 1.1rem; }
  .lock-card p { margin: 0; }
  .lock-icon {
    display: inline-flex;
    align-items: center;
    line-height: 0;
    color: var(--text-muted);
  }
  .title-ico {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    margin-right: .25rem;
    vertical-align: -1px;
    line-height: 0;
  }
  .pin-input {
    width: 100%;
    padding: .55rem .7rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
    text-align: center;
    letter-spacing: .25em;
  }
  .pin-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
  .lock-btn {
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    background: transparent;
    border: 1px solid var(--border);
    color: inherit;
    font: inherit;
    font-size: .85rem;
    padding: .35rem .7rem;
    border-radius: var(--radius);
    cursor: pointer;
    line-height: 1;
    align-self: center;
  }
  .lock-btn:hover { background: var(--surface-2, var(--surface)); }
  .menu-wrap { position: relative; }
  .menu-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    line-height: 0;
    padding: .4rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .menu-btn:hover { background: var(--surface-2, var(--surface)); }
  .meta-editor {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .55rem .75rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .pin-editor { flex-wrap: wrap; }
  .meta-input {
    flex: 1;
    padding: .45rem .6rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .meta-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
</style>
