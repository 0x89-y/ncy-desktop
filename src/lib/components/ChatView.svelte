<script lang="ts">
  import { activeChat } from "../stores/app";
  import { api } from "../ipc";
  import Avatar from "./Avatar.svelte";
  import MessageBubble from "./MessageBubble.svelte";

  let scroller: HTMLDivElement | undefined = $state();
  let lastCount = $state(0);
  let composeText = $state("");
  let sending = $state(false);
  let sendError = $state<string | null>(null);

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
    const text = composeText.trim();
    if (!text || sending) return;
    sending = true;
    sendError = null;
    try {
      await api.authorSendText(c.metadata.id, text);
      composeText = "";
    } catch (e: any) {
      sendError = typeof e === "string" ? e : (e?.message ?? "send failed");
    } finally {
      sending = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    // Enter sends; Shift+Enter inserts a newline.
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  }
</script>

{#if $activeChat}
  {@const chat = $activeChat}
  <div class="wrap col">
    <header class="row header">
      <Avatar name={chat.metadata.name} size={40} />
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
    </header>

    <div class="messages scroll" bind:this={scroller}>
      {#if chat.metadata.isProtected}
        <div class="protected-banner">
          🔒 This chat is marked as protected on the phone. Phase 1 doesn't sync
          its content; what you see here is whatever the phone has chosen to share.
        </div>
      {/if}
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
      <div class="compose-row">
        <textarea
          class="compose-input"
          placeholder="Type a message…  (Enter to send, Shift+Enter for newline)"
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
          Sending into protected chats from the desktop is disabled in phase 2a.
        </div>
      {/if}
    </footer>
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
  }
  .empty {
    margin: auto;
    text-align: center;
    padding: 2rem;
  }
  .protected-banner {
    align-self: stretch;
    margin-bottom: .75rem;
    padding: .55rem .8rem;
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--on-surface-muted);
    font-size: .85rem;
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
  .compose-error {
    color: var(--error, #b00);
  }
</style>
