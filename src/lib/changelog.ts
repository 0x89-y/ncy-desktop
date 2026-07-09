// Ncy desktop release notes, shown in Settings → About. Newest first. Keep
// entries short and user-facing; bump the top version to match package.json on
// release. The first entry will be 0.4.1 — do not add anything before that.

export interface ChangelogEntry {
  version: string;
  /** ISO date (YYYY-MM-DD). */
  date: string;
  changes: string[];
}

export const CHANGELOG: ChangelogEntry[] = [
  {
    version: "0.4.1",
    date: "2026-07-09",
    changes: [
      "Ncy now updates itself — you'll be told when a new version is out, and it installs in a click. Turn it off in About → Updates.",
      "New tray and startup options: start minimized, keep running in the tray on close, launch with Windows.",
      "Pick your own six quick reactions.",
      "Message timestamps read as \"Yesterday\" or \"3 days ago\" instead of a raw date.",
      "Settings reorganised into General, Sync, Interface, Transcription, Backup and About.",
      "Chats from a reinstalled phone no longer show up as a duplicate profile.",
    ],
  },
];
