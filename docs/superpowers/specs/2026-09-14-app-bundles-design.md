# Design: app bundles and installers (piece E2)

Date: 2026-09-14 · Status: proposed
Program: [ADR 2026-09-12 — cross-platform rewrite](../../adr/2026-09-12-cross-platform-tauri.md)
Follows: [piece C — the Tauri shell](2026-09-13-tauri-shell-design.md),
[piece D — the Tauri interface](2026-09-13-tauri-ui-design.md),
[piece E1 — cross-platform CLI release](2026-09-13-cross-platform-cli-release-design.md)
Target version: 0.6.0

## Goal

Turn the finished window into something a person can install. Pieces C and D
produced an application that builds and runs from a git checkout; nobody
outside this repository can get it. E2 produces **installers**, on all three
platforms, published on the release page beside the CLI artifacts E1 already
ships.

The ADR's line for this piece reads "App bundles, per-OS signing,
tap/winget/AUR distribution." The first two are E2. The third is not — see
[The scope line](#the-scope-line).

## What was measured

Everything below was run on this machine — aarch64-unknown-linux-gnu, rustc
1.98.1, Tauri CLI 2.11.4 (`cargo install tauri-cli --locked`), tauri 2.11.5 /
tauri-bundler 2.6.1 — on 2026-09-14, against the shell exactly as piece D left
it. Where a fact could not be measured here it says so.

**1. A `.deb` builds, needs no external tooling, and takes under a minute.**

    $ cargo tauri build --bundles deb
    Finished `release` profile [optimized] target(s) in 47.38s
    Bundling Screepub_0.6.0_arm64.deb
    real 0m49.901s

43 MB. `dpkg-deb` is **not installed on this machine** and was not needed:
`tauri-bundler`'s `linux/debian.rs` writes the `ar` container and both
`tar.gz` members itself, in Rust. `rpm` is the same story — pure Rust, no
`rpmbuild` — and produced `Screepub-0.6.0-1.aarch64.rpm`, also 43 MB.

**2. The sidecar and the bundled fonts both survive bundling.** Unpacked, the
`.deb` holds exactly this:

    usr/bin/screepub-desktop                                  15,482,648
    usr/bin/screepub-engine                                  102,153,058
    usr/share/icons/hicolor/512x512/apps/screepub-desktop.png     17,419
    usr/share/applications/Screepub.desktop                          231

The engine arrives **with the target triple stripped off**, beside the app
binary — the same arrangement the running app already expects (desktop/README,
"How Tauri finds the engine"). Run straight out of the archive it answers:

    $ ./usr/bin/screepub-engine --version --json
    {"ok":true,"version":"0.5.4"}

All six `desktop/ui/fonts/*.woff2` filenames appear inside
`usr/bin/screepub-desktop`: `tauri-build` embeds the frontend at compile time,
so the fonts ride in the executable and need no bundling rule of their own.

**3. That `0.5.4` is the version split, caught in the act.** The bundle is
named `0.6.0` (from `tauri.conf.json`), the crate is `0.6.0` (Cargo.toml), and
the engine inside it calls itself `0.5.4` (package.json). The window prints
that string under the title. A release built today would ship a 0.6.0
installer whose About line says 0.5.4. See
[The version pin](#the-version-pin).

**4. AppImage is broken for this app, and not because `patchelf` is missing.**
`patchelf` is indeed absent here and installing it needs sudo, which this
session does not have — but that turned out not to be the wall. `cargo tauri
build --bundles appimage` got further than expected: it downloaded `AppRun`,
`linuxdeploy-aarch64.AppImage` and the gtk/gstreamer plugins (linuxdeploy
carries its own `patchelf`), deployed 200-odd shared libraries, and then died:

    [gtk/stderr] terminate called after throwing an instance of 'std::runtime_error'
    [gtk/stderr]   what(): Failed to run ldd: exited with code 1
    ...
    [gtk/stdout] Deploying dependencies for ELF file .../Screepub.AppDir/usr/bin/screepub-engine
    ERROR: Failed to run plugin: gtk (exit code: 134)

The cause is in the AppDir. `readelf -d` on the original engine puts its
dynamic section at offset `0x5e1c000` — past the 102 MB Bun payload appended
to the ELF. The AppDir copy has been rewritten by linuxdeploy's `patchelf` to
add `RUNPATH $ORIGIN/../lib`, moving the dynamic section to `0x2a8`. The
result:

    $ ./Screepub.AppDir/usr/bin/screepub-engine --version --json
    Segmentation fault (core dumped)

So **AppImage packaging corrupts the engine**. `ldd` then fails on the
corpse and linuxdeploy aborts, which is why the failure surfaces as an
unrelated-looking gtk plugin error. Installing `patchelf` would not fix this;
it *is* patchelf. Tauri exposes no way to exclude an `externalBin` from
linuxdeploy's ELF pass. This is a property of shipping a Bun-compiled
single-file binary as a sidecar, which the 2026-07-22 ADR accepted knowingly,
so it is a cost of that decision and not a bug to chase here.

**5. macOS signing and notarization are already built into the bundler, and
want the secrets this repo already has.** `tauri-bundler`'s
`macos/sign.rs` reads `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` (a
base64 `.p12` and its password), `APPLE_SIGNING_IDENTITY`, and for
notarization either `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` or
`APPLE_API_KEY`/`APPLE_API_ISSUER`/`APPLE_API_KEY_PATH`. `macos/app.rs` signs
**inside out** — frameworks and sidecar binaries first, then the `.app` — and
the DMG is signed afterwards, which is the same order `app/release.sh` codes
by hand. Every one of those inputs maps onto a secret
`docs/release-secrets.md` already documents. **No new secret is needed.**

**6. `externalBin` is resolved as `{name}-{target_triple}`, verbatim**
(`tauri-utils-2.9.3/src/resources.rs:58`), with no special case for
`universal-apple-darwin`. A universal macOS bundle would therefore need a
third sidecar file, `screepub-engine-universal-apple-darwin`, lipo'd from the
two arch builds, *in addition* to the two per-arch files the two cargo passes
each demand at build time. `tools/build-sidecar.ts` has no such target and
`lipo` exists only on a Mac. See [macOS](#macos).

**7. Windows installer tooling is downloaded, hash-pinned, at bundle time.**
`windows/nsis/mod.rs` fetches `nsis-3.zip` and `nsis_tauri_utils.dll` from
`tauri-apps/binary-releases` and verifies both against SHA-1 constants
compiled into the bundler. The default `webviewInstallMode` is
`DownloadBootstrapper { silent: true }` — the installer downloads WebView2 if
the machine has none. (By contrast the AppImage downloads above are **not**
hash-verified, which is one more reason not to depend on that path.)

**8. `cargo tauri bundle` does not build; `cargo tauri build` does.**

    $ cargo tauri bundle --bundles deb
    Error failed to bundle project: can't open main binary .../target/release/screepub-desktop

Worth knowing because the default `bundle.targets` on Linux is *all three* —
so a bare `cargo tauri build` on Linux attempts the AppImage and fails the
whole run. The bundle list must be pinned explicitly.

**10. `cargo tauri build` rewrites `Cargo.toml`.** Every run here left the
manifest dirty:

    -tauri-build = "2"
    +tauri-build = { version = "2", features = [] }
    -tauri = "2"
    +tauri = { version = "2", features = [] }

The CLI manages the crates' feature lists from `tauri.conf.json` and writes
them back. Harmless, but it means a bundle build is not read-only: a CI job
that runs `git diff --exit-code` after bundling will go red, and a developer
who bundles locally gets an unexplained diff. Either commit the expanded
spelling once so the rewrite is a no-op, or run the diff checks before the
bundle step. `desktop.yml`'s existing "Generated window files are current"
check already runs first, so the ordering is right by accident — keep it that
way on purpose.

**9. `.github/workflows/desktop.yml` has never run.** `gh run list --workflow
desktop.yml` returns `404: workflow desktop.yml not found on the default
branch`; the `tauri-ui` branch is not pushed. The claim that "the desktop
shell compiles on all three platforms" is, today, untested — and reading
`tauri-build-2.6.3/src/lib.rs:668-675`, the Windows job will fail on first
run: absent an `.ico`, the build script errors with ``` `icons/icon.ico` not
found; required for generating a Windows Resource file during tauri-build ```.
`desktop/src-tauri/icons/` holds only `icon.png`. **E2 must add the icon set
before it can claim a Windows build.**

**Not measured, and no way to measure here:** anything that runs on macOS or
Windows. No DMG, no `.app`, no NSIS installer has been produced or opened by
this project. The Windows and macOS halves of this design are read off the
bundler's source and off `app/release.sh`'s decade of scar tissue, and they
ride on CI.

## The scope line

**E2 ships installable bundles and the per-OS signing they need. Distribution
channels — winget, AUR, and a Homebrew cask for the Tauri app — are deferred
to a later piece.**

Three reasons, in order of weight:

1. **Nothing here has ever been installed by a human.** Point 9 above is the
   whole argument: the Windows compile has not happened once. A channel is a
   promise that an artifact works; automating the promise before anyone has
   opened the artifact is how the tap came to serve five broken releases (see
   the `tap-check` job's comment in `release.yml`).
2. **Each channel needs a credential or an account this project does not
   have.** AUR needs an Arch Linux account and an SSH key registered with it;
   winget needs a pull request into `microsoft/winget-pkgs` and a manifest
   pointing at a stable installer URL and hash. Neither is engineering, and
   neither can be exercised from this session.
3. **The one channel that already exists must not be touched.**
   `<owner>/homebrew-tap` serves the *SwiftUI* app's cask and the macOS
   CLI formula, `tools/bump-tap.sh` hardcodes their filenames, and
   `tap-freshness.yml` alarms if they drift. E2 changes none of them. The
   Tauri app gets a cask when it becomes *the* Mac app, which is piece F's
   business, not E2's.

What E2 does instead is make those channels **possible**: stable artifact
filenames that carry the version, a `SHA256SUMS` file covering them, and a
build tool that can be run by hand at any tag. A winget manifest or a PKGBUILD
is then a dozen lines against facts that already exist.

## Artifacts

On a `v*` tag, in addition to everything E1 and `app/release.sh` already
publish:

| Platform | Artifact | Built on | Signed |
| --- | --- | --- | --- |
| Linux x86-64 | `Screepub_<version>_amd64.deb` | `ubuntu-latest` | no |
| Linux x86-64 | `Screepub-<version>-1.x86_64.rpm` | `ubuntu-latest` | no |
| Linux arm64 | `Screepub_<version>_arm64.deb` | `ubuntu-24.04-arm`, if available | no |
| macOS arm64 | `Screepub-Desktop-macOS-arm64.dmg` | `macos-15` | **yes** — Developer ID + notarized |
| macOS x86-64 | `Screepub-Desktop-macOS-x64.dmg` | `macos-15` | **yes** — Developer ID + notarized |
| Windows x86-64 | `Screepub-<version>-setup.exe` (NSIS) | `windows-latest` | **no** |
| all | `SHA256SUMS-app` | `ubuntu-latest` | n/a |

Deliberately absent: **AppImage** (measurement 4 — it corrupts the engine),
**MSI** (NSIS installs per-user without administrator rights; MSI does not,
and shipping two Windows installers doubles the untested surface), **a
universal macOS DMG** (measurement 6), and **Linux arm64 `.rpm`** (nobody has
asked and every extra row is an untested row).

The `.rpm` is included because it cost one flag and two minutes to prove, and
because "Linux" that means only Debian is not really Linux. It is built and
checksummed; it is not installed anywhere, and the notes say so.

## Linux

`.deb` and `.rpm`, both produced by `cargo tauri build --bundles deb,rpm`.
Install with `sudo apt install ./Screepub_0.6.0_amd64.deb` or `sudo dnf
install ./Screepub-0.6.0-1.x86_64.rpm`. No Gatekeeper, no SmartScreen, no
first-run dialog: on Linux an unsigned package is the normal case.

**Linux arm64 is the target the maintainer actually uses daily** and the only
one this session could build. GitHub's free `ubuntu-24.04-arm` runner should
build it; if the repo cannot use one, the `.deb` is produced by hand with the
same command (50 seconds, measured) and attached to the release, and the notes
say which artifacts were machine-built. Do not silently drop it — E1 already
set the precedent of shipping arm64 and being honest about what CI touched.

Four things the stock bundle gets wrong and E2 fixes in `tauri.conf.json`:

- **`Maintainer: <org>`.** Derived from the second segment of the bundle
  identifier because nothing better was configured. Set `bundle.publisher` to
  `<publisher>`.
- **`Description:` is the crate's developer-facing line** — "Screepub desktop
  shell. A window around the engine; no logic lives here." — and the long
  description is literally `(none)`. Both reach `apt show`. Set
  `bundle.shortDescription` / `bundle.longDescription` to the words a user
  should read, and the same text lands in `Comment=` in the `.desktop` file.
- **No licence travels with the binary.** `app/build-app.sh` puts `LICENSE`
  and `THIRD-PARTY-NOTICES.md` inside `Screepub.app` precisely because the
  AGPL requires it and the compiled sidecar embeds Apache-2.0 and MIT
  libraries. The `.deb` carries neither. Add them through `bundle.resources`
  (Linux puts resources under `/usr/lib/<product>/`); this is a licence
  obligation, not a nicety.
- **`Categories=` is empty and there is no `MimeType=`.** The Swift app
  declares `com.adobe.pdf` in `CFBundleDocumentTypes` so "Open With" offers
  it; the Linux bundle should declare `MimeType=application/pdf;` and
  `Categories=Office;Publishing;` for the same reason. The icon ships at
  512×512 only — generating the standard set (32/128/256/512) via `tauri
  icon` gives launchers something to scale from.

`Depends: libwebkit2gtk-4.1-0, libgtk-3-0` is already correct in the generated
control file; leave it alone.

## macOS

**Two per-arch DMGs, signed and notarized by Tauri's own bundler**, using the
secrets already in the repo. The macOS release job gains an
environment-variable translation and nothing else:

| existing secret | Tauri env var |
| --- | --- |
| `DEVELOPER_ID_CERT_P12_BASE64` | `APPLE_CERTIFICATE` |
| `CERT_PASSWORD` | `APPLE_CERTIFICATE_PASSWORD` |
| (the identity `release.yml` already discovers) | `APPLE_SIGNING_IDENTITY` |
| `AC_API_KEY_ID` | `APPLE_API_KEY` |
| `AC_API_ISSUER_ID` | `APPLE_API_ISSUER` |
| `AC_API_KEY_P8_BASE64`, decoded to a file | `APPLE_API_KEY_PATH` |

`app/release.sh` is **not** reused and **not** modified. It is 121 lines of
`xcrun`/`hdiutil`/`lipo` written around the hand-assembled Swift bundle; the
ADR retires it with that bundle at piece F. Running both paths in one job is
fine — they touch different directories and the same keychain — and the Swift
path keeps producing the DMG users actually download.

Per-arch rather than universal because of measurement 6: universal needs a
third, lipo'd sidecar that `tools/build-sidecar.ts` cannot make and this
machine cannot verify. Two DMGs on a page where the *supported* Mac download
is still the Swift one is an acceptable cost; piece F can revisit it with a
Mac in the room.

**Coexistence with the SwiftUI app.** The identifiers already differ —
`com.darkwell.screepub.desktop` vs `com.darkwell.screepub`, pinned by
`tests/desktop-shell.test.ts` — so LaunchServices treats them as two apps. The
one real collision is the filename: both bundlers want `/Applications/
Screepub.app`. E2 resolves it with a transition-only config overlay,
`desktop/src-tauri/tauri.transition.conf.json`, passed as `cargo tauri build
--config`, that overrides `productName` to **`Screepub Desktop`** for the
macOS job only. The window title is set separately (`app.windows[0].title`)
and stays `Screepub`; the Linux package name is unaffected because the Linux
job does not pass the overlay. **Piece F deletes that one file**, and the name
becomes `Screepub.app`. That is the whole transition mechanism, and it is
deliberately a file that has to be removed rather than a flag that can be
forgotten.

The library folder is *deliberately* shared: `src/library.ts` already says so
in a comment — both apps write `~/Documents/Screepub/`, so a user running both
sees one library. Nothing to fix.

## Windows

**One NSIS installer, unsigned, and the documentation says so in plain words.**

This is an instruction, not an oversight: Authenticode certificates cost money
this project does not spend, and the ADR already records Windows signing as "a
procurement problem, not an engineering one." The consequence is concrete and
must be written down before a user meets it:

> **Windows will warn you.** Screepub for Windows is unsigned — it carries no
> code-signing certificate — so the first time you run the installer,
> SmartScreen shows a blue "Windows protected your PC" screen naming an
> unknown publisher. Choose **More info**, then **Run anyway**. Windows
> Defender may also hold the download briefly. Certificates cost money this
> project does not spend yet; this note exists so the warning is expected
> rather than alarming.

That paragraph belongs in three places: `README.md` (beside the identical
paragraph E1 already wrote for the CLI), the release notes for 0.6.0, and the
download page in `site/`. E1's wording is the template; keep them consistent.

Two further Windows facts to state rather than discover:

- **The installer may need the network once.** `webviewInstallMode` defaults
  to downloading the WebView2 bootstrapper when the machine has no WebView2.
  Windows 11 ships it; Windows 10 may not. The *app* still makes no network
  requests — the project's "works fully offline" claim is about the converter
  and remains true — but the sentence in the README should be precise enough
  that nobody catches us out on it. The alternative, an offline installer, adds
  ~130 MB and is not worth it.
- **NSIS installs per-user by default**, into `%LOCALAPPDATA%`, with no
  administrator prompt. That is the right default for a tool a screenwriter
  installs on a work laptop, and it is another reason not to ship an MSI.

## The version pin

Measurement 3 is the strongest argument in this document for a check that does
not exist yet. Three files carry the version and, by design, they are only
brought into agreement at release time:

    package.json        0.5.4    → what `screepub --version` prints
    Cargo.toml          0.6.0    → the crate
    tauri.conf.json     0.6.0    → the bundle filename, the Info.plist,
                                   the deb Version:, the NSIS product version

E2 does **not** make `bun test` demand they agree — the split is deliberate
while a version is in flight, and a test that fails on every working branch is
a test that gets deleted. Instead, `release.yml`'s existing `checks` job, which
already asserts `package.json == the tag`, gains two more assertions for
`Cargo.toml` and `tauri.conf.json`. It fails in twenty seconds, at the top of
the run, before any certificate is imported — which is exactly where
`release.yml`'s own comments say this class of check belongs.

Second line of defence, and the one that catches a stale *artifact* rather than
a stale file: the smoke check below runs the engine out of the built bundle and
asserts its `version` equals the tag. That is the check that would have caught
today's 0.5.4-inside-0.6.0.

## Tooling

Two Bun scripts, following E1's pattern exactly — logic in TypeScript where it
can be unit-tested and hand-run, workflow YAML as a thin caller, because YAML
can only be tested by cutting a release.

**`tools/build-app-bundle.ts`** — takes a version and an output directory.
It builds the host sidecar (`build-sidecar.ts --host`, already written), runs
`cargo tauri build` with the bundle list **pinned per OS** (never the default,
measurement 8), copies what came out to the stable names in the artifact table,
and then **verifies what it produced**: each file exists, clears a size floor,
and carries the right container magic (`!<arch>` for a `.deb`, `\xed\xab\xee\xdb`
for an `.rpm`, `MZ` for the NSIS `.exe`, a UDIF trailer for the DMG). It writes
`SHA256SUMS-app`. Runnable by hand — `bun tools/build-app-bundle.ts --version
0.6.0 --out dist/` — because a release tool nobody can run locally is a release
tool nobody can debug, and because that is how the Linux arm64 `.deb` gets made
if no arm runner exists.

**`tools/smoke-bundle.ts`** — the cheapest check that catches a broken bundle
before a user does, and the one this session accidentally ran by hand. It
**opens the bundle without installing it** and runs the engine from inside:

- `.deb` — `ar x`, `tar xzf data.tar.gz`, run `usr/bin/screepub-engine
  --version --json`;
- `.rpm` — `rpm2cpio | cpio -id`, same;
- `.dmg` — `hdiutil attach`, run `Screepub Desktop.app/Contents/MacOS/
  screepub-engine`, `hdiutil detach`;
- NSIS `.exe` — `7z x` (present on GitHub's Windows image), run
  `screepub-engine.exe`.

It asserts three things: exactly one JSON object on stdout, `ok: true`, and
`version` equal to the tag. Then it converts `tests/fixtures/screenplay.pdf`
through that same extracted engine and asserts `ok: true`, which is E1's
`smoke-cli.ts` assertion applied to a bundled engine instead of a downloaded
one — and `smoke-cli.ts`'s `soleJson` helper should be imported rather than
re-written.

This is deliberately **not** "launch the app and look at it". No CI runner has
a display, the GUI half of the bundle cannot be exercised anywhere, and the
part that can silently break in bundling is the sidecar — which is precisely
what AppImage broke today, and precisely what this catches.

**`desktop/src-tauri/icons/`** gains the full set generated by `tauri icon`
from `assets/icon.svg` — `icon.ico` included, without which Windows cannot
compile at all (measurement 9). The icons are committed, as `icon.png` already
is, so `cargo build` needs no extra tool.

## CI

**`desktop.yml` is extended, not replaced.** It already compiles on all three
platforms with the right caches and the right path filters; E2 adds one step
to each matrix leg: after `cargo build --locked`, build the OS's bundle and
smoke it. That turns a compile check into an artifact check on every push,
which is where a broken bundle should be caught — not at a tag. macOS signing
is **not** done there: the push path builds an ad-hoc-signed bundle, because
importing a Developer ID certificate on every push is both slow and a
needlessly wide exposure of the secret.

**`release.yml` gains a three-OS `app-bundles` job**, `needs: checks`, mirroring
the shape of the existing `cross-cli` → `smoke-*` → `cross-upload` chain: each
OS builds and smokes its own bundle, uploads it as a workflow artifact, and a
final job re-verifies `SHA256SUMS-app` after the artifact round trip (the same
blind spot `cross-upload`'s comment already names) and attaches everything to
the release the macOS job published. Uploading in a separate job, after the
release is out of draft, is the existing rule and holds here for the same
reason: a failure in the new path must not strand the release in draft.

**Tag and version are shared with E1; the workflow is shared; the release is
shared.** One `v0.6.0` tag produces the CLI artifacts, the Swift DMG and the
app bundles, all on one release page, all describing one version. Nothing
drifts because nothing is separate. The alternative — a second tag namespace
for the app — was considered and rejected: two tags means two release pages,
two sets of notes, and a user asking which `screepub` they have.

## What E2 explicitly does not do

Auto-update for the Tauri app (Tauri's updater wants a signing key pair and an
update manifest; the Swift app's `UpdateCheck` is retired at F, and what
replaces it is F's question). Windows code signing. winget, AUR, or a Homebrew
cask for the Tauri app. Retiring the SwiftUI app, its DMG, its cask, or
`app/release.sh`. Flathub or Snap. A universal macOS DMG. AppImage.

## Testing

- **`tools/build-app-bundle.ts` and `tools/smoke-bundle.ts` are unit-tested in
  `bun test`** the way `build-cli.ts` and `smoke-cli.ts` are: the argv they
  construct, the artifact-name table, the container-magic check, and the
  version assertion, all against injected fakes. No test spawns `cargo`.
- **The Linux half is verified for real on this machine**, because it can be:
  `.deb` and `.rpm` built, unpacked, and the engine inside them run. That is
  the whole of measurement 1 and 2 and it should be re-run, not re-asserted,
  whenever the bundle config changes.
- **The macOS and Windows halves ride on CI, and the spec says so rather than
  implying otherwise.** `desktop.yml`'s per-push bundle+smoke is the first
  time either will have existed. The Windows leg is expected to fail on its
  very first run until the `.ico` lands — that is a known, named starting
  state, not a surprise.
- **Nothing under `app/` is modified**, and the existing suite stays green:
  1222 pass / 3 skip / 0 fail, `bunx tsc --noEmit` clean, `tests/fixtures/` at
  its five committed files.

## Acceptance criteria

1. `bun tools/build-app-bundle.ts --version <v> --out <dir>` produces this
   machine's `.deb` and `.rpm`, plus `SHA256SUMS-app`, and fails loudly if any
   artifact is missing, undersized, or the wrong container format.
2. `bun tools/smoke-bundle.ts` opens a built bundle, runs the engine from
   inside it, and asserts the engine's `--version --json` equals the version
   being built — so today's 0.5.4-inside-0.6.0 cannot ship.
3. `desktop/src-tauri/icons/` contains `icon.ico`, and the Windows leg of
   `desktop.yml` compiles.
4. The `.deb` and `.rpm` carry `LICENSE` and `THIRD-PARTY-NOTICES.md`, a
   human-readable `Description`, a real `Maintainer`, and a `.desktop` entry
   with `MimeType=application/pdf`.
5. The macOS DMG is Developer-ID-signed and notarized using only the secrets
   `docs/release-secrets.md` already documents, and installs as `Screepub
   Desktop.app` — so a machine with the SwiftUI app keeps both.
6. `release.yml`'s `checks` job fails the release when `package.json`,
   `Cargo.toml` and `tauri.conf.json` do not all equal the tag.
7. The existing macOS DMG, the CLI artifacts, `app/release.sh`, the Homebrew
   tap and its freshness alarm are untouched and still produced by the same
   code as before.
8. README, `site/`, and the 0.6.0 release notes state the unsigned-Windows
   warning, the unproven-on-hardware device limits E1 already names, and which
   artifacts CI actually executed.
9. Nothing under `app/` is modified; the engine suite stays at 1222 pass / 3
   skip / 0 fail; `bunx tsc --noEmit` clean.

## Risks

- **Two of three platforms cannot be exercised here at all.** The Windows
  installer and the macOS DMG will be produced for the first time by CI and,
  if nobody opens them, the first person to run one is a user. This is the
  same gap E1 accepted and named; E2 narrows it by running the engine out of
  each bundle in CI, which is strictly more than E1 could do for its Windows
  binary, but it does not close it. **No one should call the Windows app
  "working" until a human has run it.**
- **The Windows leg of `desktop.yml` has never run and is expected to fail
  first time.** Better to expect it than to discover it at a tag.
- **AppImage is the format most Linux users expect from a small app**, and it
  is exactly the one the Bun sidecar rules out. Anyone who later asks "why no
  AppImage?" should be pointed at measurement 4, not at a preference. If it is
  ever wanted badly enough, the fix is upstream in the AppImage story or in
  how the engine is shipped, not in this config.
- **Release size.** Each app bundle carries a 102 MB engine. Adding five
  bundles to a release that already grew ~250 MB for E1 takes a `v0.6.0`
  release well past half a gigabyte. That is the standing cost of the sidecar
  approach, recorded in the 2026-07-22 ADR, and it compounds with every
  artifact added — which is a further argument for the short artifact table
  above.
- **Two Mac apps on one release page will confuse somebody.** Mitigated by
  naming (`Screepub-Desktop-macOS-*.dmg` beside `Screepub-macOS.dmg`) and by
  the notes saying which one is supported — but the real fix is piece F, and
  E2 should not linger.
- **`cargo tauri` is a new toolchain dependency for releases.** It pins to
  `tauri-cli ^2`, installs from crates.io, and took roughly four minutes to
  build here. CI should install it with `--locked` and cache it the way
  `desktop.yml` already caches `~/.cargo`, or the release grows a four-minute
  tax per OS.
