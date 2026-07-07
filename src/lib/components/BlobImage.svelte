<script lang="ts">
  import { api, type MediaType } from "../ipc";
  import { Image as ImageIcon, ImageOff } from "@lucide/svelte";

  let {
    hash,
    mediaType = "image",
    alt = "",
    klass = "",
  }: {
    hash: string;
    mediaType?: MediaType;
    alt?: string;
    klass?: string;
  } = $props();

  let dataUrl = $state<string | null>(null);
  let pending = $state(true);
  let failed = $state(false);

  // Refetch whenever the hash changes (re-rendering the same component for a new
  // blob, e.g. when scrolling chats with different inline images).
  $effect(() => {
    const h = hash;
    pending = true;
    failed = false;
    dataUrl = null;
    api
      .readBlob(h, mediaType)
      .then((b64) => {
        if (h !== hash) return; // stale request — ignore
        if (b64 == null) {
          failed = true;
        } else {
          const mime = mediaType === "voice" ? "audio/mp4" : "image/jpeg";
          dataUrl = `data:${mime};base64,${b64}`;
        }
      })
      .catch(() => {
        failed = true;
      })
      .finally(() => {
        if (h === hash) pending = false;
      });
  });
</script>

{#if dataUrl}
  <img src={dataUrl} {alt} class={klass} />
{:else if pending}
  <span class="blob-placeholder muted tiny">
    <ImageIcon size={12} strokeWidth={2.25} />
    <span>loading…</span>
  </span>
{:else if failed}
  <span class="blob-placeholder muted tiny">
    <ImageOff size={12} strokeWidth={2.25} />
    <span>not synced yet</span>
  </span>
{/if}

<style>
  .blob-placeholder {
    display: inline-flex;
    align-items: center;
    gap: .3rem;
    padding: 0.15rem 0.4rem;
    border-radius: 6px;
    background: var(--surface-2);
    font-style: italic;
    line-height: 1;
  }
  img {
    max-width: 100%;
    border-radius: 8px;
  }
</style>
