<script lang="ts">
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, type ImageView } from "../ipc";
  import {
    chatImageHashes,
    closeViewer,
    stepViewer,
    viewerHash,
    viewerIndex,
    viewerOpen,
  } from "../stores/viewer";
  import { ChevronLeft, ChevronRight, Copy, Download, X } from "@lucide/svelte";

  const MIN_ZOOM = 0.1;
  const MAX_ZOOM = 8;

  let view = $state<ImageView | null>(null);
  let loadError = $state<string | null>(null);
  /** 1 = fit-to-window (the `<img>` is CSS-constrained), not 1:1 pixels. */
  let zoom = $state(1);
  let panX = $state(0);
  let panY = $state(0);
  let dragging = $state(false);
  let dragStartX = 0;
  let dragStartY = 0;
  /** Transient "Copied" / "Saved" confirmation under the toolbar. */
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let busy = $state(false);

  function resetTransform() {
    zoom = 1;
    panX = 0;
    panY = 0;
  }

  function flash(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 1600);
  }

  // Load whenever the displayed hash changes — stepping with the arrows swaps
  // the hash, and a stale response must not overwrite a newer one.
  $effect(() => {
    const h = $viewerHash;
    if (!h) {
      view = null;
      return;
    }
    let cancelled = false;
    view = null;
    loadError = null;
    resetTransform();
    api
      .imageView(h)
      .then((v) => {
        if (!cancelled) view = v;
      })
      .catch((e) => {
        if (!cancelled) loadError = typeof e === "string" ? e : (e?.message ?? "could not load image");
      });
    return () => {
      cancelled = true;
    };
  });

  const src = $derived(view ? `data:${view.mime};base64,${view.dataB64}` : null);
  const position = $derived(`${$viewerIndex + 1} / ${$chatImageHashes.length}`);

  async function copy() {
    if (!$viewerHash || busy) return;
    busy = true;
    try {
      await api.copyImage($viewerHash);
      flash("Copied");
    } catch (e) {
      flash(typeof e === "string" ? e : "Copy failed");
    } finally {
      busy = false;
    }
  }

  async function download() {
    if (!$viewerHash || !view || busy) return;
    busy = true;
    try {
      // Default to the sniffed extension, not the `.jpg` the reference claims.
      const dest = await saveDialog({
        defaultPath: `ncy-image-${$viewerHash.slice(0, 8)}.${view.ext}`,
        filters: [{ name: "Image", extensions: [view.ext] }],
      });
      if (dest) {
        await api.saveImage($viewerHash, dest);
        flash("Saved");
      }
    } catch (e) {
      flash(typeof e === "string" ? e : "Save failed");
    } finally {
      busy = false;
    }
  }

  function zoomBy(factor: number) {
    zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor));
    if (zoom === 1) {
      panX = 0;
      panY = 0;
    }
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    zoomBy(e.deltaY < 0 ? 1.15 : 1 / 1.15);
  }

  function onPointerDown(e: PointerEvent) {
    if (zoom === 1) return; // nothing to pan while fitted
    dragging = true;
    dragStartX = e.clientX - panX;
    dragStartY = e.clientY - panY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    panX = e.clientX - dragStartX;
    panY = e.clientY - dragStartY;
  }

  function onPointerUp(e: PointerEvent) {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!$viewerOpen) return;
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        closeViewer();
        break;
      case "ArrowLeft":
        e.preventDefault();
        stepViewer(-1);
        break;
      case "ArrowRight":
        e.preventDefault();
        stepViewer(1);
        break;
      case "+":
      case "=":
        e.preventDefault();
        zoomBy(1.25);
        break;
      case "-":
        e.preventDefault();
        zoomBy(1 / 1.25);
        break;
      case "0":
        e.preventDefault();
        resetTransform();
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if $viewerOpen}
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Image viewer">
    <!-- Clicking the empty space closes; the image and toolbar stop propagation. -->
    <button class="backdrop-close" onclick={closeViewer} aria-label="Close image viewer"></button>

    <div class="toolbar" role="toolbar" tabindex="-1">
      {#if $chatImageHashes.length > 1}
        <span class="tiny muted position">{position}</span>
      {/if}
      <span class="tiny muted position">{Math.round(zoom * 100)}%</span>
      <button type="button" class="ghost" onclick={copy} disabled={busy || !view} title="Copy image">
        <Copy size={15} strokeWidth={2.25} />
      </button>
      <button type="button" class="ghost" onclick={download} disabled={busy || !view} title="Save image">
        <Download size={15} strokeWidth={2.25} />
      </button>
      <button type="button" class="ghost" onclick={closeViewer} title="Close (Esc)">
        <X size={15} strokeWidth={2.25} />
      </button>
    </div>

    {#if $viewerIndex > 0}
      <button class="nav prev" onclick={() => stepViewer(-1)} aria-label="Previous image">
        <ChevronLeft size={22} strokeWidth={2.25} />
      </button>
    {/if}
    {#if $viewerIndex < $chatImageHashes.length - 1}
      <button class="nav next" onclick={() => stepViewer(1)} aria-label="Next image">
        <ChevronRight size={22} strokeWidth={2.25} />
      </button>
    {/if}

    <div
      class="stage"
      class:grabbing={dragging}
      class:pannable={zoom !== 1}
      onwheel={onWheel}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      role="presentation"
    >
      {#if src}
        <img
          {src}
          alt=""
          draggable="false"
          style="transform: translate({panX}px, {panY}px) scale({zoom});"
        />
      {:else if loadError}
        <p class="muted">{loadError}</p>
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    </div>

    {#if toast}
      <div class="toast tiny">{toast}</div>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: color-mix(in srgb, var(--bg) 88%, transparent);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .backdrop-close {
    position: absolute;
    inset: 0;
    border: 0;
    background: transparent;
    cursor: default;
  }
  .toolbar {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.45rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--card-shadow);
  }
  .toolbar .position {
    padding: 0 0.25rem;
    font-variant-numeric: tabular-nums;
  }
  .nav {
    position: absolute;
    z-index: 2;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    padding: 0;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 50%;
    cursor: pointer;
    box-shadow: var(--card-shadow);
  }
  .nav:hover { background: var(--surface-hover); filter: none; }
  .prev { left: 16px; }
  .next { right: 16px; }

  .stage {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    padding: 56px 64px;
    overflow: hidden;
    touch-action: none;
  }
  .stage.pannable { cursor: grab; }
  .stage.grabbing { cursor: grabbing; }
  .stage img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
    /* Panning must feel immediate; only the zoom step is animated. */
    transition: transform 90ms ease-out;
    user-select: none;
  }
  .stage.grabbing img { transition: none; }

  .toast {
    position: absolute;
    z-index: 3;
    bottom: 22px;
    left: 50%;
    transform: translateX(-50%);
    padding: 0.35rem 0.7rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--card-shadow);
  }
</style>
