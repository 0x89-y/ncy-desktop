# Ncy Desktop

Desktop client for [Ncy](https://github.com/0x89-y/Ncy), the Android "note-to-self"
chat app. Implements the phase-1 sync protocol — phone (sender) → desktop (receiver)
over LAN via WebSocket + HMAC-SHA256 challenge/response.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** + **TypeScript** (web frontend).

## Status

Phase 1 receiver, feature-coverage-first:

- ✅ WebSocket sync server with HMAC pairing handshake
- ✅ Per-chat append-only NDJSON event log + idempotent dedup by `eventId`
- ✅ Materializer with HLC ordering, tombstones, and LWW per chat metadata
- ✅ Multi-pairing support (each phone gets its own pre-shared secret)
- ✅ QR code + JSON pairing payload, LAN IP picker
- ✅ Chat list with pinned / archived / tags / search
- ✅ Chat view with rich-text rendering (paragraphs, headings, bullets, code blocks,
   bold/italic/underline/strike/color/links)
- ✅ Voice / image / checklist message rendering (media files themselves aren't
   transferred — phase 1 sends references only)
- ✅ Starred messages view (across all chats)
- ✅ Reminders view (upcoming + past)
- ✅ Settings: device name, port, theme
- ✅ Live updates as events stream in
- ❌ Sending edits back to the phone (out of scope for phase 1)
- ❌ Encrypted protected chats (phase 3)
- ❌ Media blob transfer (phase 2)

## Prerequisites

- Node.js 18+ and npm
- Rust stable + the platform's WebView2/webkitgtk dependencies, per the Tauri docs:
  <https://v2.tauri.app/start/prerequisites/>

## Run from source

```bash
npm install
npm run tauri:dev
```

The first build pulls a lot of Rust crates and takes a few minutes. Subsequent
runs are fast.

## Build a release

Drop a square PNG at `src-tauri/icons/icon.png` (1024×1024 recommended), then:

```bash
npm run tauri icon src-tauri/icons/icon.png
npm run tauri:build
```

Installers land in `src-tauri/target/release/bundle/`.

## Pairing a phone

1. Open the desktop app, go to **Devices**, click **Refresh IPs** if needed and
   pick the LAN IP your phone can reach.
2. Click **Create** to make a new pairing slot. Each slot has its own 32-byte
   pre-shared secret — do not share QR codes between phones.
3. On the phone: **Ncy → Settings → Sync devices → Paste pairing JSON** (or scan
   the QR shown beside the JSON).
4. Foreground the phone app. Events stream in immediately; the chat list updates
   as they arrive.

## Storage

Per-OS app data dir, under `ncy/`:

- `identity.json` — this desktop's stable `deviceId` and display name
- `pairings.json` — all created pairings + their secrets
- `settings.json` — port, name, theme
- `events/<chatId>.ndjson` — one append-only event log per chat, byte-identical to
  the phone's `events.ndjson` lines

Locations:

| OS      | Path |
|---------|------|
| Windows | `%APPDATA%\com.ncy.desktop\ncy\` |
| macOS   | `~/Library/Application Support/com.ncy.desktop/ncy/` |
| Linux   | `~/.local/share/com.ncy.desktop/ncy/` |

## Protocol

The wire protocol is documented in the Ncy repo at
[`docs/sync-protocol.md`](https://github.com/0x89-y/Ncy/blob/main/docs/sync-protocol.md).
The Rust implementation lives under `src-tauri/src/sync/` and `src-tauri/src/storage/`.

The Python reference server (`scripts/sync_test_server.py` in the Ncy repo) is
useful for sanity-checking phone-side changes without booting this client.

## Layout

```
src-tauri/                       # Rust + Tauri
├── src/
│   ├── lib.rs                   # entrypoint, AppState, command registration
│   ├── main.rs
│   ├── commands.rs              # Tauri commands exposed to the webview
│   ├── network.rs               # LAN IP detection
│   ├── model/                   # wire-shape data types
│   ├── storage/                 # event log, materializer, identity, settings
│   └── sync/                    # WebSocket server, HMAC handshake, wire codec
└── tauri.conf.json

src/                             # Svelte frontend
├── App.svelte
├── main.ts
├── app.css
└── lib/
    ├── ipc.ts                   # typed wrappers around Tauri invoke/listen
    ├── format.ts
    ├── stores/app.ts            # global Svelte stores + bootstrap
    ├── components/              # ChatList, ChatView, MessageBubble, RichText, …
    └── views/                   # PairingView, SettingsView, StarredView, …
```

## License

Same as the Ncy project.
