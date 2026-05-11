<script lang="ts">
  import type { Message } from "../ipc";
  import { api, extractMediaHash } from "../ipc";
  import { arrivedBlobs } from "../stores/app";
  import RichText from "./RichText.svelte";
  import { formatFullTime } from "../format";

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

<div class="row-wrap">
  <div class="bubble" style={bubbleStyle}>
    {#if message.type === "VOICE"}
      <div class="voice">
        {#if voiceDataUrl}
          <audio controls preload="none" src={voiceDataUrl}></audio>
        {:else if voiceHash}
          <div class="row muted tiny">
            <span class="icon">🎙️</span>
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
            <span class="icon">🎙️</span>
            <span>Voice message{message.voiceDurationMs ? ` · ${Math.round(message.voiceDurationMs / 1000)}s` : ""}</span>
          </div>
        {/if}
      </div>
    {:else if message.type === "CHECKLIST"}
      <ul class="checklist">
        {#each message.checklistItems as item (item.id)}
          <li>
            <input type="checkbox" checked={item.checked} disabled />
            <span class:done={item.checked}>{item.text}</span>
          </li>
        {/each}
      </ul>
    {:else if message.type === "IMAGE"}
      {#if imageDataUrl}
        <img class="image" src={imageDataUrl} alt="" />
      {:else if imageHash}
        <div class="image-placeholder muted tiny">
          🖼️ {imageMissing ? "Image · not on this device" : "Image · downloading…"}
        </div>
      {:else}
        <div class="image-placeholder muted tiny">
          🖼️ Image (legacy reference, not transferable)
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

    <div class="row meta tiny muted">
      <span>{time}</span>
      {#if message.editedAt}
        <span title="edited">· edited</span>
      {/if}
      {#if message.reminderAt}
        <span class="reminder" title={"Reminder: " + formatFullTime(message.reminderAt)}>⏰</span>
      {/if}
      <span class="spacer"></span>
      {#if message.type === "TEXT" && !editing}
        <button
          type="button"
          class="action"
          title="Edit (syncs back to phone)"
          disabled={busy}
          onclick={startEdit}
        >
          ✎
        </button>
      {/if}
      <button
        type="button"
        class="action"
        class:starred={message.isStarred}
        title={message.isStarred ? "Unstar" : "Star"}
        disabled={busy}
        onclick={toggleStar}
      >
        {message.isStarred ? "★" : "☆"}
      </button>
      <button
        type="button"
        class="action danger"
        title="Delete (syncs back to phone)"
        disabled={busy}
        onclick={deleteMessage}
      >
        ✕
      </button>
    </div>
  </div>
</div>

<style>
  .row-wrap {
    display: flex;
    justify-content: flex-end;
    margin-bottom: .35rem;
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
  .voice .icon { font-size: 1.05rem; }
  .voice audio {
    width: 100%;
    max-width: 320px;
    height: 32px;
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
  .action {
    background: transparent;
    border: none;
    color: inherit;
    opacity: .55;
    cursor: pointer;
    padding: 0 .15rem;
    font: inherit;
    line-height: 1;
  }
  .action:hover:not(:disabled) {
    opacity: 1;
  }
  .action:disabled {
    cursor: default;
    opacity: .3;
  }
  .action.starred {
    color: var(--accent, gold);
    opacity: 1;
  }
  .action.danger:hover:not(:disabled) {
    color: var(--error, #b00);
  }
</style>
