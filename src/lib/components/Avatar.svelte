<script lang="ts">
  import { avatarColor, avatarInitials } from "../format";
  import { api, extractMediaHash } from "../ipc";

  let {
    name,
    size = 40,
    avatarFile = null,
  }: { name: string; size?: number; avatarFile?: string | null } = $props();

  const initials = $derived(avatarInitials(name));
  const bg = $derived(avatarColor(name));

  // Pre-phase-2b avatars have legacy random/fixed names (e.g. "avatar.jpg"); those
  // never sync to the desktop. Only content-addressed refs are fetchable.
  const hash = $derived(extractMediaHash(avatarFile));

  let dataUrl = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    const h = hash;
    dataUrl = null;
    failed = false;
    if (!h) return;
    api
      .readBlob(h, "image")
      .then((b64) => {
        if (h !== hash) return; // stale
        if (b64 == null) failed = true;
        else dataUrl = `data:image/jpeg;base64,${b64}`;
      })
      .catch(() => {
        if (h === hash) failed = true;
      });
  });
</script>

<div
  class="avatar"
  style="--size:{size}px; background:{dataUrl ? 'transparent' : bg};"
>
  {#if dataUrl}
    <img src={dataUrl} alt={name} />
  {:else}
    <span>{initials}</span>
  {/if}
</div>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: white;
    font-weight: 600;
    font-size: calc(var(--size) * 0.4);
    user-select: none;
    flex-shrink: 0;
    overflow: hidden;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
</style>
