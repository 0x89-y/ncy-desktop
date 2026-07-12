# Changelog

## Unreleased — 2026-07-11

### Added

- Desktop sync now inherits Core's mandatory PSK-derived AES-256-GCM transport for
  both inbound and outbound connections. Welcome, events, acknowledgements, errors,
  and media are encrypted with directional keys and replay-safe sequence numbers;
  legacy plaintext peers fail clearly instead of downgrading.
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

### Verification

- Successfully compiled with `cargo check`.
- Passed `git diff --check`.
