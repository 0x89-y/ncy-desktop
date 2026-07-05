<script lang="ts">
  import { activeChatId, visibleChats, selectChat, filterText, showArchived, allTags, activeTag, profileBuckets, activeProfileId, refreshChats, refreshActiveChat } from "../stores/app";
  import { api, type ChatSummary } from "../ipc";
  import ChatListItem from "./ChatListItem.svelte";

  let filter = $state("");
  $effect(() => {
    filterText.set(filter);
  });

  function setTag(t: string | null) {
    activeTag.set(t);
  }

  function setProfile(id: string | null) {
    activeProfileId.set(id);
  }

  // Create a chat locally — the desktop works with no phone at all.
  let creating = $state(false);
  let newName = $state("");
  let createBusy = $state(false);
  let createError = $state<string | null>(null);

  function startCreate() {
    creating = true;
    newName = "";
    createError = null;
  }

  function cancelCreate() {
    creating = false;
    newName = "";
    createError = null;
  }

  async function submitCreate() {
    const name = newName.trim();
    if (!name || createBusy) return;
    createBusy = true;
    createError = null;
    try {
      const id = await api.authorCreateChat(name);
      await refreshChats();
      await selectChat(id);
      creating = false;
      newName = "";
    } catch (e: any) {
      createError = typeof e === "string" ? e : (e?.message ?? "create failed");
    } finally {
      createBusy = false;
    }
  }

  // Right-click a chat row → the same actions as the chat header's ⋯ menu.
  let ctx = $state<{ chat: ChatSummary; x: number; y: number } | null>(null);
  let metaBusy = $state(false);
  // Inline rename / tags editor (mirrors ChatView's meta-editor).
  let editor = $state<{ chat: ChatSummary; mode: "rename" | "tags"; value: string } | null>(null);

  function openCtx(e: MouseEvent, chat: ChatSummary) {
    e.preventDefault();
    e.stopPropagation();
    // Menu is ~180×240; keep it on-screen.
    const x = Math.min(e.clientX, window.innerWidth - 190);
    const y = Math.min(e.clientY, window.innerHeight - 250);
    ctx = { chat, x, y };
  }
  function closeCtx() {
    ctx = null;
  }

  async function runMeta(
    chat: ChatSummary,
    patch: { name?: string; isPinned?: boolean; isArchived?: boolean; tags?: string[] },
  ) {
    if (metaBusy) return;
    metaBusy = true;
    try {
      await api.authorUpdateChatMeta(chat.id, patch);
      await refreshChats();
      if ($activeChatId === chat.id) await refreshActiveChat();
    } catch (e) {
      console.error("update chat meta failed", e);
    } finally {
      metaBusy = false;
    }
  }

  function ctxRename() {
    if (!ctx) return;
    editor = { chat: ctx.chat, mode: "rename", value: ctx.chat.name };
    closeCtx();
  }
  function ctxTags() {
    if (!ctx) return;
    editor = { chat: ctx.chat, mode: "tags", value: ctx.chat.tags.join(", ") };
    closeCtx();
  }
  async function ctxTogglePin() {
    if (!ctx) return;
    const c = ctx.chat;
    closeCtx();
    await runMeta(c, { isPinned: !c.isPinned });
  }
  async function ctxToggleArchive() {
    if (!ctx) return;
    const c = ctx.chat;
    closeCtx();
    await runMeta(c, { isArchived: !c.isArchived });
  }
  async function ctxDelete() {
    if (!ctx) return;
    const c = ctx.chat;
    closeCtx();
    if (
      !confirm(
        `Delete "${c.name}"? This removes it on every paired device and can't be undone.`,
      )
    )
      return;
    try {
      await api.authorDeleteChat(c.id);
      if ($activeChatId === c.id) await selectChat(null);
      await refreshChats();
    } catch (e) {
      console.error("delete chat failed", e);
    }
  }

  function cancelEditor() {
    editor = null;
  }
  async function saveEditor() {
    if (!editor) return;
    const c = editor.chat;
    const v = editor.value.trim();
    if (editor.mode === "rename") {
      if (!v) return;
      await runMeta(c, { name: v });
    } else {
      await runMeta(c, { tags: v.split(",").map((t) => t.trim()).filter(Boolean) });
    }
    editor = null;
  }

  // Protect / change-PIN, mirroring the chat header's ⋯ menu (#3).
  let pinEditor = $state<{ chat: ChatSummary; mode: "protect" | "change" } | null>(null);
  let pinOld = $state("");
  let pin1 = $state("");
  let pin2 = $state("");
  let pinBusy = $state(false);
  let pinError = $state<string | null>(null);

  function openPin(mode: "protect" | "change") {
    if (!ctx) return;
    pinEditor = { chat: ctx.chat, mode };
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
    closeCtx();
  }
  function cancelPin() {
    pinEditor = null;
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
  }
  async function savePin() {
    if (!pinEditor || pinBusy) return;
    const c = pinEditor.chat;
    if (!pin1) { pinError = "PIN cannot be empty"; return; }
    if (pin1 !== pin2) { pinError = "PINs don't match"; return; }
    if (pinEditor.mode === "change" && !pinOld) { pinError = "Enter the current PIN"; return; }
    pinBusy = true;
    pinError = null;
    try {
      if (pinEditor.mode === "protect") {
        await api.protectChat(c.id, pin1);
      } else {
        await api.changeChatPin(c.id, pinOld, pin1);
      }
      await refreshChats();
      if ($activeChatId === c.id) await refreshActiveChat();
      cancelPin();
    } catch (e: any) {
      pinError = typeof e === "string" ? e : (e?.message ?? "failed");
    } finally {
      pinBusy = false;
    }
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (ctx) closeCtx();
      else if (pinEditor) cancelPin();
      else if (editor) cancelEditor();
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="wrap col">
  {#if $profileBuckets.length > 1}
    <div class="row profiles">
      <button
        class="chip"
        class:active={$activeProfileId === null}
        onclick={() => setProfile(null)}
      >All</button>
      {#each $profileBuckets as p (p.id)}
        <button
          class="chip"
          class:active={$activeProfileId === p.id}
          onclick={() => setProfile(p.id)}
          title={p.name}
        >{p.name}</button>
      {/each}
    </div>
  {/if}

  <div class="row controls">
    <input
      type="search"
      placeholder="Search chats…"
      bind:value={filter}
    />
    <button
      class="new-btn"
      title="New chat"
      aria-label="New chat"
      onclick={startCreate}
    >+</button>
  </div>

  {#if creating}
    <div class="row create">
      <input
        type="text"
        placeholder="Chat name"
        bind:value={newName}
        disabled={createBusy}
        onkeydown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); void submitCreate(); }
          if (e.key === "Escape") { e.preventDefault(); cancelCreate(); }
        }}
      />
      <button class="primary" onclick={submitCreate} disabled={createBusy || !newName.trim()}>
        {createBusy ? "…" : "Create"}
      </button>
      <button class="text-btn" onclick={cancelCreate} disabled={createBusy}>Cancel</button>
    </div>
    {#if createError}
      <div class="row tiny create-error">{createError}</div>
    {/if}
  {/if}

  {#if editor}
    <div class="row create">
      <input
        type="text"
        placeholder={editor.mode === "rename" ? "Chat name" : "tag1, tag2, tag3"}
        bind:value={editor.value}
        disabled={metaBusy}
        onkeydown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); void saveEditor(); }
          if (e.key === "Escape") { e.preventDefault(); cancelEditor(); }
        }}
      />
      <button class="primary" onclick={saveEditor} disabled={metaBusy}>
        {metaBusy ? "…" : "Save"}
      </button>
      <button class="text-btn" onclick={cancelEditor} disabled={metaBusy}>Cancel</button>
    </div>
  {/if}

  {#if pinEditor}
    <div class="col create-pin">
      <strong class="tiny">
        {pinEditor.mode === "protect" ? `Protect "${pinEditor.chat.name}"` : `Change PIN — ${pinEditor.chat.name}`}
      </strong>
      {#if pinEditor.mode === "change"}
        <input
          type="password"
          placeholder="Current PIN"
          bind:value={pinOld}
          disabled={pinBusy}
          onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
        />
      {/if}
      <input
        type="password"
        placeholder="New PIN"
        bind:value={pin1}
        disabled={pinBusy}
        onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
      />
      <input
        type="password"
        placeholder="Confirm PIN"
        bind:value={pin2}
        disabled={pinBusy}
        onkeydown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); void savePin(); }
          if (e.key === "Escape") { e.preventDefault(); cancelPin(); }
        }}
      />
      {#if pinError}
        <span class="tiny create-error">{pinError}</span>
      {/if}
      <div class="row" style="gap:.4rem;">
        <button class="primary" onclick={savePin} disabled={pinBusy}>
          {pinBusy ? "…" : (pinEditor.mode === "protect" ? "Protect" : "Change")}
        </button>
        <button class="text-btn" onclick={cancelPin} disabled={pinBusy}>Cancel</button>
      </div>
    </div>
  {/if}
  <label class="row tiny muted toggle">
    <input type="checkbox" bind:checked={$showArchived} />
    Show archived
  </label>

  {#if $allTags.length}
    <div class="row tags">
      <button
        class="chip"
        class:active={$activeTag === null}
        onclick={() => setTag(null)}
      >all</button>
      {#each $allTags as t (t)}
        <button
          class="chip"
          class:active={$activeTag === t}
          onclick={() => setTag(t)}
        >#{t}</button>
      {/each}
    </div>
  {/if}

  <div class="list scroll">
    {#each $visibleChats as c (c.id)}
      <ChatListItem
        chat={c}
        selected={$activeChatId === c.id}
        onclick={() => selectChat(c.id)}
        oncontextmenu={(e) => openCtx(e, c)}
      />
    {:else}
      <div class="empty muted">No chats yet. Create one with +, or pair a phone to sync.</div>
    {/each}
  </div>
</div>

{#if ctx}
  <button
    type="button"
    class="ctx-backdrop"
    aria-label="Close menu"
    onclick={closeCtx}
    oncontextmenu={(e) => { e.preventDefault(); closeCtx(); }}
  ></button>
  <div class="ctx-menu" role="menu" style={`left:${ctx.x}px; top:${ctx.y}px;`}>
    <button type="button" onclick={ctxRename}>Rename…</button>
    <button type="button" onclick={ctxTogglePin}>{ctx.chat.isPinned ? "Unpin" : "Pin"}</button>
    <button type="button" onclick={ctxToggleArchive}>{ctx.chat.isArchived ? "Unarchive" : "Archive"}</button>
    <button type="button" onclick={ctxTags}>Edit tags…</button>
    {#if ctx.chat.isProtected}
      <button type="button" onclick={() => openPin("change")}>Change PIN…</button>
    {:else}
      <button type="button" onclick={() => openPin("protect")}>Protect chat…</button>
    {/if}
    <button type="button" class="danger" onclick={ctxDelete}>Delete chat</button>
  </div>
{/if}

<style>
  .wrap {
    height: 100%;
    padding: .75rem;
    gap: .5rem;
  }
  .controls {
    gap: .4rem;
    align-items: center;
  }
  .controls input {
    flex: 1;
  }
  .new-btn {
    flex-shrink: 0;
    width: 2rem;
    height: 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: transparent;
    color: inherit;
    font-size: 1.25rem;
    line-height: 1;
    cursor: pointer;
  }
  .new-btn:hover { background: var(--surface-2, var(--surface)); }
  .create {
    gap: .4rem;
    align-items: center;
    padding: 0 .25rem;
  }
  .create input {
    flex: 1;
    padding: .4rem .55rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .create .primary {
    padding: .4rem .8rem;
    border-radius: 999px;
    border: none;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .create .primary:disabled { opacity: .45; cursor: default; }
  .create .text-btn {
    background: transparent;
    border: none;
    color: var(--primary, currentColor);
    font: inherit;
    cursor: pointer;
  }
  .create-error {
    color: var(--error, #b00);
    padding: 0 .25rem;
  }
  .create-pin {
    gap: .4rem;
    padding: .55rem .6rem;
    margin: 0 .25rem;
    border: 1px solid var(--outline-variant);
    border-radius: 12px;
    background: var(--surface-2, var(--surface));
  }
  .create-pin input {
    padding: .4rem .55rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface);
    color: inherit;
    font: inherit;
  }
  .create-pin .primary {
    padding: .4rem .8rem;
    border-radius: 999px;
    border: none;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .create-pin .primary:disabled { opacity: .45; cursor: default; }
  .create-pin .text-btn {
    background: transparent;
    border: none;
    color: var(--primary, currentColor);
    font: inherit;
    cursor: pointer;
  }
  .toggle {
    padding: 0 .25rem;
    user-select: none;
  }
  .toggle input { margin: 0; }
  .tags {
    flex-wrap: wrap;
    gap: .25rem;
    padding: 0 .25rem .25rem;
  }
  .profiles {
    flex-wrap: wrap;
    gap: .25rem;
    padding: 0 .25rem .25rem;
    border-bottom: 1px solid var(--outline-variant);
    margin-bottom: .25rem;
  }
  .profiles .chip {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    cursor: pointer;
    border: 1px solid transparent;
  }
  .chip.active {
    background: var(--primary-container);
    color: var(--on-primary-container);
    border-color: var(--primary);
  }
  .list {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: .25rem;
    padding-right: .25rem;
  }
  .empty {
    padding: 2rem 1rem;
    text-align: center;
  }
  /* Backdrop is a <button> for a11y; it must NOT pick up the app's global
     accent-pill button styling on hover — pseudo-class specificity would beat
     a plain `.ctx-backdrop` rule, so override `:hover` explicitly too. */
  .ctx-backdrop,
  .ctx-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 40;
    border: none;
    padding: 0;
    margin: 0;
    background: transparent;
    border-radius: 0;
    box-shadow: none;
    transition: none;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 170px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--outline-variant);
    border-radius: 12px;
    box-shadow: var(--shadow, 0 6px 24px rgba(0, 0, 0, .25));
    overflow: hidden;
    padding: .25rem;
  }
  .ctx-menu button {
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: .5rem .7rem;
    border-radius: 8px;
    cursor: pointer;
  }
  .ctx-menu button:hover { background: var(--surface-2, var(--surface)); }
  .ctx-menu button.danger { color: var(--error, #b00); }
</style>
