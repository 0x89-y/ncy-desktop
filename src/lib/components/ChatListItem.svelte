<script lang="ts">
  import type { ChatSummary } from "../ipc";
  import Avatar from "./Avatar.svelte";
  import { formatTime } from "../format";
  import { chatListDensity } from "../stores/app";
  import { Pin, Lock } from "@lucide/svelte";

  let {
    chat,
    selected,
    onclick,
    oncontextmenu,
  }: {
    chat: ChatSummary;
    selected: boolean;
    onclick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
  } = $props();
</script>

<button
  class="row-btn"
  class:selected
  class:compact={$chatListDensity === "compact"}
  onclick={onclick}
  oncontextmenu={oncontextmenu}
>
  <Avatar
    name={chat.name}
    size={$chatListDensity === "compact" ? 36 : 44}
    avatarFile={chat.avatarFile}
  />
  <div class="middle">
    <div class="row top">
      <span class="title" title={chat.name}>
        {#if chat.isPinned}<span class="badge-ico"><Pin size={12} strokeWidth={2.25} /></span>{/if}
        {#if chat.isProtected}<span class="badge-ico"><Lock size={12} strokeWidth={2.25} /></span>{/if}
        {chat.name}
      </span>
      <span class="muted tiny time">{formatTime(chat.lastMessageAt)}</span>
    </div>
    {#if $chatListDensity === "comfortable"}
      <div class="row bottom">
        <span class="preview muted">{chat.lastMessagePreview ?? "No messages yet"}</span>
      </div>
    {/if}
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
    padding: .6rem .65rem;
  }
  .row-btn:hover { background: var(--surface-2); }
  .row-btn.selected { background: var(--primary-container); color: var(--on-primary-container); }
  .row-btn.compact {
    gap: .6rem;
    padding: .4rem .65rem;
  }
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
  .badge-ico {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    margin-right: .2rem;
    line-height: 0;
    vertical-align: -1px;
  }
  .tags { flex-wrap: wrap; gap: .25rem; }
</style>
