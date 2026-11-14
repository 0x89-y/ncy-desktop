# Changelog

## Unreleased

### Removed

- Removed Pear sync. The server syncs over the local network only, pairing payloads no
  longer carry `pearPublicKey`, and `THIRD_PARTY_NOTICES_PEAR.md` is gone. Existing
  configs that still set `sync_transport_mode`, `pear_runtime_path` or
  `pear_bundle_path` load as before; those keys are ignored.

## 0.7.4 — 2026-10-20

### Added

- Events the server receives are now relayed to its other connected devices straight
  away, instead of waiting for them to reconnect.

### Fixed

- Fixed notes written on a device while it was offline never reaching devices that had
  synced in the meantime: catch-up now follows the order the server stored events in,
  not their timestamps.
- Fixed a refusal that arrives before the connection is encrypted stopping outbound
  reconnection for good.

## 0.7.2 — 2026-07-24

### Added

- Added a configurable automatic file-download limit for server-managed transfers.

### Changed

- Increased the temporary absolute file-transfer ceiling to 1 GiB.

### Fixed

- Fixed the bundled Bare runtime remaining active after the server had fully exited.
- Updated shared authentication and encryption code to remove deprecated generic-array API
  warnings.

## 0.7.1 — 2026-07-22

### Changed

- Updated the shared Core and Desktop integration for the 0.7.1 release.
- Restricted LAN discovery advertisements to the selected network interface to avoid
  recurring errors from unrelated virtual adapters.

## 0.7.0 — 2026-07-19

### Added

- Added opt-in Pear synchronization with Local only, Pear only, Local preferred, and Pear
  preferred configuration modes.
- Added a stable server Pear identity, supervised sidecar startup/shutdown, and gateway
  routing through the existing authenticated and encrypted sync protocol.
- Added Pear public keys to generated and rotated pairing QR/JSON payloads.
- Added explicit Bare runtime and worker-bundle configuration while retaining Local only as
  the default.
- Exposed the daemon and administration CLI as a reusable library so Ncy Desktop can offer
  integrated `server` and `--server` modes without duplicating server behavior.
- Added an interactive server console for pairing and diagnostics in the log terminal, plus
  automatic pairing-file reload for administration from a separate process.

### Security

- Pear identity seeds are persisted privately, and pairing credentials remain authoritative
  inside the Pear transport.

## Unreleased — 2026-07-11

### Added

- Server now inherits Core's compact indexed message-deletion ledger, reconstructed
  catch-up events, pre-storage stale-body rejection, and expanded explicitly confirmed
  purge warning for deleted chats and messages.

- Server now preserves Core's message-creator installation metadata and synced
  per-chat remote-message alignment preference while relaying, materializing, and
  compacting histories. Clients can derive stable per-device colors locally from that
  metadata; the headless server applies no presentation behavior itself.

- Server inherits Core's stricter compaction behavior and stale-restore regressions:
  unknown future operations survive compaction and exact deletion IDs are retained
  until an explicit history-generation design can make their removal safe.
- Server now compacts whole-chat deletions into Core's exact restart-safe graveyard,
  replays them during catch-up, and offers the deliberately verbose
  `purge-tombstones --yes-i-understand` escape hatch.

- Server now inherits Core support for preserving and relaying generic `FILE`
  messages and their verified opaque blobs under the existing 100 MB transfer limit.
- Server operator guidance now covers guest-network client isolation, blocked mDNS,
  saved-endpoint-only retries, the absence of subnet scanning, and the
  discoverability requirement for recovery after LAN address changes.
- `discoverable=false` now carries the same warning in example configuration,
  startup logs, and pairing output: explicit pairing works, but stale LAN addresses
  require stable DNS/IP, manual repair, or re-enabled mDNS.
- Server mDNS advertisements now use a short random installation-ID label rather
  than broadcasting the configured server name or host name. Pairing output retains
  the operator-selected friendly name.
- Added `pairings`, `rotate --id`, and `revoke --id` credential-management commands.
  Rotation preserves the slot/label and chats while replacing its secret; revocation
  removes only the pairing record. CLI output explicitly requires restarting a
  running daemon so offline pairing-file changes take effect.
- `pairings` now appends a stable short device-ID fingerprint when multiple claimed
  phones have the same case-insensitive name, without exposing pairing secrets.
- Server now inherits authenticated device-name refresh in both directions. Blank,
  oversized, unauthenticated, or failed-handshake names cannot replace stored peer
  metadata. Authenticated names are stored separately so operator aliases and
  pairing labels remain unchanged.
- Added a secret-free `peers` command showing saved endpoints, last passive discovery,
  last authenticated endpoint/time, next retry, and bounded last error. Core records
  these from existing activity without diagnostic probes.

- Server sync now inherits Core's mandatory PSK-derived AES-256-GCM transport for
  inbound and outbound sessions. All post-Hello application frames are encrypted,
  strictly sequenced, and authenticated; legacy plaintext clients receive an upgrade
  error and cannot trigger a downgrade.
- Outbound server peers now inherit Core's expiring, rate-limited mDNS candidates
  and promote a changed address only after encrypted authentication succeeds.
- Server catch-up now treats a client's empty watermark as an authoritative full
  replay, including deletion tombstones and that client's own historical events.
- Enabled the shared Core endpoint-rediscovery behavior for configured outbound
  peers, allowing paired devices to reconnect after a LAN IP-address change.
- Added the server's configured friendly name to outbound sync handshakes.
- Server mDNS advertisements now declare that pairing requires QR/JSON, avoiding
  unsupported interactive pair requests from discovered clients.
- Server sync now inherits Core's bounded WebSocket, blob, chunk, and pending-
  transfer limits.
- Server media sync now streams files through Core instead of buffering complete
  blobs in memory.
- Stalled incoming media transfers now expire through Core after 60 seconds.
- Server sync now inherits deterministic LAN-interface selection, IPv6-safe
  outbound URLs, and IPv6-only listener binding from Core.
- Configured outbound phone peers can retain foreground-only availability and
  inherit immediate trusted-mDNS retry wakeups from Core.
- Outbound peers with `"disabled": true` are now retained without starting a
  connection or retry task; missing legacy values remain enabled.
- Server sync now rejects remote events and watermarks more than five minutes ahead,
  logs the peer error, and clears poisoned legacy peer/pairing watermarks.
- Server sync now rejects its own device ID, simultaneous authenticated sessions
  claiming one remote ID, credentials bound to another ID, and unexpected outbound
  server identities. Identity failures are logged and suspend timed retries.
- Server sync now subscribes before catch-up snapshots in both roles, preserving
  events authored during handoff while filtering snapshot/live overlap.
- Server materialization now inherits tested three-device edit/delete/profile rules,
  safety-first protected conflicts, and stable locked-message compaction identities.
- `ncy-server pair` now warns that its JSON/QR output is a bearer credential, while
  Core writes pairing and peer state with owner-only Unix permissions.

### Changed

- Documented automatic mDNS recovery after DHCP address changes, including its
  same-LAN and discoverability requirements.

### Verification

- Successfully compiled against Core's compact message-tombstone and expanded purge API.
- Successfully compiled with `cargo check`.
- Recompiled successfully against Core's final generic-file message/blob model.
- Passed 2 server CLI unit tests covering duplicate-name matching and stable
  fingerprint formatting; `cargo fmt --check` passes.
- Passed `git diff --check`.
