<script lang="ts">
  let { text, query }: { text: string; query: string } = $props();

  const parts = $derived.by(() => {
    const needle = query.trim();
    if (!needle) return [{ text, match: false }];
    const result: { text: string; match: boolean }[] = [];
    const lower = text.toLocaleLowerCase();
    const target = needle.toLocaleLowerCase();
    let cursor = 0;
    while (cursor < text.length) {
      const index = lower.indexOf(target, cursor);
      if (index < 0) {
        result.push({ text: text.slice(cursor), match: false });
        break;
      }
      if (index > cursor) result.push({ text: text.slice(cursor, index), match: false });
      result.push({ text: text.slice(index, index + target.length), match: true });
      cursor = index + target.length;
    }
    return result.length ? result : [{ text, match: false }];
  });
</script>

{#each parts as part, i (i)}
  {#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}
{/each}

<style>
  mark {
    padding: 0 .06em;
    border-radius: 3px;
    background: color-mix(in oklab, var(--accent) 35%, transparent);
    color: inherit;
  }
</style>
