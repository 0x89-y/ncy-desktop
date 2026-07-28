import { api } from "../ipc";
import { refreshActiveChat, refreshChats } from "./app";

/**
 * Enforce per-chat message retention while the app is running.
 *
 * The rule lives on the chat and syncs, so whichever device is awake does the work
 * and the resulting tombstones propagate like any manual delete. Running hourly is
 * plenty: the windows are measured in days, and a message living an extra hour past
 * its window is not something anyone notices.
 */
const CHECK_MS = 60 * 60 * 1000;

let started = false;
let running = false;

async function sweep() {
  if (running) return;
  running = true;
  try {
    const deleted = await api.applyChatRetention();
    if (deleted > 0) {
      // Tombstones authored behind the UI's back — pull the views back in line.
      await refreshChats();
      await refreshActiveChat();
    }
  } catch (error) {
    console.error("retention sweep failed", error);
  } finally {
    running = false;
  }
}

export function initChatRetention() {
  if (started) return;
  started = true;
  // One-time cleanup for messages deleted before deletion learned to take their
  // text with it. Idempotent, so running it every launch costs a graveyard read.
  void api
    .scrubDeletedMessageContent()
    .catch((error) => console.error("scrubbing deleted message content failed", error));
  void sweep();
  window.setInterval(() => void sweep(), CHECK_MS);
}
