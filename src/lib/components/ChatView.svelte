<script lang="ts">
  import {
    activeChat,
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
  import { messagePreview, replyPreviewText } from "../format";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { writeText, readText } from "@tauri-apps/plugin-clipboard-manager";
  import { parseMarkdown, prefixSelectedLines, wrapSelection } from "../markdown";
  import {
    Bold, Code2, Heading1, Heading2, Heading3, Italic, List, ListChecks,
    Plus, Pin, Lock, Strikethrough, Unlock, X,
    MessageSquare, Mic, Settings, Square,
  } from "@lucide/svelte";
  import Avatar from "./Avatar.svelte";
  import MessageBubble from "./MessageBubble.svelte";

  let scroller: HTMLDivElement | undefined = $state();
  let composeTextarea: HTMLTextAreaElement | undefined = $state();
  let lastCount = $state(0);
  let composeText = $state("");
  let sending = $state(false);
  let sendError = $state<string | null>(null);

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
  $effect(() => {
    const c = $activeChat;
    const hash = c ? extractMediaHash(c.metadata.backgroundFile) : null;
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

  $effect(() => {
    const c = $activeChat;
    if (!c || !scroller) return;
    const count = c.messages.length;
    if (count !== lastCount) {
      queueMicrotask(() => {
        if (scroller) scroller.scrollTop = scroller.scrollHeight;
      });
      lastCount = count;
    }
  });

  async function send() {
    const c = $activeChat;
    if (!c) return;
    if (!composeText.trim() || sending) return;
    const blocks = parseMarkdown(composeText);
    if (blocks.length === 0) return;
    sending = true;
    sendError = null;
    try {
      await api.authorSendMessage(c.metadata.id, blocks, consumeReplyId());
      composeText = "";
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "send failed");
    } finally {
      sending = false;
    }
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
  // null = menu only; "rename"/"tags" = an inline editor is showing.
  let editMode = $state<null | "rename" | "tags">(null);
  let editValue = $state("");
  let metaBusy = $state(false);

  function openEditor(mode: "rename" | "tags") {
    const c = $activeChat;
    if (!c) return;
    editValue =
      mode === "rename"
        ? c.metadata.name
        : c.metadata.tags.join(", ");
    editMode = mode;
    chatSettingsOpen = false;
  }

  function cancelEdit() {
    editMode = null;
    editValue = "";
  }

  async function runMeta(patch: {
    name?: string;
    isPinned?: boolean;
    isArchived?: boolean;
    tags?: string[];
    remoteMessagesOnLeft?: boolean;
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

  async function saveEditor() {
    const v = editValue.trim();
    if (editMode === "rename") {
      if (!v) return;
      await runMeta({ name: v });
    } else if (editMode === "tags") {
      await runMeta({ tags: v.split(",").map((t) => t.trim()).filter(Boolean) });
    }
    cancelEdit();
  }

  async function togglePin() {
    const c = $activeChat;
    if (!c) return;
    chatSettingsOpen = false;
    await runMeta({ isPinned: !c.metadata.isPinned });
  }

  async function toggleArchive() {
    const c = $activeChat;
    if (!c) return;
    chatSettingsOpen = false;
    await runMeta({ isArchived: !c.metadata.isArchived });
  }

  async function deleteChat() {
    const c = $activeChat;
    if (!c) return;
    chatSettingsOpen = false;
    const prefs = await api.getDesktopPrefs().catch(() => ({ askBeforeDeleting: true }));
    if (
      prefs.askBeforeDeleting &&
      !confirm(
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
    chatSettingsOpen = false;
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
    chatSettingsOpen = false;
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
    chatSettingsOpen = false;
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
    chatSettingsOpen = false;
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
    }, 250);
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
      />
      <div class="col title-block">
        <div class="title">
          {#if chat.metadata.isPinned}<span class="title-ico"><Pin size={13} strokeWidth={2.25} /></span>{/if}
          {#if chat.metadata.isProtected}<span class="title-ico"><Lock size={13} strokeWidth={2.25} /></span>{/if}
          {chat.metadata.name}
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
      <button class="settings-backdrop" aria-label="Close chat settings" onclick={() => chatSettingsOpen = false}></button>
      <section class="chat-settings" aria-label="Chat settings">
        <div class="settings-head"><div><strong>Chat settings</strong><div class="tiny muted">{chat.metadata.name}</div></div><button class="icon-close" title="Close" aria-label="Close" onclick={() => chatSettingsOpen = false}><X size={17} /></button></div>
        <div class="settings-scroll">
          <div class="settings-group"><span class="settings-label">Organization</span>
            <button type="button" onclick={() => openEditor("rename")}>Rename…</button>
            <button type="button" onclick={togglePin}>
              {chat.metadata.isPinned ? "Unpin" : "Pin"}
            </button>
            <button type="button" onclick={toggleArchive}>
              {chat.metadata.isArchived ? "Unarchive" : "Archive"}
            </button>
            <button type="button" onclick={() => openEditor("tags")}>Edit tags…</button>
          </div>
          <div class="settings-group"><span class="settings-label">Appearance</span>
            <button
              type="button"
              onclick={() => {
                void runMeta({ remoteMessagesOnLeft: !chat.metadata.remoteMessagesOnLeft });
              }}
            >{chat.metadata.remoteMessagesOnLeft ? "Keep all messages on right" : "Other devices on left"}</button>
            <button type="button" onclick={setAvatar}>
              {chat.metadata.avatarFile ? "Change avatar…" : "Set avatar…"}
            </button>
            {#if chat.metadata.avatarFile}
              <button type="button" onclick={clearAvatar}>Remove avatar</button>
            {/if}
            <button type="button" onclick={setBackground}>
              {chat.metadata.backgroundFile ? "Change background…" : "Set background…"}
            </button>
            {#if chat.metadata.backgroundFile}
              <button type="button" onclick={clearBackground}>Remove background</button>
            {/if}
          </div>
          <div class="settings-group"><span class="settings-label">Privacy</span>
            {#if chat.metadata.isProtected}
              <button type="button" onclick={() => openPin("change")}>Change PIN…</button>
            {:else}
              <button type="button" onclick={() => openPin("protect")}>Protect chat…</button>
            {/if}
          </div>
          <div class="settings-group danger-group"><span class="settings-label">Data</span>
            <button type="button" class="danger" onclick={deleteChat}>Delete chat</button>
          </div>
        </div>
      </section>
    {/if}

    {#if editMode}
      <div class="meta-editor">
        <input
          class="meta-input"
          placeholder={editMode === "rename" ? "Chat name" : "tag1, tag2, tag3"}
          bind:value={editValue}
          onkeydown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); void saveEditor(); }
            if (e.key === "Escape") { e.preventDefault(); cancelEdit(); }
          }}
          disabled={metaBusy}
        />
        <button type="button" class="send" onclick={saveEditor} disabled={metaBusy}>
          {metaBusy ? "…" : "Save"}
        </button>
        <button type="button" class="text-btn" onclick={cancelEdit}>Cancel</button>
      </div>
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
        />
      {:else}
        <div class="empty muted">No messages yet.</div>
      {/each}
    </div>

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
        <div class="attach-group">
          <button
            type="button"
            class="attach"
            title="Add attachment or checklist"
            aria-label="Add attachment or checklist"
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
                formattingOpen = !formattingOpen;
              }}>{formattingOpen ? "Hide formatting" : "Formatting"}</button>
              <button type="button" role="menuitem" onclick={() => {
                composeActionsOpen = false;
                void startRecording();
              }}><Mic size={15} strokeWidth={2.25} /> Record voice message</button>
            </div>
          {/if}
          {#if recording}
          <button
            type="button"
            class="attach"
            class:recording
            title="Stop recording"
            aria-label="Stop recording"
            onclick={stopRecording}
            disabled={sending}
          >
            <Square size={16} strokeWidth={2.25} />
          </button>
          {/if}
        </div>
        {#if recording}
          <span class="rec-indicator" title="Recording…">
            <span class="rec-dot"></span>
            <span class="tiny mono">{fmtElapsed(recordElapsed)}</span>
            <button type="button" class="text-btn tiny" onclick={cancelRecording}>Cancel</button>
          </span>
        {/if}
        <textarea
          class="compose-input"
          placeholder="Type a message…"
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
          class="send"
          onclick={send}
          disabled={sending || !composeText.trim()}
        >
          {sending ? "…" : "Send"}
        </button>
      </div>
    </footer>
    {/if}
  </div>
{:else}
  <div class="placeholder muted">
    <div>
      <div style="line-height:0;color:var(--text-muted);opacity:.5"><MessageSquare size={56} strokeWidth={1.5} /></div>
      <p>Select a chat to view its messages.</p>
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
    height: 100%;
    /* Transparent so the body dot-grid shows behind bubbles, mirroring Acy's
       "nav over grid, content shows grid" composition. */
    background: transparent;
  }
  .header {
    padding: .55rem .8rem;
    background: var(--surface);
    border-bottom: 1px solid var(--outline-variant);
    gap: .75rem;
  }
  .title-block {
    flex: 1;
    min-width: 0;
    gap: .15rem;
  }
  .title {
    font-weight: 700;
    font-size: 1.05rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .messages {
    flex: 1;
    padding: 1rem;
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
    border-top: 1px solid var(--outline-variant);
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
  /* Compact action group: one add menu plus the voice recorder. */
  .attach-group {
    display: inline-flex;
    align-items: stretch;
    flex-shrink: 0;
    align-self: center;
    position: relative;
    border: 1px solid var(--outline-variant);
    border-radius: 12px;
  }
  .compose-input {
    flex: 1;
    min-height: 2.25rem;
    max-height: 9rem;
    resize: vertical;
    padding: .5rem .65rem;
    border-radius: 12px;
    border: 1px solid var(--outline-variant);
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
    padding: .5rem .9rem;
    border-radius: var(--radius);
    border: none;
    background: var(--accent, var(--bubble-mine));
    color: var(--accent-on, var(--bubble-mine-on));
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .send:disabled {
    opacity: .45;
    cursor: default;
  }
  .attach {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    padding: .45rem .6rem;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
    line-height: 0;
  }
  .attach-group > .attach:last-child { border-left: 1px solid var(--outline-variant); }
  .attach:first-child { border-radius: 11px 0 0 11px; }
  .attach:last-child { border-radius: 0 11px 11px 0; }
  .attach:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-2, var(--surface));
  }
  .attach:disabled {
    opacity: .45;
    cursor: default;
  }
  .attach.recording {
    color: var(--danger);
    background: color-mix(in oklab, var(--danger) 12%, transparent);
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
    border: 1px solid var(--outline-variant);
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
    border-radius: 8px;
    cursor: pointer;
  }
  .actions-menu button:hover {
    background: var(--surface-2, var(--surface));
  }
  .format-row { display:flex; align-items:center; gap:calc(.5rem + 2px); margin-bottom:.25rem; }
  .format-close { display:inline-flex; align-items:center; justify-content:center; flex:0 0 2.5rem; width:2.5rem; height:2rem; padding:0; border:1px solid var(--outline-variant); border-radius:var(--radius-sm); background:transparent; color:var(--text-muted); }
  .format-close:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  .format-toolbar { display:flex; align-items:center; gap:2px; width:max-content; padding:2px; border:1px solid var(--outline-variant); border-radius:var(--radius-sm); background:var(--surface-2, var(--surface)); }
  .format-toolbar button { display:inline-flex; align-items:center; justify-content:center; width:2rem; height:1.8rem; padding:0; border:0; border-radius:5px; background:transparent; color:var(--text-muted); }
  .format-toolbar button:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  .format-separator { width:1px; height:1.15rem; margin:0 2px; background:var(--outline-variant); }
  .rec-indicator {
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    padding: 0 .35rem;
  }
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
    border: 1px solid var(--outline-variant);
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
    border-radius: 8px;
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
    color: var(--error, #b00);
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
    border: 1px solid var(--outline-variant);
    border-radius: 16px;
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
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
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
    border: 1px solid var(--outline-variant);
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
  .settings-backdrop, .settings-backdrop:hover { position:fixed; inset:32px 0 0; z-index:50; margin:0; padding:0; border:0; border-radius:0; background:rgba(0,0,0,.34); box-shadow:none; }
  .chat-settings { position:fixed; top:32px; right:0; bottom:0; z-index:51; width:min(380px, 100vw); display:flex; flex-direction:column; background:var(--surface); border-left:1px solid var(--outline-variant); box-shadow:-10px 0 30px rgba(0,0,0,.2); }
  .settings-head { display:flex; align-items:center; justify-content:space-between; padding:1rem; border-bottom:1px solid var(--outline-variant); }
  .icon-close { display:inline-flex; align-items:center; justify-content:center; width:2rem; height:2rem; padding:0; border:0; background:transparent; color:var(--text-muted); }
  .icon-close:hover { background:var(--surface-hover); color:var(--text); filter:none; }
  .settings-scroll { overflow:auto; padding:.75rem; }
  .settings-group { display:flex; flex-direction:column; padding:.4rem 0 .75rem; border-bottom:1px solid var(--outline-variant); }
  .settings-label { padding:.35rem .6rem; color:var(--text-muted); font-size:.72rem; font-weight:700; text-transform:uppercase; letter-spacing:.04em; }
  .settings-group button { border:0; background:transparent; color:inherit; text-align:left; padding:.6rem; border-radius:8px; font:inherit; }
  .settings-group button:hover { background:var(--surface-hover); filter:none; }
  .settings-group button.danger { color:var(--error, #b00); }
  .danger-group { border-bottom:0; }
  .meta-editor {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .55rem .75rem;
    border-bottom: 1px solid var(--outline-variant);
    background: var(--surface);
  }
  .pin-editor { flex-wrap: wrap; }
  .meta-input {
    flex: 1;
    padding: .45rem .6rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .meta-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
</style>
