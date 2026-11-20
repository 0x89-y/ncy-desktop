<script lang="ts">
  // Collapsed form of the chat list: one avatar per chat, nothing else. Uses the
  // same filters as the full list, so what you see here is what you'd see there.
  import { activeChatId, chatListCollapsed, chatPane, newChatRequest, selectChat, visibleChats } from "../stores/app";
  import Avatar from "./Avatar.svelte";
  import { Lock, Plus } from "@lucide/svelte";

  const railChats = $derived($visibleChats.filter((c) => !c.isArchived));

  function newChat() {
    chatListCollapsed.set(false);
    chatPane.set("chats");
    newChatRequest.set(true);
  }
</script>

<nav class="rail" aria-label="Chats">
  <div class="list scroll">
    {#each railChats as c (c.id)}
      <button
        type="button"
        class="rail-btn"
        class:selected={$activeChatId === c.id}
        title={c.name}
        aria-label={c.name}
        aria-current={$activeChatId === c.id ? "true" : undefined}
        onclick={() => selectChat(c.id)}
      >
        <Avatar name={c.name} size={32} avatarFile={c.avatarFile} hue={c.bubbleColor} />
        {#if c.isProtected}<span class="lock"><Lock size={9} strokeWidth={2.5} /></span>{/if}
      </button>
    {/each}
  </div>
  <button type="button" class="new" title="New chat" aria-label="New chat" onclick={newChat}>
    <Plus size={18} strokeWidth={2.5} />
  </button>
</nav>

<style>
  .rail {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    min-height: 0;
  }
  .list {
    flex: 1;
    min-height: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: .45rem 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .rail-btn {
    position: relative;
    width: 44px;
    height: 44px;
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
  }
  .rail-btn:hover { background: var(--surface-2); filter: none; }
  .rail-btn.selected { background: var(--primary-container); }
  .lock {
    position: absolute;
    right: 4px;
    bottom: 4px;
    display: inline-flex;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    line-height: 0;
  }
  .new {
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    margin: .45rem 0 .7rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-contrast);
  }
</style>
