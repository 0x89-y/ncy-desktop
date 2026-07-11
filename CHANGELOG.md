# Changelog

## Unreleased — 2026-07-11

### Added

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

### Changed

- Updated the in-app release notes to mention automatic LAN rediscovery and
  incoming device names.
- Updated the in-app release notes to mention IPv6-only LAN support.
- Updated the in-app release notes to explain foreground-only phone hosts.
- Updated the in-app release notes to mention per-device sync pause controls.
- Updated the in-app release notes to mention clock-mismatch protection.

### Verification

- Successfully compiled with `cargo check`.
- Passed `git diff --check`.
