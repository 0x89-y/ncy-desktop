# Ncy Desktop

Desktop client for [Ncy](https://github.com/0x89-y/Ncy), the Android "note-to-self"
chat app. Implements the full Ncy sync protocol — **bidirectional**, near-real-time,
peer-to-peer over the LAN via WebSocket + HMAC-SHA256 challenge/response. No server
or account; the phone and desktop talk directly.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** + **TypeScript** (web frontend).

## Status

Sync core (phases 1–3) complete:

- ✅ **Standalone use** — works with no phone at all: create chats, author/edit/
  delete/star messages, tick checklist items, attach images — entirely offline
  on its own event log, with **backup/restore** to a portable `.zip`
- ✅ **Desktop↔desktop sync** — link two desktops by pasting one's pairing JSON
  into the other; the connecting side runs a client connector (challenge/HMAC,
  watermarked bidirectional Event/Ack/Blob, auto-reconnect) against the other's
  server. Hub topology: one desktop is the server; the other desktop and the
  phone connect to it
- ✅ WebSocket sync server with HMAC pairing handshake; multi-pairing (each peer
  gets its own 32-byte pre-shared secret)
- ✅ Per-chat append-only NDJSON event log, idempotent dedup by `eventId`,
  HLC-ordered materializer (tombstones, whole-record LWW for chat metadata)
- ✅ **Bidirectional sync** — the desktop authors events too; both sides catch up
  from a persisted per-peer watermark on reconnect
- ✅ **mDNS auto-discovery** — phones find the desktop on the LAN without typing
  an IP (`_ncy-sync._tcp`); **opt-in** (off by default — flip *Discoverable on
  the network* in settings); QR + JSON pairing works regardless
- ✅ **Media blob transfer** — voice clips, images, avatars, and chat backgrounds
  transfer content-addressed (SHA-256) over the same socket and render inline
- ✅ **Encrypted protected chats** (phase 3) — AES-256-GCM with a PBKDF2 key
  derived from the chat PIN; locked chats show a PIN prompt and only ever
  materialize `🔒` placeholders until unlocked. Keys are memory-only, never
  persisted. **Authoring into a protected chat works while it's unlocked** —
  the desktop seals each message before it touches the local log or the wire,
  so plaintext never leaves the process. The desktop can also **protect a chat
  and set/change its PIN** itself (re-seals existing messages via `supersedes`)
- ✅ **Protect-after-sync catch-up** — if a chat is marked protected *after* it
  already synced to the desktop in plaintext, the phone re-issues each message
  sealed with a `supersedes` pointer and the desktop scrubs the stale plaintext
  from its at-rest log automatically (no key needed, no manual re-sync)
- ✅ Authoring from the desktop: send text (with a Markdown-style rich-text
  toolbar — bold/italic/strike, H1–H3, bullets, code blocks), attach images,
  create checklists, edit/delete/star messages — all synced back to the phone
  (including into unlocked protected chats)
- ✅ **Profile sync** — the phone's profiles (separate chat spaces) are all
  mirrored; the chat list gets an "All / <profile>" switcher to scope by profile
- ✅ Chat list with pinned / archived / tags; cross-chat **search**; **starred**
  and **reminders** views; chat avatars + wallpapers
- ✅ Desktop niceties: system tray (close-to-tray keeps sync alive), window
  size/position restore, native notifications when a message arrives unfocused
- ✅ Settings: device name, port, theme, and **opt-in network discoverability**
  (mDNS off by default)

Out of scope / not done:

- ❌ Off-LAN relay (sync only works when both devices are on the same network)

## Prerequisites

- Node.js 18+ and npm
- Rust stable + the platform's WebView2 / WebKitGTK dependencies, per the Tauri
  docs: <https://v2.tauri.app/start/prerequisites/>

**Platform support:** built, run, and tested on **Windows** only so far. The
codebase is platform-portable by design — Tauri 2 and every dependency are
cross-platform, and there's no platform-specific code (the lone `windows_subsystem`
attribute in `main.rs` is silently ignored elsewhere) — so it *should* build and
run on macOS and Linux with the standard Tauri prerequisites above, but that is
**unverified**: nobody has built or run it there yet, there's no CI, and the
distribution/packaging path (`.dmg`, `.AppImage`, code-signing) is untried. You
build natively on each OS; there's no cross-compilation step.

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

- `ncy-desktop.exe` (or the platform binary) — standalone; needs WebView2
  (preinstalled on Win10/11) / WebKitGTK
- `bundle/nsis/…-setup.exe`, `bundle/msi/….msi` (Windows) — installers
- `bundle/dmg/…`, `bundle/deb/…`, `bundle/appimage/…` on macOS / Linux

> Note: `target/` gets large (multi-GB) — it's the Rust build cache and is
> gitignored. `cargo clean` from `src-tauri/` reclaims it (slow next build).

## Pairing a phone

1. On the desktop, go to **Devices**, click **Create** to make a pairing slot.
   Each slot has its own 32-byte pre-shared secret — don't share QR codes
   between phones.
2. On the phone: **Scan QR** (camera) or **Paste pairing JSON**.
3. Foreground the phone app. Catchup runs immediately; new events stream live.

The QR / pairing JSON carries the desktop's host:port directly, so pairing
always works regardless of network discovery.

**Network discovery is opt-in.** By default the desktop does **not** advertise
itself over mDNS — on a shared or large network you usually don't want to
broadcast its presence. Turn on **Settings → Sync server → Discoverable on the
network** to let the phone list it automatically under "Found on this network"
(**Ncy → Settings → Sync devices**); the toggle takes effect immediately, no
restart. The **Refresh IPs** control on the Devices screen picks which LAN IP is
advertised when discovery is on.

## Use without a phone

The desktop is a full client on its own — no phone required. Hit **+** in the
chat list to create a chat, then compose / edit / star / delete, tick checklist
items, attach images normally. Everything persists to the desktop's own per-chat
event log, so it survives restarts and will sync to a phone (or another desktop)
if you pair one later. A desktop-created chat starts in the **Default** profile
bucket; a phone that later receives it files it under its active profile.

**Back up your notes.** Settings → **Backup** → *Export backup…* writes a single
`.zip` of every chat's event log plus its media. *Import backup…* replays it
back — a **merge**, not an overwrite (events dedupe by id and converge by HLC),
so re-importing or importing onto a populated device is safe. The zip is
content only; device identity, pairings and settings stay device-local. This is
the recommended way to back up a phone-less desktop or move it to a new machine.

## Sync two desktops

Same handshake as the phone, just desktop-to-desktop. Pick one desktop to be
the **server** (it keeps doing what it already does). On the **other** desktop:

1. On the server desktop: **Devices → New pairing → Copy JSON**.
2. On the client desktop: **Devices → Sync with another desktop → paste the
   JSON → Add desktop**. It connects out immediately and reconnects
   automatically; the peer shows **Connected** when live.
3. Existing chats catch up on connect; new edits stream live both ways.

Topology is a **hub**: with a phone and two desktops, run the server on one
desktop; the other desktop and the phone connect to it. Server↔client and
server↔phone are real-time bidirectional. A phone-authored message reaches the
*client* desktop via catch-up when it (re)connects, not instantly — adding
real-time relay between two clients of the same hub is a deliberate
non-goal for now.

## Protected chats

A protected chat syncs with its message bodies AES-256-GCM encrypted. You can
protect a chat **from the desktop** too: the chat header's ⋯ menu →
*Protect chat…* (set a PIN) or, on an already-protected chat, *Change PIN…*.
Protecting generates the salt, derives + caches the key, and re-seals every
existing message (superseding the plaintext so neither the at-rest log nor any
peer keeps a readable copy — same mechanism as the phone's protect-after-sync).
A protected chat shows a lock screen; enter its PIN to decrypt it for that
session. The chat list shows `🔒` placeholders
(no content preview) until unlocked. Re-lock via the header button, or just
restart — keys live only in memory. Chat metadata (name, tags, pinned, avatar)
stays in plaintext so the list still renders.

While a protected chat is unlocked you can author into it normally (send, edit,
star, checklists, images). The desktop seals each message with the chat's
in-memory key *before* it's written to the local event log or sent over the
wire, so plaintext never leaves the process — the phone receives the same
`{v,enc,nonce,ct}` envelope it would author itself. Sending is refused if the
chat somehow isn't unlocked, so a readable body can't slip into an encrypted
chat. (One detail: edits/stars operate on the decrypted message, so the chat
must be unlocked — which it already is to see the compose box.)

## Profiles

The phone groups chats into **profiles** (independent spaces you switch between).
The desktop receives **every** profile in one sync stream and tags each chat with
the profile it came from. When more than one profile is present, the chat list
shows a switcher row — **All** plus one chip per profile — so you can scope the
list the same way you do on the phone. Chats that synced before profile sync
existed (or from a pre-migration phone layout) land in a single **Default**
bucket.

Profiles aren't a synced entity in their own right — there's no profile
create/rename/delete op. A profile's identity travels as a per-event stamp, so
**renaming a profile on the phone only reaches the desktop once a new event is
authored in one of that profile's chats** (send or edit a message and it
catches up). Creating/deleting a profile likewise surfaces lazily as its chats'
events arrive. Profiles are display-only on the desktop; chats are created and
moved between profiles on the phone.

## Storage

Per-OS app data dir, under `ncy/`:

- `identity.json` — this desktop's stable `deviceId` and display name
- `pairings.json` — pairings, their secrets, and per-pairing sync watermarks
- `settings.json` — port, device name, theme, `discoverable` (mDNS opt-in)
- `events/<chatId>.ndjson` — one append-only event log per chat, byte-identical
  to the phone's `events.ndjson` lines
- `blobs/<sha256>.<ext>` — content-addressed media (voice/image), shared across
  chats by hash

Locations:

| OS      | Path |
|---------|------|
| Windows | `%APPDATA%\com.ncy.desktop\ncy\` |
| macOS   | `~/Library/Application Support/com.ncy.desktop/ncy/` |
| Linux   | `~/.local/share/com.ncy.desktop/ncy/` |

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
    ├── stores/app.ts            # global stores, bootstrap, notification dispatch
    ├── components/              # ChatList, ChatView, MessageBubble, RichText,
    │                            #   BlobImage, Avatar, …
    └── views/                   # PairingView, SearchView, StarredView,
                                 #   RemindersView, SettingsView
```

## License

Same as the Ncy project.
