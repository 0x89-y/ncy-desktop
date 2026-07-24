# ncy-server

A headless member of the [Ncy](https://github.com/0x89-y) sync mesh. It runs the
same WebSocket sync engine as the desktop client — **both** server and client
roles — with no UI, and (optionally) auto-transcribes inbound voice messages with
a local `whisper-cli`. Deploy it on a NAS, VPS, or Raspberry Pi so a phone keeps
converging, and voice notes keep getting transcripts, even when no desktop is
running.

It shares all of its protocol and storage code with the desktop via the
[`ncy-core`](https://github.com/0x89-y/ncy-core) crate, so anything it stores or relays is byte-compatible
with the phone and desktop.

The server can run either as the standalone `ncy-server` binary or through an
Ncy Desktop installation with `ncy-desktop server` / `ncy-desktop --server`.
Both entry points use this same implementation and configuration format; Desktop
only supplies its packaged Pear artifacts as defaults when config paths are omitted.

## What it does

- **Full mesh member, not a relay.** It joins like any paired device: it holds
  the append-only event log, serves catch-up on (re)connect, and forwards live
  events both directions. Two clients of this hub converge through its catch-up
  stream. (See the [sync protocol](https://github.com/0x89-y/ncy-desktop/blob/main/docs/sync-protocol.md).)
- **Auto-transcription.** When enabled, it watches for inbound voice messages,
  waits for the audio blob to arrive, runs `whisper-cli`, and re-publishes the
  message with a `transcript` — which then syncs to every paired device. Same
  path the desktop uses; last-write-wins if two devices race.
- **Protected chats: relayed, never read.** Encrypted (`isProtected`) messages
  are stored and forwarded faithfully, but the server holds no PIN-derived key,
  so it never decrypts or transcribes them. Your desktop still can, on demand.

## Build

```sh
cargo build --release -p ncy-server
# binary at the workspace root: target/release/ncy-server
```

`ncy-server` lives in the Ncy Desktop Cargo workspace and depends on the sibling
`ncy-core` repository:

```
some-dir/
  ncy-core/
  ncy-desktop/
    crates/ncy-server/
```

For a static Linux build (portable across distros), target musl:

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release -p ncy-server --target x86_64-unknown-linux-musl
```

## Configure

Copy [`config.example.toml`](config.example.toml) to the default config path and
edit it:

| Platform     | Default config path                        | Default `data_dir`         |
|--------------|--------------------------------------------|----------------------------|
| Linux/macOS  | `/etc/ncy-server/config.toml`              | `/var/lib/ncy-server`      |
| Windows      | `%APPDATA%\ncy-server\config.toml`         | `%APPDATA%\ncy-server\data`|

Or pass `--config /path/to/config.toml`. If no config exists, the daemon boots
on defaults (with a warning).

## Pair a phone

```sh
ncy-server --config /etc/ncy-server/config.toml pair --name "My Phone"
```

This prints the pairing JSON on **stdout** (a QR + guidance go to stderr, so you
can redirect the JSON: `... pair > pairing.json`) and registers the pairing under
`data_dir/pairings.json`. On the phone: **Add device → scan the QR or paste the
JSON**. The phone connects, authenticates over the HMAC handshake, and starts
syncing. Run `pair` again per device you want to add.

The running daemon watches `pairings.json`, so these commands take effect for new
connections without a restart:

```sh
ncy-server --config /etc/ncy-server/config.toml pairings
ncy-server --config /etc/ncy-server/config.toml replace --id <pairing-id>
ncy-server --config /etc/ncy-server/config.toml revoke --id <pairing-id>
ncy-server --config /etc/ncy-server/config.toml peers
ncy-server --config /etc/ncy-server/config.toml purge-tombstones --yes-i-understand
```

`replace` refreshes only an unused invitation and invalidates its previous QR/JSON.
Claimed devices must be revoked and paired again instead. `revoke` removes the slot but never deletes chats. A connection
that was already authenticated is allowed to finish; rotation or revocation governs
new and reconnecting sessions. Clients distinguish the resulting permanent
credential rejection from ordinary offline/network errors and stop retrying.
`peers` prints secret-free outbound connection diagnostics. Times use Unix
milliseconds so scripts can consume them; `-` means the event has not occurred.
`purge-tombstones` permanently forgets compact deleted-chat records. Stop the daemon
first and use it only when every stale device and backup is gone: old history can
otherwise restore deleted chats. The acknowledgement flag is intentionally required.

The server must be reachable at the `host:port` in the payload — `host` is
auto-detected as your best LAN IP. On a VPS behind NAT, edit the printed `host`
to your public address (and forward the port) before pasting into the phone.

On a LAN, enabling `discoverable` lets paired Ncy clients find the server again
by its stable device ID after DHCP changes its IP address. The client updates
only the saved endpoint; the existing pairing secret remains authoritative, so
no new QR code is needed. This does not replace DNS or a stable public address
for connections across the internet. The multicast-visible service label is
`Ncy <short random installation ID>`; it does not expose `device_name` or the OS
hostname. The configured friendly name remains in explicit pairing data and direct
peer handshakes.

Public/guest Wi-Fi and enterprise networks may enable client isolation, block
peer-to-peer TCP, or filter mDNS multicast. Sharing an SSID therefore does not prove
that two devices can reach one another. Ncy does not scan the subnet: each enabled
outbound peer tries only its saved endpoint while mDNS passively looks for its stable
device ID. If a paired client stays offline, use a private LAN/hotspot or ask the
network administrator to permit direct traffic and multicast. Disabling
`discoverable` stops mDNS advertising but also prevents automatic recovery after a
LAN address change.

## Pear sync (optional)

Pear is off by default. To connect already-paired devices across different
networks, set `sync_transport_mode` to `pearOnly`, `localPreferred`, or
`pearPreferred`, and configure `pear_runtime_path` plus `pear_bundle_path` as
shown in `config.example.toml`. `localOnly` never launches the Pear process.

Start the daemon once so it creates its stable Pear identity, then run
`ncy-server pair`. The generated QR/JSON includes `pearPublicKey`. Pear does not
discover or authorize unknown Ncy devices; the existing QR/JSON pairing secret
still controls access and protects the Ncy sync session.

Keep the bundle's adjacent `out/node_modules` directory with it, since the
packaged worker loads its native addons there. Changing or deleting
`<data_dir>/pear-seed.txt` changes the server's Pear identity and requires new
pairing payloads.

Distributions must also include the repository's `THIRD_PARTY_NOTICES_PEAR.md`, generated
with `npm run notices` in ncy-core's `pear-sidecar` directory. It contains a production
dependency inventory and the license and notice texts supplied by the bundled packages.

## Run

```sh
ncy-server --config /etc/ncy-server/config.toml run   # `run` is the default
```

When attached to a terminal, the running server accepts administration commands
on stdin. Type `help` to list them. Common commands are `pair My Phone`,
`pairings`, `peers`, `replace <id>`, `revoke <id>`, and `stop`. `pair` prints its
JSON and QR directly in the same terminal as the server logs. Services with no
interactive stdin continue running normally and can be administered from another
process through the commands above.

## Transcription setup (optional)

Set `transcription_enabled = true`, then install into `whisper_dir` (default
`<data_dir>/whisper`):

- **`whisper-cli`** (`.exe` on Windows) from a
  [whisper.cpp release](https://github.com/ggerganov/whisper.cpp/releases).
  Extract the **whole** archive — the CLI links against sibling shared libraries
  (`whisper`, `ggml`, `ggml-base`, `ggml-cpu`, …). Shipping just the binary fails
  at load time.
- **`ggml-tiny.bin`** — the multilingual tiny model (~74 MB), from
  [HuggingFace](https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin).
- **`ffmpeg`** *(optional)* — only needed to decode legacy `webm/opus` voice
  blobs. Phone-recorded (AAC/MP4) audio decodes without it via the built-in
  symphonia decoder.

If `transcription_enabled` is true but these are missing, the server logs a
warning and runs as a pure relay (no transcripts) — it does not crash.

## Deploy as a service

Templates live in [`packaging/`](packaging):

- **systemd** (Linux): edit paths in `packaging/ncy-server.service`, then
  ```sh
  sudo cp target/release/ncy-server /usr/local/bin/
  sudo cp packaging/ncy-server.service /etc/systemd/system/
  sudo systemctl enable --now ncy-server
  ```
- **launchd** (macOS): edit `packaging/one.eveo.ncy-server.plist`, copy to
  `~/Library/LaunchAgents/`, then `launchctl load` it.

Logs go to stdout/stderr (captured by the service manager). No auto-updater in
v1.
