import { api } from "../ipc";

const CHECK_MS = 15 * 60 * 1000;
let started = false;
let running = false;
let retryAfter = 0;

async function checkAutomaticBackup() {
  if (running || Date.now() < retryAfter) return;
  const prefs = await api.getDesktopPrefs().catch(() => null);
  if (!prefs?.autoBackupEnabled || !prefs.autoBackupFolder) return;
  const dueAt = (prefs.lastAutoBackupAt ?? 0) + Math.max(1, prefs.autoBackupIntervalHours) * 60 * 60 * 1000;
  if (Date.now() < dueAt) return;
  running = true;
  try {
    await api.runAutomaticBackup();
    const current = await api.getDesktopPrefs();
    await api.updateDesktopPrefs({ ...current, lastAutoBackupAt: Date.now(), lastAutoBackupError: null });
  } catch (error) {
    retryAfter = Date.now() + 6 * 60 * 60 * 1000;
    const current = await api.getDesktopPrefs().catch(() => prefs);
    const message = typeof error === "string" ? error : error instanceof Error ? error.message : "Automatic backup failed";
    await api.updateDesktopPrefs({ ...current, lastAutoBackupError: message }).catch(() => undefined);
  } finally {
    running = false;
  }
}

export function initAutomaticBackups() {
  if (started) return;
  started = true;
  void checkAutomaticBackup();
  window.setInterval(() => void checkAutomaticBackup(), CHECK_MS);
}
