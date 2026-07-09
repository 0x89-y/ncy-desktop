import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { api, type DesktopPrefs } from "../ipc";

/** When true, the "keep Ncy in the tray?" prompt is showing (CloseToTrayPrompt). */
export const closePromptOpen = writable(false);

let started = false;

/**
 * Listen for the close the Rust side held back.
 *
 * `on_window_event` in `lib.rs` intercepts the ✕ and emits `close-requested`
 * only when the user has neither opted into close-to-tray nor silenced the
 * prompt — so by the time this fires, asking is always the right move.
 */
export async function initClosePrompt() {
  if (started) return;
  started = true;
  await listen("close-requested", () => closePromptOpen.set(true));
}

/**
 * Read-modify-write a subset of the desktop prefs. The prompt only owns two of
 * them, and re-reading first keeps it from clobbering a pref the Settings view
 * changed since this window loaded.
 */
export async function patchDesktopPrefs(patch: Partial<DesktopPrefs>) {
  const current = await api.getDesktopPrefs();
  await api.updateDesktopPrefs({ ...current, ...patch });
}
