# Ncy Desktop — Design Rules

The visual language for the desktop app (Tauri + Svelte 5). Keep it flat, square,
and quiet. When in doubt, match Acy (`e:\Github\Acy`) — the shared family identity.

## Principles

- **Flat, not floating.** Separate things with **borders**, never shadows.
- **Square, not round.** Rounded rectangles everywhere; circles/pills only for
  true indicators (status dots, toggle, progress).
- **Grouped with hairlines.** Related rows live in one bordered container split by
  thin dividers.
- **Accent sparingly.** Colour = primary action or the active/selected state. Most
  surfaces are neutral.
- **Quiet copy.** Short labels; one small hint per group, not a description per row.

## Tokens (from `src/app.css`)

Always use variables — never hard-code colours, radii, or shadows. Everything is
theme-aware (light / `[data-theme='dark']`) and the accent is user-swappable
(purple/blue/green/pink/orange/teal).

- **Radius:** `--radius` 10px (buttons, containers), `--radius-sm` 7px (chips,
  small icon buttons, inputs), `--radius-pill` 999px (indicators only).
- **Surfaces:** `--surface` (base), `--surface-2` / `--surface-hover` (raised /
  hover fills).
- **Borders:** `--border` (dividers, containers), `--border-strong` (inputs,
  segmented controls, emphasis).
- **Text:** `--text`, `--text-muted`.
- **Accent:** `--accent` fill + `--accent-contrast` text; `--accent-hover`.
- **Status:** `--danger`, `--success`, `--warning`.
- **Shadows:** none. `--shadow` and `--card-shadow` are `none` on purpose.
- **Type:** `--font-sans` (IBM Plex Sans), `--font-mono` (IBM Plex Mono, for IDs /
  JSON / paths).

## Shape & elevation

- No box-shadows anywhere. Popovers/menus stand out via border + surface, not lift.
- Buttons: `--radius`. Chips & compact icon buttons: `--radius-sm`.
- **Avatars are rounded squares**, radius scales with size (`calc(size * 0.28)`).
- **Count badges** are rounded rectangles (`--radius-sm`), not pills.
- Stay round only for: status dots, the toggle track/knob, progress bars, the
  scrollbar.

## Buttons

- Default `<button>` = accent fill, `--radius`. `.ghost` (transparent + border) and
  `.subtle` (surface-2) for secondary. `.danger` for destructive.
- Hover on neutral controls → `--surface-hover` / `--surface-2`. **Never** flip a
  neutral control to accent on hover.
- **Icon-only** for compact/repeated actions (X to remove, archive toggle, titlebar
  gear). **Labelled** for primary actions (Save, Create, Add).
- Active/selected/on states fill with `--accent` + `--accent-contrast`.

## Bordered lists (the core pattern)

One border around a group of rows, `--border` hairline between rows, outer corners
rounded, rows sit flush (no per-row radius or gap).

- `.opt-list` — settings (non-scrolling).
- `.divided-list` — main screen (chat list, search / starred / reminders results);
  scroll-friendly. Row buttons must be `border-radius: 0` so their fill doesn't
  bulge past the dividers.
- **Messages are the exception** — they stay as bubbles, not divided rows.

## Segmented controls

Mutually-exclusive related choices become one connected control: single border,
dividers between segments, active segment filled with accent.

- `.chip-seg` / `.chip-seg-btn` — chat-list profile buckets and tag filters.
- `.attach-group` — the +, checklist, mic buttons in the composer.
- `.seg` / `.seg-btn` — settings theme picker.

## Settings

- Left sub-nav (`.side` / `.side-link`, active = accent left-border) + `.panes`.
- Sections are borderless `.group` blocks with an `<h2>`; a squared **accent back
  button** sits by the "Settings" title.
- Controls go inside `.opt-list`; toggles are `.opt-row` (label left, switch right);
  text inputs are `.field` (label above input, `max-width ~360px`,
  `--border-strong`); helper text is one `.hint` per group.
- No cards, anywhere.

## Layout & navigation

- **No sidebar.** Titlebar holds brand, a server-status dot, and a gear that toggles
  Settings (back-arrow returns to Chats).
- Chats view: left list pane with a tab strip (Chats · Search · Starred ·
  Reminders); right pane is the conversation. Results jump into the chat.
- New-chat is a rounded-square accent **FAB**, bottom-right of the chat list.

## Do / Don't

- ✅ Border to separate. ❌ Shadow to separate.
- ✅ Rounded rectangle. ❌ Pill/circle for buttons or chips.
- ✅ One hint per group. ❌ A description under every control.
- ✅ Sentence-case labels. ❌ Uppercase + letter-spacing eyebrow labels.
- ✅ Accent for the one active/primary thing. ❌ Accent as a hover colour.
- ✅ Token variables. ❌ Hard-coded hex / px radius / shadows.
