// Ncy desktop release notes, shown in Settings → About. Newest first. Bump the
// top version to match package.json on release. The first entry will be 0.4.1 —
// do not add anything before that.
//
// One clause per entry. Say what changed, not how to use it — this is a
// changelog, not documentation. "Added an image viewer.", not "Click an image to
// open it full-size, with zoom and arrow-key navigation."

export interface ChangelogEntry {
  version: string;
  /** ISO date (YYYY-MM-DD). */
  date: string;
  changes: string[];
}

export const CHANGELOG: ChangelogEntry[] = [
  {
    version: "0.5.0",
    date: "2026-07-10",
    changes: [
      "Added replies to messages.",
      "Redesigned voice messages with a compact player, waveform, seeking, and duration.",
      "Added a compact chat-list setting that hides message previews.",
      "New app icon.",
      "Reorganised sync and backup settings.",
      "Fixed the missing divider below the last chat.",
    ],
  },
  {
    version: "0.4.3",
    date: "2026-07-09",
    changes: [
      "Added an image viewer.",
      "Images can be copied or saved.",
      "Fixed a command window flashing over the app.",
    ],
  },
  {
    version: "0.4.2",
    date: "2026-07-09",
    changes: [
      "Closing the window now asks whether to keep Ncy in the tray.",
      "Sync can be switched off, and is off for new installs.",
    ],
  },
  {
    version: "0.4.1",
    date: "2026-07-09",
    changes: [
      "Ncy can now update itself.",
      "New tray and startup options.",
      "Pick your own six quick reactions.",
      "Friendlier message timestamps.",
      "Settings reorganised.",
      "Fixed duplicate profiles from a reinstalled phone.",
    ],
  },
];
