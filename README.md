# Ncy Desktop

A note-to-self chat app for Windows. Notes are stored locally and can optionally sync
with other Ncy devices on the same network over encrypted WebSockets.

Built with Tauri 2 (Rust) and Svelte 5 + TypeScript.

## Download

Windows installers are published on the
[Releases page](https://github.com/0x89-y/ncy-desktop/releases/latest). Installed copies
update themselves: they check on startup and every 3 hours, or immediately via
**Settings → About → Check now**.

Only Windows builds are produced. The code is platform-portable (Tauri 2), so it should
build on macOS and Linux, but that is untested.

## Build from source

Prerequisites:

- Node.js 22+ and npm
- Rust stable and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
  (WebView2 on Windows, preinstalled on Windows 10/11)
- A checkout of [`ncy-core`](https://github.com/0x89-y/ncy-core) next to this repository
  (`../ncy-core`), which holds the event log, sync engine and storage

```bash
npm install
npm run tauri:dev     # development build with hot reload
npm run tauri:build   # unsigned release build in target/release/
```

The first build compiles a lot of Rust crates and takes a few minutes.

## Headless client

The desktop executable can run the same client profile without opening a window.
`--no-gui` is an alias for `client run`:

```powershell
ncy-desktop.exe --no-gui
ncy-desktop.exe client run
ncy-desktop.exe client status
ncy-desktop.exe client peers
ncy-desktop.exe client sync --seconds 30
ncy-desktop.exe client pair create --name "My phone"
ncy-desktop.exe client pair add --file pairing.json
```

`run` stays in the foreground until Ctrl+C. `sync` runs the client for a bounded window
(15 seconds by default), while `status` and `peers` only inspect the profile. The pairing
JSON printed by `pair create` can be turned into a QR code or imported directly on
another Ncy device.

The GUI and the headless client share the same profile, so do not run both at the same
time. Set `NCY_DATA_DIR` to use an isolated profile, for example in tests.

## Server mode

The same executable also runs the Ncy server daemon from
[`crates/ncy-server`](crates/ncy-server/README.md) and its administration commands:

```powershell
ncy-desktop.exe --server
ncy-desktop.exe server --config C:\path\to\config.toml run
ncy-desktop.exe server --config C:\path\to\config.toml pair --name "My phone"
ncy-desktop.exe server --config C:\path\to\config.toml pairings
ncy-desktop.exe server --config C:\path\to\config.toml peers
```

Server mode uses the standalone server's config format. It defaults to
`%APPDATA%\ncy-server\config.toml` with data under `%APPDATA%\ncy-server\data`, so it has
its own identity and store, separate from the desktop app. While it runs interactively,
type `help` in the same terminal; commands include `pair My Phone`, `pairings`, `peers`,
`rotate <id>`, `revoke <id>` and `stop`, and pairing changes apply without a restart.

## Storage

Everything lives in the app-data directory, `%APPDATA%\com.ncy.desktop\ncy\` on Windows:

- `identity.json`: this desktop's stable `deviceId` and display name
- `pairings.json`: pairings, their secrets and per-pairing sync positions
- `peers.json`: other desktops this one connects to
- `settings.json`: port, device name, appearance and sync preferences
- `events/<chatId>.ndjson`: one append-only event log per chat
- `blobs/`: content-addressed media, shared across chats by hash

## Protocol

The sync protocol is documented in
[`ncy-android/docs/sync-protocol.md`](https://github.com/0x89-y/ncy-android/blob/main/docs/sync-protocol.md).
The Rust implementation is in `ncy-core`.

## Layout

```
src-tauri/            Tauri app: entry point, app state, tray, commands
├── src/lib.rs        startup, AppState, plugins, tray
├── src/commands.rs   Tauri commands called by the frontend
├── src/cli.rs        headless client
└── src/main.rs       chooses GUI, headless client or server mode

crates/ncy-server/    standalone server binary and library

src/                  Svelte frontend
└── lib/
    ├── ipc.ts        typed wrappers around Tauri invoke/listen
    ├── stores/       global stores and bootstrap
    ├── components/   chat list, chat view, message bubbles, devices pane, …
    ├── views/        settings
    └── changelog.ts  release notes shown in Settings → About
```

## License

[MIT](LICENSE)
