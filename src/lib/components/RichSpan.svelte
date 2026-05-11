<script lang="ts">
  import type { RichTextSpan } from "../ipc";
  let { span }: { span: RichTextSpan } = $props();

  const style = $derived.by(() => {
    const parts: string[] = [];
    if (span.bold) parts.push("font-weight:700");
    if (span.italic) parts.push("font-style:italic");
    const decos: string[] = [];
    if (span.underline) decos.push("underline");
    if (span.strikethrough) decos.push("line-through");
    if (decos.length) parts.push(`text-decoration:${decos.join(" ")}`);
    if (span.color) parts.push(`color:${span.color}`);
    return parts.join(";");
  });
</script>

{#if span.link}
  <a href={span.link} target="_blank" rel="noreferrer" {style}>{span.text}</a>
{:else}
  <span {style}>{span.text}</span>
{/if}
