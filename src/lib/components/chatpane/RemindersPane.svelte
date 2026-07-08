<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChatSummary, type Message } from "../../ipc";
  import { chats, selectChat } from "../../stores/app";
  import Avatar from "../Avatar.svelte";
  import { formatFullTime } from "../../format";

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
    {:else}
      <section>
        <h2>Upcoming ({upcoming.length})</h2>
        {#if upcoming.length === 0}
          <div class="muted tiny center">Nothing scheduled.</div>
        {:else}
          <ul class="items divided-list">
            {#each upcoming as h (h.message.id)}
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
      </section>

      {#if past.length}
        <section>
          <h2>Past ({past.length})</h2>
          <ul class="items divided-list">
            {#each past as h (h.message.id)}
              <li>
                <button type="button" class="item past" onclick={() => selectChat(h.chat.id)}>
                  <Avatar name={h.chat.name} size={30} avatarFile={h.chat.avatarFile} />
                  <div class="col meta">
                    <strong class="name">{h.chat.name}</strong>
                    <span class="muted tiny preview">{preview(h.message)}</span>
                  </div>
                  <span class="chip tiny muted">{formatFullTime(h.message.reminderAt)}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </div>
</div>

<style>
  .pane { height: 100%; padding: .75rem; gap: .5rem; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding-right: .25rem; display: flex; flex-direction: column; gap: 1rem; }
  h2 { margin: 0 0 .5rem; font-size: .95rem; font-weight: 600; }
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
  .item.past { opacity: .7; }
  .meta { flex: 1; min-width: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .preview { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .center { text-align: center; padding: 1rem .5rem; }
</style>
