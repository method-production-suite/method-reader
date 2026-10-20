# Version-free download names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Linux and Windows installers publish under names without a version number, so every download link can be `releases/latest/download/<name>` and never go stale.

**Architecture:** One function decides every published installer name: `releasedName` on each row of `BUNDLE_KINDS` in `tools/build-app-bundle.ts`. Three rows change. A documentation test in `tests/release-artifacts.test.ts` derives the published names from that same function and requires the README to name them, so the README's download table and apt/dnf lines change in the same commit. The site's download list changes with it for consistency. Everything downstream (the release workflow's globs, the checksums count, the manifest builder, the smoke step) keys on file extensions, not names, and is unaffected.

**Tech Stack:** Bun + TypeScript (`bun test`, `bunx tsc --noEmit`).

**Spec:** Part 4 of [docs/superpowers/specs/2026-09-22-readme-site-screens-design.md](../specs/2026-09-22-readme-site-screens-design.md).

**First in the order of work,** because the README and site rewrites link to these names and must only do so once a real release has published them. Task 5 cuts that release.

---

## What changes

| Row | Today | After |
| --- | --- | --- |
| `deb`, x64 | `Screepub_<v>_amd64.deb` | `Screepub-linux-amd64.deb` |
| `deb`, arm64 (hand-built only) | `Screepub_<v>_arm64.deb` | `Screepub-linux-arm64.deb` |
| `rpm`, x64 | `Screepub-<v>-1.x86_64.rpm` | `Screepub-linux-x86_64.rpm` |
| `rpm`, arm64 (hand-built only) | `Screepub-<v>-1.aarch64.rpm` | `Screepub-linux-aarch64.rpm` |
| `nsis` | `Screepub-<v>-setup.exe` | `Screepub-windows-x64-setup.exe` |
| `dmg` | `Screepub-Desktop-macOS-<arch>.dmg` | unchanged |

The architecture words follow each format's own convention (`amd64`/`arm64` for Debian, `x86_64`/`aarch64` for RPM), so a Linux user sees the spelling their package tools use.

**Not changed, and why:**
- The bundler's own output names (`Screepub_0.6.0_arm64.deb` and so on, inside `target/.../bundle/`). `discoverArtifact` finds those by extension, and tests at `tests/build-app-bundle.test.ts:243-244` use them as bundler output. Leave them.
- `tests/release-app-step.test.ts` and `tests/build-update-manifest.test.ts` create files with the old names as arbitrary fixtures. Their behaviour does not depend on the names. Leave them.
- `desktop/README.md` and the 0.6.0 release notes name the old files as history. Leave them.

---

### Task 1: The naming tests say "no version"

**Files:**
- Modify: `tests/build-app-bundle.test.ts:107-129` (the naming test)
- Modify: `tests/build-app-bundle.test.ts:560-563`, `:573-576`, `:609-612`, `:666` (whole-run expectations)

- [ ] **Step 1: Replace the naming test**

In `tests/build-app-bundle.test.ts`, replace the whole test that starts
`test('the published names carry the version and say which machine they are for', () => {`
(through its closing `});`, before `test('every published name ends in the extension its own row declares'`) with:

```ts
  test('the published names carry NO version, and say which machine they are for', () => {
    // CHANGED 2026-09-22, deliberately: this used to assert the version WAS
    // in each name. Every page that named a file went stale at the next
    // release, which is why the README and the site both still said 0.6.0
    // at 0.7.1. Without the version, a link can be
    // releases/latest/download/<name> and stay right forever. The version
    // still lives INSIDE each package (the deb control file, the rpm
    // header, the NSIS product version), so package tools show it.
    // Architecture words follow each format's own convention.
    expect(kind('deb').releasedName('0.6.0', 'x64')).toBe('Screepub-linux-amd64.deb');
    expect(kind('deb').releasedName('0.6.0', 'arm64')).toBe('Screepub-linux-arm64.deb');
    expect(kind('rpm').releasedName('0.6.0', 'x64')).toBe('Screepub-linux-x86_64.rpm');
    expect(kind('rpm').releasedName('0.6.0', 'arm64')).toBe('Screepub-linux-aarch64.rpm');
    expect(kind('nsis').releasedName('0.6.0', 'x64')).toBe('Screepub-windows-x64-setup.exe');
    // The two Mac names must not collide with the SwiftUI app's
    // Screepub-macOS.dmg, which app/release.sh uploads to the same release
    // page and tools/bump-tap.sh hardcodes.
    expect(kind('dmg').releasedName('0.6.0', 'arm64')).toBe('Screepub-Desktop-macOS-arm64.dmg');
    expect(kind('dmg').releasedName('0.6.0', 'x64')).toBe('Screepub-Desktop-macOS-x64.dmg');
    // The one a release should actually ship. The frozen Swift updater takes
    // the first .dmg on a release and has no architecture logic, so per-arch
    // Mac bundles are what make an automatic migration impossible (ADR
    // 2026-09-14). This name is how the release stops being per-arch.
    expect(kind('dmg').releasedName('0.6.0', 'universal'))
      .toBe('Screepub-Desktop-macOS-universal.dmg');
    for (const arch of ['x64', 'arm64'] as const) {
      expect(kind('dmg').releasedName('0.6.0', arch)).not.toBe('Screepub-macOS.dmg');
    }
  });

  test('no published name changes when the version does', () => {
    // The property the renaming exists for, stated directly rather than
    // implied by five literals: a prerelease and a far-future release get
    // exactly the names 0.6.0 gets, and no name carries a version at all.
    for (const k of BUNDLE_KINDS) {
      for (const arch of ['x64', 'arm64', 'universal'] as const) {
        if (arch === 'universal' && k.id !== 'dmg') continue;
        const name = k.releasedName('0.6.0', arch);
        expect(k.releasedName('9.12.3-rc1', arch)).toBe(name);
        expect(name).not.toMatch(/\d+\.\d+\.\d+/);
      }
    }
  });
```

- [ ] **Step 2: Update the whole-run expectations**

In the test `'it renames to the published names and writes SHA256SUMS-app'`, replace:

```ts
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual([
      'Screepub_0.6.0_arm64.deb',
      'Screepub-0.6.0-1.aarch64.rpm',
    ]);
```

with:

```ts
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual([
      'Screepub-linux-arm64.deb',
      'Screepub-linux-aarch64.rpm',
    ]);
```

and, in the same test, replace:

```ts
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-0.6.0-1.aarch64.rpm',
      'Screepub_0.6.0_arm64.deb',
    ]);
```

with:

```ts
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-linux-aarch64.rpm',
      'Screepub-linux-arm64.deb',
    ]);
```

In the test `'the checksums file is NOT called SHA256SUMS, and names the x64 build'`, replace:

```ts
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-0.6.0-1.x86_64.rpm',
      'Screepub_0.6.0_amd64.deb',
    ]);
```

with:

```ts
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-linux-amd64.deb',
      'Screepub-linux-x86_64.rpm',
    ]);
```

In the test `'a truncated artifact fails the run instead of being published'`, replace:

```ts
    expect(() => readFileSync(join(out, 'Screepub_0.6.0_amd64.deb'))).toThrow();
```

with:

```ts
    expect(() => readFileSync(join(out, 'Screepub-linux-amd64.deb'))).toThrow();
```

- [ ] **Step 3: Run the tests and watch them fail for the right reason**

Run: `bun test tests/build-app-bundle.test.ts`
Expected: FAIL. The naming test fails on `Screepub_0.6.0_amd64.deb` versus `Screepub-linux-amd64.deb`, the new version test fails on a name containing `0.6.0`, and the three whole-run tests fail on the old names. Nothing else fails.

### Task 2: The release test derives the new names

**Files:**
- Modify: `tests/release-artifacts.test.ts:756-769`

- [ ] **Step 1: Update the derivation's expected lists**

In `tests/release-artifacts.test.ts`, in the test
`'the derivation found the four files the release actually uploads'`, replace:

```ts
    expect([...published].sort()).toEqual([
      'Screepub-0.6.0-1.x86_64.rpm',
      'Screepub-0.6.0-setup.exe',
      'Screepub-Desktop-macOS-universal.dmg',
      'Screepub_0.6.0_amd64.deb',
    ]);
```

with:

```ts
    expect([...published].sort()).toEqual([
      'Screepub-Desktop-macOS-universal.dmg',
      'Screepub-linux-amd64.deb',
      'Screepub-linux-x86_64.rpm',
      'Screepub-windows-x64-setup.exe',
    ]);
```

and replace:

```ts
    expect([...unpublished].sort()).toEqual([
      'Screepub-0.6.0-1.aarch64.rpm',
      'Screepub-Desktop-macOS-arm64.dmg',
      'Screepub-Desktop-macOS-x64.dmg',
      'Screepub_0.6.0_arm64.deb',
    ]);
```

with:

```ts
    expect([...unpublished].sort()).toEqual([
      'Screepub-Desktop-macOS-arm64.dmg',
      'Screepub-Desktop-macOS-x64.dmg',
      'Screepub-linux-aarch64.rpm',
      'Screepub-linux-arm64.deb',
    ]);
```

- [ ] **Step 2: Run it and watch it fail**

Run: `bun test tests/release-artifacts.test.ts -t "derivation found"`
Expected: FAIL, with the received list still showing the versioned names.

- [ ] **Step 3 (added during execution): pin the 0.6.0 notes as history**

The test `'the notes name the app downloads too, not only the CLI ones'`
derived the names 0.6.0's notes must contain from `BUNDLE_KINDS`. After
the rename it would demand that notes for an already-published release
name files that release never had. Published notes are never rewritten,
so it becomes `'the 0.6.0 notes named the app downloads 0.6.0 actually
published'`, asserting the four names 0.6.0 really shipped as literals:
`Screepub_0.6.0_amd64.deb`, `Screepub-0.6.0-1.x86_64.rpm`,
`Screepub-0.6.0-setup.exe` and `Screepub-Desktop-macOS-universal.dmg`.
This plan missed it; running the suite found it.

### Task 3: Rename in the one place names are decided

**Files:**
- Modify: `tools/build-app-bundle.ts` (the `deb`, `rpm` and `nsis` rows of `BUNDLE_KINDS`)

- [ ] **Step 1: Change the three `releasedName` functions**

In `tools/build-app-bundle.ts`, in the `deb` row, replace:

```ts
    releasedName: (v, arch) => `Screepub_${v}_${arch === 'x64' ? 'amd64' : 'arm64'}.deb`,
```

with:

```ts
    // No version in the name, on purpose (spec 2026-09-22, part 4): pages
    // link to releases/latest/download/<name>, which a versioned name breaks
    // at every release. The version is still inside the package.
    releasedName: (_v, arch) => `Screepub-linux-${arch === 'x64' ? 'amd64' : 'arm64'}.deb`,
```

In the `rpm` row, replace:

```ts
    releasedName: (v, arch) => `Screepub-${v}-1.${arch === 'x64' ? 'x86_64' : 'aarch64'}.rpm`,
```

with:

```ts
    releasedName: (_v, arch) => `Screepub-linux-${arch === 'x64' ? 'x86_64' : 'aarch64'}.rpm`,
```

In the `nsis` row, replace:

```ts
    releasedName: (v) => `Screepub-${v}-setup.exe`,
```

with:

```ts
    releasedName: () => 'Screepub-windows-x64-setup.exe',
```

The leading underscore on `_v` is required: `tsconfig.json` sets `noUnusedParameters`, and the `dmg` row already uses `_v` for the same reason.

- [ ] **Step 2: Run both test files**

Run: `bun test tests/build-app-bundle.test.ts tests/release-artifacts.test.ts`
Expected: `tests/build-app-bundle.test.ts` passes completely. `tests/release-artifacts.test.ts` now fails only `'the README names every file it tells people to download'`, because the README still names the versioned files. That failure is Task 4's job.

### Task 4: The README and the site name the new files

**Files:**
- Modify: `README.md:149-160` (the desktop table and install commands)
- Modify: `site/index.html:470-475` (the download list)

- [ ] **Step 1: Update the README's desktop table and commands**

In `README.md`, replace:

```markdown
| Machine | File |
| --- | --- |
| Linux, Debian or Ubuntu, Intel or AMD | `Screepub_0.6.0_amd64.deb` |
| Linux, Fedora or openSUSE, Intel or AMD | `Screepub-0.6.0-1.x86_64.rpm` |
| macOS, Apple Silicon or Intel | `Screepub-Desktop-macOS-universal.dmg` |
| Windows, 64-bit | `Screepub-0.6.0-setup.exe` |

```bash
sudo apt install ./Screepub_0.6.0_amd64.deb     # Debian, Ubuntu
sudo dnf install ./Screepub-0.6.0-1.x86_64.rpm     # Fedora
sudo zypper install ./Screepub-0.6.0-1.x86_64.rpm  # openSUSE
```
```

with:

```markdown
| Machine | File |
| --- | --- |
| Linux, Debian or Ubuntu, Intel or AMD | `Screepub-linux-amd64.deb` |
| Linux, Fedora or openSUSE, Intel or AMD | `Screepub-linux-x86_64.rpm` |
| macOS, Apple Silicon or Intel | `Screepub-Desktop-macOS-universal.dmg` |
| Windows, 64-bit | `Screepub-windows-x64-setup.exe` |

These names carry no version from 0.7.2 on, so they are the same at every
release. Releases up to 0.7.1 put the version in the Linux and Windows
names.

```bash
sudo apt install ./Screepub-linux-amd64.deb      # Debian, Ubuntu
sudo dnf install ./Screepub-linux-x86_64.rpm     # Fedora
sudo zypper install ./Screepub-linux-x86_64.rpm  # openSUSE
```
```

- [ ] **Step 2: Update the site's download list**

In `site/index.html`, replace:

```html
      <li>Linux (Debian, Ubuntu): <code>Screepub_0.6.0_amd64.deb</code></li>
      <li>Linux (Fedora, openSUSE): <code>Screepub-0.6.0-1.x86_64.rpm</code></li>
```

with:

```html
      <li>Linux (Debian, Ubuntu): <code>Screepub-linux-amd64.deb</code></li>
      <li>Linux (Fedora, openSUSE): <code>Screepub-linux-x86_64.rpm</code></li>
```

and replace:

```html
      <li>Windows: <code>Screepub-0.6.0-setup.exe</code></li>
```

with:

```html
      <li>Windows: <code>Screepub-windows-x64-setup.exe</code></li>
```

The site's list becomes real links in the site plan (spec part 2). This task only stops it naming files that no longer exist.

- [ ] **Step 3: Run the release test file**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS, every test.

### Task 5: Verify everything, commit, and ship the names in 0.7.2

**Files:** none new.

- [ ] **Step 1: Run the whole suite and the typecheck**

Run: `bun test`
Expected: all pass, 0 fail.

Run: `bunx tsc --noEmit`
Expected: no output.

- [ ] **Step 2: Check nothing still produces or names a versioned installer**

Run:

```bash
git grep -n -E 'Screepub_\$\{|Screepub-\$\{' -- tools/
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add tools/build-app-bundle.ts tests/build-app-bundle.test.ts tests/release-artifacts.test.ts README.md site/index.html
git commit -m "Download names stop carrying the version

The Linux and Windows installers published as Screepub_<v>_amd64.deb,
Screepub-<v>-1.x86_64.rpm and Screepub-<v>-setup.exe, so every page that
named them went stale at the next release: the README and the site both
still said 0.6.0 at 0.7.1. They are now Screepub-linux-amd64.deb,
Screepub-linux-x86_64.rpm and Screepub-windows-x64-setup.exe, the way the
Mac downloads already were, so a link can be
releases/latest/download/<name> and stay right. The version still lives
inside each package.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Push, then cut 0.7.2 promptly**

GitHub renders the README from `main` the moment it lands, so between this push and the 0.7.2 release the README names files the newest release does not yet carry. Keep that window short: push, then run `/release` for 0.7.2 straight away. The 0.7.2 notes should say, in the reader's words, that the Linux and Windows downloads have new names that will not change again.

0.7.2 is also the release that proves the updater: a Mac running 0.7.1's window with updates switched on should find it, download it, verify it and install it. Watch for that on the owner's Mac once 0.7.2 publishes.

- [ ] **Step 5: Confirm the release carries the new names**

Run:

```bash
gh release view v0.7.2 --repo method-production-suite/method-reader --json assets -q '.assets[].name' | sort
```

Expected: the list includes `Screepub-linux-amd64.deb`, `Screepub-linux-x86_64.rpm` and `Screepub-windows-x64-setup.exe`, and no asset name contains `0.7.2`. The CLI archives and the DMGs were already version-free.

Run:

```bash
curl -sIL https://github.com/method-production-suite/method-reader/releases/latest/download/Screepub-linux-amd64.deb | grep -i '^HTTP' | tail -1
```

Expected: `HTTP/2 200`. That is the property the whole change exists for.
