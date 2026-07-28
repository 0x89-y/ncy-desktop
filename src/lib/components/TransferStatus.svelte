<script lang="ts">
  import { X } from "@lucide/svelte";
  import { api, type MediaType } from "../ipc";
  import { blobFailures, blobProgress, CANCELLED_REASON } from "../stores/app";
  import { formatFileSize } from "../format";

  /**
   * The status line for a media blob that is not on disk yet.
   *
   * Three states used to look identical — actively transferring, never started, and
   * quietly given up — which left the user staring at "downloading…" forever with no
   * way to tell a slow transfer from a dead one. The core now reports progress and
   * failure, and this renders that distinction.
   */
  let {
    hash,
    mediaType,
    label = "",
    idleText,
  }: {
    hash: string;
    mediaType: MediaType;
    /** Noun for this media, e.g. "Voice message". Omit where the surrounding
     *  layout already names it — the file card prints the name and size itself. */
    label?: string;
    /** Shown when nothing is in flight and nothing has failed. */
    idleText: string;
  } = $props();

  const prefix = $derived(label ? `${label} · ` : "");

  let cancelling = $state(false);

  async function cancel(event: MouseEvent) {
    // The file variant lives inside a clickable card whose own handler would
    // otherwise fire and immediately re-request what we just cancelled.
    event.stopPropagation();
    event.preventDefault();
    if (cancelling) return;
    cancelling = true;
    try {
      await api.cancelBlobDownload(hash, mediaType);
    } catch (e) {
      console.error("cancel download failed", e);
    } finally {
      cancelling = false;
    }
  }

  const progress = $derived($blobProgress.get(hash));
  const failure = $derived($blobFailures.get(hash));
  const percent = $derived(
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.received / progress.total) * 100))
      : null,
  );
</script>

{#if failure === CANCELLED_REASON}
  <span class="muted">{prefix}download cancelled</span>
{:else if failure}
  <span class="failed">{prefix}transfer failed ({failure})</span>
{:else if progress}
  <span class="progress">
    {prefix}{percent ?? 0}%
    <span class="bytes">({formatFileSize(progress.received)} of {formatFileSize(progress.total)})</span>
  </span>
  <span class="bar" aria-hidden="true"><span class="fill" style={`width:${percent ?? 0}%`}></span></span>
  <button
    type="button"
    class="cancel"
    disabled={cancelling}
    title="Stop downloading"
    aria-label="Stop downloading"
    onclick={cancel}
  ><X size={12} strokeWidth={2.5} /></button>
{:else}
  <span>{idleText}</span>
{/if}

<style>
  .failed { color: var(--danger); }
  .progress { font-variant-numeric: tabular-nums; }
  .bytes { opacity: .75; }
  .bar {
    flex: 1 1 4rem;
    min-width: 3rem;
    max-width: 10rem;
    height: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface-hover);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width .2s linear;
  }
  .cancel {
    flex: 0 0 auto;
    width: 1.05rem;
    height: 1.05rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
  }
  .cancel:hover { background: var(--surface-hover); color: var(--danger); filter: none; }
  .cancel:disabled { opacity: .5; }
</style>
