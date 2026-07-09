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
    version: "0.4.3",
    date: "2026-07-09",
    changes: [
      "Click an image in a chat to open it full-size. Zoom with the scroll wheel, drag to move around, and use the arrow keys to step through every image in that chat.",
      "Copy an image straight to the clipboard, ready to paste anywhere, or save it to disk with the right file type.",
      "Fixed a black command window flashing over the app when opening Settings or changing a setting.",
    ],
  },
  {
    version: "0.4.2",
    date: "2026-07-09",
    changes: [
      "Closing the window now asks whether to keep Ncy running in the tray, rather than quietly hiding it. Choose once and it remembers.",
      "Sync is now a switch you control, under Sync → Sync server. Turn it off and Ncy stops serving on your network entirely — no port, no advertising, no outgoing connections.",
      "If you were already syncing with a phone, sync stays on. Fresh installs start with it off.",
    ],
  },
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
