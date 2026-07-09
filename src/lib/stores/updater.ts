import { writable, get } from "svelte/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { sendNotification } from "@tauri-apps/plugin-notification";
import { api } from "../ipc";
import { ensureNotificationPermission } from "./app";

// Drives the app's own self-update: a manual check from Settings → About that
// asks before downloading, plus an optional background check (on startup and
// periodically) that only notifies.

export type UpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "uptodate"
  | "downloading"
  | "error";

export const updaterPhase = writable<UpdaterPhase>("idle");
export const updaterVersion = writable<string | null>(null);
export const updaterError = writable<string | null>(null);

/** The found-but-not-yet-installed update, kept so the user can confirm download. */
let pending: Update | null = null;

/** How often the background check runs when auto-check is enabled. */
const CHECK_INTERVAL_MS = 3 * 60 * 60 * 1000; // 3 hours

async function autoCheckEnabled(): Promise<boolean> {
  try {
    return (await api.getDesktopPrefs()).autoCheckUpdates;
  } catch {
    return false;
  }
}

async function runCheck(silent: boolean) {
  const phase = get(updaterPhase);
  if (phase === "checking" || phase === "downloading") return;
  if (!silent) updaterPhase.set("checking");
  updaterError.set(null);
  try {
    const update = await check();
    if (update) {
      pending = update;
      updaterVersion.set(update.version);
      updaterPhase.set("available");
      if (silent && (await ensureNotificationPermission())) {
        try {
          sendNotification({
            title: "Update available",
            body: `Ncy ${update.version} is ready to install.`,
          });
        } catch {
          /* swallow — a failed toast shouldn't break the check */
        }
      }
    } else if (!silent) {
      pending = null;
      updaterVersion.set(null);
      updaterPhase.set("uptodate");
    }
  } catch (e) {
    // A silent check failing is usually just "offline" — stay quiet.
    if (!silent) {
      updaterError.set(String(e));
      updaterPhase.set("error");
    }
  }
}

/** Manual check from Settings — surfaces checking / up-to-date / available. */
export const checkForUpdate = () => runCheck(false);

/** Silent background check; only notifies (and arms the download) if found. */
export const backgroundCheck = () => runCheck(true);

/** Download + install the pending update, then relaunch. Called on user confirm. */
export async function installUpdate() {
  if (!pending) return;
  updaterPhase.set("downloading");
  updaterError.set(null);
  try {
    await pending.downloadAndInstall();
    await relaunch();
  } catch (e) {
    updaterError.set(String(e));
    updaterPhase.set("error");
  }
}

let started = false;

/** Set up startup + periodic background checks (gated by the auto-check pref). */
export function initAppUpdater() {
  if (started) return;
  started = true;
  const maybeCheck = async () => {
    if (await autoCheckEnabled()) backgroundCheck();
  };
  maybeCheck();
  setInterval(maybeCheck, CHECK_INTERVAL_MS);
}
