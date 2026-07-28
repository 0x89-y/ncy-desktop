// Link scheme guard, shared by the compose-box parser and the message renderer.
//
// Message content arrives from whoever authored it — a paired phone or a peer
// desktop — so `span.link` is untrusted by the time it reaches the UI. Anything
// that ends up in an `href`, or gets handed to the shell's `open`, goes through
// here first: only http(s) survives, so a `javascript:`, `file:`, or `data:` URL
// can never be rendered as a live link or handed to the OS.

export function safeLink(link: string | null | undefined): string | null {
  return link && /^https?:\/\//i.test(link) ? link : null;
}
