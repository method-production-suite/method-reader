# Method Reader

Formerly Screepub (renamed 2026-10-16; the repo folder and the internal `sp-*`
crate names still say so). Part of the Method Production Suite.

Screenplay PDF → Fountain → reflowable EPUB3/MOBI/AZW3, with a native
window (Mac, Windows and Linux) that converts and sends to e-readers
(Kindle first; Kobo/tolino via USB volume signatures, reMarkable via its
USB web interface: see `rust/crates/sp-device`). All of it is Rust, one
Cargo workspace in `rust/`: the engine crates, the `method-reader` CLI (with `screepub` kept as an alias), the
egui window (which links the engine; there is no sidecar), the updater,
the release tools, the device-request Worker and the developer tools
(`cargo xtask`). The TypeScript engine, Bun and the Tauri window were
deleted at the cutover (2026-10-12; plan:
docs/superpowers/plans/2026-10-11-pure-rust-port.md). Plans and specs under
docs/superpowers/ that edit src/, tests/*.ts, desktop/ or tools/*.ts are
history, as are those that edit the SwiftUI Mac app deleted at piece F3
(docs/retired-coverage.md records what its checks covered).

## Commands

From `rust/` unless a line says otherwise:

```bash
cargo test --workspace      # every crate's tests (fixtures: tests/fixtures/ at the repo root)
cargo fmt --all --check     # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p sp-cli --bin method-reader -- <pdf>   # convert (see --help; --json is the app contract)
cargo run -p sp-app         # the window (bundles: cargo xtask build-app-bundle)
cargo xtask parity          # the release CLI against the TypeScript's frozen answers
epubcheck <out.epub>        # validate output (brew-installed)
cargo xtask capture-screens # retake README + site pictures (needs Chrome; macOS)
cargo xtask review-screens --out <page.html>   # which pictures changed, old beside new
cargo xtask bump-version <version>   # release bump: rust/Cargo.toml, Cargo.lock, the window's notes
cargo xtask mobi-picture-probe --out <folder>  # the .mobi picture-size probe books for an older Kindle
cargo xtask make-fixture <name> <out.pdf>      # regenerate a committed fixture
cargo xtask --help          # every tool
```

## Architecture

The crates, in the order a conversion passes through them (each one's
`lib.rs` opens with what it ports and why):

- `sp-pdf`: what pdf.js 6.3.289 gave the parser, in pure Rust: per page
  the text items, the font names behind them, the operators the parser
  walks, and each painted picture's pixels. hayro-syntax reads the file;
  the parts whose exact behaviour the parser depends on are ported from
  pdf.js (its data tables generated once from pdfjs-dist:
  `src/tables.rs`).
- `sp-parser`: PDF facts → classified elements. Adapted from an earlier
  table-read parser by the same author and heavily extended here; this is
  the only copy, so no cross-repo mirroring. Parser stays
  FORMAT-OPTION-FREE. `src/js.rs` holds the JavaScript semantics the
  TypeScript relied on silently (UTF-16 lengths, ECMAScript `\s` and
  `trim`, `Math.round`, `/i` folding).
- `sp-parser/src/cue.rs`: the ONE definition of what a character cue
  looks like as TEXT, used by classify.rs and by extract.rs's
  dual-dialogue detector. Third module of the shared-discriminator kind,
  and the one that proved why the pattern exists: there used to be two
  definitions and they disagreed, so a script whose lead was named "Q"
  rendered ordinary cues fine and collapsed a whole dual-dialogue scene
  into interleaved action. **Geometry stays with the caller** (classify
  owns the indent band, the cluster split owns the column test), because
  forcing the dual path to invent an indent is how the second copy got
  written.
- `sp-fountain`: elements → Fountain (`serialize.rs`), and a fountain-js
  1.2.4 tokenizer (`tokenizer.rs`) that starts every parse fresh (PARITY
  Decisions). The `.fountain` is a durable artifact and the app's cache
  boundary. Beside it, `slug.rs` owns the stage-2
  `PRIMARY_SLUG_SOURCE`/`is_mini_slug` discriminator every renderer
  imports (registry #5b; sp-parser keeps its own copy of the literal,
  pinned to this one by sp-epub's tests), and `notes.rs` is the second
  module of that kind: the one copy of `[[...]]` note stripping and
  `[[fmt: ...]]` parsing, used by EVERY renderer (registry #18). Notes are
  invisible by spec, so anything that renders token text has to strip
  them; two regexes would drift.
- `sp-epub` + `sp-mobi`: tokens → EPUB3 (its zip writer lays the container
  out as JSZip did) / MOBI 6 (hand-built PalmDB container for
  dependency-free USB sideload).
- `sp-azw3`: the EPUB's own bytes → AZW3 (KF8), with no Calibre: a port
  of Calibre's KF8 writer (credited in each file and
  THIRD-PARTY-NOTICES.md), reusing sp-mobi's PalmDB container. BETA until
  a Kindle has opened one (registry #20). It reads the EPUB back out of its
  zip rather than rendering tokens, so the AZW3 can never disagree with the
  book; tests/common/kf8_reader.rs reads it back structurally, and
  tests/readers.rs through Calibre's reader.
- `sp-fdx`: tokens → Final Draft (.fdx), the third renderer of the same
  tokens, for turning a PDF back into an editable script. Like the others
  it strips notes via sp-fountain's notes.rs and asks its slug.rs, so a
  mini-slug becomes a Shot, not a numbered Scene Heading.
  Format-option-free; the version it declares is one constant,
  `FDX_VERSION`.
- `sp-model`: the parser's types, the token shape, and FormatOptions
  (`src/options.rs`), the single knob surface: CLI `--options file.json` ↔
  the window's Settings page. Defaults are pinned to root
  `format-defaults.json` by `sp-model/tests/options.rs`: change options.rs
  and format-defaults.json together or it fails.
- `sp-engine`: orchestration (`convert.rs`, with the scanned and
  not-a-screenplay guards), the library, settings, update decisions, and
  every send route and export (Calibre's KFX build, our AZW3, Books, Mail,
  Send to Kindle). The engine owns ALL decisions. It takes its environment
  as a value (`env::Env`), never this process's own, which is also how
  tests stay off the real settings and Calibre folders.
- `sp-device` + `sp-fsutil` + `sp-mtp`: device detection, USB copy, eject,
  reveal, the reMarkable client, the Kindle ladder's device half; atomic
  writes and node:path's algorithms (`sp-fsutil/src/nodepath.rs`, the one
  copy; sp-engine re-exports it); the MTP helper (off until hardware day).
- `sp-cli`: the `method-reader` binary (and the `screepub` alias): the same flags, verbs and `--json`
  contract the TypeScript CLI had.
- `sp-app`: the window, drawn with egui, screen-reader accessible through
  AccessKit. **No logic lives there**: it calls the engine in-process
  through one `Engine` trait whose requests stand for the CLI's `--json`
  argv (`Request::argv`, held to the Tauri window's own by
  tests/argv_contract.rs), and draws the answers. The book preview is the
  engine's own XHTML and css.rs rendered by Blitz, so the window and the
  book cannot disagree. Design: docs/superpowers/specs/2026-10-11-sp-app-design.md.
- `sp-update` + `sp-release`: the updater (latest.json, minisign, the
  bundle swap) and the release pipeline's library (bundle layouts, update
  archive, latest.json, the signer). `sp_update::PUBKEY` is the one home of
  the updater's public key.
- `sp-device-requests`: the "My device isn't here" service, a Cloudflare
  Worker (workers-rs, wasm32) with D1; its schema, migrations and
  wrangler.toml sit beside it.
- `xtask`: the developer tools (`cargo xtask --help`), among them the
  parity harness against the frozen TypeScript answers (rust/oracle/,
  committed; rust/oracle-real/, gitignored) and the fixture generator.

## Non-negotiable invariants

- **Classification always uses PLAIN text.** Styled text (fountain
  emphasis from PDF font styles) rides in `styled_text` beside it; only
  dialogue/action emit styled. Markers in cues/parens/slugs break parsing.
- **EPUB CSS: horizontal in %, vertical in em, no max-width, no body
  line-height.** Kindle strips max-width and owns line-height. Two more
  hard NOs: **never `background-color` on html or body**: it makes the KFX
  converter synthesize its own wrapper block and every keep in the book
  then dies silently (MobileRead t=330798). This is a ban on the ROOT
  background, not a general fear of nesting: `section.scene` >
  `.dialogue-block` is device-confirmed to hold. But **a keep must sit on
  an element that carries TEXT, never on a structural wrapper**: that
  converter honors `break-after: avoid` on a `<p>` or `<h2>` and ignores it
  on a `<div>`, which stranded every cue in the book once (registry #8b,
  2026-09-14). Keeps are chains on paragraphs here, not wrappers. And **CSS value SYNTAX
  stays CSS-2.1-vintage**: no `min()`/`clamp()`/`var()`, because Adobe
  RMSDK (Kobo's EPUB path, tolino) violates CSS error handling and can
  blank a whole book on a value function it cannot parse; CSS3 PROPERTIES
  that degrade harmlessly are fine (`opacity` on the page marker is the
  precedent). See docs/screenplay-format-reference.md before touching
  `rust/crates/sp-epub/src/css.rs`, and docs/device-map.md §6 for what each
  renderer honors.
- **Kindles never index sideloaded EPUBs.** USB = AZW3 or KFX.
  **2026-07-29: sideloaded KFX renders with Enhanced Typesetting**: keeps
  hold, and it indexes, so KFX (Calibre + jhowell's KFX Output plugin +
  Kindle Previewer, through ebook-convert with the flags that stop Calibre
  re-breaking scenes and stripping div margins; see
  `rust/crates/sp-engine/src/export/calibre.rs`) beats AZW3 whenever that
  toolchain is present. The renderer follows the FORMAT, not the delivery
  route; registry §8b has the full verdict. **2026-10-10: everywhere else
  the AZW3 is Method Reader's own** (sp-azw3, beta, registry #20), even where
  Calibre is installed; Calibre is for KFX only. The engine's MOBI is the
  file for the oldest Kindles, asked for by name, never the ladder's answer.
- **sp-pdf reproduces pdf.js, it does not improve on it.** The parser's
  thresholds were tuned on pdf.js's numbers, so a "better" text item is a
  regression: change sp-pdf only to match pdf.js 6.3.289 more exactly
  (rust/oracle/<fixture>/pages.json is what pdf.js answered). Text and
  operators come first (`read_pages`, no pixels), and the scanned and
  not-a-screenplay guards run on them before any picture is decoded; each
  picture's pixels are fetched only as the parser reaches it
  (`image_fact`), inside the picture budget, which is wall-clock (so the
  workspace builds the pixel crates at opt-level 3 even in tests). Font
  names come from the operator walk, which style detection needs. The JPEG
  2000 decoder is compiled in (hayro-jpeg2000): there is no decoder folder
  to find at run time. Exact numbers matter: JSON reads use serde_json's
  `float_roundtrip`.
- **The TypeScript is the answer key, frozen.** rust/oracle/ holds what the
  TypeScript engine made of each committed fixture, stage by stage, and
  rust/oracle/parity/ what its CLI answered for every command row; the
  tests and `cargo xtask parity` compare against them. Never edit a frozen
  answer to make a check pass: a difference is a bug, or a decision written
  into rust/PARITY.md for the owner. A deliberate output change goes through
  `cargo xtask parity --accept <row>...` (it shows each row's diff and
  rewrites only those rows), with the reason in the commit.
- Formatting behaviors are cataloged in **docs/formatting-options-log.md**.
  Update it whenever one changes; it's the app's options registry.

## Working style

- TDD: failing test first (`cargo test`). Two fixture sets:
  `tests/fixtures/` is invented, committed, and runs in CI (regenerate with
  `cargo xtask make-fixture`); root `/fixtures/` holds real scripts and is
  gitignored. Never commit it, and never let a real title, author, or
  character name reach an assertion, a doc, or a screenshot.
- Committed fixtures: `screenplay.pdf` (clean happy path), `prose.pdf` and
  `blank-pages.pdf` (the not-a-screenplay and scanned guards), and
  **`torture.pdf`**: 17 sheets exercising every content-driven registry
  behavior, whose coverage is tracked row-by-row in
  `tools/torture-manifest.json`. `cargo xtask device-checklist` prints the
  device-side half of that manifest for a hardware pass. And
  **`field-station.pdf`**, the invented 18-page feature every README and site
  picture shows; the site's own scene sits on its pages 14 to 18, and
  sp-parser's and sp-engine's `tests/field_station.rs` check that.
  **`images.pdf`** pins four of the six rules that tell a picture from page
  furniture (registry #19): two pictures kept and four refused, one by each
  of pixels, width, furniture and background, with every picture's margin
  to its one governing threshold stated in the fixture generator
  (`rust/crates/xtask/src/fixture/`). Height and area are pinned only by
  hand-built candidates in `sp-parser/tests/artwork.rs`. **`jpx.pdf`** pins
  the JPEG 2000 decoder: its one lossless picture must decode pixel for
  pixel (`sp-pdf/tests/jpx.rs`), and `cargo xtask smoke-cli` converts it
  with each CLI it smokes. All seven regenerate from
  `cargo xtask make-fixture`, and `xtask/tests/fixture.rs` fails if any but
  `torture.pdf` stops reproducing byte-for-byte.
- No test may read or write the real app settings file or the real Calibre
  folder. The engine takes its environment as a value: tests hand it
  sp-engine's `common::guarded_env()` (the settings and Calibre guard paths
  under /dev/null, which nothing can create), and a test that lets Calibre
  run hands it `common::calibre_env()`, a scratch COPY of the real Calibre
  folder. Every Calibre spawn gets exactly the `Env` it is handed.
  `sp-engine/tests/guards.rs` holds both rules. Never run Calibre,
  calibre-debug, ebook-convert, calibre-customize or Kindle Previewer
  outside such a test, not even `--version`: it rewrites the owner's
  settings.
- Fixture sweep + epubcheck after any stage-1/CSS change. Test
  end-to-end with real PDFs too; outputs land in the app library folder.
- **The corpus diff is the tool for "does this actually matter?"** Convert
  every script in root `/fixtures/` with and without a change and diff the
  `.fountain`. It is what proved the dual-dialogue fix was fixture-only, the
  page-furniture fix was worth shipping (celtx 19 lines), and a pdf.js bump
  was inert. Synthetic fixtures cannot answer that question.
  `cargo xtask parity` runs the real scripts too, against their frozen
  answers in rust/oracle-real/ (counts only, never names).
- **Rebuild `torture.pdf` before any device pass.** A stale build once put
  an already-fixed defect in front of a reviewer and nearly cost a verdict.

## Context

Built first for an older USB-mass-storage Kindle that is often offline, so
USB is the primary route and everything works without a network. Public
repo, MIT OR Apache-2.0 (sp-azw3 alone GPL-3.0-only, being a Calibre
port; nothing else may take GPL code), © Method Production Suite Team and the Method Reader contributors;
releases are
notarized DMGs built by CI on `v*` tags.
