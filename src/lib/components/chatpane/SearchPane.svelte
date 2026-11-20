<script module lang="ts">
  import type { ChatSummary as CachedChatSummary, Message as CachedMessage } from "../../ipc";
  type CachedHit = { chat: CachedChatSummary; message: CachedMessage };
  let cachedKey = "";
  let cachedMessages: CachedHit[] = [];
</script>
<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../../ipc";
  import { chats, lastSyncEvent, selectChat } from "../../stores/app";
  import Avatar from "../Avatar.svelte";
  import HitRow from "./HitRow.svelte";
  import HighlightedText from "../HighlightedText.svelte";

  type Hit = { chat: ChatSummary; message: Message };

  let { query }: { query: string } = $props();
  let allMessages = $state<Hit[]>([]);
  let loading = $state(true);
  let selectedIndex = $state(0);
  let paneEl: HTMLDivElement | undefined = $state();

  // Build the searchable index lazily on mount. Re-running on every keystroke would
  // be wasteful for the simple substring filter we're doing here.
  async function load() {
    loading = true;
    const key = `${$chats.map((chat) => chat.id).join("|")}:${JSON.stringify($lastSyncEvent)}`;
    if (key === cachedKey) {
      allMessages = cachedMessages;
      loading = false;
      return;
    }
    const loaded = await Promise.all($chats.map(async (chat) => ({ chat, data: await api.getChat(chat.id) })));
    const all: Hit[] = [];
    for (const { chat, data } of loaded) {
      if (!data) continue;
      for (const message of data.messages) all.push({ chat, message });
    }
    all.sort((a, b) => b.message.createdAt - a.message.createdAt);
    cachedKey = key;
    cachedMessages = all;
    allMessages = all;
    loading = false;
  }

  onMount(load);

  // Flatten a message to plain text so a substring query can hit any content type.
  function messageText(m: Message): string {
    // Voice-message transcripts land in `m.transcript`; adding it to the corpus
    // makes voice messages searchable by their spoken content once whisper has run.
    const transcript = m.transcript ?? "";
    if (m.type === "CHECKLIST") {
      return [m.checklistItems.map((i) => i.text).join(" "), transcript]
        .filter(Boolean)
        .join(" ");
    }
    const textFromBlocks = m.content
      .map((b) => b.spans.map((s) => s.text).join(""))
      .join(" ");
    return [textFromBlocks, transcript].filter(Boolean).join(" ");
  }

  // Chats whose name matches — shown as a "Chats" section above message hits.
  const chatHits = $derived.by<ChatSummary[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return $chats.filter((c) => c.name.toLowerCase().includes(q));
  });

  // Messages whose content (or transcript) matches — the "Messages" section.
  const messageHits = $derived.by<Hit[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return allMessages.filter((h) => messageText(h.message).toLowerCase().includes(q));
  });

  const hasResults = $derived(chatHits.length > 0 || messageHits.length > 0);
  const resultCount = $derived(chatHits.length + messageHits.length);

  $effect(() => {
    query;
    selectedIndex = 0;
  });

  function moveSelection(delta: number) {
    if (!resultCount) return;
    selectedIndex = (selectedIndex + delta + resultCount) % resultCount;
    queueMicrotask(() => {
      paneEl?.querySelector(`[data-search-index="${selectedIndex}"]`)?.scrollIntoView({ block: "nearest" });
    });
  }

  function openSelected() {
    if (selectedIndex < chatHits.length) {
      const chat = chatHits[selectedIndex];
      if (chat) void selectChat(chat.id);
      return;
    }
    const hit = messageHits[selectedIndex - chatHits.length];
    if (hit) void selectChat(hit.chat.id);
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveSelection(-1);
    } else if (e.key === "Enter" && resultCount) {
      e.preventDefault();
      openSelected();
    }
  }
</script>

<svelte:window onkeydown={onSearchKey} />

<div class="pane col" bind:this={paneEl}>
  <div class="list scroll">
    {#if loading}
      <div class="muted center tiny">Building index…</div>
    {:else if !query.trim()}
      <div class="muted center tiny">Search your chats and messages.</div>
    {:else if !hasResults}
      <div class="muted center tiny">No matches for "{query}".</div>
    {:else}
      {#if chatHits.length}
        <div class="section-h muted tiny">Chats</div>
        <ul class="hits divided-list">
          {#each chatHits as c, i (c.id)}
            <li>
              <button type="button" class="chat-hit" class:selected={selectedIndex === i} data-search-index={i} onclick={() => selectChat(c.id)}>
                <Avatar name={c.name} size={26} avatarFile={c.avatarFile} hue={c.bubbleColor} />
                <strong class="name"><HighlightedText text={c.name} {query} /></strong>
                {#if c.lastMessagePreview}
                  <span class="muted tiny prev"><HighlightedText text={c.lastMessagePreview} {query} /></span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if messageHits.length}
        <div class="section-h muted tiny">Messages</div>
        <ul class="hits divided-list">
          {#each messageHits as h, i (h.message.id)}
            <HitRow chat={h.chat} message={h.message} onopen={selectChat} {query} selected={selectedIndex === chatHits.length + i} resultIndex={chatHits.length + i} />
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; padding: .45rem; gap: .5rem; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding-right: .25rem; }
  .hits { list-style: none; margin: 0 0 .75rem; padding: 0; }
  .section-h { padding: .1rem .1rem .3rem; font-weight: 600; }
  .chat-hit {
    display: flex;
    align-items: center;
    gap: .5rem;
    width: 100%;
    text-align: left;
    padding: .5rem .6rem;
    background: var(--surface);
    color: inherit;
    cursor: pointer;
  }
  .chat-hit:hover,
  .chat-hit.selected { background: var(--surface-2); filter: none; }
  .name { flex-shrink: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prev { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; }
  .center { text-align: center; padding: 1.5rem .5rem; }
</style>
