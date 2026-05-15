<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../ipc";
  import { chats, selectChat, activeView } from "../stores/app";
  import RichText from "../components/RichText.svelte";
  import Avatar from "../components/Avatar.svelte";
  import { formatFullTime } from "../format";

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
    if (m.type === "CHECKLIST") {
      return m.checklistItems.map((i) => i.text).join(" ");
    }
    return m.content
      .map((b) => b.spans.map((s) => s.text).join(""))
      .join(" ");
  }

  const hits = $derived.by<Hit[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return allMessages.filter((h) => {
      if (h.chat.name.toLowerCase().includes(q)) return true;
      return messageText(h.message).toLowerCase().includes(q);
    });
  });

  async function open(chatId: string) {
    activeView.set("chats");
    await selectChat(chatId);
  }
</script>

<div class="page scroll">
  <header>
    <h1>🔎 Search</h1>
    <p class="muted">Find any message across every chat.</p>
  </header>

  <input
    type="search"
    class="search"
    placeholder="Search messages…"
    bind:value={query}
  />

  {#if loading}
    <div class="muted center">Building index…</div>
  {:else if !query.trim()}
    <div class="muted center">Type to search across {allMessages.length} message{allMessages.length === 1 ? "" : "s"}.</div>
  {:else if hits.length === 0}
    <div class="muted center">No matches for "{query}".</div>
  {:else}
    <div class="muted tiny">
      {hits.length} match{hits.length === 1 ? "" : "es"}
    </div>
    <ul class="list">
      {#each hits as h (h.message.id)}
        <li class="card hit">
          <button class="row chat-row" onclick={() => open(h.chat.id)}>
            <Avatar name={h.chat.name} size={28} avatarFile={h.chat.avatarFile} />
            <strong>{h.chat.name}</strong>
            <span class="muted tiny spacer">{formatFullTime(h.message.createdAt)}</span>
          </button>
          <div class="content">
            {#if h.message.type === "TEXT"}
              <RichText blocks={h.message.content} />
            {:else if h.message.type === "VOICE"}
              <span>🎙️ Voice message</span>
            {:else if h.message.type === "CHECKLIST"}
              <ul class="checklist">
                {#each h.message.checklistItems as item (item.id)}
                  <li>
                    <input type="checkbox" disabled checked={item.checked} />
                    {item.text}
                  </li>
                {/each}
              </ul>
            {:else}
              <span>🖼️ Image</span>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page {
    padding: 1.5rem;
    height: 100%;
    max-width: 820px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  h1 { margin: 0 0 .25rem; }
  header p { margin: 0; }
  .search {
    padding: .65rem .85rem;
    border-radius: 12px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .search:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
  .list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: .65rem; }
  .hit { display: flex; flex-direction: column; gap: .5rem; }
  .chat-row {
    background: transparent;
    color: var(--on-surface);
    padding: 0;
    border-radius: 0;
    text-align: left;
    width: 100%;
  }
  .chat-row:hover { filter: none; opacity: .8; }
  .chat-row .spacer { flex: 1; text-align: right; }
  .content { padding-left: .5rem; }
  .center { text-align: center; padding: 2rem; }
  .checklist { list-style: none; margin: 0; padding: 0; }
</style>
