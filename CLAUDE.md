# Screepub

Screenplay PDF → Fountain → reflowable EPUB3/MOBI, with a Tauri desktop
window (`desktop/`, Mac, Windows and Linux) that converts and sends to
e-readers (Kindle first; Kobo/tolino via USB volume signatures, reMarkable
via its USB web interface: see `src/device/`). Engine is Bun/TypeScript;
the window shells out to it as a compiled sidecar. The older SwiftUI Mac
app is FROZEN, not deleted: it still builds and its checks still run in CI
until piece F3 removes it (docs/superpowers/specs/2026-09-14-retire-swiftui-design.md).

## Commands

```bash
bun test                    # engine suite (integration tests need fixtures/)
bunx tsc --noEmit           # typecheck
bun src/cli.ts <pdf>        # convert (see --help; --json is the app contract)
bun tools/build-sidecar.ts --host   # compile the engine for this machine (before cargo)
(cd desktop/src-tauri && cargo run)   # the window; bundles via tools/build-app-bundle.ts
app/build-app.sh            # FROZEN Swift app: sidecar + SwiftUI app
(cd app && swift run -c release kit-check)   # FROZEN Swift app's behavior checks (CI, until F3)
epubcheck <out.epub>        # validate output (brew-installed)
bun tools/capture-screens.ts   # retake README + site pictures (needs Chrome; macOS)
bun tools/review-screens.ts --out <page.html>   # which pictures changed, old beside new
bun tools/bump-version.ts <version>   # release bump: all 4 version files + desktop/ui/notes.js
```

## Architecture

- `src/parser/` — PDF → classified elements. Adapted from an earlier
  table-read parser by the same author and heavily extended here; this is
  now the only copy, so no cross-repo mirroring. Parser stays
  FORMAT-OPTION-FREE.
- `src/parser/cue.ts` — the ONE definition of what a character cue looks
  like as TEXT, imported by classify.ts and by extract.ts's dual-dialogue
  detector. Third module of the shared-discriminator kind, and the one that
  proved why the pattern exists: there used to be two definitions and they
  disagreed, so a script whose lead was named "Q" rendered ordinary cues
  fine and collapsed a whole dual-dialogue scene into interleaved action.
  **Geometry stays with the caller** — classify owns the indent band,
  clusterSplit owns the column test — because forcing the dual path to
  invent an indent is how the second copy got written.
- `src/fountain/serialize.ts` — elements → Fountain. The `.fountain` is a
  durable artifact and the app's cache boundary. Beside it,
  `src/fountain/slug.ts` owns the stage-2 `PRIMARY_SLUG`/`isMiniSlug`
  discriminator both renderers import (registry #5b), and
  `src/fountain/notes.ts` is the second module of that kind: the one copy
  of `[[...]]` note stripping and `[[fmt: ...]]` parsing, imported by BOTH
  renderers (registry #18). Notes are invisible by spec, so anything that
  renders token text has to strip them; two regexes would drift.
- `src/epub/` + `src/mobi/` — fountain-js tokens → EPUB3 (jszip) / MOBI 6
  (hand-built PalmDB container for dependency-free USB sideload).
- `src/options.ts` — FormatOptions, the single knob surface: CLI
  `--options file.json` ↔ the window's Settings page. Defaults are pinned to
  root `format-defaults.json` by BOTH suites (options.test.ts,
  kit-check) — change all three together or a suite fails.
- `src/convert.ts` — orchestration + scanned/non-screenplay guards.
- `src/device/` + `src/export/` — device detection, USB copy, and every
  send route (Calibre and KFX builds, Books, Mail, Send to Kindle). The
  engine owns ALL of it.
- `desktop/` — the Tauri window. **No logic lives there**: the Rust shell
  spawns the engine sidecar and renders the JSON it prints, and the UI is
  static files with no bundler. See desktop/README.md and ADR 2026-09-12.
- `app/` — the FROZEN SwiftPM app (NO Xcode project; CommandLineTools only,
  so no XCTest/swift-testing, `kit-check` executable instead). Don't build
  new features there.

## Non-negotiable invariants

- **Classification always uses PLAIN text.** Styled text (fountain
  emphasis from PDF font styles) rides in `styledText` beside it; only
  dialogue/action emit styled. Markers in cues/parens/slugs break parsing.
- **EPUB CSS: horizontal in %, vertical in em, no max-width, no body
  line-height.** Kindle strips max-width and owns line-height. Two more
  hard NOs: **never `background-color` on html or body** — it makes the KFX
  converter synthesize its own wrapper block and every keep in the book
  then dies silently (MobileRead t=330798). This is a ban on the ROOT
  background, not a general fear of nesting: `section.scene` >
  `.dialogue-block` is device-confirmed to hold. But **a keep must sit on
  an element that carries TEXT, never on a structural wrapper** — that
  converter honors `break-after: avoid` on a `<p>` or `<h2>` and ignores it
  on a `<div>`, which stranded every cue in the book once (registry #8b,
  2026-09-14). Keeps are chains on paragraphs here, not wrappers. And **CSS value SYNTAX
  stays CSS-2.1-vintage** — no `min()`/`clamp()`/`var()`, because Adobe
  RMSDK (Kobo's EPUB path, tolino) violates CSS error handling and can
  blank a whole book on a value function it cannot parse; CSS3 PROPERTIES
  that degrade harmlessly are fine (`opacity` on the page marker is the
  precedent). See docs/screenplay-format-reference.md before touching
  css.ts, and docs/device-map.md §6 for what each renderer honors.
- **Kindles never index sideloaded EPUBs.** USB = AZW3 via Calibre's
  ebook-convert (with flags that stop Calibre re-breaking scenes and
  stripping div margins — see src/export/calibre.ts) or the engine's MOBI.
  **2026-07-29: sideloaded KFX renders with Enhanced Typesetting** — keeps
  hold, and it indexes — so KFX (Calibre + jhowell's KFX Output plugin +
  Kindle Previewer) beats AZW3 whenever that toolchain is present. The
  renderer follows the FORMAT, not the delivery route; registry §8b has
  the full verdict. AZW3/MOBI stay as fallbacks.
- **pdf.js:** getDocument TRANSFERS the buffer (never reuse bytes); modern
  build + DOMMatrix shim (legacy build breaks under bun test); fonts only
  resolve after getOperatorList (style detection needs it); the worker is
  statically imported for `bun build --compile`.
- Formatting behaviors are cataloged in **docs/formatting-options-log.md**
  — update it whenever one changes; it's the app's options registry.

## Working style

- TDD: failing test first (bun:test). Two fixture sets: `tests/fixtures/`
  is invented, committed, and runs in CI (regenerate with
  `tools/make-fixture.py`); root `/fixtures/` holds real scripts and is
  gitignored — never commit it, and never let a real title, author, or
  character name reach an assertion, a doc, or a screenshot.
- Committed fixtures: `screenplay.pdf` (clean happy path), `prose.pdf` and
  `blank-pages.pdf` (the not-a-screenplay and scanned guards), and
  **`torture.pdf`** — 14 sheets exercising every content-driven registry
  behavior, whose coverage is tracked row-by-row in
  `tools/torture-manifest.json`. `bun tools/device-checklist.ts` prints the
  device-side half of that manifest for a hardware pass. And
  **`field-station.pdf`**, the invented 18-page feature every README and site
  picture shows; the site's own scene sits on its pages 14 to 18, and
  `tests/field-station.test.ts` checks that. **`images.pdf`** pins the six
  rules that tell a picture from page furniture (registry #19): two pictures
  kept and four refused, each by a different rule, with every picture's margin
  to its one governing threshold stated in `make-fixture.py`. **`jpx.pdf`**
  pins the JPEG 2000 decoder pdf.js loads from outside its bundle: its one
  lossless picture must decode pixel for pixel, and `tools/smoke-cli.ts`
  converts it with each compiled CLI it smokes. All seven regenerate from
  `make-fixture.py`, and `tests/fixture-stability.test.ts` fails if any but
  `torture.pdf` stops reproducing byte-for-byte.
- No test may read or write the real app settings file: `bunfig.toml`
  preloads `tests/isolate-app-settings.ts` and root `.env.test` sets the
  same guard (SCREEPUB_CONFIG_DIR under `/dev/null/`). Run `bun test` from
  the repo root, not a subfolder, and give any test that needs settings its
  own scratch path. The same goes for Calibre: `tests/isolate-calibre-config.ts`
  points `CALIBRE_CONFIG_DIRECTORY` at a scratch COPY of the real Calibre
  folder for the whole run, and `.env.test` sets a guard path so a Calibre
  started without the test's environment fails instead of reaching the real
  one. Pass `process.env` to every Calibre spawn.
- Fixture sweep + epubcheck after any stage-1/CSS change. Test
  end-to-end with real PDFs too; outputs land in the app library folder.
- **The corpus diff is the tool for "does this actually matter?"** Convert
  every script in root `/fixtures/` with and without a change and diff the
  `.fountain`. It is what proved the dual-dialogue fix was fixture-only, the
  page-furniture fix was worth shipping (celtx 19 lines), and a pdf.js bump
  was inert. Synthetic fixtures cannot answer that question.
- **Rebuild `torture.pdf` before any device pass.** A stale build once put
  an already-fixed defect in front of a reviewer and nearly cost a verdict.
- After engine changes, rebuild the sidecar before running the window
  (`bun tools/build-sidecar.ts --host`): it embeds the engine.

## Context

Built first for an older USB-mass-storage Kindle that is often offline, so
USB is the primary route and everything works without a network. Public
repo, AGPL-3.0-or-later, © Darkwell Entertainment LLC; releases are
notarized DMGs built by CI on `v*` tags.
