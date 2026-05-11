<script lang="ts">
  import type { RichTextBlock } from "../ipc";
  import RichSpan from "./RichSpan.svelte";
  let { blocks }: { blocks: RichTextBlock[] } = $props();
</script>

<div class="rt">
  {#each blocks as block, i (i)}
    {#if block.type === "HEADING1"}
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
    font-family: "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: 0.9rem;
    margin: 0 0 0.25rem 0;
  }
  .rt :global(a) { color: var(--primary); }
</style>
