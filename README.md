# Ncy Desktop

Desktop client for [Ncy](https://github.com/0x89-y/ncy-android), the Android "note-to-self"
chat app. Implements the full Ncy sync protocol — **bidirectional**, near-real-time,
peer-to-peer over authenticated, encrypted LAN WebSockets.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** + **TypeScript** (web frontend).

## Prerequisites

- Node.js 18+ and npm
- Rust stable + WebView2 (the Windows Tauri dependency, preinstalled on Win10/11),
  per the Tauri docs: <https://v2.tauri.app/start/prerequisites/>
- A sibling `../ncy-core` checkout (path dependency)

**Platform support:** built and tested on **Windows** only. The codebase is
platform-portable (Tauri 2, no platform-specific code), so it *should* build on
macOS and Linux with the prerequisites above — but that's **unverified**: no CI,
and the packaging path (`.dmg`, `.AppImage`, code-signing) is untried.

## Run from source

```bash
npm install
npm run tauri:dev
```

The first build pulls a lot of Rust crates and takes a few minutes. Subsequent
runs are fast.

## Headless client

The desktop executable can run the same client profile without opening Tauri or
creating a window. `--no-gui` is an alias for the long-running client mode:

```powershell
ncy-desktop.exe --no-gui
ncy-desktop.exe client run
ncy-desktop.exe client status
ncy-desktop.exe client peers
ncy-desktop.exe client sync --seconds 30
ncy-desktop.exe client pair create --name "My phone"
ncy-desktop.exe client pair add --file pairing.json
```

`run` stays in the foreground until Ctrl+C. `sync` runs the normal
client stack for a bounded window (15 seconds by default), while `status` and
`peers` only inspect the profile. Pairing JSON printed by `pair create` can be
turned into a QR code or imported directly on another Ncy device.

The GUI and CLI deliberately share `%APPDATA%\com.ncy.desktop\ncy`: identity,
settings, notes, pairings, and sync watermarks are the same. Do not run them at
the same time; the first version does not yet coordinate concurrent profile
ownership. `NCY_DATA_DIR` may point development and automated tests at an
isolated profile.

## Server mode

The same executable can also run the existing Ncy server daemon and its
administration commands:

```powershell
ncy-desktop.exe --server
ncy-desktop.exe server --config C:\path\to\config.toml run
ncy-desktop.exe server --config C:\path\to\config.toml pair --name "My phone"
ncy-desktop.exe server --config C:\path\to\config.toml pairings
ncy-desktop.exe server --config C:\path\to\config.toml peers
```

Server mode retains the standalone server's config format and defaults to
`%APPDATA%\ncy-server\config.toml` with runtime data under
`%APPDATA%\ncy-server\data`. It therefore has a separate identity and store from
the GUI/CLI client.
While server mode is running interactively, type `help` in that same terminal.
Commands include `pair My Phone`, `pairings`, `peers`, `rotate <id>`,
`revoke <id>`, and `stop`; pairing changes become active without restarting.

## Build a release

Releases are signed so the in-app updater will accept them. Generate the signing
key once:

```bash
npx tauri signer generate -w "$USERPROFILE/.ncy/ncy-updater.key"
```

Keep `ncy-updater.key` private and backed up — losing it means existing installs
can never be updated again, since they only trust its public half (baked into
`tauri.conf.json` as `plugins.updater.pubkey`).

### GitHub release (normal path)

The [Release workflow](.github/workflows/release.yml) builds the signed installer on
`windows-latest` and stages a **draft** GitHub release holding
`Ncy-windows-x64-setup.exe`, `latest.json` and the CHANGELOG.md section as the body.
It checks out `0x89-y/ncy-core` next to this repo, the same sibling layout as local dev.

One-time setup, under the repo's Settings → Secrets and variables → Actions:

- `TAURI_SIGNING_PRIVATE_KEY`: the contents of `ncy-updater.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password (skip if it has none)
- `NCY_CORE_TOKEN`: only while `ncy-core` is private, a fine-grained token with
  read access to its contents

To release:

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` together, add the `CHANGELOG.md` section and the
   `src/lib/changelog.ts` entry, and commit.
2. `git tag v<version>` and `git push origin v<version>`. The workflow refuses a tag
   that doesn't match the app version.
3. Check the draft on GitHub, then publish it. Only then do installed copies see it.

The updater reads `releases/latest/download/latest.json`, which always resolves to
the newest published, non-prerelease release. The workflow can also be run by hand
from the Actions tab, optionally against a specific `ncy-core` ref; rerunning it for
the same version replaces the draft's files.

### Local build (fallback)

From the project root:

```bat
build.bat            :: NSIS installer + .sig  (prompts for the key password)
publish-release.bat  :: stages release\ with the installer + latest.json
```

`build.bat` output lands in `target/release/`:

- `ncy-desktop.exe` — standalone binary (needs WebView2, preinstalled on Win10/11)
- `bundle/nsis/Ncy_<version>_x64-setup.exe` — installer, plus its `.sig`

NSIS uses LZMA compression.

`publish-release.bat` renames the installer to the stable asset name and writes
`latest.json` with the signature already inlined. Attach both files from `release/`
to a GitHub release tagged `v<version>`. Installed copies pick the update up on
their next check (startup, then every 3 hours), or immediately via
**Settings → About → Check now**.

Bump the version in all three files the same way as above; `src/lib/changelog.ts`
is what Settings → About renders.

`npm run tauri:build` still works for an unsigned local build, but the resulting
installer can't be published as an update.

> Note: `target/` gets large (multi-GB) — it's the shared Rust workspace build
> cache and is gitignored. `cargo clean` from the repository root reclaims it
> (slow next build).

## Storage

App-data dir (`%APPDATA%\com.ncy.desktop\ncy\` on Windows), under `ncy/`:

- `identity.json` — this desktop's stable `deviceId` and display name
- `pairings.json` — pairings, their secrets, and per-pairing sync watermarks
- `settings.json` — port, device name, theme, `discoverable` (mDNS opt-in)
- `events/<chatId>.ndjson` — one append-only event log per chat, byte-identical
  to the phone's `events.ndjson` lines
- `blobs/<sha256>.<ext>` — content-addressed media (voice/image), shared across
  chats by hash

## Protocol

The wire protocol is documented in the Ncy repo at
[`docs/sync-protocol.md`](https://github.com/0x89-y/ncy-android/blob/main/docs/sync-protocol.md)
— handshake, HLC, event/blob/Ack frames, watermark semantics, and the phase-3
encryption envelope. The Rust implementation lives under `src-tauri/src/sync/`
and `src-tauri/src/storage/`.

The Python reference server (`scripts/sync_test_server.py` in the Ncy repo) is
useful for sanity-checking phone-side changes without booting this client; it
also has a `--reply` echo mode for exercising the phone's inbound path.

## Layout

```
src-tauri/                       # Rust + Tauri
├── src/
│   ├── lib.rs                   # entrypoint, AppState, plugins, tray, command reg
│   ├── main.rs
│   ├── cli.rs                   # headless client commands and foreground runtime
│   ├── commands.rs              # Tauri commands (list/get, author_*, unlock_chat, …)
│   ├── network.rs               # LAN IP detection
│   ├── model/                   # wire-shape data types (event, hlc, message, …)
│   ├── storage/                 # event log, materializer, compaction, identity
│   └── sync/                    # WS server, HMAC, wire codec, mDNS, blobs,
│                                #   outbound author pipeline, protection (crypto +
│                                #   in-memory key store)
└── tauri.conf.json

crates/ncy-server/              # standalone server binary + reusable server library

src/                             # Svelte frontend
├── App.svelte
├── main.ts
├── app.css
└── lib/
    ├── ipc.ts                   # typed wrappers around Tauri invoke/listen
    ├── markdown.ts              # compose-box rich-text helpers
    ├── format.ts
    ├── changelog.ts             # release notes shown in Settings → About
    ├── stores/app.ts            # global stores, bootstrap, notification dispatch
    ├── components/              # ChatList(+Item/+Pane), ChatView, MessageBubble,
    │   │                        #   DevicesPane, RichText, BlobImage, Avatar, …
    │   └── chatpane/            # SearchPane, StarredPane, RemindersPane, HitRow
    └── views/                   # SettingsView
```

## License

Same as the Ncy project.
