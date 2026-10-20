# App Bundles and Installers (piece E2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the finished Tauri window into something a person can install — `.deb` and `.rpm` on Linux, two signed-and-notarized per-arch DMGs on macOS, and one unsigned NSIS installer on Windows — built, verified and smoke-tested by tools that can be run by hand.

**Architecture:** Two Bun scripts hold every decision (`tools/build-app-bundle.ts` builds, renames and verifies; `tools/smoke-bundle.ts` opens a bundle without installing it and runs the engine out of it), with a third module (`tools/bundle-archive.ts`) holding pure-TypeScript readers for the `.deb` and `.rpm` containers. `cargo tauri build` does the bundling; every artifact-shaping decision that would otherwise live in YAML lives in TypeScript where `bun test` can reach it. The workflows are thin callers.

**Tech Stack:** Bun/TypeScript (`bun:test`), Rust/Cargo, `cargo-tauri` 2.11.4 (tauri-bundler 2.9.4), GitHub Actions.

**Spec:** [`docs/superpowers/specs/2026-09-14-app-bundles-design.md`](../specs/2026-09-14-app-bundles-design.md)
**Program:** [ADR 2026-09-12 — cross-platform Tauri](../../adr/2026-09-12-cross-platform-tauri.md) (this is piece **E2**)

---

## Global Constraints

Every task's requirements implicitly include all of these.

- **Nothing under `app/` may be modified.** The SwiftUI app ships until piece F and the two must coexist. `app/release.sh` is neither reused nor modified. The Homebrew tap, `tools/bump-tap.sh`, `tools/check-tap.sh` and `tap-freshness.yml` are untouched.
- **The engine suite stays green.** Baseline at the start of this piece: **1219 pass / 3 skip / 0 fail (1222 total)**. New tasks add passing tests; the three pre-existing skips stay skips and the fail count stays 0. Every task that adds tests must record the new totals in its commit message so the next reviewer can see the delta rather than guess it.
- **`bunx tsc --noEmit` stays clean.**
- **`tests/fixtures/` holds exactly its five committed files** — `blank-pages.pdf`, `engine-result-sample.json`, `prose.pdf`, `screenplay.pdf`, `torture.pdf`. No task adds a sixth.
- **No test may write into a real home directory.** Scratch goes to `mkdtempSync(join(tmpdir(), …))` and is removed in `afterAll`.
- **No large binary enters the repo.** The only new binaries are the icon set (about 160 KB in total, sizes listed in Task 1). A `.deb`, `.rpm`, `.dmg` or `.exe` is never committed.
- **No test spawns `cargo`.** The unit tests use injected fakes. The one test that opens a real bundle is gated on a bundle already existing on disk and self-skips otherwise.
- **Distribution channels are out of scope**: no Homebrew cask for the Tauri app, no winget manifest, no AUR PKGBUILD, no Flathub, no Snap, no auto-updater, no Windows code signing, no universal macOS DMG, no AppImage.
- **Target version is `0.6.0`.** `package.json` stays at `0.5.4` on this branch; the split is deliberate and no `bun test` assertion may demand the three version files agree (a test that fails on every working branch gets deleted). The tag-time agreement check lives in `release.yml`.
- **Honesty rule.** No prose may claim a platform works that nobody has exercised. Every claim about macOS or Windows in this piece is a claim about what CI *ran*, not about what a person *saw*. The last review's three blocking findings were all shipped prose claiming something the code did not do.

## What was verified on this machine while writing this plan

These re-check the spec's measurements on 2026-09-14, aarch64-unknown-linux-gnu, `cargo-tauri` 2.11.4, tauri-bundler **2.9.4** (the spec says 2.6.1 — it has moved). Build steps below may rely on them.

1. `cargo tauri build --bundles deb,rpm` succeeds and produces `Screepub_0.6.0_arm64.deb` (44,354,712 B) and `Screepub-0.6.0-1.aarch64.rpm` (44,347,227 B).
2. The `.deb` is an `ar` archive (magic `!<arch>\n`) holding exactly `debian-binary`, `control.tar.gz`, `data.tar.gz`. Tar member names carry **no** `./` prefix: `usr/bin/screepub-engine`.
3. The `.rpm` magic is `ed ab ee db`; its payload is **cpio compressed with gzip** (`PAYLOADCOMPRESSOR: gzip`, `PAYLOADFORMAT: cpio`). Its cpio entry names **do** carry a `./` prefix.
4. `./usr/bin/screepub-engine --version --json` extracted from the `.deb` prints `{"ok":true,"version":"0.5.4"}` — the version split, still live.
5. Setting `bundle.publisher`, `bundle.category`, `bundle.shortDescription`, `bundle.longDescription`, `bundle.resources` and `bundle.fileAssociations` fixes all four Linux defects in one config edit. Observed `control`: `Maintainer: <publisher>`, human `Description:` with the long description as its continuation line. Observed `Screepub.desktop`: `Categories=Office;`, `Comment=…`, `MimeType=application/pdf`. Observed data listing gained `usr/lib/Screepub/LICENSE` and `usr/lib/Screepub/THIRD-PARTY-NOTICES.md`.
6. **`Categories=Office;Publishing;` is not reachable.** `bundle.category` is a fixed enum; `Productivity` maps to the literal `"Office;"` (`tauri-bundler-2.9.4/src/bundle/category.rs:183`). Only a custom `linux.deb.desktopTemplate` could add `Publishing;`. This plan does not add one — see Task 2.
7. **`cargo tauri build` rewrites `Cargo.toml`, and the expanded spelling is a fixed point.** After the first run left `tauri-build = { version = "2", features = [] }`, a second run left the file byte-identical. So committing the expanded spelling really does make the rewrite a no-op — the spec offered that as an untested option.
8. `cargo tauri icon assets/icon.svg` works from the SVG with no extra tooling and writes `icon.ico` (13,673 B, six sizes) and `icon.icns` (74,938 B), plus `android/`, `ios/`, `Square*Logo.png` and `StoreLogo.png` that this project does not use, and it **overwrites `icon.png`** (12,738 B, 512×512, replacing the committed 17,419 B one).
9. **`rpm2cpio` is not installed on this machine** (nor `dpkg-deb`, `rpm`, `7z`). `ar`, `cpio`, `tar` and `bsdtar` are. This is why Task 3 writes the readers in TypeScript instead of shelling out as the spec sketched.
10. Exact bundler output names (`tauri-bundler-2.9.4`): DMG is `{productName}_{version}_{x64|aarch64}.dmg` in `bundle/dmg/` (`macos/dmg/mod.rs:41-56`); NSIS is `{productName}_{version}_{x64|arm64}-setup.exe` in `bundle/nsis/` (`windows/nsis/mod.rs:651-666`). `bundle_dmg` writes a temporary `rw.$$.<name>.dmg` **in the same directory** (`macos/dmg/bundle_dmg:317`) which survives a crashed run.
11. `--bundles dmg` builds the `.app` itself if it is not already in the bundle list (`bundle.rs:164-173`), so `app,dmg` is safe and explicit.

**Still unverifiable here, by anyone, at the time of writing:** every macOS and Windows fact. No `.app`, no `.dmg`, no NSIS `.exe` has been produced or opened by this project. `desktop.yml` has never run at all — the branch is unpushed — so its Windows leg is expected to be red the first time and Task 1 exists to change that.

---

## File Structure

**Created**

| File | Responsibility |
| --- | --- |
| `desktop/src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` | The icon set the three bundlers need. `icon.ico` is what makes a Windows build possible at all. |
| `desktop/src-tauri/tauri.transition.conf.json` | Transition-only overlay: renames the macOS product to `Screepub Desktop` so it cannot collide with the SwiftUI `Screepub.app`. Piece F deletes this one file. |
| `tools/bundle-archive.ts` | Pure-TypeScript readers for the `.deb` (`ar` + gzip + tar) and `.rpm` (RPM headers + gzip + cpio) containers. No external tool, on any platform. |
| `tools/build-app-bundle.ts` | The bundle matrix, the `cargo tauri build` argv, artifact discovery and renaming, container verification, `SHA256SUMS-app`. |
| `tools/smoke-bundle.ts` | Opens a built bundle without installing it, runs the engine from inside it, converts the committed fixture through it. |
| `tests/bundle-archive.test.ts` | Unit tests for both container readers, against containers the test builds itself. |
| `tests/build-app-bundle.test.ts` | Unit tests for the matrix, argv, naming, version gate and verification. |
| `tests/smoke-bundle.test.ts` | Unit tests for the smoke assertions, against injected fakes. |
| `tests/app-bundle-e2e.test.ts` | The one test that opens a real bundle. Self-skips when none is on disk. |

**Modified**

| File | Change |
| --- | --- |
| `desktop/src-tauri/tauri.conf.json` | `bundle.icon` list; `publisher`, `category`, `shortDescription`, `longDescription`, `resources`, `fileAssociations`. |
| `desktop/src-tauri/icons/icon.png` | Regenerated by `cargo tauri icon` so the whole set comes from one renderer. |
| `desktop/src-tauri/Cargo.toml` | The expanded `features = []` spelling the Tauri CLI writes, committed once so bundling is a no-op against it. |
| `tools/build-cli.ts` | Two small extractions so `bundle-archive.ts` and `build-app-bundle.ts` need no copies: `tarEntries(bytes)` split out of `tarGzEntries(path)`, and a `fileName` parameter on `writeChecksums`. |
| `tests/desktop-shell.test.ts` | Pins the new `tauri.conf.json` bundle metadata and the icon list. |
| `tests/release-artifacts.test.ts` | Pins the new `release.yml` jobs and the new documentation claims. |
| `.github/workflows/desktop.yml` | Installs `cargo-tauri`, bundles and smokes on every push, on each of the three legs. |
| `.github/workflows/release.yml` | `checks` gains two version assertions; new `app-bundles` matrix job and `app-upload` job. |
| `README.md`, `site/index.html`, `docs/releases/0.6.0.md` | The app downloads, the unsigned-Windows warning, and what CI actually executed. |
| `desktop/README.md` | The verification ledger: what is verified locally, by CI, and by nobody. |

---

## Task 1: The icon set, and the Windows build that cannot happen without it

**Files:**
- Create: `desktop/src-tauri/icons/32x32.png`, `desktop/src-tauri/icons/128x128.png`, `desktop/src-tauri/icons/128x128@2x.png`, `desktop/src-tauri/icons/icon.icns`, `desktop/src-tauri/icons/icon.ico`
- Modify: `desktop/src-tauri/icons/icon.png` (regenerated), `desktop/src-tauri/tauri.conf.json`
- Test: `tests/desktop-shell.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `desktop/src-tauri/icons/icon.ico` on disk, and `CONFIG.bundle.icon` in `tauri.conf.json` as the exact array `["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.png", "icons/icon.icns", "icons/icon.ico"]`. Every later task that builds a bundle relies on this array.

- [ ] **Step 1: Write the failing test**

Append to `tests/desktop-shell.test.ts`, inside a new `describe` at the end of the file. `CONFIG` and `REPO` are already defined at the top of that file (line 10 reads `tauri.conf.json` into `CONFIG`).

```ts
describe('the icon set the bundlers need', () => {
  const icons = join(REPO, 'desktop', 'src-tauri', 'icons');

  test('tauri.conf.json names every icon the three bundlers ask for', () => {
    // Order matters to nobody, presence matters to everybody: without
    // icons/icon.ico the Windows build script errors out before a single
    // Rust file compiles (tauri-build's lib.rs, "required for generating a
    // Windows Resource file"). desktop.yml's Windows leg has never run, so
    // this list is the only thing standing between it and a red first run.
    expect(CONFIG.bundle.icon).toEqual([
      'icons/32x32.png',
      'icons/128x128.png',
      'icons/128x128@2x.png',
      'icons/icon.png',
      'icons/icon.icns',
      'icons/icon.ico',
    ]);
  });

  test('every file it names is really there', () => {
    for (const rel of CONFIG.bundle.icon as string[]) {
      expect(existsSync(join(REPO, 'desktop', 'src-tauri', rel))).toBe(true);
    }
  });

  test('icon.ico is a real ICO and not a renamed PNG', () => {
    // A copied-and-renamed icon.png passes an existsSync check and then
    // fails the Windows build anyway. The ICONDIR header is 6 bytes:
    // reserved=0 (u16 LE), type=1 (u16 LE, 1 = icon), count > 0 (u16 LE).
    const head = readFileSync(join(icons, 'icon.ico')).subarray(0, 6);
    const u16 = (o: number) => head[o]! | (head[o + 1]! << 8);
    expect(u16(0)).toBe(0);
    expect(u16(2)).toBe(1);
    expect(u16(4)).toBeGreaterThan(0);
  });

  test('icon.icns is a real ICNS whose declared length matches the file', () => {
    // Same trap on the macOS side. The header is the ASCII magic 'icns'
    // followed by the total file length as a BIG-endian u32 -- so a
    // truncated copy fails here even though its first four bytes are right.
    const bytes = readFileSync(join(icons, 'icon.icns'));
    expect(bytes.subarray(0, 4).toString('latin1')).toBe('icns');
    expect(bytes.readUInt32BE(4)).toBe(bytes.length);
  });

  test('icon.png is still the 512-pixel square the Linux packages scale from', () => {
    // The .deb installs the largest PNG as the hicolor icon. IHDR puts
    // width and height at bytes 16..24, big-endian.
    const bytes = readFileSync(join(icons, 'icon.png'));
    expect(bytes.subarray(1, 4).toString('latin1')).toBe('PNG');
    expect(bytes.readUInt32BE(16)).toBe(512);
    expect(bytes.readUInt32BE(20)).toBe(512);
  });

  test('the platform icon sets nobody ships were not committed', () => {
    // `cargo tauri icon` also writes android/, ios/, Square*Logo.png and
    // StoreLogo.png. Screepub has no mobile build and no MSIX, so those are
    // 750 KB of files no bundler opens.
    for (const junk of ['android', 'ios', 'StoreLogo.png', 'Square44x44Logo.png']) {
      expect(existsSync(join(icons, junk))).toBe(false);
    }
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test tests/desktop-shell.test.ts`
Expected: FAIL. `tauri.conf.json` currently has `"icon": ["icons/icon.png"]` and `desktop/src-tauri/icons/` holds only `icon.png`, so the first two tests fail and the ICO and ICNS tests throw ENOENT.

- [ ] **Step 3: Generate the icons**

```bash
cargo tauri icon assets/icon.svg -o desktop/src-tauri/icons
```

Takes a few seconds and needs no `rsvg-convert`, no ImageMagick and no network. It rewrites `icon.png` from the same SVG, which is intentional: the whole set then comes from one renderer.

- [ ] **Step 4: Delete the files this project does not ship**

```bash
rm -rf desktop/src-tauri/icons/android desktop/src-tauri/icons/ios
rm -f desktop/src-tauri/icons/Square*Logo.png desktop/src-tauri/icons/StoreLogo.png
rm -f desktop/src-tauri/icons/64x64.png
ls -l desktop/src-tauri/icons
```

Expected remainder, six files, about 160 KB in total: `32x32.png` (~1.0 KB), `128x128.png` (~3.2 KB), `128x128@2x.png` (~6.5 KB), `icon.png` (~12.7 KB), `icon.icns` (~74.9 KB), `icon.ico` (~13.7 KB). `64x64.png` goes because nothing in `bundle.icon` names it; leaving an unnamed PNG beside named ones is the kind of file that later gets "fixed" into the list by someone guessing.

- [ ] **Step 5: Point the config at them**

In `desktop/src-tauri/tauri.conf.json`, replace the `bundle.icon` line:

```json
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
```

- [ ] **Step 6: Run the tests**

Run: `bun test tests/desktop-shell.test.ts`
Expected: PASS, all six new tests plus the file's existing ones.

- [ ] **Step 7: Prove the icons did not break the build that already worked**

```bash
bun tools/build-sidecar.ts --host
cd desktop/src-tauri && cargo build --locked && cd ../..
git checkout desktop/src-tauri/Cargo.toml
```

Expected: `Finished dev profile`. `cargo build` (not `cargo tauri build`) reads `icons/icon.png` in its build script, so a corrupt regeneration fails here rather than at bundling time. The `git checkout` is belt and braces — a plain `cargo build` should not rewrite the manifest, and Task 8 makes the question moot.

- [ ] **Step 8: Commit**

```bash
git add desktop/src-tauri/icons desktop/src-tauri/tauri.conf.json tests/desktop-shell.test.ts
git commit -m "Give the Windows build the icon it refuses to compile without

tauri-build's build script errors out on a missing icons/icon.ico before
any Rust compiles, so desktop.yml's Windows leg -- which has never run --
could not have gone green. The whole set is regenerated from assets/icon.svg
by \`cargo tauri icon\` so one renderer produces all six, and the android,
ios and Windows Store sets it also writes are deleted rather than committed:
this project ships none of them.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe"
```

---

## Task 2: The four things the stock Linux package gets wrong

**Files:**
- Modify: `desktop/src-tauri/tauri.conf.json`
- Test: `tests/desktop-shell.test.ts`

**Interfaces:**
- Consumes: Task 1's `bundle.icon` array.
- Produces: in `tauri.conf.json`, `bundle.publisher = "<publisher>"`, `bundle.category = "Productivity"`, `bundle.shortDescription`, `bundle.longDescription`, `bundle.resources` as the object `{"../../LICENSE": "LICENSE", "../../THIRD-PARTY-NOTICES.md": "THIRD-PARTY-NOTICES.md"}`, and `bundle.fileAssociations` as a one-element array. Task 6's e2e test asserts these reach the built package.

- [ ] **Step 1: Write the failing test**

Append to `tests/desktop-shell.test.ts`:

```ts
describe('what the Linux package tells a user about itself', () => {
  test('the publisher is the company, not a slice of the bundle identifier', () => {
    // Absent this, tauri-bundler derives Maintainer: from the SECOND segment
    // of com.darkwell.screepub.desktop and the .deb says "Maintainer:
    // <org>". Observed, before this change, in a real build's control file.
    expect(CONFIG.bundle.publisher).toBe('<publisher>');
  });

  test('the descriptions are written for a user, not for a contributor', () => {
    const short = CONFIG.bundle.shortDescription as string;
    const long = CONFIG.bundle.longDescription as string;
    // Both reach `apt show`. The crate's own description -- "A window around
    // the engine; no logic lives here" -- is a note to the next maintainer
    // and was what shipped.
    expect(short.length).toBeGreaterThan(20);
    expect(long.length).toBeGreaterThan(60);
    for (const text of [short, long]) {
      expect(text.toLowerCase()).not.toContain('sidecar');
      expect(text.toLowerCase()).not.toContain('shell');
      expect(text.toLowerCase()).not.toContain('no logic lives here');
    }
    // The short one also becomes Comment= in the .desktop entry, where a
    // trailing newline or a leading space would be copied verbatim.
    expect(short).toBe(short.trim());
    expect(short).not.toContain('\n');
  });

  test('the AGPL text and the third-party notices travel with the binary', () => {
    // app/build-app.sh puts both inside Screepub.app for exactly this
    // reason: the AGPL requires the licence to accompany the work, and the
    // compiled engine embeds Apache-2.0 and MIT libraries. Paths are
    // relative to tauri.conf.json, hence ../../.
    expect(CONFIG.bundle.resources).toEqual({
      '../../LICENSE': 'LICENSE',
      '../../THIRD-PARTY-NOTICES.md': 'THIRD-PARTY-NOTICES.md',
    });
    // And the sources really exist, or the bundle step fails minutes later
    // with a glob that matched nothing.
    expect(existsSync(join(REPO, 'LICENSE'))).toBe(true);
    expect(existsSync(join(REPO, 'THIRD-PARTY-NOTICES.md'))).toBe(true);
  });

  test('the launcher entry offers to open a PDF and files itself under Office', () => {
    // MimeType= comes from fileAssociations[].mimeType and Categories= from
    // the category enum. The Swift app declares com.adobe.pdf in
    // CFBundleDocumentTypes so "Open With" offers it; this is the same
    // promise on Linux, and on macOS the same key produces the same plist.
    expect(CONFIG.bundle.category).toBe('Productivity');
    expect(CONFIG.bundle.fileAssociations).toEqual([
      { ext: ['pdf'], mimeType: 'application/pdf', name: 'PDF', role: 'Viewer' },
    ]);
  });

  test('the window title is NOT changed by any of this', () => {
    // productName drives the package name and the .app filename; the window
    // title is a separate key. Task 10 overrides productName for macOS only,
    // and this is the assertion that catches the overlay reaching too far.
    expect(CONFIG.productName).toBe('Screepub');
    expect(CONFIG.app.windows[0].title).toBe('Screepub');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test tests/desktop-shell.test.ts`
Expected: FAIL — `CONFIG.bundle.publisher` is `undefined`, as are `shortDescription`, `longDescription`, `resources`, `category` and `fileAssociations`.

- [ ] **Step 3: Write the config**

In `desktop/src-tauri/tauri.conf.json`, replace the whole `bundle` object with:

```json
  "bundle": {
    "externalBin": ["binaries/screepub-engine"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "publisher": "<publisher>",
    "category": "Productivity",
    "shortDescription": "Turn a screenplay PDF into a reflowable e-book.",
    "longDescription": "Screepub converts a screenplay PDF into an EPUB or MOBI that reflows properly on an e-reader, keeping cues with their dialogue and scenes with their headings, and copies the finished book onto a connected reader over USB.",
    "resources": {
      "../../LICENSE": "LICENSE",
      "../../THIRD-PARTY-NOTICES.md": "THIRD-PARTY-NOTICES.md"
    },
    "fileAssociations": [
      { "ext": ["pdf"], "mimeType": "application/pdf", "name": "PDF", "role": "Viewer" }
    ]
  }
```

`Depends: libwebkit2gtk-4.1-0, libgtk-3-0` is already correct in the generated control file and is not configured here; leave it alone.

- [ ] **Step 4: Run the tests**

Run: `bun test tests/desktop-shell.test.ts`
Expected: PASS.

- [ ] **Step 5: Record the one thing the spec asked for that this cannot give**

Add this comment immediately above `"category"` in `tauri.conf.json`... which JSON cannot hold. Put it in `desktop/README.md` instead, under a new heading near the end:

```markdown
### Why the launcher entry says `Categories=Office;` and not `Office;Publishing;`

The design doc asked for both. `bundle.category` is a fixed enum, not a
free string, and `Productivity` maps to the literal `"Office;"`
(`tauri-bundler/src/bundle/category.rs`); no enum member produces
`Publishing;`. The only way to add it is a custom
`linux.deb.desktopTemplate`, which replaces the generated `.desktop` file
wholesale and so takes over `Exec=`, `Icon=`, `StartupWMClass=` and
`MimeType=` as well — four more things to keep correct by hand, on a
surface nobody here can test, to add one category string. Not worth it.
`Office;` is what ships and this paragraph is why.
```

- [ ] **Step 6: Prove it against a real package, by hand, once**

```bash
bun tools/build-sidecar.ts --host
cd desktop/src-tauri && cargo tauri build --bundles deb && cd ../..
D=desktop/src-tauri/target/release/bundle/deb/Screepub_0.6.0_arm64.deb
ar p "$D" control.tar.gz | tar xzO control
ar p "$D" data.tar.gz | tar xzO usr/share/applications/Screepub.desktop
ar p "$D" data.tar.gz | tar tzf - | grep usr/lib
git checkout desktop/src-tauri/Cargo.toml
```

Expected, all four, on this machine (Linux with `ar` and GNU `tar`; a machine without them should skip this step and rely on Task 6's e2e test):

```
Maintainer: <publisher>
Description: Turn a screenplay PDF into a reflowable e-book.
 Screepub converts a screenplay PDF into an EPUB or MOBI that reflows properly ...
Categories=Office;
Comment=Turn a screenplay PDF into a reflowable e-book.
MimeType=application/pdf
usr/lib/Screepub
usr/lib/Screepub/LICENSE
usr/lib/Screepub/THIRD-PARTY-NOTICES.md
```

The `git checkout` undoes the manifest rewrite `cargo tauri build` performs. Task 8 makes that rewrite a no-op permanently.

- [ ] **Step 7: Commit**

```bash
git add desktop/src-tauri/tauri.conf.json tests/desktop-shell.test.ts desktop/README.md
git commit -m "Say who made the Linux package, what it does, and ship its licence

Four defects the stock bundle had, all fixed in config: Maintainer: read
'<org>' (a slice of the bundle identifier), Description: was the crate's
note to the next maintainer with '(none)' beneath it, no licence travelled
with a binary that embeds Apache-2.0 and MIT code, and the launcher entry
declared neither a category nor a MIME type. Verified against a real .deb.

Categories= says Office; and not Office;Publishing; because the category
field is an enum and no member produces Publishing; desktop/README.md says
so where the next person will look.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe"
```

---

## Task 3: Opening a `.deb` and an `.rpm` with no external tool

**Files:**
- Create: `tools/bundle-archive.ts`
- Modify: `tools/build-cli.ts` (extract `tarEntries` out of `tarGzEntries`)
- Test: `tests/bundle-archive.test.ts`

**Interfaces:**
- Consumes: `ArchiveEntry { name: string; size: number; mode: number; data: Uint8Array }` and `tarGzEntries(path: string): ArchiveEntry[]`, both already exported from `tools/build-cli.ts`.
- Produces, all from `tools/bundle-archive.ts`:
  - `export function arMembers(bytes: Uint8Array): Map<string, Uint8Array>`
  - `export function cpioEntries(bytes: Uint8Array): ArchiveEntry[]`
  - `export function rpmPayload(bytes: Uint8Array): Uint8Array`
  - `export function debEntries(path: string): ArchiveEntry[]`
  - `export function rpmEntries(path: string): ArchiveEntry[]`
  - `export function bundleEntries(path: string): ArchiveEntry[]` — dispatches on extension, throws on anything but `.deb`/`.rpm`.
  - `export function findEntry(entries: ArchiveEntry[], suffix: string): ArchiveEntry` — the one lookup both `.deb` (no `./` prefix) and `.rpm` (`./` prefix) can share; throws listing what it found.
- Also produces from `tools/build-cli.ts`: `export function tarEntries(tar: Uint8Array): ArchiveEntry[]`.

**Why TypeScript and not a shell-out.** The spec sketched `ar x` for the `.deb` and `rpm2cpio | cpio -id` for the `.rpm`. `rpm2cpio` is not installed on this machine and is not guaranteed on GitHub's `ubuntu-latest` image; `bsdtar` reads both but is not guaranteed either. The formats are 60-byte and 110-byte fixed ASCII headers plus a gzip stream Bun already has, so the readers are about a hundred lines and they work identically on all three runners with nothing installed — which is the same argument `build-cli.ts` already made for walking the tar itself instead of parsing `tar -tzvf` output.

- [ ] **Step 1: Extract `tarEntries` from `tarGzEntries`**

In `tools/build-cli.ts`, split the existing function so the byte-level walk can be called on bytes that never were a file. Replace the body of `tarGzEntries` and add the new export above it:

```ts
/** Walk a tar we already hold in memory. Split out of tarGzEntries so a
 *  caller holding decompressed bytes -- a .deb's data.tar.gz member, an
 *  .rpm's payload -- does not need a temporary file to read them. */
export function tarEntries(tar: Uint8Array): ArchiveEntry[] {
  const entries: ArchiveEntry[] = [];
  let off = 0;
  while (off + 512 <= tar.length) {
    const header = tar.subarray(off, off + 512);
    if (header.every((b) => b === 0)) break; // end-of-archive marker
    const name = tarString(header.subarray(0, 100));
    const mode = tarOctal(header.subarray(100, 108));
    const size = tarOctal(header.subarray(124, 136));
    const typeflag = String.fromCharCode(header[156] ?? 0);
    const start = off + 512;
    if (typeflag === '0' || typeflag === '\0') {
      entries.push({ name, size, mode, data: tar.subarray(start, start + size) });
    }
    off = start + Math.ceil(size / 512) * 512;
  }
  return entries;
}

export function tarGzEntries(archivePath: string): ArchiveEntry[] {
  return tarEntries(Bun.gunzipSync(readFileSync(archivePath)));
}
```

- [ ] **Step 2: Run the existing suite to prove the extraction changed nothing**

Run: `bun test tests/build-cli.test.ts tests/build-cli-e2e.test.ts`
Expected: PASS, with the same counts as before the edit. `tarGzEntries` is what `archiveEntries` calls for every `.tar.gz` artifact, so this is the guard that a pure refactor stayed pure.

- [ ] **Step 3: Write the failing tests**

Create `tests/bundle-archive.test.ts`. The tests build their own containers, so they assert against bytes whose correct answer is known by construction — no fixture file, no `cargo`, nothing committed.

```ts
import { describe, test, expect, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  arMembers,
  bundleEntries,
  cpioEntries,
  debEntries,
  findEntry,
  rpmEntries,
  rpmPayload,
} from '../tools/bundle-archive';
import { tarEntries, type ArchiveEntry } from '../tools/build-cli';

const OUT = mkdtempSync(join(tmpdir(), 'screepub-archive-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const enc = (s: string) => new TextEncoder().encode(s);

function concat(parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

/** A tar member, built by hand: 512-byte header, octal size, data padded
 *  to 512. Enough for these tests; not a general tar writer. */
function tarMember(name: string, data: Uint8Array, mode = 0o755): Uint8Array {
  const header = new Uint8Array(512);
  header.set(enc(name), 0);
  header.set(enc(mode.toString(8).padStart(7, '0') + '\0'), 100);
  header.set(enc(data.length.toString(8).padStart(11, '0') + '\0'), 124);
  header[156] = 0x30; // typeflag '0', a regular file
  // The checksum field must be spaces while a checksum is computed, and GNU
  // tar rejects a wrong one -- but tarEntries never reads it, so these stay
  // spaces and the test stays honest about what it exercises.
  header.set(enc('        '), 148);
  const pad = new Uint8Array((512 - (data.length % 512)) % 512);
  return concat([header, data, pad]);
}

function arMember(name: string, data: Uint8Array): Uint8Array {
  const header = new Uint8Array(60).fill(0x20); // ar space-pads every field
  header.set(enc(name), 0);
  header.set(enc(String(data.length)), 48);
  header[58] = 0x60; // the two-byte end magic, 0x60 0x0a
  header[59] = 0x0a;
  const pad = data.length % 2 ? new Uint8Array([0x0a]) : new Uint8Array(0);
  return concat([header, data, pad]);
}

function cpioMember(name: string, data: Uint8Array, mode = 0o100755): Uint8Array {
  const nameBytes = concat([enc(name), new Uint8Array([0])]);
  const hex = (n: number) => n.toString(16).padStart(8, '0');
  const header = enc(
    '070701' +
      hex(1) + hex(mode) + hex(0) + hex(0) + hex(1) + hex(0) +
      hex(data.length) + hex(0) + hex(0) + hex(0) + hex(0) +
      hex(nameBytes.length) + hex(0),
  );
  const namePad = new Uint8Array((4 - ((header.length + nameBytes.length) % 4)) % 4);
  const dataPad = new Uint8Array((4 - (data.length % 4)) % 4);
  return concat([header, nameBytes, namePad, data, dataPad]);
}

function cpioTrailer(): Uint8Array {
  return cpioMember('TRAILER!!!', new Uint8Array(0), 0);
}

/** An RPM is a 96-byte lead, then a signature header padded to 8 bytes,
 *  then a header, then the payload. Each header is 16 bytes of preamble
 *  plus nindex*16 index entries plus hsize bytes of store. */
function rpmHeader(nindex: number, hsize: number): Uint8Array {
  const h = new Uint8Array(16 + nindex * 16 + hsize);
  h.set([0x8e, 0xad, 0xe8, 0x01, 0, 0, 0, 0], 0);
  const view = new DataView(h.buffer);
  view.setUint32(8, nindex, false);
  view.setUint32(12, hsize, false);
  return h;
}

function fakeRpm(payload: Uint8Array): Uint8Array {
  const lead = new Uint8Array(96);
  lead.set([0xed, 0xab, 0xee, 0xdb, 0x03, 0x00, 0x00, 0x00], 0);
  // nindex=2, hsize=5 => 16 + 32 + 5 = 53 bytes, which is NOT a multiple of
  // 8, so the signature header's pad is exercised rather than skipped.
  const sig = rpmHeader(2, 5);
  const pad = new Uint8Array((8 - (sig.length % 8)) % 8);
  const hdr = rpmHeader(1, 3); // 16 + 16 + 3 = 35, never padded
  return concat([lead, sig, pad, hdr, payload]);
}

describe('the ar container a .deb is', () => {
  test('names its three members and hands back their exact bytes', () => {
    const deb = concat([
      enc('!<arch>\n'),
      arMember('debian-binary/  ', enc('2.0\n')),
      arMember('control.tar.gz/ ', enc('CONTROL')),
      arMember('data.tar.gz/    ', enc('DATA')),
    ]);
    const members = arMembers(deb);
    expect([...members.keys()]).toEqual(['debian-binary', 'control.tar.gz', 'data.tar.gz']);
    expect(new TextDecoder().decode(members.get('data.tar.gz')!)).toBe('DATA');
  });

  test('an odd-sized member does not shift the one after it', () => {
    // ar pads odd-length data to an even offset with a single \n. Get this
    // wrong and the NEXT member's header is read one byte late, which
    // yields a garbage name rather than an error -- so a reader that
    // ignored padding would still "work" on this project's real .deb,
    // where debian-binary happens to be 4 bytes.
    const deb = concat([
      enc('!<arch>\n'),
      arMember('odd/            ', enc('abc')), // 3 bytes, padded
      arMember('after/          ', enc('TAIL')),
    ]);
    const members = arMembers(deb);
    expect([...members.keys()]).toEqual(['odd', 'after']);
    expect(new TextDecoder().decode(members.get('after')!)).toBe('TAIL');
  });

  test('it refuses something that is not an ar archive at all', () => {
    expect(() => arMembers(enc('not an archive'))).toThrow(/!<arch>/);
  });

  test('debEntries reads a whole .deb off disk', () => {
    const tar = concat([
      tarMember('usr/bin/screepub-engine', enc('ENGINE'), 0o755),
      new Uint8Array(1024), // end-of-archive
    ]);
    const deb = concat([
      enc('!<arch>\n'),
      arMember('debian-binary/  ', enc('2.0\n')),
      arMember('control.tar.gz/ ', Bun.gzipSync(new Uint8Array(1024))),
      arMember('data.tar.gz/    ', Bun.gzipSync(tar)),
    ]);
    const path = join(OUT, 'fake.deb');
    writeFileSync(path, deb);
    const entries = debEntries(path);
    expect(entries.map((e) => e.name)).toEqual(['usr/bin/screepub-engine']);
    expect(new TextDecoder().decode(entries[0]!.data)).toBe('ENGINE');
    expect(entries[0]!.mode & 0o111).not.toBe(0);
  });

  test('a .deb with no data.tar.gz says so instead of returning nothing', () => {
    const deb = concat([enc('!<arch>\n'), arMember('debian-binary/  ', enc('2.0\n'))]);
    const path = join(OUT, 'empty.deb');
    writeFileSync(path, deb);
    expect(() => debEntries(path)).toThrow(/data\.tar\.gz/);
  });
});

describe('the cpio payload an .rpm carries', () => {
  test('it reads names, modes and bytes, and stops at the trailer', () => {
    const cpio = concat([
      cpioMember('./usr/bin/screepub-engine', enc('ENGINE'), 0o100755),
      cpioMember('./usr/lib/Screepub/LICENSE', enc('AGPL'), 0o100644),
      cpioTrailer(),
    ]);
    const entries = cpioEntries(cpio);
    expect(entries.map((e) => e.name)).toEqual([
      './usr/bin/screepub-engine',
      './usr/lib/Screepub/LICENSE',
    ]);
    expect(new TextDecoder().decode(entries[0]!.data)).toBe('ENGINE');
    expect(entries[0]!.mode & 0o111).not.toBe(0);
    // The second file is NOT executable. Without this, mode could be a
    // hardcoded 0o755 and every assertion above would still pass.
    expect(entries[1]!.mode & 0o111).toBe(0);
  });

  test('a name whose length is not a multiple of four does not shift the data', () => {
    // The header+name run is padded to 4 bytes and the data is padded
    // separately. These three names land on different sides of that rule.
    for (const name of ['./a', './abcd', './abcde']) {
      const entries = cpioEntries(concat([cpioMember(name, enc('XY')), cpioTrailer()]));
      expect(entries.map((e) => e.name)).toEqual([name]);
      expect(new TextDecoder().decode(entries[0]!.data)).toBe('XY');
    }
  });

  test('it refuses a payload that is not cpio', () => {
    expect(() => cpioEntries(enc('nope nope nope nope nope nope nope nope nope nope nope nope'))).toThrow(
      /07070/,
    );
  });

  test('rpmPayload skips the lead and both headers, padding included', () => {
    const payload = enc('PAYLOAD-BYTES');
    expect(new TextDecoder().decode(rpmPayload(fakeRpm(payload)))).toBe('PAYLOAD-BYTES');
  });

  test('rpmPayload refuses a file that is not an rpm', () => {
    expect(() => rpmPayload(new Uint8Array(200))).toThrow(/rpm/i);
  });

  test('rpmEntries reads a whole .rpm off disk', () => {
    const cpio = concat([cpioMember('./usr/bin/screepub-engine', enc('ENGINE')), cpioTrailer()]);
    const path = join(OUT, 'fake.rpm');
    writeFileSync(path, fakeRpm(Bun.gzipSync(cpio)));
    const entries = rpmEntries(path);
    expect(entries.map((e) => e.name)).toEqual(['./usr/bin/screepub-engine']);
    expect(new TextDecoder().decode(entries[0]!.data)).toBe('ENGINE');
  });
});

describe('finding one file inside either container', () => {
  const entries: ArchiveEntry[] = [
    { name: 'usr/bin/screepub-desktop', size: 3, mode: 0o755, data: enc('abc') },
    { name: './usr/bin/screepub-engine', size: 6, mode: 0o755, data: enc('ENGINE') },
  ];

  test('it matches across the ./ prefix the two formats disagree about', () => {
    // The .deb's tar names have no ./ and the .rpm's cpio names do. Both
    // were observed in real bundles on 2026-09-14.
    expect(findEntry(entries, 'usr/bin/screepub-engine').size).toBe(6);
    expect(findEntry(entries, './usr/bin/screepub-desktop').size).toBe(3);
  });

  test('it does not match a longer name that merely ends the same way', () => {
    const decoys: ArchiveEntry[] = [
      { name: 'usr/bin/not-screepub-engine', size: 1, mode: 0, data: enc('x') },
    ];
    expect(() => findEntry(decoys, 'usr/bin/screepub-engine')).toThrow(/not-screepub-engine/);
  });

  test('a miss names what it did find, so the failure is diagnosable', () => {
    expect(() => findEntry(entries, 'usr/bin/nothing')).toThrow(/screepub-desktop/);
  });
});

describe('bundleEntries dispatches on the extension', () => {
  test('it refuses a container it has no reader for', () => {
    expect(() => bundleEntries('/tmp/Screepub.dmg')).toThrow(/\.deb|\.rpm/);
    expect(() => bundleEntries('/tmp/Screepub-setup.exe')).toThrow(/\.deb|\.rpm/);
  });

  test('tarEntries still walks a plain tar, which both readers lean on', () => {
    const tar = concat([tarMember('a', enc('one')), tarMember('b', enc('two')), new Uint8Array(1024)]);
    expect(tarEntries(tar).map((e) => e.name)).toEqual(['a', 'b']);
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `bun test tests/bundle-archive.test.ts`
Expected: FAIL — `Cannot find module '../tools/bundle-archive'`.

- [ ] **Step 5: Write `tools/bundle-archive.ts`**

```ts
// Open a .deb or an .rpm and read the files inside it, with no external
// tool, on any platform.
//
// The design sketched `ar x` for the .deb and `rpm2cpio | cpio -id` for the
// .rpm. Neither is safe to assume: rpm2cpio is absent on this development
// machine, and neither it nor bsdtar is guaranteed on GitHub's ubuntu
// image. Both containers are fixed ASCII headers around a gzip stream Bun
// already decompresses, so the readers below are shorter than the CI step
// that would install the tools -- and they behave identically on Linux,
// macOS and Windows runners.
//
// Same reasoning tools/build-cli.ts already recorded for walking a tar
// itself rather than parsing `tar -tzvf` output.

import { readFileSync } from 'node:fs';
import { tarEntries, type ArchiveEntry } from './build-cli';

const AR_MAGIC = '!<arch>\n';

function ascii(bytes: Uint8Array): string {
  return new TextDecoder('latin1').decode(bytes);
}

/** The members of an `ar` archive, in order, by name.
 *
 *  Each member is a 60-byte header -- name(16) mtime(12) uid(6) gid(6)
 *  mode(8) size(10) magic(2) -- with every field space-padded, followed by
 *  the data padded to an EVEN offset with a single \n. GNU ar terminates
 *  the name with '/'; both spellings are trimmed here. */
export function arMembers(bytes: Uint8Array): Map<string, Uint8Array> {
  if (ascii(bytes.subarray(0, 8)) !== AR_MAGIC) {
    throw new Error(
      `bundle-archive: not an ar archive (expected the ${JSON.stringify(AR_MAGIC)} magic, ` +
        `got ${JSON.stringify(ascii(bytes.subarray(0, 8)))})`,
    );
  }
  const members = new Map<string, Uint8Array>();
  let off = 8;
  while (off + 60 <= bytes.length) {
    const header = bytes.subarray(off, off + 60);
    if (header[58] !== 0x60 || header[59] !== 0x0a) {
      throw new Error(`bundle-archive: ar member header at offset ${off} has no 0x60 0x0a magic`);
    }
    const name = ascii(header.subarray(0, 16)).trim().replace(/\/$/, '');
    const size = parseInt(ascii(header.subarray(48, 58)).trim(), 10);
    if (!Number.isFinite(size) || size < 0) {
      throw new Error(`bundle-archive: ar member "${name}" declares an unreadable size`);
    }
    const start = off + 60;
    members.set(name, bytes.subarray(start, start + size));
    off = start + size + (size % 2); // the \n pad to an even offset
  }
  return members;
}

/** Every regular file in a `newc`/`crc` cpio stream.
 *
 *  110-byte header of 8-hex-digit fields, then the NUL-terminated name,
 *  then the data. The header+name run is padded to 4 bytes and the data is
 *  padded to 4 bytes SEPARATELY. The stream ends at the TRAILER!!! entry. */
export function cpioEntries(bytes: Uint8Array): ArchiveEntry[] {
  const entries: ArchiveEntry[] = [];
  let off = 0;
  while (off + 110 <= bytes.length) {
    const magic = ascii(bytes.subarray(off, off + 6));
    if (magic !== '070701' && magic !== '070702') {
      throw new Error(
        `bundle-archive: cpio entry at offset ${off} has magic ${JSON.stringify(magic)}, ` +
          'expected 070701 or 070702',
      );
    }
    const field = (i: number): number =>
      parseInt(ascii(bytes.subarray(off + 6 + i * 8, off + 6 + i * 8 + 8)), 16);
    const mode = field(1);
    const fileSize = field(6);
    const nameSize = field(11);
    const nameStart = off + 110;
    const name = ascii(bytes.subarray(nameStart, nameStart + nameSize - 1)); // drop the NUL
    if (name === 'TRAILER!!!') break;
    const dataStart = nameStart + nameSize + ((4 - ((110 + nameSize) % 4)) % 4);
    // S_IFREG is 0o100000. Directories and symlinks carry no bytes worth
    // returning and would collide by name with the files under them.
    if ((mode & 0o170000) === 0o100000) {
      entries.push({
        name,
        size: fileSize,
        mode: mode & 0o7777,
        data: bytes.subarray(dataStart, dataStart + fileSize),
      });
    }
    off = dataStart + fileSize + ((4 - (fileSize % 4)) % 4);
  }
  return entries;
}

/** The compressed payload of an .rpm: everything after the 96-byte lead,
 *  the signature header (padded to an 8-byte boundary) and the header
 *  (not padded). Each header is 16 bytes of preamble, then nindex 16-byte
 *  index entries, then hsize bytes of store. */
export function rpmPayload(bytes: Uint8Array): Uint8Array {
  if (bytes[0] !== 0xed || bytes[1] !== 0xab || bytes[2] !== 0xee || bytes[3] !== 0xdb) {
    throw new Error('bundle-archive: not an rpm (the ed ab ee db lead magic is missing)');
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const headerEnd = (start: number): number => {
    if (bytes[start] !== 0x8e || bytes[start + 1] !== 0xad || bytes[start + 2] !== 0xe8) {
      throw new Error(`bundle-archive: rpm header at offset ${start} has no 8e ad e8 magic`);
    }
    const nindex = view.getUint32(start + 8, false);
    const hsize = view.getUint32(start + 12, false);
    return start + 16 + nindex * 16 + hsize;
  };
  const sigEnd = headerEnd(96);
  const hdrStart = sigEnd + ((8 - (sigEnd % 8)) % 8); // only the SIGNATURE header is padded
  return bytes.subarray(headerEnd(hdrStart));
}

export function debEntries(path: string): ArchiveEntry[] {
  const members = arMembers(readFileSync(path));
  const data = members.get('data.tar.gz');
  if (!data) {
    throw new Error(
      `bundle-archive: ${path} holds no data.tar.gz (it holds: ` +
        `${[...members.keys()].join(', ') || '<nothing>'})`,
    );
  }
  return tarEntries(Bun.gunzipSync(data));
}

export function rpmEntries(path: string): ArchiveEntry[] {
  // Observed 2026-09-14: tauri-bundler's rpm carries PAYLOADFORMAT cpio and
  // PAYLOADCOMPRESSOR gzip. If that ever becomes zstd or xz, gunzipSync
  // throws here and the message below is the first thing anyone reads.
  let cpio: Uint8Array;
  try {
    cpio = Bun.gunzipSync(rpmPayload(readFileSync(path)));
  } catch (err) {
    throw new Error(
      `bundle-archive: ${path}'s payload did not gunzip (${
        err instanceof Error ? err.message : String(err)
      }). tauri-bundler wrote a gzip cpio payload when this reader was written; ` +
        'a different PAYLOADCOMPRESSOR needs a new branch here.',
    );
  }
  return cpioEntries(cpio);
}

export function bundleEntries(path: string): ArchiveEntry[] {
  if (path.endsWith('.deb')) return debEntries(path);
  if (path.endsWith('.rpm')) return rpmEntries(path);
  throw new Error(
    `bundle-archive: ${path} is neither a .deb nor an .rpm. A .dmg needs hdiutil and ` +
      'an NSIS .exe needs 7z; tools/smoke-bundle.ts opens those, on the OS that has them.',
  );
}

/** One entry by path, tolerating the ./ prefix the two formats disagree
 *  about: a .deb's tar names it `usr/bin/screepub-engine` and an .rpm's
 *  cpio names it `./usr/bin/screepub-engine`. A bare `endsWith` would also
 *  answer `usr/bin/not-screepub-engine`, so the prefix is normalised on
 *  both sides rather than ignored. */
export function findEntry(entries: ArchiveEntry[], suffix: string): ArchiveEntry {
  const want = suffix.replace(/^\.?\//, '');
  const found = entries.find((e) => e.name.replace(/^\.?\//, '') === want);
  if (!found) {
    const held = entries.map((e) => e.name).join(', ') || '<nothing>';
    throw new Error(`bundle-archive: no ${suffix} inside the bundle (it holds: ${held})`);
  }
  return found;
}
```

- [ ] **Step 6: Run the tests**

Run: `bun test tests/bundle-archive.test.ts`
Expected: PASS, all fifteen.

- [ ] **Step 7: Prove the readers against the real bundles, by hand**

```bash
bun -e '
  const { bundleEntries, findEntry } = await import("./tools/bundle-archive.ts");
  for (const p of process.argv.slice(1)) {
    const entries = bundleEntries(p);
    console.log(p, entries.length, "files");
    console.log("  engine:", findEntry(entries, "usr/bin/screepub-engine").size);
    console.log("  licence:", findEntry(entries, "usr/lib/Screepub/LICENSE").size);
  }
' desktop/src-tauri/target/release/bundle/deb/Screepub_0.6.0_arm64.deb \
  desktop/src-tauri/target/release/bundle/rpm/Screepub-0.6.0-1.aarch64.rpm
```

Expected: both print the same engine size (102,153,058 bytes when measured on 2026-09-14; it drifts with the engine) and a non-zero licence size. If the bundles are not on disk, build them first with `cargo tauri build --bundles deb,rpm` from `desktop/src-tauri`, then `git checkout desktop/src-tauri/Cargo.toml`.

- [ ] **Step 8: Typecheck and commit**

```bash
bunx tsc --noEmit
bun test
```

```
Read a .deb and an .rpm without asking the machine for tools

The design sketched `ar x` and `rpm2cpio | cpio -id`. rpm2cpio is not
installed here and neither it nor bsdtar is guaranteed on GitHub's ubuntu
image, so a smoke check built on them would be a smoke check that skips.
Both formats are fixed ASCII headers around a gzip stream Bun already
decompresses; the readers are shorter than the apt-get that would replace
them and behave the same on all three runners.

tarGzEntries is split so tarEntries can walk bytes that never were a file:
the .deb's data.tar.gz member and the .rpm's payload both arrive that way.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 4: `tools/build-app-bundle.ts` — the matrix, the names, and what is checked

**Files:**
- Create: `tools/build-app-bundle.ts`
- Modify: `tools/build-cli.ts` (`writeChecksums` gains a filename parameter)
- Test: `tests/build-app-bundle.test.ts`

**Interfaces:**
- Consumes: `REPO_DIR`, `writeChecksums`, `sha256File`, `parseChecksums` from `tools/build-cli.ts`; `Spawn = (argv: string[], cwd: string) => { exitCode: number; stderr: string }` from the same file.
- Produces, from `tools/build-app-bundle.ts`:
  - `export type BundleOs = 'linux' | 'macos' | 'windows'`
  - `export type BundleArch = 'x64' | 'arm64'`
  - `export interface BundleKind { id: 'deb' | 'rpm' | 'dmg' | 'nsis'; os: BundleOs; dir: string; ext: string; magic: readonly number[]; magicAt: 'head' | 'udif-trailer'; floorBytes: number; releasedName(version: string, arch: BundleArch): string }`
  - `export const BUNDLE_KINDS: readonly BundleKind[]`
  - `export const DESKTOP_DIR: string` — absolute path of `desktop/src-tauri`
  - `export function kindsForOs(os: BundleOs): BundleKind[]`
  - `export function bundleListFor(os: BundleOs): string`
  - `export function buildArgv(os: BundleOs, opts: { target?: string; config?: string }): string[]`
  - `export function bundleDirFor(kind: BundleKind, target?: string): string`
  - `export function discoverArtifact(dir: string, kind: BundleKind): string`
  - `export function verifyBundleFile(path: string, kind: BundleKind): void`
  - `export function assertBundleVersions(version: string, repoDir?: string): void`
  - `export interface BundleArgs { version: string; outDir: string; os: BundleOs; arch: BundleArch; target?: string; config?: string }`
  - `export function parseBundleArgs(argv: string[], platform?: string, arch?: string): BundleArgs`
  - `export function buildBundles(args: BundleArgs, spawn?: Spawn): Promise<string[]>`
- Also produces from `tools/build-cli.ts`: `writeChecksums(outDir: string, archiveNames: string[], fileName?: string): string` — default `'SHA256SUMS'`, unchanged for every existing caller.
- Task 5 imports `BUNDLE_KINDS`, `BundleKind` and `kindsForOs`. Task 9 and Task 10 call the CLI entry point only.

**Why the artifact is discovered rather than reconstructed.** `tauri-bundler` names the DMG `{productName}_{version}_{aarch64|x64}.dmg` and the installer `{productName}_{version}_{arm64|x64}-setup.exe` — and Task 10's macOS overlay changes `productName` to `Screepub Desktop`, which puts a space in both. Reconstructing those names in TypeScript means two places that must agree about a third party's format string. Globbing the bundle directory for the one file with the right extension is the fact itself. It is only safe because it asserts **exactly one match**: `bundle_dmg` leaves a `rw.$$.<name>.dmg` behind when it crashes, so that prefix is excluded by name and the count check catches anything else.

- [ ] **Step 1: Give `writeChecksums` a filename**

In `tools/build-cli.ts`, change the signature and the one `join` inside it. Nothing else in the function body changes — the merge-and-re-hash behaviour its comment describes is exactly what `SHA256SUMS-app` wants too.

```ts
export function writeChecksums(
  outDir: string,
  archiveNames: string[],
  fileName: string = 'SHA256SUMS',
): string {
  const path = join(outDir, fileName);
```

- [ ] **Step 2: Run the existing checksum tests**

Run: `bun test tests/build-cli.test.ts`
Expected: PASS unchanged — every existing call passes two arguments and gets the default.

- [ ] **Step 3: Write the failing tests**

Create `tests/build-app-bundle.test.ts`:

```ts
import { describe, test, expect, afterAll } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  BUNDLE_KINDS,
  assertBundleVersions,
  buildArgv,
  buildBundles,
  bundleDirFor,
  bundleListFor,
  discoverArtifact,
  kindsForOs,
  parseBundleArgs,
  verifyBundleFile,
  type BundleKind,
} from '../tools/build-app-bundle';
import { parseChecksums } from '../tools/build-cli';

const OUT = mkdtempSync(join(tmpdir(), 'screepub-bundle-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const kind = (id: string): BundleKind => BUNDLE_KINDS.find((k) => k.id === id)!;

/** A file that passes `kind`'s magic check and clears its floor, without
 *  writing forty megabytes. */
function plausible(path: string, k: BundleKind): void {
  const bytes = new Uint8Array(k.floorBytes + 1024);
  if (k.magicAt === 'head') bytes.set(k.magic, 0);
  else bytes.set(k.magic, bytes.length - 512);
  writeFileSync(path, bytes);
}

describe('the bundle matrix', () => {
  test('it is exactly the four kinds the design shipped, and no AppImage', () => {
    expect(BUNDLE_KINDS.map((k) => k.id).sort()).toEqual(['deb', 'dmg', 'nsis', 'rpm']);
    // AppImage packaging rewrites the Bun-compiled engine's dynamic section
    // with linuxdeploy's own patchelf and the extracted engine segfaults.
    // Measured; not a preference. An added row here would ship that.
    expect(BUNDLE_KINDS.some((k) => k.id.includes('appimage'))).toBe(false);
    // MSI too: NSIS installs per-user with no administrator prompt, and two
    // Windows installers doubles a surface nobody has opened once.
    expect(BUNDLE_KINDS.some((k) => k.ext === '.msi')).toBe(false);
  });

  test('every OS gets the kinds the design assigned it, and only those', () => {
    expect(kindsForOs('linux').map((k) => k.id)).toEqual(['deb', 'rpm']);
    expect(kindsForOs('macos').map((k) => k.id)).toEqual(['dmg']);
    expect(kindsForOs('windows').map((k) => k.id)).toEqual(['nsis']);
  });

  test('the --bundles list is pinned per OS and never left to the default', () => {
    // The default bundle.targets on Linux is deb, rpm AND appimage, so a
    // bare `cargo tauri build` attempts the AppImage and fails the whole
    // run. This is the assertion that keeps the list explicit.
    expect(bundleListFor('linux')).toBe('deb,rpm');
    expect(bundleListFor('macos')).toBe('app,dmg');
    expect(bundleListFor('windows')).toBe('nsis');
    for (const os of ['linux', 'macos', 'windows'] as const) {
      expect(bundleListFor(os)).not.toContain('appimage');
    }
  });

  test('the published names carry the version and say which machine they are for', () => {
    expect(kind('deb').releasedName('0.6.0', 'x64')).toBe('Screepub_0.6.0_amd64.deb');
    expect(kind('deb').releasedName('0.6.0', 'arm64')).toBe('Screepub_0.6.0_arm64.deb');
    expect(kind('rpm').releasedName('0.6.0', 'x64')).toBe('Screepub-0.6.0-1.x86_64.rpm');
    expect(kind('rpm').releasedName('0.6.0', 'arm64')).toBe('Screepub-0.6.0-1.aarch64.rpm');
    expect(kind('nsis').releasedName('0.6.0', 'x64')).toBe('Screepub-0.6.0-setup.exe');
    // The two Mac names must not collide with the SwiftUI app's
    // Screepub-macOS.dmg, which app/release.sh uploads to the same release
    // page and tools/bump-tap.sh hardcodes.
    expect(kind('dmg').releasedName('0.6.0', 'arm64')).toBe('Screepub-Desktop-macOS-arm64.dmg');
    expect(kind('dmg').releasedName('0.6.0', 'x64')).toBe('Screepub-Desktop-macOS-x64.dmg');
    for (const arch of ['x64', 'arm64'] as const) {
      expect(kind('dmg').releasedName('0.6.0', arch)).not.toBe('Screepub-macOS.dmg');
    }
  });

  test('no two rows can ever produce the same published filename', () => {
    const names = BUNDLE_KINDS.flatMap((k) =>
      (['x64', 'arm64'] as const).map((a) => k.releasedName('0.6.0', a)),
    );
    // nsis ignores the arch (only x86-64 ships), so it contributes one name
    // twice; everything else must be distinct.
    expect(new Set(names).size).toBe(names.length - 1);
  });
});

describe('the cargo tauri invocation', () => {
  test('it is `build`, never `bundle`', () => {
    // `cargo tauri bundle` does not build: it fails with "can't open main
    // binary .../target/release/screepub-desktop". Measured.
    const argv = buildArgv('linux', {});
    expect(argv.slice(0, 3)).toEqual(['cargo', 'tauri', 'build']);
    expect(argv).not.toContain('bundle');
  });

  test('it always pins the bundle list', () => {
    expect(buildArgv('linux', {})).toEqual(['cargo', 'tauri', 'build', '--bundles', 'deb,rpm']);
  });

  test('a target triple and a config overlay are passed through when given', () => {
    expect(buildArgv('macos', { target: 'x86_64-apple-darwin', config: 'tauri.transition.conf.json' }))
      .toEqual([
        'cargo', 'tauri', 'build',
        '--bundles', 'app,dmg',
        '--target', 'x86_64-apple-darwin',
        '--config', 'tauri.transition.conf.json',
      ]);
  });

  test('the bundle directory moves under the triple when one is given', () => {
    // `cargo tauri build --target X` writes to target/X/release/bundle/...,
    // not target/release/bundle/... Looking in the wrong one is how a build
    // "succeeds" and produces nothing.
    expect(bundleDirFor(kind('deb'))).toMatch(/target[/\\]release[/\\]bundle[/\\]deb$/);
    expect(bundleDirFor(kind('dmg'), 'aarch64-apple-darwin')).toMatch(
      /target[/\\]aarch64-apple-darwin[/\\]release[/\\]bundle[/\\]dmg$/,
    );
  });
});

describe('finding the file the bundler wrote', () => {
  test('it returns the one artifact with the right extension', () => {
    const dir = join(OUT, 'find-one');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'Screepub Desktop_0.6.0_aarch64.dmg'), 'x');
    expect(discoverArtifact(dir, kind('dmg'))).toBe(
      join(dir, 'Screepub Desktop_0.6.0_aarch64.dmg'),
    );
  });

  test('it ignores the temporary image bundle_dmg leaves behind on a crash', () => {
    // bundle_dmg writes rw.$$.<name>.dmg beside the real one and removes it
    // on success. After a crashed run both are there; without this filter
    // the count check below fires on a directory that has exactly one real
    // artifact, and the build fails for the wrong reason.
    const dir = join(OUT, 'find-rw');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'rw.4711.Screepub Desktop_0.6.0_x64.dmg'), 'x');
    writeFileSync(join(dir, 'Screepub Desktop_0.6.0_x64.dmg'), 'x');
    expect(discoverArtifact(dir, kind('dmg'))).toBe(join(dir, 'Screepub Desktop_0.6.0_x64.dmg'));
  });

  test('two candidates is an error that names both', () => {
    // A stale artifact from a previous version is the realistic case, and
    // picking "the newest" silently would publish the wrong bytes.
    const dir = join(OUT, 'find-two');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'Screepub_0.5.4_arm64.deb'), 'x');
    writeFileSync(join(dir, 'Screepub_0.6.0_arm64.deb'), 'x');
    expect(() => discoverArtifact(dir, kind('deb'))).toThrow(/0\.5\.4.*0\.6\.0|0\.6\.0.*0\.5\.4/s);
  });

  test('no candidate names the directory it looked in', () => {
    const dir = join(OUT, 'find-none');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'notes.txt'), 'x');
    expect(() => discoverArtifact(dir, kind('deb'))).toThrow(/find-none/);
  });

  test('a missing directory is a clear message, not an ENOENT stack', () => {
    expect(() => discoverArtifact(join(OUT, 'never-made'), kind('rpm'))).toThrow(/never-made/);
  });
});

describe('verifying what came out', () => {
  test('a plausible artifact of every kind passes', () => {
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `good${k.ext}`);
      plausible(path, k);
      expect(() => verifyBundleFile(path, k)).not.toThrow();
    }
  });

  test('an undersized artifact is rejected for every kind', () => {
    // Each bundle carries a ~102 MB engine. Anything at a few kilobytes is
    // a truncated write, and an exit code of 0 will not say so.
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `small${k.ext}`);
      const bytes = new Uint8Array(2048);
      if (k.magicAt === 'head') bytes.set(k.magic, 0);
      else bytes.set(k.magic, bytes.length - 512);
      writeFileSync(path, bytes);
      expect(() => verifyBundleFile(path, k)).toThrow(/floor/);
    }
  });

  test('the wrong container is rejected for every kind', () => {
    // The realistic failure is a renamed file: the build produced an .rpm
    // and something copied it to the .deb's published name.
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `wrong${k.ext}`);
      writeFileSync(path, new Uint8Array(k.floorBytes + 1024)); // all zeroes
      expect(() => verifyBundleFile(path, k)).toThrow(/magic|container/i);
    }
  });

  test('the DMG check reads the trailer and not the head', () => {
    // A UDIF image has no head magic at all -- its 512-byte koly trailer is
    // at the END. A head-only check would accept any large file as a DMG,
    // which is exactly the vacuous assertion this test exists to forbid.
    const k = kind('dmg');
    expect(k.magicAt).toBe('udif-trailer');
    const path = join(OUT, 'headmagic.dmg');
    const bytes = new Uint8Array(k.floorBytes + 1024);
    bytes.set(k.magic, 0); // koly at the FRONT, where it means nothing
    writeFileSync(path, bytes);
    expect(() => verifyBundleFile(path, k)).toThrow(/magic|container/i);
  });

  test('a missing file says which one', () => {
    expect(() => verifyBundleFile(join(OUT, 'absent.deb'), kind('deb'))).toThrow(/absent\.deb/);
  });
});

describe('the version gate', () => {
  const fakeRepo = (pkg: string, cargo: string, conf: string): string => {
    const dir = mkdtempSync(join(OUT, 'repo-'));
    mkdirSync(join(dir, 'desktop', 'src-tauri'), { recursive: true });
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ version: pkg }));
    writeFileSync(
      join(dir, 'desktop', 'src-tauri', 'Cargo.toml'),
      `[package]\nname = "screepub-desktop"\nversion = "${cargo}"\nedition = "2021"\n`,
    );
    writeFileSync(
      join(dir, 'desktop', 'src-tauri', 'tauri.conf.json'),
      JSON.stringify({ productName: 'Screepub', version: conf }),
    );
    return dir;
  };

  test('all three agreeing with the version passes', () => {
    expect(() => assertBundleVersions('0.6.0', fakeRepo('0.6.0', '0.6.0', '0.6.0'))).not.toThrow();
  });

  test('a 0.6.0 bundle around a 0.5.4 engine is refused, and says which file', () => {
    // This is the defect the design caught inside a real artifact: the
    // bundle was named 0.6.0, the crate was 0.6.0, and the engine inside it
    // answered {"ok":true,"version":"0.5.4"}. package.json is what the
    // engine reports, so this is the file that must be named.
    expect(() => assertBundleVersions('0.6.0', fakeRepo('0.5.4', '0.6.0', '0.6.0'))).toThrow(
      /package\.json/,
    );
  });

  test('a stale Cargo.toml is refused and named', () => {
    expect(() => assertBundleVersions('0.6.0', fakeRepo('0.6.0', '0.5.4', '0.6.0'))).toThrow(
      /Cargo\.toml/,
    );
  });

  test('a stale tauri.conf.json is refused and named', () => {
    // This one names the BUNDLE FILE and the deb Version: field, so a
    // mismatch here ships an installer whose filename lies.
    expect(() => assertBundleVersions('0.6.0', fakeRepo('0.6.0', '0.6.0', '0.5.4'))).toThrow(
      /tauri\.conf\.json/,
    );
  });

  test('the message carries both numbers, not just a complaint', () => {
    let message = '';
    try {
      assertBundleVersions('0.6.0', fakeRepo('0.5.4', '0.6.0', '0.6.0'));
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toContain('0.5.4');
    expect(message).toContain('0.6.0');
  });
});

describe('the argument parser', () => {
  test('it derives the OS and architecture from the host by default', () => {
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'linux', 'arm64')).toMatchObject({
      version: '0.6.0',
      os: 'linux',
      arch: 'arm64',
    });
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'win32', 'x64')).toMatchObject({
      os: 'windows',
      arch: 'x64',
    });
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'darwin', 'arm64')).toMatchObject({
      os: 'macos',
      arch: 'arm64',
    });
  });

  test('it strips a leading v so a tag can be passed straight through', () => {
    expect(parseBundleArgs(['--version', 'v0.6.0', '--out', OUT], 'linux', 'x64').version).toBe(
      '0.6.0',
    );
  });

  test('it refuses a branch name where a version belongs', () => {
    // release.yml can be dispatched against a ref; a non-tag ref must not
    // reach a bundle filename.
    expect(() => parseBundleArgs(['--version', 'main', '--out', OUT], 'linux', 'x64')).toThrow(
      /MAJOR\.MINOR\.PATCH/,
    );
  });

  test('it accepts a prerelease suffix, which release.yml already supports', () => {
    expect(parseBundleArgs(['--version', 'v0.6.0-rc1', '--out', OUT], 'linux', 'x64').version).toBe(
      '0.6.0-rc1',
    );
  });

  test('--out is required, because a bundle run writes ~90 MB', () => {
    expect(() => parseBundleArgs(['--version', '0.6.0'], 'linux', 'x64')).toThrow(/--out/);
  });

  test('it refuses a host it has no bundles for', () => {
    expect(() => parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'freebsd', 'x64')).toThrow(
      /freebsd/,
    );
  });

  test('--out is made absolute so the tool can be run from anywhere', () => {
    const args = parseBundleArgs(['--version', '0.6.0', '--out', 'dist'], 'linux', 'x64');
    expect(args.outDir.startsWith('/') || /^[A-Za-z]:/.test(args.outDir)).toBe(true);
  });
});

describe('a whole run, against a fake cargo', () => {
  /** Stands in for `cargo tauri build`: records the argv, then writes the
   *  artifacts a real bundler would have written, under the bundler's own
   *  naming rather than the published one. */
  const fakeCargo = (dirs: BundleKind[]) => {
    const calls: string[][] = [];
    const spawn = (argv: string[], cwd: string) => {
      calls.push(argv);
      for (const k of dirs) {
        const dir = bundleDirFor(k, argv.includes('--target') ? argv[argv.indexOf('--target') + 1] : undefined);
        mkdirSync(dir, { recursive: true });
        plausible(join(dir, `Screepub_0.6.0_bundler-name${k.ext}`), k);
      }
      return { exitCode: 0, stderr: '' };
    };
    return { calls, spawn };
  };

  test('it renames to the published names and writes SHA256SUMS-app', async () => {
    const { calls, spawn } = fakeCargo(kindsForOs('linux'));
    const out = join(OUT, 'run-linux');
    const made = await buildBundles(
      { version: '0.6.0', outDir: out, os: 'linux', arch: 'arm64' },
      spawn,
    );
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual([
      'Screepub_0.6.0_arm64.deb',
      'Screepub-0.6.0-1.aarch64.rpm',
    ]);
    // Exactly one cargo invocation: deb and rpm come out of a single build.
    expect(calls.length).toBe(1);
    expect(calls[0]).toEqual(['cargo', 'tauri', 'build', '--bundles', 'deb,rpm']);

    const sums = parseChecksums(readFileSync(join(out, 'SHA256SUMS-app'), 'utf8'));
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-0.6.0-1.aarch64.rpm',
      'Screepub_0.6.0_arm64.deb',
    ]);
    // The digests describe the files that are actually there, not the ones
    // the bundler wrote before the rename.
    for (const [name, digest] of sums) {
      expect(digest).toBe(
        new Bun.CryptoHasher('sha256').update(readFileSync(join(out, name))).digest('hex'),
      );
    }
  });

  test('the checksums file is NOT called SHA256SUMS', () => {
    // E1's cross-upload job publishes SHA256SUMS for the three CLI
    // archives. One file overwriting the other on the release page is how a
    // download silently stops being checkable.
    expect(BUNDLE_KINDS.length).toBeGreaterThan(0);
    const { spawn } = fakeCargo(kindsForOs('linux'));
    const out = join(OUT, 'run-names');
    return buildBundles({ version: '0.6.0', outDir: out, os: 'linux', arch: 'x64' }, spawn).then(
      () => {
        expect(() => readFileSync(join(out, 'SHA256SUMS-app'))).not.toThrow();
        expect(() => readFileSync(join(out, 'SHA256SUMS'))).toThrow();
      },
    );
  });

  test('a failing cargo fails the run and forwards its stderr', async () => {
    const spawn = () => ({ exitCode: 101, stderr: 'error: linker `cc` not found' });
    await expect(
      buildBundles({ version: '0.6.0', outDir: join(OUT, 'run-fail'), os: 'linux', arch: 'x64' }, spawn),
    ).rejects.toThrow(/linker/);
  });

  test('a cargo that exits 0 and writes nothing still fails', async () => {
    // The failure this whole tool exists for: a green exit code and an
    // empty bundle directory.
    const spawn = () => ({ exitCode: 0, stderr: '' });
    await expect(
      buildBundles({ version: '0.6.0', outDir: join(OUT, 'run-empty'), os: 'linux', arch: 'x64' }, spawn),
    ).rejects.toThrow(/deb/);
  });

  test('the macOS run passes the target triple and the overlay through', async () => {
    const { calls, spawn } = fakeCargo(kindsForOs('macos'));
    await buildBundles(
      {
        version: '0.6.0',
        outDir: join(OUT, 'run-macos'),
        os: 'macos',
        arch: 'x64',
        target: 'x86_64-apple-darwin',
        config: 'tauri.transition.conf.json',
      },
      spawn,
    );
    expect(calls[0]).toContain('--target');
    expect(calls[0]).toContain('x86_64-apple-darwin');
    expect(calls[0]).toContain('--config');
    expect(calls[0]).toContain('tauri.transition.conf.json');
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `bun test tests/build-app-bundle.test.ts`
Expected: FAIL — `Cannot find module '../tools/build-app-bundle'`.

- [ ] **Step 5: Write `tools/build-app-bundle.ts`**

```ts
// Build, name and VERIFY the installable app bundles.
//
//   bun tools/build-app-bundle.ts --version 0.6.0 --out dist/
//   bun tools/build-app-bundle.ts --version 0.6.0 --out dist/ \
//     --target aarch64-apple-darwin --config tauri.transition.conf.json
//
// A Bun script and not workflow YAML, following tools/build-cli.ts exactly:
// YAML can only be tested by cutting a release, and a release tool nobody
// can run locally is a release tool nobody can debug. It is also how the
// Linux arm64 .deb gets made by hand if no arm64 runner is available.
//
// It never signs anything. On macOS, tauri-bundler signs and notarizes from
// the APPLE_* environment variables the release workflow sets; there is no
// codesign call in this file and app/release.sh is neither read nor touched.

import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, copyFileSync } from 'node:fs';
import { join, isAbsolute, resolve, basename } from 'node:path';
import { parseArgs } from 'node:util';
import { REPO_DIR, writeChecksums, type Spawn } from './build-cli';

export type BundleOs = 'linux' | 'macos' | 'windows';
export type BundleArch = 'x64' | 'arm64';

export interface BundleKind {
  id: 'deb' | 'rpm' | 'dmg' | 'nsis';
  os: BundleOs;
  /** The directory under `target/[<triple>/]release/bundle/`. */
  dir: string;
  ext: string;
  /** Container magic, checked rather than assumed. */
  magic: readonly number[];
  /** Where that magic sits. A UDIF image's `koly` block is the LAST 512
   *  bytes of the file; a DMG has no header magic at all. */
  magicAt: 'head' | 'udif-trailer';
  floorBytes: number;
  /** The stable published filename. Deliberately ours, not the bundler's:
   *  tauri names the DMG after productName, which the macOS transition
   *  overlay changes to "Screepub Desktop" (with a space). */
  releasedName(version: string, arch: BundleArch): string;
}

/** Every bundle carries the ~102 MB engine, so 40 MB compressed is the
 *  right order of magnitude and 10 MB is an unambiguous floor. Same
 *  reasoning as build-cli.ts's RELEASE_FLOORS. */
const FLOOR = 10_000_000;

const ARCH_MAGIC = [0x21, 0x3c, 0x61, 0x72, 0x63, 0x68, 0x3e, 0x0a]; // "!<arch>\n"
const RPM_MAGIC = [0xed, 0xab, 0xee, 0xdb];
const MZ_MAGIC = [0x4d, 0x5a]; // "MZ"
const KOLY_MAGIC = [0x6b, 0x6f, 0x6c, 0x79]; // "koly", the UDIF trailer

export const BUNDLE_KINDS: readonly BundleKind[] = [
  {
    id: 'deb',
    os: 'linux',
    dir: 'deb',
    ext: '.deb',
    magic: ARCH_MAGIC,
    magicAt: 'head',
    floorBytes: FLOOR,
    releasedName: (v, arch) => `Screepub_${v}_${arch === 'x64' ? 'amd64' : 'arm64'}.deb`,
  },
  {
    id: 'rpm',
    os: 'linux',
    dir: 'rpm',
    ext: '.rpm',
    magic: RPM_MAGIC,
    magicAt: 'head',
    floorBytes: FLOOR,
    releasedName: (v, arch) => `Screepub-${v}-1.${arch === 'x64' ? 'x86_64' : 'aarch64'}.rpm`,
  },
  {
    // "Desktop" in the name, and NOT Screepub-macOS.dmg: app/release.sh
    // uploads that one to the same release page and tools/bump-tap.sh
    // hardcodes it. Two Mac downloads is confusing enough without a clash.
    id: 'dmg',
    os: 'macos',
    dir: 'dmg',
    ext: '.dmg',
    magic: KOLY_MAGIC,
    magicAt: 'udif-trailer',
    floorBytes: FLOOR,
    releasedName: (_v, arch) => `Screepub-Desktop-macOS-${arch}.dmg`,
  },
  {
    // Only x86-64 ships, so the arch is not in the name; an arm64 Windows
    // build would need its own row here rather than a silent overwrite.
    id: 'nsis',
    os: 'windows',
    dir: 'nsis',
    ext: '.exe',
    magic: MZ_MAGIC,
    magicAt: 'head',
    floorBytes: FLOOR,
    releasedName: (v) => `Screepub-${v}-setup.exe`,
  },
];

export const DESKTOP_DIR = join(REPO_DIR, 'desktop', 'src-tauri');

export function kindsForOs(os: BundleOs): BundleKind[] {
  return BUNDLE_KINDS.filter((k) => k.os === os);
}

/** The `--bundles` value, pinned per OS and NEVER the default: on Linux the
 *  default is deb, rpm and appimage, so a bare `cargo tauri build` attempts
 *  an AppImage whose linuxdeploy pass corrupts the Bun-compiled engine, and
 *  fails the whole run. `app,dmg` rather than `dmg` alone is redundant --
 *  the dmg step builds the .app itself -- but it says what is produced. */
export function bundleListFor(os: BundleOs): string {
  if (os === 'linux') return 'deb,rpm';
  if (os === 'macos') return 'app,dmg';
  return 'nsis';
}

export function buildArgv(os: BundleOs, opts: { target?: string; config?: string }): string[] {
  const argv = ['cargo', 'tauri', 'build', '--bundles', bundleListFor(os)];
  if (opts.target) argv.push('--target', opts.target);
  if (opts.config) argv.push('--config', opts.config);
  return argv;
}

export function bundleDirFor(kind: BundleKind, target?: string): string {
  return target
    ? join(DESKTOP_DIR, 'target', target, 'release', 'bundle', kind.dir)
    : join(DESKTOP_DIR, 'target', 'release', 'bundle', kind.dir);
}

/** The one file the bundler wrote, found rather than reconstructed.
 *
 *  Reconstructing tauri's own filename would mean two places agreeing about
 *  a third party's format string -- and productName, which is half of it,
 *  is exactly what the macOS transition overlay changes. So: glob for the
 *  extension and insist on EXACTLY ONE match, which is what turns a glob
 *  into a fact. `rw.` is excluded by name because bundle_dmg leaves
 *  `rw.$$.<name>.dmg` behind when it dies partway. */
export function discoverArtifact(dir: string, kind: BundleKind): string {
  if (!existsSync(dir)) {
    throw new Error(
      `build-app-bundle: ${kind.id}: no bundle directory at ${dir}. cargo tauri build ` +
        'reported success and produced nothing there.',
    );
  }
  const found = readdirSync(dir).filter((n) => n.endsWith(kind.ext) && !n.startsWith('rw.'));
  if (found.length !== 1) {
    throw new Error(
      `build-app-bundle: ${kind.id}: expected exactly one ${kind.ext} in ${dir}, found ` +
        `${found.length}${found.length ? ` (${found.join(', ')})` : ''}. A leftover from an ` +
        'earlier version is the usual cause; clear the directory and rebuild.',
    );
  }
  return join(dir, found[0]!);
}

function headBytes(path: string, n: number, fromEnd = false): Uint8Array {
  const bytes = readFileSync(path);
  return fromEnd ? bytes.subarray(bytes.length - n) : bytes.subarray(0, n);
}

/** Check what was PRODUCED. `cargo tauri build` can exit 0 and leave a
 *  truncated image, and a copy step can put the wrong container behind the
 *  right name. */
export function verifyBundleFile(path: string, kind: BundleKind): void {
  if (!existsSync(path)) {
    throw new Error(`build-app-bundle: ${kind.id}: nothing at ${path}`);
  }
  const bytes = statSync(path).size;
  if (bytes < kind.floorBytes) {
    throw new Error(
      `build-app-bundle: ${kind.id}: ${basename(path)} is ${bytes} bytes, under the ` +
        `${kind.floorBytes}-byte floor. Every bundle carries the ~102 MB engine.`,
    );
  }
  const window =
    kind.magicAt === 'udif-trailer'
      ? headBytes(path, 512, true)
      : headBytes(path, kind.magic.length);
  const ok = kind.magic.every((b, i) => window[i] === b);
  if (!ok) {
    const got = [...window.subarray(0, kind.magic.length)]
      .map((b) => b.toString(16).padStart(2, '0'))
      .join(' ');
    throw new Error(
      `build-app-bundle: ${kind.id}: ${basename(path)} does not carry the ${kind.id} container ` +
        `magic at the ${kind.magicAt === 'head' ? 'start' : 'UDIF trailer'} (saw ${got}). ` +
        'A renamed file of another format is the usual cause.',
    );
  }
}

const VERSION_RE = /^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$/;

/** package.json is what the ENGINE reports, Cargo.toml is the crate, and
 *  tauri.conf.json names the bundle file, the Info.plist, the deb Version:
 *  and the NSIS product version. A release in which they disagree ships an
 *  installer whose About line contradicts its own filename -- which is
 *  exactly what a real 0.6.0 bundle around a 0.5.4 engine did on
 *  2026-09-14. This is a RELEASE-tool gate, not a `bun test` assertion: the
 *  split is a normal working-branch state and a test that failed on every
 *  branch would be deleted within a week. */
export function assertBundleVersions(version: string, repoDir: string = REPO_DIR): void {
  const pkg = (
    JSON.parse(readFileSync(join(repoDir, 'package.json'), 'utf8')) as { version?: string }
  ).version;
  const cargoText = readFileSync(join(repoDir, 'desktop', 'src-tauri', 'Cargo.toml'), 'utf8');
  // The FIRST version key under [package]; a dependency's version = "2"
  // must never be mistaken for the crate's.
  const cargo = /^\s*\[package\][\s\S]*?^\s*version\s*=\s*"([^"]+)"/m.exec(cargoText)?.[1];
  const conf = (
    JSON.parse(
      readFileSync(join(repoDir, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8'),
    ) as { version?: string }
  ).version;

  for (const [file, found] of [
    ['package.json', pkg],
    ['desktop/src-tauri/Cargo.toml', cargo],
    ['desktop/src-tauri/tauri.conf.json', conf],
  ] as const) {
    if (found !== version) {
      throw new Error(
        `build-app-bundle: ${file} says ${found ?? '<nothing>'} but the version being built is ` +
          `${version}. All three must agree at a release, or the bundle's filename and the ` +
          'engine inside it tell a user two different things.',
      );
    }
  }
}

export interface BundleArgs {
  version: string;
  outDir: string;
  os: BundleOs;
  arch: BundleArch;
  target?: string;
  config?: string;
}

export function osForPlatform(platform: string): BundleOs {
  if (platform === 'linux') return 'linux';
  if (platform === 'darwin') return 'macos';
  if (platform === 'win32') return 'windows';
  throw new Error(`build-app-bundle: no app bundle is defined for platform "${platform}"`);
}

export function parseBundleArgs(
  argv: string[],
  platform: string = process.platform,
  arch: string = process.arch,
): BundleArgs {
  const { values } = parseArgs({
    args: argv,
    options: {
      version: { type: 'string' },
      out: { type: 'string' },
      target: { type: 'string' },
      config: { type: 'string' },
    },
    strict: true,
    allowPositionals: false,
  });

  const version = (values.version ?? '').replace(/^v/, '');
  if (!VERSION_RE.test(version)) {
    throw new Error(
      `build-app-bundle: --version must be MAJOR.MINOR.PATCH (got ${
        values.version ?? '<missing>'
      }). release.yml can be dispatched against a branch, and a branch name must never ` +
        'reach a bundle filename.',
    );
  }
  if (!values.out) {
    throw new Error(
      'build-app-bundle: --out <dir> is required. There is no default: each bundle is ' +
        '~43 MB and a Linux run writes two of them plus a checksums file.',
    );
  }
  if (arch !== 'x64' && arch !== 'arm64') {
    throw new Error(`build-app-bundle: no app bundle is defined for architecture "${arch}"`);
  }

  return {
    version,
    outDir: isAbsolute(values.out) ? values.out : resolve(values.out),
    os: osForPlatform(platform),
    arch,
    target: values.target,
    config: values.config,
  };
}

const realSpawn: Spawn = (argv, cwd) => {
  const proc = Bun.spawnSync(argv, { cwd, stdout: 'inherit', stderr: 'pipe' });
  return { exitCode: proc.exitCode ?? 1, stderr: proc.stderr.toString() };
};

/** One cargo run per OS, then discover, verify, rename and checksum.
 *
 *  Verifying BEFORE the rename means a broken artifact fails naming the
 *  bundler's own file, which is the one a person can go and look at. */
export async function buildBundles(args: BundleArgs, spawn: Spawn = realSpawn): Promise<string[]> {
  assertBundleVersions(args.version);
  mkdirSync(args.outDir, { recursive: true });

  const argv = buildArgv(args.os, { target: args.target, config: args.config });
  console.log(`── ${argv.join(' ')}`);
  const { exitCode, stderr } = spawn(argv, DESKTOP_DIR);
  if (exitCode !== 0) {
    throw new Error(
      `build-app-bundle: cargo tauri build failed (exit ${exitCode})\n${stderr.trim()}`,
    );
  }

  const names: string[] = [];
  for (const kind of kindsForOs(args.os)) {
    const built = discoverArtifact(bundleDirFor(kind, args.target), kind);
    verifyBundleFile(built, kind);
    const name = kind.releasedName(args.version, args.arch);
    const dest = join(args.outDir, name);
    copyFileSync(built, dest);
    // Verified again AFTER the copy: a full disk truncates silently.
    verifyBundleFile(dest, kind);
    console.log(`   ${basename(built)} → ${name}  ${statSync(dest).size} bytes  ok`);
    names.push(name);
  }

  // NOT "SHA256SUMS": release.yml's cross-upload job already publishes a
  // file by that name for the three CLI archives, onto the same release.
  writeChecksums(args.outDir, names, 'SHA256SUMS-app');
  return names.map((n) => join(args.outDir, n));
}

if (import.meta.main) {
  try {
    const args = parseBundleArgs(Bun.argv.slice(2));
    const made = await buildBundles(args);
    console.log(`\n${made.length} bundle(s) + SHA256SUMS-app in ${args.outDir}`);
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }
}
```

- [ ] **Step 6: Run the tests**

Run: `bun test tests/build-app-bundle.test.ts`
Expected: PASS.

- [ ] **Step 7: Typecheck, run the whole suite, commit**

```bash
bunx tsc --noEmit
bun test
```

```
Build the app bundles with a tool a person can run

tools/build-app-bundle.ts pins the bundle list per OS (the Linux default
includes appimage, which fails the whole run), finds what the bundler wrote
instead of reconstructing its filename, checks the container magic -- the
DMG's at the UDIF trailer, where it actually lives -- renames to stable
published names and writes SHA256SUMS-app, which is deliberately not the
SHA256SUMS the CLI artifacts already publish to the same release page.

The version gate refuses to build a 0.6.0 bundle around a 0.5.4 engine. It
lives here and not in bun test because the split is a normal working-branch
state; a test that failed on every branch would be deleted.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 5: `tools/smoke-bundle.ts` — open the bundle, run the engine out of it

**Files:**
- Create: `tools/smoke-bundle.ts`
- Test: `tests/smoke-bundle.test.ts`

**Interfaces:**
- Consumes: `bundleEntries`, `findEntry` from `tools/bundle-archive.ts`; `RunResult`, `Runner`, `soleJson`, `checkConvertResult` from `tools/smoke-cli.ts`; `BundleKind`, `BUNDLE_KINDS` from `tools/build-app-bundle.ts`.
- Produces, from `tools/smoke-bundle.ts`:
  - `export function checkEngineVersion(result: RunResult, expected: string): void`
  - `export function extractArchiveEngine(bundlePath: string, workDir: string): string` — `.deb`/`.rpm`, pure TypeScript
  - `export function extractDmgEngine(bundlePath: string, workDir: string, run: Runner): { enginePath: string; detach: () => void }`
  - `export function extractExeEngine(bundlePath: string, workDir: string, run: Runner): string`
  - `export function smokeBundle(bundlePath: string, fixture: string, workDir: string, expectedVersion: string, run?: Runner): void`
- Task 8 and Task 9 call the CLI entry point: `bun tools/smoke-bundle.ts --bundle <path> --expect-version <v>`.

**Why `--expect-version` is a required argument and not read from `package.json`.** `smoke-cli.ts` reads `package.json` because the CLI binary *is* built from it. A bundle's engine is also built from `package.json`, but the bundle's *name* comes from `tauri.conf.json`, and the whole point of this check is to catch the two disagreeing. Taking the expected version as an argument lets CI pass the tag — the only number a user ever sees — and lets a local run pass whatever the branch is actually at.

**Why not "launch the app and look at it".** No CI runner has a display, the GUI half cannot be exercised anywhere, and the part that silently breaks during bundling is the sidecar — which is exactly what AppImage broke, and exactly what this catches.

- [ ] **Step 1: Write the failing tests**

Create `tests/smoke-bundle.test.ts`:

```ts
import { describe, test, expect, afterAll } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  checkEngineVersion,
  extractArchiveEngine,
  extractDmgEngine,
  extractExeEngine,
  smokeBundle,
} from '../tools/smoke-bundle';
import type { RunResult } from '../tools/smoke-cli';

const OUT = mkdtempSync(join(tmpdir(), 'screepub-smokebundle-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const ok = (stdout: string): RunResult => ({ exitCode: 0, stdout, stderr: '' });
const enc = (s: string) => new TextEncoder().encode(s);

function concat(parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

/** The same hand-built containers Task 3's tests use, reduced to what this
 *  file needs: one .deb holding one executable file. */
function fakeDeb(path: string, contents: string): void {
  const header = new Uint8Array(512);
  header.set(enc('usr/bin/screepub-engine'), 0);
  header.set(enc('0000755\0'), 100);
  header.set(enc(contents.length.toString(8).padStart(11, '0') + '\0'), 124);
  header[156] = 0x30;
  header.set(enc('        '), 148);
  const body = enc(contents);
  const tar = concat([
    header,
    body,
    new Uint8Array((512 - (body.length % 512)) % 512),
    new Uint8Array(1024),
  ]);
  const member = (name: string, data: Uint8Array): Uint8Array => {
    const h = new Uint8Array(60).fill(0x20);
    h.set(enc(name), 0);
    h.set(enc(String(data.length)), 48);
    h[58] = 0x60;
    h[59] = 0x0a;
    return concat([h, data, data.length % 2 ? new Uint8Array([0x0a]) : new Uint8Array(0)]);
  };
  writeFileSync(
    path,
    concat([
      enc('!<arch>\n'),
      member('debian-binary/  ', enc('2.0\n')),
      member('control.tar.gz/ ', Bun.gzipSync(new Uint8Array(1024))),
      member('data.tar.gz/    ', Bun.gzipSync(tar)),
    ]),
  );
}

describe('the version assertion', () => {
  test('the version the engine reports must equal the version being shipped', () => {
    expect(() =>
      checkEngineVersion(ok('{"ok":true,"version":"0.6.0"}\n'), '0.6.0'),
    ).not.toThrow();
  });

  test('a 0.5.4 engine inside a 0.6.0 bundle is refused, naming both numbers', () => {
    // The exact defect the design found inside a real artifact.
    let message = '';
    try {
      checkEngineVersion(ok('{"ok":true,"version":"0.5.4"}\n'), '0.6.0');
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toContain('0.5.4');
    expect(message).toContain('0.6.0');
  });

  test('an engine that reports failure is refused even with the right version', () => {
    expect(() =>
      checkEngineVersion(ok('{"ok":false,"version":"0.6.0","error":"broken"}\n'), '0.6.0'),
    ).toThrow(/ok/);
  });

  test('two lines on stdout is refused: the --json contract is exactly one object', () => {
    // A progress line before the result would otherwise pass a first-line
    // check while breaking every caller that parses the whole stream.
    expect(() =>
      checkEngineVersion(ok('starting\n{"ok":true,"version":"0.6.0"}\n'), '0.6.0'),
    ).toThrow(/one/);
  });

  test('a non-zero exit is refused and the stderr is forwarded', () => {
    expect(() =>
      checkEngineVersion(
        { exitCode: 127, stdout: '', stderr: 'cannot execute binary file' },
        '0.6.0',
      ),
    ).toThrow(/cannot execute binary file/);
  });

  test('unparseable output is refused rather than coerced', () => {
    expect(() => checkEngineVersion(ok('not json at all\n'), '0.6.0')).toThrow(/JSON/i);
  });

  test('a bundle with no version field at all is refused', () => {
    // `{"ok":true}` must not read as "version matched".
    expect(() => checkEngineVersion(ok('{"ok":true}\n'), '0.6.0')).toThrow(/0\.6\.0/);
  });
});

describe('opening a .deb without installing it', () => {
  test('the engine comes out with its bytes and an executable bit', () => {
    const deb = join(OUT, 'one.deb');
    fakeDeb(deb, '#!/bin/sh\necho engine\n');
    const work = mkdtempSync(join(OUT, 'work-'));
    const enginePath = extractArchiveEngine(deb, work);
    expect(enginePath.startsWith(work)).toBe(true);
    expect(statSync(enginePath).size).toBe('#!/bin/sh\necho engine\n'.length);
    // Written to disk from an archive, the mode does NOT come along for
    // free. Without this the extracted engine cannot be run at all, and the
    // failure is a bare EACCES naming nothing.
    expect(statSync(enginePath).mode & 0o111).not.toBe(0);
  });

  test('a bundle with no engine inside says so and lists what it held', () => {
    const deb = join(OUT, 'noengine.deb');
    // Same builder, different member name.
    const header = new Uint8Array(512);
    header.set(enc('usr/bin/screepub-desktop'), 0);
    header.set(enc('0000755\0'), 100);
    header.set(enc('0'.padStart(11, '0') + '\0'), 124);
    header[156] = 0x30;
    header.set(enc('        '), 148);
    const tar = concat([header, new Uint8Array(1024)]);
    const member = (name: string, data: Uint8Array): Uint8Array => {
      const h = new Uint8Array(60).fill(0x20);
      h.set(enc(name), 0);
      h.set(enc(String(data.length)), 48);
      h[58] = 0x60;
      h[59] = 0x0a;
      return concat([h, data, data.length % 2 ? new Uint8Array([0x0a]) : new Uint8Array(0)]);
    };
    writeFileSync(
      deb,
      concat([
        enc('!<arch>\n'),
        member('data.tar.gz/    ', Bun.gzipSync(tar)),
      ]),
    );
    expect(() => extractArchiveEngine(deb, mkdtempSync(join(OUT, 'work-')))).toThrow(
      /screepub-desktop/,
    );
  });
});

describe('opening a .dmg', () => {
  const app = 'Screepub Desktop.app';

  /** Stands in for hdiutil: `attach` creates the mount tree the real one
   *  would have created, `detach` records that it was called. */
  const fakeHdiutil = (mountRoot: string) => {
    const calls: string[][] = [];
    const run = (argv: string[]): RunResult => {
      calls.push(argv);
      if (argv[1] === 'attach') {
        const mount = argv[argv.indexOf('-mountpoint') + 1]!;
        mkdirSync(join(mount, app, 'Contents', 'MacOS'), { recursive: true });
        writeFileSync(join(mount, app, 'Contents', 'MacOS', 'screepub-engine'), 'ENGINE');
        return ok('');
      }
      return ok('');
    };
    return { calls, run, mountRoot };
  };

  test('it attaches read-only, finds the app, and points at the engine inside it', () => {
    const work = mkdtempSync(join(OUT, 'dmg-'));
    const h = fakeHdiutil(work);
    const { enginePath, detach } = extractDmgEngine(join(OUT, 'x.dmg'), work, h.run);
    expect(enginePath).toContain(join(app, 'Contents', 'MacOS', 'screepub-engine'));
    // -readonly and -nobrowse both matter: a writable attach can modify the
    // artifact being verified, and a browsable one leaves a volume on the
    // runner's desktop that the next job inherits.
    expect(h.calls[0]).toContain('-readonly');
    expect(h.calls[0]).toContain('-nobrowse');
    detach();
    expect(h.calls.some((c) => c[1] === 'detach')).toBe(true);
  });

  test('it finds the .app by suffix rather than by a hardcoded name', () => {
    // The macOS transition overlay renames the product to "Screepub
    // Desktop"; piece F renames it back to "Screepub". A hardcoded name
    // here would break on exactly the commit that deletes the overlay.
    const work = mkdtempSync(join(OUT, 'dmg2-'));
    const run = (argv: string[]): RunResult => {
      if (argv[1] === 'attach') {
        const mount = argv[argv.indexOf('-mountpoint') + 1]!;
        mkdirSync(join(mount, 'Screepub.app', 'Contents', 'MacOS'), { recursive: true });
        writeFileSync(join(mount, 'Screepub.app', 'Contents', 'MacOS', 'screepub-engine'), 'E');
      }
      return ok('');
    };
    expect(extractDmgEngine(join(OUT, 'y.dmg'), work, run).enginePath).toContain('Screepub.app');
  });

  test('a mount holding two .app bundles is an error, not a coin flip', () => {
    const work = mkdtempSync(join(OUT, 'dmg3-'));
    const run = (argv: string[]): RunResult => {
      if (argv[1] === 'attach') {
        const mount = argv[argv.indexOf('-mountpoint') + 1]!;
        for (const name of ['A.app', 'B.app']) {
          mkdirSync(join(mount, name, 'Contents', 'MacOS'), { recursive: true });
        }
      }
      return ok('');
    };
    expect(() => extractDmgEngine(join(OUT, 'z.dmg'), work, run)).toThrow(/A\.app.*B\.app/s);
  });

  test('a failed attach is reported with hdiutil’s own stderr', () => {
    const run = (): RunResult => ({ exitCode: 1, stdout: '', stderr: 'no mountable file systems' });
    expect(() =>
      extractDmgEngine(join(OUT, 'bad.dmg'), mkdtempSync(join(OUT, 'dmg4-')), run),
    ).toThrow(/no mountable file systems/);
  });
});

describe('opening an NSIS installer', () => {
  test('it unpacks with 7z and looks for the .exe-suffixed engine', () => {
    const work = mkdtempSync(join(OUT, 'exe-'));
    const calls: string[][] = [];
    const run = (argv: string[]): RunResult => {
      calls.push(argv);
      writeFileSync(join(work, 'screepub-engine.exe'), 'ENGINE');
      return ok('');
    };
    const enginePath = extractExeEngine(join(OUT, 'setup.exe'), work, run);
    expect(enginePath).toBe(join(work, 'screepub-engine.exe'));
    expect(calls[0]![0]).toBe('7z');
    expect(calls[0]).toContain('x');
  });

  test('an unpack that produces no engine names the directory it searched', () => {
    const work = mkdtempSync(join(OUT, 'exe2-'));
    const run = (): RunResult => ok('');
    expect(() => extractExeEngine(join(OUT, 'setup.exe'), work, run)).toThrow(/screepub-engine/);
  });

  test('a failing 7z is reported rather than swallowed', () => {
    const run = (): RunResult => ({ exitCode: 2, stdout: '', stderr: 'Cannot open the file' });
    expect(() =>
      extractExeEngine(join(OUT, 'setup.exe'), mkdtempSync(join(OUT, 'exe3-')), run),
    ).toThrow(/Cannot open the file/);
  });
});

describe('a whole smoke run over a .deb', () => {
  const fixture = 'tests/fixtures/screenplay.pdf';

  test('it runs --version and then a real conversion through the same engine', () => {
    const deb = join(OUT, 'full.deb');
    fakeDeb(deb, 'ENGINE');
    const work = mkdtempSync(join(OUT, 'full-'));
    const calls: string[][] = [];
    const run = (argv: string[]): RunResult => {
      calls.push(argv);
      if (argv.includes('--version')) return ok('{"ok":true,"version":"0.6.0"}\n');
      const epub = argv[argv.indexOf('-o') + 1]!;
      writeFileSync(epub, new Uint8Array([0x50, 0x4b, 0x03, 0x04]));
      return ok(`{"ok":true,"epubPath":${JSON.stringify(epub)},"pages":12}\n`);
    };
    smokeBundle(deb, fixture, work, '0.6.0', run);

    // Both runs used the EXTRACTED engine, not some engine on PATH. Without
    // this the whole check could pass against a developer's installed copy.
    expect(calls.length).toBe(2);
    for (const call of calls) expect(call[0]!.startsWith(work)).toBe(true);
    expect(calls[0]).toEqual([join(work, 'usr', 'bin', 'screepub-engine'), '--version', '--json']);
    expect(calls[1]).toContain(fixture);
    expect(calls[1]).toContain('--json');
  });

  test('a wrong version fails the run before any conversion is attempted', () => {
    const deb = join(OUT, 'wrongver.deb');
    fakeDeb(deb, 'ENGINE');
    const calls: string[][] = [];
    const run = (argv: string[]): RunResult => {
      calls.push(argv);
      return ok('{"ok":true,"version":"0.5.4"}\n');
    };
    expect(() => smokeBundle(deb, fixture, mkdtempSync(join(OUT, 'wv-')), '0.6.0', run)).toThrow(
      /0\.5\.4/,
    );
    expect(calls.length).toBe(1);
  });

  test('a conversion that reports zero pages is a failure, not a success', () => {
    // An empty book is not a conversion. checkConvertResult, shared with
    // smoke-cli.ts, is what says so; this proves the sharing is wired up.
    const deb = join(OUT, 'zeropages.deb');
    fakeDeb(deb, 'ENGINE');
    const work = mkdtempSync(join(OUT, 'zp-'));
    const run = (argv: string[]): RunResult => {
      if (argv.includes('--version')) return ok('{"ok":true,"version":"0.6.0"}\n');
      const epub = argv[argv.indexOf('-o') + 1]!;
      writeFileSync(epub, new Uint8Array([0x50, 0x4b, 0x03, 0x04]));
      return ok(`{"ok":true,"epubPath":${JSON.stringify(epub)},"pages":0}\n`);
    };
    expect(() => smokeBundle(deb, fixture, work, '0.6.0', run)).toThrow(/pages/);
  });

  test('a missing bundle says which path, before anything is spawned', () => {
    const calls: string[][] = [];
    expect(() =>
      smokeBundle(join(OUT, 'nope.deb'), fixture, OUT, '0.6.0', (a) => {
        calls.push(a);
        return ok('');
      }),
    ).toThrow(/nope\.deb/);
    expect(calls.length).toBe(0);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun test tests/smoke-bundle.test.ts`
Expected: FAIL — `Cannot find module '../tools/smoke-bundle'`.

- [ ] **Step 3: Write `tools/smoke-bundle.ts`**

```ts
// Open a built bundle WITHOUT installing it, and run the engine from
// inside.
//
//   bun tools/smoke-bundle.ts --bundle dist/Screepub_0.6.0_arm64.deb \
//     --expect-version 0.6.0
//
// The cheapest check that catches a broken bundle before a user does, and
// the one that would have caught a 0.6.0 installer whose engine answers
// 0.5.4. It is deliberately NOT "launch the app and look at it": no CI
// runner has a display, the GUI half of the bundle cannot be exercised
// anywhere, and the part that silently breaks during bundling is the
// sidecar -- which is exactly what AppImage's linuxdeploy pass broke.
//
// --expect-version is required rather than read from package.json, unlike
// smoke-cli.ts. A bundle's NAME comes from tauri.conf.json and the engine
// inside it is built from package.json; catching those two disagreeing is
// the entire point, so the number to compare against comes from outside.

import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { parseArgs } from 'node:util';
import { bundleEntries, findEntry } from './bundle-archive';
import { checkConvertResult, soleJson, type RunResult, type Runner } from './smoke-cli';

const realRun: Runner = (argv) => {
  const proc = Bun.spawnSync(argv, { stdout: 'pipe', stderr: 'pipe' });
  return {
    exitCode: proc.exitCode ?? 1,
    stdout: proc.stdout.toString(),
    stderr: proc.stderr.toString(),
  };
};

/** The engine's own `--version --json`, checked against the version the
 *  bundle claims to be. soleJson enforces the one-object contract, so a
 *  progress line before the answer fails here rather than downstream. */
export function checkEngineVersion(result: RunResult, expected: string): void {
  if (result.exitCode !== 0) {
    throw new Error(
      `smoke-bundle: the engine inside the bundle exited ${result.exitCode}: ` +
        result.stderr.trim().slice(0, 500),
    );
  }
  const json = soleJson(result.stdout, 'the bundled engine');
  if (json.ok !== true) {
    throw new Error(`smoke-bundle: the bundled engine reported ${JSON.stringify(json)}`);
  }
  if (json.version !== expected) {
    throw new Error(
      `smoke-bundle: the engine inside this bundle says ${JSON.stringify(json.version)} but the ` +
        `bundle is ${expected}. An installer whose About line contradicts its own filename ` +
        'lies about itself in every bug report it appears in.',
    );
  }
}

/** The engine's path inside an installed tree, per container. */
const ENGINE_IN_ARCHIVE = 'usr/bin/screepub-engine';
const ENGINE_IN_APP = join('Contents', 'MacOS', 'screepub-engine');
const ENGINE_IN_EXE = 'screepub-engine.exe';

/** A .deb or an .rpm, opened in TypeScript -- no dpkg-deb, no rpm2cpio, no
 *  7z, nothing to apt-get on a runner. */
export function extractArchiveEngine(bundlePath: string, workDir: string): string {
  const entry = findEntry(bundleEntries(bundlePath), ENGINE_IN_ARCHIVE);
  const dest = join(workDir, ENGINE_IN_ARCHIVE);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, entry.data);
  // The archive records the mode; writeFileSync does not honour it. Without
  // this the run below fails with a bare EACCES naming no file.
  chmodSync(dest, 0o755);
  return dest;
}

/** A .dmg, via hdiutil. macOS only; nothing else has hdiutil. Returns a
 *  `detach` the caller must run in a finally -- an attached image on a
 *  runner outlives the job and the next one inherits a busy mount point. */
export function extractDmgEngine(
  bundlePath: string,
  workDir: string,
  run: Runner = realRun,
): { enginePath: string; detach: () => void } {
  const mount = join(workDir, 'mnt');
  mkdirSync(mount, { recursive: true });
  // -readonly so verifying an artifact cannot modify it; -nobrowse so no
  // volume appears on a desktop the next job inherits.
  const attach = run(['hdiutil', 'attach', bundlePath, '-readonly', '-nobrowse', '-mountpoint', mount]);
  if (attach.exitCode !== 0) {
    throw new Error(
      `smoke-bundle: hdiutil attach failed on ${bundlePath}: ${attach.stderr.trim().slice(0, 500)}`,
    );
  }
  const detach = (): void => {
    run(['hdiutil', 'detach', mount, '-force']);
  };
  try {
    // Found by suffix, not by name: the transition overlay ships "Screepub
    // Desktop.app" and piece F renames it to "Screepub.app".
    const apps = readdirSync(mount).filter((n) => n.endsWith('.app'));
    if (apps.length !== 1) {
      throw new Error(
        `smoke-bundle: expected exactly one .app on the mounted image, found ` +
          `${apps.length}${apps.length ? ` (${apps.join(', ')})` : ''}`,
      );
    }
    return { enginePath: join(mount, apps[0]!, ENGINE_IN_APP), detach };
  } catch (err) {
    detach();
    throw err;
  }
}

/** An NSIS installer, via 7z, which is present on GitHub's Windows image.
 *  Extracting rather than installing keeps the runner clean and, more to
 *  the point, proves the payload without needing a machine to install on. */
export function extractExeEngine(
  bundlePath: string,
  workDir: string,
  run: Runner = realRun,
): string {
  const result = run(['7z', 'x', bundlePath, `-o${workDir}`, '-y']);
  if (result.exitCode !== 0) {
    throw new Error(
      `smoke-bundle: 7z could not unpack ${bundlePath}: ${result.stderr.trim().slice(0, 500)}`,
    );
  }
  const dest = join(workDir, ENGINE_IN_EXE);
  if (!existsSync(dest)) {
    throw new Error(
      `smoke-bundle: no ${ENGINE_IN_EXE} under ${workDir} after unpacking ${bundlePath} ` +
        `(it holds: ${readdirSync(workDir).join(', ') || '<nothing>'})`,
    );
  }
  return dest;
}

/** Two runs of the engine that came out of the bundle: its own version, and
 *  a real conversion of the committed fixture. The second is smoke-cli.ts's
 *  assertion applied to a bundled engine rather than a downloaded one, and
 *  it imports checkConvertResult rather than restating it. */
export function smokeBundle(
  bundlePath: string,
  fixture: string,
  workDir: string,
  expectedVersion: string,
  run: Runner = realRun,
): void {
  if (!existsSync(bundlePath)) throw new Error(`smoke-bundle: no bundle at ${bundlePath}`);
  if (!existsSync(fixture)) throw new Error(`smoke-bundle: no fixture at ${fixture}`);

  let enginePath: string;
  let detach: (() => void) | undefined;
  if (bundlePath.endsWith('.deb') || bundlePath.endsWith('.rpm')) {
    enginePath = extractArchiveEngine(bundlePath, workDir);
  } else if (bundlePath.endsWith('.dmg')) {
    ({ enginePath, detach } = extractDmgEngine(bundlePath, workDir, run));
  } else if (bundlePath.endsWith('.exe')) {
    enginePath = extractExeEngine(bundlePath, workDir, run);
  } else {
    throw new Error(`smoke-bundle: ${bundlePath} is not a .deb, .rpm, .dmg or .exe`);
  }

  try {
    checkEngineVersion(run([enginePath, '--version', '--json']), expectedVersion);
    const epub = join(workDir, 'smoke.epub');
    checkConvertResult(run([enginePath, fixture, '-o', epub, '--no-fountain', '--json']), epub);
  } finally {
    detach?.();
  }
}

if (import.meta.main) {
  try {
    const { values } = parseArgs({
      args: Bun.argv.slice(2),
      options: {
        bundle: { type: 'string' },
        'expect-version': { type: 'string' },
        fixture: { type: 'string' },
      },
      strict: true,
      allowPositionals: false,
    });
    if (!values.bundle) throw new Error('smoke-bundle: --bundle <path> is required');
    const expected = (values['expect-version'] ?? '').replace(/^v/, '');
    if (!expected) throw new Error('smoke-bundle: --expect-version <version> is required');
    const repo = join(import.meta.dir, '..');
    const fixture = values.fixture ?? join(repo, 'tests', 'fixtures', 'screenplay.pdf');
    const work = mkdtempSync(join(tmpdir(), 'screepub-bundle-smoke-'));
    smokeBundle(values.bundle, fixture, work, expected);
    console.log(
      `smoke-bundle: the engine inside ${values.bundle} reports ${expected} and converts the fixture`,
    );
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }
}
```

- [ ] **Step 4: Run the tests**

Run: `bun test tests/smoke-bundle.test.ts`
Expected: PASS.

- [ ] **Step 5: Run it against the real `.deb` and `.rpm` on this machine**

```bash
cd desktop/src-tauri && cargo tauri build --bundles deb,rpm && cd ../..
git checkout desktop/src-tauri/Cargo.toml
V=$(bun -e 'console.log(JSON.parse(require("fs").readFileSync("package.json","utf8")).version)')
bun tools/smoke-bundle.ts \
  --bundle desktop/src-tauri/target/release/bundle/deb/Screepub_0.6.0_arm64.deb \
  --expect-version "$V"
bun tools/smoke-bundle.ts \
  --bundle desktop/src-tauri/target/release/bundle/rpm/Screepub-0.6.0-1.aarch64.rpm \
  --expect-version "$V"
```

Expected on this branch, twice: `smoke-bundle: the engine inside … reports 0.5.4 and converts the fixture`. The version passed is `package.json`'s, because that is what the engine reports; the bundle's own filename says `0.6.0`, and that gap is the release-time check's job (Task 7), not this one's.

Then prove the check is not vacuous:

```bash
bun tools/smoke-bundle.ts \
  --bundle desktop/src-tauri/target/release/bundle/deb/Screepub_0.6.0_arm64.deb \
  --expect-version 9.9.9; echo "exit=$?"
```

Expected: a message naming `0.5.4` and `9.9.9`, and `exit=1`.

- [ ] **Step 6: Typecheck, run the whole suite, commit**

```bash
bunx tsc --noEmit
bun test
```

```
Run the engine out of the bundle, before a user has to

tools/smoke-bundle.ts opens a built bundle without installing it -- .deb and
.rpm in TypeScript, .dmg through hdiutil, NSIS through 7z -- runs the engine
from inside, and asserts one JSON object, ok: true, and a version equal to
the version being shipped. Then it converts the committed fixture through
that same extracted engine, reusing smoke-cli.ts's checkConvertResult rather
than restating it.

--expect-version is an argument rather than package.json because catching
package.json and tauri.conf.json disagreeing is the whole point. Verified by
hand against this machine's real .deb and .rpm, including that a wrong
expected version fails.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 6: The experiment — build the real Linux bundles and let what you see override this plan

**This task is framed as an experiment, not a checklist.** Everything above was written against containers built by hand and against two real bundles produced on 2026-09-14. The purpose here is to run the whole chain once, end to end, on a real build, and to **write down what actually happened** — including anything that contradicts the plan. If an observation here disagrees with a step above, the observation wins and the plan is amended in the same commit.

**Files:**
- Create: `tests/app-bundle-e2e.test.ts`
- Modify: `desktop/README.md`

**Interfaces:**
- Consumes: `BUNDLE_KINDS`, `kindsForOs`, `bundleDirFor`, `discoverArtifact`, `verifyBundleFile` from `tools/build-app-bundle.ts`; `bundleEntries`, `findEntry` from `tools/bundle-archive.ts`; `smokeBundle` from `tools/smoke-bundle.ts`.
- Produces: no new exported code. It produces **evidence**, and a test that keeps that evidence true on any machine that has a bundle to hand.

- [ ] **Step 1: Build both Linux bundles from a clean bundle directory**

```bash
rm -rf desktop/src-tauri/target/release/bundle
bun tools/build-sidecar.ts --host
cd desktop/src-tauri && time cargo tauri build --bundles deb,rpm && cd ../..
git status --short
```

Record, in a scratch note you will paste into Step 7: the wall time, the two filenames, the two sizes, and exactly which files `git status` shows as modified. Expected from 2026-09-14: about 50 seconds warm, `Screepub_0.6.0_arm64.deb` and `Screepub-0.6.0-1.aarch64.rpm` at roughly 43 MB each, and `desktop/src-tauri/Cargo.toml` modified (the feature-list rewrite Task 8 defuses).

- [ ] **Step 2: Open both and check the four things the config was supposed to fix**

```bash
bun -e '
  const { bundleEntries, findEntry } = await import("./tools/bundle-archive.ts");
  for (const p of process.argv.slice(1)) {
    const e = bundleEntries(p);
    console.log("\n" + p);
    for (const n of e) console.log("  ", n.name, n.size);
    console.log("  engine:", findEntry(e, "usr/bin/screepub-engine").size);
    console.log("  licence:", findEntry(e, "usr/lib/Screepub/LICENSE").size);
    console.log("  notices:", findEntry(e, "usr/lib/Screepub/THIRD-PARTY-NOTICES.md").size);
    const d = findEntry(e, "usr/share/applications/Screepub.desktop");
    console.log(new TextDecoder().decode(d.data));
  }
' desktop/src-tauri/target/release/bundle/deb/Screepub_0.6.0_arm64.deb \
  desktop/src-tauri/target/release/bundle/rpm/Screepub-0.6.0-1.aarch64.rpm
```

**What to look for, and what to do if it differs.** The `.rpm`'s names carry a `./` prefix and the `.deb`'s do not — `findEntry` is supposed to absorb that. If a lookup fails on one container and succeeds on the other, the prefix handling in Task 3 is wrong and the fix goes there, not into a special case here. If the `.rpm` holds no `usr/lib/Screepub/` files at all, then `bundle.resources` is honoured by the deb bundler and not the rpm one, and that is a real finding: record it in Step 7 and say so in the release notes rather than claiming both carry the licence.

- [ ] **Step 3: Smoke both, and prove the smoke is not vacuous**

```bash
V=$(bun -e 'console.log(JSON.parse(require("fs").readFileSync("package.json","utf8")).version)')
for B in desktop/src-tauri/target/release/bundle/deb/*.deb \
         desktop/src-tauri/target/release/bundle/rpm/*.rpm; do
  bun tools/smoke-bundle.ts --bundle "$B" --expect-version "$V" || echo "FAILED: $B"
  bun tools/smoke-bundle.ts --bundle "$B" --expect-version 9.9.9 >/dev/null 2>&1 \
    && echo "VACUOUS: $B accepted a wrong version" || echo "  rejects a wrong version: ok"
done
```

Both bundles must pass with the real version and **fail** with `9.9.9`. A run where both lines say "ok" for the wrong version means the version assertion is not wired to anything.

- [ ] **Step 4: Run `build-app-bundle.ts` end to end and watch it refuse**

```bash
bun tools/build-app-bundle.ts --version 0.6.0 --out /tmp/screepub-bundles; echo "exit=$?"
```

Expected on this branch: it **fails**, naming `package.json`, because `package.json` says `0.5.4` and the version being built is `0.6.0`. That refusal is the tool working. To see the happy path, temporarily point it at a fake repo rather than editing `package.json`:

```bash
bun -e '
  const { assertBundleVersions } = await import("./tools/build-app-bundle.ts");
  try { assertBundleVersions("0.6.0"); console.log("all three agree"); }
  catch (e) { console.log("refused:", e.message); }
'
```

Record which of the two you saw. Do **not** bump `package.json` to make this pass; the split is deliberate until the release commit.

- [ ] **Step 5: Write the gated end-to-end test**

Create `tests/app-bundle-e2e.test.ts`. It opens a bundle that is already on disk and never spawns `cargo`, so it is a real check on a developer machine and a clean skip everywhere else.

```ts
import { describe, test, expect, afterAll } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { bundleEntries, findEntry } from '../tools/bundle-archive';
import {
  bundleDirFor,
  discoverArtifact,
  kindsForOs,
  verifyBundleFile,
  type BundleKind,
} from '../tools/build-app-bundle';
import { smokeBundle } from '../tools/smoke-bundle';

const OUT = mkdtempSync(join(tmpdir(), 'screepub-bundle-e2e-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const VERSION = (JSON.parse(readFileSync('package.json', 'utf8')) as { version: string }).version;

/** The bundle a previous `cargo tauri build` left in the tree, or
 *  undefined. Gated rather than built: this suite must not spawn cargo, and
 *  a five-minute Rust build inside `bun test` is a suite nobody runs. CI's
 *  desktop.yml builds the bundle and then calls smoke-bundle.ts directly,
 *  which is where the real per-push coverage lives. */
function builtBundle(kind: BundleKind): string | undefined {
  const dir = bundleDirFor(kind);
  if (!existsSync(dir)) return undefined;
  try {
    return discoverArtifact(dir, kind);
  } catch {
    return undefined;
  }
}

const LINUX_KINDS = kindsForOs('linux');
const AVAILABLE = LINUX_KINDS.map((k) => ({ kind: k, path: builtBundle(k) })).filter(
  (row): row is { kind: BundleKind; path: string } => row.path !== undefined,
);

describe('a real Linux bundle, when one has been built', () => {
  test.skipIf(AVAILABLE.length === 0)(
    'every bundle on disk passes the production verifier',
    () => {
      // Not a tautology: verifyBundleFile checks a size floor and the
      // container magic, and the next test proves it rejects a file that
      // fails them.
      for (const { kind, path } of AVAILABLE) {
        expect(() => verifyBundleFile(path, kind)).not.toThrow();
        expect(statSync(path).size).toBeGreaterThan(kind.floorBytes);
      }
    },
  );

  test.skipIf(AVAILABLE.length === 0)(
    'the verifier rejects a truncated copy of that same bundle',
    () => {
      // The mutation that makes the test above evidence rather than
      // decoration. Writes to scratch; the real artifact is untouched.
      const { kind, path } = AVAILABLE[0]!;
      const copy = join(OUT, `truncated${kind.ext}`);
      Bun.write(copy, readFileSync(path).subarray(0, 4096));
      expect(() => verifyBundleFile(copy, kind)).toThrow(/floor/);
    },
  );

  test.skipIf(AVAILABLE.length === 0)(
    'each bundle carries the engine, the licence and the notices',
    () => {
      for (const { path } of AVAILABLE) {
        const entries = bundleEntries(path);
        // The engine is the reason the bundle is 43 MB; anything small here
        // is a stub that would pass an existence check.
        expect(findEntry(entries, 'usr/bin/screepub-engine').size).toBeGreaterThan(20_000_000);
        // The AGPL requires the licence to travel with the work, and the
        // compiled engine embeds Apache-2.0 and MIT libraries.
        expect(findEntry(entries, 'usr/lib/Screepub/LICENSE').size).toBeGreaterThan(1000);
        expect(
          findEntry(entries, 'usr/lib/Screepub/THIRD-PARTY-NOTICES.md').size,
        ).toBeGreaterThan(100);
      }
    },
  );

  test.skipIf(AVAILABLE.length === 0)(
    'the launcher entry offers to open a PDF and carries a human comment',
    () => {
      for (const { path } of AVAILABLE) {
        const entry = findEntry(bundleEntries(path), 'usr/share/applications/Screepub.desktop');
        const text = new TextDecoder().decode(entry.data);
        expect(text).toContain('MimeType=application/pdf');
        expect(text).toContain('Categories=Office;');
        expect(text).toMatch(/^Comment=.{20,}$/m);
        // The contributor-facing crate description must not have leaked in.
        expect(text).not.toContain('no logic lives here');
      }
    },
  );

  test.skipIf(AVAILABLE.length === 0)(
    'the engine inside runs, reports package.json’s version, and converts the fixture',
    () => {
      // The real thing: the bundled engine executes and produces an EPUB.
      // package.json's version, not tauri.conf.json's -- the engine is
      // built from the former and the split is deliberate on a branch.
      for (const { path } of AVAILABLE) {
        const work = mkdtempSync(join(OUT, 'run-'));
        smokeBundle(path, 'tests/fixtures/screenplay.pdf', work, VERSION);
      }
    },
    300000,
  );

  test.skipIf(AVAILABLE.length === 0)(
    'and it refuses a version that is not the one inside',
    () => {
      const { path } = AVAILABLE[0]!;
      const work = mkdtempSync(join(OUT, 'wrong-'));
      expect(() =>
        smokeBundle(path, 'tests/fixtures/screenplay.pdf', work, '9.9.9'),
      ).toThrow(/9\.9\.9/);
    },
    300000,
  );

  test('the gate itself is honest about what it found', () => {
    // Always runs. On a machine with no bundle this prints the skip reason
    // rather than leaving six silent skips that look like passes.
    if (AVAILABLE.length === 0) {
      console.log(
        'app-bundle-e2e: no bundle under desktop/src-tauri/target/release/bundle; ' +
          'run `cargo tauri build --bundles deb,rpm` to exercise these tests locally. ' +
          'CI covers this path in desktop.yml, which builds and then calls ' +
          'tools/smoke-bundle.ts directly.',
      );
    }
    expect(AVAILABLE.length).toBeLessThanOrEqual(LINUX_KINDS.length);
  });
});
```

- [ ] **Step 6: Run the test both ways**

```bash
bun test tests/app-bundle-e2e.test.ts
rm -rf desktop/src-tauri/target/release/bundle
bun test tests/app-bundle-e2e.test.ts
```

Expected: the first run passes six tests (the bundles are still on disk from Step 1); the second passes one and skips six, printing the skip reason. Both runs must be green. Then rebuild the bundles so later tasks have them:

```bash
cd desktop/src-tauri && cargo tauri build --bundles deb,rpm && cd ../..
git checkout desktop/src-tauri/Cargo.toml
```

- [ ] **Step 7: Write down what you observed, including anything that contradicts this plan**

Add to `desktop/README.md`, as a new section, with **your** numbers rather than the ones above:

```markdown
## Bundling: what was built, and what was checked (observed <date>)

`cargo tauri build --bundles deb,rpm` on aarch64-unknown-linux-gnu, with
`cargo-tauri <version>`:

| artifact | size | wall time |
| --- | --- | --- |
| `Screepub_<v>_arm64.deb` | … | … |
| `Screepub-<v>-1.aarch64.rpm` | … | … |

Opened with `tools/bundle-archive.ts` — no `dpkg-deb`, no `rpm2cpio`,
neither of which is installed here — both hold `usr/bin/screepub-engine`,
`usr/lib/Screepub/LICENSE`, `usr/lib/Screepub/THIRD-PARTY-NOTICES.md` and a
`.desktop` entry declaring `MimeType=application/pdf`. The engine ran
straight out of both and converted `tests/fixtures/screenplay.pdf`.

**What this does NOT show.** No `.deb` or `.rpm` has been *installed* on
any machine; they have been opened and their contents executed in place.
No `.dmg` and no NSIS installer has ever been produced by this project at
all — see "What nobody has verified" below.

<anything that contradicted the plan goes here, in plain words>
```

If nothing contradicted the plan, write "Nothing here contradicted the implementation plan" rather than deleting the heading — the absence of surprises is itself worth recording.

- [ ] **Step 8: Typecheck, run the whole suite, commit**

```bash
bunx tsc --noEmit
bun test
```

```
Build the real Linux bundles and write down what came out

Ran the whole chain once against a real build: both bundles open with the
TypeScript readers, both carry the engine, the licence, the notices and a
PDF-declaring launcher entry, and the engine runs out of both and converts
the fixture. The version check refuses a wrong version, which is what makes
the passing run evidence rather than decoration.

tests/app-bundle-e2e.test.ts keeps all of that true on any machine that has
a bundle to hand, and says out loud when it has none rather than leaving
six silent skips. desktop/README.md records the numbers and, just as
importantly, what this run does not show: nothing has been installed, and
no DMG or Windows installer exists anywhere yet.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 7: The version pin, at the top of the release run

**Files:**
- Modify: `.github/workflows/release.yml` (the `checks` job's "Release notes and version are publishable" step)
- Test: `tests/release-artifacts.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks — this is a YAML-only change plus its guard.
- Produces: two new assertions inside the existing `checks` job. Task 10's `app-bundles` job depends on `checks`, so this gate runs before any bundle is built and before any certificate is imported.

**Why here and not in `bun test`.** The three version files are only brought into agreement at release time; the split is a normal working-branch state. A `bun test` assertion demanding they agree would fail on every branch and would be deleted within a week. `release.yml`'s `checks` job already asserts `package.json == the tag` in exactly this way, twenty seconds into the run, and its own comment says this class of check belongs there.

- [ ] **Step 1: Write the failing test**

Add to `tests/release-artifacts.test.ts`, inside the existing `describe('release.yml ships the cross-platform artifacts', …)` block:

```ts
  test('the checks job fails a tag whose three version files disagree', () => {
    const text = runText(rel.jobs['checks']!);
    // package.json was already checked. These two are new, and between them
    // they name the bundle filename, the Info.plist, the deb Version: field
    // and the NSIS product version.
    expect(text).toContain('desktop/src-tauri/Cargo.toml');
    expect(text).toContain('desktop/src-tauri/tauri.conf.json');
    // Read from the TAGGED COMMIT, like every other assertion in that step,
    // not from the working tree: a checkout is not proof of what was tagged.
    expect(text).toMatch(/git cat-file blob "\$GITHUB_SHA:desktop\/src-tauri\/tauri\.conf\.json"/);
    expect(text).toMatch(/git cat-file blob "\$GITHUB_SHA:desktop\/src-tauri\/Cargo\.toml"/);
  });

  test('those two assertions run before any certificate is imported', () => {
    // The whole value of putting them in `checks` is that a mismatched
    // version costs twenty seconds instead of failing after notarization,
    // with a DMG already built. `release` needs `checks`, so the ordering
    // is structural rather than a matter of step order.
    expect(needs('release')).toEqual(['checks']);
    const release = runText(rel.jobs['release']!);
    expect(release).toContain('app/release.sh');
    // And the version check is NOT duplicated into the signing job.
    expect(release).not.toContain('desktop/src-tauri/tauri.conf.json');
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — `checks` mentions neither file.

- [ ] **Step 3: Add the assertions to the workflow**

In `.github/workflows/release.yml`, in the `checks` job's `Release notes and version are publishable` step, immediately after the existing `package.json must agree with the tag` block and before the `git fetch --no-tags origin main` block, insert:

```bash
          # The other two version files. package.json is what the ENGINE
          # reports; Cargo.toml is the crate; tauri.conf.json names the
          # bundle FILE, the Info.plist, the deb Version: field and the NSIS
          # product version. They are deliberately allowed to disagree on a
          # working branch -- that is why no `bun test` asserts this -- but a
          # tag is the moment they must agree, or a 0.6.0 installer ships an
          # engine that calls itself 0.5.4. That exact artifact was built on
          # 2026-09-14; this is the check that would have stopped it.
          #
          # Read from the tagged commit, like everything else in this step.
          # The FIRST version key AFTER [package] -- a dependency's
          # `version = "2"` must not be mistaken for the crate's, and
          # [dependencies] can appear first in the file. node, not awk: this
          # is the same regex tools/build-app-bundle.ts's assertBundleVersions
          # uses, so the two cannot drift into disagreeing about which
          # version they are reading.
          CARGO="$(git cat-file blob "$GITHUB_SHA:desktop/src-tauri/Cargo.toml" \
            | node -p "(/^\s*\[package\][\s\S]*?^\s*version\s*=\s*\"([^\"]+)\"/m.exec(require('fs').readFileSync(0,'utf8'))||[])[1]||''")"
          [ "$CARGO" = "$VERSION" ] || {
            echo "::error file=desktop/src-tauri/Cargo.toml::desktop/src-tauri/Cargo.toml says '$CARGO' but the tag says $VERSION"; exit 1; }

          CONF="$(git cat-file blob "$GITHUB_SHA:desktop/src-tauri/tauri.conf.json" \
            | node -p "JSON.parse(require('fs').readFileSync(0,'utf8')).version")"
          [ "$CONF" = "$VERSION" ] || {
            echo "::error file=desktop/src-tauri/tauri.conf.json::desktop/src-tauri/tauri.conf.json says $CONF but the tag says $VERSION"; exit 1; }
```

- [ ] **Step 4: Run the test**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS.

- [ ] **Step 5: Prove the extractor actually reads the CRATE version, on this machine**

YAML cannot be tested by cutting a release, but this one-liner can be. Run it against the real manifest and against a manifest whose `[dependencies]` section carries a `version` key the naive reading would reach first:

```bash
printf '[dependencies]\nfoo = { version = "9.9.9" }\n\n[package]\nname = "x"\nversion = "0.6.0"\n' > /tmp/trap.toml
node -p "(/^\s*\[package\][\s\S]*?^\s*version\s*=\s*\"([^\"]+)\"/m.exec(require('fs').readFileSync('/tmp/trap.toml','utf8'))||[])[1]||'NONE'"
node -p "(/^\s*\[package\][\s\S]*?^\s*version\s*=\s*\"([^\"]+)\"/m.exec(require('fs').readFileSync('desktop/src-tauri/Cargo.toml','utf8'))||[])[1]||'NONE'"
```

Expected: `0.6.0` from both. Confirmed on 2026-09-14 while this plan was written. The first is the trap — a `grep -m1 version` answers `9.9.9` — and it is the reason this reads a regex anchored on `[package]` rather than the first version-looking line. If either prints `NONE`, fix the pattern here, paste the working one into Step 3, and paste the same one into `assertBundleVersions` so the two stay identical.

- [ ] **Step 6: Commit**

```bash
bun test
```

```
Fail a tag whose three version files disagree, in twenty seconds

release.yml's checks job already asserted package.json against the tag. It
now asserts desktop/src-tauri/Cargo.toml and tauri.conf.json too, read from
the tagged commit rather than a checkout. Between them those two name the
bundle filename, the Info.plist, the deb Version: field and the NSIS
product version -- so without this a v0.6.0 tag could ship a 0.6.0
installer whose engine answers 0.5.4, which is an artifact that was really
built here on 2026-09-14.

Not a bun test assertion: the split is deliberate while a version is in
flight, and a test that fails on every working branch gets deleted. The
crate version is read with the same regex build-app-bundle.ts uses, checked
against a manifest whose [dependencies] carries a version key first.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 8: Bundle and smoke on every push, on all three platforms

**Files:**
- Modify: `desktop/src-tauri/Cargo.toml`, `tools/smoke-bundle.ts`, `.github/workflows/desktop.yml`
- Test: `tests/smoke-bundle.test.ts`, `tests/release-artifacts.test.ts`

**Interfaces:**
- Consumes: `BUNDLE_KINDS`, `kindsForOs`, `bundleDirFor`, `discoverArtifact`, `verifyBundleFile`, `osForPlatform`, `BundleOs` from `tools/build-app-bundle.ts`; `smokeBundle` from Task 5.
- Produces, added to `tools/smoke-bundle.ts`:
  - `export function smokeBuiltBundles(os: BundleOs, expectedVersion: string, fixture: string, workRoot: string, run?: Runner): string[]` — finds every bundle this runner just built, verifies each container, smokes each, and returns the paths it smoked.
  - A `--built` CLI mode: `bun tools/smoke-bundle.ts --built --expect-version <v>`.
- Task 10's release job uses `build-app-bundle.ts` instead, because a tag is the one moment the version gate must apply.

**The one gap this task does not close, stated rather than hidden.** `desktop.yml` runs `cargo tauri build` directly and then `smoke-bundle.ts --built`; it does **not** run `build-app-bundle.ts`, because that tool refuses to build when the three version files disagree and they deliberately do on every working branch. So the renaming and `SHA256SUMS-app` half of `build-app-bundle.ts` is covered by its unit tests and by the release run, and by nothing on the push path. Say so in the workflow comment; do not paper over it.

- [ ] **Step 1: Commit the manifest spelling the Tauri CLI writes**

`cargo tauri build` rewrites `desktop/src-tauri/Cargo.toml`, expanding the two Tauri dependencies with the feature list it derives from `tauri.conf.json`. Committing the expanded spelling makes that rewrite a no-op — **verified on 2026-09-14: after one bundle run left the file expanded, a second run left it byte-identical.** Edit the file to:

```toml
[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
# No serde_json, on purpose and permanently. The engine's answer is an
# opaque string on its way to the frontend; a JSON parser in this crate is
# the first step to a decision being made in it.
#
# `features = []` is not decoration and must not be tidied away: the Tauri
# CLI derives these lists from tauri.conf.json and writes them back on every
# `cargo tauri build`. Committed in the spelling the CLI produces, the
# rewrite is a no-op and a developer who bundles locally gets no unexplained
# diff. If tauri.conf.json ever gains a feature-bearing option the CLI will
# rewrite these again -- which is why desktop.yml still runs its
# `git diff --exit-code` BEFORE the bundle step and not after.
tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
```

- [ ] **Step 2: Prove the rewrite is now a no-op**

```bash
cd desktop/src-tauri && cargo tauri build --bundles deb && cd ../..
git status --short desktop/src-tauri/Cargo.toml
```

Expected: `git status` prints **nothing** for that file. If it prints a modification, the CLI wants a different spelling than the one committed — copy what it wrote into the file, note it in the commit message, and re-run this step until it is clean.

- [ ] **Step 3: Write the failing tests for `--built`**

Add to `tests/smoke-bundle.test.ts`:

```ts
describe('finding and smoking whatever this runner just built', () => {
  test('it refuses an OS whose bundles are not on disk, naming the directory', () => {
    // On a machine that has not run `cargo tauri build`, the failure must
    // say where it looked -- not return an empty list, which would make a
    // CI step that checked nothing look green.
    expect(() =>
      smokeBuiltBundles('windows', '0.6.0', 'tests/fixtures/screenplay.pdf', OUT, () => ok('')),
    ).toThrow(/bundle[/\\]nsis|nsis/);
  });

  test('it returns one path per kind the OS defines, never fewer', () => {
    // Linux defines two kinds. A run that found only the .deb and reported
    // success would ship an unchecked .rpm; the spec's whole argument for
    // per-push bundling is that an unchecked artifact is the failure mode.
    expect(kindsForOs('linux').length).toBe(2);
  });
});
```

Add `smokeBuiltBundles` to the import list at the top of that file, and `kindsForOs` from `../tools/build-app-bundle`.

- [ ] **Step 4: Run the tests to verify they fail**

Run: `bun test tests/smoke-bundle.test.ts`
Expected: FAIL — `smokeBuiltBundles` is not exported.

- [ ] **Step 5: Add `smokeBuiltBundles` and the `--built` mode**

Append to `tools/smoke-bundle.ts`. It needs one new import block; `mkdirSync`, `join` and `mkdtempSync` are already imported by Task 5's version of the file, so do **not** add a second `node:fs` import.

```ts
import {
  bundleDirFor,
  discoverArtifact,
  kindsForOs,
  osForPlatform,
  verifyBundleFile,
  type BundleOs,
} from './build-app-bundle';

/** Verify and smoke every bundle this runner just built.
 *
 *  The affordance CI needs, in TypeScript rather than in YAML: a workflow
 *  step that loops over bundle kinds in shell is a step that can only be
 *  tested by pushing. It deliberately does NOT call build-app-bundle.ts --
 *  that tool refuses to run while package.json, Cargo.toml and
 *  tauri.conf.json disagree, which they do on every working branch by
 *  design. The renaming and checksum half of build-app-bundle.ts is
 *  therefore exercised by its unit tests and by the release run, and not
 *  here. */
export function smokeBuiltBundles(
  os: BundleOs,
  expectedVersion: string,
  fixture: string,
  workRoot: string,
  run: Runner = realRun,
): string[] {
  const smoked: string[] = [];
  for (const kind of kindsForOs(os)) {
    // Throws, naming the directory, when the bundler produced nothing --
    // rather than returning an empty list, which would make a step that
    // checked nothing look green.
    const path = discoverArtifact(bundleDirFor(kind), kind);
    verifyBundleFile(path, kind);
    const work = join(workRoot, kind.id);
    mkdirSync(work, { recursive: true });
    smokeBundle(path, fixture, work, expectedVersion, run);
    console.log(`smoke-bundle: ${kind.id} ok — ${path}`);
    smoked.push(path);
  }
  if (smoked.length !== kindsForOs(os).length) {
    throw new Error(`smoke-bundle: expected ${kindsForOs(os).length} bundles for ${os}, smoked ${smoked.length}`);
  }
  return smoked;
}
```

And replace the `import.meta.main` block's body so it handles both modes:

```ts
if (import.meta.main) {
  try {
    const { values } = parseArgs({
      args: Bun.argv.slice(2),
      options: {
        bundle: { type: 'string' },
        built: { type: 'boolean', default: false },
        'expect-version': { type: 'string' },
        fixture: { type: 'string' },
      },
      strict: true,
      allowPositionals: false,
    });
    const expected = (values['expect-version'] ?? '').replace(/^v/, '');
    if (!expected) throw new Error('smoke-bundle: --expect-version <version> is required');
    const repo = join(import.meta.dir, '..');
    const fixture = values.fixture ?? join(repo, 'tests', 'fixtures', 'screenplay.pdf');
    const work = mkdtempSync(join(tmpdir(), 'screepub-bundle-smoke-'));

    if (values.built) {
      const smoked = smokeBuiltBundles(osForPlatform(process.platform), expected, fixture, work);
      console.log(`smoke-bundle: ${smoked.length} bundle(s) report ${expected} and convert the fixture`);
    } else {
      if (!values.bundle) throw new Error('smoke-bundle: pass --bundle <path> or --built');
      smokeBundle(values.bundle, fixture, work, expected);
      console.log(
        `smoke-bundle: the engine inside ${values.bundle} reports ${expected} and converts the fixture`,
      );
    }
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }
}
```

- [ ] **Step 6: Run the tests, then run `--built` for real**

```bash
bun test tests/smoke-bundle.test.ts
cd desktop/src-tauri && cargo tauri build --bundles deb,rpm && cd ../..
V=$(bun -e 'console.log(JSON.parse(require("fs").readFileSync("package.json","utf8")).version)')
bun tools/smoke-bundle.ts --built --expect-version "$V"
```

Expected: two `smoke-bundle: … ok` lines and `smoke-bundle: 2 bundle(s) report 0.5.4 and convert the fixture`. `git status` should still show `Cargo.toml` clean, from Step 1.

- [ ] **Step 7: Write the failing workflow test**

Add to `tests/release-artifacts.test.ts`, as a new top-level `describe`:

```ts
describe('desktop.yml bundles and smokes on every push', () => {
  const desktop = workflow('desktop.yml');
  const job = desktop.jobs['build']!;
  const steps = job.steps ?? [];
  const stepIndex = (re: RegExp): number => steps.findIndex((s) => re.test(s.run ?? ''));

  test('it still compiles on all three platforms', () => {
    // The matrix predates this piece and is not replaced by it.
    expect(JSON.stringify(job)).toContain('cargo build --locked');
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    for (const os of ['ubuntu-latest', 'macos-15', 'windows-latest']) {
      expect(yml).toContain(os);
    }
  });

  test('the generated-files diff runs BEFORE anything that rewrites Cargo.toml', () => {
    // `cargo tauri build` rewrites desktop/src-tauri/Cargo.toml with the
    // feature lists it derives from tauri.conf.json. The committed spelling
    // makes that a no-op today, but a future tauri.conf.json option would
    // make it rewrite again -- and a `git diff --exit-code` running after
    // the bundle step would then go red for a reason nobody could read.
    const diff = stepIndex(/git diff --exit-code/);
    const bundle = stepIndex(/cargo tauri build/);
    expect(diff).toBeGreaterThanOrEqual(0);
    expect(bundle).toBeGreaterThanOrEqual(0);
    expect(diff).toBeLessThan(bundle);
  });

  test('the bundle list is pinned and never the per-OS default', () => {
    // The Linux default is deb, rpm AND appimage, and the AppImage step
    // corrupts the Bun-compiled engine and fails the whole run.
    const text = runText(job);
    expect(text).toContain('--bundles');
    expect(text).not.toContain('appimage');
  });

  test('it runs smoke-bundle after building, not instead of it', () => {
    const bundle = stepIndex(/cargo tauri build/);
    const smoke = stepIndex(/tools\/smoke-bundle\.ts/);
    expect(smoke).toBeGreaterThan(bundle);
    expect(steps[smoke]!.run).toContain('--built');
  });

  test('cargo-tauri is installed at a pinned version and cached', () => {
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    // An unpinned `cargo install tauri-cli` is a four-minute build against
    // whatever crates.io served that morning.
    expect(yml).toMatch(/cargo install tauri-cli --version [0-9]/);
    expect(yml).toContain('--locked');
    expect(yml).toContain('~/.cargo/bin/cargo-tauri');
  });

  test('the push path imports no Developer ID certificate', () => {
    // Signing on every push is slow and exposes the secret far more widely
    // than a release does. The push bundle is unsigned on purpose.
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    for (const secret of ['APPLE_CERTIFICATE', 'DEVELOPER_ID_CERT_P12_BASE64', 'APPLE_API_KEY']) {
      expect(yml).not.toContain(secret);
    }
  });
});
```

- [ ] **Step 8: Run the test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — `desktop.yml` has no bundle step, no smoke step and no `cargo install tauri-cli`.

- [ ] **Step 9: Extend `desktop.yml`**

Append these steps to the `build` job, after the existing `Compile the shell` step, and add the cache step before it. The `Generated window files are current` step stays exactly where it is — first — and the comment there now says why on purpose rather than by accident.

```yaml
      # cargo-tauri is a new release-toolchain dependency. Pinned exactly --
      # an unpinned `cargo install tauri-cli` is a four-minute build against
      # whatever crates.io served that morning -- and cached, or every push
      # pays that four minutes three times over.
      - name: Cache the Tauri CLI
        id: tauri-cli-cache
        uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0
        with:
          path: |
            ~/.cargo/bin/cargo-tauri
            ~/.cargo/bin/cargo-tauri.exe
          key: tauri-cli-2.11.4-${{ matrix.os }}
      - name: Install the Tauri CLI
        if: steps.tauri-cli-cache.outputs.cache-hit != 'true'
        run: cargo install tauri-cli --version 2.11.4 --locked
      # Bundling on every push turns a compile check into an ARTIFACT check,
      # which is where a broken bundle should be caught -- not at a tag.
      #
      # The bundle list is pinned per OS because the Linux default is deb,
      # rpm AND appimage, and the AppImage step runs linuxdeploy's patchelf
      # over the Bun-compiled engine, moves its dynamic section past the
      # 102 MB appended payload, and leaves a binary that segfaults. Measured
      # 2026-09-14; see the design doc.
      #
      # No certificate is imported here and the macOS bundle is UNSIGNED.
      # Importing a Developer ID on every push is slow and exposes the
      # secret far more widely than a release does; signing happens in
      # release.yml, once, at a tag.
      - name: Bundle this runner's installers
        working-directory: desktop/src-tauri
        shell: bash
        run: |
          set -euo pipefail
          case "${{ runner.os }}" in
            Linux)   BUNDLES=deb,rpm ;;
            macOS)   BUNDLES=app,dmg ;;
            Windows) BUNDLES=nsis ;;
          esac
          cargo tauri build --bundles "$BUNDLES"
      # The check that would have caught a 0.6.0 installer whose engine
      # answers 0.5.4: open each bundle without installing it and run the
      # engine from inside. package.json's version is the right expectation
      # HERE -- the engine is built from it, and it is deliberately allowed
      # to lag tauri.conf.json on a working branch. release.yml asserts all
      # three against the tag instead.
      #
      # This calls smoke-bundle.ts and NOT build-app-bundle.ts, because that
      # tool refuses to run while the three version files disagree. The
      # renaming and SHA256SUMS-app half of build-app-bundle.ts is covered
      # by its unit tests and by the release run, and by nothing here.
      - name: Run the engine out of each bundle
        shell: bash
        run: |
          set -euo pipefail
          V="$(node -p "require('./package.json').version")"
          bun tools/smoke-bundle.ts --built --expect-version "$V"
```

Also add `'tools/bundle-archive.ts'`, `'tools/build-app-bundle.ts'`, `'tools/smoke-bundle.ts'` and `'package.json'` to both `paths:` filters at the top of the file, so an edit to any of them triggers the workflow that tests it.

- [ ] **Step 10: Run the tests**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS.

- [ ] **Step 11: Say plainly, in the workflow's header comment, what this does and does not prove**

Replace the first paragraph of `desktop/.github/workflows/desktop.yml`'s header — that is, `.github/workflows/desktop.yml`'s opening comment block — with:

```yaml
# The desktop shell compiles on all three platforms, bundles an installer on
# each, and runs the engine out of that installer. It does NOT install the
# bundle, launch the window, or click anything: no runner has a display, so
# the GUI half of the app is exercised nowhere. What breaks silently during
# bundling is the sidecar -- AppImage's patchelf pass corrupted it outright
# -- and that is exactly what the smoke step catches.
#
# Nobody on this project has a Windows machine and the Mac is elsewhere, so
# these three jobs are the only evidence that exists that the shell builds
# or bundles there at all. As of the commit that added the bundle steps,
# this workflow had never run: the branch was unpushed. A green Windows leg
# here is the FIRST time a Screepub Windows installer will have existed.
```

- [ ] **Step 12: Typecheck, run the whole suite, commit**

```bash
bunx tsc --noEmit
bun test
```

```
Turn the desktop compile check into an artifact check

desktop.yml now bundles an installer on each of the three runners and runs
the engine out of it. That is the check that catches a broken sidecar -- the
failure AppImage produced -- and it catches it on a push rather than at a
tag.

The Cargo.toml feature lists are committed in the spelling the Tauri CLI
writes, so bundling no longer leaves the manifest dirty; verified by running
a bundle twice and seeing the second leave it byte-identical. The
generated-files diff still runs first, now on purpose rather than by
accident, because a future tauri.conf.json option would make the CLI rewrite
the manifest again.

No certificate is imported on the push path: the macOS bundle built here is
unsigned, and signing happens once, at a tag. The renaming half of
build-app-bundle.ts is not exercised here, because its version gate refuses
to run on a branch; the workflow comment says so.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 9: The macOS transition overlay — one file, deleted at piece F

**Files:**
- Create: `desktop/src-tauri/tauri.transition.conf.json`
- Test: `tests/desktop-shell.test.ts`

**Interfaces:**
- Consumes: `tauri.conf.json`'s `productName` and `identifier`, as pinned by Task 2.
- Produces: `desktop/src-tauri/tauri.transition.conf.json`, passed by Task 10's macOS job as `cargo tauri build --config tauri.transition.conf.json`. Nothing imports it in TypeScript.

**The collision it resolves.** The bundle identifiers already differ — `com.darkwell.screepub.desktop` against the SwiftUI app's `com.darkwell.screepub`, pinned by `tests/desktop-shell.test.ts` — so LaunchServices treats them as two applications. The one real clash is the filename: both bundlers want `/Applications/Screepub.app`. The overlay overrides `productName` to `Screepub Desktop` for the macOS job only. The Linux job does not pass it, so the `.deb` package name is untouched; the window title is a separate key (`app.windows[0].title`) and stays `Screepub`.

**Why a file and not a flag.** Piece F deletes this file, and the name becomes `Screepub.app`. A file that has to be removed is harder to forget than a flag that has to be unset.

**What is unverifiable here.** Everything. No `.app` has ever been produced by this project on any machine. The overlay's *effect* — that the DMG installs `Screepub Desktop.app` beside an existing `Screepub.app` — will first be observable when a human opens the DMG, and no step in this plan can claim it before then.

- [ ] **Step 1: Write the failing test**

Add to `tests/desktop-shell.test.ts`:

```ts
describe('the macOS transition overlay', () => {
  const overlayPath = join(REPO, 'desktop', 'src-tauri', 'tauri.transition.conf.json');

  test('it exists, and it is passed to cargo tauri build by the macOS job only', () => {
    expect(existsSync(overlayPath)).toBe(true);
  });

  test('it overrides the product NAME and nothing else', () => {
    // A config overlay is merged into tauri.conf.json wholesale. Every
    // extra key here is a setting that silently differs between the macOS
    // build and the other two, on a platform nobody here can inspect. One
    // key is auditable; three are not.
    const overlay = JSON.parse(readFileSync(overlayPath, 'utf8')) as Record<string, unknown>;
    // Keys beginning with _ are ignored by Tauri and carry the note JSON
    // cannot hold as a comment; the EFFECTIVE key set is what is pinned.
    expect(Object.keys(overlay).filter((k) => !k.startsWith('_'))).toEqual(['productName']);
    expect(overlay.productName).toBe('Screepub Desktop');
  });

  test('it does not collide with the SwiftUI app’s bundle name', () => {
    const overlay = JSON.parse(readFileSync(overlayPath, 'utf8')) as { productName: string };
    // app/build-app.sh produces Screepub.app and app/release.sh ships it
    // inside Screepub-macOS.dmg, which tools/bump-tap.sh hardcodes. Both
    // apps must be installable at once until piece F.
    expect(overlay.productName).not.toBe('Screepub');
    expect(overlay.productName).not.toBe(CONFIG.productName);
  });

  test('it does not touch the identifier, which already differs', () => {
    // If the overlay ever set an identifier, the two apps could collide in
    // LaunchServices in a way the filename difference would hide.
    const overlay = JSON.parse(readFileSync(overlayPath, 'utf8')) as Record<string, unknown>;
    expect(overlay.identifier).toBeUndefined();
    expect(CONFIG.identifier).toBe('com.darkwell.screepub.desktop');
  });

  test('it does not touch the window title', () => {
    // productName names the .app; app.windows[0].title names the window. A
    // user who opens the app should see "Screepub", not the transition
    // spelling, on every platform.
    const overlay = JSON.parse(readFileSync(overlayPath, 'utf8')) as Record<string, unknown>;
    expect(overlay.app).toBeUndefined();
    expect(CONFIG.app.windows[0].title).toBe('Screepub');
  });

  test('the file says, in itself, that piece F deletes it', () => {
    // JSON has no comments, so the marker is a key. Without it, this file
    // is indistinguishable from permanent configuration and outlives the
    // transition it exists for.
    const raw = readFileSync(overlayPath, 'utf8');
    expect(raw).toContain('piece F');
  });
});
```

The marker the last test looks for is a JSON key beginning with `_`, because JSON has no comments. Tauri ignores unknown top-level keys in an overlay, which is why the key-set assertion filters them out rather than forbidding them.

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test tests/desktop-shell.test.ts`
Expected: FAIL — the file does not exist.

- [ ] **Step 3: Write the overlay**

Create `desktop/src-tauri/tauri.transition.conf.json`:

```json
{
  "_why": "TRANSITION ONLY. Passed as `cargo tauri build --config tauri.transition.conf.json` by release.yml's macOS job, and by nothing else. The SwiftUI app installs /Applications/Screepub.app and ships until piece F, so the Tauri app must not want that same path. Piece F DELETES this file and the Tauri app becomes Screepub.app. Do not add keys: every extra one is a setting that differs on the platform nobody here can inspect.",
  "productName": "Screepub Desktop"
}
```

- [ ] **Step 4: Run the test**

Run: `bun test tests/desktop-shell.test.ts`
Expected: PASS.

- [ ] **Step 5: Check that Tauri accepts the overlay, as far as can be checked here**

`cargo tauri build --config` on Linux would produce a `.deb` named `Screepub Desktop`, which is not what any job does — so do not run it that way. What *can* be checked here is that the CLI parses the file and merges it:

```bash
cd desktop/src-tauri
cargo tauri build --config tauri.transition.conf.json --bundles deb 2>&1 | grep -i "bundling\|error"
cd ../..
rm -rf desktop/src-tauri/target/release/bundle/deb
git status --short desktop/src-tauri/Cargo.toml
```

Expected: a `Bundling Screepub Desktop_0.6.0_arm64.deb` line and no error — which proves the overlay is well-formed and reaches `productName`, without claiming anything about macOS. Delete that `.deb` afterwards so Task 6's e2e test does not find two candidates, and rebuild the ordinary pair:

```bash
cd desktop/src-tauri && cargo tauri build --bundles deb,rpm && cd ../..
```

- [ ] **Step 6: Record what remains unproven**

Add to `desktop/README.md`, under the bundling section Task 6 created:

```markdown
### The macOS name, and what has not been seen

`desktop/src-tauri/tauri.transition.conf.json` renames the macOS product to
`Screepub Desktop` so the Tauri app and the SwiftUI app can both be
installed. The overlay is passed only by release.yml's macOS job; piece F
deletes the file and the name becomes `Screepub.app`.

**Nobody has opened the result.** No `.app`, no `.dmg` and no NSIS
installer has been produced by this project on any machine at the time of
writing; the macOS and Windows halves are read off tauri-bundler's source
and ride on CI. That the overlay is well-formed and reaches `productName`
was checked by bundling a throwaway `.deb` with it on Linux, which proves
the file parses and nothing more.
```

- [ ] **Step 7: Commit**

```bash
bun test
```

```
Give the Tauri app a Mac name that does not fight the Swift one

Both bundlers want /Applications/Screepub.app and the SwiftUI app ships
until piece F, so the macOS job builds "Screepub Desktop" through a
one-key config overlay. The identifiers already differ and the window
title is a separate key, so neither moves.

A file rather than a flag on purpose: piece F has to delete it, and a
deletion is harder to forget than an unset. Its own _why key says so, and
the tests refuse any second key -- an overlay is merged wholesale, and
every extra key is a setting that differs on the platform nobody here can
inspect.

Checked only as far as it can be: bundling a throwaway .deb with the
overlay proves it parses and reaches productName. No .app has been
produced by this project on any machine.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 10: The release path — build, smoke, and attach after the release is out of draft

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `tests/release-artifacts.test.ts`

**Interfaces:**
- Consumes: `tools/build-app-bundle.ts`'s CLI (`--version`, `--out`, `--target`, `--config`) from Task 4; `tools/smoke-bundle.ts`'s `--bundle`/`--expect-version` from Task 5; `desktop/src-tauri/tauri.transition.conf.json` from Task 9; the version assertions from Task 7.
- Produces: two new jobs, `app-bundles` (a three-OS matrix, `needs: checks`) and `app-upload` (`needs: [release, app-bundles]`). No existing job is modified except `checks`, which Task 7 already changed.

**The secrets translation, and why no new secret is needed.** `tauri-bundler`'s `macos/sign.rs` reads its own environment variable names. Each maps onto a secret `docs/release-secrets.md` already documents:

| existing secret | Tauri env var |
| --- | --- |
| `DEVELOPER_ID_CERT_P12_BASE64` | `APPLE_CERTIFICATE` |
| `CERT_PASSWORD` | `APPLE_CERTIFICATE_PASSWORD` |
| the identity the `release` job already discovers | `APPLE_SIGNING_IDENTITY` |
| `AC_API_KEY_ID` | `APPLE_API_KEY` |
| `AC_API_ISSUER_ID` | `APPLE_API_ISSUER` |
| `AC_API_KEY_P8_BASE64`, decoded to a file | `APPLE_API_KEY_PATH` |

`app/release.sh` is not reused and not modified. The two paths touch different directories and the same keychain, and the Swift path keeps producing the DMG users actually download.

**Three facts about that signing path, read off `tauri-bundler-2.9.4` on 2026-09-14 rather than assumed.** They are why this job needs no `security create-keychain` block of its own:

1. `macos/sign.rs:19-42` — given `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`, the bundler builds **its own keychain and imports the certificate itself**. The workflow sets two variables; it runs no `security` command.
2. `APPLE_SIGNING_IDENTITY` is read by the CLI (`tauri-cli-2.11.4/src/interface/rust.rs:1467`) and used in `sign.rs:29-34` as a **substring guard**: the run fails if the imported certificate's identity does not contain it. So `Developer ID Application` is a correct and deliberately loose value — it asserts the right *kind* of certificate was imported without pinning the team name into YAML.
3. `sign.rs:113-115` — notarization takes `APPLE_API_KEY`, `APPLE_API_ISSUER` and `APPLE_API_KEY_PATH`, which is why the key is decoded to a file in its own step.

None of this has been executed. It is source reading, and the first run will be the first evidence.

**Why the upload is a separate job.** The existing rule: a failure in a new path must not strand the release in draft. `cross-upload` says so in its own comment and this follows it for the same reason.

- [ ] **Step 1: Write the failing tests**

Add to `tests/release-artifacts.test.ts`, inside `describe('release.yml ships the cross-platform artifacts', …)`:

```ts
  test('app-bundles builds on all three runners, after the checks pass', () => {
    const job = rel.jobs['app-bundles'];
    expect(job).toBeDefined();
    expect(needs('app-bundles')).toEqual(['checks']);
    const yml = read(join(WORKFLOWS, 'release.yml'));
    for (const os of ['ubuntu-latest', 'macos-15', 'windows-latest']) {
      expect(yml).toContain(os);
    }
    const text = runText(job!);
    expect(text).toContain('tools/build-app-bundle.ts');
    // The TAG, so a version mismatch fails the build rather than shipping a
    // bundle that misreports itself.
    expect(text).toContain('${TAG#v}');
  });

  test('the macOS leg builds two per-arch DMGs and never a universal one', () => {
    // A universal bundle would need a third, lipo'd sidecar that
    // tools/build-sidecar.ts cannot make: externalBin resolves
    // {name}-{target_triple} verbatim, with no special case for
    // universal-apple-darwin.
    const text = runText(rel.jobs['app-bundles']!);
    expect(text).toContain('aarch64-apple-darwin');
    expect(text).toContain('x86_64-apple-darwin');
    expect(text).not.toContain('universal-apple-darwin');
  });

  test('the macOS leg passes the transition overlay so the two Mac apps coexist', () => {
    const text = runText(rel.jobs['app-bundles']!);
    expect(text).toContain('tauri.transition.conf.json');
  });

  test('the macOS leg signs and notarizes from secrets the repo already has', () => {
    const yml = read(join(WORKFLOWS, 'release.yml'));
    // tauri-bundler reads its OWN variable names; these are the translation.
    for (const v of [
      'APPLE_CERTIFICATE',
      'APPLE_CERTIFICATE_PASSWORD',
      'APPLE_SIGNING_IDENTITY',
      'APPLE_API_KEY',
      'APPLE_API_ISSUER',
      'APPLE_API_KEY_PATH',
    ]) {
      expect(yml).toContain(v);
    }
    // And no NEW secret was invented for it.
    const declared = yml.match(/secrets\.[A-Z_]+/g) ?? [];
    expect([...new Set(declared)].sort()).toEqual([
      'secrets.AC_API_ISSUER_ID',
      'secrets.AC_API_KEY_ID',
      'secrets.AC_API_KEY_P8_BASE64',
      'secrets.CERT_PASSWORD',
      'secrets.DEVELOPER_ID_CERT_P12_BASE64',
      'secrets.KEYCHAIN_PASSWORD',
      'secrets.TAP_TOKEN',
    ]);
  });

  test('the Windows leg is deliberately unsigned', () => {
    // Authenticode is a procurement problem, not an engineering one. This
    // assertion is what keeps that a decision rather than a drift.
    const yml = read(join(WORKFLOWS, 'release.yml'));
    expect(yml).not.toContain('WINDOWS_CERTIFICATE');
    expect(yml).not.toContain('signtool');
  });

  test('every bundle is smoked on the OS that built it, before any upload', () => {
    const text = runText(rel.jobs['app-bundles']!);
    expect(text).toContain('tools/smoke-bundle.ts');
    // The upload waits for BOTH the release job and every bundle leg. If it
    // did not wait for the smoke, the smoke would be decoration.
    expect(needs('app-upload').sort()).toEqual(['app-bundles', 'release']);
  });

  test('the checksums are re-verified after the artifact round trip', () => {
    // Same blind spot cross-upload already names: the files are hashed in
    // one job's $RUNNER_TEMP and then cross a job boundary.
    const steps = rel.jobs['app-upload']!.steps ?? [];
    const verify = steps.findIndex((s) => /sha256sum -c SHA256SUMS-app/.test(s.run ?? ''));
    const upload = steps.findIndex((s) => /gh release upload/.test(s.run ?? ''));
    expect(verify).toBeGreaterThanOrEqual(0);
    expect(verify).toBeLessThan(upload);
  });

  test('the app checksums file does not overwrite the CLI one', () => {
    // cross-upload publishes SHA256SUMS to the same release page. One
    // clobbering the other leaves downloads silently uncheckable.
    const text = runText(rel.jobs['app-upload']!);
    expect(text).toContain('SHA256SUMS-app');
    expect(text).not.toMatch(/SHA256SUMS(?!-app)/);
  });

  test('the SwiftUI release path is untouched', () => {
    // Acceptance criterion 7, as an assertion rather than a promise.
    const release = runText(rel.jobs['release']!);
    expect(release).toContain('app/release.sh');
    for (const name of MACOS_ASSETS) expect(release).toContain(`app/dist/${name}`);
    // The new jobs must not touch the Swift artifacts or the tap.
    for (const jobName of ['app-bundles', 'app-upload']) {
      const text = runText(rel.jobs[jobName]!);
      expect(text).not.toContain('app/release.sh');
      expect(text).not.toContain('bump-tap');
      for (const name of MACOS_ASSETS) expect(text).not.toContain(name);
    }
    expect(needs('tap')).toEqual(['release']);
    expect(needs('tap-check')).toEqual(['tap']);
  });

  test('the Linux leg ships both a .deb and an .rpm', () => {
    const text = runText(rel.jobs['app-upload']!);
    expect(text).toContain('.deb');
    expect(text).toContain('.rpm');
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — `rel.jobs['app-bundles']` is `undefined`.

- [ ] **Step 3: Add the `app-bundles` job**

Append to `.github/workflows/release.yml`:

```yaml
  # The installable app bundles: .deb and .rpm, two signed and notarized
  # per-arch DMGs, and one unsigned NSIS installer. Every decision lives in
  # tools/build-app-bundle.ts, which is unit-tested and hand-runnable; this
  # job is a thin caller, because workflow YAML can only be tested by
  # cutting a release.
  #
  # Per-arch DMGs and never a universal one: externalBin resolves
  # {name}-{target_triple} verbatim, so a universal bundle would need a
  # third, lipo'd sidecar that tools/build-sidecar.ts cannot make and that
  # nothing here could verify.
  #
  # No AppImage. linuxdeploy runs its own patchelf over the Bun-compiled
  # engine, moves the dynamic section past the 102 MB appended payload, and
  # the extracted engine segfaults. Measured; see the design doc.
  app-bundles:
    needs: checks
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            arch: x64
          - os: macos-15
            arch: arm64
            target: aarch64-apple-darwin
            sidecar: bun-darwin-arm64
          - os: macos-15
            arch: x64
            target: x86_64-apple-darwin
            sidecar: bun-darwin-x64
          - os: windows-latest
            arch: x64
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
        with:
          bun-version: 1.3.14
      - name: Linux webview libraries
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev
      - run: bun install --frozen-lockfile
      # A release must build from bytes fetched fresh rather than from a
      # cache a push left a week ago -- the same decision cross-cli's
      # comment already records, for the same reason.
      - name: Install the Tauri CLI
        run: cargo install tauri-cli --version 2.11.4 --locked
      # The macOS legs cross-compile: each needs the Rust target and a
      # sidecar named for THAT triple, not the runner's own.
      - name: Add the Rust target
        if: matrix.target != ''
        run: rustup target add ${{ matrix.target }}
      - name: Build the engine sidecar
        run: |
          if [ -n "${{ matrix.sidecar }}" ]; then
            bun tools/build-sidecar.ts --target ${{ matrix.sidecar }}
          else
            bun tools/build-sidecar.ts --host
          fi
        shell: bash
      # Signing and notarization are tauri-bundler's own, from the secrets
      # docs/release-secrets.md already documents under different names.
      # app/release.sh is neither read nor modified; the two paths touch
      # different directories and the same keychain.
      - name: Write the App Store Connect API key
        if: runner.os == 'macOS'
        env:
          AC_KEY_B64: ${{ secrets.AC_API_KEY_P8_BASE64 }}
        run: |
          echo "$AC_KEY_B64" | base64 --decode > "$RUNNER_TEMP/ac_api_key.p8"
          echo "APPLE_API_KEY_PATH=$RUNNER_TEMP/ac_api_key.p8" >> "$GITHUB_ENV"
      - name: Build and verify this platform's bundles
        shell: bash
        env:
          TAG: ${{ github.ref_name }}
          TARGET: ${{ matrix.target }}
          # Read by tauri-bundler's macos/sign.rs. Empty on Linux and
          # Windows, where the bundler skips signing entirely.
          APPLE_CERTIFICATE: ${{ runner.os == 'macOS' && secrets.DEVELOPER_ID_CERT_P12_BASE64 || '' }}
          APPLE_CERTIFICATE_PASSWORD: ${{ runner.os == 'macOS' && secrets.CERT_PASSWORD || '' }}
          APPLE_SIGNING_IDENTITY: ${{ runner.os == 'macOS' && 'Developer ID Application' || '' }}
          APPLE_API_KEY: ${{ runner.os == 'macOS' && secrets.AC_API_KEY_ID || '' }}
          APPLE_API_ISSUER: ${{ runner.os == 'macOS' && secrets.AC_API_ISSUER_ID || '' }}
        run: |
          set -euo pipefail
          ARGS=(--version "${TAG#v}" --out "$RUNNER_TEMP/app")
          if [ -n "$TARGET" ]; then
            ARGS+=(--target "$TARGET" --config tauri.transition.conf.json)
          fi
          bun tools/build-app-bundle.ts "${ARGS[@]}"
      # Cross-compiling is not cross-testing, and bundling is not running.
      # Each bundle is opened and its engine executed, here, on the machine
      # that built it. The x86_64 DMG built on an arm64 runner is the one
      # exception and this job says so rather than implying otherwise.
      - name: Run the engine out of each bundle
        shell: bash
        env:
          TAG: ${{ github.ref_name }}
        run: |
          set -euo pipefail
          if [ "${{ matrix.target }}" = "x86_64-apple-darwin" ]; then
            echo "::notice::The x86_64 DMG was cross-compiled on an arm64 runner; its engine cannot be executed here and is NOT smoke-tested."
            exit 0
          fi
          for B in "$RUNNER_TEMP"/app/*.deb "$RUNNER_TEMP"/app/*.rpm \
                   "$RUNNER_TEMP"/app/*.dmg "$RUNNER_TEMP"/app/*.exe; do
            [ -e "$B" ] || continue
            bun tools/smoke-bundle.ts --bundle "$B" --expect-version "${TAG#v}"
          done
      - uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4
        with:
          name: app-bundles-${{ matrix.os }}-${{ matrix.arch }}
          path: ${{ runner.temp }}/app/*
          retention-days: 1
          if-no-files-found: error
```

- [ ] **Step 4: Add the `app-upload` job**

Append:

```yaml
  # Attach the bundles to the release the macOS job already published.
  # A SEPARATE job, after it, rather than moving the un-draft down here: a
  # failure in this path must not leave the release permanently in draft,
  # which is the rule cross-upload already follows.
  app-upload:
    needs: [release, app-bundles]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4
        with:
          pattern: app-bundles-*
          merge-multiple: true
          path: app
      # Each leg wrote its own SHA256SUMS-app, and merge-multiple keeps only
      # one of them. Rebuilt here from what actually arrived, so the
      # published file describes every file published beside it -- which the
      # per-leg files could not, since no leg ever saw the others.
      - name: Rebuild the checksums from what arrived, then verify them
        run: |
          set -euo pipefail
          cd app
          rm -f SHA256SUMS-app
          sha256sum *.deb *.rpm *.dmg *.exe > SHA256SUMS-app
          # Not a tautology: -c re-reads every file and re-hashes it, which
          # is what catches a truncated artifact round trip.
          sha256sum -c SHA256SUMS-app
          cat SHA256SUMS-app
      - name: Upload the app bundles
        env:
          GH_TOKEN: ${{ github.token }}
          GH_REPO: ${{ github.repository }}
          TAG: ${{ github.ref_name }}
        run: |
          set -euo pipefail
          cd app
          gh release upload "$TAG" \
            *.deb *.rpm *.dmg *.exe SHA256SUMS-app \
            --clobber
```

- [ ] **Step 5: Run the tests**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS. If the "no new secret" assertion fails, it is listing the secrets the whole file names — compare the list in the failure against `docs/release-secrets.md` and fix whichever is wrong; do not loosen the assertion.

- [ ] **Step 6: Check the YAML parses and the bash is bash-3.2-safe**

```bash
bun -e 'Bun.YAML.parse(require("fs").readFileSync(".github/workflows/release.yml","utf8")); console.log("parses")'
bun test tests/workflow-shell.test.ts
```

Expected: `parses`, and the bash-3.2 guard tests still pass. `ARGS=(...)` with `ARGS+=(...)` is always non-empty here, so it needs no `${ARGS[@]+…}` guard — but `workflow-shell.test.ts`'s regex will tell you if that reading is wrong, and it is the file that already caught this class of bug after notarization.

- [ ] **Step 7: Commit**

```bash
bun test
```

```
Ship the installers on the same tag as everything else

release.yml gains app-bundles (three runners, four legs, each building and
smoking its own installers) and app-upload (which rebuilds SHA256SUMS-app
from what actually arrived, verifies it, and attaches everything after the
release is out of draft). One v0.6.0 tag produces the CLI artifacts, the
Swift DMG and the app bundles, on one release page, describing one version.

macOS signs and notarizes through tauri-bundler's own environment
variables, which map onto secrets the repo already has; no new secret and
no change to app/release.sh. Windows is unsigned, on purpose, and a test
asserts no signing secret has crept in.

The x86_64 DMG is cross-compiled on an arm64 runner, so its engine cannot
be executed there and is NOT smoke-tested. The job prints that as a notice
rather than passing silently.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Task 11: Say what ships, what warns, and what nobody has run

**Files:**
- Modify: `README.md`, `site/index.html`, `docs/releases/0.6.0.md`, `desktop/README.md`
- Test: `tests/release-artifacts.test.ts`

**Interfaces:**
- Consumes: `BUNDLE_KINDS` from `tools/build-app-bundle.ts` — the documentation tests read the published filenames from the table rather than restating them, so a renamed artifact fails the docs test rather than silently making the README wrong.
- Produces: no code.

**The rule this task exists to obey.** The last review's three blocking findings were all shipped prose claiming something the code did not do. Every sentence below is either a fact this plan verified, or an explicit statement that something is unverified. There is no third category.

- [ ] **Step 1: Write the failing tests**

Add to `tests/release-artifacts.test.ts`. Add `BUNDLE_KINDS` to the imports.

```ts
describe('the app downloads are described where a reader meets them', () => {
  const readme = read('README.md');
  const notes = read('docs/releases/0.6.0.md');
  const site = read('site/index.html');
  const names = BUNDLE_KINDS.flatMap((k) =>
    (['x64', 'arm64'] as const).map((a) => k.releasedName('0.6.0', a)),
  );

  test('the README names every file it tells people to download', () => {
    // Read off the table, so a renamed artifact fails here rather than
    // leaving the README pointing at a file the release does not carry.
    for (const name of new Set(names)) expect(readme).toContain(name);
  });

  test('the README says the Windows installer is unsigned, in the same words as the CLI note', () => {
    const lower = readme.toLowerCase();
    expect(lower).toContain('smartscreen');
    expect(/not signed|unsigned/.test(lower)).toBe(true);
    // E1 already wrote this paragraph for the CLI. Two differently-worded
    // warnings on one page read as two different problems.
    expect(lower).toContain('more info');
    expect(lower).toContain('run anyway');
  });

  test('the README does not claim the installer works fully offline', () => {
    // NSIS's default webviewInstallMode downloads the WebView2 bootstrapper
    // when the machine has none. Windows 11 ships it; Windows 10 may not.
    // The APP still makes no network requests and that claim stands -- but
    // the INSTALLER sentence has to be precise enough not to be caught out.
    const section = readme.slice(readme.indexOf('### Desktop app'));
    expect(section.toLowerCase()).toContain('webview2');
  });

  test('the README says which of these artifacts CI actually executed', () => {
    const lower = readme.toLowerCase();
    // The x86_64 DMG is cross-compiled on an arm64 runner and its engine is
    // never run. Saying so is the difference between a limitation and a
    // surprise.
    expect(/intel mac|x86_64|x64/.test(lower)).toBe(true);
    expect(lower).toContain('never been installed');
  });

  test('the 0.6.0 notes carry the same three limits', () => {
    const lower = notes.toLowerCase();
    expect(/not signed|unsigned/.test(lower)).toBe(true);
    expect(lower).toContain('smartscreen');
    expect(lower).toContain('never been installed');
    // E1's limits are still there and were not overwritten.
    expect(lower).toContain('tolino');
  });

  test('the notes name the app downloads too, not only the CLI ones', () => {
    for (const name of new Set(names)) expect(notes).toContain(name);
  });

  test('the download page carries the unsigned-Windows warning', () => {
    // site/index.html offered only the macOS DMG before this piece, so this
    // is a new section rather than an edited one.
    const lower = site.toLowerCase();
    expect(lower).toContain('smartscreen');
    expect(/not signed|unsigned/.test(lower)).toBe(true);
  });

  test('the site still offers the SwiftUI DMG as the supported Mac download', () => {
    // Two Mac downloads on one page will confuse somebody. The mitigation
    // is that the page says which one is supported; piece F is the real fix.
    expect(site).toContain('Screepub-macOS.dmg');
    expect(site.toLowerCase()).toContain('supported');
  });

  test('nothing anywhere promises an AppImage, a cask, winget or the AUR', () => {
    // All four are out of scope, three of them deferred with reasons and
    // one ruled out on merits. A promise in prose is a promise.
    for (const text of [readme, notes, site]) {
      const lower = text.toLowerCase();
      expect(lower).not.toContain('appimage');
      expect(lower).not.toContain('winget');
      expect(lower).not.toContain('aur ');
    }
    // The Homebrew tap is still named -- it serves the SwiftUI app and the
    // macOS CLI, and that is unchanged -- but never beside the Tauri app.
    const around = readme.slice(readme.indexOf('### Desktop app'));
    expect(around.toLowerCase()).not.toContain('brew install');
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — none of the four documents mentions an app bundle.

- [ ] **Step 3: Rewrite the README's "Desktop app (in progress)" section**

Replace that section with the following. It sits after the existing `### Linux and Windows (command line)` section and before `### Development`.

```markdown
### Desktop app

From 0.6.0 there is a window as well as a command line, built on Tauri
around the same engine: the app spawns the engine binary and renders its
`--json` answer, so there is exactly one implementation of everything that
thinks.

| Machine | File |
| --- | --- |
| Linux, Debian or Ubuntu, Intel or AMD | `Screepub_0.6.0_amd64.deb` |
| Linux, Debian or Ubuntu, ARM | `Screepub_0.6.0_arm64.deb` |
| Linux, Fedora or openSUSE, Intel or AMD | `Screepub-0.6.0-1.x86_64.rpm` |
| macOS, Apple Silicon | `Screepub-Desktop-macOS-arm64.dmg` |
| macOS, Intel | `Screepub-Desktop-macOS-x64.dmg` |
| Windows, 64-bit | `Screepub-0.6.0-setup.exe` |

```bash
sudo apt install ./Screepub_0.6.0_amd64.deb     # Debian, Ubuntu
sudo dnf install ./Screepub-0.6.0-1.x86_64.rpm  # Fedora, openSUSE
```

`SHA256SUMS-app` on the release page covers these six files. (`SHA256SUMS`,
beside it, covers the three command-line downloads.)

**On a Mac, `Screepub-macOS.dmg` is still the supported download.** It
installs `Screepub.app` and it is the one this project has been shipping.
The two `Screepub-Desktop-macOS-*.dmg` files are the new cross-platform app;
they install `Screepub Desktop.app`, so both can sit in Applications at
once, and they share the same `~/Documents/Screepub/` library. When the new
app replaces the old one, that name goes back to `Screepub.app`.

**Windows will warn you.** Screepub for Windows is unsigned — it carries no
code-signing certificate — so the first time you run the installer,
SmartScreen shows a blue "Windows protected your PC" screen naming an
unknown publisher. Choose **More info**, then **Run anyway**. Windows
Defender may also hold the download briefly. Certificates cost money this
project does not spend yet; this note exists so the warning is expected
rather than alarming.

**The Windows installer may need the network once.** It installs Microsoft's
WebView2 runtime if the machine has none — Windows 11 ships it, Windows 10
may not — and fetches it from Microsoft at install time. Converting itself
never touches the network, on any platform, and never has.

**Nobody has installed these yet.** Every app bundle is built by CI, and CI
opens each one and runs the engine out of it before publishing — which
catches a broken payload, and does not catch anything a person would notice
about the window. No `.deb`, `.rpm`, `.dmg` or installer has been installed
on a real machine by this project. The Intel Mac DMG is the least proven of
the six: it is cross-compiled on an Apple Silicon runner, so not even its
engine has been executed anywhere. Treat 0.6.0's app downloads as a first
release that wants your bug reports.
```

- [ ] **Step 4: Add the app downloads to `docs/releases/0.6.0.md`**

Under `## Screepub runs where you work`, replace the first bullet and add a second:

```markdown
- **Linux and Windows downloads.** The converter ships as one file for Linux,
  on both Intel and ARM, and for Windows. Unpack it, run it, convert a script.
  There is nothing else to install.
- **And there is a window now, on all three.** `Screepub_0.6.0_amd64.deb`,
  `Screepub_0.6.0_arm64.deb` and `Screepub-0.6.0-1.x86_64.rpm` for Linux,
  `Screepub-Desktop-macOS-arm64.dmg` and `Screepub-Desktop-macOS-x64.dmg`
  for macOS, and `Screepub-0.6.0-setup.exe` for Windows. `SHA256SUMS-app`
  covers all six.
- **The Mac app has not changed.** `Screepub-macOS.dmg` is the same download,
  the same signing by Apple, and the same behaviour, and it is still the one
  we support on a Mac. The new `Screepub Desktop` installs beside it and
  shares the same library folder, so you can try one without losing the
  other.
```

And under `## Good to know`, extend the Windows bullet and add one more:

```markdown
- **Windows will warn you the first time you run it.** Both the command-line
  download and the installer are unsigned, so Windows shows a "publisher
  unknown" SmartScreen screen. Choose More info, then Run anyway. A
  certificate costs money this project does not spend yet, and we would
  rather tell you than let you meet that screen alone. The installer also
  fetches Microsoft's WebView2 runtime if your machine has none, which is the
  one moment Screepub needs the network; converting never does.
- **The app downloads have never been installed by anyone.** Our automation
  builds each one, opens it, and runs the converter out of it before
  publishing — so we know the engine inside is intact. Nobody has run the
  installer on a real machine and clicked through the window. The Intel Mac
  build is the least proven: it is compiled on an Apple Silicon machine, so
  not even its converter has been executed. If something is wrong, a bug
  report is genuinely the fastest way we will find out.
```

- [ ] **Step 5: Add a downloads section to `site/index.html`**

The page currently offers only `DOWNLOAD FOR MACOS`, three times. Add one section, after the first download button block, using the page's existing class names:

```html
      <section class="downloads" id="all-downloads">
        <h2>Every download</h2>
        <p>
          <strong>On a Mac, the supported download is
          <code>Screepub-macOS.dmg</code></strong> &mdash; signed and
          notarized by Apple, and the one this project has been shipping.
        </p>
        <ul>
          <li>Linux (Debian, Ubuntu): <code>Screepub_0.6.0_amd64.deb</code>,
            <code>Screepub_0.6.0_arm64.deb</code></li>
          <li>Linux (Fedora, openSUSE): <code>Screepub-0.6.0-1.x86_64.rpm</code></li>
          <li>macOS, the new cross-platform app:
            <code>Screepub-Desktop-macOS-arm64.dmg</code>,
            <code>Screepub-Desktop-macOS-x64.dmg</code></li>
          <li>Windows: <code>Screepub-0.6.0-setup.exe</code></li>
          <li>Command line, any platform: see the
            <a href="https://github.com/method-production-suite/method-reader#readme">README</a></li>
        </ul>
        <p>
          <strong>Windows will warn you.</strong> Screepub for Windows is
          unsigned &mdash; it carries no code-signing certificate &mdash; so
          SmartScreen shows a blue &ldquo;Windows protected your PC&rdquo;
          screen naming an unknown publisher. Choose <strong>More info</strong>,
          then <strong>Run anyway</strong>. Certificates cost money this
          project does not spend yet; this note exists so the warning is
          expected rather than alarming.
        </p>
        <p>
          <strong>Nobody has installed the app downloads yet.</strong> Our
          automation builds each one and runs the converter out of it before
          publishing; no one has run an installer on a real machine.
        </p>
        <p><a href="https://github.com/method-production-suite/method-reader/releases/latest">All files on the latest release &rarr;</a></p>
      </section>
```

If `site/index.html` has no `.downloads` style, add a minimal rule beside the existing ones rather than importing anything: the page ships as one static file and has no build step.

- [ ] **Step 6: Add the verification ledger to `desktop/README.md`**

Append, as the final section of the file:

```markdown
## What nobody has verified

The honest version of this app's status, kept here so that the next person
does not have to infer it from a green checkmark.

**Verified on a real machine, by a person:**

- The Linux `.deb` and `.rpm` build, and both contain the engine, the
  licence, the third-party notices and a `.desktop` entry declaring
  `MimeType=application/pdf`.
- The engine runs straight out of both and converts
  `tests/fixtures/screenplay.pdf`.
- The window opens, converts a PDF and shows a result — on Linux, from
  `cargo run`. See "Running it: what a good run looks like" above.

**Verified only by CI, and only as far as CI can reach:**

- That the shell compiles on macOS and Windows at all.
- That a `.dmg` and an NSIS installer can be produced.
- That the engine inside the Linux bundles, the arm64 `.dmg` and the Windows
  installer runs and converts the fixture. CI opens each bundle without
  installing it.

**Verified by nobody:**

- Installing any of these. No `.deb`, `.rpm`, `.dmg` or `.exe` has been
  installed on a real machine.
- The window, on macOS or on Windows. No runner has a display; the GUI half
  of the app has never been exercised off Linux.
- The Intel Mac `.dmg`. It is cross-compiled on an Apple Silicon runner, so
  its engine is not even executed in CI — it is built, verified as a
  container, and published.
- Gatekeeper actually accepting the notarized bundle, and SmartScreen
  actually showing the screen the README describes.

No sentence in `README.md`, `site/index.html` or the release notes may move
an item up this list without someone doing the thing.
```

- [ ] **Step 7: Run the tests**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS.

- [ ] **Step 8: Read the four documents once, as a stranger**

Not a command — a step. Open `README.md`, `docs/releases/0.6.0.md`, `site/index.html` and `desktop/README.md` and check three things a test cannot:

1. Does any sentence say a platform "works" where the ledger says nobody has run it? Fix the sentence, not the ledger.
2. Are the two Windows warnings (the CLI one E1 wrote, the installer one this task wrote) consistent in wording? Two differently-worded warnings read as two different problems.
3. Does the Mac section make it clear which of the three `.dmg` files a reader should download? If a stranger could pick wrong, rewrite until they cannot.

- [ ] **Step 9: Typecheck, run the whole suite, commit**

```bash
bunx tsc --noEmit
bun test
```

```
Say what ships, what warns, and what nobody has run

The README, the download page and the 0.6.0 notes now name all six app
downloads, carry the unsigned-Windows warning in E1's own words, say that
the Windows installer fetches WebView2 once (and that converting never
touches the network), and say plainly that nobody has installed any of
these -- with the Intel Mac DMG called out as the least proven, since it is
cross-compiled and its engine is executed nowhere.

desktop/README.md gains a three-part ledger: verified by a person, verified
only by CI, verified by nobody. The tests read the artifact names off
build-app-bundle.ts's table, so a renamed artifact fails a test rather than
leaving the README pointing at a file the release does not carry.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
```

---

## Self-review, run against the spec

**Spec coverage.** Every section of the design has a task:

| Spec section | Task |
| --- | --- |
| Artifacts table | 4 (names), 10 (what is published) |
| Linux — four config defects | 2, verified again in 6 |
| macOS — per-arch DMGs, secrets translation, coexistence | 9 (the overlay), 10 (the job) |
| Windows — one unsigned NSIS installer, the warning, WebView2 | 1 (the `.ico` that makes it buildable), 10, 11 |
| The version pin | 7 (`checks`), 4 (`assertBundleVersions`), 5 (the smoke assertion) |
| Tooling — `build-app-bundle.ts`, `smoke-bundle.ts`, the icon set | 4, 5, 1 |
| CI — `desktop.yml` extended, `release.yml`'s new jobs | 8, 10 |
| Testing — unit tests with fakes, the Linux half verified for real | 3, 4, 5 (unit), 6 (real) |
| Acceptance criteria 1–9 | 4, 5, 1, 2, 9+10, 7, 10, 11, all |
| Risks — two platforms unexercisable, first Windows run red, release size | 8 (the workflow header), 11 (the ledger) |

**Three things where this plan departs from the spec, each on evidence:**

1. **The `.deb`/`.rpm` readers are TypeScript, not `ar x` and `rpm2cpio | cpio -id`** (Task 3). `rpm2cpio` is not installed here and is not guaranteed on GitHub's ubuntu image; a smoke check that silently skips is worse than none.
2. **`Categories=Office;`, not `Office;Publishing;`** (Task 2). `bundle.category` is an enum and no member yields `Publishing;`; the alternative is a hand-maintained `.desktop` template that takes over four more fields on a surface nobody tests.
3. **Artifacts are discovered rather than named** (Task 4). The bundler's filename depends on `productName`, which Task 9's overlay changes; globbing with an exactly-one assertion is the fact, and it also survives piece F deleting the overlay.

**One spec instruction this plan makes stronger:** the spec offered "commit the expanded `Cargo.toml` spelling **or** run the diff checks before the bundle step". Task 8 does **both**, because committing the spelling is only a fixed point for the current `tauri.conf.json`, and Task 2 just changed that file.

**One gap this plan does not close, named rather than hidden:** `build-app-bundle.ts`'s renaming and checksum half is exercised by its unit tests and by the release run, and by nothing on the push path, because its version gate refuses to run while the three version files disagree — which they do, deliberately, on every working branch. Task 8's workflow comment says so.
