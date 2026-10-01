# Plan amendment: the identifier release (F2) and the deletion (F3), re-mapped after 0.7.4

Date: 2026-10-01. Amends
[the handover plan](2026-09-20-swift-to-tauri-handover.md) (its steps 6 to 13,
written as "v0.6.1") and
[the retirement spec](../specs/2026-09-14-retire-swiftui-design.md) (F2, F3).
Decision record: [ADR 2026-09-20](../../adr/2026-09-20-swift-app-migrates-itself.md).

The handover plan was written before the window had an updater (0.7.1), app
settings (piece C), or a second release channel of its own. Its steps are
still right; three things around them changed, and one of them is a whole
population of users the ADR does not mention. This file records what was
measured on 2026-10-01 against main at `7eb40e5` (v0.7.4), what that changes,
and the order of work. Nothing here has been built yet.

## What was measured

1. **v0.7.4 publishes TWO `.dmg` assets**: `Screepub-macOS.dmg` (the Swift
   app, from `app/release.sh`) and `Screepub-Desktop-macOS-universal.dmg`
   (the window). `gh release view v0.7.4` lists both. The frozen Swift
   updater takes the first `.dmg` in GitHub's name-ordered asset list
   (`UpdateCheck.swift`; ordering measured 2026-09-21, see the spec), so F2
   must publish exactly one. ADR consequence 1 already says this; restated
   because both names are still live.
2. **The Swift updater takes the first `.app` inside the mounted image**
   (`UpdateInstall.appInside`: `items.first { $0.pathExtension == "app" }`),
   verifies it against the pinned requirement including the identifier, and
   swaps it into the RUNNING bundle's path. The name of the `.app` inside
   the DMG does not matter to it; the DMG's own `Applications` symlink has
   no extension and is skipped.
3. **The window's own updater never looks at the identifier.**
   `tauri-plugin-updater` 2.12.0, `updater.rs` lines 1294 to 1388 (macOS
   `install_inner`): it unpacks the archive, dropping the top-level
   directory name (`.skip(1)`), renames the running bundle aside and
   renames the new contents into `extract_path`, which is the running
   bundle's path. Read in `~/.cargo/registry/src/.../tauri-plugin-updater-2.12.0`.
4. **The window keeps four values in WebKit `localStorage`**
   (`desktop/ui/update.js`: `STAMP`, `OPT_IN`, `ASKED`, `FOUND`): when it
   last checked, whether the person opted in to automatic checks, whether
   the one-time question was asked, and the version it last found. WebKit
   keys that storage by bundle identifier
   (`~/Library/WebKit/<identifier>/`). Everything else the window and
   engine keep lives in the engine's settings file and the library
   (`~/Library/Application Support/Screepub`, `~/Documents/Screepub`),
   which no identifier touches.
5. **The `release` job is three jobs wearing one name.** It builds and
   notarizes the Swift app, it builds, signs and notarizes the two macOS CLI
   tarballs the Homebrew FORMULA serves (`app/release.sh` lines 45 to 78 and
   112 to 117), and it creates the GitHub Release itself (draft, upload,
   publish). `cross-upload`, `app-upload` and `tap` all `needs: release`.
   `tools/build-cli.ts` says outright that it never makes a macOS artifact.
   So "take `app/release.sh` out of `release.yml`" (handover step 8) is not
   a deletion: the CLI build and the release creation need a new home
   first.
6. **The tap's scripts name `Screepub-macOS.dmg` and nothing else.**
   `tools/bump-tap.sh` (digest, cask stanza), `tools/check-tap.sh`
   (`check_sha "Screepub-macOS.dmg"`), and the crib sheet in `release.yml`.

## The population the ADR does not cover: people already running the window

ADR 2026-09-20 covers Swift installs (`com.darkwell.screepub`) upgrading to
the window. Since 0.7.1 there is a second group: people running the window
itself, installed as `Screepub Desktop.app` with identifier
`com.darkwell.screepub.desktop`, updating through `latest.json`.

At F2, `latest.json` carries them to the new identifier automatically (3).
`latest.json` is keyed by platform, not identifier, so there is no way to
publish an update for one group and not the other; "have them re-download
instead" is not actually available unless the release also stops publishing
`latest.json`, which would strand them on a 404. What they get instead is
four one-time costs, all measured or read rather than guessed:

- **The update question comes back once.** (4) The opt-in answer and the
  "asked" flag are in identifier-keyed storage. Someone who had opted in is
  opted out until they answer again.
- **macOS asks again for removable-volume access** the first time the new
  identity copies to a Kindle. TCC grants are keyed to the signing identity.
  (Swift installs probably keep theirs: same team, same identifier, so the
  same designated requirement. Not verified.)
- **The bundle keeps its file name**, `Screepub Desktop.app`, because the
  updater replaces in place (3). The menu bar says "Screepub"; Finder and
  the Dock say "Screepub Desktop".
- **Anyone with BOTH apps ends up with two copies of the same app**, one
  named `Screepub.app` (the Swift install, swapped by its updater) and one
  named `Screepub Desktop.app`, both `com.darkwell.screepub`. The owner is in
  this group: checked 2026-10-01, `/Applications/Screepub.app` is the Swift
  app at 0.7.3 and `/Applications/Screepub Desktop.app` is the window at
  0.7.2.

Making it fully seamless would mean moving the four update values into the
engine's settings file in a release BEFORE F2 (so they survive), and having
something rename the bundle after the swap. The first is a modest window
change; the second is logic in the shell or a self-rename from the engine,
against `desktop/README.md`'s rule and fragile when `Screepub.app` already
exists. **Recommendation: accept the four costs and name them in the F2
release notes**, with one line telling people who had both apps to delete
`Screepub Desktop`. This is the owner's call (queued below).

## Decisions queued for the owner

1. **Window users: accept the four one-time costs?** (Recommended: yes.)
2. **The single Mac download's name.** Recommended: `Screepub-macOS.dmg`.
   It is the URL the site buttons, the README, the cask and every external
   link point at today; naming the window's image that way means
   they all start serving the window with no edit, and `bump-tap.sh` and
   `check-tap.sh` keep working unchanged (6). The cost: links to
   `Screepub-Desktop-macOS-universal.dmg` 404 from F2 on. The alternative
   keeps the Desktop name and edits every Swift-name reference instead.
   **2b, only if 2 is yes:** the spec settled "retire the cask at F2". With
   the name kept, the cask needs no change to serve the window: `app
   "Screepub.app"` matches once the transition overlay is gone. Keep the
   cask after all, or retire it as decided?
3. **The F3 wait.** The spec says F3 runs "one release cycle after F2". With
   the CLI move below there are naturally two releases after F2 anyway.
   Keep the wait? (Recommended: yes; it is free.)
4. **Version and go-ahead** for each release: asked at the time, in the
   owner's own chat, never relayed.

## Status, 2026-10-01 evening

The owner answered every decision below: accept the four window-user
costs; the image is `Screepub-macOS.dmg`; the cask is RETIRED (his direct
choice, over "keep"); keep the F3 wait. Built, none of it pushed:

- **F2** on `worktree-f2-identifier`: identity, image name, one `.dmg`,
  cask retirement in the tap scripts, the QA section, and (cherry-picked
  from the other session's `f2-docs`) the README, site and doc lines.

**From the whole-branch review (2026-10-01):**

- Fixed on F2: the release used to go public in the release job, before
  app-upload attached the window's image, so for the length of the bundle
  legs (estimated 15 to 30 minutes) the site's download buttons 404'd and
  Swift update checks found no image. The release job now leaves a draft;
  a final `publish` job un-drafts after every upload job has had its turn
  (running even after a failed leg), and refuses to publish any image
  list but exactly `Screepub-macOS.dmg` (or none). Run against a fake `gh`
  in five cases and its filter against the real v0.7.4 asset list.
- To do at release time: `tap-freshness.yml` demands the cask's
  `deprecate!` as soon as F2 is on main, and only the F2 tag's `tap` job
  adds it. **Merge F2 immediately before tagging it**, or the Monday check
  goes red in between.

**Release notes for F2** (drafted by the other session, plain reader
words, for `docs/releases/<version>.md`; whoever cuts F2 uses these):

1. There is one Screepub for Mac now: Screepub-macOS.dmg, notarized,
   universal, macOS 13 or later, and it updates itself. The older Mac app
   is retired; with its update check on, it installs the new app in its
   place. Books are untouched.
2. Screepub Desktop users cross over through their own update, with four
   one-time costs: the update question is asked once more; drive
   permission is asked again on the first copy to a reader; Finder and the
   Dock keep the name "Screepub Desktop" (rename it if you like); and if
   you had both apps, delete Screepub Desktop.
3. Cancel during a conversion is gone; most scripts finish in seconds.
4. Per-script settings from the older Mac app reset once. Books are
   untouched.
5. The Homebrew cask is retired: download the DMG instead. The
   command-line converter stays on Homebrew as a formula
   (`brew install --formula ssandweiss/tap/screepub`).
- **The CLI move** on `cli-move`, stacked on F2.
- **F3** not started: it waits for the other session's second batch
  (which edits retired-coverage, app-references and weekly-toolchain) to
  land, to avoid rebasing a 66-file sweep across it.

## Order of work

Four releases from here, each changing one kind of thing, because each is
only provable at a tag.

| Release | Carries | Why separate |
| --- | --- | --- |
| N (next) | the housekeeping batch (other session) | already in flight |
| N+1 | **F2: identity only** | the handover plan's own rule: "Do not bundle other changes into" it |
| N+2 | **the macOS CLI build moves out of `app/release.sh`**; the Swift job leaves `release.yml`; **Intel Macs are dropped** (see below) | changes how the formula's binaries are signed, provable only at a tag, and `app/` is still there to fall back on |
| N+3 | **F3: `git rm -r app/`** and the reference sweep to empty | the last step, one release after F2 at the earliest |

N+2 and N+3 can merge into one release if the owner prefers fewer, at the
cost of losing the fallback for a broken CLI signing path.

## F2 (release N+1): steps, re-mapped onto today's files

Branch `worktree-f2-identifier`, rebased onto main after the batch lands
(the batch's window helper also edits `tauri.conf.json` and
`tests/desktop-shell.test.ts`).

1. **Identifier.** `desktop/src-tauri/tauri.conf.json`:
   `com.darkwell.screepub.desktop` to `com.darkwell.screepub`.
2. **Invert the tripwire.** `tests/desktop-shell.test.ts`'s identifier test
   asserts `.desktop` and NOT the Swift id. Invert it deliberately, comment
   rewritten to say the apps now share an identity on purpose.
3. **Drop the transition overlay.** Delete
   `desktop/src-tauri/tauri.transition.conf.json`; remove `--config
   tauri.transition.conf.json` from `release.yml`'s universal row; update
   the pins (`git grep` for `transition.conf` and `Screepub Desktop` found
   about 20 files on 2026-10-01, most of them tests:
   `build-app-bundle`, `release-app-step`, `verify-signing`,
   `build-update-manifest`, `check-latest`, `update-signature`,
   `smoke-bundle`, `desktop-shell`).
4. **Name the image** per decision 2: `tools/build-app-bundle.ts`'s DMG row
   `releasedName`, and the updater archive's row to match
   (`Screepub-macOS-universal.app.tar.gz`), with the parsers in
   `tools/build-update-manifest.ts` and `tools/check-latest.ts` that match
   archive names by regex.
5. **Publish one `.dmg`.** `release.yml`'s `release` job stops uploading
   `app/dist/Screepub-macOS.dmg` and stops hashing it into the notes; the
   job KEEPS running `app/release.sh` for the CLI tarballs until N+2. The
   install line in the notes and the crib sheet name the window's image.
   `app-upload`'s count stays at one `.dmg`; add a check that the release
   as a whole holds exactly one `.dmg` (the Swift job no longer adds one,
   and nothing should ever again).
6. **Signing check.** `tools/verify-signing.ts --expect handover` already
   exists and asserts the Swift updater would INSTALL the image. Run it on
   the downloaded DMG after the tag, as v0.6.0 ran `--expect coexist`.
7. **Release notes disclosures** (`docs/releases/<N+1>.md`): Cancel during
   conversion is gone; Swift-app tuning is lost once (books untouched);
   the four window-user costs above, plus "if you had both apps, delete
   Screepub Desktop".
8. **`docs/mac-qa.md` §4 and §5** become "one app replaces the other": a
   Swift install updates into the window; a window install updates and
   keeps its name.
9. **The README and the site** change only where they name the Desktop
   image (and not at all for the buttons, if decision 2 is yes). The other
   session owns both; message it with the final names.
10. **The tap** per decision 2b. If the cask is retired: deprecate it in
    `homebrew-tap/` (a separate repo, hidden from `git status`), retire the
    cask half of `tap-freshness.yml` and `check-tap.sh`, drop the cask digest
    from `bump-tap.sh`, and verify against the PUBLISHED tap.
11. **The owner's hands, after the tag**: his Swift install updates itself
    into the window and relaunches; his window install updates and asks
    the update question once. This is handover step 13 and the only test
    that matters.

## Found while building F2 (2026-10-01, later)

Steps 1 to 3 are built (commit "F2: the window takes the Swift app's
identity"). Steps 4 and 5 wait on decisions 2 and 2b, because they decide
what the README, site and verification ledger must say, and those are the
other session's files. Three things the steps above did not know:

- **The tap job would race the image.** `tap` is `needs: release`, and
  `tools/bump-tap.sh` reads `Screepub-macOS.dmg`'s digest off the
  published release. Today the release job uploads that image itself. After
  F2 the image comes from `app-upload`, which runs later, so a kept cask
  needs `tap` to wait on `app-upload` too, or `bump-tap.sh` finds no image
  and the tap goes red. A retired cask makes this moot.
- **Swift users can be told "no update" for a day.** The release job makes
  the release public before `app-upload` attaches the image. A Swift app
  that checks in that gap finds no `.dmg`, throws, and has already stamped
  its last-check time (the release job's own comment describes this), so
  it waits 24 hours. Accepted rather than fixed: making `app-upload`
  publish would break the rule that a failing bundle leg can never strand
  the release in draft. It costs a day, once, for whoever is unlucky.
- **The window claims macOS 10.13 and cannot run below 13.0.** The
  installed window's `LSMinimumSystemVersion` is `10.13` (Tauri's default),
  but its engine's `LC_BUILD_VERSION` says `minos 13.0` (bun's floor;
  measured with `otool -l` on `Screepub Desktop.app`). On macOS 11 or 12
  the window opens and every conversion fails. The fix is
  `bundle.macOS.minimumSystemVersion: "13.0"`; it is independent of F2 and
  goes on its own branch. The release notes' "macOS 14+" line was the
  Swift app's floor and should say 13 once the window is the download.

## The CLI move (release N+2)

The two macOS CLI tarballs need a builder that does not live in `app/`. The
shape that fits the repo: extend `tools/build-cli.ts` with `macos-arm64` and
`macos-x64` targets for packaging and verification (it already detects
Mach-O; since the Intel decision below, `macos-arm64` alone), and do the
signing and notarization in a `release.yml` step on the
macOS runner, the way `app/release.sh` does now (codesign with hardened
runtime and timestamp, `notarytool` on a zip, no staple for a bare
Mach-O). The `release` job then imports the certificate, builds the CLI,
creates the release and uploads; the Swift build, Rosetta and
`app/release.sh` leave the workflow. `tests/release-app-step.test.ts` and
the build-cli tests follow. The formula's URLs and names do not change.

The CLI is signed today with `app/screepub-engine.entitlements` (allow-jit,
allow-unsigned-executable-memory, disable-library-validation), so that file
moves out of `app/` with the build. Observed 2026-10-01: the window's
shipped engine (`Screepub Desktop.app/Contents/MacOS/screepub-engine`) is
signed with the hardened runtime and NO entitlements, and converts fine, so
they may not be needed. Keep them anyway: the move should change where the
CLI is built, not how it is signed.

## Intel Macs: dropped at N+2, after the identifier release

**The decision.** The owner, in his own chat, 2026-10-01: "Ditch old
Intel completely." Asked when, he answered precisely: after the identifier
release, not in it.

**Why not in F2.** The frozen Swift updater has no architecture check.
`UpdateCheck.swift`'s `select()` takes the newest release that is neither
a draft nor a prerelease and the first `.dmg` on it; `UpdateInstall.swift`
checks only that image's code signature against the pinned requirement and
swaps the `.app` inside into the running app's path. An Apple-silicon-only
F2 image would replace a working Intel install with an app macOS refuses
to open. So F2 (`worktree-f2-identifier`) stays universal: it is the last
bridge, for Swift installs on Intel that update while F2 is the newest
release. F2 carries one Intel change of its own, the window's plain-words
update message (below), because F2 is the last build an Intel Mac runs.

**Release order: a REQUIREMENT, not a preference.** Merge F2 to main and
TAG it before `cli-move` is merged. `cli-move` sits on F2, so a main that
already contains it has no F2 left to tag: an "F2" tagged from there would
ship the Apple-silicon-only image, a `latest.json` with no darwin-x86_64,
an arm64-only formula and a DISABLED cask at F2 itself, and the bridge
above would never exist. So: merge F2, tag F2, let it go public, and only
then merge `cli-move` for N+2.

**What N+2 does** (branch `cli-move`, built 2026-10-01, not pushed):

- The Mac image, still `Screepub-macOS.dmg`, is Apple silicon only.
  `tools/build-app-bundle.ts` names `MAC_ARCH` (arm64) and `MAC_TARGET`
  (aarch64-apple-darwin) once and refuses any other Mac arch or triple;
  `release.yml`'s Mac row is `arch: arm64`; the lipo, `--universal` and
  the x86_64 sidecar row are deleted. `minimumSystemVersion` stays 13.0.
- `latest.json` lists `darwin-aarch64` only. `tools/check-latest.ts`
  requires that key and judges `darwin-x86_64` per release: on a release
  that still carries the Intel command-line tarball (F2, whose archive is
  universal) it may point at the same archive, so the weekly check stays
  green against F2's; on a later release that is a problem, because an
  Intel copy would install an Apple silicon build.
- The macOS command-line tarball is `screepub-cli-macos-arm64.tar.gz`
  only. `tools/bump-tap.sh` writes the formula whole: one url, one
  sha256, `depends_on arch: :arm64` (Homebrew's ArchRequirement, fatal),
  passing `brew style` from the published 0.7.4 formula and cask.
  `tools/check-tap.sh` judges each release by whether it carries the
  Intel tarball, so F2's two-arch tap stays green until N+2 is tagged.
  `bump-tap.sh` refuses a release that still carries it, so a hand bump
  of F2 from a later main cannot drop Intel brew users a release early.
- The cask, deprecated at F2, is DISABLED from N+2 (`bump-tap.sh` turns
  `deprecate!` into `disable!`; `check-tap.sh` expects deprecate! while a
  release carries the Intel tarball and disable! after). Deprecated, it
  still installs the last Swift release, whose updater would offer N+2 to
  an Intel Mac that installs the cask after N+2 ships, and time does not
  help that one. Disabled, `brew install --cask screepub` stops with
  "has been disabled because it is now downloaded from the releases page
  and updates itself!". The `disable!` keeps the `deprecate!` line's own
  date (or, with neither, 2026-10-01): Homebrew compares it with the
  user's LOCAL day and only deprecates while it is in the future, so a
  "today in UTC" left the cask installable in the Americas after a
  US-evening tag. `check-tap.sh` also fails a cask that keeps a
  `deprecate!` beside its `disable!`: Homebrew asks deprecated? first, so
  such a cask still installs. Checked against Homebrew's `cask/dsl.rb` and
  `deprecate_disable.rb`; `brew style` passes on the written cask, from
  both the published cask and a deprecated one.

**What an Intel Mac meets after N+2.**

- **The window** (a universal build, F2 or earlier, running its x86_64
  slice): its updater looks for `darwin-x86_64`, finds none, and `check()`
  rejects with TargetsNotFound. The launch check is silent on errors, as
  it always was. From F2 on, the manual "Check for updates" button does
  not show the plugin's text; it says "New versions of Screepub run only
  on Macs with Apple silicon, so this copy will not be offered updates.
  The version you have keeps working." (`desktop/ui/update.js`,
  `INTEL_MAC_RETIRED`, recognised from the plugin's own text, never from
  `navigator.platform`, which says MacIntel on every Mac). It ships IN F2,
  commit "F2: an Intel copy's update check says why, in plain words",
  because F2 is the last build an Intel Mac runs: carried by N+2 it would
  never be seen there. "This copy", because an Apple silicon Mac running
  F2 under Rosetta gets the same answer. A version remembered from an
  earlier check is forgotten, so from the NEXT launch the label stops
  offering an install that cannot happen (this launch's label stays:
  `update-flow.js`'s `checkAnswered` ignores errors). That only matters if
  a release ships between F2 and N+2: with N+2 straight after F2, an F2
  window on Intel never found anything newer to remember. A window older
  than F2 shows the plugin's raw text.
- **Homebrew**: `brew install` and `brew upgrade` stop with "The arm64
  architecture is required for this software." and leave the installed
  copy alone. Every `brew upgrade` repeats it until the person runs
  `brew uninstall screepub`.
- **A Swift install on Intel that has not crossed by the time N+2 is
  public is not covered by the bridge.** Its updater offers the newest
  release, so once N+2 is newest it offers N+2's Apple-silicon-only
  image; installed, that Mac is left with an app that does not open
  (books untouched; F2's image, downloaded by hand from F2's release
  page, still runs there). The window's own updater has no such hole,
  because it asks for its own platform key.
- **What that Swift user sees is the N+2 release notes.** The Swift app's
  release notes sheet (`ReleaseNotesSheet.swift`) shows every newer
  version's notes above INSTALL AND RELAUNCH and NOT NOW, so the notes
  below are the lever, not time alone: they tell an Intel reader not to
  install and where to go instead. The "keeps working; will not be
  offered updates" wording would be false for this reader, who IS offered
  N+2. Caveat, read in the Swift source: two of its three install paths
  do not put the notes on screen. The footer popover ("REV. X IS READY",
  `ContentView.swift`) offers INSTALL AND RELAUNCH beside VIEW RELEASE
  NOTES, and the menu's Check for Updates alert (`ScreepubApp.swift`)
  offers Install and Relaunch, View Release and Later. The notes reach
  whoever opens them before installing.

**Release note for N+2**, plain words, for `docs/releases/<N+2>.md`.
`<F2 version>` is a placeholder: whoever cuts N+2 writes F2's version
number there (and may link its release page).

> Intel Macs are no longer supported. On an Intel Mac, choose Not Now:
> this version will not open there. To move from the old Mac app to the
> new one, download Screepub <F2 version> from its release page instead.

The README, the site and the user docs still describe a universal Mac
download and an Intel command-line tarball; the other session owns that
wording and lands it with N+2.

## F3 (release N+3): the deletion

Acceptance criteria 11 to 15 of the spec, re-checked against today:

- **11, coverage and the sweep.** `docs/retired-coverage.md`'s Built column
  is stale: `send-menu`, `mail-and-books` and `feedback-url` were built by
  parity pieces B and C (2026-09-24) and still read "no". Update it with
  the test that now covers each, then shrink `tools/app-references.json` to
  empty. `tests/frozen-app.test.ts` and `tools/frozen-app-manifest.json`
  (the F1 freeze guard) go with `app/`.
- **12, brand tokens.** Done early (`brand/tokens.json`, 2026-09-23).
- **13, the KFX plugin.** Answered by piece D: `replaced`, the engine
  installs through Calibre's own index. Check `THIRD-PARTY-NOTICES.md` no
  longer describes a vendored zip.
- **14, `CLAUDE.md`'s `format-defaults.json` invariant** loses kit-check
  and names the pins that remain.
- **15, one reviewable commit.** `git rm -r app/`; `ci.yml`'s `app` job
  goes, and so does `weekly-toolchain.yml`'s Swift job; no workflow
  references a Swift job. Suite green. (`weekly-toolchain.yml` was
  Swift-only on 2026-10-01, but the other session's second batch adds a
  weekly Calibre job to it, so F3 removes the Swift job, not the file.)

The F3 branch can be prepared any time and held; it merges only after
N+1 has shipped and N+2 has proven the CLI move.

**The per-file list already exists**: `tools/app-references.json` (66 rows,
425 references on 2026-10-01 after the batch), each with a "breaks" note
saying what it is. F3 drives every row to zero and deletes the file's rows
as it goes. Most are provenance comments (`note`) and doc lines. The ones
that need thought, from those notes:

- `tests/theme-colors.ts` opens `Theme.swift` for its CONTENT, and
  `tests/brand-tokens.test.ts` imports it. The comparison is already
  `skipIf` the Swift file is missing; the parser and helpers it shares must
  stay or move, not vanish.
- `tests/verify-signing.test.ts` pins `tools/verify-signing.ts`'s
  requirement strings clause by clause against `UpdateInstall.swift`. After
  F3 the strings have no upstream: they become this repository's own
  definition, with the Swift quoted in a comment as history. Keep the tool:
  installed Swift apps in the wild still demand exactly those strings.
- `tests/update-compare.test.ts` mirrors kit-check lines 774 to 812 pair
  for pair. The assertions stay; the "mirror" framing becomes a comment.
- `tests/release-artifacts.test.ts` READS `app/release.sh` (about eight
  tests). Most of them change at N+2 with the CLI move, not at F3.
- `tap-freshness.yml` has no textual reference and is invisible to the
  sweep (its row is `tools/check-tap.sh`): re-check it by hand.
- `retired-coverage.md` (55 references) is the gate-2 record. Every `port`
  row is built as of 2026-10-01 (the last, `update-error-descriptions`, was
  re-audited that day). It stays as history with its app/ paths, which means
  the sweep must learn to exclude it, the way it excludes docs/superpowers/;
  or it moves under docs/superpowers/. Owner's choice, small.
- `tests/app-references.test.ts` itself: criterion 11 says the inventory is
  EMPTY when F3 is done. The test then has nothing to pin and should go
  with the directory, or stay as a guard that `app/` never comes back.

### F3 merge notes (2026-10-01, from the review of `f3-delete-app`)

Built and held on `f3-delete-app`. When it merges (after N+1 has shipped
and N+2 has proven the CLI move), two things outside the repository's
tracked files need a person's hands. Neither may be done before F3 ships:
releases N, N+1 and N+2 still have `app/`, and their preflight still
builds it.

**1. Delete the Swift app's leftover build output.** The main checkout and
the `layout-pass` worktree each hold `app/.build/` (407 MB, SwiftPM),
`app/build/` and `app/dist/`. F3's `.gitignore` keeps all three ignored
(an unanchored `.build/` beside `build/` and `dist/`), so nothing fails and
nothing gets staged, but the space stays taken. After merging, run in each
of those two places:

```bash
rm -rf app
```

**2. Edit the `/release` skill** (`.claude/skills/release/SKILL.md` in the
main checkout; gitignored, so no branch can change it). Its preflight runs
the Swift build and kit-check, which fail once `app/` is gone. Four spots,
quoted as they read on 2026-10-01, each with its replacement.

(a) Step 1, the background preflight (lines 65 to 82). Replace

````
Immediately, before any git reading, start all five in the background
(Bash with `run_in_background: true`, one call each):

```bash
bun test
bunx tsc --noEmit
(cd app && swift build -c release)
(cd app && swift run -c release kit-check)
bun tools/capture-screens.ts
```

Then move straight to step 2. **Do not block on these.** Nothing is
written yet, so drafting first is free, and waiting would put minutes of
dead air in front of the only screen the user cares about. You collect the
capture at step 7 and the other four at step 9, before anything is
committed.

If one of the four finishes and fails while you are still drafting, note it
````

with

````
Immediately, before any git reading, start all three in the background
(Bash with `run_in_background: true`, one call each):

```bash
bun test
bunx tsc --noEmit
bun tools/capture-screens.ts
```

Then move straight to step 2. **Do not block on these.** Nothing is
written yet, so drafting first is free, and waiting would put minutes of
dead air in front of the only screen the user cares about. You collect the
capture at step 7 and the other two at step 9, before anything is
committed.

If one of the two finishes and fails while you are still drafting, note it
````

(b) Step 9, item 1 (line 363). Replace

```
1. **Collect the background preflight.** All four must pass. If any
```

with

```
1. **Collect the background preflight.** Both must pass. If either
```

(c) Step 9, item 3 (line 375). The Swift release script stopped reading
`package.json` at the CLI move; `tools/build-cli.ts` and release.yml's
`checks` job are what hold a tag to it now. Replace

```
   `package.json` (what the engine reports; `app/release.sh` reads it),
```

with

```
   `package.json` (what the engine reports; `tools/build-cli.ts` and the
   `checks` job hold the tag to it),
```

(d) The "Preflight failed" example (lines 566 to 569). Replace

```
  bun test              passed
  bunx tsc --noEmit     passed
  swift build -c release  FAILED
  kit-check             not run, the build it needs did not finish
```

with

```
  bun test              passed
  bunx tsc --noEmit     FAILED
```

After the edit, `grep -n -i 'swift\|kit-check\|app/' .claude/skills/release/SKILL.md`
should print nothing.
