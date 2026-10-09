# Screepub brand system

The identity, in a form a website can use. The design decisions and their
reasoning live in
[the spec](../docs/superpowers/specs/2026-07-30-visual-identity-design.md);
this file is the operating manual.

## What's here

- `tokens.json`: source of truth. Every color, with a `from` field saying
  where it came from.
- `tokens.css`: the same values as custom properties, light and dark.
- `components/`: eight self-contained previews. Open any of them in a
  browser. Each opens with a `@dsCard` marker so the Design System pane
  indexes it without explicit registration.

## Where the seven core colors came from

Seven tokens (`paper`, `ink`, `ink-muted`, `ink-on-brass`, `brass`, `alarm`,
`hole`) were first defined by the Swift Mac app's theme, and their `from`
field says `swift-app`. That app was retired and deleted (piece F3), so
`tokens.json` is the only source: change a value here and the copies that
ship follow it, `tokens.css` and the window's `desktop/ui/style.css` and
generated `desktop/ui/tokens.css`, each pinned to this file by a test.

Two of those seven exist because a color that is right in one place is wrong
in another:

- `ink-on-brass` is the label on a brass button, and it is deliberately the
  same in both modes. `ink` flips to cream in dark mode while `brass` holds,
  which put the site's primary call to action at 1.67:1. Pinned, it is 7.98:1
  either way. A test also reads the components and fails if anything sets
  `var(--ink)` on a `var(--brass)` ground again.
- `ink-muted` is secondary TEXT. `inkFaint` is `ink` at 55% alpha, which
  blends to `#7F7C74` on paper: 3.73:1, failing AA. The web split this off
  first and the Swift app adopted it, so `inkFaint` was for hairlines,
  strokes and fills only. If a reader has to read it, it is `ink-muted`.

## Adding a color

Don't, if you can avoid it. The palette is six colors and one of them is for
errors. If you must:

1. Add it to `tokens.json` with an honest `from` value.
2. Add it to `tokens.css` in both blocks, or only `:root` if it is
   mode-independent the way brass is.
3. If anything sets text in it, add `"text": true` to its `tokens.json` entry
   AND add a pair to the contrast test in `tests/brand-tokens.test.ts`. Floors
   are 7:1 for body and headings, 4.5:1 for captions and transitions. Darken
   the color rather than lowering the floor. `tests/brand-tokens.test.ts`
   fails loudly if a token is marked `"text": true` with no pair, so this
   cannot go unnoticed the way `ink-footnote` once did.

## var() is fine here

`CLAUDE.md` bans `var()`, `min()` and `clamp()` in CSS. That rule is about
`src/epub/css.ts` and only that: Adobe RMSDK, which is Kobo and tolino's EPUB
path, can blank an entire book on a value function it cannot parse. This
folder ships to modern browsers. Use `var()` and `clamp()` here, and don't
carry them back into `css.ts`.

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

Courier Prime for structure, Literata for prose. The previews pull both from
Google Fonts for convenience. The site self-hosts them; both are open licensed,
which is what makes that legal.

## Pushing to Claude Design

`DesignSync` reads this folder. List, then plan, then write:

    list_files → finalize_plan (writes: brand/**) → write_files

Push components one at a time rather than replacing the project wholesale.

Previews must not reference anything outside `brand/`. DesignSync only
carries `brand/**`, so a path like `../../assets/screenshot-drop.png` leaves
the folder it writes and 404s once synced, the same way it 404s under
`python3 -m http.server --directory brand`. `shot-frame.html` hit exactly
this and now uses an inline SVG placeholder instead of a real screenshot.
