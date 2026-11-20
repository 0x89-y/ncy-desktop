<script lang="ts">
  import { Users, X } from "@lucide/svelte";
  import { api } from "../ipc";
  import { activeProfileId, profileBuckets, refreshChats, selectChat } from "../stores/app";

  let menuOpen = $state(false);
  let managerOpen = $state(false);
  let profileName = $state("");
  let chatName = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const activeProfileLabel = $derived(
    $activeProfileId === null
      ? "All chats"
      : ($profileBuckets.find((profile) => profile.id === $activeProfileId)?.name ?? "All chats"),
  );

  function selectProfile(id: string | null) {
    activeProfileId.set(id);
    menuOpen = false;
  }

  function openManager() {
    menuOpen = false;
    managerOpen = true;
    profileName = "";
    chatName = "";
    error = null;
  }

  function closeManager() {
    if (busy) return;
    managerOpen = false;
    profileName = "";
    chatName = "";
    error = null;
  }

  async function createProfile() {
    const name = profileName.trim();
    const firstChatName = chatName.trim();
    if (!name || !firstChatName || busy) return;
    busy = true;
    error = null;
    try {
      const profileId = crypto.randomUUID();
      const chatId = await api.authorCreateChat(firstChatName, profileId, name);
      await refreshChats();
      const created = $profileBuckets.find((profile) => profile.profileId === profileId);
      activeProfileId.set(created?.id ?? null);
      await selectChat(chatId);
      managerOpen = false;
    } catch (e: any) {
      error = typeof e === "string" ? e : (e?.message ?? "create failed");
    } finally {
      busy = false;
    }
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    // Only claim Escape when a layer is actually open — see ChatListPane.
    if (managerOpen) {
      e.stopImmediatePropagation();
      closeManager();
    } else if (menuOpen) {
      e.stopImmediatePropagation();
      menuOpen = false;
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="profile-control">
  <button
    class="profile-button"
    class:active={$activeProfileId !== null}
    onclick={() => menuOpen = !menuOpen}
    aria-expanded={menuOpen}
    title={`Profile: ${activeProfileLabel}`}
    aria-label={`Switch profile. Current profile: ${activeProfileLabel}`}
  >
    <Users size={17} strokeWidth={2.25} aria-hidden="true" />
  </button>

  {#if menuOpen}
    <button class="menu-backdrop" aria-label="Close profile menu" onclick={() => menuOpen = false}></button>
    <div class="profile-menu">
      <button class:current={$activeProfileId === null} aria-current={$activeProfileId === null ? "true" : undefined} onclick={() => selectProfile(null)}><span>All chats</span></button>
      {#each $profileBuckets as profile (profile.id)}
        <button class:current={$activeProfileId === profile.id} aria-current={$activeProfileId === profile.id ? "true" : undefined} onclick={() => selectProfile(profile.id)}><span>{profile.name}</span></button>
      {/each}
      <div class="separator"></div>
      <button onclick={openManager}><span>Manage profiles…</span><Users size={14} /></button>
    </div>
  {/if}
</div>

{#if managerOpen}
  <button class="modal-backdrop" aria-label="Close profiles" onclick={closeManager}></button>
  <div class="profile-manager" role="dialog" aria-modal="true" aria-labelledby="profiles-title">
    <header>
      <div>
        <h2 id="profiles-title">Profiles</h2>
        <p class="muted">Organize chats into separate spaces.</p>
      </div>
      <button class="icon-btn" onclick={closeManager} disabled={busy} title="Close" aria-label="Close">
        <X size={18} />
      </button>
    </header>

    <div class="profile-list">
      <button class:current={$activeProfileId === null} aria-current={$activeProfileId === null ? "true" : undefined} onclick={() => { selectProfile(null); closeManager(); }}>
        <span><strong>All chats</strong><small class="muted">Show every profile</small></span>
      </button>
      {#each $profileBuckets as profile (profile.id)}
        <button class:current={$activeProfileId === profile.id} aria-current={$activeProfileId === profile.id ? "true" : undefined} onclick={() => { selectProfile(profile.id); closeManager(); }}>
          <span><strong>{profile.name}</strong><small class="muted">{profile.profileId ? "Synchronized profile" : "Chats without a profile"}</small></span>
        </button>
      {/each}
    </div>

    <form class="new-profile" onsubmit={(e) => { e.preventDefault(); void createProfile(); }}>
      <div>
        <strong>Create profile</strong>
        <p class="tiny muted">A profile synchronizes when its first chat is created.</p>
      </div>
      <input type="text" placeholder="Profile name" maxlength="120" bind:value={profileName} disabled={busy} />
      <input type="text" placeholder="First chat name" bind:value={chatName} disabled={busy} />
      {#if error}<span class="tiny create-error">{error}</span>{/if}
      <button class="primary" type="submit" disabled={busy || !profileName.trim() || !chatName.trim()}>
        {busy ? "Creating…" : "Create profile"}
      </button>
    </form>
  </div>
{/if}

<style>
  .profile-control { position: relative; flex: 0 0 auto; }
  .profile-button {
    width: 2rem;
    height: 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
  }
  .profile-button:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .profile-button.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .menu-backdrop,
  .menu-backdrop:hover {
    position: fixed;
    inset: 0;
    z-index: 20;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
    filter: none;
  }
  .profile-menu {
    position: absolute;
    z-index: 21;
    top: calc(100% + .35rem);
    left: 0;
    width: 14rem;
    display: flex;
    flex-direction: column;
    padding: .3rem;
    max-height: 20rem;
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
  }
  .profile-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: .45rem .55rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .profile-menu button:hover { background: var(--surface-hover); filter: none; }
  .separator { height: 1px; margin: .35rem .25rem; background: var(--border); }
  .modal-backdrop,
  .modal-backdrop:hover,
  .modal-backdrop:focus,
  .modal-backdrop:active {
    position: fixed;
    inset: 0;
    z-index: 50;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: rgba(0, 0, 0, .52);
    box-shadow: none;
    filter: none;
    transform: none;
    transition: none;
    cursor: default;
  }
  .profile-manager {
    position: fixed;
    z-index: 51;
    top: 50%;
    left: 50%;
    width: min(430px, calc(100vw - 2rem));
    max-height: min(650px, calc(100vh - 2rem));
    overflow: auto;
    transform: translate(-50%, -50%);
    border: 1px solid var(--border);
    border-radius: var(--radius-dialog);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .profile-manager header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem;
    border-bottom: 1px solid var(--border);
  }
  .profile-manager h2,
  .profile-manager p { margin: 0; }
  .profile-manager h2 { font-size: 1.1rem; }
  .profile-manager header p { margin-top: .2rem; font-size: .85rem; }
  .icon-btn {
    width: 2rem;
    height: 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
  }
  .icon-btn:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .profile-list {
    display: flex;
    flex-direction: column;
    padding: .5rem;
    border-bottom: 1px solid var(--border);
  }
  .profile-list button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .75rem;
    padding: .65rem .7rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .profile-list button:hover,
  /* The current profile: a stronger accent tint, an accent bar and a bolder name,
     like the open chat and the active settings section. No check mark. */
  .profile-menu button.current,
  .profile-list button.current {
    position: relative;
    background: var(--accent-selected);
    color: var(--on-accent-soft);
    font-weight: 600;
  }
  .profile-menu button.current::before,
  .profile-list button.current::before {
    content: "";
    position: absolute;
    left: 0;
    top: .35rem;
    bottom: .35rem;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .profile-list button.current .muted { color: var(--on-accent-soft); }
  .profile-list span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: .12rem;
  }
  .profile-list small { font-size: .75rem; }
  .new-profile {
    display: flex;
    flex-direction: column;
    gap: .6rem;
    padding: 1rem;
  }
  .new-profile p { margin: .15rem 0 0; }
  .new-profile input {
    min-width: 0;
    padding: .55rem .65rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2, var(--surface));
    color: inherit;
    font: inherit;
  }
  .new-profile .primary {
    align-self: flex-end;
    padding: .5rem .85rem;
    border: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-contrast);
    font: inherit;
    font-weight: 600;
  }
  .new-profile .primary:disabled { opacity: .45; }
  .create-error { color: var(--danger); }
</style>
