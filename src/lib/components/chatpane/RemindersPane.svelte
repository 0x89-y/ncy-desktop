<script lang="ts">
  import { api, type ChatSummary, type Message } from "../../ipc";
  import { chats, lastSyncEvent, selectChat } from "../../stores/app";
  import Avatar from "../Avatar.svelte";
  import { formatFullTime } from "../../format";

  type Hit = { chat: ChatSummary; message: Message };
  let reminders = $state<Hit[]>([]);
  let loading = $state(true);
  let loadRun = 0;

  async function load(run: number) {
    loading = true;
    const now = Date.now();
    const hits: Hit[] = [];
    for (const c of $chats) {
      const data = await api.getChat(c.id);
      if (!data) continue;
      for (const m of data.messages) {
        if (m.reminderAt && m.reminderAt >= now) hits.push({ chat: c, message: m });
      }
    }
    hits.sort((a, b) => a.message.reminderAt! - b.message.reminderAt!);
    if (run !== loadRun) return;
    reminders = hits;
    loading = false;
  }

  $effect(() => {
    $chats;
    $lastSyncEvent;
    const run = ++loadRun;
    void load(run);
  });

  function preview(m: Message): string {
    if (m.type === "CHECKLIST") {
      const done = m.checklistItems.filter((i) => i.checked).length;
      return `${done}/${m.checklistItems.length} ${m.checklistItems[0]?.text ?? ""}`;
    }
    if (m.type === "VOICE") return "Voice message";
    if (m.type === "IMAGE") return "Image";
    return m.content
      .flatMap((b) => b.spans.map((s) => s.text))
      .join("")
      .slice(0, 120);
  }
</script>

<div class="pane col">
  <div class="list scroll">
    {#if loading}
      <div class="muted center tiny">Loading…</div>
    {:else if reminders.length === 0}
      <div class="muted center tiny">No reminders</div>
    {:else}
      <ul class="items divided-list">
        {#each reminders as h (h.message.id)}
          <li>
            <button type="button" class="item" onclick={() => selectChat(h.chat.id)}>
              <Avatar name={h.chat.name} size={30} avatarFile={h.chat.avatarFile} />
              <div class="col meta">
                <strong class="name">{h.chat.name}</strong>
                <span class="muted tiny preview">{preview(h.message)}</span>
              </div>
              <span class="chip tiny">{formatFullTime(h.message.reminderAt)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; padding: .75rem; gap: .5rem; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding-right: .25rem; display: flex; flex-direction: column; gap: 1rem; }
  .items { list-style: none; margin: 0; padding: 0; }
  .item {
    display: flex;
    align-items: center;
    gap: .55rem;
    width: 100%;
    text-align: left;
    cursor: pointer;
    padding: .5rem .6rem;
    background: var(--surface);
    color: inherit;
  }
  .item:hover { background: var(--surface-2); filter: none; }
  .meta { flex: 1; min-width: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .preview { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .center { text-align: center; padding: 1.5rem .5rem; }
</style>
