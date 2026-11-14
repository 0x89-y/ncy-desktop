# Changelog

## 0.7.5 — 2026-11-14

### Changed

- Updates now come from the GitHub releases of 0x89-y/ncy-desktop instead of
  0x89-y.xyz.

### Removed

- Removed Pear sync and the Connection mode setting. Devices sync over the local network
  only, and the installer no longer includes the Bare runtime and Pear worker. Devices
  that were paired only over Pear need to be paired again.

## 0.7.4 — 2026-10-20

### Added

- Images, voice notes and files sent in protected chats are now encrypted with the chat's
  PIN. Protecting a chat or changing its PIN encrypts its existing attachments too, and
  the unencrypted originals are removed.
- Notes this desktop receives are now passed on to its other connected devices straight
  away, instead of waiting for them to reconnect.

### Security

- Pairing requests from current phones use a committed key exchange, so a device relaying
  between phone and desktop can no longer force the two safety codes to match. Older
  phones can still pair.

### Fixed

- Fixed notes written on a device while it was offline never reaching devices that had
  synced in the meantime.
- Fixed a refusal that arrives before the connection is encrypted stopping reconnection
  for good; it now shows a warning and keeps retrying.

## 0.7.3 — 2026-07-27

### Added

- Added transfer progress for incoming media, showing the percentage and bytes received
  with a button to stop a download.
- Added per-message removal of downloaded media that keeps the message: the file leaves
  this device only, is never fetched back automatically, and can be downloaded again at
  any time.
- Added a media storage panel under Backup that reports how much space downloads use and
  reclaims files no message refers to any more; the same cleanup now runs automatically
  after messages are deleted.
- Added optional per-chat automatic deletion of messages older than a chosen age. The rule
  travels with the chat, so every paired device applies it; starred messages and ones with
  an upcoming reminder are always kept.
- Added a "Force full resync" action under Sync that asks paired devices to send their
  whole history again, for a device that is missing messages or still showing ones
  deleted elsewhere.

### Changed

- Deleting a message now removes its text from this device's records rather than only
  hiding it. Messages deleted before this update are cleaned up on the next launch.

- Refined Sync settings with contextual connection descriptions, a compact custom
  file-download limit, and device identity under General.
- Pairing invitations now use temporary random labels until authentication supplies the
  device name.
- New chats can now use the Default profile, an existing profile, or create a new
  synchronized profile.
- Removed QR and JSON credentials from paired-device views and restricted invitation-code
  replacement to unused invitations.
- Moved search and filtering into the compact sidebar header and added a pinned-chat filter.
- Made message links open safely in the system browser, kept message menus inside the
  viewport, retained composer focus after sending, and added Up Arrow editing.
- Added active-filter counts and reset, keyboard shortcuts, highlighted keyboard search,
  per-chat drafts, unsaved-edit protection, and new-message navigation.
- Reorganized General, Sync, and Interface into clearer sections with immediate saving,
  compact technical disclosures, and simplified reaction and connection controls.

### Fixed

- Fixed voice messages longer than about three seconds, and files of any appreciable size,
  failing to transfer over Pear.
- Fixed files larger than roughly nine megabytes ending the sync session on a local
  network instead of transferring.
- Fixed a media transfer that failed once never being retried, leaving the message showing
  "not on this device" until it was deleted.
- Fixed a large transfer stalling every other message on the same connection until it
  finished.
- Fixed links in received messages being opened without checking their address first.
- Fixed a removed download reverting to "Downloading…" after a restart, and voice
  messages and images offering no way to fetch them back once removed.
- Fixed images and voice messages reporting that the other device no longer had a file
  it was still holding, because every manual download asked for it as a file attachment.
- Fixed a removed voice message closing the app when played, and a missing image
  rendering as an empty gap with nothing to say a picture belonged there.

## 0.7.2 — 2026-07-24

### Added

- Added a configurable automatic file-download limit with an explicit download action for
  files above the local limit.

### Changed

- Increased the temporary absolute file-transfer ceiling to 1 GiB.
- Already-paired devices now use the existing remove action instead of offering credential
  rotation as an additional replacement workflow.

### Fixed

- Fixed edited Markdown checklists losing their task markers or collapsing their labels.
- Fixed the bundled Bare runtime remaining active after Ncy had fully exited.
- Updated Core's authentication and encryption calls to remove deprecated generic-array API
  warnings.

## 0.7.0 — 2026-07-19

### Added

- Added optional Pear synchronization for already-paired devices on different networks.
- Added Local only, Pear only, Local preferred, and Pear preferred routing policies, with
  Pear off by default and explicit consent before it starts.
- Added Pear public keys to QR/JSON pairing and accepted Pear-capable peer payloads.
- Added a headless desktop client with foreground and timed sync modes, status and peer
  inspection, and JSON pairing commands using the existing desktop profile.
- Added an integrated server mode that retains the standalone server configuration and
  isolated identity while reusing the desktop installation's bundled Pear runtime.
- Added interactive server commands and live pairing reload, allowing devices to be paired
  from the running server's log terminal without restarting it.
- Moved the standalone server crate into the Desktop Cargo workspace while retaining its
  independent binary, configuration, data directory, and deployment files.

### Packaging

- The Windows x64 NSIS installer now includes its version-pinned Bare runtime, worker
  bundle, and native addons using LZMA compression.
- Preserved the existing Ncy application identity and local data across the upgrade.

## Unreleased — 2026-07-11

### Added

- Desktop now inherits Core's compact restart-safe message-deletion ledger. Portable
  backups reconstruct its normal tombstone events, stale bodies cannot trigger blob
  downloads, and **Forget deletion records** now warns about chats and messages.

- Incoming bubbles now use a stable, theme-aware color derived from their originating
  device. Android and Desktop use the same mapping without adding synchronized color state.

- Added **Other devices on left** to each chat's options. Messages authored by another
  installation use an incoming bubble style; local and unknown legacy messages remain
  on the right. Desktop authors and preserves the shared creator metadata through edits.

- Desktop inherits Core's stricter compaction behavior: unknown future operations
  are retained, and exact chat tombstones remain load-bearing against stale restores.
  Time or current-peer acknowledgement alone never authorizes tombstone deletion.
- Desktop backups now carry Core's compact deletion graveyard and restore it before
  stale chat events. Backup settings include an advanced, strongly warned action to
  forget those records when the user accepts the resurrection risk.

- Desktop can send and receive generic `FILE` messages. Local files are streamed
  into Core's content-addressed store, synced as opaque blobs, shown with filename
  and size, and saved with their original display name without routing large files
  through base64 IPC.
- Device settings now explain guest-network client isolation and multicast
  filtering, clarify that Ncy does not scan the subnet, and show a private-LAN or
  hotspot suggestion beside failed peer connections.
- The discoverability toggle now states its address-recovery effect in both states
  and warns that disabling it leaves paired devices on the old endpoint after DHCP
  changes until discovery is restored or the endpoint is repaired manually.
- Desktop mDNS advertisements now use a short random installation-ID label instead
  of broadcasting the configured device name or operating-system hostname. Pairing
  payloads and direct peer handshakes retain the friendly name.
- Paired-device cards can now rotate a credential in place or revoke/remove it with
  an affected-device confirmation. Rotation keeps the slot/label and chats, clears
  the former owner/watermark, publishes a new QR, and promptly disconnects the old
  session. Rejected outbound credentials stop retrying and request re-pairing.
- Paired-device cards now show the authenticated phone name for claimed inbound
  slots and append a stable eight-character device-ID fingerprint when inbound or
  outbound names collide case-insensitively. Unique names remain unchanged.
- Outbound device names now refresh from Core's encrypted authenticated `Welcome`;
  claimed inbound phone names refresh from authenticated `Hello`. User-assigned
  pairing-slot labels and outbound aliases remain separate and unchanged; only a
  system-derived label that followed the previous authenticated name is refreshed.
- Outbound peer cards now expose last discovery, last authenticated endpoint/time,
  retry scheduling, last error, and an honest reachable/possibly-blocked LAN status.
  Diagnostics reuse passive discovery and existing connections without extra probes.

- Desktop sync now inherits Core's mandatory PSK-derived AES-256-GCM transport for
  both inbound and outbound connections. Welcome, events, acknowledgements, errors,
  and media are encrypted with directional keys and replay-safe sequence numbers;
  legacy plaintext peers fail clearly instead of downgrading.
- Desktop now inherits Core's authenticated endpoint promotion and mDNS anti-flapping
  policy, so spoofed discovery records cannot replace persisted peer addresses.
- Desktop now honors an empty/reinstalled peer's full-replay request instead of
  suppressing history with its cached acknowledgement watermark. Deleted-chat
  tombstones and the peer's own historical events are replayed during recovery.
- Enabled the shared Core endpoint-rediscovery behavior for outbound desktop
  peers, allowing paired devices to reconnect after a LAN IP-address change.
- Added the desktop's configured friendly name to outbound sync handshakes.
- Desktop mDNS advertisements now declare support for interactive pairing
  requests.
- Desktop sync now inherits Core's bounded WebSocket, blob, chunk, and pending-
  transfer limits.
- Desktop media sync now streams files through Core instead of buffering complete
  blobs in memory.
- Stalled incoming media transfers now expire through Core after 60 seconds.
- Desktop sync now inherits deterministic LAN-interface selection, IPv6-safe
  outbound URLs, and IPv6-only listener binding from Core.
- Imported phone-host pairings now retain foreground-only availability, display
  **Open Ncy** while offline, and inherit immediate mDNS retry wakeups from Core.
- Added **Pause/Resume** controls for each outbound peer; paused peers retain their
  pairing but no longer create Core connection or retry tasks.
- Desktop now inherits Core clock-skew rejection and watermark recovery, and shows
  the peer's clock mismatch in the device list instead of silently appearing offline.
- Backup import now pauses live sync, clears and persists local progress watermarks
  before applying events, rejects invalid or future-skewed backup clocks, and always
  restores the prior sync state. Portable ZIPs remain free of sync identity and trust.
- Desktop now inherits Core detection for cloned local IDs, duplicate live remote
  IDs, pairing-credential ownership mismatches, and unexpected server identities.
- Desktop now inherits Core's lossless snapshot/live handoff and overlap filtering
  in both its inbound server and outbound client roles.
- Desktop now inherits deterministic three-device conflict behavior and stable
  identities for locked encrypted edits, deletes, and compaction.
- Pairing views now label JSON as a credential, clear unchanged copied payloads after
  60 seconds, and clear a successfully imported paste immediately. Desktop also
  inherits Core's owner-only Unix permissions for secret-bearing state files.

### Changed

- Updated the in-app release notes to mention automatic LAN rediscovery and
  incoming device names.
- Updated the in-app release notes to mention IPv6-only LAN support.
- Updated the in-app release notes to explain foreground-only phone hosts.
- Updated the in-app release notes to mention per-device sync pause controls.
- Updated the in-app release notes to mention clock-mismatch protection.
- Updated the in-app release notes to mention safe backup replay.
- Updated the in-app release notes to mention cloned-device detection.
- Updated the in-app release notes to mention lossless catch-up handoff.
- Updated the in-app release notes to explain automatic conflict resolution.
- Updated the in-app release notes to mention pairing-credential safeguards.
- Updated the in-app release notes to mention encrypted LAN sync traffic.
- Updated the in-app release notes to mention authenticated discovery changes.
- Updated the in-app release notes to mention complete reinstall recovery.

### Verification

- Successfully compiled the compact message-tombstone backup/purge integration with
  `cargo check`; `svelte-check` reports zero errors and zero warnings.
- Successfully compiled with `cargo check`.
- Passed `svelte-check` with zero errors or warnings after adding generic-file
  send, receive, and save controls.
- Passed `svelte-check` with zero errors and zero warnings after duplicate-name
  disambiguation was added.
- Passed `git diff --check`.
