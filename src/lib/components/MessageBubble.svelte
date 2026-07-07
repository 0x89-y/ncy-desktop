<script lang="ts">
  import type { Message } from "../ipc";
  import { api, extractMediaHash } from "../ipc";
  import {
    arrivedBlobs,
    transcribing,
    transcriptionErrors,
    markTranscribing,
    unmarkTranscribing,
    setTranscriptionError,
  } from "../stores/app";
  import RichText from "./RichText.svelte";
  import { formatFullTime } from "../format";
  import { Mic, Image as ImageIcon, AlarmClock } from "@lucide/svelte";

  let {
    message,
    bubbleColor,
    bubbleGradient,
  }: {
    message: Message;
    bubbleColor: string | null;
    bubbleGradient: boolean;
  } = $props();

  const time = $derived(formatFullTime(message.createdAt));

  const bubbleStyle = $derived.by(() => {
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

  // Reactions — curated quick strip + an extended set behind a toggle.
  const QUICK_REACTIONS = ["👍", "❤️", "😂", "🎉", "✅", "🔥"];
  const EXTRA_REACTIONS = [
    "🙏", "👀", "🤔", "😮", "😢", "😡",
    "💯", "⭐", "🚀", "💡", "👏", "🤝",
    "😍", "🥳", "🤯", "😅", "🙌", "✨",
  ];
  let showExtras = $state(false);
  // Hover strip is only revealed after a small settle (avoids flickering when
  // scanning down a chat) and hides when the pointer leaves both the bubble
  // and the strip itself.
  let hovering = $state(false);
  // Picker popover triggered by the ⋯ button or by "React…" in the ctx menu —
  // stays open even after the pointer leaves so the user can browse the extras.
  let pickerOpen = $state(false);

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
  function pickReaction(emoji: string) {
    pickerOpen = false;
    showExtras = false;
    void toggleReaction(emoji);
  }
  function openPickerFromMenu() {
    closeCtx();
    pickerOpen = true;
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
    if (!confirm("Delete this message? This will sync the deletion back to the phone.")) return;
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

  let voiceDataUrl = $state<string | null>(null);
  let voiceMissing = $state(false);
  let imageDataUrl = $state<string | null>(null);
  let imageMissing = $state(false);

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
</script>

<div
  class="row-wrap"
  onmouseenter={() => (hovering = true)}
  onmouseleave={() => (hovering = false)}
  role="group"
>
  {#if hovering && !editing && !reminding && !pickerOpen}
    <!-- Hover quick-reactions strip, Slack-style, floating above the bubble. -->
    <div class="hover-strip">
      {#each QUICK_REACTIONS as e (e)}
        <button
          type="button"
          class="rx-quick"
          class:on={message.reactions.includes(e)}
          disabled={busy}
          title={message.reactions.includes(e) ? `Remove ${e}` : `React ${e}`}
          onclick={() => void toggleReaction(e)}
        >{e}</button>
      {/each}
      <button
        type="button"
        class="rx-quick more"
        disabled={busy}
        title="More reactions"
        aria-label="More reactions"
        onclick={() => (pickerOpen = true)}
      >⋯</button>
    </div>
  {/if}
  <div
    class="bubble"
    style={bubbleStyle}
    role="group"
    oncontextmenu={openCtx}
  >
    {#if message.type === "VOICE"}
      <div class="voice">
        {#if voiceDataUrl}
          <audio controls preload="none" src={voiceDataUrl}></audio>
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
        <img class="image" src={imageDataUrl} alt="" />
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
          <button type="button" class="action-btn" onclick={cancelEdit} disabled={busy}>
            Cancel
          </button>
          <button type="button" class="action-btn primary" onclick={saveEdit} disabled={busy || !editText.trim()}>
            {busy ? "…" : "Save"}
          </button>
        </div>
      </div>
    {:else}
      <RichText blocks={message.content} />
    {/if}

    {#if message.tags.length}
      <div class="row tags">
        {#each message.tags as t (t)}
          <span class="chip tiny">#{t}</span>
        {/each}
      </div>
    {/if}

    {#if message.reactions.length}
      <div class="row reactions">
        {#each message.reactions as e (e)}
          <button
            type="button"
            class="rx-chip"
            disabled={busy}
            title={`Remove ${e}`}
            onclick={() => void toggleReaction(e)}
          >{e}</button>
        {/each}
      </div>
    {/if}

    <div class="row meta tiny muted">
      <span>{time}</span>
      {#if message.editedAt}
        <span title="edited">· edited</span>
      {/if}
      <span class="spacer"></span>
      <!-- Passive indicators — actions live in the right-click menu. -->
      {#if message.isStarred}
        <span class="indicator starred" title="Starred" aria-label="Starred">★</span>
      {/if}
      {#if message.reminderAt}
        <span class="indicator starred" title="Reminder set" aria-label="Reminder set">
          <AlarmClock size={13} strokeWidth={2.25} />
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
        />
        <button class="action-btn primary" onclick={saveReminder} disabled={busy || !reminderInput}>
          Save
        </button>
        {#if message.reminderAt}
          <button class="action-btn" onclick={clearReminder} disabled={busy}>Clear</button>
        {/if}
        <button class="action-btn" onclick={cancelReminder} disabled={busy}>Cancel</button>
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
      onclick={openPickerFromMenu}
    >React…</button>
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
    >{message.isStarred ? "Unstar" : "Star"}</button>
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

{#if pickerOpen}
  <button
    type="button"
    class="ctx-backdrop"
    aria-label="Close picker"
    onclick={() => { pickerOpen = false; showExtras = false; }}
    oncontextmenu={(e) => { e.preventDefault(); pickerOpen = false; showExtras = false; }}
  ></button>
  <div class="rx-picker" role="dialog" aria-label="Pick a reaction">
    <div class="rx-picker-row">
      {#each QUICK_REACTIONS as e (e)}
        <button
          type="button"
          class="rx-quick"
          class:on={message.reactions.includes(e)}
          disabled={busy}
          onclick={() => pickReaction(e)}
        >{e}</button>
      {/each}
      <button
        type="button"
        class="rx-quick more"
        disabled={busy}
        aria-label={showExtras ? "Fewer reactions" : "More reactions"}
        onclick={() => (showExtras = !showExtras)}
      >{showExtras ? "▲" : "▼"}</button>
    </div>
    {#if showExtras}
      <div class="rx-picker-grid">
        {#each EXTRA_REACTIONS as e (e)}
          <button
            type="button"
            class="rx-quick"
            class:on={message.reactions.includes(e)}
            disabled={busy}
            onclick={() => pickReaction(e)}
          >{e}</button>
        {/each}
      </div>
    {/if}
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
  /* Slack-style floating quick-reactions strip that appears on hover, above
     the bubble. Absolutely positioned so its show/hide doesn't reflow the
     surrounding messages. */
  .hover-strip {
    position: absolute;
    right: 8px;
    top: -12px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px;
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
    padding: 2px 6px;
    font-size: 16px;
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
  .rx-quick.more { font-size: 12px; opacity: .7; }
  .rx-quick:disabled { opacity: .5; cursor: default; }
  /* Row of reactions actually attached to the message, rendered inside the
     bubble under the content. Tap to remove. */
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: .3rem;
  }
  .rx-chip {
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 12%, transparent);
    color: inherit;
    border: 1px solid color-mix(in srgb, var(--bubble-mine-on, currentColor) 18%, transparent);
    border-radius: 999px;
    padding: 1px 7px;
    font-size: 13px;
    line-height: 1.4;
    cursor: pointer;
  }
  .rx-chip:hover:not(:disabled) {
    background: color-mix(in srgb, var(--bubble-mine-on, currentColor) 22%, transparent);
  }
  .rx-chip:disabled { opacity: .5; cursor: default; }
  /* Popover picker opened from the ⋯ button or the "React…" ctx menu entry.
     Positioned near the bubble via CSS anchor to the row-wrap wrapper. */
  .rx-picker {
    position: absolute;
    right: 0;
    bottom: 100%;
    margin-bottom: 4px;
    z-index: 42;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--card-shadow);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 240px;
  }
  .rx-picker-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .rx-picker-grid {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 2px;
    border-top: 1px solid var(--border);
    padding-top: 6px;
  }
  .bubble {
    background: var(--bubble-mine);
    color: var(--bubble-mine-on);
    border-radius: 18px 18px 4px 18px;
    padding: .55rem .8rem;
    max-width: min(640px, 85%);
    box-shadow: var(--shadow);
    position: relative;
  }
  .meta {
    margin-top: .25rem;
    gap: .35rem;
  }
  .voice .icon { display: inline-flex; align-items: center; line-height: 0; }
  .voice audio {
    width: 100%;
    max-width: 320px;
    height: 32px;
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
    gap: .35rem;
    min-width: min(340px, 80vw);
  }
  .edit-input {
    resize: vertical;
    padding: .4rem .55rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
    line-height: 1.3;
  }
  .edit-error { color: var(--error, #b00); }
  .edit-actions {
    display: flex;
    justify-content: flex-end;
    gap: .35rem;
  }
  .action-btn {
    padding: .25rem .65rem;
    border-radius: 999px;
    border: 1px solid var(--outline-variant);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .action-btn.primary {
    background: var(--accent, var(--bubble-mine));
    color: var(--accent-on, var(--bubble-mine-on));
    border-color: transparent;
  }
  .action-btn:disabled {
    opacity: .5;
    cursor: default;
  }
  .reminder-editor {
    display: flex;
    align-items: center;
    gap: .35rem;
    margin-top: .35rem;
    flex-wrap: wrap;
  }
  .reminder-input {
    padding: .25rem .4rem;
    border-radius: 8px;
    border: 1px solid var(--outline-variant);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: .8rem;
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
  .image-placeholder { font-style: italic; }
  .indicator {
    display: inline-flex;
    align-items: center;
    line-height: 1;
    padding: 0 .1rem;
  }
  .indicator.starred {
    color: var(--accent, gold);
    opacity: 1;
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
