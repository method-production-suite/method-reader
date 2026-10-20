# Design: retiring the SwiftUI app (piece F)

Date: 2026-09-14 · Status: draft
Program: [ADR 2026-09-12](../../adr/2026-09-12-cross-platform-tauri.md), piece F
Follows: [C](2026-09-13-tauri-shell-design.md), [D](2026-09-13-tauri-ui-design.md),
[E2](2026-09-14-app-bundles-design.md)

## Goal

Stop maintaining two applications, without taking anything away from the
people who already have one installed.

The ADR's one-line description of this piece is "Retire SwiftUI." This spec
takes the position that the line is a *destination*, not an instruction that
can be followed today, and specifies the conditions under which it becomes
one.

## The answer first: F cannot execute yet

**Piece F must not run now.** Not "should be careful"; must not. The
disqualifying fact is short enough to state in one sentence:

> The application that would replace the shipping Mac app **has never been
> launched or run on a Mac by a person**, and no one has seen it open past
> Gatekeeper.

**Updated 2026-09-14, after this spec was written:** the branch was pushed and
`desktop.yml` ran for the first time (run 34876329117). All three legs passed.
The shell now demonstrably *compiles* on macos-15 and windows-latest, a `.dmg`
and an NSIS installer were produced, and CI opened each and ran the engine out
of it. That closes the "never built" half of the sentence above and leaves the
half that matters for this gate: **a build is not a person.** Nobody has
mounted the DMG, cleared Gatekeeper, opened the window, or converted a script
on a Mac. `docs/mac-qa.md` is the checklist for doing exactly that, and gate 1
stays shut until someone works through it.

`.github/workflows/desktop.yml` says this about itself in its own header
comment: "As of the commit that added the bundle steps, this workflow had
never run: the branch was unpushed." Everything this program knows about
macOS bundling, signing and notarization for the Tauri app is read off
tauri-bundler's source, off `app/release.sh`'s scar tissue, and asserted
against fakes in `bun test`. `tests/build-app-bundle.test.ts` and
`tests/smoke-bundle.test.ts` spawn no `cargo` and open no DMG. The E2 spec
says so plainly and is right to: "No DMG, no `.app`, no NSIS installer has
been produced or opened by this project."

Meanwhile `app/` is the shipping product. It is what `README.md` links at the
top of the page, what `site/index.html` puts on three DOWNLOAD FOR MACOS
buttons, what `brew install --cask <owner>/tap/screepub` installs, and
what `app/release.sh` signs and notarizes on every tag. Deleting it before
its replacement has run once on the platform would leave every Mac user with
a dead cask, a dead download button, and an app that still works only until
they upgrade their OS.

There is a second reason, independent of the platform gap and in some ways
worse, because it would survive even a green macOS CI run: **`app/` still
holds behaviour and coverage that nothing else in the repository has.** See
"What was measured", items 2 and 3. Deleting `app/` today is not a
subtraction of duplicated code. It is a subtraction of features.

### The three gates

F executes when all three are true, each checkable by a human looking at a
specific artifact. None is a judgement call.

**Gate 1 — the replacement has run on a Mac, twice, by a person.**

1a. `.github/workflows/desktop.yml` has completed green on `macos-15`, on a
    pushed branch, with the bundle and smoke steps included. A run URL is
    recorded in this spec's follow-up or in `docs/releases/0.6.0.md`.
1b. A human has mounted the resulting DMG on a real Mac, dragged the app to
    `/Applications`, launched it, dropped a PDF on it, and got a book.
    **PASSED 2026-09-14.** `Screepub Desktop_0.6.0_universal.dmg` (the
    universal build, both slices) mounted, dragged to `/Applications`,
    launched from there, and used to convert TWO real feature scripts. The
    library holds them in the new per-script layout beside the Swift app's
    older flat files, so the coexistence §4 describes is observed, not
    predicted. Evidence beyond "a window opened": the app's own `.fountain`
    for one of those scripts is **byte-for-byte identical** to a fresh
    `bun src/cli.ts` run over the same PDF, and its EPUB matches the CLI's on
    scene headings (162) and mini-slugs (19). Since the `.fountain` is the
    cache boundary, identical bytes there is a stronger answer to "do the
    numbers agree" than reading four numbers off a screen. Its EPUB also
    carries both of the day's renderer fixes (no `keep-together`, and the
    multicol shadow rule), confirming the shipped bundle is the current
    engine and not a stale sidecar. Not
    "the engine ran out of the bundle in CI" — that is `smoke-bundle.ts`'s
    job and it is gate 1a. A window opening is a separate fact from a binary
    executing, and on macOS specifically: Gatekeeper, the notarization
    ticket, the hardened runtime and the sidecar's `posix_spawn` out of a
    quarantined bundle are four things that can each fail after a green CI
    run.
1c. **DEFERRED INDEFINITELY, 2026-09-14 by the owner.** He has a Windows
    machine; the gate waits anyway. It does not gate the macOS retirement —
    the spec already said so — and it is no longer treated as a thing
    pending soon. It gates calling 0.6.0 cross-platform, so either that
    claim waits with it or the release says macOS and Linux and means it.
    The original wording, kept because the bar is still right when someone
    picks it up: the same on Windows, at whatever fidelity is available. Windows is
    unsigned by decision (E2), so the bar here is "SmartScreen warned, the
    user clicked through, the app converted a script" — recorded, not
    assumed. Windows does not gate the *macOS* retirement on its own, but it
    gates calling 0.6.0 cross-platform.

**Gate 2 — the replacement does what the thing being retired does, or the
gap is written down and accepted by name.**

Measured below: the Tauri app today has **no updater**, **no Apple Books
route**, **no Send-to-Kindle route**, **no email-to-Kindle route**, **no
"save a copy" route**, and **no Cancel**. The Swift app has all six. Each
must be either ported, or listed in `docs/releases/<version>.md` as a thing
that went away, before the app that has them is withdrawn. Silently removing
a feature by deleting its only implementation is the failure mode this gate
exists to prevent.

**SETTLED 2026-09-14 by the owner: FIVE are ported, ONE is named as gone.**
Ported: the updater (full self-update, not notify-only), Apple Books,
Send-to-Kindle, email-to-Kindle, save-a-copy. **Cancel during conversion is
OUT**, and the reason is a rule rather than a cost: Cancel needs a third
Rust command, `desktop/README.md` records that having only `run_engine` and
`pick_file` is deliberate, and the ADR's governing principle is that Rust is
a window and not a brain. The owner's words: conversion is fast enough that
Cancel is close to irrelevant, and he would rather keep the surface to as
near one language as possible. So the shell stays at two commands, and
Cancel goes in `docs/releases/<version>.md` as a thing that went away.

Gate 2 is therefore a parity gate for five features plus one named loss, and
it is the largest thing standing between here and F3. Two consequences
the gate's original wording did not have to carry:

- Cancel being out is the one place this gate now says "named" rather than
  "ported", so the release notes owe it a line. A feature that disappears
  because a rule held is still a feature that disappeared.
- The updater is not just "notify"; it is the full self-update, which moves
  `self-update-installer`'s 26 checks from `accept-loss` to `port` in
  `docs/retired-coverage.md` on top of the 59 that hung on the product
  question. That is codesign pinning and an in-place bundle swap on three
  platforms, and it deserves its own piece rather than a bullet here.
- Apple Books is macOS-only by nature. Porting it means the cross-platform
  app has a route that exists on one platform, which is a thing the ADR's
  governing principle has an opinion about; worth a line in the piece that
  builds it rather than discovering it at review.

**Gate 3 — a Mac user has somewhere to get the new app.**

E2 deliberately deferred distribution channels and said the Tauri app "gets a
cask when it becomes *the* Mac app, which is piece F's business." So F owns
it.

**SETTLED 2026-09-14 by the owner: the Tauri app gets NO cask. The cask is
retired at F2 and the FORMULA stays.** The tap holds two different things and
only one of them is going: `Casks/screepub.rb` is the .app, and
`Formula/screepub.rb` is the CLI. The CLI's audience is exactly the people
who type `brew install`; the app's audience is screenwriters. Nothing has
been downloaded yet, so there is no installed base to strand.

Gate 3 therefore reads: **a Mac user has somewhere to get the new app, and
that somewhere is the releases page and the site.** Both already exist, so
gate 3 collapses from "build a cask and have a human install through brew"
to "point the two existing download references at the universal DMG." With
the universal build there is one artifact to point at rather than two.

This deletes what this spec called the single largest piece of real work in
F: the per-arch `on_arm`/`on_intel` stanzas and the fourth and fifth digest
lookups in `tools/bump-tap.sh`. Both are now moot rather than merely halved.

**Sequencing, and it matters.** The cask today serves the SWIFT app, which
still ships. Retiring it before F2 would strand anyone who installed that way
for no gain. So: the decision is recorded now, executed at F2, and it is
three things rather than one — deprecate the cask so `brew` tells people
rather than silently serving a stale app, retire the cask half of
`tap-freshness.yml` while the formula half stays, and drop the cask's digest
lookups from `bump-tap.sh`. A tap that keeps serving an app which no longer
ships is worse than no tap, and this project has already lived that once:
five releases served 0.3.0 from an asset the workflow had stopped
producing.

## What F designs

F is specified here as **three stages, in order**, because "retire" is not
one action and the three have very different risk.

### F1 — Freeze (executable now, before any gate)

No deletion. `app/` becomes read-only by convention and by check:

- `app/README-FROZEN.md` states that the directory is maintained for
  bug-compatibility only, names the ADR, and says what replaces it.
- `.github/workflows/ci.yml` keeps its `app` job (kit-check) — it is
  coverage, see below — but no new Swift is written against `ScreepubKit`.
- A `bun test` guard asserts `app/`'s Swift file list is unchanged against a
  committed manifest, so a future session cannot quietly extend it.

F1 costs one CI job per push and buys the ability to ship 0.6.0 with both
apps on the page, which is what E2 already assumes.

### F2 — Hand over (after gates 1 and 3)

The Tauri app becomes *the* Mac app. `app/` is still on disk.

- `tauri.conf.json`'s `productName` stays `Screepub`; the transition overlay
  is deleted (see item 4 below for what that actually costs).
- `tools/build-app-bundle.ts`'s DMG row loses the `Desktop` infix. Note the
  arch problem: the cask hardcodes one universal `Screepub-macOS.dmg`, while
  the Tauri path produces `Screepub-Desktop-macOS-{arm64,x64}.dmg` per arch
  (E2 measurement 6 — a universal sidecar cannot be built here). The cask
  therefore needs `on_arm`/`on_intel` stanzas with two urls and two SHAs, and
  `tools/bump-tap.sh` needs a fourth and fifth digest lookup. This is the
  single largest piece of real work in F and it is invisible from the ADR.
- `app/release.sh` stops being called by `release.yml`; the Swift DMG stops
  being published. The **last** Swift release is tagged and its notes say it
  is the last.
- `README.md` and `site/index.html`'s download references move to the Tauri
  artifacts.

An installed Swift app keeps working after F2. It is a signed, notarized
`.app` on the user's disk; nothing in this repository can reach it. Its
in-app updater (`UpdateCheck.swift`) polls GitHub releases for a `.dmg`
asset and will, after F2, find releases whose newest `.dmg` is the Tauri one.
**That is a live hazard, not a cosmetic one** — `UpdateInstall.swift` pins
codesign designated requirements (Apple anchor, our Team ID, our bundle
identifier) and swaps the bundle in place. The Tauri DMG carries identifier
`com.darkwell.screepub.desktop`, not `com.darkwell.screepub`, so the pin
should *refuse* it — which surfaces to the user as a failed update rather
than a wrong one.

**REVERSED 2026-09-20 by [ADR: the Swift app migrates
itself](../../adr/2026-09-20-swift-app-migrates-itself.md).** F2 now TAKES the
`com.darkwell.screepub` identifier and existing installs upgrade themselves
through the updater they already have. The universal build removed the
architecture-blindness that disqualified it, the decision to port a full
self-update removed the "payload deletes the updater" objection, and gate 2
becoming a parity gate removed the "features vanish" one. What remains, and what F2 now owns, is
SMALLER than first written: there is no library view in the new app, so
nothing appears to empty itself. The one real consequence is that per-script
TUNING is lost, because the Swift app writes `<stem>.screepub.json` flat in
the library root and the engine looks beside the input PDF and inside the
script's library folder, neither of which is that. Verified, not reasoned: a
flat sidecar asking for serif produced a Courier book. The owner then accepted a HARD
BREAK rather than carry a compatibility path: **F2 ships no migration at
all.** Anyone who tuned a script in the Swift app loses that tuning once,
their books are untouched, and the alternative was a branch in
`src/library.ts` existing solely to serve installs of an app being deleted. The paragraph below is the superseded reasoning,
kept because it is still an accurate account of the hazard.

**SETTLED 2026-09-14 by [ADR: how an installed Swift app gets off the Swift
app](../../adr/2026-09-14-swift-app-update-path.md).** Read it before
building F2; it changes this paragraph's conclusion in three ways. The
refusal is structural rather than probable (`identifier` is an exact match,
so the pin cannot pass). The refusal is also *late and repeating* — the DMG
requirement pins no identifier, so the user downloads and mounts the whole
image before the app inside fails, on every check, forever. So **the final
Swift release that disables the update check is the PRIMARY path, not the
fallback for an unverifiable refusal.** And the hazard is dormant until F2
itself wakes it: while both DMGs are published the updater takes the Swift
one, because `release.yml` uploads it first. That ordering is incidental, not
designed, and should not be read as the plan already working.

**WRONG, corrected 2026-09-21 by measuring it.** The hazard was never
dormant, and the reasoning above is wrong twice over. `UpdateCheck.swift`
takes `assets.first(where: { $0.name.hasSuffix(".dmg") })` over the array
GitHub's API returns, and that array is ordered by NAME, not by upload
time: on the real v0.6.0 release the `created_at` timestamps run
19:34, 19:34, 19:25, 19:25, 19:24, which is not upload order in any
reading. And by name, `Screepub-Desktop-macOS-universal.dmg` sorts BEFORE
`Screepub-macOS.dmg`. So the updater took the TAURI image the moment both
were published, and an installed 0.5.4 app checking for updates against
v0.6.0 downloaded it, mounted it, and refused it on the identifier pin.

That is the behaviour v0.6.0 wanted, so nothing was harmed: the refusal is
the design. What was wrong was believing the ordering protected anything.
Anyone reasoning from "the Swift one is first" about a future release would
have reached the opposite conclusion from the truth. Observed on a real Mac
2026-09-21: "the update failed signature verification and was not
installed", app still at rev 0.5.4.

The ADR also records why taking the `com.darkwell.screepub` identifier — the
only automatic migration — is rejected, and what would flip that. The short
version, and the part this spec had not connected: the old updater is
architecture-blind, so the per-arch DMG problem below is not only a cask
packaging inconvenience. On the updater path it would hand half of all users
an app that cannot run on their Mac.

### F3 — Delete (after gates 1, 2 and 3, and after F2 has been live through at least one release cycle)

`git rm -r app/`, plus the reference sweep below. This is the step the ADR
describes and it is the *last* one, not the only one.

## What F explicitly does not do

- It does not rewrite the engine, the parser, or `src/device/`. Piece A
  already moved what moves.
- It does not add Rust. The ADR's rule holds: Rust is a window, not a brain.
  Anything ported out of `app/` under gate 2 lands in `src/`.
- It does not build a Tauri auto-updater. Tauri's updater wants a key pair
  and an update manifest; that is its own piece with its own secrets
  question. F's obligation is only to say, in the app and in the notes, that
  it does not update itself — which `desktop/ui/notes.js` already does
  (`tests/desktop-notes.test.ts`: "it says plainly that it does not update
  itself").
- It does not touch `tests/fixtures/` or any parser behaviour.
- It does not delete the Homebrew tap. The tap serves a **formula** (the
  macOS CLI) as well as a cask, and the CLI is not being retired.

## What was measured

Every number here came from running something in the worktree on 2026-09-14,
not from reading the ADR.

**1. The size of the deletion.** `app/` is 42 files, 916 KB, 6,925 lines of
Swift + shell + entitlements. Of that, `app/Sources/KitCheck/main.swift` is
1,532 lines and 264 `check()` call sites.

**2. `kit-check`'s coverage was NOT fully replaced. About two thirds of it
has no counterpart in `bun test`.** This is the most important finding in
this spec and it contradicts the ADR's "`kit-check` becomes `bun test`".

Counting `check()` call sites by section of `app/Sources/KitCheck/main.swift`:

| kit-check section | lines | checks | replaced in `bun test`? |
| --- | --- | --- | --- |
| Kindle volume detection, copy semantics, multi-vendor classify, per-vendor destinations | 28-79 | 16 | **yes** — `tests/device-kindle.test.ts`, `device-classify.test.ts`, `device-transfer.test.ts` |
| ebook-convert discovery, AZW3/KEPUB conversion | 80-181, 334-380 | ~14 | **yes** — `tests/export-calibre.test.ts` (which cites `KitCheck/main.swift:349-350` by line) |
| `--json` contract sample, format defaults | 182-224 | 6 | **yes** — `tests/cli.test.ts`, `tests/options.test.ts`, same committed sample file |
| reMarkable endpoint + transfer | 225-333 | 8 | **yes** — `tests/device-remarkable.test.ts` ("mirrors kit-check's StubRemarkable") |
| settings sidecar | 381-503 | ~13 | **yes** — `tests/cli-settings.test.ts`, `conversion-settings.test.ts`; the sidecar name `<Stem>.screepub.json` is identical between `ScriptSettings.swift` and `src/settings/sidecar.ts` |
| device presets, export formats, staleness, `Export.copy` | 517-610 | ~18 | **yes** — `tests/presets.test.ts`, `export-formats.test.ts`, `export-freshness.test.ts`, `replace-file.test.ts` |
| KFX toolchain | 1000-1066 | ~8 | **partly** — `tests/export-kfx.test.ts` covers discovery and status; see item 3 |
| **feedback issue URL** | 504-516 | **5** | **no** |
| **route ordering / the send menu / remembered choice / send verb** | 611-773 | **45** | **no** |
| **updater version comparison** | 774-812 | **17** | **no** |
| **self-update installer (requirement pinning, swap mechanics)** | 813-974 | **26** | **no** |
| **default mail client, Apple Books** | 975-999 | **5** | **no** |
| **engine progress and cancellation** | 1067-1132 | **10** | **partly** — progress is covered by `tests/cli.test.ts`; **cancellation is not, because the Tauri shell has no Cancel** (`desktop/README.md`: "There is no Cancel") |
| **update selection** | 1133-1232 | **17** | **no** |
| **update decoding** | 1233-1296 | **14** | **no** |
| release-notes parsing | 1297-1467 | 21 | **in kind, not ported** — `tools/build-desktop-notes.ts` + `tests/desktop-notes.test.ts` parse the same `docs/releases/<v>.md` into the same block shapes (headings, bullets, prose, caveat, "no text is lost"). Different code, comparable coverage. |
| **update error descriptions** | 1468-1532 | **11** | **no** |

**171 of 264 check sites — 65% — assert behaviour that exists nowhere else
in this repository.** The ADR's scope line for piece A is why, and it is
honest about it: "The updater (`UpdateCheck`, `UpdateInstall`,
`ReleaseNotes`) and the OS-launch shims (`AppleBooks`, `SendToKindle`,
`Feedback`, `InstalledApp`) are **deferred to piece C**, where Tauri's own
plugins may answer them outright." Piece C did not answer them. Piece D did
not either — its "what D explicitly does not do" names auto-update. So the
deferral has been carried, unremarked, from A through E2 and arrives at F as
an undischarged debt. `grep -rln "UpdateCheck\|ReleaseNotes\|Feedback\|AppleBooks\|SendToKindle\|InstalledApp" src/ tests/ desktop/`
returns exactly two files, both about the generated notes module.

The send menu is the largest single block (45 checks) and it is worth naming
what it covers that `desktop/ui/send.js` does not: `ResultActions.swift`
offers six destination kinds — device, reMarkable, Apple Books, Send to
Kindle, email to Kindle, save a copy — with an ordering heuristic, a
remembered choice that outranks it, a catalog that lists routes that are not
currently available, and a per-route send verb. The Tauri surface offers four
device kinds (`KINDS = ['kindle', 'kobo', 'tolino', 'remarkable']`). That is
a narrower feature, honestly scoped for piece D, but it means the assertions
are superseded rather than replaced.

**3. `app/` holds a 485 KB vendored binary that nothing else has a copy of.**
`app/Packages/KFXKit/Sources/KFXKit/Vendor/KFX_Output_plugin.zip`, with its
`PROVENANCE.md` and GPL-3 `COPYING`. `KFXToolchain.installPlugin()`
(`KFXToolchain.swift:157`) installs it into the user's Calibre. The piece A
plan deferred exactly this: "Installing the vendored 485 KB
`KFX_Output_plugin.zip` requires deciding how a `bun build --compile` binary
embeds a binary asset — a packaging decision that belongs with the app
packaging piece." `src/export/kfx.ts` ports *discovery*
(`pluginInstalled()` shells `calibre-customize --list-plugins`) and nothing
else. So after F3, the KFX ladder can still detect the plugin but Screepub
can no longer install it — and `THIRD-PARTY-NOTICES.md` lines 87-96 point at
two files that would no longer exist.

**4. The transition overlay costs more than one deleted file.**
`desktop/src-tauri/tauri.transition.conf.json` is genuinely one key
(`{"productName": "Screepub Desktop"}`) and `tests/desktop-shell.test.ts`
pins it to exactly that. But deleting the file alone breaks the build:
`.github/workflows/release.yml:648` passes
`--config tauri.transition.conf.json` on the macOS legs, and `cargo tauri
build` fails on a missing config path. Deleting it therefore requires, in one
commit: the file, that `release.yml` line, the seven tests in
`tests/desktop-shell.test.ts`'s "the macOS transition overlay" describe
block, and two tests in `tests/release-app-step.test.ts` ("a macOS leg passes
its triple AND the transition overlay", "the Linux and Windows legs pass no
target and no overlay"). **Ten edits, not one.** The E2 spec's "Piece F
deletes that one file" is right about the mechanism and wrong about the cost;
the design intent — that removal is a deliberate act, not a forgotten flag —
holds either way.

**5. The live (non-prose) references into `app/` that a deletion must
resolve.** Prose references in `docs/superpowers/plans/` are historical
records of completed work and should be left alone. These are not:

| reference | what breaks |
| --- | --- |
| `tests/theme-colors.ts:5` reads `app/Sources/ScreepubApp/Theme.swift` | `tests/brand-tokens.test.ts` fails outright. This is the pin that keeps `brand/tokens.json` honest. Deleting `app/` requires promoting `tokens.json` to the source of truth and reversing the test's direction — and `brand/README.md:20` and `brand/tokens.json`'s `$comment` both say Theme.swift is the source, so they change too. |
| `tests/release-artifacts.test.ts` reads `app/release.sh` and asserts the macOS assets | ~8 tests fail |
| `.github/workflows/ci.yml` `app` + `artifact` jobs (`macos-15`, `swift build`, `swift run kit-check`) | must be removed; CI's own header comment describes kit-check's self-skipping |
| `.github/workflows/release.yml` lines 75, 209, 223, 256-258, 274-276, 293-295, 392, 610 | the whole macOS Swift job and the tap crib sheet |
| `tools/bump-tap.sh` hardcodes `Screepub-macOS.dmg` | the cask bump breaks the moment the Swift DMG stops existing; `tools/check-tap.sh` then alarms weekly via `tap-freshness.yml` |
| `CLAUDE.md` lines 15, 41, and the `format-defaults.json` "BOTH suites (options.test.ts, kit-check)" invariant | the triple-pin becomes a single pin; the invariant's wording must change or it becomes false |
| `README.md` 10, 98, 169, 205, 347, 373; `site/index.html` 341, 354, 456, 465 | user-facing download and install instructions |
| `THIRD-PARTY-NOTICES.md` 11, 92, 96; `SECURITY.md` 35; `CONTRIBUTING.md` 31; `.github/dependabot.yml` 56 | dangling paths |
| `tools/build-cli.ts` 8, 108, 240; `tools/sidecar-targets.ts` 13; `tools/build-app-bundle.ts` 14, 84 | comments that explain a coexistence that no longer exists |

**6. The two libraries do not collide, and that is measured rather than
assumed.** `src/library.ts`'s `folderFor()` only claims a path that is free
or already carries its own `source.json`: "A folder with no `source.json`
(one the user made, **or an older layout**) is neither, and is left alone
rather than written into." The Swift app writes flat into
`~/Documents/Screepub/` (`AppSettings.swift:16-20`), so its artifacts are
*files*, and the Tauri app's are *folders*. `Foo.epub` and `Foo/` coexist in
one directory. **No data is overwritten or lost.**

What *is* lost is continuity, and this is the part an "it just coexists"
answer glosses:

- **Tuned settings do not carry over.** Both apps name the sidecar
  identically (`<Stem>.screepub.json`, beside the `.fountain`), but the Swift
  one is at `~/Documents/Screepub/Foo.screepub.json` and the Tauri one looks
  at `~/Documents/Screepub/Foo/Foo.screepub.json`. A user who tuned a script
  in the Swift app and reconverts it in the Tauri app gets **defaults**, with
  no message saying so. `src/cli.ts` reads the script's own sidecar before
  rendering (`desktop/README.md`, task 10b) — but "the script's own" is the
  one in the folder it did not find.
- **The `.fountain` cache is not reused**, for the same reason. That is a
  slow first conversion, not a wrong one.

F therefore owns a **one-shot migration**, specified here and implemented in
`src/library.ts` where the layout knowledge already lives:

> On library resolution, if `<root>/<Stem>.fountain` exists as a flat file
> and `<root>/<Stem>/` does not, move `<Stem>.*` into `<Stem>/` and write
> `source.json` recording the original input path if the flat layout knows
> it, or omitting the marker if not (an unmarked folder is left alone by
> `folderFor()`, which is the safe direction). Report what moved on stderr.
> Never delete; a failure leaves the flat files exactly where they were.

This must be implemented and tested against `SCREEPUB_LIBRARY` **before F2**,
because F2 is when Mac users start using the Tauri app in earnest. Doing it
at F3 is too late.

**7. What `brew` does when a cask disappears.** `<owner>/homebrew-tap`
carries `Casks/screepub.rb` (the Swift app, keyed to `Screepub-macOS.dmg`)
*and* `Formula/screepub.rb` (the macOS CLI, two per-arch tarballs) —
`tools/bump-tap.sh` edits both and `tools/check-tap.sh` verifies both. So:

- Deleting the whole tap is never correct; the CLI formula is not retiring.
- Deleting only the cask leaves an installed app in place and working —
  Homebrew does not uninstall on removal — but `brew upgrade` and `brew
  outdated` start reporting the cask as no longer available in any tap, which
  reads to a user as "abandoned", not "replaced".
- **The correct move is to repoint the cask, not remove it**, so `brew
  upgrade` carries the user across to the Tauri app. That needs the arch
  question in F2 answered (two urls, or a renamed universal artifact), and it
  needs the `uninstall`/`zap` stanzas reviewed, because the two apps have
  different bundle identifiers and different `.app` names and Homebrew tracks
  the old ones.
- `tap-freshness.yml` must keep passing throughout. It runs weekly against
  the *published* tap and it exists precisely because this machinery failed
  silently for five releases. Any F change that leaves it red for a week is
  the same failure again.

**8. Engine suite baseline, this worktree, this commit.** `bun test` →
**1425 pass / 3 skip / 0 fail**, 4855 `expect()` calls, 1428 tests across 60
files, 38.14 s. `tests/fixtures/` holds its five committed files. Nothing
under `app/` was modified.

## Testing

F is mostly deletion, so its tests are mostly *guards against deleting the
wrong thing*.

- **A coverage-parity test, written before any deletion.** For each of the
  unreplaced kit-check sections in item 2, either a `bun test` counterpart
  exists, or the section is named in a committed `docs/retired-coverage.md`
  with the reason. The test reads that file and fails if a section is neither
  covered nor listed. This is the mechanism that makes gate 2 checkable
  rather than remembered.
- **The reference sweep is a test, not a checklist.** A `bun test` case greps
  the tree for `app/`, `ScreepubKit`, `kit-check` and `Theme.swift` outside
  `docs/superpowers/plans/` and fails on any hit. It goes in at F1, passing
  trivially (it lists today's hits as known), and its allowlist shrinks to
  empty at F3.
- **The library migration gets ordinary tests** under `SCREEPUB_LIBRARY`: a
  flat library migrates, a migrated library is idempotent, a library holding
  both layouts migrates only the flat half, an unreadable file aborts without
  losing anything, and a folder without `source.json` is never written into.
- **`epubcheck` and the fixture sweep are not affected** and are not re-run
  as part of F — no parser, CSS or renderer code is touched. If a diff in F
  reaches `src/epub/`, `src/mobi/` or `src/parser/`, the change is out of
  scope.
- **The corpus diff is not the right tool here** and is deliberately not
  used: F changes no conversion behaviour, so the `.fountain` output is
  identical by construction and a diff would prove only that.
- Throughout: `bun test` at 1425/3/0 or better, `bunx tsc --noEmit` clean,
  `tests/fixtures/` at five files.

## Acceptance criteria

**F1 (now):**

1. `app/README-FROZEN.md` exists and names the ADR.
2. The reference-sweep test and the coverage-parity test exist and pass.
3. `docs/retired-coverage.md` lists every unreplaced kit-check section from
   item 2, each marked `port` or `accept-loss`.
4. CI still builds and runs `kit-check`. Nothing under `app/` is modified.
5. Suite green at 1425/3/0; `bunx tsc --noEmit` clean.

**F2 (gates 1 and 3):**

6. A recorded `desktop.yml` run on `macos-15`, green, with bundle and smoke
   steps, plus a named human who launched the DMG's app on a real Mac and
   converted a script.
7. The library migration ships, with the tests above, and has been run once
   against a real flat library.
8. The tap's cask points at the Tauri artifact, `brew upgrade` has been run
   by a human and produced a working app, and `tools/check-tap.sh` exits 0.
9. The final Swift release is tagged, its notes say it is the last, and the
   Swift updater's behaviour against a Tauri-only release page has been
   observed on a Mac (expected: a refusal, because the bundle identifiers
   differ).
10. README and `site/` point at the new artifacts.

**F3 (all gates, one release cycle after F2):**

11. Every item in item 5's table is resolved; the reference sweep's allowlist
    is empty.
12. `brand/tokens.json` is the source of truth for the brand tokens, and
    `tests/brand-tokens.test.ts` passes without reading any Swift.
13. The KFX plugin question is answered: either `installPlugin` is ported and
    the zip moves into the repository proper, or `docs/retired-coverage.md`
    records that Screepub no longer installs it and the user-facing text says
    to install it through Calibre. `THIRD-PARTY-NOTICES.md` is correct either
    way.
14. `CLAUDE.md`'s `format-defaults.json` invariant is rewritten to name the
    pins that still exist.
15. `git rm -r app/` is one reviewable commit; the suite is green; no
    workflow references a macOS Swift job.

## Risks

- **The biggest risk is that F is executed as written in the ADR.** "Retire
  SwiftUI" is one line in a table, in a document whose other lines were
  single implementable pieces. Read literally and done in one session, it
  deletes a shipping product, 171 assertions, a vendored binary, and the only
  brand-token source, and it does so on a platform nobody in the loop can
  test. This spec exists mostly to make that outcome require an explicit
  override.
- **Gate 1b needs a Mac that "is elsewhere"** (`desktop.yml`'s own words).
  The gate cannot be satisfied by more CI, more reading of tauri-bundler, or
  more fakes. If the Mac stays unavailable, the correct outcome is F1
  indefinitely — two apps — not F3 on faith.
- **The updater hazard is the one that can hurt an existing user.** Every
  other consequence of F is "something is missing"; this one is "something
  ran". It is gated, but gating it depends on a Mac to observe it on, which
  is the same scarce resource as gate 1b.
- **The tap has failed silently before**, for five releases, and the whole of
  `tools/check-tap.sh` and `tap-freshness.yml` exists because of it. F
  touches exactly that machinery. Every tap edit in F2 should be verified by
  `tools/check-tap.sh` against the published tap, never against a local
  checkout — that distinction is the original bug.
- **The feature gap may turn out to be a bigger piece than F.** Apple Books,
  Send to Kindle, email-to-Kindle, save-a-copy, Cancel, and an updater are
  six things. If gate 2 is answered with `port` rather than `accept-loss`,
  that is a piece of its own and should be numbered as one rather than
  smuggled into a deletion.
- **`docs/superpowers/plans/` will be full of instructions that no longer
  apply** after F3 — four plan documents edit `app/Sources/KitCheck/main.swift`
  by line number. They are historical records and should stay; the reference
  sweep must exclude that directory or it will fight them forever.
