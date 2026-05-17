<script lang="ts">
  import { activeChatId, visibleChats, selectChat, filterText, showArchived, allTags, activeTag, profileBuckets, activeProfileId } from "../stores/app";
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
</script>

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
  </div>
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
      />
    {:else}
      <div class="empty muted">No chats yet. Pair a phone to start syncing.</div>
    {/each}
  </div>
</div>

<style>
  .wrap {
    height: 100%;
    padding: .75rem;
    gap: .5rem;
  }
  .controls input {
    flex: 1;
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
</style>
