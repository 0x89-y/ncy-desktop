<script lang="ts">
  import { extractMediaHash, type RichTextSpan } from "../ipc";
  import BlobImage from "./BlobImage.svelte";

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

  // Content-addressed inline image: pull the SHA-256 out of the reference; render
  // via BlobImage. Legacy random-named inline images (pre-phase-2b) fall through
  // to text rendering since they can't be fetched over the wire anyway.
  const inlineHash = $derived(extractMediaHash(span.inlineImage));
</script>

{#if inlineHash}
  <BlobImage hash={inlineHash} mediaType="image" klass="inline-image" />
  {#if span.text}<span {style}>{span.text}</span>{/if}
{:else if span.link}
  <a href={span.link} target="_blank" rel="noreferrer" {style}>{span.text}</a>
{:else}
  <span {style}>{span.text}</span>
{/if}

<style>
  :global(.inline-image) {
    display: inline-block;
    max-width: 100%;
    max-height: 280px;
    vertical-align: middle;
    margin: 0.15rem 0;
  }
</style>
