<script lang="ts">
  import { activeChat, refreshActiveChat, refreshChats, selectChat } from "../stores/app";
  import { api, extractMediaHash } from "../ipc";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { writeText, readText } from "@tauri-apps/plugin-clipboard-manager";
  import { parseMarkdown, prefixSelectedLines, wrapSelection } from "../markdown";
  import Avatar from "./Avatar.svelte";
  import MessageBubble from "./MessageBubble.svelte";

  let scroller: HTMLDivElement | undefined = $state();
  let composeTextarea: HTMLTextAreaElement | undefined = $state();
  let lastCount = $state(0);
  let composeText = $state("");
  let sending = $state(false);
  let sendError = $state<string | null>(null);

  // Checklist composer: a small inline form. One item per line.
  let checklistOpen = $state(false);
  let checklistText = $state("");

  // Phase 2b polish: pull the chat's wallpaper blob if backgroundFile is content-addressed.
  // Legacy `background.jpg` references (pre-content-addressing) extract to null and are
  // silently skipped — older builds rendered them locally; they can't sync.
  let backgroundDataUrl = $state<string | null>(null);
  $effect(() => {
    const c = $activeChat;
    const hash = c ? extractMediaHash(c.metadata.backgroundFile) : null;
    backgroundDataUrl = null;
    if (!hash) return;
    api
      .readBlob(hash, "image")
      .then((b64) => {
        if (
          !b64 ||
          // Active chat may have changed while the request was in flight.
          extractMediaHash($activeChat?.metadata.backgroundFile) !== hash
        ) {
          return;
        }
        backgroundDataUrl = `data:image/jpeg;base64,${b64}`;
      })
      .catch(() => {});
  });

  async function sendChecklist() {
    const c = $activeChat;
    if (!c || sending) return;
    const items = checklistText
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
    if (items.length === 0) {
      sendError = "checklist has no items";
      return;
    }
    sending = true;
    sendError = null;
    try {
      await api.authorSendChecklist(c.metadata.id, items);
      checklistText = "";
      checklistOpen = false;
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "checklist send failed");
    } finally {
      sending = false;
    }
  }

  $effect(() => {
    const c = $activeChat;
    if (!c || !scroller) return;
    const count = c.messages.length;
    if (count !== lastCount) {
      queueMicrotask(() => {
        if (scroller) scroller.scrollTop = scroller.scrollHeight;
      });
      lastCount = count;
    }
  });

  async function send() {
    const c = $activeChat;
    if (!c) return;
    if (!composeText.trim() || sending) return;
    const blocks = parseMarkdown(composeText);
    if (blocks.length === 0) return;
    sending = true;
    sendError = null;
    try {
      await api.authorSendMessage(c.metadata.id, blocks);
      composeText = "";
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "send failed");
    } finally {
      sending = false;
    }
  }

  function applyWrap(prefix: string, suffix: string = prefix) {
    if (!composeTextarea) return;
    const next = wrapSelection(composeTextarea, prefix, suffix);
    composeText = next.value;
    // Restore selection after Svelte re-renders the textarea.
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  function applyLinePrefix(prefix: string) {
    if (!composeTextarea) return;
    const next = prefixSelectedLines(composeTextarea, prefix);
    composeText = next.value;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  function insertCodeBlock() {
    if (!composeTextarea) return;
    // Multi-line wrap: inject ``` on its own lines around the selection.
    const next = wrapSelection(composeTextarea, "```\n", "\n```");
    composeText = next.value;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(next.selectionStart, next.selectionEnd);
    });
  }

  // ----- Compose right-click menu: formatting + clipboard -----
  // Replaces the native menu in the compose box. Selection is snapshotted on
  // open (it survives the right-click but the menu would steal focus).
  let composeMenu = $state<{ x: number; y: number; selStart: number; selEnd: number } | null>(null);
  let composeMenuEl: HTMLDivElement | undefined = $state();

  function openComposeMenu(e: MouseEvent) {
    if (sending || !composeTextarea) return; // disabled → fall back to native
    e.preventDefault();
    const ta = composeTextarea;
    const selStart = ta.selectionStart ?? 0;
    const selEnd = ta.selectionEnd ?? selStart;
    // Raw click point; the $effect below measures the rendered menu and clamps
    // it fully on-screen (the compose box sits at the bottom, so it usually
    // needs to open upward).
    composeMenu = { x: e.clientX, y: e.clientY, selStart, selEnd };
  }

  // Keep the menu inside the viewport once it has a real size.
  $effect(() => {
    const m = composeMenu;
    const el = composeMenuEl;
    if (!m || !el) return;
    const pad = 8;
    const r = el.getBoundingClientRect();
    const left = Math.max(pad, Math.min(m.x, window.innerWidth - r.width - pad));
    const top = Math.max(pad, Math.min(m.y, window.innerHeight - r.height - pad));
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  });
  function closeComposeMenu() {
    composeMenu = null;
  }

  function restoreSelection() {
    const m = composeMenu;
    if (!m || !composeTextarea) return;
    composeTextarea.focus();
    composeTextarea.setSelectionRange(m.selStart, m.selEnd);
  }

  // Formatting items: restore the snapshot selection, then reuse the existing
  // apply* helpers (they read + re-set the selection themselves).
  function menuFormat(fn: () => void) {
    restoreSelection();
    fn();
    closeComposeMenu();
  }

  async function menuCopy() {
    const m = composeMenu;
    if (!m) return;
    const text = composeText.slice(m.selStart, m.selEnd);
    closeComposeMenu();
    if (!text) return;
    try {
      await writeText(text);
    } catch (e) {
      console.error("copy failed", e);
    }
  }

  async function menuCut() {
    const m = composeMenu;
    if (!m) return;
    const { selStart, selEnd } = m;
    const text = composeText.slice(selStart, selEnd);
    closeComposeMenu();
    if (!text) return;
    try {
      await writeText(text);
    } catch (e) {
      console.error("cut failed", e);
      return;
    }
    composeText = composeText.slice(0, selStart) + composeText.slice(selEnd);
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(selStart, selStart);
    });
  }

  async function menuPaste() {
    const m = composeMenu;
    if (!m) return;
    const { selStart, selEnd } = m;
    closeComposeMenu();
    let clip = "";
    try {
      clip = (await readText()) ?? "";
    } catch (e) {
      console.error("paste failed", e);
      return;
    }
    if (!clip) return;
    composeText = composeText.slice(0, selStart) + clip + composeText.slice(selEnd);
    const caret = selStart + clip.length;
    queueMicrotask(() => {
      if (!composeTextarea) return;
      composeTextarea.focus();
      composeTextarea.setSelectionRange(caret, caret);
    });
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape" && composeMenu) {
      e.preventDefault();
      closeComposeMenu();
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    // Ctrl/Cmd-B / -I — standard formatting shortcuts.
    const mod = e.ctrlKey || e.metaKey;
    if (mod && (e.key === "b" || e.key === "B")) {
      e.preventDefault();
      applyWrap("**");
      return;
    }
    if (mod && (e.key === "i" || e.key === "I")) {
      e.preventDefault();
      applyWrap("*");
      return;
    }
    // Enter sends; Shift+Enter inserts a newline.
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  }

  // ----- Phase 3: protected-chat unlock -----
  let pinInput = $state("");
  let unlocking = $state(false);
  let unlockError = $state<string | null>(null);

  async function unlock() {
    const c = $activeChat;
    if (!c || unlocking || !pinInput.trim()) return;
    unlocking = true;
    unlockError = null;
    try {
      const ok = await api.unlockChat(c.metadata.id, pinInput);
      if (ok) {
        pinInput = "";
        await refreshActiveChat();
        await refreshChats();
      } else {
        unlockError = "Wrong PIN";
        pinInput = "";
      }
    } catch (e: any) {
      unlockError = typeof e === "string" ? e : (e?.message ?? "unlock failed");
    } finally {
      unlocking = false;
    }
  }

  async function lock() {
    const c = $activeChat;
    if (!c) return;
    await api.lockChat(c.metadata.id);
    await refreshActiveChat();
    await refreshChats();
  }

  // ----- Chat management (rename / pin / archive / tags / delete) -----
  let menuOpen = $state(false);
  // null = menu only; "rename"/"tags" = an inline editor is showing.
  let editMode = $state<null | "rename" | "tags">(null);
  let editValue = $state("");
  let metaBusy = $state(false);

  function openEditor(mode: "rename" | "tags") {
    const c = $activeChat;
    if (!c) return;
    editValue =
      mode === "rename"
        ? c.metadata.name
        : c.metadata.tags.join(", ");
    editMode = mode;
    menuOpen = false;
  }

  function cancelEdit() {
    editMode = null;
    editValue = "";
  }

  async function runMeta(patch: {
    name?: string;
    isPinned?: boolean;
    isArchived?: boolean;
    tags?: string[];
  }) {
    const c = $activeChat;
    if (!c || metaBusy) return;
    metaBusy = true;
    try {
      await api.authorUpdateChatMeta(c.metadata.id, patch);
      await refreshActiveChat();
      await refreshChats();
    } catch (e) {
      console.error("update chat meta failed", e);
    } finally {
      metaBusy = false;
    }
  }

  // ----- Protect chat / change PIN (#3) -----
  let pinMode = $state<null | "protect" | "change">(null);
  let pinOld = $state("");
  let pin1 = $state("");
  let pin2 = $state("");
  let pinBusy = $state(false);
  let pinError = $state<string | null>(null);

  function openPin(mode: "protect" | "change") {
    pinMode = mode;
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
    menuOpen = false;
  }
  function cancelPin() {
    pinMode = null;
    pinOld = "";
    pin1 = "";
    pin2 = "";
    pinError = null;
  }
  async function savePin() {
    const c = $activeChat;
    if (!c || pinBusy) return;
    if (!pin1) { pinError = "PIN cannot be empty"; return; }
    if (pin1 !== pin2) { pinError = "PINs don't match"; return; }
    if (pinMode === "change" && !pinOld) { pinError = "Enter the current PIN"; return; }
    pinBusy = true;
    pinError = null;
    try {
      if (pinMode === "protect") {
        await api.protectChat(c.metadata.id, pin1);
      } else {
        await api.changeChatPin(c.metadata.id, pinOld, pin1);
      }
      await refreshActiveChat();
      await refreshChats();
      cancelPin();
    } catch (e: any) {
      pinError = typeof e === "string" ? e : (e?.message ?? "failed");
    } finally {
      pinBusy = false;
    }
  }

  async function saveEditor() {
    const v = editValue.trim();
    if (editMode === "rename") {
      if (!v) return;
      await runMeta({ name: v });
    } else if (editMode === "tags") {
      await runMeta({ tags: v.split(",").map((t) => t.trim()).filter(Boolean) });
    }
    cancelEdit();
  }

  async function togglePin() {
    const c = $activeChat;
    if (!c) return;
    menuOpen = false;
    await runMeta({ isPinned: !c.metadata.isPinned });
  }

  async function toggleArchive() {
    const c = $activeChat;
    if (!c) return;
    menuOpen = false;
    await runMeta({ isArchived: !c.metadata.isArchived });
  }

  async function deleteChat() {
    const c = $activeChat;
    if (!c) return;
    menuOpen = false;
    if (
      !confirm(
        `Delete "${c.metadata.name}"? This removes it on every paired device and can't be undone.`,
      )
    )
      return;
    try {
      await api.authorDeleteChat(c.metadata.id);
      await selectChat(null);
      await refreshChats();
    } catch (e) {
      console.error("delete chat failed", e);
    }
  }

  async function attachImage() {
    const c = $activeChat;
    if (!c || sending) return;
    let picked: string | string[] | null;
    try {
      picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          { name: "Images", extensions: ["jpg", "jpeg", "png", "gif", "webp", "bmp"] },
        ],
      });
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "could not open picker");
      return;
    }
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    sending = true;
    sendError = null;
    try {
      await api.authorSendImage(c.metadata.id, path);
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "image send failed");
    } finally {
      sending = false;
    }
  }
</script>

{#if $activeChat}
  {@const chat = $activeChat}
  <div class="wrap col">
    <header class="row header">
      <Avatar
        name={chat.metadata.name}
        size={40}
        avatarFile={chat.metadata.avatarFile}
      />
      <div class="col title-block">
        <div class="title">
          {#if chat.metadata.isPinned}<span class="muted tiny">📌</span>{/if}
          {#if chat.metadata.isProtected}<span class="muted tiny">🔒</span>{/if}
          {chat.metadata.name}
        </div>
        <div class="muted tiny">
          {chat.messages.length} message{chat.messages.length === 1 ? "" : "s"}
          {#if chat.metadata.tags.length} ·
            {#each chat.metadata.tags as t (t)}<span class="chip tiny">#{t}</span>{/each}
          {/if}
        </div>
      </div>
      {#if chat.metadata.isProtected && chat.isUnlocked}
        <button type="button" class="lock-btn" title="Lock this chat" onclick={lock}>
          🔓 Lock
        </button>
      {/if}
      <div class="menu-wrap">
        <button
          type="button"
          class="menu-btn"
          title="Chat options"
          onclick={() => { menuOpen = !menuOpen; }}
        >⋯</button>
        {#if menuOpen}
          <div class="menu" role="menu">
            <button type="button" onclick={() => openEditor("rename")}>Rename…</button>
            <button type="button" onclick={togglePin}>
              {chat.metadata.isPinned ? "Unpin" : "Pin"}
            </button>
            <button type="button" onclick={toggleArchive}>
              {chat.metadata.isArchived ? "Unarchive" : "Archive"}
            </button>
            <button type="button" onclick={() => openEditor("tags")}>Edit tags…</button>
            {#if chat.metadata.isProtected}
              <button type="button" onclick={() => openPin("change")}>Change PIN…</button>
            {:else}
              <button type="button" onclick={() => openPin("protect")}>Protect chat…</button>
            {/if}
            <button type="button" class="danger" onclick={deleteChat}>Delete chat</button>
          </div>
        {/if}
      </div>
    </header>

    {#if editMode}
      <div class="meta-editor">
        <input
          class="meta-input"
          placeholder={editMode === "rename" ? "Chat name" : "tag1, tag2, tag3"}
          bind:value={editValue}
          onkeydown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); void saveEditor(); }
            if (e.key === "Escape") { e.preventDefault(); cancelEdit(); }
          }}
          disabled={metaBusy}
        />
        <button type="button" class="send" onclick={saveEditor} disabled={metaBusy}>
          {metaBusy ? "…" : "Save"}
        </button>
        <button type="button" class="text-btn" onclick={cancelEdit}>Cancel</button>
      </div>
    {/if}

    {#if pinMode}
      <div class="meta-editor pin-editor">
        {#if pinMode === "change"}
          <input
            class="meta-input"
            type="password"
            placeholder="Current PIN"
            bind:value={pinOld}
            disabled={pinBusy}
            onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
          />
        {/if}
        <input
          class="meta-input"
          type="password"
          placeholder="New PIN"
          bind:value={pin1}
          disabled={pinBusy}
          onkeydown={(e) => { if (e.key === "Escape") { e.preventDefault(); cancelPin(); } }}
        />
        <input
          class="meta-input"
          type="password"
          placeholder="Confirm PIN"
          bind:value={pin2}
          disabled={pinBusy}
          onkeydown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); void savePin(); }
            if (e.key === "Escape") { e.preventDefault(); cancelPin(); }
          }}
        />
        <button type="button" class="send" onclick={savePin} disabled={pinBusy}>
          {pinBusy ? "…" : (pinMode === "protect" ? "Protect" : "Change")}
        </button>
        <button type="button" class="text-btn" onclick={cancelPin}>Cancel</button>
      </div>
      {#if pinError}
        <div class="meta-editor"><span class="compose-error tiny">{pinError}</span></div>
      {/if}
    {/if}

    {#if chat.metadata.isProtected && !chat.isUnlocked}
      <div class="lock-screen">
        <div class="lock-card">
          <div class="lock-icon">🔒</div>
          <h2>{chat.metadata.hideName ? "Protected chat" : chat.metadata.name}</h2>
          <p class="muted">
            Enter the chat PIN to decrypt and view its messages on this device.
          </p>
          <input
            type="password"
            class="pin-input"
            placeholder="PIN"
            bind:value={pinInput}
            onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); void unlock(); } }}
            disabled={unlocking}
          />
          {#if unlockError}
            <div class="compose-error tiny">{unlockError}</div>
          {/if}
          <button
            type="button"
            class="send"
            onclick={unlock}
            disabled={unlocking || !pinInput.trim()}
          >
            {unlocking ? "…" : "Unlock"}
          </button>
        </div>
      </div>
    {:else}
    <div
      class="messages scroll"
      class:has-background={!!backgroundDataUrl}
      style={backgroundDataUrl ? `background-image:url(${backgroundDataUrl});` : ""}
      bind:this={scroller}
    >
      {#each chat.messages as m (m.id)}
        <MessageBubble
          message={m}
          bubbleColor={chat.metadata.bubbleColor}
          bubbleGradient={chat.metadata.bubbleGradient}
        />
      {:else}
        <div class="empty muted">No messages yet.</div>
      {/each}
    </div>

    <footer class="compose">
      {#if sendError}
        <div class="compose-error tiny">{sendError}</div>
      {/if}
      {#if checklistOpen}
        <div class="checklist-composer">
          <div class="row" style="justify-content:space-between;">
            <strong class="tiny">📋 New checklist</strong>
            <button type="button" class="text-btn" onclick={() => { checklistOpen = false; checklistText = ""; }}>
              Cancel
            </button>
          </div>
          <textarea
            class="compose-input"
            placeholder="One item per line"
            bind:value={checklistText}
            disabled={sending}
            rows="4"
          ></textarea>
          <div class="row" style="justify-content:flex-end;">
            <button
              type="button"
              class="send"
              onclick={sendChecklist}
              disabled={sending || !checklistText.trim()}
            >
              {sending ? "…" : "Send checklist"}
            </button>
          </div>
        </div>
      {/if}
      <div class="compose-row">
        <button
          type="button"
          class="attach"
          title="Attach image"
          onclick={attachImage}
          disabled={sending}
        >
          📎
        </button>
        <button
          type="button"
          class="attach"
          title="New checklist"
          onclick={() => { checklistOpen = !checklistOpen; }}
          disabled={sending}
        >
          📋
        </button>
        <textarea
          class="compose-input"
          placeholder="Type a message…"
          bind:this={composeTextarea}
          bind:value={composeText}
          onkeydown={onKeyDown}
          oncontextmenu={openComposeMenu}
          disabled={sending}
          rows="1"
        ></textarea>
        <button
          type="button"
          class="send"
          onclick={send}
          disabled={sending || !composeText.trim()}
        >
          {sending ? "…" : "Send"}
        </button>
      </div>
    </footer>
    {/if}
  </div>
{:else}
  <div class="placeholder muted">
    <div>
      <div style="font-size:3rem;line-height:1">💬</div>
      <p>Select a chat to view its messages.</p>
    </div>
  </div>
{/if}

<svelte:window onkeydown={onWindowKey} />

{#if composeMenu}
  {@const hasSel = composeMenu.selStart !== composeMenu.selEnd}
  <button
    type="button"
    class="ctx-backdrop"
    aria-label="Close menu"
    onclick={closeComposeMenu}
    oncontextmenu={(e) => { e.preventDefault(); closeComposeMenu(); }}
  ></button>
  <div
    class="ctx-menu"
    role="menu"
    bind:this={composeMenuEl}
    style={`left:${composeMenu.x}px; top:${composeMenu.y}px;`}
  >
    <button type="button" disabled={!hasSel} onclick={menuCut}>Cut</button>
    <button type="button" disabled={!hasSel} onclick={menuCopy}>Copy</button>
    <button type="button" onclick={menuPaste}>Paste</button>
    <div class="ctx-sep"></div>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyWrap("**"))}><b>Bold</b></button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyWrap("*"))}><i>Italic</i></button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyWrap("~~"))}><s>Strikethrough</s></button>
    <div class="ctx-sep"></div>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyLinePrefix("# "))}>Heading 1</button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyLinePrefix("## "))}>Heading 2</button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyLinePrefix("### "))}>Heading 3</button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(() => applyLinePrefix("- "))}>Bullet list</button>
    <button type="button" disabled={!hasSel} onclick={() => menuFormat(insertCodeBlock)}>Code block</button>
  </div>
{/if}

<style>
  .wrap {
    height: 100%;
    /* Transparent so the body dot-grid shows behind bubbles, mirroring Acy's
       "nav over grid, content shows grid" composition. */
    background: transparent;
  }
  .header {
    padding: .9rem 1rem;
    background: var(--surface);
    border-bottom: 1px solid var(--outline-variant);
    gap: .75rem;
  }
  .title-block {
    flex: 1;
    min-width: 0;
    gap: .15rem;
  }
  .title {
    font-weight: 700;
    font-size: 1.05rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .messages {
    flex: 1;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    background-size: cover;
    background-position: center;
    background-repeat: no-repeat;
  }
  /* Dim the wallpaper slightly so bubbles stay readable. The bubble's own
     box-shadow + colored background does most of the work; this is belt-and-suspenders. */
  .messages.has-background::before {
    content: "";
    position: absolute;
    inset: 0;
    background: color-mix(in oklab, var(--bg) 35%, transparent);
    pointer-events: none;
    z-index: 0;
  }
  .messages.has-background { position: relative; }
  .messages.has-background > :global(*) { position: relative; z-index: 1; }
  .empty {
    margin: auto;
    text-align: center;
    padding: 2rem;
  }
  .placeholder {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .compose {
    border-top: 1px solid var(--outline-variant);
    background: var(--surface);
    padding: .55rem .75rem .65rem;
    display: flex;
    flex-direction: column;
    gap: .35rem;
  }
  .compose-row {
    display: flex;
    align-items: flex-end;
    gap: .5rem;
  }
  .compose-input {
    flex: 1;
    min-height: 2.25rem;
    max-height: 9rem;
    resize: vertical;
    padding: .5rem .65rem;
    border-radius: 12px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
    line-height: 1.3;
  }
  .compose-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
  .send {
    padding: .5rem .9rem;
    border-radius: 999px;
    border: none;
    background: var(--accent, var(--bubble-mine));
    color: var(--accent-on, var(--bubble-mine-on));
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .send:disabled {
    opacity: .45;
    cursor: default;
  }
  .attach {
    padding: .4rem .55rem;
    border-radius: 12px;
    border: 1px solid var(--outline-variant);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 1.05rem;
    cursor: pointer;
    line-height: 1;
  }
  .attach:hover:not(:disabled) {
    background: var(--surface-2, var(--surface));
  }
  .attach:disabled {
    opacity: .45;
    cursor: default;
  }
  /* See ChatList.svelte — must beat `button:hover` from app.css. */
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
    padding: .25rem;
    box-sizing: border-box;
    /* Safety net if the menu is ever taller than the window. */
    max-height: calc(100vh - 1rem);
    overflow-y: auto;
  }
  .ctx-menu button {
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: .45rem .7rem;
    border-radius: 8px;
    cursor: pointer;
  }
  .ctx-menu button:hover:not(:disabled) {
    background: var(--surface-2, var(--surface));
  }
  .ctx-menu button:disabled {
    opacity: .4;
    cursor: default;
  }
  .ctx-sep {
    height: 1px;
    background: var(--outline-variant);
    margin: .25rem .35rem;
  }
  .compose-error {
    color: var(--error, #b00);
  }
  .checklist-composer {
    display: flex;
    flex-direction: column;
    gap: .35rem;
    padding: .55rem .65rem;
    border: 1px solid var(--outline-variant);
    border-radius: 12px;
    background: var(--surface-2, var(--surface));
  }
  .text-btn {
    background: transparent;
    border: none;
    color: var(--primary, currentColor);
    font: inherit;
    cursor: pointer;
    padding: 0;
  }
  .lock-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .lock-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: .65rem;
    max-width: 320px;
    text-align: center;
    padding: 1.75rem;
    border: 1px solid var(--outline-variant);
    border-radius: 16px;
    background: var(--surface);
  }
  .lock-card h2 { margin: 0; font-size: 1.1rem; }
  .lock-card p { margin: 0; }
  .lock-icon { font-size: 2.5rem; line-height: 1; }
  .pin-input {
    width: 100%;
    padding: .55rem .7rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
    text-align: center;
    letter-spacing: .25em;
  }
  .pin-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
  .lock-btn {
    background: transparent;
    border: 1px solid var(--outline-variant);
    color: inherit;
    font: inherit;
    font-size: .85rem;
    padding: .35rem .7rem;
    border-radius: 999px;
    cursor: pointer;
    align-self: center;
  }
  .lock-btn:hover { background: var(--surface-2, var(--surface)); }
  .menu-wrap { position: relative; }
  .menu-btn {
    background: transparent;
    border: 1px solid var(--outline-variant);
    color: inherit;
    font: inherit;
    font-size: 1.1rem;
    line-height: 1;
    padding: .25rem .55rem;
    border-radius: 999px;
    cursor: pointer;
  }
  .menu-btn:hover { background: var(--surface-2, var(--surface)); }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + .35rem);
    z-index: 20;
    min-width: 160px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--outline-variant);
    border-radius: 12px;
    box-shadow: var(--shadow, 0 6px 24px rgba(0,0,0,.25));
    overflow: hidden;
  }
  .menu button {
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: .55rem .8rem;
    cursor: pointer;
  }
  .menu button:hover { background: var(--surface-2, var(--surface)); }
  .menu button.danger { color: var(--error, #b00); }
  .meta-editor {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .55rem .75rem;
    border-bottom: 1px solid var(--outline-variant);
    background: var(--surface);
  }
  .pin-editor { flex-wrap: wrap; }
  .meta-input {
    flex: 1;
    padding: .45rem .6rem;
    border-radius: 10px;
    border: 1px solid var(--outline-variant);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .meta-input:focus {
    outline: 2px solid color-mix(in oklab, var(--accent, currentColor) 35%, transparent);
    outline-offset: 0;
  }
</style>
