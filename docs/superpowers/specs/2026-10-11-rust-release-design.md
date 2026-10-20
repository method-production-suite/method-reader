# Design: releasing the Rust app, and handing the Tauri windows over to it

Date: 2026-10-11. Status: built and tested where no secret is needed;
nothing signed, notarized, published or deployed. Wave 3 of
[the pure-Rust port](../plans/2026-10-11-pure-rust-port.md), branch
`rust/release` cut from `rust-port` at 501d7ec.

Serves: the plan's definition of done ("the release pipeline builds, signs,
notarizes and publishes the Rust app and CLI ... the updater handover is
proved"), inventory rows W6 to W9 and U1 to U10 in `rust/PARITY.md`, and
the decision there to port `services/device-requests`.

## The short version

- **The handover works, and it was run, not reasoned.** The exact updater
  code every Tauri window runs (tauri-plugin-updater 2.12.0 on tauri
  2.11.5) was run headless against the bundle v0.11.1 actually published,
  downloaded from the release, and it verified and installed a Rust
  `Screepub-macOS.app.tar.gz` made by this branch's tools, in place,
  removing the Bun engine; the relaunch target it leaves is the Rust
  window. `release.yml` now does that same check on every release, with the
  v0.11.1 public key and the release's own signed archive, before
  `latest.json` or the release goes public (job `handover-gate`).
- **Nothing a person or a page depends on changes name.** Same bundle
  identifier, same published file names, same `latest.json` endpoint and
  key, same Homebrew tarball, same secrets.
- **What needs the owner:** the secrets already exist; one real Mac run of
  `tools/rehearse-handover.sh run` to watch it with a window and confirm the
  update consent carries over (the WebKit storage path is read, not
  measured, on a real install); two decisions (TLS crypto provider, Windows
  code signing); and the Worker deploy when wanted.

## What was built

| Path | What |
|---|---|
| `rust/crates/sp-update` | The updater transport, rule for rule from the plugin: latest.json, platform keys, minisign, download, the Mac bundle swap, AppImage and Windows hand-offs, the restart, the engine gate, the install-and-restart order, and the update consent read from the Tauri window's WebKit storage (an SQLite reader, WAL included, no SQLite). Tested against a local fake release server with a throwaway key, and real SQLite fixtures. |
| `rust/crates/sp-release` (binary `sp-release`) | The release tools: the Mac bundle layout and Info.plist, the reproducible update archive, the minisign signature in tauri-cli's format, latest.json, a pre-publish verifier, the .deb (by hand), the .rpm (pure-Rust `rpm` crate), the AppDir, the NSIS script, the command-line archives, the version gate, check-latest, a local server. Tested down to the whole update chain through the binary. |
| `rust/crates/sp-app` (small) | `ManifestUpdater`: check() through sp-update, ready only inside a Mac bundle. First-run import of the update consent. Storage follows the running bundle's identifier. |
| `rust/crates/sp-device-requests` | The device-request Worker in Rust (workers-rs, wasm32). All 24 TypeScript checks ported, plus its SQL and the live database's migration run against real SQLite. Not deployed. |
| `rust/handover-proof` | The plugin itself, headless, as the handover gate. Its own workspace and lockfile (copied from `desktop/src-tauri`), so Tauri never enters `rust/Cargo.lock`. Deleted once the Tauri windows in the field are carried over. |
| `tools/rehearse-handover.sh` | prepare / prove / run / verify / clean: a local Tauri 0.11.1 window and a local Rust 0.12.0, a throwaway key, a local `latest.json`. |
| `.github/workflows/release.yml` | Rewritten for cargo. Not run. |
| `.github/workflows/rust.yml`, `tap-freshness.yml`, `.github/actions/import-developer-id` | The push checks (fmt, clippy, tests, the Worker's wasm build, every installer built and opened unsigned, the rehearsal on demand) and the weekly manifest guard. |

`cargo fmt`, `clippy -D warnings` and `cargo test` are clean for every
crate this branch touched. The rehearsal's `prepare` and `prove` were run
end to end on 2026-10-11 on an Apple-silicon Mac.

## (a) The bundles

### Tool choice

Read from their current sources: cargo-packager (crabnebula, the Tauri
authors) makes .app, .dmg, NSIS, WiX, .deb, AppImage and pacman but no
.rpm, and writes its own Info.plist, NSIS template and file names;
cargo-bundle makes .app, .deb, .msi and AppImage with its own templates.
Both would need overriding at exactly the points that matter here, because
**the Rust bundles must look like the Tauri ones to everything that already
knows them**: the Tauri updater's `skip(1)` unpack, Launch Services, the
frozen Swift updater (first `.dmg` on the page, team and identifier
pinned), dpkg and rpm upgrading package `screepub` in place, the NSIS
uninstall key. So the layouts are hand-rolled in `sp-release` (about 600
tested lines) and the platform work stays with the tools CI already uses:
codesign, notarytool, stapler, hdiutil, makensis, appimagetool.

### macOS (Apple silicon only, as since 0.10.0)

`sp-release mac-app` lays out `Screepub.app` the way tauri-bundler 2.9.4
did (`macos/app.rs`): `Contents/Info.plist` with the same keys
(CFBundleIdentifier `com.darkwell.screepub`, CFBundleExecutable
`screepub-desktop`, CFBundleIconFile `icon.icns`, LSMinimumSystemVersion
13.0, LSApplicationCategoryType productivity, NSHighResolutionCapable,
LSRequiresCarbon, CSResourcesFileMapped), the window binary as
`Contents/MacOS/screepub-desktop` (the name tauri-cli gave the window's
binary, since no `mainBinaryName` was set; kept so nothing that names it
breaks), `icon.icns`, `LICENSE` and `THIRD-PARTY-NOTICES.md` in Resources
(row W8). Helpers such as `screepub-mtp` ride in `Contents/MacOS` when MTP
ships.

In release.yml: every executable is signed inside out, then the bundle,
with the hardened runtime and a timestamp and **no entitlements** (the Bun
engine needed JIT entitlements; the Rust app does not JIT). `codesign -d
-r-` is checked to name the identifier. The bundle is notarized (zip to
notarytool), stapled, assessed by `spctl`. The update archive is made from
the **stapled** bundle. The image is `hdiutil create` of the app and an
Applications link, then signed, notarized and stapled itself (the v0.6.0
lesson). One image, `Screepub-macOS.dmg`.

### Windows x64

`sp-release nsis` writes the NSIS script; makensis compiles it to
`Screepub-windows-x64-setup.exe`. It installs where the Tauri installer did
(per user, `%LOCALAPPDATA%\Screepub`), under the same uninstall key
(`HKCU\...\Uninstall\Screepub`), with the same executable name, and deletes
`screepub-engine.exe`, so running it over a Tauri install upgrades in
place. `/S` silent, `/R` relaunch (for a future in-app Windows update).
Unsigned, as the Tauri installer was. **Owner decision:** a code-signing
certificate would let release.yml run signtool on the window and the
installer and end the SmartScreen warning.

### Linux x64

The same files the Tauri .deb and .rpm installed, under the same package
name `screepub`, so apt and dnf upgrade in place and the engine sidecar
leaves with the old file list: `/usr/bin/screepub-desktop`,
`/usr/share/applications/Screepub.desktop` (Categories=Office, row W9),
hicolor icons, `/usr/lib/Screepub/LICENSE` and the notices. The .deb's
`Depends:` is measured on the built binary by `dpkg-shlibdeps` in CI, never
guessed. **New: an AppImage**, `Screepub-linux-x86_64.AppImage`. The Tauri
era could not ship one (linuxdeploy's patchelf corrupted the 102 MB Bun
engine); the Rust app has no sidecar. appimagetool 1.9.1 and the type-2
runtime 20251108 are pinned by the SHA-256 GitHub records for them.
`SHA256SUMS-app` therefore covers five installers, not four.

### The command-line tool

Built per OS on its own runner (Rust does not cross-compile a whole OS's
libraries the way Bun did; a native runner is also the smoke test), and
linux-arm64 now runs on GitHub's arm64 runner, so it is executed in CI for
the first time. Same four archive names; each holds one file named
`screepub`. The Mac binary is signed with the hardened runtime, notarized,
then packaged.

## (b) The updater handover

Every Tauri window that can update (0.7.1 through 0.11.1; each tag's
`desktop/src-tauri/Cargo.lock` and `tauri.conf.json` were read: all ship
tauri 2.11.5 and tauri-plugin-updater 2.12.0, all check
`https://github.com/method-production-suite/method-reader/releases/latest/download/latest.json`,
all trust minisign key `E4D0668D54F6EB34`, none sets
`requireSignedVersion` or `dangerousInsecureTransportProtocol`) is handed
over by a release delivered in exactly that format. A 0.7.0 window trusts
`EC5C19F83FC2D502`, the key whose password was lost (commit 17ad548,
2026-09-22); it could not take 0.7.1 and still cannot take anything, as
before. What happens, line by line, in
`~/.cargo/registry/src/.../tauri-plugin-updater-2.12.0/src/updater.rs`:

1. **The manifest** (lines 1460 to 1506, `impl Deserialize for
   RemoteRelease`): `version` (a leading `v` trimmed, semver), `notes`,
   RFC 3339 `pub_date`, `platforms`. `sp-release manifest` writes exactly
   the shape `tools/build-update-manifest.ts` wrote (a test pins the text).
2. **The platform** (`get_urls`, line 608): `darwin-aarch64-app`, then
   `darwin-aarch64`. The manifest keeps `darwin-aarch64`, so every window
   finds its row, and an Intel 0.9.0 copy still finds none and is told why
   in plain words (unchanged).
3. **Newer** (line 576): `release.version > current_version`, semver; the
   window then applies its own comparator (`update-compare.js`); 0.12.0
   passes both.
4. **The signature** (`verify_signature`, line 1530; `verify_signed_version`,
   line 1562): base64 of the key box and of the signature box, minisign
   with legacy allowed, then, ONLY if the trusted comment carries
   `version:`, it must equal the announced version. `sp-release
   sign-update` uses the same `minisign` crate tauri-cli 2.11.4 uses
   (`helpers/updater_signature.rs`), the same untrusted comment, and adds
   `\tversion:<v>`: an older window checks it and accepts it; a tampered
   manifest pairing a new version with an older archive is refused by every
   window and by the Rust app.
5. **The install** (macOS `install_inner`, line 1294): gunzip, untar,
   **drop the first path component of every entry** (line 1315), so the
   archive's own `.app` name never matters; rename the running bundle into
   a temp folder; rename the new contents into **the running bundle's own
   path** (line 1379; `extract_path_from_executable`, line 1430, walks up
   from `Contents/MacOS`); `touch` it. The identifier is never looked at.
   An unwritable folder goes through an administrator prompt (line 1351).
6. **The restart** (tauri 2.11.5 `process.rs`, `restart_macos_app`, line
   92): the window's `relaunch()` reads `CFBundleExecutable` **from the new
   Info.plist** (line 115) and spawns `Contents/MacOS/<that>` with the same
   arguments, then exits. So the Rust app starts from the same path, as a
   child of the old process, inheriting its environment.

**Proved by running it.** `rust/handover-proof` builds a Tauri app on
tauri's mock runtime with the plugin configured as tauri.conf.json
configures a window, points it at a bundle as if running from it, and makes
the window's calls: `check()`, `downloadAndInstall()`. Run 2026-10-11:

```text
before: Screepub 0.11.1 (com.darkwell.screepub), runs screepub-desktop
check(): 0.11.1 -> 0.12.0 for darwin from http://127.0.0.1:50099/Screepub-macOS.app.tar.gz
downloadAndInstall(): 7524931 bytes, verified, the bundle swapped in place
after: Screepub 0.12.0 (com.darkwell.screepub.rehearsal), runs screepub-desktop
relaunch() would start .../Screepub.app/Contents/MacOS/screepub-desktop
```

The "before" bundle is the one v0.11.1 published (its
`Screepub-macOS.app.tar.gz`, downloaded); the archive is a Rust 0.12.0
built by the rehearsal (hence its rehearsal identifier) and signed with a
throwaway key the proof was configured to trust. Afterwards
`Contents/MacOS` holds only the Rust binary; the Bun engine is gone; nothing
is left beside the bundle.

### What carries across, and how

| | How it carries |
|---|---|
| The bundle path, Dock and Finder | Same path, swapped in place. |
| The identifier | `com.darkwell.screepub` in the Rust Info.plist. |
| The settings file and the library | The engine's, by path (`~/Library/Application Support/Screepub/settings.json`, `~/Documents/Screepub`), not by identifier; sp-engine keeps the paths and `SCREEPUB_CONFIG_DIR` / `SCREEPUB_LIBRARY`. |
| The update consent | The four keys the window kept in WebKit localStorage (`updateLastChecked`, `updateOptIn`, `updateAsked`, `updateFound`). On its first run (no `window-storage.json` yet) the Rust app reads them from `~/Library/WebKit/<identifier>/WebsiteData/` (both known layouts; a 0.7.x window's `.desktop` identifier as a fallback), WAL included, read-only, and writes them into its own storage. Nothing found means the question is asked again, never a silent yes. |
| The removable-volume (TCC) grant | Keyed to the designated requirement: same Developer ID team and identifier, so expected to hold (not verified on a device; the F2 notes say the same of the Swift-to-Tauri move). |
| The frozen Swift updater | Still finds exactly one `.dmg`, signed by the same team, with the same identifier. Unchanged. |

### Between install and restart

The old window retires its engine the moment the install returns
(`retireEngine`, app.js), waits for 500 ms of quiet capped at 120 s, then
relaunches. After the swap its engine sidecar path no longer exists; the
window already refuses every new engine call by then with "Screepub has
been updated. Restart it to keep going." A call already running finishes
(its process was exec'd from the old file). Nothing here changes.

### Risks

1. **The consent import is read off WebKit's layout, not measured on a real
   install.** Mitigated: both layouts are read, the origin is matched, a
   damaged or ambiguous store reads as nothing (asked again), and the
   rehearsal's `verify` prints what was imported. One `run` on a real Mac
   settles it.
2. **`latest.json` must keep `darwin-aarch64` for as long as any Tauri
   window may be out there**, and the Rust app reads the same key, so this
   is the natural state. The manifest builder refuses anything else.
3. **The key cannot change.** Every window since 0.7.1 and the Rust app
   trust `E4D0668D54F6EB34`; losing it (or its password, as happened to
   0.7.0's key) ends updates for all of them.
4. **A window with no write access to its folder** gets the plugin's
   administrator prompt (AppleScript), exactly as for any Tauri update.
5. **An app outside `/Applications` on another volume**: the plugin's
   rename into the temp folder fails across volumes, as it always has for
   Tauri updates. The Rust app's own updater stages beside the bundle and
   does not have this problem.
6. **The first Rust release must be a normal (non-prerelease) tag.**
   `releases/latest/download` never resolves to a prerelease.
7. **The Rust app's engine must read the TypeScript engine's settings file
   as written.** That is sp-engine's parity, not this wave's, but a
   regression there would surface first at the handover.

## (c) The Rust app's own updater (sp-update)

Same `latest.json`, same key, same rules as the plugin (each function cites
its plugin source), with three deliberate differences: the Rust app
**requires** the signed version (every release it can be offered is signed
by `sp-release`, which always records it); the swap stages beside the
bundle, so both renames stay on one volume, and puts the old bundle back if
the second rename fails; and an archive entry that would land outside the
bundle (`..`, an absolute path) or a bundle without an Info.plist naming an
existing executable is refused before anything moves.

- **Transport**: ureq 3 with rustls and the webpki roots (the stack sp-app
  already builds). rustls is pure Rust; its crypto provider is `ring`,
  which carries C and assembly for the primitives. **Owner decision:** the
  pure-Rust provider (`rustls-rustcrypto`) exists but is not audited; ring
  is what the rest of the Rust ecosystem ships. A 1 GB cap refuses a server
  that never stops sending.
- **Install per OS**: the Mac swap above; an AppImage written beside the
  running file and renamed over it; Windows hands the verified installer
  `/S /R`. Only the Mac is offered updates (row U1: the manifest lists
  darwin only); the other two are built and tested for the day it does not.
- **Restart** (`restart.rs`): the new Info.plist's CFBundleExecutable,
  this process's arguments, then exit, as tauri's restart. A refused
  restart is "installed", never "failed" (row U7).
- **The engine gate** (`gate.rs`): counted work and holds, 500 ms of quiet,
  the 120 s cap that turns into "Restart now", and retirement with the
  window's own sentence. In the Rust app the engine is linked in, so the
  hazard is narrower (only what is started from the bundle path, such as
  the MTP helper), but the rule is kept whole.
- **The order** (`flow.rs`): `installAndRestart` and `restartInstalled` from
  update.js, every side effect injected; tests cover the whole run,
  waiting, stalled, a refused restart, a copy that cannot restart, failed
  download or install never retiring, and a remembered offer checking
  afresh.
- **Wired into sp-app now**: the check (through sp-app's existing
  `run_check`, stamp and throttle unchanged) and the consent import.
  **Not yet wired**: the install button's run and the engine gate around
  sp-app's engine calls. That is the screens' work (rows U7, U9); the API
  is ready for it.

## (d) The Homebrew formula

Unchanged in shape, so `tools/bump-tap.sh` and `tools/check-tap.sh` need no
edit (they stay shell, as CI glue): the formula serves
`screepub-cli-macos-arm64.tar.gz`, one file named `screepub`,
`depends_on arch: :arm64`. Its `test do` block is the contract the Rust CLI
(sp-cli) must keep: `screepub --version` prints the version and
`screepub --help` contains "screenplay". The tarball is the signed,
notarized binary; the Rust one needs no entitlements file.

## (e) CI

`release.yml`, same secrets (`DEVELOPER_ID_CERT_P12_BASE64`,
`CERT_PASSWORD`, `KEYCHAIN_PASSWORD`, `AC_API_KEY_P8_BASE64`,
`AC_API_KEY_ID`, `AC_API_ISSUER_ID`, `TAURI_SIGNING_PRIVATE_KEY`,
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `TAP_TOKEN`), same draft-first,
publish-last order:

```text
checks ─┬─ release (Mac CLI: build, sign, notarize, package; the draft)
        ├─ cli (linux-x64, linux-arm64, windows-x64: build, run, package) ── cli-upload
        └─ app-bundles (mac, linux, windows: build, sign, notarize, package, open and check)
               └─ handover-gate (v0.11.1's own updater installs this release)
                      └─ app-upload (installers, update archive + .sig, then latest.json LAST,
                                     verified against the shipped key first)
publish (needs release, cli-upload, app-upload, handover-gate; refuses if the gate failed)
  └─ tap ── tap-check
latest-check (reads latest.json from the endpoint, downloads each archive and verifies it)
```

- `checks` has one version file to compare with the tag now:
  `[workspace.package] version` in `rust/Cargo.toml` (`sp-release
  version-check`), which every crate inherits. The four-file agreement of
  the TypeScript era is gone with the files.
- The Developer ID import is one composite action used by both signing
  jobs (tauri-bundler used to import its own).
- `rust.yml` replaces `ci.yml` and `desktop.yml` at the cutover: fmt,
  clippy, tests on three OS (sqlite3 installed on Linux for the Worker's
  SQL tests); the Worker's wasm32 build and clippy; every installer built
  unsigned, opened and checked on its own OS (the .deb installed and
  removed, the NSIS installer run silently and uninstalled, the update
  archive signed with a throwaway key and verified); the rehearsal on
  demand.
- `tap-freshness.yml`'s manifest guard runs `sp-release check-latest
  --deep`.
- Not ported here, for the xtask wave: `tools/smoke-cli.ts` (release.yml
  runs a smaller inline smoke: the version line, the two fixtures,
  `devices`), `tools/verify-signing.ts`'s deeper certificate checks (the
  YAML checks codesign, the requirement's identifier, notarization and
  stapling), and `tools/bump-version.ts` (now one line in one file).
- Release builds are uncached; `--locked` everywhere.

## (f) The Worker

`rust/crates/sp-device-requests` answers every request the way
`services/device-requests/src/worker.ts` does: the same two forms, the same
validation (JavaScript's `trim`, `\s`, UTF-16 lengths, `Number()` and the
`application/json\b` test reproduced, because Rust's differ at the edges),
the limiter asked only about a good form and keyed on `cf-connecting-ip`,
dates rounded to the UTC day, CORS for the three Tauri origins, and the
same JSON answers byte for byte. The handler is plain Rust over two small
traits; `glue.rs` (wasm32 only) adapts workers-rs 0.8.7's request, D1 and
rate limiter (`env.rate_limiter`). Its `wrangler.toml`
binds the same Worker name, database id and limiter as the live one (a test
holds the two files together), so a deploy replaces the TypeScript Worker
at the same address with no database change.

Two inputs answer differently, both refused either way (400), and neither
is anything the window sends: a JSON number beyond the double range, and a
lone UTF-16 surrogate escape (`"\ud800"`). `JSON.parse` accepts both and the
form check then refuses with "not one this service takes"; serde_json
refuses them as "not JSON". Recorded in PARITY.md.

**Deploying it** (owner, when wanted; nothing here contacted the live
service): `wrangler login`; from `rust/crates/sp-device-requests`,
`npx wrangler dev` first (local, on Miniflare, which should simulate the D1 and rate-limit bindings; worker-build downloads
wasm-bindgen and esbuild on first use), then `npx wrangler deploy`, which
replaces the live Worker; `wrangler rollback` returns to the TypeScript one.
The schema and the one migration are unchanged and already applied.
Wrangler itself is JavaScript, like Calibre an outside tool the platform
requires.

## Owner actions

1. Run the rehearsal with a window on an Apple-silicon Mac:
   `tools/rehearse-handover.sh prepare`, then `run <work>`, click through
   the update, `verify <work>`, `clean <work>`. Answer "yes" to the
   once-a-day question first to see the consent carry over. About 15
   minutes, most of it the first build. Uses a rehearsal identifier and a
   throwaway key; the real app, its settings and its storage are not
   touched.
2. Decide: the TLS crypto provider (ring, or the unaudited pure-Rust one);
   Windows code signing (a certificate and signtool, or stay unsigned).
3. When the Rust app is ready to ship: the secrets are unchanged, so the
   first `v0.12.0` tag from main runs this release.yml as is; its
   `handover-gate` must be green before anything goes public.
4. The Worker deploy, when wanted (above).
