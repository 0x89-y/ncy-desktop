<script lang="ts">
  import type { Message } from "../ipc";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, extractMediaHash } from "../ipc";
  import {
    arrivedBlobs,
    transcribing,
    transcriptionErrors,
    quickReactions,
    messageDensity,
    markTranscribing,
    unmarkTranscribing,
    setTranscriptionError,
  } from "../stores/app";
  import { openViewer } from "../stores/viewer";
  import RichText from "./RichText.svelte";
  import { formatFullTime, formatMessageTime, replyPreviewText } from "../format";
  import {
    Mic,
    Image as ImageIcon,
    AlarmClock,
    Check,
    Pause,
    Pencil,
    Play,
    Reply,
    Star,
    Trash2,
    X,
    Paperclip,
    Download,
  } from "@lucide/svelte";

  let {
    message,
    bubbleColor,
    bubbleGradient,
    onReply,
    onJumpToReplyTarget,
    replyTargetExists = false,
    highlighted = false,
    isRemote = false,
  }: {
    message: Message;
    bubbleColor: string | null;
    bubbleGradient: boolean;
    onReply?: (m: Message) => void;
    onJumpToReplyTarget?: (id: string) => void;
    /** False when the quoted message has since been deleted — quote stays, jump doesn't. */
    replyTargetExists?: boolean;
    /** Briefly tinted after a jump landed on this message. */
    highlighted?: boolean;
    isRemote?: boolean;
  } = $props();

  const quoteText = $derived(replyPreviewText(message.replyToPreview, message.replyToType));
  const quoteTime = $derived(formatMessageTime(message.replyToCreatedAt));

  const time = $derived(formatMessageTime(message.createdAt));
  const fullTime = $derived(formatFullTime(message.createdAt));
  const editedTime = $derived(formatFullTime(message.editedAt));
  const inlineTag = $derived(
    message.tags.length === 1 && message.tags[0].length <= 16 ? message.tags[0] : null,
  );

  function remoteDeviceColor(deviceId: string | null | undefined): string {
    let hash = 0;
    const id = deviceId ?? "";
    for (let i = 0; i < id.length; i += 1) {
      hash = Math.imul(hash, 31) + id.charCodeAt(i);
    }
    return `hsl(${(hash >>> 0) % 360} 65% 52%)`;
  }

  const bubbleStyle = $derived.by(() => {
    if (isRemote) return `--remote-device-color:${remoteDeviceColor(message.authorDeviceId)}`;
    if (!bubbleColor) return "";
    if (bubbleGradient) {
      return `background:linear-gradient(135deg, ${bubbleColor}, color-mix(in oklab, ${bubbleColor} 60%, var(--bubble-mine)))`;
    }
    return `background:${bubbleColor}`;
  });

  let busy = $state(false);
  let editing = $state(false);
  let editText = $state("");
  let editError = $state<string | null>(null);
  let reminding = $state(false);
  let reminderInput = $state("");

  // Right-click context menu (replaces the always-visible action row).
  let ctx = $state<{ x: number; y: number } | null>(null);
  function openCtx(e: MouseEvent) {
    if (editing || reminding) return;
    e.preventDefault();
    e.stopPropagation();
    // Menu is ~180×180; keep it on-screen.
    const x = Math.min(e.clientX, window.innerWidth - 190);
    const y = Math.min(e.clientY, window.innerHeight - 190);
    ctx = { x, y };
  }
  function closeCtx() { ctx = null; }

  // Reactions — a small delayed hover strip for the configured common cases.
  let hovering = $state(false);
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;

  async function toggleReaction(emoji: string) {
    if (busy) return;
    busy = true;
    try {
      await api.authorToggleReaction(message.chatId, message.id, emoji);
    } catch (e) {
      console.error("toggle reaction failed", e);
    } finally {
      busy = false;
    }
  }
  function showHoverStripSoon() {
    if (editing || reminding) return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      hovering = true;
      hoverTimer = null;
    }, 260);
  }
  function hideHoverStrip() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    hovering = false;
  }

  const isTranscribing = $derived($transcribing.has(message.id));
  const transcriptionError = $derived($transcriptionErrors[message.id] ?? null);

  async function transcribeThis() {
    if (message.type !== "VOICE") {
      console.warn("transcribeThis: not a voice message", message.type);
      return;
    }
    if ($transcribing.has(message.id)) {
      console.warn("transcribeThis: already in flight");
      return;
    }
    console.log("transcribeThis: starting", message.chatId, message.id);
    markTranscribing(message.id);
    try {
      await api.authorTranscribeVoice(message.chatId, message.id);
      console.log("transcribeThis: done");
    } catch (e: any) {
      const msg = typeof e === "string" ? e : (e?.message ?? String(e));
      console.error("transcribeThis failed:", msg, e);
      setTranscriptionError(message.id, msg);
    } finally {
      unmarkTranscribing(message.id);
    }
  }

  // ISO `yyyy-MM-ddTHH:mm` for the datetime-local input, in local time.
  function toLocalIsoMinutes(ms: number): string {
    const d = new Date(ms);
    const off = d.getTimezoneOffset();
    const local = new Date(d.getTime() - off * 60000);
    return local.toISOString().slice(0, 16);
  }
  function startReminder() {
    const seed = message.reminderAt ?? Date.now() + 60 * 60 * 1000; // +1h default
    reminderInput = toLocalIsoMinutes(seed);
    reminding = true;
  }
  function cancelReminder() { reminding = false; }
  function onReminderKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancelReminder();
    } else if (e.key === "Enter") {
      e.preventDefault();
      void saveReminder();
    }
  }
  async function saveReminder() {
    if (busy) return;
    const ms = new Date(reminderInput).getTime();
    if (!Number.isFinite(ms)) return;
    busy = true;
    try {
      await api.authorSetReminder(message.chatId, message.id, ms);
      reminding = false;
    } catch (e) {
      console.error("set reminder failed", e);
    } finally {
      busy = false;
    }
  }
  async function clearReminder() {
    if (busy) return;
    busy = true;
    try {
      await api.authorSetReminder(message.chatId, message.id, null);
      reminding = false;
    } catch (e) {
      console.error("clear reminder failed", e);
    } finally {
      busy = false;
    }
  }

  // Flatten the message's rich text into a single plain string. Mirrors what the
  // phone's edit-saving path produces — phase 2a only supports plaintext edits
  // from the desktop (no rich formatting).
  function flattenContent(m: Message): string {
    return m.content
      .map((b) => b.spans.map((s) => s.text).join(""))
      .join("\n");
  }

  async function toggleStar() {
    if (busy) return;
    busy = true;
    try {
      await api.authorToggleStar(message.chatId, message.id);
    } catch (e) {
      console.error("toggle star failed", e);
    } finally {
      busy = false;
    }
  }

  async function toggleChecklistItem(itemId: string) {
    if (busy) return;
    busy = true;
    try {
      await api.authorToggleChecklistItem(message.chatId, message.id, itemId);
    } catch (e) {
      console.error("toggle checklist item failed", e);
    } finally {
      busy = false;
    }
  }

  async function deleteMessage() {
    if (busy) return;
    const prefs = await api.getDesktopPrefs().catch(() => ({ askBeforeDeleting: true }));
    if (
      prefs.askBeforeDeleting &&
      !confirm("Delete this message? This will sync the deletion back to the phone.")
    )
      return;
    busy = true;
    try {
      await api.authorDeleteMessage(message.chatId, message.id);
    } catch (e) {
      console.error("delete failed", e);
    } finally {
      busy = false;
    }
  }

  function startEdit() {
    editText = flattenContent(message);
    editError = null;
    editing = true;
  }

  function cancelEdit() {
    editing = false;
    editError = null;
  }

  async function saveEdit() {
    if (busy) return;
    const text = editText.trim();
    if (!text) {
      editError = "edit cannot be empty";
      return;
    }
    busy = true;
    editError = null;
    try {
      await api.authorEditText(message.chatId, message.id, text);
      editing = false;
    } catch (e: any) {
      editError = typeof e === "string" ? e : (e?.message ?? "edit failed");
    } finally {
      busy = false;
    }
  }

  function onEditKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void saveEdit();
    }
  }

  // ---- Phase 2b — media blob rendering ----

  // Pull the first image reference out of the message (content's inlineImage takes
  // precedence; legacy `imageFiles[0]` as fallback).
  function firstImageRef(m: Message): string | null {
    for (const block of m.content) {
      for (const span of block.spans) {
        if (span.inlineImage) return span.inlineImage;
      }
    }
    return m.imageFiles[0] ?? null;
  }

  const voiceHash = $derived(extractMediaHash(message.voiceFile));
  const imageHash = $derived(extractMediaHash(firstImageRef(message)));
  const fileAttachment = $derived(message.fileAttachments?.[0] ?? null);
  const fileHash = $derived(extractMediaHash(fileAttachment?.file ?? null));

  let voiceDataUrl = $state<string | null>(null);
  let voiceMissing = $state(false);
  let voiceAudio = $state<HTMLAudioElement>();
  let voicePlaying = $state(false);
  let voiceCurrentSeconds = $state(0);
  let voiceMetadataDuration = $state<number | null>(null);
  let imageDataUrl = $state<string | null>(null);
  let imageMissing = $state(false);
  let fileAvailable = $state(false);
  let fileSaving = $state(false);

  const voiceDurationSeconds = $derived(
    voiceMetadataDuration ?? (message.voiceDurationMs ?? 0) / 1000,
  );
  const voiceProgress = $derived(
    voiceDurationSeconds > 0
      ? Math.min(1, voiceCurrentSeconds / voiceDurationSeconds)
      : 0,
  );
  const voiceBars = Array.from({ length: 30 }, (_, index) => {
    // Stable, message-specific pseudo-waveform. It keeps the player visually
    // useful without decoding every audio file just to sample its amplitude.
    let value = 0;
    for (let i = 0; i < message.id.length; i += 1) {
      value = (value * 31 + message.id.charCodeAt(i) + index * 17) | 0;
    }
    return 0.25 + (Math.abs(value) % 75) / 100;
  });

  function formatVoiceTime(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
    const whole = Math.floor(seconds);
    return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
  }

  function syncVoiceState() {
    if (!voiceAudio) return;
    voiceCurrentSeconds = Number.isFinite(voiceAudio.currentTime)
      ? voiceAudio.currentTime
      : 0;
    if (Number.isFinite(voiceAudio.duration) && voiceAudio.duration > 0) {
      voiceMetadataDuration = voiceAudio.duration;
    }
  }

  async function toggleVoicePlayback() {
    if (!voiceAudio) return;
    if (voiceAudio.paused) {
      try {
        await voiceAudio.play();
      } catch (e) {
        console.error("play voice message", e);
      }
    } else {
      voiceAudio.pause();
    }
  }

  function seekVoice(e: Event) {
    if (!voiceAudio) return;
    const next = Number((e.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(next)) return;
    voiceAudio.currentTime = next;
    voiceCurrentSeconds = next;
  }

  // Re-fetch when the hash changes OR a blob with the relevant hash just arrived.
  $effect(() => {
    const arrived = $arrivedBlobs;
    const h = voiceHash;
    if (!h) {
      voiceDataUrl = null;
      voiceMissing = false;
      return;
    }
    voiceMissing = false;
    api
      .readBlob(h, "voice")
      .then((b64) => {
        if (b64 == null) {
          voiceDataUrl = null;
          voiceMissing = !arrived.has(h);
        } else {
          voiceDataUrl = `data:audio/mp4;base64,${b64}`;
        }
      })
      .catch((e) => console.error("readBlob voice", e));
  });

  $effect(() => {
    const arrived = $arrivedBlobs;
    const h = imageHash;
    if (!h) {
      imageDataUrl = null;
      imageMissing = false;
      return;
    }
    imageMissing = false;
    api
      .readBlob(h, "image")
      .then((b64) => {
        if (b64 == null) {
          imageDataUrl = null;
          imageMissing = !arrived.has(h);
        } else {
          imageDataUrl = `data:image/jpeg;base64,${b64}`;
        }
      })
      .catch((e) => console.error("readBlob image", e));
  });

  $effect(() => {
    const arrived = $arrivedBlobs;
    const h = fileHash;
    if (!h) {
      fileAvailable = false;
      return;
    }
    void arrived;
    api.blobExists(h, "file").then((exists) => (fileAvailable = exists))
      .catch((e) => console.error("blobExists file", e));
  });

  function formatFileSize(bytes: number | null | undefined): string {
    if (bytes == null) return "";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function copyMessage() {
    const text = message.type === "CHECKLIST"
      ? message.checklistItems.map((item) => `${item.checked ? "[x]" : "[ ]"} ${item.text}`).join("\n")
      : (message.transcript || flattenContent(message));
    if (text) await navigator.clipboard.writeText(text);
  }

  async function toggleTextChecklistBlock(blockIndex: number) {
    if (busy) return;
    busy = true;
    try {
      await api.authorToggleTextChecklistBlock(message.chatId, message.id, blockIndex);
    } catch (e) {
      console.error("toggle text checklist item failed", e);
    } finally {
      busy = false;
    }
  }

  async function downloadFile() {
    if (!fileHash || !fileAttachment || !fileAvailable || fileSaving) return;
    fileSaving = true;
    try {
      const safeName = fileAttachment.name.replace(/[\\/:*?"<>|]/g, "_") || "file";
      const dest = await saveDialog({ defaultPath: safeName });
      if (dest) await api.saveBlob(fileHash, "file", dest);
    } catch (e) {
      console.error("save file", e);
    } finally {
      fileSaving = false;
    }
  }
</script>

<div
  class="row-wrap"
  class:compact={$messageDensity === "compact"}
  class:highlighted
  class:remote={isRemote}
  data-mid={message.id}
  onmouseenter={showHoverStripSoon}
  onmouseleave={hideHoverStrip}
  role="group"
>
  {#if hovering && !editing && !reminding}
    <!-- Hover strip, Slack-style, floating above the bubble: reply + quick reactions. -->
    <div class="hover-strip">
      <button
        type="button"
        class="rx-quick reply-hover"
        disabled={busy}
        title="Reply"
        aria-label="Reply"
        onclick={() => onReply?.(message)}
      ><Reply size={14} /></button>
      {#each $quickReactions as e (e)}
        <button
          type="button"
          class="rx-quick"
          class:on={message.reactions.includes(e)}
          disabled={busy}
          title={message.reactions.includes(e) ? `Remove ${e}` : `React ${e}`}
          onclick={() => void toggleReaction(e)}
        >{e}</button>
      {/each}
    </div>
  {/if}
  <div
    class="bubble"
    class:compact={$messageDensity === "compact"}
    class:remote={isRemote}
    style={bubbleStyle}
    role="group"
    oncontextmenu={openCtx}
  >
    {#if message.replyToId}
      <!-- Rendered from the snapshot frozen on this message, not the live original.
           A quote whose target was deleted stays readable but is inert. -->
      {#if replyTargetExists}
        <button
          type="button"
          class="reply-quote"
          title="Jump to the original message"
          onclick={() => onJumpToReplyTarget?.(message.replyToId!)}
        >
          <span class="reply-quote-time">{quoteTime}</span>
          <span class="reply-quote-text">{quoteText}</span>
        </button>
      {:else}
        <div class="reply-quote deleted">
          <span class="reply-quote-time">{quoteTime}</span>
          <span class="reply-quote-text">{quoteText}</span>
        </div>
      {/if}
    {/if}
    {#if message.type === "VOICE"}
      <div class="voice">
        {#if voiceDataUrl}
          <div class="voice-player">
            <audio
              bind:this={voiceAudio}
              preload="metadata"
              src={voiceDataUrl}
              onloadedmetadata={syncVoiceState}
              ondurationchange={syncVoiceState}
              ontimeupdate={syncVoiceState}
              onplay={() => (voicePlaying = true)}
              onpause={() => (voicePlaying = false)}
              onended={(event) => {
                voicePlaying = false;
                event.currentTarget.currentTime = 0;
                voiceCurrentSeconds = 0;
              }}
            ></audio>
            <button
              type="button"
              class="voice-toggle"
              onclick={() => void toggleVoicePlayback()}
              title={voicePlaying ? "Pause voice message" : "Play voice message"}
              aria-label={voicePlaying ? "Pause voice message" : "Play voice message"}
            >
              {#if voicePlaying}
                <Pause size={17} fill="currentColor" strokeWidth={2.25} />
              {:else}
                <Play size={17} fill="currentColor" strokeWidth={2.25} />
              {/if}
            </button>
            <div class="voice-track">
              <div class="voice-waveform" aria-hidden="true">
                {#each voiceBars as height, index}
                  <span
                    class:played={(index + 1) / voiceBars.length <= voiceProgress}
                    style={`height:${Math.round(height * 100)}%`}
                  ></span>
                {/each}
              </div>
              <input
                class="voice-seek"
                type="range"
                min="0"
                max={Math.max(voiceDurationSeconds, 0.01)}
                step="0.01"
                value={voiceCurrentSeconds}
                oninput={seekVoice}
                aria-label="Voice message position"
                aria-valuetext={`${formatVoiceTime(voiceCurrentSeconds)} of ${formatVoiceTime(voiceDurationSeconds)}`}
              />
              <div class="voice-time">
                <span>{formatVoiceTime(voiceCurrentSeconds)}</span>
                <span>{formatVoiceTime(voiceDurationSeconds)}</span>
              </div>
            </div>
          </div>
        {:else if voiceHash}
          <div class="row muted tiny">
            <span class="icon"><Mic size={14} strokeWidth={2.25} /></span>
            <span>
              {#if voiceMissing}
                Voice message · not on this device
              {:else}
                Voice message{message.voiceDurationMs ? ` · ${Math.round(message.voiceDurationMs / 1000)}s` : ""} · downloading…
              {/if}
            </span>
          </div>
        {:else}
          <div class="row muted tiny">
            <span class="icon"><Mic size={14} strokeWidth={2.25} /></span>
            <span>Voice message{message.voiceDurationMs ? ` · ${Math.round(message.voiceDurationMs / 1000)}s` : ""}</span>
          </div>
        {/if}
        {#if isTranscribing}
          <div class="transcript-loading tiny muted">Transcribing…</div>
        {:else if message.transcript}
          <div class="transcript tiny">{message.transcript}</div>
        {/if}
        {#if transcriptionError}
          <div class="transcript-error tiny">
            Transcription failed: {transcriptionError}
          </div>
        {/if}
      </div>
    {:else if message.type === "CHECKLIST"}
      <ul class="checklist">
        {#each message.checklistItems as item (item.id)}
          <li>
            <input
              type="checkbox"
              checked={item.checked}
              disabled={busy}
              onchange={() => toggleChecklistItem(item.id)}
            />
            <span class:done={item.checked}>{item.text}</span>
          </li>
        {/each}
      </ul>
    {:else if message.type === "IMAGE"}
      {#if imageDataUrl}
        <button
          type="button"
          class="image-btn"
          onclick={() => imageHash && openViewer(imageHash)}
          title="Open image"
          aria-label="Open image"
        >
          <img class="image" src={imageDataUrl} alt="" />
        </button>
      {:else if imageHash}
        <div class="image-placeholder muted tiny row">
          <ImageIcon size={14} strokeWidth={2.25} />
          <span>{imageMissing ? "Image · not on this device" : "Image · downloading…"}</span>
        </div>
      {:else}
        <div class="image-placeholder muted tiny row">
          <ImageIcon size={14} strokeWidth={2.25} />
          <span>Image (legacy reference, not transferable)</span>
        </div>
      {/if}
    {:else if message.type === "FILE"}
      {#if fileAttachment}
        <button
          type="button"
          class="file-card"
          disabled={!fileAvailable || fileSaving}
          onclick={() => void downloadFile()}
          title={fileAvailable ? "Save file" : "Downloading file"}
        >
          <Paperclip size={20} strokeWidth={2.1} />
          <span class="file-info">
            <strong>{fileAttachment.name}</strong>
            <small>{formatFileSize(fileAttachment.sizeBytes)}{fileAvailable ? "" : " · downloading…"}</small>
          </span>
          <Download size={17} strokeWidth={2.1} />
        </button>
      {:else}
        <div class="row muted tiny"><Paperclip size={14} /><span>File</span></div>
      {/if}
    {:else if editing}
      <div class="edit">
        <textarea
          class="edit-input"
          bind:value={editText}
          onkeydown={onEditKey}
          rows="2"
        ></textarea>
        {#if editError}
          <div class="edit-error tiny">{editError}</div>
        {/if}
        <div class="edit-actions">
          <button
            type="button"
            class="edit-action"
            onclick={cancelEdit}
            disabled={busy}
            title="Cancel"
            aria-label="Cancel"
          >
            <X class="edit-icon" size={15} strokeWidth={2.5} aria-hidden="true" />
          </button>
          <button
            type="button"
            class="edit-action primary"
            onclick={saveEdit}
            disabled={busy || !editText.trim()}
            title="Save edit"
            aria-label="Save edit"
          >
            <Check class="edit-icon" size={15} strokeWidth={2.5} aria-hidden="true" />
          </button>
        </div>
      </div>
    {:else}
      <RichText blocks={message.content} onChecklistToggle={toggleTextChecklistBlock} />
    {/if}

    {#if message.tags.length && !inlineTag}
      <div class="row tags">
        {#each message.tags as t (t)}
          <span class="chip tiny">#{t}</span>
        {/each}
      </div>
    {/if}

    <div class="row meta tiny muted">
      <span title={fullTime}>{time}</span>
      {#if message.editedAt}
        <span class="edited-indicator" title={editedTime ? `Edited ${editedTime}` : "Edited"} aria-label="Edited">
          <Pencil size={11} strokeWidth={2.35} />
        </span>
      {/if}
      {#if inlineTag}
        <span class="meta-tag">#{inlineTag}</span>
      {/if}
      <span class="spacer"></span>
      {#if message.reactions.length}
        <span class="meta-reactions" aria-label="Reactions">
          {#each message.reactions as e (e)}
            <button
              type="button"
              class="rx-chip"
              disabled={busy}
              title={`Remove ${e}`}
              onclick={() => void toggleReaction(e)}
            >{e}</button>
          {/each}
        </span>
      {/if}
      <!-- Passive indicators — actions live in the right-click menu. -->
      {#if message.isStarred}
        <span class="indicator" title="Favorite" aria-label="Favorite">
          <Star size={12} strokeWidth={2.35} />
        </span>
      {/if}
      {#if message.reminderAt}
        <span class="indicator" title="Reminder set" aria-label="Reminder set">
          <AlarmClock size={12} strokeWidth={2.35} />
        </span>
      {/if}
    </div>

    {#if reminding}
      <div class="reminder-editor">
        <input
          type="datetime-local"
          class="reminder-input"
          bind:value={reminderInput}
          disabled={busy}
          aria-label="Reminder time"
          onkeydown={onReminderKey}
        />
        <div class="reminder-actions">
          <button
            type="button"
            class="reminder-action primary"
            onclick={saveReminder}
            disabled={busy || !reminderInput}
            title="Save reminder"
            aria-label="Save reminder"
          >
            <Check class="reminder-icon" size={15} strokeWidth={2.5} aria-hidden="true" />
          </button>
          {#if message.reminderAt}
            <button
              type="button"
              class="reminder-action danger"
              onclick={clearReminder}
              disabled={busy}
              title="Clear reminder"
              aria-label="Clear reminder"
            >
              <Trash2 class="reminder-icon" size={14} strokeWidth={2.35} aria-hidden="true" />
            </button>
          {/if}
          <button
            type="button"
            class="reminder-action"
            onclick={cancelReminder}
            disabled={busy}
            title="Cancel"
            aria-label="Cancel"
          >
            <X class="reminder-icon" size={15} strokeWidth={2.5} aria-hidden="true" />
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if ctx}
  <button
    type="button"
    class="ctx-backdrop"
    aria-label="Close menu"
    onclick={closeCtx}
    oncontextmenu={(e) => { e.preventDefault(); closeCtx(); }}
  ></button>
  <div class="ctx-menu" role="menu" style={`left:${ctx.x}px; top:${ctx.y}px;`}>
    <button
      type="button"
      disabled={busy}
      onclick={() => { closeCtx(); onReply?.(message); }}
    >Reply</button>
    {#if message.type === "TEXT" || message.type === "IMAGE" || message.type === "CHECKLIST" || (message.type === "VOICE" && message.transcript)}
      <button
        type="button"
        disabled={busy}
        onclick={() => { closeCtx(); void copyMessage(); }}
      >Copy</button>
    {/if}
    {#if message.type === "TEXT"}
      <button
        type="button"
        disabled={busy}
        onclick={() => { closeCtx(); startEdit(); }}
      >Edit…</button>
    {/if}
    {#if message.type === "VOICE"}
      <button
        type="button"
        disabled={busy || isTranscribing}
        onclick={() => { closeCtx(); void transcribeThis(); }}
      >{message.transcript ? "Re-transcribe" : "Transcribe"}</button>
    {/if}
    <button
      type="button"
      disabled={busy}
      onclick={() => { closeCtx(); void toggleStar(); }}
    >{message.isStarred ? "Remove from favorites" : "Add to favorites"}</button>
    <button
      type="button"
      disabled={busy}
      onclick={() => { closeCtx(); startReminder(); }}
    >{message.reminderAt ? "Change reminder…" : "Set reminder…"}</button>
    {#if message.reminderAt}
      <button
        type="button"
        disabled={busy}
        onclick={() => { closeCtx(); void clearReminder(); }}
      >Clear reminder</button>
    {/if}
    <hr class="ctx-sep" />
    <button
      type="button"
      class="danger"
      disabled={busy}
      onclick={() => { closeCtx(); void deleteMessage(); }}
    >Delete</button>
  </div>
{/if}

<style>
  .row-wrap {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    margin-bottom: .35rem;
    position: relative;
  }
  .row-wrap.compact {
    margin-bottom: .22rem;
  }
  .row-wrap.remote { align-items: flex-start; }
  .row-wrap.remote .hover-strip { left: 8px; right: auto; }
  /* Flash after a jump-to-original lands here. Fades on its own; the timer in
     ChatView clears the flag afterwards. */
  .row-wrap.highlighted .bubble {
    animation: reply-flash 1.4s ease-out;
  }
  @keyframes reply-flash {
    0%, 55% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 60%, transparent); }
    100% { box-shadow: 0 0 0 3px transparent; }
  }
  /* The reply button sits in the same strip as the reactions but carries an icon,
     so it opts out of the emoji font-size. */
  .rx-quick.reply-hover {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted, inherit);
  }
  /* Floating quick-reactions strip. Delayed in JS so scanning messages does not
     constantly flash controls. */
  .hover-strip {
    position: absolute;
    right: 8px;
    top: -16px;
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 4px 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill, 999px);
    box-shadow: var(--card-shadow);
    z-index: 2;
  }
  .rx-quick {
    background: transparent;
    border: none;
    border-radius: 999px;
    width: 1.85rem;
    height: 1.85rem;
    padding: 0;
    font-size: 17px;
    line-height: 1;
    cursor: pointer;
    color: inherit;
  }
  .rx-quick:hover:not(:disabled) {
    background: var(--surface-hover, var(--surface-2));
  }
  .rx-quick.on {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
  .rx-quick:disabled { opacity: .5; cursor: default; }
  .meta-reactions {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    max-width: min(11rem, 45%);
    overflow: hidden;
  }
  .rx-chip {
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 12%, transparent);
    color: inherit;
    border: 1px solid color-mix(in srgb, var(--bubble-mine-on, currentColor) 18%, transparent);
    border-radius: 999px;
    padding: 0 5px;
    min-width: 1.35rem;
    height: 1.15rem;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .rx-chip:hover:not(:disabled) {
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 22%, transparent);
  }
  .rx-chip:disabled { opacity: .5; cursor: default; }
  /* Quoted message at the top of a reply bubble. Rendered as a <button> when the
     original still exists so it is keyboard-reachable, as a <div> when it does not. */
  .reply-quote {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    margin-bottom: .3rem;
    padding: .25rem .45rem;
    text-align: left;
    font: inherit;
    color: inherit;
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 12%, transparent);
    border: none;
    border-left: 3px solid color-mix(in srgb, var(--bubble-mine-on, currentColor) 70%, transparent);
    border-radius: 6px;
    cursor: pointer;
  }
  .reply-quote:hover:not(.deleted) {
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 18%, transparent);
  }
  .reply-quote.deleted { cursor: default; }
  .reply-quote-time {
    font-size: .68rem;
    opacity: .7;
  }
  .reply-quote-text {
    font-size: .82rem;
    opacity: .9;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .bubble {
    background: var(--bubble-mine);
    color: var(--bubble-mine-on);
    border-radius: 18px 18px 4px 18px;
    padding: .48rem .72rem;
    max-width: min(640px, 85%);
    box-shadow: var(--shadow);
    position: relative;
  }
  .bubble.compact {
    border-radius: 14px 14px 4px 14px;
    padding: .34rem .58rem;
  }
  .bubble.remote {
    --bubble-mine: var(--surface-2);
    --bubble-mine-on: var(--text);
    background: color-mix(in oklab, var(--remote-device-color) 24%, var(--surface));
    color: var(--text);
    border: 1px solid color-mix(in oklab, var(--remote-device-color) 38%, var(--border));
    border-radius: 18px 18px 18px 4px;
  }
  .bubble.remote.compact { border-radius: 14px 14px 14px 4px; }
  .meta {
    margin-top: .2rem;
    gap: .35rem;
  }
  .bubble.compact .meta {
    margin-top: .12rem;
    gap: .28rem;
  }
  .edited-indicator {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 0;
    overflow: hidden;
    opacity: 0;
    color: currentColor;
    transition: width .12s ease, opacity .12s ease;
  }
  .bubble:hover .edited-indicator,
  .bubble:focus-within .edited-indicator {
    width: .8rem;
    opacity: .75;
  }
  .edited-indicator :global(svg) {
    display: block;
    color: currentColor;
    stroke: currentColor;
  }
  .meta-tag {
    display: inline-flex;
    align-items: center;
    max-width: 7.5rem;
    height: 1.15rem;
    padding: 0 .35rem;
    border: 1px solid color-mix(in srgb, var(--bubble-mine-on, currentColor) 18%, transparent);
    border-radius: 999px;
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 10%, transparent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: inherit;
    font-size: 11px;
    line-height: 1;
  }
  .voice .icon { display: inline-flex; align-items: center; line-height: 0; }
  .file-card {
    display: flex;
    align-items: center;
    gap: .55rem;
    width: min(310px, 65vw);
    padding: .55rem .65rem;
    color: inherit;
    text-align: left;
    background: color-mix(in srgb, currentColor 12%, transparent);
    border: 1px solid color-mix(in srgb, currentColor 18%, transparent);
    border-radius: 10px;
  }
  .file-card:not(:disabled) { cursor: pointer; }
  .file-card:disabled { opacity: .7; }
  .file-info { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 1px; }
  .file-info strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-info small { opacity: .72; }
  .voice-player {
    display: flex;
    align-items: center;
    gap: .6rem;
    width: min(260px, 62vw);
    min-width: 210px;
  }
  .voice-player audio { display: none; }
  .voice-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 2.15rem;
    height: 2.15rem;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, currentColor 18%, transparent);
    color: inherit;
  }
  .voice-toggle:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 28%, transparent);
  }
  .voice-toggle :global(svg) { display: block; }
  .voice-track {
    position: relative;
    flex: 1;
    min-width: 0;
    padding-top: 2px;
  }
  .voice-waveform {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 24px;
    overflow: hidden;
  }
  .voice-waveform span {
    flex: 1 1 0;
    min-width: 2px;
    border-radius: 999px;
    background: currentColor;
    opacity: .35;
  }
  .voice-waveform span.played { opacity: 1; }
  .voice-seek {
    position: absolute;
    inset: 0 0 14px;
    width: 100%;
    height: 24px;
    margin: 0;
    padding: 0;
    cursor: pointer;
    opacity: 0;
  }
  .voice-seek:focus-visible {
    accent-color: currentColor;
  }
  .voice-track:focus-within {
    border-radius: 4px;
    outline: 2px solid currentColor;
    outline-offset: 2px;
  }
  .voice-time {
    display: flex;
    justify-content: space-between;
    margin-top: 1px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
    opacity: .72;
  }
  .bubble.compact .voice-player {
    width: min(240px, 60vw);
  }
  .transcript {
    margin-top: .3rem;
    font-style: italic;
    opacity: .8;
    line-height: 1.35;
    white-space: pre-wrap;
    max-width: 320px;
  }
  .transcript-loading {
    margin-top: .3rem;
    font-style: italic;
    opacity: .55;
  }
  .transcript-error {
    margin-top: .3rem;
    color: var(--danger, #b00);
    white-space: pre-wrap;
    max-width: 320px;
  }
  .bubble.compact .transcript,
  .bubble.compact .transcript-loading,
  .bubble.compact .transcript-error {
    margin-top: .2rem;
  }
  /* A bare wrapper so the thumbnail is keyboard-reachable; it must not add any
     box of its own or the bubble's image layout shifts. */
  .image-btn {
    display: block;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 12px;
    cursor: zoom-in;
    line-height: 0;
  }
  .image-btn:hover { filter: none; }
  .image-btn:hover .image { opacity: 0.92; }
  .image {
    display: block;
    max-width: 100%;
    max-height: 360px;
    border-radius: 12px;
    object-fit: contain;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: .4rem;
    min-width: min(340px, 80vw);
    padding: .35rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 6px 18px rgba(0, 0, 0, .14);
  }
  .edit-input {
    resize: vertical;
    min-height: 4.75rem;
    padding: .5rem .6rem;
    border-radius: 8px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: var(--text);
    font: inherit;
    line-height: 1.3;
  }
  .edit-input:focus {
    outline: 2px solid color-mix(in srgb, var(--accent) 55%, transparent);
    outline-offset: 1px;
  }
  .edit-error {
    color: var(--danger, var(--error, #b00));
    padding: 0 .1rem;
  }
  .edit-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
  }
  .edit-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.15rem;
    min-width: 2.15rem;
    height: 2.15rem;
    padding: 0;
    border: 1px solid var(--outline-variant);
    border-radius: 8px 0 0 8px;
    background: var(--surface-2, var(--surface));
    color: var(--text);
    cursor: pointer;
  }
  .edit-action + .edit-action {
    border-left: none;
    border-radius: 0 8px 8px 0;
  }
  .edit-action:hover:not(:disabled) {
    background: var(--surface-hover, var(--surface-3, var(--surface-2)));
  }
  .edit-action.primary {
    background: var(--accent, var(--bubble-mine));
    color: var(--accent-on, var(--bubble-mine-on));
    border-color: var(--accent, var(--bubble-mine));
  }
  .edit-action:disabled {
    opacity: .45;
    cursor: default;
  }
  .edit-action :global(.edit-icon) {
    display: block;
    width: 15px;
    height: 15px;
    color: currentColor;
    stroke: currentColor;
    flex: 0 0 auto;
    pointer-events: none;
  }
  .reminder-editor {
    display: flex;
    align-items: center;
    margin-top: .45rem;
    padding: .35rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 6px 18px rgba(0, 0, 0, .14);
    max-width: 100%;
  }
  .reminder-input {
    min-width: 0;
    flex: 1 1 12.5rem;
    height: 2.15rem;
    padding: 0 .55rem;
    border-radius: 8px 0 0 8px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: var(--text);
    font: inherit;
    font-size: .85rem;
    line-height: 1;
  }
  .reminder-input:focus {
    outline: 2px solid color-mix(in srgb, var(--accent) 55%, transparent);
    outline-offset: 1px;
    z-index: 1;
  }
  .reminder-actions {
    display: flex;
    align-items: center;
    height: 2.15rem;
  }
  .reminder-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.15rem;
    min-width: 2.15rem;
    height: 2.15rem;
    padding: 0;
    border: 1px solid var(--outline-variant);
    border-left: none;
    border-radius: 0;
    background: var(--surface-2, var(--surface));
    color: var(--text);
    cursor: pointer;
  }
  .reminder-action:first-child {
    border-radius: 0;
  }
  .reminder-action:last-child {
    border-radius: 0 8px 8px 0;
  }
  .reminder-action:hover:not(:disabled) {
    background: var(--surface-hover, var(--surface-3, var(--surface-2)));
    color: var(--text);
  }
  .reminder-action.primary {
    background: var(--accent, var(--bubble-mine));
    color: var(--accent-on, var(--bubble-mine-on));
    border-color: var(--accent, var(--bubble-mine));
  }
  .reminder-action.danger:hover:not(:disabled) {
    color: var(--danger, var(--error, #b00));
  }
  .reminder-action:disabled {
    opacity: .45;
    cursor: default;
  }
  .reminder-action :global(.reminder-icon) {
    display: block;
    width: 15px;
    height: 15px;
    color: currentColor;
    stroke: currentColor;
    flex: 0 0 auto;
    pointer-events: none;
  }
  .reminder-action.danger :global(.reminder-icon) {
    width: 14px;
    height: 14px;
  }
  @media (max-width: 520px) {
    .reminder-editor {
      align-items: stretch;
      flex-direction: column;
    }
    .reminder-input {
      width: 100%;
      flex-basis: auto;
      border-radius: 8px;
    }
    .reminder-actions {
      width: 100%;
    }
    .reminder-action {
      flex: 1 1 0;
      min-width: 0;
      border-left: 1px solid var(--outline-variant);
    }
    .reminder-action + .reminder-action {
      border-left: none;
    }
    .reminder-action:first-child {
      border-radius: 8px 0 0 8px;
    }
    .reminder-action:last-child {
      border-radius: 0 8px 8px 0;
    }
  }
  .checklist {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .checklist li {
    display: flex;
    align-items: center;
    gap: .4rem;
    padding: .1rem 0;
  }
  .done {
    text-decoration: line-through;
    opacity: .65;
  }
  .tags { margin-top: .35rem; flex-wrap: wrap; }
  .bubble.compact .tags { margin-top: .2rem; }
  .image-placeholder { font-style: italic; }
  .indicator {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    width: .9rem;
    height: .9rem;
    color: currentColor;
    opacity: .8;
  }
  .indicator :global(svg) {
    display: block;
    color: currentColor;
    stroke: currentColor;
  }
  /* Right-click context menu. Same pattern as ChatList: backdrop is a <button>
     for a11y and must not pick up the global accent-pill hover styling. */
  .ctx-backdrop,
  .ctx-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: transparent;
    border: none;
    padding: 0;
    margin: 0;
    border-radius: 0;
    box-shadow: none;
    transition: none;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 170px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--card-shadow);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: .25rem;
  }
  .ctx-menu button {
    background: transparent;
    border: none;
    color: inherit;
    padding: .45rem .6rem;
    text-align: left;
    font: inherit;
    border-radius: 8px;
    cursor: pointer;
  }
  .ctx-menu button:hover:not(:disabled) {
    background: var(--surface-hover, var(--surface-2));
  }
  .ctx-menu button:disabled {
    opacity: .5;
    cursor: default;
  }
  .ctx-menu button.danger { color: var(--danger, var(--error, #b00)); }
  .ctx-sep {
    border: none;
    border-top: 1px solid var(--border);
    margin: .25rem 0;
  }
</style>
