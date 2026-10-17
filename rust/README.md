# Method Reader in Rust

All of Method Reader: the engine, the `method-reader` command line, the window, the
updater, the release tools, the device-request service and the developer
tools, in one Cargo workspace. It was ported from the TypeScript engine and
the Tauri window, and switched over on 2026-10-12 once it matched them on
every fixture. The plan, the crate map and the rules the port followed are
in [docs/superpowers/plans/2026-10-11-pure-rust-port.md](../docs/superpowers/plans/2026-10-11-pure-rust-port.md);
[PARITY.md](PARITY.md) is the ledger of every TypeScript test and window
feature and where its checks live now, with the decisions taken on the way.

## Layout

```text
rust/
  Cargo.toml            the workspace; the one version file; shared dependency versions
  rust-toolchain.toml   the pinned toolchain (rustup installs it on first use)
  deny.toml             cargo deny check: advisories, licenses, sources (rust.yml runs it)
  .cargo/config.toml    the `cargo xtask` alias
  crates/
    sp-model/           parser types, FormatOptions and presets, the token shape
    sp-pdf/             what pdf.js gave the parser: text items, fonts, operators, pictures
    sp-parser/          PDF facts to classified elements
    sp-fountain/        elements to Fountain; notes, slugs, the fountain-js tokenizer
    sp-epub/            tokens to EPUB3 and the preview
    sp-mobi/            tokens to MOBI 6, and the PalmDB container
    sp-azw3/            the EPUB to AZW3 (KF8), ported from Calibre's writer (beta)
    sp-fdx/             tokens to Final Draft (beta)
    sp-fsutil/          atomic writes, file replacement, node:path's algorithms
    sp-device/          readers on USB, the reMarkable, eject, reveal, the MTP route (off)
    sp-mtp/             the MTP helper binary (method-reader-mtp; off until hardware day)
    sp-engine/          conversion, library, settings, update decisions, export, routes
    sp-cli/             the `method-reader` binary and its --json contract
    sp-app/             the window (egui and AccessKit), its fonts and icons
    sp-update/          the updater: latest.json, minisign, the bundle swap, the restart
    sp-release/         the release tools (binary sp-release): layouts, update archive, latest.json
    sp-device-requests/ the device-request Worker (workers-rs, wasm32), its schema and wrangler.toml
    xtask/              the developer tools (`cargo xtask --help`)
  handover-proof/       the Tauri window's own updater, headless: the release's
                        handover gate (its own workspace; cargo xtask rehearse-handover)
  oracle/               the TypeScript engine's answers for tests/fixtures (committed, frozen)
  oracle-real/          the same for the real scripts in root fixtures/ (gitignored)
  PARITY.md             every TypeScript test file and window feature, and who carries it now
```

Every crate sets `[lints] workspace = true`, which forbids `unsafe`.

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask parity --committed-only   # add the real scripts by leaving the flag off
cargo deny check                      # advisories, licenses, sources (rust/deny.toml)
```

CI runs the first three on Linux, macOS and Windows, the parity on macOS,
and cargo deny on Linux (.github/workflows/rust.yml).

## The oracle: the TypeScript's answers, frozen

The TypeScript engine is gone, but what it answered is kept as data, and
the Rust is held to it.

`rust/oracle/<fixture>/` is what the TypeScript engine made of each
committed fixture, stage by stage:

| File | Stage | Rust type (sp-model) |
|---|---|---|
| `pages.json` | what pdf.js handed the parser, per page: text items, resolved font names, the operators the parser walks, picture facts | (sp-pdf defines it) |
| `lines.json` | extract.ts's answer: `RawLine`s and the pictures kept | `Vec<RawLine>`, `RawImage` |
| `elements.json` | the parser's answer (the CLI's `--debug` dump) | `ParsedScreenplay` |
| `script.fountain` | the serializer's answer | text |
| `tokens.json` | fountain-js tokens, as every renderer receives them | `Vec<Token>` |
| `book.epub`, `book.mobi`, `book.azw3`, `book.fdx`, `book.preview.html` | the books | bytes |
| `images/` | the picture folder | PNG bytes |
| `convert.json` | the CLI's `--json` answer, scratch folder as `$WORK` | JSON |
| `meta.json` | input hash, clock, tool versions, every file's sha256 | JSON |

The two fixtures the engine refuses (`prose`, `blank-pages`) have only
`pages.json`, `lines.json` and the refusal in `convert.json`.

`rust/oracle/parity/<fixture>.json` is what the TypeScript CLI answered for
every row of the parity table (crates/xtask/src/table.rs): per row the exit
code, the `--json` answer, stderr where the row compares it, and a digest of
exactly what the harness compares of each output file. `rust/oracle-real/`
holds the same for the real scripts, on the machine that has them, and is
never committed.

Other frozen answers live beside the tests that read them: sp-engine's
`tests/data/parity-cli.json` (each verb handler against the TypeScript CLI),
sp-device's `tests/data/parity.json`, sp-fountain's fountain-js cases and
fuzz digests, sp-cli's help texts, sp-app's `tests/data/app-js-argv.json`
(the Tauri window's argv builders), and the generated tables and goldens
the transitional generators wrote (pinned by xtask's
`tests/generated_files.rs`).

The books carry times, so every answer was made with the clock frozen at
`SOURCE_DATE_EPOCH` (1767225600, 2026-01-01T00:00:00Z) and `TZ=UTC`, and the
Rust CLI honors both the same way: `SOURCE_DATE_EPOCH` for the EPUB's
`dcterms:modified` and zip dates, the PalmDB header times and the AZW3's
EXTH 106.

There is no TypeScript left to ask again. A deliberate change to what the
engine writes therefore shows up as a parity difference first. To make it
the new answer: run parity, read the diff, accept the rows, and explain it
in the commit (and in PARITY.md's Decisions when it is a choice the owner
should see):

```bash
cargo xtask parity --committed-only               # see what differs
cargo xtask parity --accept help help-short       # show each row's diff, rewrite only those rows
```

`--accept` refuses unless rows are named (exact names, or prefixes ending
in `*`; never a bare `*`), runs only those rows on the committed fixtures
(`--fixture` narrows it further), prints each row's diff against its frozen
answer, and rewrites just those rows in rust/oracle/parity/. A row this
machine cannot answer as the freezing Mac did (Calibre, the routes) is left
alone, and the real scripts' answers in rust/oracle-real/ are never
rewritten. The stage dumps in rust/oracle/<fixture>/ and the frozen answers
beside the tests are not rows; a test that reads one says what it pins.

## Parity

```bash
cargo xtask parity                       # every committed fixture and real script
cargo xtask parity --committed-only      # invented fixtures only
cargo xtask parity --fixture torture --command convert
cargo xtask parity --rust-cli path/to/method-reader
cargo xtask parity --accept <row>...     # a deliberate change (above)
```

The Rust CLI gets, per row, a scratch folder (a copy of the input, empty
settings, an empty library, a home folder of its own), the frozen clock and
`TZ`, the device seams pinned, and Calibre at a guard path, or, for the
rows that ask Calibre a question, a scratch COPY of its settings. Compared
with the frozen answer: the exit code, stdout (the `--json` answer as
values, or a person's text) with paths and times normalized, stderr where
the row asks, the EPUB entry by entry, MOBI and AZW3 record by record, and
the FDX, preview, `.fountain` and folder trees byte for byte. Pictures
compare by their decoded pixels wherever they appear, and `--debug`'s
elements as JSON values (PARITY.md, Decisions). Real scripts are reported
as counts only.

A few rows depend on what the machine has installed (Calibre and its KFX
plugin; Books, Send to Kindle and the mail handler for the routes). Each
answer file records the facts of the machine it was frozen on
(crates/sp-engine/tests/common/machine.rs), and those rows are compared
only where the facts read the same; elsewhere they are counted as skipped.

The Rust CLI is `crates/sp-cli` (binary `method-reader`), built in the release
profile. `--command` takes a row name or a prefix ending in `*`
(`--command 'export-*'`).

## No JavaScript runtime

Nothing in the workspace runs bun, node or a package runner, with one
exception: `cargo xtask rehearse-handover prepare` builds the old v0.11.1
Tauri window from its own tag, whose engine was TypeScript, so the
rehearsal can start from the window people have. Nothing it builds ships.
xtask's tests/no_js_runtime.rs fails if any other Rust file starts one.

## Rules, in short

- TDD. Never weaken a check to pass it: a difference is a bug, or a
  decision written into PARITY.md for the owner.
- Never run Calibre, calibre-debug, ebook-convert, calibre-customize or
  Kindle Previewer outside a test that points `CALIBRE_CONFIG_DIRECTORY` at
  a scratch copy, not even `--version`.
- Never touch the real app settings or library, and never let a real
  script's title, author or character name reach code, tests, docs or
  commits.
