# `app/` is frozen, and not yet deletable

If you found this directory while cleaning up: **not yet, and the reasons are
specific.** Each one below is checkable rather than a matter of taste.

Frozen means: maintained for bug compatibility only. Fix a defect a shipping
user hits. Do not write new Swift against `ScreepubKit`, do not add a file,
do not port a feature *into* this directory. New behaviour goes in `src/`
(the engine) and is driven from `desktop/` (the Tauri window).

## Who decided this, and where

- ADR: `docs/adr/2026-09-12-cross-platform-tauri.md`: one Rust window over
  the TypeScript engine, replacing the macOS-only SwiftUI app.
- The retirement is piece F of that ADR, specified in
  `docs/superpowers/specs/2026-09-14-retire-swiftui-design.md` in three
  stages: **F1 freeze** (this file), **F2 hand over** (the identifier
  release), **F3 delete**.
- The freeze is enforced: `tools/frozen-app-manifest.json` pins this
  directory's Swift file list and each file's code-line count, and
  `tests/frozen-app.test.ts` fails if either grows. Read that test's header
  before trying to make it pass; regenerating the manifest is deliberately
  not enough.

## What replaces it

`desktop/`: a Tauri window over the same engine in `src/`, plus the
`screepub` CLI. It ships on every tag for macOS, Windows and Linux, and on a
Mac it is signed, notarized and updates itself.

## Why it is still here

**1. It is still what Mac users are given.** `Screepub-macOS.dmg` is the
README's and the site's Mac download, `brew install --cask
ssandweiss/tap/screepub` installs it, and `app/release.sh` signs and
notarizes it on every tag. The identifier release, which gives the window
the `com.darkwell.screepub` identifier and the Screepub name, is planned and
has not happened. Deleting this directory first leaves Mac users with a dead
download button and a dead cask.

**2. Its `kit-check` is the only test of its own Swift code.** While the app
ships, its behaviour is worth asserting. The sections that matter most for
the retirement cover `ResultActions.swift` (the send menu),
`UpdateCheck.swift` and `UpdateInstall.swift` (its updater), and
`AppleBooks.swift`. `docs/retired-coverage.md` records, section by section,
what the window and `src/` now do in their place and what was decided as
an accepted loss.

**3. Two things here have no copy anywhere else.**

- `Packages/KFXKit/Sources/KFXKit/Vendor/KFX_Output_plugin.zip`, with its
  `PROVENANCE.md` and GPL-3 `COPYING`. This app installs the plugin from that
  zip. The engine and the window do not need it: `screepub kfx-install`
  fetches the plugin from Calibre's own plugin index. `THIRD-PARTY-NOTICES.md`
  points at the two files.
- `Sources/ScreepubApp/Theme.swift` is the declared source of truth for the
  brand colours. `tests/theme-colors.ts` parses it and
  `tests/brand-tokens.test.ts` pins `brand/tokens.json` to it. Deleting it
  means reversing that test's direction and editing `brand/README.md` and
  `brand/tokens.json`'s `$comment` too.

`format-defaults.json` is pinned by **both** `tests/options.test.ts` and
`kit-check` (see the root `CLAUDE.md`). While this directory lives, that is
a double pin for the keys Swift knows: changing one of their defaults means
changing `FormatSettings.defaults` too. A NEW key needs no Swift, and must
not get any: kit-check decodes with `JSONDecoder`, which ignores keys it
does not know, and the freeze guard refuses Swift edits.

## When it may go

On three gates, stated in full in the spec:

1. **The window is built green on `macos-15` in CI, and a person has
   installed its DMG on a real Mac and converted a script.** Met: see
   `docs/verification-ledger.md`.
2. **Every feature this app has is ported, or named in
   `docs/releases/<version>.md` as a thing that went away.** In progress:
   `docs/retired-coverage.md` tracks each one.
3. **A Mac user can get the new app where they get this one**: the Homebrew
   cask points at the window's download and someone has installed it through
   `brew`. Not met; this is the identifier release.

Then F2 (the identifier release: the window takes this app's identifier,
and this app's own updater carries its users across, as
`docs/adr/2026-09-20-swift-app-migrates-itself.md` decides), and only after
a release cycle, F3 (delete, with the reference sweep in
`tools/app-references.json`).

## Meanwhile

CI builds this directory and runs `kit-check` (`.github/workflows/ci.yml`,
the `app` job), and `weekly-toolchain.yml` runs its Calibre checks every
Monday. Both are coverage for a shipping app. They go when the directory
goes.
