# Contributing to Screepub

Bug reports are as welcome as patches, especially from people reading
scripts on devices nobody here owns.

## Reporting a bug

[Open an issue](https://github.com/ssandweiss/screepub/issues/new/choose).
Say which computer (Mac, Windows or Linux) and which Screepub: the app or
the command line. For a conversion
problem, the useful details are what the PDF was written in (Final Draft,
Highland, Fade In, Celtx, WriterDuet…), what came out wrong, and what you
expected.

**Please don't attach a confidential script.** Most conversion bugs are about
*layout*, not words: a description of where the text sat on the page is
usually enough, and `tools/make-fixture.py` shows how to build a small
invented script that reproduces a given shape.

Kobo, tolino and reMarkable are written but have never run on real hardware,
and the Windows and Linux downloads have never been installed by a person
([the verification ledger](docs/verification-ledger.md)). A report either
way is genuinely valuable.

## Getting set up

**The engine and the command line** need [Bun](https://bun.sh):

```bash
bun install
bun test                    # engine suite (run it from the repository root)
bunx tsc --noEmit           # typecheck
bun src/cli.ts <script.pdf> # convert one
```

**The window** (`desktop/`) also needs a Rust toolchain
([rustup](https://rustup.rs), which brings `cargo`):

```bash
bun tools/build-sidecar.ts --host    # build the engine the window runs
cd desktop/src-tauri && cargo run    # open the window
```

An Intel Mac cannot run the window any more: it is built for Macs with
Apple Silicon only, and `--host` refuses an Intel Mac with that reason.
The engine still runs there from source (`bun src/cli.ts`); cross-building
the Apple Silicon bundle from one is in desktop/README.md.

[desktop/README.md](desktop/README.md) covers bundles, the Apple Silicon
Mac build and how the window talks to the engine.
[docs/developers.md](docs/developers.md) covers the command line, the
library and the architecture.

`epubcheck` (`brew install epubcheck`) validates output and is worth running
after any change to the EPUB or CSS builders.

**The older Mac app** (`app/`) is retired and frozen: no new features, and
it is no longer published (see [app/README-FROZEN.md](app/README-FROZEN.md)). Its own checks need
Xcode Command Line Tools on a Mac:

```bash
app/build-app.sh                             # the frozen Mac app
(cd app && swift run -c release kit-check)   # its behaviour checks
```

## Fixtures

`tests/fixtures/` holds small invented screenplays, committed and used by CI.
Regenerate them with `tools/make-fixture.py`, for example:

```bash
python3 tools/make-fixture.py screenplay tests/fixtures/screenplay.pdf
```

A root-level `fixtures/` directory is gitignored, for testing against real
scripts locally. If you use it: **never commit those files, and never let a
real title, author, or character name reach a test assertion, a doc, or a
screenshot.** Some tests skip themselves when it's absent; that's expected.

## Before you send a change

- **Write the failing test first.** The suite is `bun:test`.
- **Read the invariants** in [`CLAUDE.md`](CLAUDE.md) before touching the
  parser or the EPUB CSS. Each is a device behaviour someone verified on real
  hardware, and
  [`docs/screenplay-format-reference.md`](docs/screenplay-format-reference.md)
  explains why Kindle's renderer forces several of them.
- **Log formatting changes** in
  [`docs/formatting-options-log.md`](docs/formatting-options-log.md), the
  registry of every formatting option.
- **Keep defaults in sync.** `src/options.ts`, the root
  `format-defaults.json` and the frozen Mac app's `kit-check` pin the same
  defaults; change them together.
- **Keep the window logic-free.** Decisions belong in `src/`; the window only
  builds argument lists and draws the engine's answer.
- Run `bun test` and `bunx tsc --noEmit` before you push.

## What CI runs

- **Every push** (`ci.yml`): typecheck and `bun test` on Linux, the
  command-line archives for every platform, and a Mac job that builds the
  frozen Mac app and runs its checks.
- **Every push** (`desktop.yml`): the window compiled and bundled on macOS,
  Windows and Linux, with the engine inside each bundle run against a fixture.
- **Every `v*` tag** (`release.yml`): the release. It signs and notarizes the
  Mac downloads, builds every bundle and archive, checks each one before
  uploading, and publishes the window's update manifest.
- **Weekly**: the Homebrew formula and the update manifest still match the
  newest release, the retired Homebrew cask still says it is deprecated, and
  the frozen Mac app's checks still pass against Calibre's newest version.

## Licensing of contributions

Screepub is AGPL-3.0-or-later. By opening a pull request you agree your
contribution ships under that license, and you confirm you have the right to
contribute it: the [Developer Certificate of Origin](https://developercertificate.org).
Add a `Signed-off-by:` line to your commits (`git commit -s`) to certify it.
