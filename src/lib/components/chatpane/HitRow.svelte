<script lang="ts">
  import type { ChatSummary, Message } from "../../ipc";
  import HighlightedText from "../HighlightedText.svelte";
  import Avatar from "../Avatar.svelte";
  import { formatFullTime } from "../../format";
  import { Mic, Image as ImageIcon, Paperclip } from "@lucide/svelte";

  // A single message hit — avatar + chat name + time header, then a compact
  // rendering of the message content. The whole card is one clickable button so
  // the hover highlight covers it edge to edge. Shared by SearchPane and
  // StarredPane, which listed byte-identical rows before this was extracted.
  let { chat, message, onopen, query = "", selected = false, resultIndex }: {
    chat: ChatSummary;
    message: Message;
    onopen: (chatId: string) => void;
    query?: string;
    selected?: boolean;
    resultIndex?: number;
  } = $props();

  const searchableText = $derived(
    message.type === "CHECKLIST"
      ? message.checklistItems.map((item) => item.text).join(" ")
      : [
          message.content.map((block) => block.spans.map((span) => span.text).join("")).join(" "),
          message.transcript ?? "",
        ].filter(Boolean).join(" "),
  );
</script>

<li data-search-index={resultIndex}>
  <button type="button" class="hit" class:selected onclick={() => onopen(chat.id)}>
    <span class="head">
      <Avatar name={chat.name} size={26} avatarFile={chat.avatarFile} />
      <strong class="name"><HighlightedText text={chat.name} {query} /></strong>
      <span class="muted tiny time">{formatFullTime(message.createdAt)}</span>
    </span>
    <span class="content">
      {#if searchableText}
        <HighlightedText text={searchableText} {query} />
      {:else if message.type === "VOICE"}
        <span class="media-chip"><Mic size={13} strokeWidth={2.25} /> Voice message</span>
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
  .hit:hover,
  .hit.selected { background: var(--surface-2); filter: none; }
  .head { display: flex; align-items: center; gap: .5rem; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .time { flex: 1; text-align: right; white-space: nowrap; }
  .content {
    padding-left: .25rem;
    font-size: .88rem;
    overflow-wrap: anywhere;
  }
  .media-chip { display: inline-flex; align-items: center; gap: .3rem; line-height: 1; }
</style>
