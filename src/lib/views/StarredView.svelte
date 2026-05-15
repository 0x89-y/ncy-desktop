<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../ipc";
  import { chats, selectChat, activeView } from "../stores/app";
  import RichText from "../components/RichText.svelte";
  import Avatar from "../components/Avatar.svelte";
  import { formatFullTime } from "../format";

  type Hit = { chat: ChatSummary; message: Message };
  let hits = $state<Hit[]>([]);
  let loading = $state(true);

  async function load() {
    loading = true;
    const all: Hit[] = [];
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        if (m.isStarred) all.push({ chat: c, message: m });
      }
    }
    all.sort((a, b) => b.message.createdAt - a.message.createdAt);
    hits = all;
    loading = false;
  }

  onMount(load);

  async function open(chatId: string) {
    activeView.set("chats");
    await selectChat(chatId);
  }
</script>

<div class="page scroll">
  <header>
    <h1>⭐ Starred messages</h1>
    <p class="muted">All starred messages across every chat.</p>
  </header>

  {#if loading}
    <div class="muted center">Loading…</div>
  {:else if hits.length === 0}
    <div class="muted center">No starred messages yet.</div>
  {:else}
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
