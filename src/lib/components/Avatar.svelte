<script lang="ts">
  import { avatarColor } from "../format";
  import { api, extractMediaHash } from "../ipc";
  import { arrivedBlobs } from "../stores/app";

  let {
    name,
    size = 40,
    avatarFile = null,
    hue = null,
  }: { name: string; size?: number; avatarFile?: string | null; hue?: string | null } = $props();

  // One letter on a soft tint of the chat's colour, as on Android: the colour
  // picked for the chat, or one derived from its name.
  const initial = $derived(name.trim().charAt(0).toUpperCase() || "·");
  const color = $derived(hue || avatarColor(name));

  // Pre-phase-2b avatars have legacy random/fixed names (e.g. "avatar.jpg"); those
  // never sync to the desktop. Only content-addressed refs are fetchable.
  const hash = $derived(extractMediaHash(avatarFile));
  // A picture set on another device can arrive after its chat does. Asking again
  // when it lands shows it at once; before, it waited for a restart.
  const arrived = $derived(hash ? $arrivedBlobs.has(hash) : false);

  let dataUrl = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    const h = hash;
    void arrived;
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
  class:image={!!dataUrl}
  style="--size:{size}px; --hue:{color};"
>
  {#if dataUrl}
    <img src={dataUrl} alt={name} />
  {:else}
    <span>{initial}</span>
  {/if}
</div>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: calc(var(--size) * 0.31);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--hue) 17%, var(--surface));
    color: color-mix(in srgb, var(--hue) 72%, var(--text));
    font-weight: 600;
    font-size: calc(var(--size) * 0.42);
    user-select: none;
    flex-shrink: 0;
    overflow: hidden;
  }
  .avatar.image { background: transparent; }
  :global([data-theme='dark']) .avatar {
    background: color-mix(in srgb, var(--hue) 24%, var(--surface));
    color: color-mix(in srgb, var(--hue) 55%, var(--text));
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
</style>
