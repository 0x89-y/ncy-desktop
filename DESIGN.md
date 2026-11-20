# Ncy Desktop — design

Ncy has its own visual identity, shared by this app and the Android app. It used to
follow [`cy-design`](https://github.com/0x89-y/cy-design), the spec for the "cy"
family; since the 0.9 redesign it does not. The other cy apps still do.

The mockups the identity was settled on live in the Android repository under
`docs/redesign/` (`desktop-mockups.html` for this app). Android's theme is
`app/src/main/java/com/ncy/notechaty/ui/theme/NcyTheme.kt`; the colour formulas here
are kept in step with it.

## The identity in one paragraph

Neutral greys, each tinted lightly toward the accent the user picks, so the whole
window takes the accent's temperature. Geist for everything read, Geist Mono for
times, counts and section labels. Notes are bubbles: yours on the accent tint on the
right, notes from other devices on plain paper on the left. The accent marks what
is yours, primary or selected; hover never uses it.

## Where things live

| | |
|---|---|
| Tokens | `src/app.css`: CSS custom properties, derived from `--accent` with `color-mix()` |
| Theme | `data-theme` attribute + `prefers-color-scheme` |
| Accent | `data-accent` attribute; the seven presets are in `src/lib/accent.ts` and `app.css` |
| Legacy accent names | `normalizeAccent()` in `src/lib/accent.ts` (purple → violet, blue → inkblue, …) |
| Chat colour | `avatarColor()` in `src/lib/format.ts`; the same hash and hues as Android |
| Fonts | `@fontsource/geist-sans` and `@fontsource/geist-mono`, imported in `src/main.ts` |

Always reference the variables. A hard-coded hex, px radius, or shadow is a bug:
every neutral is derived from the accent, so a literal breaks in one theme or one
accent and nobody notices until a screenshot.

### Token roles

- `--bg`, `--surface`, `--surface-2`, `--surface-hover`: grounds, all accent-tinted.
- `--text`, `--text-muted`, `--border`, `--border-strong`: text and hairlines.
- `--accent`, `--accent-contrast`, `--accent-hover`: the accent and text on it.
- `--accent-soft`, `--accent-edge`, `--on-accent-soft`: the accent's soft side,
  used for your bubbles, the selected filter chip, the active settings section, the
  selected segment and the Up next card.
- `--online`, `--warning`, `--danger`: status colours.

## Local class vocabulary

- **Lists**: `.divided-list` (chat list, search, starred, reminders). In the chat
  list it runs flush to the pane, with mono `.chat-section-label`s (Pinned, All
  chats, Archived). Row buttons need `border-radius: 0` so their hover fill doesn't
  bulge past the hairlines.
- **Settings lists**: `.opt-list`, with `.opt-row` for label-left/control-right rows
  and `.field` for label-above-input, edge-to-edge and inset by `--settings-pad`.
- **Segmented controls**: `.seg` / `.seg-btn`; the selected segment takes the accent
  tint, not a solid fill.
- **Settings frame**: `.side` / `.side-link` for the category rail (active = accent
  tint), `.panes` for the content side.

## Screens

**Title bar**: a button at the top left collapses the chat list to a narrow column of
chat tiles (remembered per device; Ctrl+K and Ctrl+N expand it). On the right: the
sync status dot, the settings gear and the window buttons. No wordmark.

**Chats**: the left pane mirrors the phone's home screen. A header row with the
profile menu, search and filters; a large pane title; a sync line ("Synced with
Pixel 8 · 2 minutes ago", hidden while nothing is paired); filter chips for All,
Starred and Reminders; an Up next card for the next reminder; then the chats, pinned
first. New chat is an accent button with its label, bottom-right. The conversation
on the right sets its bubbles in a centred column up to 720px wide.

**Settings**: one master-detail panel, category rail left, content right. Sections
are General · Sync · Interface · Transcription · Backup · Changelog · About.
Settings apply immediately; there is no Save button.

## Deliberate choices

- **Flat overlays.** `--shadow` and `--card-shadow` are `none`; menus read against
  the page by border and surface step alone.
- **Rounded-square buttons** on desktop (circles are for the phone).
- **Count badges** are rounded rectangles (`--radius-sm`); pills are for true
  indicators (status dots, toggles, filter chips).
- **No chat colours on bubbles.** A chat's colour is the tint of its tile; bubbles
  always follow the accent so the window stays one system.

## Reviewing UI without the back end

`npm run dev` outside Tauri installs a sample-data stand-in for the back end
(`src/dev/tauriMock.ts`, dev builds only). URL parameters: `?theme=dark|light`,
`?accent=<name>`, `?chat=ideas`, `?view=settings&tab=interface`, `?pair=1`.

## Checks worth running before shipping UI

- No hard-coded hex, `px` radius, or `box-shadow` outside the token set.
- Both themes, and at least two accents: a literal usually only breaks in one.
- Hover on a neutral control goes to `--surface-hover` / `--surface-2`, never to the
  accent. The accent means yours, primary or selected, not "pointer is here".
