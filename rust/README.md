# Screepub in Rust

The pure-Rust port of Screepub, built on the `rust-port` branch and switched
over in one release when it matches the TypeScript engine on every fixture.
The plan, the crate map and **the rules every helper follows** are in
[docs/superpowers/plans/2026-10-11-pure-rust-port.md](../docs/superpowers/plans/2026-10-11-pure-rust-port.md).
Read them before starting a crate.

## Layout

```text
rust/
  Cargo.toml            the workspace; shared dependency versions live here
  rust-toolchain.toml   the pinned toolchain (rustup installs it on first use)
  .cargo/config.toml    the `cargo xtask` alias
  crates/
    sp-model/           parser types, FormatOptions and presets, the fountain-js token shape
    xtask/              the oracle and parity commands
    sp-update/          the updater: latest.json, minisign, the bundle swap, the restart
    sp-release/         the release tools (binary sp-release): layouts, update archive, latest.json
    sp-device-requests/ the device-request Worker (workers-rs, wasm32; not deployed)
    ...                 one folder per crate in the plan's table, as each lands
  handover-proof/       the Tauri window's own updater, headless: the release's
                        handover gate (its own workspace; tools/rehearse-handover.sh)
  oracle/               the TypeScript engine's answers for tests/fixtures (committed)
  oracle-real/          the same for the real scripts in root fixtures/ (gitignored)
  PARITY.md             every TypeScript test file and window feature, and who ports it
```

Every crate sets `[lints] workspace = true`, which forbids `unsafe`.

## The oracle

`rust/oracle/<fixture>/` is what the TypeScript engine makes of each
committed fixture, stage by stage, so a crate can be built and checked
before the crates upstream of it exist:

| File | Stage | Rust type (sp-model) |
|---|---|---|
| `pages.json` | what pdf.js hands the parser, per page: text items, resolved font names, the operators the parser walks, picture facts | (sp-pdf defines it) |
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

Regenerate after any engine change (needs bun and `bun install`):

```bash
cargo xtask oracle                   # committed fixtures and real scripts
cargo xtask oracle --committed-only  # committed fixtures only
bun tools/oracle/dump.ts <file.pdf|file.fountain> --out <dir> [--options f.json]
```

A dump is reproducible: two dumps of one input are byte-identical. The
books carry times, so the dump freezes the clock at `SOURCE_DATE_EPOCH`
(default 1767225600, 2026-01-01T00:00:00Z) and runs with `TZ=UTC`, because a
zip's dates are local time. **The Rust CLI must honor both the same way**:
`SOURCE_DATE_EPOCH` for the EPUB's `dcterms:modified` and zip dates, the
PalmDB header times and the AZW3's EXTH 106.

## Parity

```bash
cargo xtask parity                       # every committed fixture and real script
cargo xtask parity --committed-only      # CI-safe: invented fixtures only
cargo xtask parity --fixture torture --command convert
cargo xtask parity --rust-cli path/to/screepub
cargo xtask parity --against-ts          # the harness against the TypeScript alone
```

Both CLIs get the same arguments; a scratch folder each (a copy of the input,
empty settings, an empty library); the same `SOURCE_DATE_EPOCH` and `TZ`;
the device seams pinned; and Calibre at a guard path, or, for the rows that
ask Calibre a question, a scratch COPY of its settings. Compared: the exit
code, stdout (the `--json` answer as values, or a person's text) with paths
and times normalized, stderr where the row asks, the EPUB entry by entry,
MOBI and AZW3 record by record, and the FDX, preview, `.fountain` and
folder trees byte for byte. Pictures compare by their decoded pixels
wherever they appear, and `--debug`'s elements as JSON values (PARITY.md,
Decisions). Real scripts are reported as counts only.

The Rust CLI is `crates/sp-cli` (binary `screepub`), built in the release
profile. Commands are rows in `crates/xtask/src/table.rs`: every verb of
`--help` and every flag combination the TypeScript CLI tests exercise. A
row can write files into its scratch folder first and run earlier steps
with the same side's CLI (a conversion before an `export`). `--command`
takes a row name or a prefix ending in `*` (`--command 'export-*'`).

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs all three on Linux, macOS and Windows (.github/workflows/rust.yml).

## Rules, in short

The plan has them in full. The ones that bite:

- Work in your own worktree on a branch from `rust-port`, in your crate's
  folder and the files your brief names. A `Cargo.lock` conflict is resolved
  by regenerating it.
- TDD against the oracle. Never weaken a parity check to pass it: a
  difference is a bug, or a decision written into PARITY.md for the owner.
- Never run Calibre, calibre-debug, ebook-convert, calibre-customize or
  Kindle Previewer outside a test that points `CALIBRE_CONFIG_DIRECTORY` at a
  scratch copy, not even `--version`.
- Never touch the real app settings or library, and never let a real
  script's title, author or character name reach code, tests, docs or commits.
- No push, merge to main, tag or release.
