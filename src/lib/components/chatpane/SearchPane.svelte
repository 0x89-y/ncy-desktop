<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../../ipc";
  import { chats, selectChat } from "../../stores/app";
  import Avatar from "../Avatar.svelte";
  import HitRow from "./HitRow.svelte";

  type Hit = { chat: ChatSummary; message: Message };

  let query = $state("");
  let allMessages = $state<Hit[]>([]);
  let loading = $state(true);

  // Build the searchable index lazily on mount. Re-running on every keystroke would
  // be wasteful for the simple substring filter we're doing here.
  async function load() {
    loading = true;
    const all: Hit[] = [];
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        all.push({ chat: c, message: m });
      }
    }
    all.sort((a, b) => b.message.createdAt - a.message.createdAt);
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
</script>

<div class="pane col">
  <input
    type="search"
    class="search"
    placeholder="Search chats & messages…"
    bind:value={query}
  />

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
          {#each chatHits as c (c.id)}
            <li>
              <button type="button" class="chat-hit" onclick={() => selectChat(c.id)}>
                <Avatar name={c.name} size={26} avatarFile={c.avatarFile} />
                <strong class="name">{c.name}</strong>
                {#if c.lastMessagePreview}
                  <span class="muted tiny prev">{c.lastMessagePreview}</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if messageHits.length}
        <div class="section-h muted tiny">Messages</div>
        <ul class="hits divided-list">
          {#each messageHits as h (h.message.id)}
            <HitRow chat={h.chat} message={h.message} onopen={selectChat} />
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; padding: .75rem; gap: .5rem; }
  .search {
    padding: .5rem .7rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant, var(--border));
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .search:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
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
  .chat-hit:hover { background: var(--surface-2); filter: none; }
  .name { flex-shrink: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prev { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; }
  .center { text-align: center; padding: 1.5rem .5rem; }
</style>
