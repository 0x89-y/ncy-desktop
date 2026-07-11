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

### Changed

- Updated the in-app release notes to mention automatic LAN rediscovery and
  incoming device names.

### Verification

- Successfully compiled with `cargo check`.
- Passed `git diff --check`.
