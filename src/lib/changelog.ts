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
    version: "0.7.3",
    date: "2026-07-27",
    changes: [
      "Added transfer progress for incoming media, with a button to stop a download.",
      "Added per-message removal of downloaded media that keeps the message and can be downloaded again.",
      "Added a media storage panel that reclaims files no message refers to, and runs automatically after deletions.",
      "Added optional per-chat automatic deletion of messages older than a chosen age, keeping starred messages and upcoming reminders.",
      "Fixed voice messages over three seconds and files of any appreciable size failing to transfer over Pear.",
      "Fixed files larger than roughly nine megabytes ending the sync session on a local network.",
      "Fixed a failed media transfer never being retried, and a large transfer stalling every other message on the connection.",
      "Fixed links in received messages being opened without checking their address first.",
      "Refined Sync settings with contextual connection descriptions and a compact custom file-download limit.",
      "Moved the editable device name to General and kept its installation ID behind advanced details.",
      "Pairing invitations now use temporary random labels until the authenticated device name is available.",
      "Added a top-level profile switcher and dedicated profile management.",
      "Removed QR and JSON credentials from paired-device views and restricted code replacement to unused invitations.",
      "Moved search and filtering into the compact sidebar header and added a pinned-chat filter.",
      "Made message links clickable, kept context menus on-screen, retained composer focus after sending, and added Up Arrow editing.",
      "Added active-filter counts and reset, keyboard shortcuts, highlighted keyboard search, per-chat drafts, unsaved-edit protection, and new-message navigation.",
      "Reorganized every settings page into consistent sections, removed redundant page headings, simplified Sync status, and moved network details into Technical.",
      "Hardened settings persistence, pairing invitation reads, backup imports, links, edit guards, search, keyboard handling, CSP, and continuous validation.",
    ],
  },
  {
    version: "0.7.2",
    date: "2026-07-24",
    changes: [
      "Added a configurable automatic file-download limit with an explicit download action for larger files.",
      "Increased the temporary absolute file-transfer ceiling to 1 GiB.",
      "Fixed edited Markdown checklists losing their task markers or collapsing their labels.",
      "Fixed the bundled Bare runtime remaining active after Ncy had fully exited.",
      "Simplified already-paired device actions by retaining removal instead of credential rotation.",
    ],
  },
  {
    version: "0.7.1",
    date: "2026-07-22",
    changes: [
      "New checklists are ordinary messages using interactive Markdown task markers, while legacy checklists remain fully supported.",
      "Combined attachments, voice recording, and a toggle for the new formatting toolbar into one composer add menu.",
      "Removed the dedicated checklist composer; checklist formatting now works directly in the normal message field.",
      "Consolidated profile, tag, and archived-chat filters into one menu.",
      "Archived chats now remain hidden until enabled and appear in a separate section on both Android and Desktop.",
      "Aligned the Android and Desktop composers, contextual message actions, and chat-settings organization.",
      "Standardized Favorites, Paired devices, connection statuses, last-synchronized times, and empty-state wording across Android and Desktop.",
      "Restricted LAN discovery advertisements to the selected network interface to avoid recurring virtual-adapter errors.",
      "Replaced the dedicated Desktop search view with an inline chat-and-message search bar and compact adjacent filter button.",
      "Refreshed the chat sidebar with an icon-only segmented view selector and an edge-to-edge layout.",
      "Moved chat organization, appearance, privacy, and deletion controls into a dedicated settings panel.",
      "Added scheduled automatic backups with preset or custom intervals, selectable destinations, configurable retention, and status reporting.",
      "Moved deletion-record cleanup from Backup to advanced sync data maintenance.",
      "Replaced the expanding changelog list with a latest-release preview and a series-filtered history view.",
      "Reworked Settings into an edge-to-edge navigation rail and independently scrolling content pane.",
      "Pear connection choices now save immediately and remain selected if the runtime cannot start.",
      "Pear-enabled pairing codes now wait for a public key instead of silently falling back to local-only pairing.",
      "Simplified local voice transcription to a one-click, progress-reported setup with pinned and verified downloads, including the Whisper runtime version and speech-model details in diagnostics.",
      "Added verified Fast, Balanced, and Accurate speech-model choices with multilingual and English-only variants.",
      "Downloaded speech models are now cached for instant switching, with an optional cleanup that preserves the active model.",
    ],
  },
  {
    version: "0.7.0",
    date: "2026-07-19",
    changes: [
      "Added optional Pear sync for paired devices on different networks.",
      "Added Local only, Pear only, Local preferred, and Pear preferred connection modes.",
      "Pairing codes can now include a Pear identity for first-time pairing across networks.",
      "Pear remains off by default and starts only after explicit consent.",
      "The Windows installer now includes its version-pinned Pear runtime.",
      "The desktop executable can now run as a foreground CLI client with sync, status, peer, and pairing commands.",
      "The desktop executable can also run the existing isolated server daemon and administration commands.",
      "Running server mode now accepts pairing and diagnostic commands directly in its log terminal.",
      "The standalone server is now built from the same Desktop workspace while keeping its separate deployment options.",
    ],
  },
  {
    version: "0.6.5",
    date: "2026-07-11",
    changes: [
      "Compacted deletion records now have additional stale-restore protection.",
      "Deleted chats now use compact backup-safe records with an optional warned purge.",
      "Deleted messages now use compact restart-safe records that remain protected from stale restores.",
      "Chats can show messages created on other devices as incoming bubbles on the left.",
      "Incoming messages use a stable, theme-aware color for each originating device.",
    ],
  },
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
