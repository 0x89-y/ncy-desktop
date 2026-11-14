# Ncy Desktop — design

The visual language is **not defined here**. It lives in
[`cy-design`](https://github.com/0x89-y/cy-design) (`e:\Github\cy-design`), the
single source of truth for the whole "cy" family — principles, colour tokens with
values, typography, shape, layout primitives, components, interaction patterns, and
the avatar-colour hash. Read that first; §9 there maps each rule onto this app.

This file records only what is **specific to ncy-desktop**: where the tokens live,
the local class vocabulary, the screens, and the places this app deliberately picks
one option where the spec allows several. Anything that duplicated the spec has been
removed rather than kept in sync badly — a second copy of a rule is a second copy to
get wrong.

## Where things live

| | |
|---|---|
| Tokens | `src/app.css` — CSS custom properties |
| Theme | `data-theme` attribute + `prefers-color-scheme` |
| Accent | `data-accent` attribute; user-swappable (purple/blue/green/pink/orange/teal) |
| Avatar colour | `avatarColor()` in `src/lib/format.ts` |

Always reference the variables. A hard-coded hex, px radius, or shadow is a bug —
the tokens are theme-aware and the accent is user-swappable, so a literal breaks in
one theme or one accent and nobody notices until a screenshot.

## Local class vocabulary

The spec names the *patterns*; these are what they are called here.

- **Divided lists** — `.divided-list` (chat list, search, starred, reminders).
  Row buttons need `border-radius: 0` so their hover fill doesn't bulge past the
  hairline dividers.
- **Settings lists** — `.opt-list`, with `.opt-row` for label-left/control-right
  rows and `.field` for label-above-input. Inside the settings panel these run
  edge-to-edge; the panel frame is the only border, content inset by
  `--settings-pad`.
- **Segmented controls** — `.chip-seg` / `.chip-seg-btn` (tag filters),
  `.attach-group` (composer +, checklist, mic), `.seg` / `.seg-btn` (theme picker).
- **Settings frame** — `.side` / `.side-link` for the category rail (active = accent
  left-bar + fill), `.panes` for the content side.

**Message bubbles are the documented exception** to divided lists — they stay
bubbles, per the spec's own carve-out.

## Screens

**Title bar**: in the chats view, a button at the top left collapses the chat list to a
narrow column of chat avatars with a new-chat button, or expands it again
(remembered per device; Ctrl+K and Ctrl+N expand it). On the right:
the sync status dot, the settings gear and the window buttons. No wordmark.

**Chats** — a left pane with three tabs (Chats · Favorites · Reminders), a profile
switcher and search/filter actions in its header row, and the conversation on the
right. Search is a **toggle in the header**, not a tab; opening it reveals a search
row above the list and results replace the list while a query is present. New chat
is a rounded-square accent FAB, bottom-right.

**Settings** — one master-detail panel: category rail left, content right. Sections
are General · Sync · Interface · Transcription · Backup · Changelog · About.
Settings **apply immediately**; there is no Save button. The selected rail item is
not repeated as a page heading. About shows the version from `getVersion()`, a
source link, and the changelog from `src/lib/changelog.ts`.

## Deliberate local choices

Where cy-design offers a choice, this is what ncy-desktop picked and why.

- **No wordmark, no dotted ground.** Like Acy and Spool, the title bar has no
  "0x" wordmark, and like Acy the page background is plain `--bg` without the
  dot grid. Dialogs use `--radius-dialog` (12px), matching Acy.

- **Flat overlays.** The spec permits a single soft shadow (`--shadow-pop`) on
  transient floating layers — context menus, popovers, inline editors — but does
  not require it. This app declines: `--shadow` and `--card-shadow` are both `none`
  and menus read against the page by border plus surface step alone. There is no
  `--shadow-pop` token here. If one is ever added, it applies *only* to those
  transient layers; structural surfaces stay flat.
- **Square primary icon buttons.** The spec makes this platform-specific — circle on
  Android, rounded square on desktop. Desktop uses `--radius`.

## Checks worth running before shipping UI

- No hard-coded hex, `px` radius, or `box-shadow` outside the token set.
- Both themes, and at least two accents — a literal usually only breaks in one.
- Count badges are rounded rectangles (`--radius-sm`), not pills; pills are reserved
  for true indicators (status dots, toggle track/knob, progress bars, scrollbar).
- Hover on a neutral control goes to `--surface-hover` / `--surface-2`, never to
  accent. Accent means primary or active, not "pointer is here".
