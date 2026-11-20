<script lang="ts">
  import type { ChatSummary } from "../ipc";
  import Avatar from "./Avatar.svelte";
  import { formatTime } from "../format";
  import { chatListDensity } from "../stores/app";
  import { Lock } from "@lucide/svelte";

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
    hue={chat.bubbleColor}
  />
  <div class="middle">
    <div class="row top">
      <span class="title" title={chat.name}>
        {#if chat.isProtected}<span class="badge-ico"><Lock size={12} strokeWidth={2.25} /></span>{/if}
        {chat.name}
      </span>
      <span class="time">{formatTime(chat.lastMessageAt)}</span>
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
    position: relative;
    display: flex;
    align-items: center;
    gap: .75rem;
    width: 100%;
    text-align: left;
    background: transparent;
    color: var(--text);
    padding: .55rem 1.1rem;
    border-radius: 0;
  }
  .row-btn:hover { background: var(--surface-hover); }
  /* The open chat: the accent tint and a short accent bar, not a solid fill. */
  .row-btn.selected { background: var(--accent-soft); color: var(--text); }
  .row-btn.selected::before {
    content: "";
    position: absolute;
    left: 0;
    top: .45rem;
    bottom: .45rem;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .row-btn.compact {
    gap: .6rem;
    padding: .4rem .65rem;
  }
  .middle { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .15rem; }
  .top { width: 100%; }
  .bottom { width: 100%; }
  .title {
    font-weight: 600;
    letter-spacing: -.01em;
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
  .time {
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: .7rem;
    color: var(--text-muted);
  }
  .preview { font-size: .86rem; }
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
