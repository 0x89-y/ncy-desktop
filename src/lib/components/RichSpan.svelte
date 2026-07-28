<script lang="ts">
  import { extractMediaHash, type RichTextSpan } from "../ipc";
  import { safeLink } from "../links";
  import BlobImage from "./BlobImage.svelte";
  import { open } from "@tauri-apps/plugin-shell";

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

  // `span.link` is authored by whoever sent the message, so it arrives here
  // unvalidated. Every link below funnels through `safeLink` before it can reach
  // an `href`; rejected links degrade to plain text rather than a dead anchor.
  const textParts = $derived.by(() => {
    if (span.link) return [{ text: span.text, link: safeLink(span.link) }];
    const parts: { text: string; link: string | null }[] = [];
    const pattern = /https?:\/\/[^\s<]+/gi;
    let cursor = 0;
    for (const match of span.text.matchAll(pattern)) {
      const index = match.index ?? 0;
      if (index > cursor) parts.push({ text: span.text.slice(cursor, index), link: null });
      const raw = match[0];
      const link = raw.replace(/[.,!?;:\])]+$/g, "");
      const trailing = raw.slice(link.length);
      parts.push({ text: link, link: safeLink(link) });
      if (trailing) parts.push({ text: trailing, link: null });
      cursor = index + raw.length;
    }
    if (cursor < span.text.length) parts.push({ text: span.text.slice(cursor), link: null });
    return parts.length ? parts : [{ text: span.text, link: null }];
  });

  async function openLink(e: MouseEvent, link: string | null) {
    e.preventDefault();
    const target = safeLink(link);
    if (!target) return;
    try {
      await open(target);
    } catch (error) {
      console.error("open link failed", error);
    }
  }
</script>

{#if inlineHash}
  <BlobImage hash={inlineHash} mediaType="image" klass="inline-image" />
  {#if span.text}<span {style}>{span.text}</span>{/if}
{:else}
  {#each textParts as part, i (i)}
    {#if part.link}
      <!-- No `target="_blank"`: the webview treats that as a new-window request and
           opens the URL itself, on top of the `shell.open` below — one click, two
           browser tabs. `openLink` preventDefaults, so navigation stays with us. -->
      <a href={part.link} title={part.link} rel="noreferrer noopener" {style} onclick={(e) => openLink(e, part.link)}>{part.text}</a>
    {:else}
      <span {style}>{part.text}</span>
    {/if}
  {/each}
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
