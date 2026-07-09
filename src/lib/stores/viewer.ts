import { derived, get, writable } from "svelte/store";
import { extractMediaHash, type Message } from "../ipc";
import { activeChat } from "./app";

/** Blob hashes of every image message in the open chat, oldest first. The viewer's
 *  arrow keys step through this, so it must match the order on screen. */
export const chatImageHashes = derived(activeChat, ($chat) =>
  ($chat?.messages ?? [])
    .filter((m: Message) => m.type === "IMAGE")
    .map((m: Message) => extractMediaHash(firstImageRef(m)))
    .filter((h): h is string => h !== null),
);

/** Index into `chatImageHashes`, or -1 when the viewer is closed. */
export const viewerIndex = writable(-1);

export const viewerOpen = derived(viewerIndex, ($i) => $i >= 0);

/** Hash currently on display, or null when closed. */
export const viewerHash = derived(
  [chatImageHashes, viewerIndex],
  ([$hashes, $i]) => ($i >= 0 ? ($hashes[$i] ?? null) : null),
);

/** The image reference a message carries: an inline span wins, else the legacy
 *  `imageFiles[0]`. Mirrors MessageBubble's own lookup. */
export function firstImageRef(m: Message): string | null {
  for (const block of m.content) {
    for (const span of block.spans) {
      if (span.inlineImage) return span.inlineImage;
    }
  }
  return m.imageFiles[0] ?? null;
}

/** Open the viewer on `hash`. A no-op if the hash isn't among the chat's images
 *  (e.g. a blob that hasn't synced), so a click can't strand the viewer empty. */
export function openViewer(hash: string) {
  const i = get(chatImageHashes).indexOf(hash);
  if (i >= 0) viewerIndex.set(i);
}

export function closeViewer() {
  viewerIndex.set(-1);
}

/** Step by `delta`, clamped — the ends don't wrap, so holding an arrow key
 *  stops rather than cycling forever. */
export function stepViewer(delta: number) {
  const count = get(chatImageHashes).length;
  if (count === 0) return;
  viewerIndex.update((i) => {
    if (i < 0) return i;
    return Math.min(count - 1, Math.max(0, i + delta));
  });
}

// Close on a chat switch — the index points into the old chat's image list.
// `activeChat` also re-emits on every sync event for the *same* chat, so key off
// the id rather than the object, or the viewer would slam shut mid-look.
let lastChatId: string | null = null;
activeChat.subscribe((chat) => {
  const id = chat?.metadata.id ?? null;
  if (id !== lastChatId) {
    lastChatId = id;
    closeViewer();
  }
});
