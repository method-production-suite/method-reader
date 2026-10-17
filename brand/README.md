# Method Reader's window identity

The window's colors, type and spacing, in a form a browser can preview. They
are Method Reader's part of the Method family palette: the site's tokens
([site/assets/tokens.css](../site/assets/tokens.css)) and their rules
([docs/BRAND.md](../docs/BRAND.md)) are the reference, and this folder is
the window's copy of them under the window's own token names. The original
design decisions live in
[the spec](../docs/superpowers/specs/2026-07-30-visual-identity-design.md),
written for the Screepub look. Since 2026-10-16 the window's type and
layout are the Method family's instead (docs/BRAND.md, "The window"): the
system's own UI sans for all chrome, sentence case, a title bar and a tab
strip, Method Budget's control proportions, no brads. The type scale is
unchanged; the radii grew to the family's.

## What's here

- `tokens.json`: source of truth. Every color, light and dark, with a
  `family` field naming the site token it takes its value from.
- `tokens.css`: the same values as custom properties, light and dark.
- `components/`: self-contained previews. Open any of them in a browser.
  Each opens with a `@dsCard` marker so the Design System pane indexes it
  without explicit registration.

Change a value in `tokens.json` and the copies follow it: `tokens.css` and
the window's colors in `rust/crates/sp-app/src/theme.rs`, each pinned to
this file by a test (`rust/crates/xtask/tests/brand.rs` for the CSS and the
contrast, `rust/crates/xtask/tests/brand_tokens.rs` and theme.rs's own tests
for the window).

## The window's names and the family's

The window's token names are older than the palette and were kept so the
code that paints with them did not have to change. `brass` is the accent
role, as it was when the accent was a brass yellow.

| Window token | Family token | Light | Dark |
|---|---|---|---|
| `paper` | `--bg` | `#F7F6F2` | `#111417` |
| `panel` | `--surface` | `#FFFFFF` | `#1B1F24` |
| `ink` | `--ink` | `#1C2228` | `#E4E8EC` |
| `ink-soft` | `--ink-soft` | `#3A424C` | `#C3CAD2` |
| `ink-muted` | `--muted` | `#59626D` | `#9AA3AD` |
| `brass` | `--accent` | `#7052BA` | `#B49CF0` |
| `ink-on-brass` | `--accent-ink` | `#FFFFFF` | `#160F26` |
| `note` | `--note` | `#8A5A00` | `#E3B341` |
| `hole` | `--ink` at 10% (light) | `rgba(28,34,40,0.10)` | `rgba(0,0,0,0.45)` |
| `ground` | `--ink` / the dark `--device` | `#1C2228` | `#0B0C0E` |
| `alarm` | none | `#AF3220` | `#EA7A65` |
| `present` | none | `#3E7D4A` | `#6FBF7E` |
| `bar` | `--surface-2` | `#EFEDE7` | `#22272E` |
| `rule` | `--rule` | `#DEDBD3` | `#2B3138` |
| `rule-strong` | `--rule-strong` | `#8F897C` | `#67717D` |
| `brass-soft` | `--accent-soft` | `#ECE7F7` | `#2A2140` |

`alarm` (errors only) and `present` (the dot on a reader the computer can
see) have no family token because the site has no error state and no
device list; they kept their values. `note` is the family's caution amber
and colors the Beta badge only: never an accent, never decoration.

`bar` is the tab strip under the title bar and a code chip's ground;
`rule` every hairline (controls, cards, sheets, the tab strip's foot);
`rule-strong` a border that carries meaning (the drop well's dashes, a
hovered control); `brass-soft` a selection's ground (the chosen half of
Read it | Edit it, the current scene, a plugged-in device's card). None of
them sets text.

The six `brass-*` ramp tokens are the accent's ramp. Only `brass-highlight`
is still painted (the primary button's hover, which holds its label at
4.5:1 or better); the others shaded the brads the window no longer draws
and are kept for the brad.html preview: `brass-specular`, `brass-shadow`,
`brass-rim`, `brass-edge` and `brass-bounce`. They follow the accent, so
they change with the mode as it does.

Two rules carried over from the brass theme still hold:

- `ink-on-brass` is the label on an accent fill. Plain `ink` on the violet
  is 2.75:1 light and 1.9:1 dark; a test reads the components and fails if
  anything sets `var(--ink)` on a `var(--brass)` ground.
- `ink-muted` is secondary TEXT. `ink` at 55% alpha blends to `#7F8183` on
  paper: 3.62:1, failing AA. If a reader has to read it, it is `ink-muted`;
  faint ink is for hairlines, strokes and fills only (`hole`).

## Adding a color

Don't, if you can avoid it. If you must:

1. Take it from the family palette (site/assets/tokens.css) when one fits,
   and add it to `tokens.json` with an honest `family` value.
2. Add it to `tokens.css` in both blocks, or only `:root` if it is the same
   in both modes.
3. Add it to `Tokens` in theme.rs, in LIGHT and DARK.
4. If anything sets text in it, add `"text": true` to its `tokens.json` entry
   AND add a pair to the contrast test in `rust/crates/xtask/tests/brand.rs`.
   Floors are WCAG 2.x as docs/BRAND.md holds the site: 4.5:1 for text; the
   window's ink and prose are held to 7:1, which they clear. Darken the
   color rather than lowering the floor. The test fails loudly if a token is
   marked `"text": true` with no pair.

## The app icon

The window's icon is not in this folder: `scripts/make-brand.py` draws it
from the mark into `rust/crates/sp-app/icons/` (and `assets/icon.svg`), and
docs/BRAND.md describes it.

## var() is fine here

`CLAUDE.md` bans `var()`, `min()` and `clamp()` in CSS. That rule is about
the EPUB's stylesheet (`rust/crates/sp-epub/src/css.rs`) and only that:
Adobe RMSDK, which is Kobo and tolino's EPUB path, can blank an entire book
on a value function it cannot parse. This
folder ships to modern browsers. Use `var()` and `clamp()` here, and don't
carry them back into `css.rs`.

## The brad symbols are defined per-document

While building `page-frame.html`, cross-file SVG `<use href="brad.html#brad">`
turned out not to resolve in Chromium. Serving the files over HTTP instead of
`file://` did not fix it either: the referenced document is HTML, not an
XML-parseable SVG resource, so the fragment never resolves. `page-frame.html`
works around this by inlining `brad.html`'s `<defs>` block and pointing its
`<use>` elements at `#brad`/`#punch` within its own document instead of
across files. `brad.html` keeps its own copy of the same `<defs>` for its own
preview.

Consequence for the site: the `<defs>` block cannot be written once and
shared by reference across pages. The site's layout must define the `#brad`
and `#punch` symbols once at its root rather than referencing them across
files.

## Fonts

The window's chrome is the system's own UI sans (`--font-ui` in
`tokens.css`: San Francisco, Segoe UI, the Linux desktop's sans), loaded
from the system at start by `rust/crates/sp-app/src/fonts.rs`, with Inter
(SIL OFL 1.1, unmodified, `rust/crates/sp-app/fonts-ui/`) bundled as the
fallback. Courier Prime and Literata set the script only. The previews in
`components/` were drawn for the Screepub look and still show Courier
chrome; the window is the reference now. They pull Courier Prime and
Literata from Google Fonts for convenience. The site self-hosts them; both
are open licensed, which is what makes that legal.

## Pushing to Claude Design

`DesignSync` reads this folder. List, then plan, then write:

    list_files → finalize_plan (writes: brand/**) → write_files

Push components one at a time rather than replacing the project wholesale.

Previews must not reference anything outside `brand/`. DesignSync only
carries `brand/**`, so a path like `../../assets/screenshot-drop.png` leaves
the folder it writes and 404s once synced, the same way it 404s under
`python3 -m http.server --directory brand`. `shot-frame.html` hit exactly
this and now uses an inline SVG placeholder instead of a real screenshot.
