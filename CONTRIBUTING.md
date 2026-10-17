# Contributing to Method Reader

Bug reports are as welcome as patches, especially from people reading
scripts on devices nobody here owns.

## Reporting a bug

[Open an issue](https://github.com/method-production-suite/method-reader/issues/new/choose).
Say which computer (Mac, Windows or Linux) and which Method Reader: the app or
the command line. For a conversion
problem, the useful details are what the PDF was written in (Final Draft,
Highland, Fade In, Celtx, WriterDuet…), what came out wrong, and what you
expected.

**Please don't attach a confidential script.** Most conversion bugs are about
*layout*, not words: a description of where the text sat on the page is
usually enough, and `cargo xtask make-fixture` (its source is
`rust/crates/xtask/src/fixture/`) shows how to build a small invented script
that reproduces a given shape.

Kobo, tolino and reMarkable are written but have never run on real hardware,
and the Windows and Linux downloads have never been installed by a person
([the verification ledger](docs/verification-ledger.md)). A report either
way is genuinely valuable.

## Getting set up

Method Reader is one Cargo workspace in `rust/`: the engine, the command line and
the window. You need a Rust toolchain from [rustup](https://rustup.rs). The
version is pinned in `rust/rust-toolchain.toml`, and rustup installs it (with
clippy and rustfmt) the first time you run `cargo` in `rust/`.

Run everything from `rust/`:

```bash
cargo test --workspace                                 # every crate's tests
cargo fmt --all --check                                # formatting
cargo clippy --workspace --all-targets -- -D warnings  # lints, warnings as errors
cargo run -p sp-cli --bin method-reader -- <script.pdf>     # convert one
cargo run -p sp-app                                    # open the window
```

The window links the engine in, so there is nothing else to build first.

**On Linux** the window needs the windowing and sound headers. CI installs
them with apt; the list is in `.github/workflows/rust.yml` (the "Linux tools
and headers" step), and today it is:

```bash
sudo apt-get install -y sqlite3 libasound2-dev libfontconfig1-dev libxkbcommon-dev libwayland-dev libgl-dev libgtk-3-dev
```

(`sqlite3` is for the device-request service's tests, not the window.)

**On an Intel Mac** the window is not offered: Method Reader ships it for Macs
with Apple silicon only, and `cargo xtask build-app-bundle` refuses to make
an Intel bundle. An Intel Mac can still build and run the command line from
source with `cargo`, and can cross-build the Apple silicon bundle
(`rustup target add aarch64-apple-darwin`, then `--arch arm64`).

`cargo xtask --help` lists the developer tools (fixtures, screenshots,
release helpers). [docs/developers.md](docs/developers.md) covers the
command line, the library and the architecture, and
[the window's design](docs/superpowers/specs/2026-10-11-sp-app-design.md)
covers how the window is built and tested.

`epubcheck` (`brew install epubcheck`) validates output and is worth running
after any change to the EPUB or CSS builders.

## Fixtures

`tests/fixtures/` holds small invented screenplays, committed and used by CI.
Regenerate them with `cargo xtask make-fixture <name> <out.pdf>` from
`rust/`, for example:

```bash
cargo xtask make-fixture screenplay ../tests/fixtures/screenplay.pdf
```

The names are `screenplay`, `prose`, `blank` (blank-pages.pdf), `images`,
`jpx`, `torture` and `demo` (field-station.pdf).

A root-level `fixtures/` directory is gitignored, for testing against real
scripts locally. If you use it: **never commit those files, and never let a
real title, author, or character name reach a test assertion, a doc, or a
screenshot.** Some tests skip themselves when it's absent; that's expected.

## Before you send a change

- **Write the failing test first.** The suite is `cargo test`.
- **Read the invariants** in [`CLAUDE.md`](CLAUDE.md) before touching the
  parser or the EPUB CSS. Each is a device behaviour someone verified on real
  hardware, and
  [`docs/screenplay-format-reference.md`](docs/screenplay-format-reference.md)
  explains why Kindle's renderer forces several of them.
- **Log formatting changes** in
  [`docs/formatting-options-log.md`](docs/formatting-options-log.md), the
  registry of every formatting option.
- **Keep defaults in sync.** `rust/crates/sp-model/src/options.rs` and the
  root `format-defaults.json` hold the same defaults, and
  `defaults_match_the_canonical_format_defaults_json` in
  `rust/crates/sp-model/tests/options.rs` fails if they differ; change them
  together.
- **Keep the window logic-free.** Decisions belong in the engine crates;
  `sp-app` asks the engine and draws its answer.
- **Run `cargo xtask parity`** after any change to what the command line
  writes or says. It holds the Rust CLI to the frozen answers in
  `rust/oracle/`. A deliberate output change: run parity, read the diff,
  accept the rows (`cargo xtask parity --accept <row>...`, which shows each
  row's diff and rewrites only those rows' frozen answers), and explain it
  in the commit. Never accept a row to make a difference you cannot explain
  go away.
- Run `cargo fmt --all --check`, the clippy line above and
  `cargo test --workspace` before you push.

## What CI runs

- **Every push** (`rust.yml`): formatting, clippy with warnings as errors,
  and every test on macOS, Windows and Linux; the parity check against the
  TypeScript engine's frozen answers, on macOS; the release command line
  converting the fixtures, with `epubcheck` on its books; the device-request
  service built for wasm32; and every installer built, opened and checked on
  its own system.
- **Every `v*` tag** (`release.yml`): a draft release. It signs and
  notarizes the Mac downloads, builds every bundle and archive, checks each
  one before uploading, attaches the window's update manifest, and checks
  the draft carries every file. A person checks the Mac image and publishes
  the draft; that runs `release-published.yml`, which bumps the Homebrew
  formula and reads the release back. The steps are in
  [docs/RELEASING.md](docs/RELEASING.md).
- **Weekly**: the Homebrew formula and the update manifest still match the
  newest release, and the retired Homebrew cask still says it is retired
  (deprecated, and disabled from the release after 0.9.0); that is
  `tap-freshness.yml`. And the whole test suite still passes with Calibre's
  newest version installed (`weekly-toolchain.yml`).

## Licensing of contributions

Method Reader is MIT OR Apache-2.0, except `rust/crates/sp-azw3`, which is
GPL-3.0-only. By opening a pull request you agree your contribution ships
under the licence of the code it changes (both MIT and Apache-2.0, for
everything outside sp-azw3), and you confirm you have the right to
contribute it: the [Developer Certificate of Origin](https://developercertificate.org).
Add a `Signed-off-by:` line to your commits (`git commit -s`) to certify it.
