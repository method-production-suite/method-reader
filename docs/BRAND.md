# Method Reader brand

How Method Reader looks and sounds on methodreader.com, in the README and anywhere else it
shows up. It is one of a family with Method Budget (methodbudget.com) and Method Schedule
(methodschedule.com): the same voice, the same neutrals and type, the same way of drawing
the mark and the name, and an accent of its own. The site's tokens live in
[site/assets/tokens.css](../site/assets/tokens.css); the artwork is drawn by
[scripts/make-brand.py](../scripts/make-brand.py).

Method Reader is the new name for Screepub. This file covers the website, the app icon,
and the palette the window shares with them. The window's own colors are this palette
under the window's token names ([brand/](../brand/README.md), held to `sp-app`'s
`theme.rs` by tests); brand/README.md maps each one to the site token below.

## The name

- Always **Method Reader**, two words, both capitalized, in running text.
- Never shorten it to initials. On second mention "the app" or "it" is fine.
- The wordmark is set in lowercase (`method reader`). That is artwork, not spelling: in
  text, write Method Reader.
- Method Reader was called Screepub until 0.13. Say so once where someone arriving from
  the old name meets it (the README's opening, the site's notice). Files released under
  the old name (the Intel Mac's 0.9.0, for one) keep it: never pretend a file is named
  something it is not.
- Credit "the Method Reader contributors". No personal, company or studio names in
  public material: not in pages, alt text, metadata or file names.

## Voice

Plain, confident, specific. Write for someone who reads scripts, not for someone who
reads the code.

- Say what a person can do, in their words.
- Claim only what is built. Beta is labelled beta, with the one sentence that says what
  beta means (the site and the README carry it; tests hold both to it).
- No hype, no em dashes, no exclamation marks.
- Examples use invented scripts and invented people only (Field Station is the house
  example; never a real title, author or character).

## The mark

An e-reader showing a script page: the device body at full strength, and on its screen
the shape a screenplay keeps when it reflows (a scene heading, two lines of action, a cue
set in, and its dialogue) at half strength. The screen is cut out, so the mark sits on
any background. It uses the family's box (40 by 52 units on a 64-unit grid), the same box
as Method Budget's page and Method Schedule's stripboard, and half strength means what it
means there: the working, under the thing that counts.

A separate 16-unit drawing (whole units only, three lines on the screen) is used for
favicons so it stays sharp at 16 and 32 pixels.

| File | Use |
|---|---|
| `site/assets/brand/mark.svg` | The mark, violet, for light backgrounds |
| `site/assets/brand/mark-dark.svg` | The mark, light violet, for dark backgrounds |
| `site/assets/brand/wordmark.svg` | Mark and name, for light backgrounds |
| `site/assets/brand/wordmark-dark.svg` | Mark and name, for dark backgrounds |
| `site/favicon.svg` | The 16-unit mark; switches color with the browser's scheme |
| `site/favicon.ico` | The 16-unit mark at 16 and 32 pixels |
| `site/apple-touch-icon.png` | 180 pixels, white mark on a full-bleed violet square |

Rules: clear space of at least a quarter of the mark's height; don't recolor it outside
the palette, outline it, add effects or change its lines; use the `-dark` files on dark
backgrounds (a `<picture>` with a `prefers-color-scheme: dark` source, as the site does).

### The app icon

The white mark on a rounded-square violet plate, lit from above (`--accent` at the top
to `--accent-strong` at the bottom), on macOS's icon grid: the plate is 824 of 1024
units with Apple's corner and a soft shadow, and the mark is centred at 60% of the
plate's height. At 32 pixels and under the plate fills the square and carries the
16-unit drawing instead, so the 16 and 32 point sizes stay sharp.

| File | Use |
|---|---|
| `assets/icon.svg` | The icon as a vector, for reference |
| `rust/crates/sp-app/icons/icon.icns` | The Mac bundle (16 to 1024 pixels) |
| `rust/crates/sp-app/icons/icon.ico` | The Windows installer and app (16 to 256) |
| `rust/crates/sp-app/icons/32x32.png`, `128x128.png`, `128x128@2x.png` | Linux (hicolor 32, 128 and 256) |
| `rust/crates/sp-app/icons/icon.png` | 512 pixels |

### The wordmark

`method reader` in lowercase, drawn from strokes, one weight, a 100-unit x-height. m, e,
t, h, o and d are Method Budget's drawings; r and a are new here and follow the same
rules (the a is the d without its ascender; the r is the h's shoulder, stopped where it
starts to fall). Like Method Schedule's, it has no descender, so its files are 162 units
tall, and in a header it is set at the same height as Method Schedule's.

Regenerate everything with `python3 scripts/make-brand.py` (`--check` changes nothing and
exits 1 when a file is out of date or a color pair fails its contrast; `--contrast`
prints the table below). The script is a copy of Method Schedule's with our mark, letters
and accent; keep the shared parts in step.

## Color

The neutrals, the amber and the token names are the family's, value for value. What is
ours is the accent, a violet in the same lightness band as Method Budget's teal and
Method Schedule's slate blue (relative luminance 0.13, between their 0.136 and 0.12), so
the three accents read as a set. Amber (`--note`, `--note-fill`) stays the family's
caution color: the site's beta notes. It is never an accent.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--accent` | `#7052BA` | `#B49CF0` | Links, buttons, the mark |
| `--accent-strong` | `#573E96` | `#CDBDF6` | Hover, card headings |
| `--accent-ink` | `#FFFFFF` | `#160F26` | Text on an accent fill |
| `--accent-soft` | `#ECE7F7` | `#2A2140` | Highlights |

The drawn e-readers (`--device`, `--screen`, `--screen-ink`, `--screen-muted`,
`--pdf-page`, `--pdf-ink`, `--device-accent`) stay the same in both schemes, like the
terminal sample on the other two sites: a device is a picture of an object.

### Contrast (WCAG 2.x)

| Pair | Light | Dark | Needs |
|---|---|---|---|
| ink on bg | 14.84 | 15.01 | 4.5 |
| ink on surface-2 | 13.71 | 12.20 | 4.5 |
| ink-soft on surface-2 | 8.69 | 9.09 | 4.5 |
| muted on bg | 5.72 | 7.23 | 4.5 |
| muted on surface-2 | 5.29 | 5.88 | 4.5 |
| accent on bg | 5.39 | 7.88 | 4.5 |
| accent on surface | 5.83 | 7.06 | 4.5 |
| accent on surface-2 | 4.98 | 6.41 | 4.5 |
| accent on accent-soft | 4.81 | 6.44 | 4.5 |
| accent-strong on accent-soft | 6.84 | 8.78 | 4.5 |
| accent-ink on accent | 5.83 | 7.92 | 4.5 |
| ink on accent-soft | 13.25 | 12.27 | 4.5 |
| note on surface | 5.93 | 8.51 | 4.5 |
| rule-strong on bg | 3.22 | 3.73 | 3 |
| screen-ink on screen | 13.00 | 13.00 | 4.5 |
| screen-muted on screen | 5.55 | 5.55 | 4.5 |

## Type

The page uses the system's own faces, like the rest of the family. The script itself (the
demo, the sluglines that head each section, the share card's e-reader) is set in the faces
the app ships, Courier Prime and Literata, self-hosted in `site/fonts/` and held
byte-identical to the app's copies by `xtask/tests/brand_tokens.rs`.

## Imagery

- No stock photos and no other product's screens, logos or text.
- The drag-and-drop pictures are the real window, taken by `cargo xtask capture-screens`.
- The share card (`site/og.png`, 1200 by 630) is drawn from `site/og-card.html` by
  `cargo xtask make-og`.
