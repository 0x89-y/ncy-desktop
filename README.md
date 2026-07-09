# Ncy Desktop

Desktop client for [Ncy](https://github.com/0x89-y/Ncy), the Android "note-to-self"
chat app. Implements the full Ncy sync protocol — **bidirectional**, near-real-time,
peer-to-peer over the LAN via WebSocket + HMAC-SHA256 challenge/response. The phone
and desktop talk directly.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** + **TypeScript** (web frontend).

## Limitations

Sync runs over the LAN only — there's no off-LAN relay, so both devices must be
on the same network.

## Prerequisites

- Node.js 18+ and npm
- Rust stable + WebView2 (the Windows Tauri dependency, preinstalled on Win10/11),
  per the Tauri docs: <https://v2.tauri.app/start/prerequisites/>

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

## Build a release

Icons are already committed under `src-tauri/icons/`, so just:

```bash
npm run tauri:build
```

Output lands in `src-tauri/target/release/`:

- `ncy-desktop.exe` — standalone binary (needs WebView2, preinstalled on Win10/11)
- `bundle/nsis/…-setup.exe`, `bundle/msi/….msi` — installers

> Note: `target/` gets large (multi-GB) — it's the Rust build cache and is
> gitignored. `cargo clean` from `src-tauri/` reclaims it (slow next build).

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
[`docs/sync-protocol.md`](https://github.com/0x89-y/Ncy/blob/main/docs/sync-protocol.md)
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
│   ├── commands.rs              # Tauri commands (list/get, author_*, unlock_chat, …)
│   ├── network.rs               # LAN IP detection
│   ├── model/                   # wire-shape data types (event, hlc, message, …)
│   ├── storage/                 # event log, materializer, compaction, identity
│   └── sync/                    # WS server, HMAC, wire codec, mDNS, blobs,
│                                #   outbound author pipeline, protection (crypto +
│                                #   in-memory key store)
└── tauri.conf.json

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
