<script lang="ts">
  import type { ChatSummary, Message } from "../../ipc";
  import RichText from "../RichText.svelte";
  import Avatar from "../Avatar.svelte";
  import { formatFullTime } from "../../format";
  import { Mic, Image as ImageIcon, Paperclip } from "@lucide/svelte";

  // A single message hit — avatar + chat name + time header, then a compact
  // rendering of the message content. The whole card is one clickable button so
  // the hover highlight covers it edge to edge. Shared by SearchPane and
  // StarredPane, which listed byte-identical rows before this was extracted.
  let { chat, message, onopen }: {
    chat: ChatSummary;
    message: Message;
    onopen: (chatId: string) => void;
  } = $props();
</script>

<li>
  <button type="button" class="hit" onclick={() => onopen(chat.id)}>
    <span class="head">
      <Avatar name={chat.name} size={26} avatarFile={chat.avatarFile} />
      <strong class="name">{chat.name}</strong>
      <span class="muted tiny time">{formatFullTime(message.createdAt)}</span>
    </span>
    <span class="content">
      {#if message.type === "TEXT"}
        <RichText blocks={message.content} />
      {:else if message.type === "VOICE"}
        <span class="media-chip"><Mic size={13} strokeWidth={2.25} /> Voice message</span>
      {:else if message.type === "CHECKLIST"}
        <span class="checklist">
          {#each message.checklistItems as item (item.id)}
            <span class="check-item">{item.checked ? "☑" : "☐"} {item.text}</span>
          {/each}
        </span>
      {:else if message.type === "FILE"}
        <span class="media-chip"><Paperclip size={13} strokeWidth={2.25} /> {message.fileAttachments[0]?.name ?? "File"}</span>
      {:else}
        <span class="media-chip"><ImageIcon size={13} strokeWidth={2.25} /> Image</span>
      {/if}
    </span>
  </button>
</li>

<style>
  .hit {
    display: flex;
    flex-direction: column;
    gap: .35rem;
    width: 100%;
    text-align: left;
    padding: .55rem .6rem;
    background: var(--surface);
    color: inherit;
    cursor: pointer;
  }
  .hit:hover { background: var(--surface-2); filter: none; }
  .head { display: flex; align-items: center; gap: .5rem; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .time { flex: 1; text-align: right; white-space: nowrap; }
  .content {
    padding-left: .25rem;
    font-size: .88rem;
    overflow-wrap: anywhere;
  }
  .checklist { display: flex; flex-direction: column; gap: .1rem; }
  .media-chip { display: inline-flex; align-items: center; gap: .3rem; line-height: 1; }
</style>
