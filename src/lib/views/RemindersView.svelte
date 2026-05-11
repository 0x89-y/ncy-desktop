<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../ipc";
  import { chats, selectChat, activeView } from "../stores/app";
  import Avatar from "../components/Avatar.svelte";
  import { formatFullTime } from "../format";

  type Hit = { chat: ChatSummary; message: Message };
  let upcoming = $state<Hit[]>([]);
  let past = $state<Hit[]>([]);
  let loading = $state(true);

  async function load() {
    loading = true;
    const now = Date.now();
    const u: Hit[] = [];
    const p: Hit[] = [];
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        if (!m.reminderAt) continue;
        if (m.reminderAt >= now) u.push({ chat: c, message: m });
        else p.push({ chat: c, message: m });
      }
    }
    u.sort((a, b) => a.message.reminderAt! - b.message.reminderAt!);
    p.sort((a, b) => b.message.reminderAt! - a.message.reminderAt!);
    upcoming = u;
    past = p;
    loading = false;
  }

  onMount(load);

  async function open(chatId: string) {
    activeView.set("chats");
    await selectChat(chatId);
  }

  function preview(m: Message): string {
    if (m.type === "CHECKLIST") {
      const done = m.checklistItems.filter((i) => i.checked).length;
      return `${done}/${m.checklistItems.length} ${m.checklistItems[0]?.text ?? ""}`;
    }
    if (m.type === "VOICE") return "🎙️ Voice";
    if (m.type === "IMAGE") return "🖼️ Image";
    return m.content
      .flatMap((b) => b.spans.map((s) => s.text))
      .join("")
      .slice(0, 120);
  }
</script>

<div class="page scroll">
  <header>
    <h1>⏰ Reminders</h1>
    <p class="muted">Messages with a reminder set on the phone.</p>
  </header>

  {#if loading}
    <div class="muted center">Loading…</div>
  {:else}
    <section>
      <h2>Upcoming ({upcoming.length})</h2>
      {#if upcoming.length === 0}
        <div class="muted">Nothing scheduled.</div>
      {:else}
        <ul class="list">
          {#each upcoming as h (h.message.id)}
            <li>
              <button type="button" class="card row item" onclick={() => open(h.chat.id)}>
                <Avatar name={h.chat.name} size={32} />
                <div class="col" style="flex:1; min-width:0;">
                  <strong>{h.chat.name}</strong>
                  <span class="muted tiny preview">{preview(h.message)}</span>
                </div>
                <span class="chip">{formatFullTime(h.message.reminderAt)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if past.length}
      <section>
        <h2>Past ({past.length})</h2>
        <ul class="list">
          {#each past as h (h.message.id)}
            <li>
              <button type="button" class="card row item past" onclick={() => open(h.chat.id)}>
                <Avatar name={h.chat.name} size={32} />
                <div class="col" style="flex:1; min-width:0;">
                  <strong>{h.chat.name}</strong>
                  <span class="muted tiny preview">{preview(h.message)}</span>
                </div>
                <span class="chip muted">{formatFullTime(h.message.reminderAt)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
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
  h2 { margin: .5rem 0; font-size: 1rem; font-weight: 600; }
  header p { margin: 0; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .5rem; }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .past { opacity: .7; }
  .center { text-align: center; padding: 2rem; }
  .item {
    cursor: pointer;
    background: var(--surface);
    color: var(--on-surface);
    width: 100%;
    text-align: left;
    border-radius: var(--radius-lg);
    border: 1px solid var(--outline-variant);
  }
  .item:hover { background: var(--surface-2); filter: none; }
</style>
