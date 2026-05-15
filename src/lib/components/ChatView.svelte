<script lang="ts">
  import { activeChat, refreshActiveChat, refreshChats } from "../stores/app";
  import { api, extractMediaHash } from "../ipc";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
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
    </header>

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
            disabled={sending || chat.metadata.isProtected}
            rows="4"
          ></textarea>
          <div class="row" style="justify-content:flex-end;">
            <button
              type="button"
              class="send"
              onclick={sendChecklist}
              disabled={sending || !checklistText.trim() || chat.metadata.isProtected}
            >
              {sending ? "…" : "Send checklist"}
            </button>
          </div>
        </div>
      {/if}
      <div class="toolbar">
        <button type="button" title="Bold (Ctrl+B)" disabled={sending || chat.metadata.isProtected} onclick={() => applyWrap("**")}><b>B</b></button>
        <button type="button" title="Italic (Ctrl+I)" disabled={sending || chat.metadata.isProtected} onclick={() => applyWrap("*")}><i>I</i></button>
        <button type="button" title="Strikethrough" disabled={sending || chat.metadata.isProtected} onclick={() => applyWrap("~~")}><s>S</s></button>
        <span class="sep"></span>
        <button type="button" title="Heading 1" disabled={sending || chat.metadata.isProtected} onclick={() => applyLinePrefix("# ")}>H1</button>
        <button type="button" title="Heading 2" disabled={sending || chat.metadata.isProtected} onclick={() => applyLinePrefix("## ")}>H2</button>
        <button type="button" title="Heading 3" disabled={sending || chat.metadata.isProtected} onclick={() => applyLinePrefix("### ")}>H3</button>
        <button type="button" title="Bullet list" disabled={sending || chat.metadata.isProtected} onclick={() => applyLinePrefix("- ")}>• List</button>
        <button type="button" title="Code block" disabled={sending || chat.metadata.isProtected} onclick={insertCodeBlock}>{"</>"}</button>
      </div>
      <div class="compose-row">
        <button
          type="button"
          class="attach"
          title="Attach image"
          onclick={attachImage}
          disabled={sending || chat.metadata.isProtected}
        >
          📎
        </button>
        <button
          type="button"
          class="attach"
          title="New checklist"
          onclick={() => { checklistOpen = !checklistOpen; }}
          disabled={sending || chat.metadata.isProtected}
        >
          📋
        </button>
        <textarea
          class="compose-input"
          placeholder="Type a message…  (Enter to send, Shift+Enter for newline)"
          bind:this={composeTextarea}
          bind:value={composeText}
          onkeydown={onKeyDown}
          disabled={sending || chat.metadata.isProtected}
          rows="1"
        ></textarea>
        <button
          type="button"
          class="send"
          onclick={send}
          disabled={sending || !composeText.trim() || chat.metadata.isProtected}
        >
          {sending ? "…" : "Send"}
        </button>
      </div>
      {#if chat.metadata.isProtected}
        <div class="compose-error tiny">
          View-only: authoring into protected chats from the desktop isn't supported
          yet — compose on the phone.
        </div>
      {/if}
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

<style>
  .wrap {
    height: 100%;
    background: var(--bg);
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
  .toolbar {
    display: flex;
    align-items: center;
    gap: .15rem;
    flex-wrap: wrap;
  }
  .toolbar button {
    padding: .15rem .45rem;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: .85rem;
    cursor: pointer;
    line-height: 1.2;
  }
  .toolbar button:hover:not(:disabled) {
    background: var(--surface-2, var(--surface));
    border-color: var(--outline-variant);
  }
  .toolbar button:disabled {
    opacity: .4;
    cursor: default;
  }
  .toolbar .sep {
    width: 1px;
    height: 1rem;
    background: var(--outline-variant);
    margin: 0 .25rem;
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
</style>
