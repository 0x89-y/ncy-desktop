<script lang="ts">
  import { requestConfirmation } from "../stores/confirm";
  import { activeChatId, visibleChats, selectChat, profileBuckets, activeProfileId, newChatRequest, refreshChats, refreshActiveChat } from "../stores/app";
  import { api, type ChatSummary } from "../ipc";
  import ChatListItem from "./ChatListItem.svelte";
  import SearchPane from "./chatpane/SearchPane.svelte";
  import { Plus } from "@lucide/svelte";

  let { query = "" }: { query?: string } = $props();
  const activeVisibleChats = $derived($visibleChats.filter((chat) => !chat.isArchived));
  const archivedVisibleChats = $derived($visibleChats.filter((chat) => chat.isArchived));



  // Create a chat locally — the desktop works with no phone at all.
  let creating = $state(false);
  let newName = $state("");
  let createProfileChoice = $state("");
  let createInput: HTMLInputElement | undefined = $state();

  let createBusy = $state(false);
  let createError = $state<string | null>(null);
  function startCreate() {
    creating = true;
    newName = "";

    const activeBucket = $profileBuckets.find((profile) => profile.id === $activeProfileId);
    createProfileChoice = activeBucket?.profileId ?? "";
    createError = null;
    // Focus the field so the row is usable straight from Ctrl+N without a click —
    // and so its Escape handler is reachable. The input mounts on the next tick.
    queueMicrotask(() => createInput?.focus());
  }

  // Ctrl+N from the Starred/Reminders tabs flips the pane and sets the request in
  // the same tick this component mounts, so the flag has to still be pending when
  // the effect first runs. Clearing it here keeps a later manual remount quiet.
  $effect(() => {
    if (!$newChatRequest) return;
    newChatRequest.set(false);
    startCreate();
  });

  function cancelCreate() {
    creating = false;
    newName = "";

    createProfileChoice = "";
    createError = null;
  }

  async function submitCreate() {
    const name = newName.trim();
    const profileName = $profileBuckets.find((profile) => profile.profileId === createProfileChoice)?.name ?? "";
    const profileId = createProfileChoice;
    if (!name || createBusy) return;
    createBusy = true;
    createError = null;
    try {
      const id = await api.authorCreateChat(name, profileId, profileName);
      await refreshChats();
      const createdBucket = $profileBuckets.find((profile) => profile.profileId === profileId);
      activeProfileId.set(createdBucket?.id ?? (profileId ? null : ""));
      await selectChat(id);
      creating = false;
      newName = "";

      createProfileChoice = "";
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
    const prefs = await api.getDesktopPrefs().catch(() => ({ askBeforeDeleting: true }));
    if (prefs.askBeforeDeleting &&
      !await requestConfirmation(
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
    if (e.key !== "Escape") return;
    // Only claim Escape when a layer is actually open — see ChatListPane.
    if (ctx) {
      e.stopImmediatePropagation();
      closeCtx();
    } else if (pinEditor) {
      e.stopImmediatePropagation();
      cancelPin();
    } else if (editor) {
      e.stopImmediatePropagation();
      cancelEditor();
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="wrap col">
  {#if creating}
    <div class="row create">
      <input
        type="text"
        placeholder="Chat name"
        bind:this={createInput}
        bind:value={newName}
        disabled={createBusy}
        onkeydown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); void submitCreate(); }
          if (e.key === "Escape") { e.preventDefault(); cancelCreate(); }
        }}
      />
      <select bind:value={createProfileChoice} disabled={createBusy} aria-label="Profile">
        <option value="">Default profile</option>
        {#each $profileBuckets.filter((profile) => profile.profileId) as profile (profile.id)}
          <option value={profile.profileId}>{profile.name}</option>
        {/each}
      </select>
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
  {#if query.trim()}
    <div class="search-results"><SearchPane {query} /></div>
  {:else}
  <div class="list scroll divided-list">
    {#each activeVisibleChats as c (c.id)}
      <ChatListItem
        chat={c}
        selected={$activeChatId === c.id}
        onclick={() => selectChat(c.id)}
        oncontextmenu={(e) => openCtx(e, c)}
      />
    {/each}
    {#if archivedVisibleChats.length}
      <div class="chat-section-label tiny muted">Archived chats</div>
      {#each archivedVisibleChats as c (c.id)}
        <ChatListItem
          chat={c}
          selected={$activeChatId === c.id}
          onclick={() => selectChat(c.id)}
          oncontextmenu={(e) => openCtx(e, c)}
        />
      {/each}
    {/if}
    {#if !activeVisibleChats.length && !archivedVisibleChats.length}
      <div class="empty muted">No chats yet. Create one with +, or pair a phone to sync.</div>
    {/if}
  </div>
  {/if}

  {#if !creating && !query.trim()}
    <button class="fab" title="New chat" aria-label="New chat" onclick={startCreate}>
      <Plus size={22} strokeWidth={2.5} />
    </button>
  {/if}
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
    <button type="button" onclick={ctxTogglePin}>{ctx.chat.isPinned ? "Unpin" : "Pin"}</button>
    <button type="button" onclick={ctxToggleArchive}>{ctx.chat.isArchived ? "Unarchive" : "Archive"}</button>
  </div>
{/if}

<style>
  .wrap {
    position: relative;
    height: 100%;
    padding: 0;
    gap: 0;
  }
  /* Floating "New chat" action — sits in the bottom-right corner of the list
     pane so it never consumes a row. Anchored to `.wrap` (which doesn't scroll;
     only the inner `.list` does), so it stays put over the list. */
  .fab {
    position: absolute;
    right: 1rem;
    bottom: 1rem;
    z-index: 5;
    width: 44px;
    height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--radius);
    box-shadow: var(--shadow, 0 4px 14px rgba(0, 0, 0, .3));
  }
  .create {
    gap: .4rem;
    align-items: center;
    flex-wrap: wrap;
    padding: 0 .25rem;
  }
  .create input,
  .create select {
    flex: 1 1 100%;
    min-width: 0;
    padding: .4rem .55rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .create .primary {
    padding: .4rem .8rem;
    border-radius: var(--radius);
    border: none;
    background: var(--accent);
    color: var(--accent-contrast);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .create .primary:disabled { opacity: .45; cursor: default; }
  .create .text-btn {
    background: transparent;
    border: none;
    color: var(--accent, currentColor);
    font: inherit;
    cursor: pointer;
  }
  .create-error {
    color: var(--danger);
    padding: 0 .25rem;
  }
  .create-pin {
    gap: .4rem;
    padding: .55rem .6rem;
    margin: 0 .25rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2, var(--surface));
  }
  .create-pin input {
    padding: .4rem .55rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    font: inherit;
  }
  .create-pin .primary {
    padding: .4rem .8rem;
    border-radius: var(--radius);
    border: none;
    background: var(--accent);
    color: var(--accent-contrast);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .create-pin .primary:disabled { opacity: .45; cursor: default; }
  .create-pin .text-btn {
    background: transparent;
    border: none;
    color: var(--accent, currentColor);
    font: inherit;
    cursor: pointer;
  }
  /* The outer ChatListPane supplies the frame; keep only the row dividers here. */
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    border: none;
    border-radius: 0;
  }
  .search-results { flex:1; min-height:0; overflow:hidden; }
  .chat-section-label { padding:.65rem .65rem .35rem; border-top:1px solid var(--border); font-weight:600; text-transform:uppercase; letter-spacing:.06em; }
  .list > :global(.row-btn:last-child) {
    border-bottom: 1px solid var(--border);
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
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: none;
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
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .ctx-menu button:hover { background: var(--surface-2, var(--surface)); }
</style>
