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
    version: "0.6.4",
    date: "2026-07-11",
    changes: [
      "Device settings now explain guest-network isolation and blocked discovery.",
      "The discoverability switch now explains its effect on IP-address recovery.",
      "Local discovery no longer broadcasts the desktop's friendly name or hostname.",
      "Pairing credentials can now be rotated or revoked with clear re-pairing behavior.",
      "Identically named paired devices now show a stable short ID fingerprint.",
      "Renamed devices now refresh after authenticating without changing pairing labels.",
      "Device cards now show connection, discovery, retry, and LAN diagnostics.",
    ],
  },
  {
    version: "0.6.3",
    date: "2026-07-11",
    changes: [
      "Discovered address changes are now trusted only after the device authenticates.",
      "Reinstalled devices now recover complete chat history and deletion tombstones.",
      "Any file type can now be sent, received, and saved as a chat attachment.",
    ],
  },
  {
    version: "0.6.2",
    date: "2026-07-11",
    changes: [
      "Backup restore now safely replays sync state without stale progress markers.",
      "Sync now detects cloned or mismatched device identities.",
      "Events created during initial catch-up are now delivered without gaps.",
      "Concurrent edits, deletes, profiles, and protected chats now resolve consistently.",
      "Pairing credentials now have clearer warnings and short clipboard lifetimes.",
      "Messages and media are now encrypted while syncing across the local network.",
    ],
  },
  {
    version: "0.6.1",
    date: "2026-07-11",
    changes: [
      "Sync now supports IPv6-only local networks.",
      "Offline phone hosts now explain when Ncy needs to be opened on the phone.",
      "Automatic sync can now be paused for individual devices.",
      "Sync now rejects device clocks that are dangerously far ahead.",
    ],
  },
  {
    version: "0.6.0",
    date: "2026-07-11",
    changes: [
      "Paired devices are rediscovered automatically after their LAN address changes.",
      "Incoming sync connections now report their device name.",
      "Discovered devices now advertise which pairing flow they support.",
      "Sync now rejects oversized frames and media transfers.",
      "Media sync now streams files with bounded memory use.",
      "Stalled media transfers now clean themselves up automatically.",
    ],
  },
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
