<script lang="ts">
  import type { ChatSummary } from "../ipc";
  import Avatar from "./Avatar.svelte";
  import { formatTime } from "../format";

  let {
    chat,
    selected,
    onclick,
  }: {
    chat: ChatSummary;
    selected: boolean;
    onclick: () => void;
  } = $props();
</script>

<button class="row-btn" class:selected onclick={onclick}>
  <Avatar name={chat.name} size={44} avatarFile={chat.avatarFile} />
  <div class="middle">
    <div class="row top">
      <span class="title" title={chat.name}>
        {#if chat.isPinned}<span class="pin">📌</span>{/if}
        {#if chat.isProtected}<span class="lock">🔒</span>{/if}
        {chat.name}
      </span>
      <span class="muted tiny time">{formatTime(chat.lastMessageAt)}</span>
    </div>
    <div class="row bottom">
      <span class="preview muted">{chat.lastMessagePreview ?? "No messages yet"}</span>
    </div>
    {#if chat.tags.length}
      <div class="row tags">
        {#each chat.tags as t (t)}
          <span class="chip tiny">#{t}</span>
        {/each}
      </div>
    {/if}
  </div>
</button>

<style>
  .row-btn {
    display: flex;
    align-items: center;
    gap: .75rem;
    width: 100%;
    text-align: left;
    background: transparent;
    color: var(--on-surface);
    border-radius: var(--radius-md);
    padding: .55rem .65rem;
    border: 1px solid transparent;
  }
  .row-btn:hover { background: var(--surface-2); }
  .row-btn.selected { background: var(--primary-container); color: var(--on-primary-container); }
  .middle { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .15rem; }
  .top { width: 100%; }
  .bottom { width: 100%; }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .time { white-space: nowrap; }
  .pin, .lock { margin-right: .15rem; }
  .tags { flex-wrap: wrap; gap: .25rem; }
</style>
