<script lang="ts">
  import { Lock, Pencil, X } from "@lucide/svelte";
  import Avatar from "./Avatar.svelte";
  import { avatarColor, CHAT_HUES } from "../format";
  import type { MaterializedChat } from "../ipc";

  type MetaPatch = {
    name?: string;
    isPinned?: boolean;
    isArchived?: boolean;
    tags?: string[];
    remoteMessagesOnLeft?: boolean;
    bubbleColor?: string;
  };

  let {
    chat,
    busy = false,
    retentionChoices,
    onClose,
    onMeta,
    onRetention,
    onSetAvatar,
    onClearAvatar,
    onSetBackground,
    onClearBackground,
    onPin,
    onDelete,
  }: {
    chat: MaterializedChat;
    busy?: boolean;
    retentionChoices: { days: number; label: string }[];
    onClose: () => void;
    onMeta: (patch: MetaPatch) => void | Promise<void>;
    onRetention: (days: number) => void | Promise<void>;
    onSetAvatar: () => void;
    onClearAvatar: () => void;
    onSetBackground: () => void;
    onClearBackground: () => void;
    onPin: (mode: "protect" | "change") => void;
    onDelete: () => void;
  } = $props();

  const meta = $derived(chat.metadata);
  const retention = $derived(meta.retentionDays ?? 0);
  const nameHue = $derived(avatarColor(meta.name));
  const pickedHue = $derived(meta.bubbleColor?.toLowerCase() ?? null);
  const started = $derived(
    meta.createdAt
      ? new Date(meta.createdAt).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" })
      : null,
  );

  // Name and tags are edited in place, inside the panel.
  let renaming = $state(false);
  let nameDraft = $state("");
  let editingTags = $state(false);
  let tagsDraft = $state("");

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function startRename() {
    nameDraft = meta.name;
    renaming = true;
  }
  async function saveName() {
    const name = nameDraft.trim();
    if (name && name !== meta.name) await onMeta({ name });
    renaming = false;
  }

  function startTags() {
    tagsDraft = meta.tags.join(", ");
    editingTags = true;
  }
  async function saveTags() {
    await onMeta({ tags: tagsDraft.split(",").map((t) => t.replace(/^#/, "").trim()).filter(Boolean) });
    editingTags = false;
  }

  /** Enter saves, Escape backs out of the field without closing the panel. */
  function fieldKeys(e: KeyboardEvent, save: () => void, cancel: () => void) {
    if (e.key === "Enter") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancel();
    }
  }
</script>

<button class="backdrop" aria-label="Close chat settings" onclick={onClose}></button>
<section class="panel" aria-label="Chat settings">
  <div class="head">
    <button type="button" class="icon-btn" title="Close" aria-label="Close" onclick={onClose}><X size={17} /></button>
  </div>

  <div class="scroll">
    <div class="hero">
      <button
        type="button"
        class="hero-tile"
        title={meta.avatarFile ? "Change picture" : "Set a picture"}
        aria-label={meta.avatarFile ? "Change picture" : "Set a picture"}
        onclick={onSetAvatar}
      >
        <Avatar name={meta.name} size={72} avatarFile={meta.avatarFile} hue={meta.bubbleColor} />
        <span class="badge" aria-hidden="true"><Pencil size={12} strokeWidth={2.5} /></span>
      </button>
      {#if renaming}
        <input
          class="name-input"
          aria-label="Chat name"
          bind:value={nameDraft}
          use:focusSelect
          disabled={busy}
          onkeydown={(e) => fieldKeys(e, () => void saveName(), () => (renaming = false))}
          onblur={() => void saveName()}
        />
      {:else}
        <button type="button" class="name" title="Rename" onclick={startRename}>
          {#if meta.isProtected}<Lock size={15} strokeWidth={2.25} aria-hidden="true" />{/if}
          <span>{meta.name}</span>
          <Pencil size={13} strokeWidth={2.25} aria-hidden="true" />
        </button>
      {/if}
      <div class="sub">
        {chat.messages.length} {chat.messages.length === 1 ? "note" : "notes"}{started ? ` · started ${started}` : ""}
      </div>
    </div>

    <div class="label">Organise</div>
    <label class="row">
      <span class="grow">Pin to top</span>
      <span class="switch"><input type="checkbox" checked={meta.isPinned} disabled={busy} onchange={(e) => void onMeta({ isPinned: e.currentTarget.checked })} /><span class="slider"></span></span>
    </label>
    {#if editingTags}
      <div class="row">
        <input
          class="field"
          placeholder="work, ideas"
          aria-label="Tags, separated by commas"
          bind:value={tagsDraft}
          use:focusSelect
          disabled={busy}
          onkeydown={(e) => fieldKeys(e, () => void saveTags(), () => (editingTags = false))}
        />
        <button type="button" class="link" onclick={() => void saveTags()} disabled={busy}>Save</button>
      </div>
    {:else}
      <div class="row">
        <span>Tags</span>
        <span class="grow tags">
          {#each meta.tags as tag (tag)}<span class="tag">#{tag}</span>{:else}<span class="muted">None</span>{/each}
        </span>
        <button type="button" class="link" onclick={startTags}>{meta.tags.length ? "Edit" : "Add"}</button>
      </div>
    {/if}

    <div class="label">Look</div>
    <div class="row bare">Colour</div>
    <div class="hues" role="radiogroup" aria-label="Chat colour">
      <button
        type="button"
        role="radio"
        class="hue"
        class:on={pickedHue === null}
        aria-checked={pickedHue === null}
        title="From the name"
        style="--hue:{nameHue}"
        disabled={busy}
        onclick={() => void onMeta({ bubbleColor: "" })}
      ><span class="auto">A</span></button>
      {#each CHAT_HUES as hue (hue)}
        <button
          type="button"
          role="radio"
          class="hue"
          class:on={pickedHue === hue}
          aria-checked={pickedHue === hue}
          aria-label={hue}
          style="--hue:{hue}"
          disabled={busy}
          onclick={() => void onMeta({ bubbleColor: hue.toUpperCase() })}
        ><span class="dot"></span></button>
      {/each}
    </div>
    <div class="row">
      <span class="grow">Picture</span>
      <button type="button" class="link" onclick={onSetAvatar}>{meta.avatarFile ? "Change" : "Set"}</button>
      {#if meta.avatarFile}<button type="button" class="link quiet" onclick={onClearAvatar}>Remove</button>{/if}
    </div>
    <div class="row">
      <span class="grow">Background</span>
      <button type="button" class="link" onclick={onSetBackground}>{meta.backgroundFile ? "Change" : "Set"}</button>
      {#if meta.backgroundFile}<button type="button" class="link quiet" onclick={onClearBackground}>Remove</button>{/if}
    </div>
    <label class="row">
      <span class="grow">Other devices on the left<span class="hint">Notes written on another device sit on the left.</span></span>
      <span class="switch"><input type="checkbox" checked={meta.remoteMessagesOnLeft} disabled={busy} onchange={(e) => void onMeta({ remoteMessagesOnLeft: e.currentTarget.checked })} /><span class="slider"></span></span>
    </label>

    <div class="label">Delete notes older than</div>
    <div class="pills" role="radiogroup" aria-label="Delete notes older than">
      {#each retentionChoices as choice (choice.days)}
        <button
          type="button"
          role="radio"
          class="pill"
          class:on={retention === choice.days}
          aria-checked={retention === choice.days}
          disabled={busy}
          onclick={() => void onRetention(choice.days)}
        >{choice.label}</button>
      {/each}
    </div>
    <p class="hint pad">Applies on every device. Starred notes and ones with an upcoming reminder are kept.</p>

    <div class="label">Privacy</div>
    <div class="row">
      <span class="grow">{meta.isProtected ? "Protected with a PIN" : "Protect with a PIN"}</span>
      <button type="button" class="link" onclick={() => onPin(meta.isProtected ? "change" : "protect")}>
        {meta.isProtected ? "Change PIN" : "Set PIN"}
      </button>
    </div>

    <div class="end">
      <button type="button" class="row action" onclick={() => void onMeta({ isArchived: !meta.isArchived })} disabled={busy}>
        {meta.isArchived ? "Unarchive chat" : "Archive chat"}
      </button>
      <button type="button" class="row action danger" onclick={onDelete}>Delete chat</button>
    </div>
  </div>
</section>

<style>
  .backdrop,
  .backdrop:hover {
    position: fixed;
    inset: 32px 0 0;
    z-index: 50;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: rgba(0, 0, 0, 0.28);
    box-shadow: none;
  }
  .panel {
    position: fixed;
    top: 32px;
    right: 0;
    bottom: 0;
    z-index: 51;
    width: min(380px, 100vw);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border-strong);
    box-shadow: -10px 0 30px rgba(0, 0, 0, 0.16);
  }
  .head {
    display: flex;
    justify-content: flex-end;
    padding: 0.6rem 0.6rem 0;
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
  }
  .icon-btn:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .scroll { overflow: auto; padding-bottom: 1rem; }

  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 0 1.1rem 0.75rem;
  }
  .hero-tile {
    position: relative;
    padding: 0;
    border: 0;
    background: transparent;
    border-radius: 22px;
    line-height: 0;
  }
  .hero-tile:hover { filter: none; }
  .hero-tile:hover :global(.avatar) { filter: brightness(0.96); }
  .badge {
    position: absolute;
    right: -6px;
    bottom: -6px;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--text);
    color: var(--surface);
    border: 3px solid var(--surface);
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.7rem;
    padding: 0.15rem 0.45rem;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 1.25rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    max-width: 100%;
  }
  .name span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name :global(svg:last-child) { color: var(--text-muted); flex: none; }
  .name:hover { background: var(--surface-hover); filter: none; }
  .name-input {
    margin-top: 0.7rem;
    width: 100%;
    text-align: center;
    font: inherit;
    font-size: 1.15rem;
    font-weight: 600;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--accent);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
    outline: none;
  }
  .sub { font-size: 0.8rem; color: var(--text-muted); margin-top: 0.1rem; }

  .label {
    font-family: var(--font-mono);
    font-size: 0.66rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 1rem 1.1rem 0.4rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-height: 46px;
    padding: 0.35rem 1.1rem;
    border-top: 1px solid var(--border);
    font-size: 0.88rem;
    color: var(--text);
  }
  label.row { cursor: pointer; }
  .label + .row { border-top: 0; }
  .row.bare { min-height: 0; padding-bottom: 0.45rem; }
  .grow { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .hint { font-size: 0.74rem; color: var(--text-muted); line-height: 1.35; }
  .hint.pad { padding: 0.1rem 1.1rem 0.2rem; margin: 0; }
  .muted { color: var(--text-muted); }
  .tags { flex-direction: row; flex-wrap: wrap; justify-content: flex-end; gap: 0.3rem; }
  .tag {
    font-size: 0.75rem;
    padding: 0.08rem 0.45rem;
    border-radius: 6px;
    background: var(--surface-2);
  }
  .link {
    border: 0;
    background: transparent;
    padding: 0.25rem 0.35rem;
    border-radius: 6px;
    color: var(--accent);
    font: inherit;
    font-size: 0.82rem;
    font-weight: 600;
  }
  .link:hover { background: var(--surface-hover); filter: none; }
  .link.quiet { color: var(--text-muted); }
  .field {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 0.86rem;
    padding: 0.4rem 0.55rem;
    border: 1px solid var(--accent);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
    outline: none;
  }

  .hues {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    padding: 0.15rem 1.1rem 0.8rem;
  }
  .hue {
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--hue) 18%, var(--surface));
  }
  :global([data-theme='dark']) .hue { background: color-mix(in srgb, var(--hue) 28%, var(--surface)); }
  .hue:hover { filter: brightness(0.96); }
  .hue.on { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--text); }
  .hue .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--hue); }
  .hue .auto { font-weight: 600; font-size: 0.8rem; color: color-mix(in srgb, var(--hue) 72%, var(--text)); }

  .pills { display: flex; flex-wrap: wrap; gap: 0.35rem; padding: 0.15rem 1.1rem 0.5rem; }
  .pill {
    padding: 0.28rem 0.7rem;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
  }
  .pill:hover:not(.on) { background: var(--surface-hover); filter: none; }
  .pill.on { background: var(--accent); border-color: var(--accent); color: var(--accent-on); font-weight: 600; }

  .end { margin-top: 1rem; border-bottom: 1px solid var(--border); }
  .action {
    width: 100%;
    border-left: 0;
    border-right: 0;
    border-bottom: 0;
    border-radius: 0;
    background: transparent;
    font: inherit;
    font-size: 0.88rem;
    text-align: left;
  }
  .action:hover { background: var(--surface-hover); filter: none; }
  .action.danger { color: var(--danger); }

  .switch { position: relative; display: inline-block; width: 40px; height: 22px; flex-shrink: 0; }
  .switch input { opacity: 0; width: 0; height: 0; }
  .slider {
    position: absolute;
    inset: 0;
    cursor: pointer;
    background: var(--border-strong);
    border-radius: var(--radius-pill);
    transition: background 0.15s;
  }
  .slider::before {
    content: "";
    position: absolute;
    height: 16px;
    width: 16px;
    left: 3px;
    top: 3px;
    background: #fff;
    border-radius: 50%;
    transition: transform 0.15s;
  }
  .switch input:checked + .slider { background: var(--accent); }
  .switch input:checked + .slider::before { transform: translateX(18px); }
  .switch input:focus-visible + .slider { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
