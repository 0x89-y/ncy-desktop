<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../../ipc";
  import { chats, selectChat } from "../../stores/app";
  import HitRow from "./HitRow.svelte";

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
</script>

<div class="pane col">
  <div class="list scroll">
    {#if loading}
      <div class="muted center tiny">Loading…</div>
    {:else if hits.length === 0}
      <div class="muted center tiny">No starred messages yet.</div>
    {:else}
      <ul class="hits divided-list">
        {#each hits as h (h.message.id)}
          <HitRow chat={h.chat} message={h.message} onopen={selectChat} />
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; padding: .75rem; gap: .5rem; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding-right: .25rem; }
  .hits { list-style: none; margin: 0; padding: 0; }
  .center { text-align: center; padding: 1.5rem .5rem; }
</style>
