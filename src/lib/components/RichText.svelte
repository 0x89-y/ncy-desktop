<script lang="ts">
  import type { RichTextBlock, RichTextSpan } from "../ipc";
  import RichSpan from "./RichSpan.svelte";
  let { blocks, onChecklistToggle }: {
    blocks: RichTextBlock[];
    onChecklistToggle?: (blockIndex: number) => void;
  } = $props();

  function checklistState(block: RichTextBlock): boolean | null {
    const text = block.spans.map((span) => span.text).join("");
    if (text.startsWith("[ ] ")) return false;
    if (text.startsWith("[x] ") || text.startsWith("[X] ")) return true;
    return null;
  }

  function withoutChecklistMarker(spans: RichTextSpan[]): RichTextSpan[] {
    let remaining = 4;
    return spans.flatMap((span) => {
      if (remaining === 0) return [span];
      if (span.text.length <= remaining) {
        remaining -= span.text.length;
        return [];
      }
      const result = { ...span, text: span.text.slice(remaining) };
      remaining = 0;
      return [result];
    });
  }
</script>

<div class="rt">
  {#each blocks as block, i (i)}
    {@const checked = checklistState(block)}
    {#if checked !== null}
      <label class="task-item">
        <input
          type="checkbox"
          {checked}
          disabled={!onChecklistToggle}
          onchange={() => onChecklistToggle?.(i)}
        />
        <span class:checked>{#each withoutChecklistMarker(block.spans) as s, j (j)}<RichSpan span={s} />{/each}</span>
      </label>
    {:else if block.type === "HEADING1"}
      <h1>{#each block.spans as s, j (j)}<RichSpan span={s} />{/each}</h1>
    {:else if block.type === "HEADING2"}
      <h2>{#each block.spans as s, j (j)}<RichSpan span={s} />{/each}</h2>
    {:else if block.type === "HEADING3"}
      <h3>{#each block.spans as s, j (j)}<RichSpan span={s} />{/each}</h3>
    {:else if block.type === "BULLET_LIST"}
      <ul><li>{#each block.spans as s, j (j)}<RichSpan span={s} />{/each}</li></ul>
    {:else if block.type === "CODE_BLOCK"}
      <pre><code>{block.spans.map((s) => s.text).join("")}</code></pre>
    {:else}
      <p>{#each block.spans as s, j (j)}<RichSpan span={s} />{/each}</p>
    {/if}
  {/each}
</div>

<style>
  .rt :global(p),
  .rt :global(h1),
  .rt :global(h2),
  .rt :global(h3) { margin: 0 0 0.25rem 0; }
  .rt :global(p):last-child,
  .rt :global(h1):last-child,
  .rt :global(h2):last-child,
  .rt :global(h3):last-child,
  .rt :global(ul):last-child,
  .rt :global(pre):last-child { margin-bottom: 0; }
  .rt :global(h1) { font-size: 1.4rem; font-weight: 700; }
  .rt :global(h2) { font-size: 1.2rem; font-weight: 700; }
  .rt :global(h3) { font-size: 1.05rem; font-weight: 600; }
  .rt :global(ul) { margin: 0 0 0.25rem 1.2rem; padding: 0; }
  .rt :global(li) { margin: 0.1rem 0; }
  .rt :global(pre) {
    background: var(--surface-3);
    border-radius: 8px;
    padding: 0.5rem 0.75rem;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 0.9rem;
    margin: 0 0 0.25rem 0;
  }
  .rt :global(a) {
    color: inherit;
    font-weight: 600;
    text-decoration: underline;
    text-decoration-thickness: .08em;
    text-underline-offset: .14em;
  }
  .task-item { display: flex; align-items: flex-start; gap: 0.45rem; margin: 0.1rem 0; }
  .task-item input { margin-top: 0.22rem; accent-color: var(--primary); }
  .task-item .checked { text-decoration: line-through; opacity: 0.72; }
</style>
