# Cross-Platform CLI Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the command-line converter to Linux and Windows, as release artifacts built beside the existing macOS ones and never in place of them. Bun cross-compiles every target from one machine, so this is an **additive** change: three new artifacts, one new build tool, one new smoke tool, four new workflow jobs. The macOS binaries, the DMG, the Homebrew tap and everything under `app/` are untouched.

**Architecture:** Two Bun/TypeScript tools carry all the logic, and the YAML only calls them. `tools/build-cli.ts` holds the target table, the cross-compile, the packaging (tar.gz via system `tar`, zip via the already-vendored `jszip`), a **byte-level binary-format check** that reads ELF/PE/Mach-O magic rather than shelling out to `file`, the artifact verification, and `SHA256SUMS`. `tools/smoke-cli.ts` holds what a per-OS job must prove about a binary it just downloaded — `--version` agrees with `package.json`, the committed fixture converts to a real EPUB, and `devices --json` starts. Both split into small pure functions with injected seams (a `Spawn`, a `Runner`, a `Floors` record), so every branch is a cheap unit test and the two slow things — a real cross-compile and a real conversion — are exercised once, in one end-to-end test. Workflow YAML is asserted against by parsing it with `Bun.YAML.parse` and checking the **job graph**, not by grepping prose.

**Tech Stack:** TypeScript, Bun (`bun:test`, `Bun.spawnSync`, `Bun.gunzipSync`, `Bun.YAML`), Node built-ins (`node:fs`, `node:path`, `node:crypto`, `node:util`'s `parseArgs`), `jszip` (already a dependency), system `tar`. No new dependencies.

**Spec:** [`docs/superpowers/specs/2026-09-13-cross-platform-cli-release-design.md`](../specs/2026-09-13-cross-platform-cli-release-design.md)
**Program ADR:** [`docs/adr/2026-09-12-cross-platform-tauri.md`](../../adr/2026-09-12-cross-platform-tauri.md)
**Follows:** [piece B — CLI device commands](2026-09-13-cli-device-commands.md)

## Global Constraints

- **The macOS release path is untouchable.** `app/release.sh` signs and notarizes the macOS CLI binaries and `tools/bump-tap.sh` hardcodes their filenames. Do not move, rename, rebuild or re-package them, and do not add a macOS target to the new builder. Verified two ways: permanent tests on the artifact names (Task 12), and a byte-for-byte digest check in Final Verification.
- **No file under `app/` may be modified.** Verify with `git status --short app/` before every commit; it must print nothing.
- **No file under `tools/bump-tap.sh` or `tools/check-tap.sh` may be modified.** Same check, same reason: the tap has its own freshness alarm and no test here can catch a tap that stops installing.
- **Nothing asserts a property a plausible wrong implementation could still satisfy.** In particular: a test that a build produced a file is worthless, because a 0-byte file or an x86-64 binary named `linux-arm64` would pass it. Every artifact assertion checks the **format magic bytes**, a **non-trivial size floor**, and — for archives — that the member is present under the right name, at the right size, in the right format, with its executable bit set.
- **No test commits or leaves a build artifact in the repo.** Artifacts are 39–119 MB each. Every test builds into `mkdtempSync(join(tmpdir(), …))` and removes it in `afterAll`. `--out` on `build-cli.ts` is **required with no default**, so nothing can write 250 MB somewhere nobody asked for. `.gitignore` coverage of `dist/` and `build/` is pinned by a test (Task 9) for the hand-run case.
- **`tests/fixture-stability.test.ts` and `tests/fixtures/` are not touched.** The committed fixtures keep reproducing byte-for-byte; `screenplay.pdf` is read by the smoke path and never rewritten.
- **No conversion changes of any kind.** The parser, the renderers, `convert.ts`, `options.ts` and `src/epub/css.ts` are untouched. Nothing here is a formatting behavior, so `docs/formatting-options-log.md` is not updated.
- **`package.json`'s version is NOT bumped by this piece.** `screepub --version` reports `package.json` inlined at compile time, so `build-cli.ts` *verifies* the two agree and refuses to build when they don't — but choosing when 0.6.0 ships is the release's job, not this piece's.
- **Release notes must never contain a checksum or the word for one.** `release.yml`'s `checks` job greps the notes for `sha-?256` and 64-hex runs and fails the tag. Task 15 adds that rule to the in-suite banned list so it fails in nine seconds instead of after notarization.
- **`tsconfig.json` includes only `src` and `tests`.** The new tools are typechecked because the tests import them; that is deliberate and no tsconfig change is needed. `verbatimModuleSyntax` is on, so type-only imports must be spelled `import type`.
- **Existing suite stays green:** 799 tests pass (3 skipped) and `bunx tsc --noEmit` is clean after every task.

## What rides untested until the next tag

Stated plainly, because the spec asks for it and because the honest list is short:

- The **release.yml job graph** itself — that `cross-cli` runs after `checks`, that the two smoke jobs find the uploaded artifact, that `cross-upload` attaches four assets to an already-published release. Task 14's tests parse the YAML and assert the graph, which catches a typo'd `needs:` or a renamed asset, but nothing local can prove GitHub schedules it.
- **`actions/upload-artifact` / `actions/download-artifact`** round-tripping 250 MB.
- **`gh release upload` against a published release**, including `GH_REPO` standing in for a checkout.
- **`Expand-Archive` on windows-latest**, and the Windows runner's `bun` invoking `tools/smoke-cli.ts`.
- **The Windows binary ever executing.** Nothing on this project has a Windows machine. The release run's smoke job is the first execution that will ever happen, which is exactly why it exists and why it gates `cross-upload`.
- **linux-arm64 is built but never smoke-tested** in CI: no arm64 runner is assumed available. It is the maintainer's own daily architecture and Task 11 runs it for real on this machine when the host is arm64 Linux, but that is a developer machine, not the release. The notes say so.

## File Structure

**Created:**

| File | Responsibility |
|---|---|
| `tools/build-cli.ts` | The build matrix. Target table, cross-compile, packaging, binary-format detection, artifact verification, `SHA256SUMS`. Runnable by hand. |
| `tools/smoke-cli.ts` | What a per-OS job proves about a downloaded binary: version, a real conversion, `devices --json`. Dependency-free so a bare Windows runner can run it. |
| `tests/build-cli.test.ts` | Unit tests for every function in `build-cli.ts`, with crafted bytes and injected seams. |
| `tests/build-cli-e2e.test.ts` | One real cross-compile of two targets, packaged and verified with the production floors, smoke-run when the host can execute one of them. |
| `tests/smoke-cli.test.ts` | The smoke assertions against canned CLI output — the pass case and every failure mode. |
| `tests/release-artifacts.test.ts` | The release machinery's invariants: macOS artifact names, the tap script's hardcoded names, the workflow job graph, SHA-pinned actions. |
| `docs/releases/0.6.0.md` | Reader-facing notes carrying the two honesty statements. |

**Modified:**

| File | Change |
|---|---|
| `.github/workflows/ci.yml` | A `cross-cli` job: build all three targets on every push, and run the linux-x64 one. |
| `.github/workflows/release.yml` | Four jobs added — `cross-cli`, `smoke-linux-x64`, `smoke-windows-x64`, `cross-upload`. The `checks`, `release`, `tap` and `tap-check` jobs are not edited. |
| `README.md` | Linux and Windows install, the unsigned-Windows warning, the unproven-hardware caveat. |
| `tests/release-notes.test.ts` | `sha256` and `sha-256` join the banned-term list. |

**Untouched, and verified so:** everything under `app/`, `tools/bump-tap.sh`, `tools/check-tap.sh`, `.github/workflows/tap-freshness.yml`, `tests/fixture-stability.test.ts`, `tests/fixtures/`.

---

### Task 1: Binary format from the magic bytes

The whole piece rests on this. Everything downstream ("the artifact is a genuine aarch64 ELF") is only as strong as the function that decides it, and the obvious alternative — shelling out to `file` — is not available on a Windows runner, is not guaranteed in a bare container, and returns prose. Reading the header is four lines per format and exact.

The offsets below were confirmed against real `bun build --compile` output for all four targets: ELF `e_machine` at `0x12` is `0x3e` (x86-64) and `0xb7` (aarch64); the Windows build is `MZ` with its PE header at `0x78` and machine `0x8664`; the darwin build is `cf fa ed fe` with cputype `0x0100000c`. Mach-O is recognised even though nothing here builds it, because Task 12 asserts the builder never *emits* one and a detector that could not name it would make that assertion vacuous.

**Files:**
- Create: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (create)

**Interfaces:**
- Produces:
  ```ts
  export type BinaryFormat =
    | 'elf-x86-64' | 'elf-aarch64' | 'pe-x86-64'
    | 'macho-arm64' | 'macho-x86-64' | 'unknown';
  export function detectBinaryFormat(head: Uint8Array): BinaryFormat;
  export function readBinaryFormat(path: string): BinaryFormat;
  ```

- [ ] **Step 1: Write the failing test**

Create `tests/build-cli.test.ts`:

```ts
import { describe, test, expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { detectBinaryFormat, readBinaryFormat } from '../tools/build-cli';

/** A header with the exact bytes a real executable of that shape carries,
 *  and zeros elsewhere. Built by hand rather than by copying a 100 MB
 *  artifact into the repo. */
function elfHeader(machine: number): Uint8Array {
  const b = new Uint8Array(256);
  b.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0], 0); // magic, 64-bit, LE, SysV
  new DataView(b.buffer).setUint16(0x12, machine, true);
  return b;
}

function peHeader(machine: number, peOffset = 0x78): Uint8Array {
  const b = new Uint8Array(256);
  b.set([0x4d, 0x5a], 0); // 'MZ'
  new DataView(b.buffer).setUint32(0x3c, peOffset, true);
  b.set([0x50, 0x45, 0x00, 0x00], peOffset); // 'PE\0\0'
  new DataView(b.buffer).setUint16(peOffset + 4, machine, true);
  return b;
}

function machoHeader(cputype: number): Uint8Array {
  const b = new Uint8Array(256);
  const v = new DataView(b.buffer);
  v.setUint32(0, 0xfeedfacf, true);
  v.setUint32(4, cputype, true);
  return b;
}

describe('detectBinaryFormat', () => {
  test('names each format this project can produce', () => {
    expect(detectBinaryFormat(elfHeader(0x3e))).toBe('elf-x86-64');
    expect(detectBinaryFormat(elfHeader(0xb7))).toBe('elf-aarch64');
    expect(detectBinaryFormat(peHeader(0x8664))).toBe('pe-x86-64');
    expect(detectBinaryFormat(machoHeader(0x0100000c))).toBe('macho-arm64');
    expect(detectBinaryFormat(machoHeader(0x01000007))).toBe('macho-x86-64');
  });

  test('the two ELF architectures are never confused for each other', () => {
    // The whole point of the check. An implementation that stopped at the
    // 0x7f454c46 magic would pass every "is it an ELF" question and ship an
    // x86-64 binary in the arm64 tarball.
    expect(detectBinaryFormat(elfHeader(0x3e))).not.toBe('elf-aarch64');
    expect(detectBinaryFormat(elfHeader(0xb7))).not.toBe('elf-x86-64');
    expect(detectBinaryFormat(elfHeader(0x28))).toBe('unknown'); // 32-bit ARM
  });

  test('a 32-bit or big-endian ELF is not accepted as one of ours', () => {
    const b32 = elfHeader(0x3e);
    b32[4] = 1; // EI_CLASS = 32-bit
    expect(detectBinaryFormat(b32)).toBe('unknown');
    const be = elfHeader(0x3e);
    be[5] = 2; // EI_DATA = big endian
    expect(detectBinaryFormat(be)).toBe('unknown');
  });

  test('an MZ stub that is not a PE, or is the wrong machine, is unknown', () => {
    const notPe = peHeader(0x8664);
    notPe[0x78] = 0x00; // break the 'PE\0\0' signature
    expect(detectBinaryFormat(notPe)).toBe('unknown');
    expect(detectBinaryFormat(peHeader(0x014c))).toBe('unknown'); // i386
    expect(detectBinaryFormat(peHeader(0xaa64))).toBe('unknown'); // arm64
  });

  test('empty, short and garbage input is unknown, never a crash', () => {
    // A 0-byte output file is the single most likely compiler failure, and
    // it must NOT read as a valid anything.
    expect(detectBinaryFormat(new Uint8Array(0))).toBe('unknown');
    expect(detectBinaryFormat(new Uint8Array([0x7f, 0x45]))).toBe('unknown');
    expect(detectBinaryFormat(new Uint8Array(64))).toBe('unknown');
    expect(detectBinaryFormat(new TextEncoder().encode('#!/bin/sh\nexit 0\n'))).toBe('unknown');
    // A PE whose header offset points past the bytes we read.
    expect(detectBinaryFormat(peHeader(0x8664, 0xfffff0))).toBe('unknown');
  });
});

describe('readBinaryFormat', () => {
  test('reads the format off disk, including a 0-byte file', () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-fmt-'));
    try {
      writeFileSync(join(dir, 'arm'), elfHeader(0xb7));
      writeFileSync(join(dir, 'empty'), new Uint8Array(0));
      expect(readBinaryFormat(join(dir, 'arm'))).toBe('elf-aarch64');
      expect(readBinaryFormat(join(dir, 'empty'))).toBe('unknown');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `tools/build-cli.ts` does not exist.

- [ ] **Step 3: Write minimal implementation**

Create `tools/build-cli.ts`:

```ts
// Cross-compile, package and VERIFY the Linux and Windows CLI artifacts.
//
// A Bun script and not shell, because a build matrix in bash is exactly the
// platform-locked tooling the cross-platform ADR is retiring, and because a
// release tool nobody can run locally is a release tool nobody can debug.
//
// It never produces a macOS artifact. Those are built, signed and notarized
// by app/release.sh on a macOS runner, and tools/bump-tap.sh hardcodes their
// names; a cross-compiled Mach-O could not be notarized from here, so moving
// that build would trade a clean download for a Gatekeeper warning.
//
//   bun tools/build-cli.ts --version 0.6.0 --out dist/
//   bun tools/build-cli.ts --version 0.6.0 --out dist/ --only windows-x64

import { closeSync, openSync, readSync } from 'node:fs';

export type BinaryFormat =
  | 'elf-x86-64'
  | 'elf-aarch64'
  | 'pe-x86-64'
  | 'macho-arm64'
  | 'macho-x86-64'
  | 'unknown';

/** How many leading bytes are enough to name every format below. A real
 *  PE header sits at 0x78 in bun's output; 4096 leaves room to spare. */
const HEAD_BYTES = 4096;

/** Name the executable format from its header.
 *
 *  Shelling out to `file` was the alternative and is not usable: it does not
 *  exist on a Windows runner, is not guaranteed in a container, and answers
 *  in prose. These offsets are the on-disk ABI and were confirmed against
 *  real `bun build --compile` output for all four targets. */
export function detectBinaryFormat(head: Uint8Array): BinaryFormat {
  const view = new DataView(head.buffer, head.byteOffset, head.byteLength);
  const u16 = (o: number): number => (o >= 0 && o + 2 <= head.length ? view.getUint16(o, true) : -1);
  const u32 = (o: number): number => (o >= 0 && o + 4 <= head.length ? view.getUint32(o, true) : -1);

  // ELF: 7f 'E' 'L' 'F', EI_CLASS=2 (64-bit), EI_DATA=1 (little endian),
  // then e_machine as a u16 at 0x12.
  if (head[0] === 0x7f && head[1] === 0x45 && head[2] === 0x4c && head[3] === 0x46) {
    if (head[4] !== 2 || head[5] !== 1) return 'unknown';
    const machine = u16(0x12);
    if (machine === 0x3e) return 'elf-x86-64';
    if (machine === 0xb7) return 'elf-aarch64';
    return 'unknown';
  }

  // PE: 'MZ', a u32 at 0x3c pointing at 'PE\0\0', machine as a u16 after it.
  if (head[0] === 0x4d && head[1] === 0x5a) {
    const off = u32(0x3c);
    if (off < 0 || off + 6 > head.length) return 'unknown';
    const isPe =
      head[off] === 0x50 && head[off + 1] === 0x45 && head[off + 2] === 0 && head[off + 3] === 0;
    if (!isPe) return 'unknown';
    return u16(off + 4) === 0x8664 ? 'pe-x86-64' : 'unknown';
  }

  // Mach-O 64-bit, little endian: magic 0xfeedfacf, cputype at 0x04.
  if (u32(0) === 0xfeedfacf) {
    const cpu = u32(4);
    if (cpu === 0x0100000c) return 'macho-arm64';
    if (cpu === 0x01000007) return 'macho-x86-64';
  }

  return 'unknown';
}

/** Read only the header, never the whole 100 MB file. Throws if the path
 *  does not exist; callers check existence first and say so themselves. */
export function readBinaryFormat(path: string): BinaryFormat {
  const fd = openSync(path, 'r');
  try {
    const head = new Uint8Array(HEAD_BYTES);
    const read = readSync(fd, head, 0, HEAD_BYTES, 0);
    return detectBinaryFormat(head.subarray(0, read));
  } finally {
    closeSync(fd);
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — the existing 799 plus 7 new, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
Name a binary's real architecture from its own header

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 2: The target table, and the paths around it

One table is the whole build matrix. It carries, per target, the `bun build --target=` name, the format its output must have, how it is packaged, what the binary is called inside the archive, and what the archive is called on the release page.

Two path facts have to be encoded rather than remembered. First, **`bun build --compile` appends `.exe` itself** for the Windows target: the outfile it is *given* must not carry the extension, and the file it *produces* does. Second, all three targets would otherwise compile to the same filename, so each gets its own build directory while the archives land flat in the output directory — which is what keeps `SHA256SUMS` free of path components.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export type TargetId = 'linux-x64' | 'linux-arm64' | 'windows-x64';
  export interface Target {
    id: TargetId;
    bunTarget: string;        // 'bun-linux-x64'
    format: BinaryFormat;     // what its output must be
    packaging: 'tar.gz' | 'zip';
    binaryName: string;       // 'screepub' | 'screepub.exe'
    archiveName: string;      // 'screepub-cli-linux-x64.tar.gz'
  }
  export const TARGETS: readonly Target[];
  export function buildDir(target: Target, outDir: string): string;
  export function compileOutfile(target: Target, outDir: string): string;
  export function binaryPath(target: Target, outDir: string): string;
  export function archivePath(target: Target, outDir: string): string;
  export function targetIdForHost(platform: string, arch: string): TargetId | undefined;
  export function hostTarget(): Target | undefined;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts` (and extend the import from `../tools/build-cli`):

```ts
describe('the target table', () => {
  test('is exactly the three non-macOS targets, and nothing darwin', () => {
    // Pinned as a whole, not sampled. An added 'darwin-arm64' row would
    // silently start producing an unsigned, unnotarized macOS artifact
    // alongside the signed one app/release.sh builds.
    expect(TARGETS.map((t) => t.id)).toEqual(['linux-x64', 'linux-arm64', 'windows-x64']);
    expect(TARGETS.map((t) => t.bunTarget)).toEqual([
      'bun-linux-x64',
      'bun-linux-arm64',
      'bun-windows-x64',
    ]);
    expect(TARGETS.map((t) => t.format)).toEqual(['elf-x86-64', 'elf-aarch64', 'pe-x86-64']);
    expect(TARGETS.map((t) => t.archiveName)).toEqual([
      'screepub-cli-linux-x64.tar.gz',
      'screepub-cli-linux-arm64.tar.gz',
      'screepub-cli-windows-x64.zip',
    ]);
  });

  test('every archive name is a bare filename with no path', () => {
    // SHA256SUMS lists these verbatim; a path component there breaks
    // `sha256sum -c` for anyone who downloads the file.
    for (const t of TARGETS) {
      expect(t.archiveName).not.toContain('/');
      expect(t.archiveName).not.toContain('\\');
    }
  });
});

describe('paths around a target', () => {
  const OUT = '/tmp/screepub-out';

  test('each target builds in its own directory, archives land flat', () => {
    const dirs = TARGETS.map((t) => buildDir(t, OUT));
    expect(new Set(dirs).size).toBe(TARGETS.length); // no two collide
    expect(dirs).toEqual([
      '/tmp/screepub-out/linux-x64',
      '/tmp/screepub-out/linux-arm64',
      '/tmp/screepub-out/windows-x64',
    ]);
    for (const t of TARGETS) {
      expect(archivePath(t, OUT)).toBe(`/tmp/screepub-out/${t.archiveName}`);
    }
  });

  test('the outfile we PASS never carries .exe; the file bun WRITES does', () => {
    // bun build --compile appends .exe for the windows target. Passing
    // 'screepub.exe' would produce 'screepub.exe.exe'; expecting no .exe
    // afterwards would look for a file that is not there.
    for (const t of TARGETS) {
      expect(compileOutfile(t, OUT).endsWith('.exe')).toBe(false);
    }
    const win = TARGETS.find((t) => t.id === 'windows-x64')!;
    const lin = TARGETS.find((t) => t.id === 'linux-x64')!;
    expect(binaryPath(win, OUT)).toBe('/tmp/screepub-out/windows-x64/screepub.exe');
    expect(binaryPath(lin, OUT)).toBe('/tmp/screepub-out/linux-x64/screepub');
  });
});

describe('hostTarget', () => {
  test('maps the platform/arch pairs that matter', () => {
    expect(targetIdForHost('linux', 'x64')).toBe('linux-x64');
    expect(targetIdForHost('linux', 'arm64')).toBe('linux-arm64');
    expect(targetIdForHost('win32', 'x64')).toBe('windows-x64');
    // macOS is deliberately NOT a target of this tool, so the host mapping
    // must answer "nothing", not fall back to something runnable-looking.
    expect(targetIdForHost('darwin', 'arm64')).toBeUndefined();
    expect(targetIdForHost('darwin', 'x64')).toBeUndefined();
    expect(targetIdForHost('linux', 'ia32')).toBeUndefined();
    expect(targetIdForHost('freebsd', 'x64')).toBeUndefined();
  });

  test('hostTarget agrees with the running process', () => {
    const host = hostTarget();
    const expected = targetIdForHost(process.platform, process.arch);
    expect(host?.id).toBe(expected as TargetId | undefined);
    if (host) expect(TARGETS).toContain(host);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `TARGETS` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (import `join` from `node:path` at the top):

```ts
export type TargetId = 'linux-x64' | 'linux-arm64' | 'windows-x64';

export interface Target {
  id: TargetId;
  /** The value for `bun build --compile --target=`. */
  bunTarget: string;
  /** What the compiled output must be. Checked, never assumed. */
  format: BinaryFormat;
  packaging: 'tar.gz' | 'zip';
  /** The name the binary has INSIDE the archive, so `tar -xzf` and
   *  `Expand-Archive` both drop a runnable `screepub` in the cwd — the same
   *  arrangement as the macOS tarballs app/release.sh produces. */
  binaryName: string;
  archiveName: string;
}

/** No darwin row, ever. See the file header. */
export const TARGETS: readonly Target[] = [
  {
    id: 'linux-x64',
    bunTarget: 'bun-linux-x64',
    format: 'elf-x86-64',
    packaging: 'tar.gz',
    binaryName: 'screepub',
    archiveName: 'screepub-cli-linux-x64.tar.gz',
  },
  {
    // Asahi, Raspberry Pi, ARM servers — and the machine this is developed
    // on, so it is the Linux target that gets exercised daily.
    id: 'linux-arm64',
    bunTarget: 'bun-linux-arm64',
    format: 'elf-aarch64',
    packaging: 'tar.gz',
    binaryName: 'screepub',
    archiveName: 'screepub-cli-linux-arm64.tar.gz',
  },
  {
    id: 'windows-x64',
    bunTarget: 'bun-windows-x64',
    format: 'pe-x86-64',
    packaging: 'zip',
    binaryName: 'screepub.exe',
    archiveName: 'screepub-cli-windows-x64.zip',
  },
];

/** Per-target build directory: all three would otherwise compile to the
 *  same `screepub` and overwrite each other. */
export function buildDir(target: Target, outDir: string): string {
  return join(outDir, target.id);
}

/** What we hand `--outfile`. Never carries `.exe`: bun appends it for the
 *  windows target, and passing it would produce `screepub.exe.exe`. */
export function compileOutfile(target: Target, outDir: string): string {
  return join(buildDir(target, outDir), 'screepub');
}

/** Where the compiled binary actually lands. */
export function binaryPath(target: Target, outDir: string): string {
  return join(buildDir(target, outDir), target.binaryName);
}

/** Archives sit flat in the output directory, so SHA256SUMS names them
 *  without a path. */
export function archivePath(target: Target, outDir: string): string {
  return join(outDir, target.archiveName);
}

/** Which target, if any, this machine can actually execute. Pure, so the
 *  mapping is tested rather than only observed on whatever host ran the
 *  suite. macOS answers `undefined` on purpose: this tool builds no macOS
 *  artifact, so a Mac has no host target here. */
export function targetIdForHost(platform: string, arch: string): TargetId | undefined {
  if (platform === 'linux' && arch === 'x64') return 'linux-x64';
  if (platform === 'linux' && arch === 'arm64') return 'linux-arm64';
  if (platform === 'win32' && arch === 'x64') return 'windows-x64';
  return undefined;
}

export function hostTarget(): Target | undefined {
  const id = targetIdForHost(process.platform, process.arch);
  return id ? TARGETS.find((t) => t.id === id) : undefined;
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 12, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
The build matrix is a table, and macOS is not in it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 3: Arguments, and the version the binary will actually report

`--out` has no default. A default would let a mistyped command write 250 MB into the repo, and the first symptom would be a `git status` nobody reads.

`--version` is not injected into anything: `screepub --version` reports `package.json`'s version, inlined at compile time. So the flag's real job is to be *checked* against `package.json` — the same guard `app/release.sh` already applies on the macOS side, for the same reason. A binary that disagrees with its tag lies about itself in every bug report.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export interface BuildArgs { version: string; outDir: string; only: TargetId[] }
  export function parseBuildArgs(argv: string[]): BuildArgs;
  export function assertPackageVersion(version: string, repoDir?: string): void;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('parseBuildArgs', () => {
  test('takes a version and an output directory', () => {
    const args = parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x']);
    expect(args.version).toBe('0.6.0');
    expect(args.outDir).toBe('/tmp/x');
    expect(args.only).toEqual(['linux-x64', 'linux-arm64', 'windows-x64']);
  });

  test('accepts a tag spelling and normalises it', () => {
    expect(parseBuildArgs(['--version', 'v0.6.0', '--out', '/tmp/x']).version).toBe('0.6.0');
    expect(parseBuildArgs(['--version', 'v0.6.0-rc1', '--out', '/tmp/x']).version).toBe('0.6.0-rc1');
  });

  test('refuses a version that is not MAJOR.MINOR.PATCH', () => {
    expect(() => parseBuildArgs(['--out', '/tmp/x'])).toThrow(/--version/);
    expect(() => parseBuildArgs(['--version', '0.6', '--out', '/tmp/x'])).toThrow(/--version/);
    expect(() => parseBuildArgs(['--version', 'main', '--out', '/tmp/x'])).toThrow(/--version/);
    expect(() => parseBuildArgs(['--version', '', '--out', '/tmp/x'])).toThrow(/--version/);
  });

  test('refuses to invent an output directory', () => {
    // The artifacts are 39-119 MB each. A default would put a quarter of a
    // gigabyte somewhere nobody asked for.
    expect(() => parseBuildArgs(['--version', '0.6.0'])).toThrow(/--out/);
  });

  test('resolves the output directory to an absolute path', () => {
    const args = parseBuildArgs(['--version', '0.6.0', '--out', 'dist']);
    expect(isAbsolute(args.outDir)).toBe(true);
    expect(args.outDir.endsWith('dist')).toBe(true);
  });

  test('--only narrows the matrix, and rejects a target that does not exist', () => {
    expect(parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--only', 'windows-x64']).only)
      .toEqual(['windows-x64']);
    expect(parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--only', 'linux-x64,windows-x64']).only)
      .toEqual(['linux-x64', 'windows-x64']);
    // A typo must stop the build, not quietly produce nothing — which is
    // what filtering an unknown id out of the matrix would do.
    expect(() => parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--only', 'darwin-arm64']))
      .toThrow(/darwin-arm64/);
    expect(() => parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--only', 'linux_x64']))
      .toThrow(/linux-x64, linux-arm64, windows-x64/);
  });

  test('rejects an unknown flag rather than ignoring it', () => {
    expect(() => parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--sign'])).toThrow();
  });
});

describe('assertPackageVersion', () => {
  function repoWith(version: unknown): string {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-pkg-'));
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'screepub', version }));
    return dir;
  }

  test('passes when package.json agrees with the requested version', () => {
    const dir = repoWith('0.6.0');
    try {
      expect(() => assertPackageVersion('0.6.0', dir)).not.toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('refuses to build a binary that would misreport itself', () => {
    const dir = repoWith('0.5.4');
    try {
      // Both numbers in the message: the whole value of this guard is that
      // whoever hits it knows which one to change.
      expect(() => assertPackageVersion('0.6.0', dir)).toThrow(/0\.5\.4/);
      expect(() => assertPackageVersion('0.6.0', dir)).toThrow(/0\.6\.0/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the real repo agrees with itself', () => {
    const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as { version: string };
    expect(() => assertPackageVersion(pkg.version)).not.toThrow();
  });
});
```

Add `isAbsolute` to the `node:path` import and `readFileSync` to the `node:fs` import at the top of the test file.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `parseBuildArgs` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `readFileSync` to the `node:fs` import, `isAbsolute`/`resolve` to `node:path`, and `import { parseArgs } from 'node:util';`):

```ts
/** The repo root, from this file's location: the tool is run from anywhere
 *  and must still find package.json and src/cli.ts. */
export const REPO_DIR = join(import.meta.dir, '..');

const VERSION_RE = /^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$/;

export interface BuildArgs {
  version: string;
  outDir: string;
  only: TargetId[];
}

export function parseBuildArgs(argv: string[]): BuildArgs {
  const { values } = parseArgs({
    args: argv,
    options: {
      version: { type: 'string' },
      out: { type: 'string' },
      only: { type: 'string', multiple: true },
    },
    strict: true,
    allowPositionals: false,
  });

  const version = (values.version ?? '').replace(/^v/, '');
  if (!VERSION_RE.test(version)) {
    throw new Error(
      `build-cli: --version must be MAJOR.MINOR.PATCH (got ${values.version ?? '<missing>'})`,
    );
  }
  if (!values.out) {
    throw new Error(
      'build-cli: --out <dir> is required. There is no default: each artifact is 39-119 MB, ' +
        'and a default would write a quarter of a gigabyte somewhere you did not ask for.',
    );
  }

  const known = TARGETS.map((t) => t.id);
  const requested = (values.only ?? [])
    .flatMap((s) => s.split(','))
    .map((s) => s.trim())
    .filter(Boolean);
  for (const id of requested) {
    if (!known.includes(id as TargetId)) {
      throw new Error(`build-cli: unknown target "${id}"; known targets are ${known.join(', ')}`);
    }
  }

  return {
    version,
    outDir: isAbsolute(values.out) ? values.out : resolve(values.out),
    only: (requested.length ? requested : known) as TargetId[],
  };
}

/** `screepub --version` reports package.json's version, inlined at compile
 *  time; the --version flag changes nothing about the binary. So the flag's
 *  job is to be CHECKED. app/release.sh applies the same guard on the macOS
 *  side, and for the same reason: a binary that disagrees with its tag lies
 *  about itself in every bug report it ever appears in. */
export function assertPackageVersion(version: string, repoDir: string = REPO_DIR): void {
  const pkg = JSON.parse(readFileSync(join(repoDir, 'package.json'), 'utf8')) as {
    version?: string;
  };
  if (pkg.version !== version) {
    throw new Error(
      `build-cli: package.json says ${pkg.version} but --version says ${version}. ` +
        '`screepub --version` reports package.json inlined at compile time, so this binary ' +
        'would misreport itself. Bump package.json, or build the version it already names.',
    );
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 22, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
No default output directory, and no binary that misreports its version

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 4: The cross-compile itself

The invocation is one line, and the risk is entirely in getting that line exactly right — a dropped `--target=` silently builds the host architecture, and a wrong entry point builds a working binary of the wrong program. So the argv is built by a pure function and asserted element by element, with the spawn injected so the argv test costs nothing.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export interface SpawnResult { exitCode: number; stderr: string }
  export type Spawn = (argv: string[], cwd: string) => SpawnResult;
  export function compileArgv(target: Target, outDir: string): string[];
  export function compileTarget(target: Target, outDir: string, spawn?: Spawn, repoDir?: string): string;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('compileArgv', () => {
  test('is the exact bun invocation, per target', () => {
    // Element-by-element. A dropped --target= silently builds the HOST
    // architecture and every later format check would then be comparing a
    // native binary against itself.
    expect(compileArgv(TARGETS[0]!, '/tmp/out')).toEqual([
      'bun', 'build', '--compile', '--target=bun-linux-x64',
      'src/cli.ts', '--outfile=/tmp/out/linux-x64/screepub',
    ]);
    expect(compileArgv(TARGETS[2]!, '/tmp/out')).toEqual([
      'bun', 'build', '--compile', '--target=bun-windows-x64',
      'src/cli.ts', '--outfile=/tmp/out/windows-x64/screepub',
    ]);
  });

  test('every target names its own bun target and nothing else', () => {
    for (const t of TARGETS) {
      const argv = compileArgv(t, '/tmp/out');
      expect(argv).toContain(`--target=${t.bunTarget}`);
      expect(argv.filter((a) => a.startsWith('--target=')).length).toBe(1);
      expect(argv).toContain('src/cli.ts');
      expect(argv.some((a) => a.endsWith('.exe'))).toBe(false);
    }
  });
});

describe('compileTarget', () => {
  test('runs bun from the repo root and returns the path bun WROTE', () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-compile-'));
    try {
      const seen: { argv: string[]; cwd: string }[] = [];
      const fake = (argv: string[], cwd: string) => {
        seen.push({ argv, cwd });
        return { exitCode: 0, stderr: '' };
      };
      const win = TARGETS.find((t) => t.id === 'windows-x64')!;
      const out = compileTarget(win, dir, fake, '/repo');
      expect(seen.length).toBe(1);
      expect(seen[0]!.cwd).toBe('/repo');
      expect(seen[0]!.argv).toEqual(compileArgv(win, dir));
      // The returned path is where bun PUTS it, not what we asked for.
      expect(out).toBe(join(dir, 'windows-x64', 'screepub.exe'));
      expect(existsSync(join(dir, 'windows-x64'))).toBe(true); // dir made first
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a failed compile throws, naming the target and bun stderr', () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-compile-'));
    try {
      const fake = () => ({ exitCode: 1, stderr: 'error: unknown target\n' });
      expect(() => compileTarget(TARGETS[1]!, dir, fake, '/repo')).toThrow(/linux-arm64/);
      expect(() => compileTarget(TARGETS[1]!, dir, fake, '/repo')).toThrow(/unknown target/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

Add `existsSync` to the `node:fs` import at the top of the test file.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `compileArgv` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `mkdirSync` to the `node:fs` import):

```ts
export interface SpawnResult {
  exitCode: number;
  stderr: string;
}
export type Spawn = (argv: string[], cwd: string) => SpawnResult;

const realSpawn: Spawn = (argv, cwd) => {
  const proc = Bun.spawnSync(argv, { cwd, stdout: 'pipe', stderr: 'pipe' });
  return { exitCode: proc.exitCode ?? 1, stderr: proc.stderr.toString() };
};

/** The one invocation, as data, so it can be asserted element by element
 *  instead of reviewed by eye. */
export function compileArgv(target: Target, outDir: string): string[] {
  return [
    'bun',
    'build',
    '--compile',
    `--target=${target.bunTarget}`,
    'src/cli.ts',
    `--outfile=${compileOutfile(target, outDir)}`,
  ];
}

/** Returns the path bun actually WROTE — which is not the outfile it was
 *  given for the windows target, where bun appends `.exe`. */
export function compileTarget(
  target: Target,
  outDir: string,
  spawn: Spawn = realSpawn,
  repoDir: string = REPO_DIR,
): string {
  mkdirSync(buildDir(target, outDir), { recursive: true });
  const argv = compileArgv(target, outDir);
  const { exitCode, stderr } = spawn(argv, repoDir);
  if (exitCode !== 0) {
    throw new Error(
      `build-cli: ${target.id} failed to compile (exit ${exitCode})\n${stderr.trim()}`,
    );
  }
  return binaryPath(target, outDir);
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 26, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
One cross-compile invocation, asserted argument by argument

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 5: Reading an archive back

Verification has to see *inside* the archive, because that is where the two failures nobody notices live: a tarball that contains nothing, and a binary that lost its executable bit on the way in. Both look like a perfectly good 39 MB download.

Both readers are written here rather than shelled out to. `tar -tzvf` answers in a format that varies between GNU tar and bsdtar; the tar header is 512 fixed bytes and `Bun.gunzipSync` is already in the runtime. `jszip` is already a dependency and is the only thing here that can read back the permission bits it wrote.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export interface ArchiveEntry { name: string; size: number; mode: number; data: Uint8Array }
  export function tarGzEntries(archivePath: string): ArchiveEntry[];
  export function zipEntries(archivePath: string): Promise<ArchiveEntry[]>;
  export function archiveEntries(archivePath: string): Promise<ArchiveEntry[]>;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('reading an archive back', () => {
  /** A tar.gz built by the system tar, from files with known contents,
   *  sizes and modes. Two files, one of them longer than a tar block, so
   *  the 512-byte walk is genuinely exercised rather than accidentally
   *  right for a single small entry. */
  function scratchTarGz(): { dir: string; archive: string; big: Uint8Array } {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-tar-'));
    const stage = join(dir, 'stage');
    mkdirSync(stage, { recursive: true });
    const big = new Uint8Array(1500);
    for (let i = 0; i < big.length; i++) big[i] = i % 251;
    writeFileSync(join(stage, 'screepub'), big);
    chmodSync(join(stage, 'screepub'), 0o755);
    writeFileSync(join(stage, 'NOTES'), 'plain\n');
    chmodSync(join(stage, 'NOTES'), 0o644);
    const archive = join(dir, 'a.tar.gz');
    const proc = Bun.spawnSync(['tar', '-czf', archive, '-C', stage, 'screepub', 'NOTES']);
    if (proc.exitCode !== 0) throw new Error(`test setup: tar failed: ${proc.stderr.toString()}`);
    return { dir, archive, big };
  }

  test('tarGzEntries reports every member with its size, mode and bytes', () => {
    const { dir, archive, big } = scratchTarGz();
    try {
      const entries = tarGzEntries(archive);
      expect(entries.map((e) => e.name).sort()).toEqual(['NOTES', 'screepub']);

      const bin = entries.find((e) => e.name === 'screepub')!;
      expect(bin.size).toBe(1500);
      // The bytes, not just the length: a walker that mis-added the block
      // padding would return 1500 bytes starting in the wrong place.
      expect(Array.from(bin.data)).toEqual(Array.from(big));
      expect(bin.mode & 0o111).not.toBe(0);

      const notes = entries.find((e) => e.name === 'NOTES')!;
      expect(new TextDecoder().decode(notes.data)).toBe('plain\n');
      // The second entry proves the walk advanced past the first one's
      // padded 2048 bytes rather than stopping.
      expect(notes.mode & 0o111).toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('archiveEntries dispatches on the extension and refuses anything else', async () => {
    const { dir, archive } = scratchTarGz();
    try {
      expect((await archiveEntries(archive)).length).toBe(2);
      await expect(archiveEntries(join(dir, 'a.rar'))).rejects.toThrow(/tar\.gz|zip/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('zipEntries round-trips a name, bytes and the executable bit', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-zip-'));
    try {
      const JSZip = (await import('jszip')).default;
      const payload = new Uint8Array(1500);
      for (let i = 0; i < payload.length; i++) payload[i] = (i * 7) % 253;
      const zip = new JSZip();
      zip.file('screepub.exe', payload, { unixPermissions: 0o755 });
      const buf = await zip.generateAsync({
        type: 'nodebuffer',
        platform: 'UNIX',
        compression: 'DEFLATE',
      });
      const archive = join(dir, 'a.zip');
      writeFileSync(archive, buf);

      const entries = await zipEntries(archive);
      expect(entries.map((e) => e.name)).toEqual(['screepub.exe']);
      expect(entries[0]!.size).toBe(1500);
      expect(Array.from(entries[0]!.data)).toEqual(Array.from(payload));
      expect(entries[0]!.mode & 0o111).not.toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

Add `chmodSync` and `mkdirSync` to the `node:fs` import at the top of the test file.

> If `zipEntries` comes back with `mode === 0`, jszip did not surface the permissions: read the high 16 bits of the central-directory external attributes yourself rather than dropping the assertion. The executable bit is one of the two things this reader exists to see.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `tarGzEntries` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `readFileSync` if not already imported, and `basename`/`extname` are not needed here):

```ts
export interface ArchiveEntry {
  name: string;
  size: number;
  /** Unix mode as recorded in the archive. 0 when the format carries none. */
  mode: number;
  data: Uint8Array;
}

function tarString(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes).replace(/\0[\s\S]*$/, '');
}

function tarOctal(bytes: Uint8Array): number {
  const text = tarString(bytes).trim();
  return text ? parseInt(text, 8) : 0;
}

/** Walk the tar ourselves. `tar -tzvf` answers in a listing format that
 *  differs between GNU tar and bsdtar; the header is 512 fixed bytes and
 *  gunzip is already in the runtime. */
export function tarGzEntries(archivePath: string): ArchiveEntry[] {
  const tar = Bun.gunzipSync(readFileSync(archivePath));
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

export async function zipEntries(archivePath: string): Promise<ArchiveEntry[]> {
  const JSZip = (await import('jszip')).default;
  const zip = await JSZip.loadAsync(readFileSync(archivePath));
  const out: ArchiveEntry[] = [];
  for (const file of Object.values(zip.files)) {
    if (file.dir) continue;
    const data = await file.async('uint8array');
    const perms = (file as unknown as { unixPermissions: number | null }).unixPermissions;
    out.push({ name: file.name, size: data.length, mode: perms ?? 0, data });
  }
  return out;
}

export function archiveEntries(archivePath: string): Promise<ArchiveEntry[]> {
  if (archivePath.endsWith('.tar.gz')) return Promise.resolve(tarGzEntries(archivePath));
  if (archivePath.endsWith('.zip')) return zipEntries(archivePath);
  return Promise.reject(
    new Error(`build-cli: ${archivePath} is neither a .tar.gz nor a .zip`),
  );
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 29, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
See inside the archive: names, sizes, bytes and the executable bit

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 6: Packaging

tar.gz for Linux, zip for Windows, matching what each platform's users can open without installing anything. The binary is named plainly inside both, so `tar -xzf` and `Expand-Archive` each leave a runnable file in the current directory — the same arrangement the macOS tarballs already have, and what the Homebrew formula's `bin.install` relies on there.

The executable bit is set explicitly before packing rather than inherited. On Linux it is what makes the download runnable at all; in the zip it is what a WSL or macOS user gets when they unpack a Windows build to inspect it.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces: `export function packageTarget(target: Target, outDir: string): Promise<string>;` — returns the archive path.

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('packageTarget', () => {
  /** Stage a small stand-in binary exactly where compileTarget would have
   *  left one, so packaging is tested without a 100 MB compile. */
  function stage(target: Target, bytes: Uint8Array): string {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-pack-'));
    mkdirSync(buildDir(target, dir), { recursive: true });
    writeFileSync(binaryPath(target, dir), bytes);
    return dir;
  }

  test('a Linux target becomes a tar.gz holding one executable `screepub`', async () => {
    const target = TARGETS.find((t) => t.id === 'linux-arm64')!;
    const payload = new Uint8Array(3000);
    payload.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0], 0);
    new DataView(payload.buffer).setUint16(0x12, 0xb7, true);
    const dir = stage(target, payload);
    try {
      // Deliberately NOT executable on disk first: packaging must set the
      // bit, not inherit whatever the compiler happened to leave.
      chmodSync(binaryPath(target, dir), 0o644);
      const archive = await packageTarget(target, dir);
      expect(archive).toBe(join(dir, 'screepub-cli-linux-arm64.tar.gz'));

      const entries = await archiveEntries(archive);
      expect(entries.map((e) => e.name)).toEqual(['screepub']); // no path prefix
      expect(entries[0]!.size).toBe(3000);
      expect(entries[0]!.mode & 0o111).not.toBe(0);
      // The member is the binary, byte for byte, and still reads as an
      // aarch64 ELF after the round trip.
      expect(detectBinaryFormat(entries[0]!.data)).toBe('elf-aarch64');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the Windows target becomes a zip holding one `screepub.exe`', async () => {
    const target = TARGETS.find((t) => t.id === 'windows-x64')!;
    const payload = new Uint8Array(3000);
    payload.set([0x4d, 0x5a], 0);
    const v = new DataView(payload.buffer);
    v.setUint32(0x3c, 0x78, true);
    payload.set([0x50, 0x45, 0x00, 0x00], 0x78);
    v.setUint16(0x7c, 0x8664, true);
    const dir = stage(target, payload);
    try {
      const archive = await packageTarget(target, dir);
      expect(archive).toBe(join(dir, 'screepub-cli-windows-x64.zip'));

      const entries = await archiveEntries(archive);
      expect(entries.map((e) => e.name)).toEqual(['screepub.exe']);
      expect(entries[0]!.size).toBe(3000);
      expect(detectBinaryFormat(entries[0]!.data)).toBe('pe-x86-64');
      expect(entries[0]!.mode & 0o111).not.toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('re-packaging replaces the archive instead of appending to it', async () => {
    const target = TARGETS.find((t) => t.id === 'linux-x64')!;
    const dir = stage(target, new Uint8Array(3000));
    try {
      await packageTarget(target, dir);
      await packageTarget(target, dir);
      const entries = await archiveEntries(archivePath(target, dir));
      expect(entries.length).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `packageTarget` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `chmodSync`, `rmSync`, `writeFileSync` to the `node:fs` import):

```ts
/** tar.gz for Linux, zip for Windows: what each platform's users can open
 *  with nothing installed. The binary is named plainly inside both, so an
 *  unpack leaves a runnable file in the current directory — the same
 *  arrangement the macOS tarballs already have. */
export async function packageTarget(target: Target, outDir: string): Promise<string> {
  const bin = binaryPath(target, outDir);
  // Set, not inherited. On Linux this bit is the difference between a
  // download that runs and one that does not.
  chmodSync(bin, 0o755);

  const out = archivePath(target, outDir);
  rmSync(out, { force: true }); // tar -czf appends into an existing file

  if (target.packaging === 'tar.gz') {
    const proc = Bun.spawnSync(
      ['tar', '-czf', out, '-C', buildDir(target, outDir), target.binaryName],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    if ((proc.exitCode ?? 1) !== 0) {
      throw new Error(`build-cli: tar failed for ${target.id}: ${proc.stderr.toString().trim()}`);
    }
  } else {
    const JSZip = (await import('jszip')).default;
    const zip = new JSZip();
    zip.file(target.binaryName, readFileSync(bin), { unixPermissions: 0o755 });
    const buf = await zip.generateAsync({
      type: 'nodebuffer',
      // UNIX, so the permission bits above are actually written into the
      // central directory rather than discarded.
      platform: 'UNIX',
      compression: 'DEFLATE',
    });
    writeFileSync(out, buf);
  }
  return out;
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 32, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
Package each target the way its platform can open it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 7: Verification, which is the point of the tool

The spec's first acceptance criterion is that the tool "fails loudly if any artifact is missing, empty or the wrong format". Trusting the compiler's exit code is exactly the mistake this replaces: `bun build` can exit 0 and leave a truncated file, and a mis-typed `--target=` exits 0 while producing a perfectly valid binary for the wrong machine.

Eight checks, each of which must be able to fail. The size floors are **injectable** so every branch is exercised in milliseconds with tiny thresholds, while the production numbers are asserted separately and exercised for real in Task 11.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export interface Floors { binaryBytes: number; archiveBytes: number }
  export const RELEASE_FLOORS: Floors;
  export function verifyArtifact(target: Target, outDir: string, floors?: Floors): Promise<void>;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('verifyArtifact', () => {
  const TINY: Floors = { binaryBytes: 512, archiveBytes: 32 };
  const target = TARGETS.find((t) => t.id === 'linux-x64')!;

  function elfPayload(bytes = 3000, machine = 0x3e): Uint8Array {
    const b = new Uint8Array(bytes);
    b.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0], 0);
    new DataView(b.buffer).setUint16(0x12, machine, true);
    for (let i = 64; i < bytes; i++) b[i] = i % 251; // not compressible to nothing
    return b;
  }

  async function built(payload: Uint8Array): Promise<string> {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-verify-'));
    mkdirSync(buildDir(target, dir), { recursive: true });
    writeFileSync(binaryPath(target, dir), payload);
    await packageTarget(target, dir);
    return dir;
  }

  test('a good artifact passes', async () => {
    const dir = await built(elfPayload());
    try {
      await expect(verifyArtifact(target, dir, TINY)).resolves.toBeUndefined();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a missing binary fails', async () => {
    const dir = await built(elfPayload());
    try {
      rmSync(binaryPath(target, dir));
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/no binary/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a 0-byte binary fails, and so does a merely small one', async () => {
    // The exact failure a "does the file exist?" check waves through.
    for (const payload of [new Uint8Array(0), elfPayload(100)]) {
      const dir = await built(elfPayload());
      try {
        writeFileSync(binaryPath(target, dir), payload);
        await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/floor/);
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    }
  });

  test('the RIGHT SIZE but the WRONG ARCHITECTURE fails', async () => {
    // The mis-typed --target= case: a perfectly good binary for a machine
    // nobody downloading this tarball is running.
    const dir = await built(elfPayload(3000, 0xb7)); // aarch64 in the x64 slot
    try {
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/elf-aarch64/);
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/elf-x86-64/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a shell script where a binary should be fails', async () => {
    const dir = await built(elfPayload());
    try {
      writeFileSync(binaryPath(target, dir), new TextEncoder().encode('#!/bin/sh\n'.repeat(200)));
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/unknown/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a missing or tiny archive fails', async () => {
    const dir = await built(elfPayload());
    try {
      rmSync(archivePath(target, dir));
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/no archive/);
      writeFileSync(archivePath(target, dir), new Uint8Array(4));
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/floor/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('an archive that does not contain the binary fails, and says what it holds', async () => {
    const dir = await built(elfPayload());
    try {
      const decoy = join(buildDir(target, dir), 'READ.ME');
      writeFileSync(decoy, 'x'.repeat(400));
      rmSync(archivePath(target, dir));
      const proc = Bun.spawnSync([
        'tar', '-czf', archivePath(target, dir), '-C', buildDir(target, dir), 'READ.ME',
      ]);
      expect(proc.exitCode).toBe(0);
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/READ\.ME/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('an archive whose member is not executable fails', async () => {
    const dir = await built(elfPayload());
    try {
      chmodSync(binaryPath(target, dir), 0o644);
      rmSync(archivePath(target, dir));
      const proc = Bun.spawnSync([
        'tar', '-czf', archivePath(target, dir), '-C', buildDir(target, dir), 'screepub',
      ]);
      expect(proc.exitCode).toBe(0);
      chmodSync(binaryPath(target, dir), 0o755); // on-disk bit is fine again
      await expect(verifyArtifact(target, dir, TINY)).rejects.toThrow(/not executable/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the production floors are the real numbers and reject a toy binary', async () => {
    // TINY thresholds above prove the branches work. This proves the
    // DEFAULTS are set where a real artifact sits: bun embeds its whole
    // runtime, so every real binary is 64-119 MB.
    expect(RELEASE_FLOORS.binaryBytes).toBe(20_000_000);
    expect(RELEASE_FLOORS.archiveBytes).toBe(1_000_000);
    const dir = await built(elfPayload());
    try {
      await expect(verifyArtifact(target, dir)).rejects.toThrow(/20000000|floor/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

Add `type Floors` to the type imports from `../tools/build-cli`.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `verifyArtifact` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `existsSync`, `statSync` to the `node:fs` import and `basename` to `node:path`):

```ts
export interface Floors {
  binaryBytes: number;
  archiveBytes: number;
}

/** Every real artifact embeds a whole Bun runtime: 64-119 MB compiled,
 *  39 MB or so compressed. Anything an order of magnitude under that is a
 *  truncated write, not a lean build. */
export const RELEASE_FLOORS: Floors = { binaryBytes: 20_000_000, archiveBytes: 1_000_000 };

/** Check what was PRODUCED, not the compiler's exit code. `bun build` can
 *  exit 0 and leave a truncated file, and a mis-typed --target= exits 0
 *  while producing a perfectly valid binary for the wrong machine. */
export async function verifyArtifact(
  target: Target,
  outDir: string,
  floors: Floors = RELEASE_FLOORS,
): Promise<void> {
  const bin = binaryPath(target, outDir);
  if (!existsSync(bin)) throw new Error(`build-cli: ${target.id}: no binary at ${bin}`);

  const binBytes = statSync(bin).size;
  if (binBytes < floors.binaryBytes) {
    throw new Error(
      `build-cli: ${target.id}: binary is ${binBytes} bytes, under the ${floors.binaryBytes}-byte floor`,
    );
  }

  const binFormat = readBinaryFormat(bin);
  if (binFormat !== target.format) {
    throw new Error(
      `build-cli: ${target.id}: binary is ${binFormat}, expected ${target.format}`,
    );
  }

  const arc = archivePath(target, outDir);
  if (!existsSync(arc)) throw new Error(`build-cli: ${target.id}: no archive at ${arc}`);

  const arcBytes = statSync(arc).size;
  if (arcBytes < floors.archiveBytes) {
    throw new Error(
      `build-cli: ${target.id}: archive is ${arcBytes} bytes, under the ${floors.archiveBytes}-byte floor`,
    );
  }

  const entries = await archiveEntries(arc);
  const member = entries.find((e) => e.name === target.binaryName);
  if (!member) {
    const held = entries.map((e) => e.name).join(', ') || '<nothing>';
    throw new Error(
      `build-cli: ${target.id}: ${basename(arc)} does not contain ${target.binaryName} (it holds: ${held})`,
    );
  }
  if (member.size !== binBytes) {
    throw new Error(
      `build-cli: ${target.id}: ${target.binaryName} inside the archive is ${member.size} bytes, ` +
        `the binary on disk is ${binBytes}`,
    );
  }
  const memberFormat = detectBinaryFormat(member.data.subarray(0, HEAD_BYTES));
  if (memberFormat !== target.format) {
    throw new Error(
      `build-cli: ${target.id}: ${target.binaryName} inside the archive is ${memberFormat}, expected ${target.format}`,
    );
  }
  if ((member.mode & 0o111) === 0) {
    throw new Error(
      `build-cli: ${target.id}: ${target.binaryName} inside the archive is not executable ` +
        `(mode ${member.mode.toString(8)})`,
    );
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 41, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
Verify what was produced, not that the compiler exited zero

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 8: `SHA256SUMS`

A checksum file is only useful if the format is the one people's tools already speak: `sha256sum -c SHA256SUMS` on Linux, `shasum -a 256 -c` on macOS. That means sixty-four lowercase hex digits, exactly two spaces, a bare filename, and a trailing newline. Anything else is a file that looks right and verifies nothing.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export function sha256File(path: string): string;
  export function writeChecksums(outDir: string, archiveNames: string[]): string;
  export function parseChecksums(text: string): Map<string, string>;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('SHA256SUMS', () => {
  function withFiles(): { dir: string; names: string[] } {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-sums-'));
    const names = ['screepub-cli-windows-x64.zip', 'screepub-cli-linux-x64.tar.gz'];
    writeFileSync(join(dir, names[0]!), 'windows bytes');
    writeFileSync(join(dir, names[1]!), 'linux bytes');
    return { dir, names };
  }

  test('the digests are the real ones', () => {
    const { dir, names } = withFiles();
    try {
      writeChecksums(dir, names);
      const map = parseChecksums(readFileSync(join(dir, 'SHA256SUMS'), 'utf8'));
      for (const name of names) {
        const expected = createHash('sha256').update(readFileSync(join(dir, name))).digest('hex');
        expect(map.get(name)).toBe(expected);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the format is the one sha256sum -c actually parses', () => {
    const { dir, names } = withFiles();
    try {
      const text = writeChecksums(dir, names);
      const lines = text.split('\n');
      expect(lines.at(-1)).toBe(''); // trailing newline
      const body = lines.slice(0, -1);
      expect(body.length).toBe(2);
      for (const line of body) {
        // Two spaces, lowercase hex, bare filename. One space is the BSD
        // "text mode" spelling and a path component breaks -c for anyone
        // who downloads the file into their own directory.
        expect(line).toMatch(/^[0-9a-f]{64} {2}[^ /\\][^/\\]*$/);
      }
      // Sorted, so two runs of the same build produce the same file.
      expect(body.map((l) => l.slice(66))).toEqual([...names].sort());
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a changed byte changes the digest', () => {
    const { dir, names } = withFiles();
    try {
      const before = parseChecksums(writeChecksums(dir, names));
      writeFileSync(join(dir, names[0]!), 'windows bytez');
      const after = parseChecksums(writeChecksums(dir, names));
      expect(after.get(names[0]!)).not.toBe(before.get(names[0]!));
      expect(after.get(names[1]!)).toBe(before.get(names[1]!));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the system checker accepts it, and rejects a tampered file', () => {
    const checker = Bun.which('sha256sum') ?? Bun.which('shasum');
    if (!checker) {
      // Self-skip, matching how the suite handles Calibre and fixtures.
      console.log('skipping: no sha256sum/shasum on PATH');
      return;
    }
    const argv = checker.endsWith('shasum')
      ? [checker, '-a', '256', '-c', 'SHA256SUMS']
      : [checker, '-c', 'SHA256SUMS'];
    const { dir, names } = withFiles();
    try {
      writeChecksums(dir, names);
      const ok = Bun.spawnSync(argv, { cwd: dir, stdout: 'pipe', stderr: 'pipe' });
      expect(ok.exitCode).toBe(0);
      writeFileSync(join(dir, names[0]!), 'tampered');
      const bad = Bun.spawnSync(argv, { cwd: dir, stdout: 'pipe', stderr: 'pipe' });
      expect(bad.exitCode).not.toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

Add `import { createHash } from 'node:crypto';` to the test file.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `writeChecksums` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts` (add `import { createHash } from 'node:crypto';`):

```ts
export function sha256File(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

/** The format `sha256sum -c` and `shasum -a 256 -c` both parse: lowercase
 *  hex, exactly two spaces, a bare filename, a trailing newline. Sorted, so
 *  two runs of the same build produce the same file. Returns the text as
 *  well as writing it, so callers can assert on it without re-reading. */
export function writeChecksums(outDir: string, archiveNames: string[]): string {
  const text = [...archiveNames]
    .sort()
    .map((name) => `${sha256File(join(outDir, name))}  ${name}\n`)
    .join('');
  writeFileSync(join(outDir, 'SHA256SUMS'), text);
  return text;
}

export function parseChecksums(text: string): Map<string, string> {
  const map = new Map<string, string>();
  for (const line of text.split('\n')) {
    const m = /^([0-9a-f]{64}) {2}(.+)$/.exec(line);
    if (m) map.set(m[2]!, m[1]!);
  }
  return map;
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 45, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
Checksums in the format a downloader's own tool already speaks

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 9: `buildAll`, the command line, and the ignore rule

The orchestration is deliberately dull: check the version, then for each target compile, package and verify — **verify before moving on**, so a broken target fails at the target that broke rather than at a checksum three minutes later. Checksums cover exactly what was built.

The ignore rule matters because the tool is meant to be run by hand. `bun tools/build-cli.ts --out dist/` in the repo is the obvious thing to type, and it must not offer 250 MB to the next `git add -A`.

**Files:**
- Modify: `tools/build-cli.ts`
- Test: `tests/build-cli.test.ts` (append)

**Interfaces:**
- Produces: `export function buildAll(args: BuildArgs, floors?: Floors): Promise<string[]>;` — returns absolute archive paths. Plus an `import.meta.main` entry point.
- Consumes: everything above.

- [ ] **Step 1: Write the failing test**

Append to `tests/build-cli.test.ts`:

```ts
describe('buildAll', () => {
  test('refuses to build a version package.json does not name', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-all-'));
    try {
      await expect(
        buildAll({ version: '99.0.0', outDir: dir, only: ['linux-x64'] }),
      ).rejects.toThrow(/package\.json/);
      // Nothing was compiled: the guard runs before any 100 MB write.
      expect(existsSync(buildDir(TARGETS[0]!, dir))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('the repo will not accidentally commit an artifact', () => {
  test('.gitignore covers the obvious hand-run output directories', () => {
    // `bun tools/build-cli.ts --out dist/` is the documented example, and
    // each artifact is 39-119 MB.
    for (const path of [
      'dist/screepub-cli-linux-x64.tar.gz',
      'dist/SHA256SUMS',
      'build/screepub-cli-windows-x64.zip',
    ]) {
      const proc = Bun.spawnSync(['git', 'check-ignore', '-q', path]);
      expect({ path, ignored: proc.exitCode === 0 }).toEqual({ path, ignored: true });
    }
  });
});

describe('the tool runs from a command line', () => {
  // A path that provably does not exist yet, so "it was never created" is a
  // real assertion rather than an accident of what /tmp happens to hold.
  const NEVER = join(tmpdir(), `screepub-never-${process.pid}-${Date.now()}`);

  test('a missing --version fails with a message and a non-zero exit', () => {
    const proc = Bun.spawnSync(['bun', 'tools/build-cli.ts', '--out', NEVER], {
      stdout: 'pipe',
      stderr: 'pipe',
    });
    expect(proc.exitCode).not.toBe(0);
    expect(proc.stderr.toString()).toMatch(/--version/);
    expect(existsSync(NEVER)).toBe(false);
  });

  test('an unknown target is refused before anything is compiled', () => {
    const proc = Bun.spawnSync(
      ['bun', 'tools/build-cli.ts', '--version', '0.0.1', '--out', NEVER, '--only', 'darwin-arm64'],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    expect(proc.exitCode).not.toBe(0);
    expect(proc.stderr.toString()).toMatch(/darwin-arm64/);
    // Nothing was created: the refusal happens during argument parsing, so
    // a mistyped target never costs a 100 MB write.
    expect(existsSync(NEVER)).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/build-cli.test.ts`
Expected: FAIL — `buildAll` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `tools/build-cli.ts`:

```ts
/** Compile, package and verify one target at a time, verifying BEFORE
 *  moving on: a broken target then fails at the target that broke, not at
 *  a checksum three minutes later. */
export async function buildAll(args: BuildArgs, floors: Floors = RELEASE_FLOORS): Promise<string[]> {
  assertPackageVersion(args.version);
  mkdirSync(args.outDir, { recursive: true });

  const targets = TARGETS.filter((t) => args.only.includes(t.id));
  const names: string[] = [];
  for (const target of targets) {
    console.log(`── ${target.id} (${target.bunTarget})`);
    compileTarget(target, args.outDir);
    await packageTarget(target, args.outDir);
    await verifyArtifact(target, args.outDir, floors);
    const bytes = statSync(archivePath(target, args.outDir)).size;
    console.log(`   ${target.archiveName}  ${bytes} bytes  ok`);
    names.push(target.archiveName);
  }

  writeChecksums(args.outDir, names);
  return names.map((n) => join(args.outDir, n));
}

if (import.meta.main) {
  try {
    const args = parseBuildArgs(Bun.argv.slice(2));
    const made = await buildAll(args);
    console.log(`\n${made.length} artifact(s) + SHA256SUMS in ${args.outDir}`);
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 49, `tsc` clean. Then run the tool for real against one target, into the scratch directory, and confirm it reports `ok`:

```bash
V="$(bun --print "require('./package.json').version")"
OUT="$(mktemp -d)"
bun tools/build-cli.ts --version "$V" --out "$OUT" --only windows-x64
ls -l "$OUT"; cat "$OUT/SHA256SUMS"; rm -rf "$OUT"
```

- [ ] **Step 5: Commit**

```bash
git add tools/build-cli.ts tests/build-cli.test.ts
git commit -m "$(cat <<'EOF'
A release tool a person can actually run, and verify as it goes

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 10: `tools/smoke-cli.ts`

Cross-compiling is not cross-testing. A Windows binary built on Linux has never executed, so each OS runs its own artifact and proves three things: `--version` agrees with `package.json`, the committed fixture converts to a real EPUB, and `devices --json` starts.

It is a Bun script rather than two shell fragments because the alternative is one script in bash and one in PowerShell, drifting, neither testable. It imports nothing outside Node built-ins so a Windows runner needs no `bun install`. Every assertion is a pure function over a captured result, so the whole failure ledger is unit-tested here and the real run happens in Task 11 and in CI.

`devices --json` returning an empty list **is a pass**: it proves the command ran, which is the point.

**Files:**
- Create: `tools/smoke-cli.ts`
- Test: `tests/smoke-cli.test.ts` (create)

**Interfaces:**
- Produces:
  ```ts
  export interface RunResult { exitCode: number; stdout: string; stderr: string }
  export type Runner = (argv: string[]) => RunResult;
  export function soleJson(stdout: string, what: string): Record<string, unknown>;
  export function checkVersion(result: RunResult, expected: string): void;
  export function checkConvertResult(result: RunResult, epubPath: string): void;
  export function checkEpubBytes(head: Uint8Array): void;
  export function checkDevices(result: RunResult): void;
  export function smokeCli(binary: string, fixture: string, workDir: string, expectedVersion: string, run?: Runner): void;
  ```

- [ ] **Step 1: Write the failing test**

Create `tests/smoke-cli.test.ts`:

```ts
import { describe, test, expect } from 'bun:test';
import {
  soleJson,
  checkVersion,
  checkConvertResult,
  checkEpubBytes,
  checkDevices,
} from '../tools/smoke-cli';

const ok = (stdout: string) => ({ exitCode: 0, stdout, stderr: '' });

describe('soleJson', () => {
  test('accepts exactly one JSON object', () => {
    expect(soleJson('{"ok":true}\n', 'x')).toEqual({ ok: true });
  });

  test('rejects two objects, which is the --json contract breaking', () => {
    // The contract is ONE parseable object on stdout. Two would parse fine
    // if a checker only looked at the first line.
    expect(() => soleJson('{"ok":true}\n{"ok":false}\n', 'convert')).toThrow(/2 lines/);
  });

  test('rejects empty output and non-JSON', () => {
    expect(() => soleJson('', 'convert')).toThrow(/0 lines/);
    expect(() => soleJson('converting...\n', 'convert')).toThrow(/parseable JSON/);
  });
});

describe('checkVersion', () => {
  test('passes only on the exact expected line', () => {
    expect(() => checkVersion(ok('screepub 0.6.0\n'), '0.6.0')).not.toThrow();
  });

  test('a stale binary reporting the old version fails', () => {
    // The failure this exists for: an artifact built from a checkout that
    // was not the tagged one.
    expect(() => checkVersion(ok('screepub 0.5.4\n'), '0.6.0')).toThrow(/0\.5\.4/);
    // And a substring match must not save it.
    expect(() => checkVersion(ok('screepub 0.6.0-dirty\n'), '0.6.0')).toThrow();
    expect(() => checkVersion({ exitCode: 1, stdout: '', stderr: 'boom' }, '0.6.0')).toThrow(/exited 1/);
  });
});

describe('checkConvertResult', () => {
  const good = ok('{"ok":true,"epubPath":"/tmp/smoke.epub","pages":97}\n');

  test('passes on a real success payload', () => {
    expect(() => checkConvertResult(good, '/tmp/smoke.epub')).not.toThrow();
  });

  test('a reported failure fails, even at exit 0', () => {
    const failed = ok('{"ok":false,"error":{"code":"not-screenplay","message":"x"}}\n');
    expect(() => checkConvertResult(failed, '/tmp/smoke.epub')).toThrow(/not-screenplay/);
  });

  test('zero pages fails: an empty book is not a conversion', () => {
    const empty = ok('{"ok":true,"epubPath":"/tmp/smoke.epub","pages":0}\n');
    expect(() => checkConvertResult(empty, '/tmp/smoke.epub')).toThrow(/pages/);
    const none = ok('{"ok":true,"epubPath":"/tmp/smoke.epub"}\n');
    expect(() => checkConvertResult(none, '/tmp/smoke.epub')).toThrow(/pages/);
  });

  test('writing somewhere other than where we asked fails', () => {
    expect(() => checkConvertResult(good, '/tmp/other.epub')).toThrow(/other\.epub/);
  });

  test('a non-zero exit fails and carries stderr', () => {
    const crashed = { exitCode: 134, stdout: '', stderr: 'Segmentation fault' };
    expect(() => checkConvertResult(crashed, '/tmp/smoke.epub')).toThrow(/134/);
    expect(() => checkConvertResult(crashed, '/tmp/smoke.epub')).toThrow(/Segmentation/);
  });
});

describe('checkEpubBytes', () => {
  test('an EPUB is a zip container', () => {
    expect(() => checkEpubBytes(new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x14]))).not.toThrow();
  });

  test('a 0-byte or non-zip output fails', () => {
    // The CLI can report success and still have written nothing usable.
    expect(() => checkEpubBytes(new Uint8Array(0))).toThrow();
    expect(() => checkEpubBytes(new TextEncoder().encode('<html>'))).toThrow();
  });
});

describe('checkDevices', () => {
  test('an empty device list is a pass: the command ran', () => {
    expect(() => checkDevices(ok('{"ok":true,"devices":[]}\n'))).not.toThrow();
    expect(() => checkDevices(ok('{"ok":true,"devices":[{"id":"kindle"}]}\n'))).not.toThrow();
  });

  test('a missing array, a reported failure, or a crash all fail', () => {
    expect(() => checkDevices(ok('{"ok":true}\n'))).toThrow(/devices/);
    expect(() => checkDevices(ok('{"ok":false,"error":{"code":"internal"}}\n'))).toThrow(/internal/);
    expect(() => checkDevices({ exitCode: 1, stdout: '', stderr: 'no' })).toThrow(/exited 1/);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/smoke-cli.test.ts`
Expected: FAIL — `tools/smoke-cli.ts` does not exist.

- [ ] **Step 3: Write minimal implementation**

Create `tools/smoke-cli.ts`:

```ts
// Run a built CLI binary and prove three things about it.
//
// Cross-compiling is not cross-testing: a Windows binary built on Linux has
// never executed. So each OS downloads its own artifact and runs this.
//
// A Bun script rather than one bash fragment and one PowerShell fragment,
// which would drift and neither of which could be tested. It imports
// nothing outside Node built-ins, so a Windows runner needs no install.
//
//   bun tools/smoke-cli.ts --binary ./screepub

import { existsSync, mkdtempSync, openSync, readSync, closeSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseArgs } from 'node:util';

export interface RunResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}
export type Runner = (argv: string[]) => RunResult;

const realRun: Runner = (argv) => {
  const proc = Bun.spawnSync(argv, { stdout: 'pipe', stderr: 'pipe' });
  return {
    exitCode: proc.exitCode ?? 1,
    stdout: proc.stdout.toString(),
    stderr: proc.stderr.toString(),
  };
};

/** The --json contract is ONE parseable object on stdout, at every exit.
 *  Checking only the first line would wave through a binary that printed a
 *  progress line and then a result. */
export function soleJson(stdout: string, what: string): Record<string, unknown> {
  const lines = stdout.split('\n').filter((l) => l.trim());
  if (lines.length !== 1) {
    throw new Error(
      `smoke: ${what} printed ${lines.length} lines on stdout under --json; the contract is exactly one`,
    );
  }
  try {
    return JSON.parse(lines[0]!) as Record<string, unknown>;
  } catch {
    throw new Error(`smoke: ${what} did not print parseable JSON: ${lines[0]!.slice(0, 200)}`);
  }
}

export function checkVersion(result: RunResult, expected: string): void {
  if (result.exitCode !== 0) {
    throw new Error(`smoke: --version exited ${result.exitCode}: ${result.stderr.trim().slice(0, 500)}`);
  }
  const line = result.stdout.trim();
  if (line !== `screepub ${expected}`) {
    throw new Error(`smoke: --version printed "${line}", expected "screepub ${expected}"`);
  }
}

export function checkConvertResult(result: RunResult, epubPath: string): void {
  if (result.exitCode !== 0) {
    throw new Error(`smoke: convert exited ${result.exitCode}: ${result.stderr.trim().slice(0, 500)}`);
  }
  const json = soleJson(result.stdout, 'convert');
  if (json.ok !== true) throw new Error(`smoke: convert reported ${JSON.stringify(json)}`);
  if (json.epubPath !== epubPath) {
    throw new Error(`smoke: convert wrote ${String(json.epubPath)}, expected ${epubPath}`);
  }
  if (typeof json.pages !== 'number' || json.pages <= 0) {
    throw new Error(`smoke: convert reported ${String(json.pages)} pages; an empty book is not a conversion`);
  }
}

/** An EPUB is a zip container. A CLI can report success and still have
 *  written nothing usable. */
export function checkEpubBytes(head: Uint8Array): void {
  const isZip = head[0] === 0x50 && head[1] === 0x4b && head[2] === 0x03 && head[3] === 0x04;
  if (!isZip) throw new Error('smoke: the output is not a zip container; no EPUB was written');
}

/** An empty list is a PASS: it proves the command ran on this OS, which is
 *  the whole point. No runner has an e-reader plugged into it. */
export function checkDevices(result: RunResult): void {
  if (result.exitCode !== 0) {
    throw new Error(
      `smoke: devices --json exited ${result.exitCode}: ${result.stderr.trim().slice(0, 500)}`,
    );
  }
  const json = soleJson(result.stdout, 'devices --json');
  if (json.ok !== true) throw new Error(`smoke: devices --json reported ${JSON.stringify(json)}`);
  if (!Array.isArray(json.devices)) {
    throw new Error('smoke: devices --json carried no devices array');
  }
}

export function smokeCli(
  binary: string,
  fixture: string,
  workDir: string,
  expectedVersion: string,
  run: Runner = realRun,
): void {
  if (!existsSync(binary)) throw new Error(`smoke: no binary at ${binary}`);
  if (!existsSync(fixture)) throw new Error(`smoke: no fixture at ${fixture}`);

  checkVersion(run([binary, '--version']), expectedVersion);

  const epub = join(workDir, 'smoke.epub');
  checkConvertResult(run([binary, fixture, '-o', epub, '--no-fountain', '--json']), epub);
  if (!existsSync(epub)) throw new Error(`smoke: convert reported success but ${epub} is not there`);
  const fd = openSync(epub, 'r');
  try {
    const head = new Uint8Array(8);
    const read = readSync(fd, head, 0, 8, 0);
    checkEpubBytes(head.subarray(0, read));
  } finally {
    closeSync(fd);
  }

  checkDevices(run([binary, 'devices', '--json']));
}

if (import.meta.main) {
  try {
    const { values } = parseArgs({
      args: Bun.argv.slice(2),
      options: {
        binary: { type: 'string' },
        fixture: { type: 'string' },
      },
      strict: true,
      allowPositionals: false,
    });
    if (!values.binary) throw new Error('smoke: --binary <path> is required');
    const repo = join(import.meta.dir, '..');
    const fixture = values.fixture ?? join(repo, 'tests', 'fixtures', 'screenplay.pdf');
    const version = (
      JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8')) as { version: string }
    ).version;
    const work = mkdtempSync(join(tmpdir(), 'screepub-smoke-'));
    smokeCli(values.binary, fixture, work, version);
    console.log(`smoke: ${values.binary} converts, reports ${version}, and lists devices`);
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 62, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add tools/smoke-cli.ts tests/smoke-cli.test.ts
git commit -m "$(cat <<'EOF'
What a per-OS job must prove about a binary it has never run

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 11: One real build, end to end

Everything above is tested with crafted bytes and injected seams, which is what keeps it fast and precise — and which is also exactly how a build tool can be entirely green and produce nothing. This test compiles for real, packages for real, verifies at the **production floors**, checks the checksums against independently computed digests, and — when the host can execute one of the targets — runs the binary through the full smoke path.

Two targets, not three: linux-arm64 and linux-x64 share a code path, so the pair worth exercising is *a tar.gz Linux target* and *the zip Windows target*. The Linux one is the host's own architecture wherever the host has one, so the binary that gets run is a binary this machine can actually run. On a Mac, `hostTarget()` is `undefined` by design, the Linux target falls back to linux-x64, and the smoke half self-skips with a printed note; CI's Ubuntu jobs cover it there.

Measured cost on this machine: compiles are sub-second warm and about 2 s cold, `tar -czf` of a 103 MB binary is 3.1 s, the zip is 4.4 s, and reading both back is a few seconds more — roughly 15-20 s added to a 9 s suite.

**Files:**
- Create: `tests/build-cli-e2e.test.ts`

**Interfaces:**
- Consumes: everything exported from `tools/build-cli.ts` and `smokeCli` from `tools/smoke-cli.ts`.

- [ ] **Step 1: Write the failing test**

Create `tests/build-cli-e2e.test.ts`:

```ts
import { describe, test, expect, afterAll } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import {
  TARGETS,
  RELEASE_FLOORS,
  archiveEntries,
  archivePath,
  binaryPath,
  buildAll,
  detectBinaryFormat,
  hostTarget,
  parseChecksums,
  readBinaryFormat,
  verifyArtifact,
  type Target,
  type TargetId,
} from '../tools/build-cli';
import { smokeCli } from '../tools/smoke-cli';

// A real build writes ~200 MB. It goes to a scratch directory outside the
// repo and is removed whatever happens.
const OUT = mkdtempSync(join(tmpdir(), 'screepub-e2e-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const VERSION = (JSON.parse(readFileSync('package.json', 'utf8')) as { version: string }).version;
const HOST = hostTarget();
// A tar.gz Linux target (the host's own architecture where there is one, so
// the binary can actually be run) and the zip Windows target. linux-arm64
// and linux-x64 share every code path, so building both proves nothing the
// pair below does not.
const LINUX: Target = HOST && HOST.id.startsWith('linux')
  ? HOST
  : TARGETS.find((t) => t.id === 'linux-x64')!;
const WINDOWS: Target = TARGETS.find((t) => t.id === 'windows-x64')!;
const ONLY: TargetId[] = [LINUX.id, WINDOWS.id];

describe('a real cross-compile', () => {
  test('produces genuine binaries, archives and checksums', async () => {
    const made = await buildAll({ version: VERSION, outDir: OUT, only: ONLY });
    expect(made.map((p) => basename(p))).toEqual([LINUX.archiveName, WINDOWS.archiveName]);
    // Absolute, and inside the scratch directory: nothing landed in the repo.
    for (const p of made) expect(p.startsWith(OUT)).toBe(true);

    for (const target of [LINUX, WINDOWS]) {
      // Asserted here directly, not by calling verifyArtifact: that is the
      // code under test, and a verifier that checked nothing would agree
      // with itself perfectly.
      const bin = binaryPath(target, OUT);
      expect(readBinaryFormat(bin)).toBe(target.format);
      expect(statSync(bin).size).toBeGreaterThan(RELEASE_FLOORS.binaryBytes);

      const arc = archivePath(target, OUT);
      expect(statSync(arc).size).toBeGreaterThan(RELEASE_FLOORS.archiveBytes);
      const entries = await archiveEntries(arc);
      expect(entries.map((e) => e.name)).toEqual([target.binaryName]);
      expect(entries[0]!.size).toBe(statSync(bin).size);
      expect(entries[0]!.mode & 0o111).not.toBe(0);
      expect(detectBinaryFormat(entries[0]!.data.subarray(0, 4096))).toBe(target.format);
    }

    // The Windows artifact is a PE and the Linux one is an ELF: neither is
    // the host's build by accident.
    expect(readBinaryFormat(binaryPath(WINDOWS, OUT))).toBe('pe-x86-64');
    expect(readBinaryFormat(binaryPath(LINUX, OUT)).startsWith('elf-')).toBe(true);

    const sums = parseChecksums(readFileSync(join(OUT, 'SHA256SUMS'), 'utf8'));
    expect([...sums.keys()].sort()).toEqual([LINUX.archiveName, WINDOWS.archiveName].sort());
    for (const [name, digest] of sums) {
      expect(digest).toBe(createHash('sha256').update(readFileSync(join(OUT, name))).digest('hex'));
    }
  }, 600000);

  test.skipIf(!HOST || HOST.id !== LINUX.id)(
    'the host-architecture binary actually converts the committed fixture',
    () => {
      // The one place a compiled artifact is executed locally. On a Mac
      // there is no host target for this tool and this self-skips; CI's
      // Ubuntu jobs cover it there.
      smokeCli(binaryPath(LINUX, OUT), 'tests/fixtures/screenplay.pdf', OUT, VERSION);
    },
    300000,
  );

  test('verification is not vacuous: a truncated binary is rejected', async () => {
    // Runs last, because it destroys the artifact. Proves the PRODUCTION
    // floors reject something the happy path just accepted, so a green run
    // above is evidence and not a tautology.
    await expect(verifyArtifact(WINDOWS, OUT)).resolves.toBeUndefined();
    writeFileSync(binaryPath(WINDOWS, OUT), new Uint8Array(1024));
    await expect(verifyArtifact(WINDOWS, OUT)).rejects.toThrow(/floor/);
  }, 300000);
});
```

- [ ] **Step 2: Run test to verify it fails**

Before implementing anything, prove the test can fail rather than pass by accident. Temporarily change `RELEASE_FLOORS.binaryBytes` in `tools/build-cli.ts` to `0` and re-run: the truncated-binary test must fail. Then temporarily change `windows-x64`'s `format` to `'elf-x86-64'` and re-run: the format assertions must fail. Restore both.

Run: `bun test tests/build-cli-e2e.test.ts`
Expected: with the tampering in place, FAIL; restored, PASS in roughly 15-20 s.

- [ ] **Step 3: Write minimal implementation**

No source change — Tasks 1-10 are the implementation. If this test fails, the defect is in `build-cli.ts` or `smoke-cli.ts`, not here.

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git status --short`
Expected: PASS — 799 + 65, `tsc` clean, and `git status` shows only the intended source files: **no artifact anywhere in the tree.**

- [ ] **Step 5: Commit**

```bash
git add tests/build-cli-e2e.test.ts
git commit -m "$(cat <<'EOF'
Build all of it for real, once, and run what this machine can run

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 12: The macOS path, pinned

The decisive constraint of this piece is negative: the macOS artifacts must keep being produced by exactly the code that produces them today. `app/release.sh` signs and notarizes them, and notarization needs Apple's toolchain on a Mac — a cross-compiled Mach-O could not be notarized from Linux, so "unifying" the builds would trade a Gatekeeper-clean download for a scary warning. `tools/bump-tap.sh` then hardcodes the two filenames, and the tap has its own freshness alarm.

These tests are the standing guard. The byte-for-byte proof is in Final Verification; what lives here is the invariant that outlives this piece — the names, and the absence of a macOS row in the new builder.

The action-pin test is here for the same reason: Task 14 adds two third-party actions, the repo pins every action to a SHA, and an unpinned one is the kind of thing that gets noticed a year later.

**Files:**
- Create: `tests/release-artifacts.test.ts`

**Interfaces:**
- Consumes: `TARGETS` from `tools/build-cli.ts`; the workflow and script files as text.

- [ ] **Step 1: Write the failing test**

Create `tests/release-artifacts.test.ts`:

```ts
import { describe, test, expect } from 'bun:test';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { TARGETS } from '../tools/build-cli';

const WORKFLOWS = '.github/workflows';
const read = (p: string) => readFileSync(p, 'utf8');

const MACOS_ASSETS = [
  'Screepub-macOS.dmg',
  'screepub-cli-macos-arm64.tar.gz',
  'screepub-cli-macos-x64.tar.gz',
];

describe('the macOS release path is untouched by the cross-platform builder', () => {
  test('the cross builder emits no macOS artifact, by name or by target', () => {
    // app/release.sh signs and NOTARIZES the macOS binaries, which needs
    // Apple's toolchain on a Mac. A cross-compiled Mach-O could not be
    // notarized, so an added darwin row here would quietly replace a
    // Gatekeeper-clean download with a warning screen.
    for (const t of TARGETS) {
      expect(t.bunTarget).not.toContain('darwin');
      expect(t.format.startsWith('macho')).toBe(false);
      expect(t.archiveName.toLowerCase()).not.toContain('macos');
      expect(t.archiveName.toLowerCase()).not.toContain('darwin');
    }
    for (const name of MACOS_ASSETS) {
      expect(TARGETS.some((t) => t.archiveName === name)).toBe(false);
    }
  });

  test('app/release.sh still produces the two tarballs and the DMG', () => {
    const sh = read('app/release.sh');
    expect(sh).toContain('screepub-cli-macos-$ARCH.tar.gz');
    expect(sh).toContain('Screepub-macOS.dmg');
    expect(sh).toContain('notarytool submit');
  });

  test('tools/bump-tap.sh still names exactly the macOS assets, and nothing else', () => {
    // The Homebrew tap serves the macOS CLI. It must not learn about the
    // Linux or Windows artifacts: brew has no business installing either,
    // and tap-freshness.yml would go red on a formula it cannot audit.
    const sh = read('tools/bump-tap.sh');
    for (const name of MACOS_ASSETS) expect(sh).toContain(name);
    expect(sh.toLowerCase()).not.toContain('linux');
    expect(sh.toLowerCase()).not.toContain('windows');
    for (const t of TARGETS) expect(sh).not.toContain(t.archiveName);
  });
});

describe('every workflow action is pinned to a commit', () => {
  test('no `uses:` rides a mutable tag', () => {
    const offenders: string[] = [];
    for (const file of readdirSync(WORKFLOWS).filter((f) => f.endsWith('.yml'))) {
      for (const line of read(join(WORKFLOWS, file)).split('\n')) {
        const m = /^\s*-?\s*uses:\s*(\S+)/.exec(line);
        if (!m) continue;
        if (!/@[0-9a-f]{40}$/.test(m[1]!)) offenders.push(`${file}: ${m[1]}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts`
Expected: PASS immediately — these are regression pins, not drivers. Prove they can fail before trusting them:

1. Temporarily add a `darwin-arm64` row to `TARGETS` with `archiveName: 'screepub-cli-macos-arm64.tar.gz'` and re-run: the first two assertions must fail. Remove it.
2. Temporarily append `screepub-cli-linux-x64.tar.gz` to a comment in `tools/bump-tap.sh` and re-run: the tap test must fail. **Revert with `git checkout -- tools/bump-tap.sh`** — that file is otherwise untouchable.
3. Temporarily change one `uses:` in `ci.yml` to `actions/checkout@v7` and re-run: the pin test must fail. Revert.

- [ ] **Step 3: Write minimal implementation**

No source change.

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git status --short app/ tools/bump-tap.sh tools/check-tap.sh`
Expected: PASS — 799 + 69, `tsc` clean, and the `git status` prints **nothing**.

- [ ] **Step 5: Commit**

```bash
git add tests/release-artifacts.test.ts
git commit -m "$(cat <<'EOF'
Pin the macOS release path so adding platforms cannot move it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 13: CI gets the cross-compile

The spec asks for the cross-compile as a cheap early signal, so a target breaking is a red push rather than a surprise at tag time. Ubuntu is free, the whole job is about a minute, and it also runs the linux-x64 artifact through the real smoke path — so on every push, one compiled binary is genuinely executed, converted with, and asked for its device list.

This does **not** replace the existing `artifact` job on macOS: that one builds the host binary and runs `epubcheck`, which is the project's stated validity gate and needs Homebrew.

**Files:**
- Modify: `.github/workflows/ci.yml`
- Test: `tests/release-artifacts.test.ts` (append)

**Interfaces:**
- Consumes: `tools/build-cli.ts`, `tools/smoke-cli.ts`.
- Produces: a `cross-cli` job in `ci.yml`.

- [ ] **Step 1: Write the failing test**

Append to `tests/release-artifacts.test.ts` (add `describe`-level YAML parsing):

```ts
interface Job {
  'runs-on'?: string;
  needs?: string | string[];
  permissions?: Record<string, string>;
  steps?: { name?: string; uses?: string; run?: string; shell?: string; with?: Record<string, unknown> }[];
}
interface Workflow {
  jobs: Record<string, Job>;
}

const workflow = (file: string): Workflow =>
  Bun.YAML.parse(read(join(WORKFLOWS, file))) as Workflow;

const runText = (job: Job): string => (job.steps ?? []).map((s) => s.run ?? '').join('\n');

describe('ci.yml cross-compiles on every push', () => {
  const ci = workflow('ci.yml');

  test('a cross-cli job builds every target on a free runner', () => {
    const job = ci.jobs['cross-cli'];
    expect(job).toBeDefined();
    expect(job!['runs-on']).toBe('ubuntu-latest');
    const text = runText(job!);
    expect(text).toContain('tools/build-cli.ts');
    // No --only: the point is that ALL THREE targets keep compiling.
    expect(text).not.toContain('--only');
  });

  test('it runs the linux-x64 artifact rather than only building it', () => {
    const text = runText(ci.jobs['cross-cli']!);
    expect(text).toContain('screepub-cli-linux-x64.tar.gz');
    expect(text).toContain('tools/smoke-cli.ts');
  });

  test('the macOS artifact job is untouched and still runs epubcheck', () => {
    // The compiled-binary + epubcheck gate predates this piece and is not
    // replaced by the cheap Linux one.
    const artifact = ci.jobs['artifact'];
    expect(artifact!['runs-on']).toBe('macos-15');
    expect(runText(artifact!)).toContain('epubcheck');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — there is no `cross-cli` job in `ci.yml`.

- [ ] **Step 3: Write minimal implementation**

Append to `.github/workflows/ci.yml`, after the `artifact` job:

```yaml
  # Bun cross-compiles every target from one machine, so a broken target is
  # a red push instead of a surprise at tag time. The verification lives in
  # tools/build-cli.ts, which is unit-tested and hand-runnable; this job is
  # a thin caller, because a build matrix expressed in YAML can only ever be
  # tested by cutting a release.
  #
  # It does NOT replace the `artifact` job above: that one runs epubcheck,
  # the project's stated validity gate, and needs Homebrew.
  cross-cli:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
        with:
          bun-version: 1.3.14
      - run: bun install --frozen-lockfile
      - name: Cross-compile every CLI target and verify the binary formats
        run: |
          set -euo pipefail
          V="$(bun --print "require('./package.json').version")"
          bun tools/build-cli.ts --version "$V" --out "$RUNNER_TEMP/cli"
      - name: The linux-x64 artifact actually runs here
        run: |
          set -euo pipefail
          tar -xzf "$RUNNER_TEMP/cli/screepub-cli-linux-x64.tar.gz" -C "$RUNNER_TEMP"
          bun tools/smoke-cli.ts --binary "$RUNNER_TEMP/screepub"
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 72, `tsc` clean. Also confirm the file still parses as a workflow:

```bash
bun -e 'console.log(Object.keys((Bun.YAML.parse(require("fs").readFileSync(".github/workflows/ci.yml","utf8")) as any).jobs))'
```
Expected: `[ "engine", "app", "artifact", "cross-cli" ]`.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml tests/release-artifacts.test.ts
git commit -m "$(cat <<'EOF'
Every push cross-compiles, and runs the Linux build it made

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 14: The release workflow gains four jobs

Build once, test per OS, then upload. `cross-cli` builds all three artifacts on a free Ubuntu runner and hands them on; `smoke-linux-x64` and `smoke-windows-x64` each download their own artifact and run it on its own operating system; `cross-upload` attaches them to the release only after both pass. The Windows smoke job is the first execution the Windows binary will ever have had, so it gating the upload is the entire point of the arrangement.

**The existing jobs are not edited.** `checks`, `release`, `tap` and `tap-check` keep their current text byte for byte, the release is still created and un-drafted by the macOS `release` job, and the macOS assets still go up from there. The new assets arrive a minute later; that is preferable to moving the un-draft into a job whose failure would leave a release permanently in draft.

`cross-upload` has no checkout, so `gh` is told which repository it is talking to via `GH_REPO`.

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `tests/release-artifacts.test.ts` (append)

**Interfaces:**
- Consumes: `tools/build-cli.ts`, `tools/smoke-cli.ts`.
- Produces: `cross-cli`, `smoke-linux-x64`, `smoke-windows-x64`, `cross-upload` jobs.

- [ ] **Step 1: Write the failing test**

Append to `tests/release-artifacts.test.ts`:

```ts
describe('release.yml ships the cross-platform artifacts', () => {
  const rel = workflow('release.yml');
  const needs = (name: string): string[] => {
    const n = rel.jobs[name]?.needs;
    return Array.isArray(n) ? n : n ? [n] : [];
  };

  test('the four existing jobs are still there, in their existing shape', () => {
    expect(Object.keys(rel.jobs)).toEqual(
      expect.arrayContaining(['checks', 'release', 'tap', 'tap-check']),
    );
    expect(rel.jobs['release']!['runs-on']).toBe('macos-15');
    expect(needs('release')).toEqual(['checks']);
    expect(needs('tap')).toEqual(['release']);
    expect(needs('tap-check')).toEqual(['tap']);
  });

  test('the macOS release job still builds, signs and uploads exactly its three assets', () => {
    const text = runText(rel.jobs['release']!);
    expect(text).toContain('app/release.sh');
    for (const name of MACOS_ASSETS) expect(text).toContain(`app/dist/${name}`);
    // And it has NOT quietly become responsible for the new ones.
    for (const t of TARGETS) expect(text).not.toContain(t.archiveName);
  });

  test('cross-cli builds every artifact once, after the checks pass', () => {
    const job = rel.jobs['cross-cli'];
    expect(job).toBeDefined();
    expect(job!['runs-on']).toBe('ubuntu-latest');
    expect(needs('cross-cli')).toEqual(['checks']);
    const text = runText(job!);
    expect(text).toContain('tools/build-cli.ts');
    expect(text).not.toContain('--only');
    // The version comes from the TAG, so a mis-set package.json fails the
    // build instead of shipping a binary that misreports itself.
    expect(text).toContain('${TAG#v}');
    const upload = (job!.steps ?? []).find((s) => (s.uses ?? '').includes('upload-artifact'));
    expect(upload).toBeDefined();
    expect(String(upload!.with?.['if-no-files-found'])).toBe('error');
  });

  test('each smoke job downloads its own artifact and runs it on its own OS', () => {
    expect(rel.jobs['smoke-linux-x64']!['runs-on']).toBe('ubuntu-latest');
    expect(rel.jobs['smoke-windows-x64']!['runs-on']).toBe('windows-latest');
    for (const name of ['smoke-linux-x64', 'smoke-windows-x64']) {
      expect(needs(name)).toEqual(['cross-cli']);
      const job = rel.jobs[name]!;
      expect((job.steps ?? []).some((s) => (s.uses ?? '').includes('download-artifact'))).toBe(true);
      expect(runText(job)).toContain('tools/smoke-cli.ts');
    }
    expect(runText(rel.jobs['smoke-linux-x64']!)).toContain('screepub-cli-linux-x64.tar.gz');
    const win = rel.jobs['smoke-windows-x64']!;
    expect(runText(win)).toContain('screepub-cli-windows-x64.zip');
    expect((win.steps ?? []).some((s) => s.shell === 'pwsh')).toBe(true);
  });

  test('nothing is uploaded until both smoke jobs have passed', () => {
    // The Windows binary has never executed anywhere before that job. If
    // the upload did not wait for it, the smoke test would be decoration.
    expect(needs('cross-upload').sort()).toEqual(
      ['release', 'smoke-linux-x64', 'smoke-windows-x64'],
    );
    expect(rel.jobs['cross-upload']!.permissions?.contents).toBe('write');
  });

  test('cross-upload attaches all three archives and the checksums', () => {
    const text = runText(rel.jobs['cross-upload']!);
    for (const t of TARGETS) expect(text).toContain(t.archiveName);
    expect(text).toContain('SHA256SUMS');
    expect(text).toContain('--clobber');
    // No checkout in that job, so gh must be told the repository.
    const env = JSON.stringify(rel.jobs['cross-upload']!.steps ?? []);
    expect(env).toContain('GH_REPO');
    // It must not touch the macOS assets the release job already uploaded.
    for (const name of MACOS_ASSETS) expect(text).not.toContain(name);
  });

  test('linux-arm64 is built and shipped but never smoke-tested', () => {
    // Stated, not hidden: no arm64 runner is assumed available, so this
    // artifact ships untested and the release notes say so. If an arm64
    // runner is ever added, this assertion is the one to delete.
    const jobs = Object.keys(rel.jobs);
    expect(jobs).not.toContain('smoke-linux-arm64');
    expect(runText(rel.jobs['cross-upload']!)).toContain('screepub-cli-linux-arm64.tar.gz');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts`
Expected: FAIL — there is no `cross-cli` job in `release.yml`.

- [ ] **Step 3: Write minimal implementation**

First get the two action pins (the repo pins every action to a commit, and Task 12's test enforces it):

```bash
gh api repos/actions/upload-artifact/commits/v4 -q .sha
gh api repos/actions/download-artifact/commits/v4 -q .sha
```

Use those forty-hex values below in place of `ea165f8d65b6e75b540449e92b4886f43607fa02` / `d3f86a106a0bac45b974a628896c90dbdf5c8093`, keeping the `# v4` comment beside each, exactly as the existing pins are written. (`pinact run` does the same job if it is installed.)

Append to `.github/workflows/release.yml`, after the `tap-check` job:

```yaml
  # Bun cross-compiles every target from one machine, so the Linux and
  # Windows artifacts cost one free Ubuntu runner rather than a per-OS build
  # matrix. The macOS artifacts are NOT built here: app/release.sh signs and
  # notarizes them on the macOS runner above, notarization needs Apple's
  # toolchain, and tools/bump-tap.sh hardcodes their filenames.
  #
  # The matrix and every verification live in tools/build-cli.ts, which is
  # unit-tested and hand-runnable. This job is a thin caller on purpose:
  # workflow YAML can only be tested by cutting a release.
  cross-cli:
    needs: checks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
        with:
          bun-version: 1.3.14
      - run: bun install --frozen-lockfile
      - name: Cross-compile and verify the Linux and Windows CLI artifacts
        env:
          TAG: ${{ github.ref_name }}
        run: |
          set -euo pipefail
          bun tools/build-cli.ts --version "${TAG#v}" --out "$RUNNER_TEMP/cli"
      - uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4
        with:
          name: cross-cli
          path: |
            ${{ runner.temp }}/cli/screepub-cli-linux-x64.tar.gz
            ${{ runner.temp }}/cli/screepub-cli-linux-arm64.tar.gz
            ${{ runner.temp }}/cli/screepub-cli-windows-x64.zip
            ${{ runner.temp }}/cli/SHA256SUMS
          # The upload exists to hand these to the smoke jobs and the
          # release; once the release page has them, keeping ~250 MB of
          # duplicates for 90 days buys nothing.
          retention-days: 1
          if-no-files-found: error

  # Cross-compiling is not cross-testing. Each artifact is run on the OS it
  # was built for, converting the committed fixture and listing devices.
  #
  # linux-arm64 has no job here: no arm64 runner is assumed available to
  # this repo. It ships built and verified but never executed in CI, and the
  # release notes say so rather than implying otherwise.
  smoke-linux-x64:
    needs: cross-cli
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
        with:
          bun-version: 1.3.14
      - uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4
        with:
          name: cross-cli
          path: cli
      - name: The linux-x64 binary converts the fixture and lists devices
        run: |
          set -euo pipefail
          tar -xzf cli/screepub-cli-linux-x64.tar.gz -C "$RUNNER_TEMP"
          bun tools/smoke-cli.ts --binary "$RUNNER_TEMP/screepub"

  # Nobody on this project has a Windows machine. This job is the first time
  # the Windows binary will ever have executed, which is why cross-upload
  # waits for it.
  smoke-windows-x64:
    needs: cross-cli
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0
        with:
          bun-version: 1.3.14
      - uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4
        with:
          name: cross-cli
          path: cli
      - name: The windows-x64 binary converts the fixture and lists devices
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          Expand-Archive -Path cli/screepub-cli-windows-x64.zip -DestinationPath $env:RUNNER_TEMP -Force
          bun tools/smoke-cli.ts --binary "$env:RUNNER_TEMP\screepub.exe"
          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  # Attach the new artifacts to the release the macOS job already published.
  # Deliberately a SEPARATE job that runs after it, rather than moving the
  # un-draft down here: a failure in this path would otherwise leave the
  # release permanently in draft, which is a worse outcome than the Linux
  # and Windows downloads appearing a minute after the DMG.
  #
  # SHA256SUMS covers these three files. The macOS checksums stay in the
  # release body, where they have always been.
  cross-upload:
    needs: [release, smoke-linux-x64, smoke-windows-x64]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4
        with:
          name: cross-cli
          path: cli
      - name: Upload the Linux and Windows CLI artifacts
        env:
          GH_TOKEN: ${{ github.token }}
          # No checkout in this job, so gh has no repository to infer.
          GH_REPO: ${{ github.repository }}
          TAG: ${{ github.ref_name }}
        run: |
          set -euo pipefail
          gh release upload "$TAG" \
            cli/screepub-cli-linux-x64.tar.gz \
            cli/screepub-cli-linux-arm64.tar.gz \
            cli/screepub-cli-windows-x64.zip \
            cli/SHA256SUMS \
            --clobber
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git diff --stat main -- app/ tools/bump-tap.sh tools/check-tap.sh .github/workflows/tap-freshness.yml`
Expected: PASS — 799 + 79, `tsc` clean, and the `git diff --stat` prints **nothing**. Then confirm the graph:

```bash
bun -e 'const w=Bun.YAML.parse(require("fs").readFileSync(".github/workflows/release.yml","utf8"));
for (const [k,v] of Object.entries(w.jobs)) console.log(k, "<-", JSON.stringify(v.needs ?? null), v["runs-on"]);'
```
Expected: the four original jobs unchanged, plus `cross-cli <- "checks"`, both smoke jobs `<- "cross-cli"`, and `cross-upload <- ["release","smoke-linux-x64","smoke-windows-x64"]`.

Also re-run the bash-3.2 guard, since `release.yml` changed: `bun test tests/workflow-shell.test.ts` must still pass (none of the new YAML uses a bash array).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml tests/release-artifacts.test.ts
git commit -m "$(cat <<'EOF'
Build once, run it on its own OS, then let it onto the release

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 15: Say the two things out loud

Both limits go where a reader meets them, not where they discover them.

**Windows binaries are unsigned.** Authenticode certificates cost money and that is out of scope by explicit instruction, so SmartScreen will warn on first run. A reader who has been told that clicks through it; a reader who has not assumes the download is malware.

**Device support on Windows and Linux is unproven on hardware.** The device code has met exactly one real Kindle, on a Mac. Windows volume enumeration has never run at all, and a tolino cannot be detected on Windows even in principle: it is identified by volume name, and a Windows drive root carries none.

One more guard rides along. `release.yml`'s `checks` job greps the notes for `sha-?256` and fails the tag, because two checksum lists on one page both look official — and it fails *after* the tag is pushed. Adding the term to the in-suite banned list moves that failure from minutes-after-tagging to nine seconds.

**Files:**
- Create: `docs/releases/0.6.0.md`
- Modify: `README.md`, `tests/release-notes.test.ts`

**Interfaces:**
- Consumes: the existing `BANNED` list and word cap in `tests/release-notes.test.ts`.

- [ ] **Step 1: Write the failing test**

In `tests/release-notes.test.ts`, extend `BANNED`:

```ts
const BANNED = [
  'kepub',
  'sideload',
  'ragged-right',
  'keep-together',
  'rendering engine',
  'stylesheet',
  '—', // em dash: house rule for user-facing copy
  // release.yml's `checks` job fails a tag whose notes name a checksum,
  // because the workflow appends the real ones and two lists on one page
  // both look official. That check runs AFTER the tag is pushed; this one
  // runs in nine seconds.
  'sha256',
  'sha-256',
];
```

Then append to `tests/release-artifacts.test.ts`:

```ts
describe('the two limits are stated where a reader meets them', () => {
  const readme = read('README.md');

  test('the README says the Windows download is unsigned', () => {
    const lower = readme.toLowerCase();
    expect(lower).toContain('windows');
    expect(lower).toContain('smartscreen');
    expect(/not signed|unsigned/.test(lower)).toBe(true);
  });

  test('the README names each Linux and Windows artifact it tells people to download', () => {
    for (const t of TARGETS) expect(readme).toContain(t.archiveName);
  });

  test('the README says device support off macOS is unproven, and names the tolino case', () => {
    const lower = readme.toLowerCase();
    expect(lower).toContain('tolino');
    // The specific, checkable claim: on Windows a tolino cannot be detected
    // at all, because it is identified by volume name and a Windows drive
    // root carries none.
    expect(/tolino[^.]*windows|windows[^.]*tolino/s.test(lower)).toBe(true);
  });

  test('the 0.6.0 notes carry both limits', () => {
    const notes = read('docs/releases/0.6.0.md').toLowerCase();
    expect(notes).toContain('windows');
    expect(/not signed|unsigned/.test(notes)).toBe(true);
    expect(notes).toContain('tolino');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/release-artifacts.test.ts tests/release-notes.test.ts`
Expected: FAIL — `docs/releases/0.6.0.md` does not exist and the README says nothing about Windows.

- [ ] **Step 3: Write minimal implementation**

Create `docs/releases/0.6.0.md`. (If a later piece has already created it, add these sections rather than replacing the file.) Note the constraints: no em dash, no banned term, no word for a checksum, 350 words maximum.

```markdown
# Screepub 0.6.0

Screepub now runs on Linux and Windows, from the command line.

## Screepub runs where you work

- **Linux and Windows downloads.** The converter ships as one file for Linux,
  on both Intel and ARM, and for Windows. Unpack it, run it, convert a script.
  There is nothing else to install.
- **The Mac app has not changed.** Same download, same signing by Apple, same
  behaviour. If you use Screepub on a Mac, this release asks nothing of you.

## Getting a script onto a reader

- **Two commands on the command line.** `screepub devices` lists the readers it
  can see, and `screepub send` copies a finished file onto one.

## Good to know

- **Windows will warn you the first time you run it.** The Windows download
  carries no certificate, so Windows shows a "publisher unknown" screen. Choose
  More info, then Run anyway. A certificate costs money this project does not
  spend yet, and we would rather tell you than let you meet that screen alone.
- **Sending to a reader has only been tried on a Mac, with a Kindle.**
  Everything else is built and code-tested but has never met real hardware. On
  Windows in particular, a tolino cannot be found at all: it is recognised by
  the name of its drive, and a Windows drive root has no name to read.
- **The ARM Linux download is built and checked but never run by our tests.**
  Nothing in our automation has an ARM machine to run it on. It is the version
  the author uses daily, which is not the same as proof.
```

Then in `README.md`, after the `**Requirements:** macOS 14 (Sonoma) or later…` paragraph in the **Install** section, insert:

````markdown
### Linux and Windows (command line)

There is no window to open yet on Linux or Windows: what ships is the
converter itself, run from a terminal. Download the file for your machine
from the [latest release](https://github.com/method-production-suite/method-reader/releases/latest),
unpack it, and run it.

| Machine | File |
| --- | --- |
| Linux, Intel or AMD | `screepub-cli-linux-x64.tar.gz` |
| Linux, ARM (Asahi, Raspberry Pi, ARM servers) | `screepub-cli-linux-arm64.tar.gz` |
| Windows, 64-bit | `screepub-cli-windows-x64.zip` |

```bash
tar -xzf screepub-cli-linux-x64.tar.gz
./screepub script.pdf
```

`SHA256SUMS` on the release page covers these three files, for anyone who
wants to check what they downloaded.

**Windows will warn you.** The Windows build carries no code-signing
certificate, so SmartScreen shows a "publisher unknown" screen the first time
you run it. Choose **More info**, then **Run anyway**. Certificates cost money
this project does not spend yet; this note exists so the warning is expected
rather than alarming.

**Device support off macOS is unproven.** `screepub devices` and
`screepub send` are built for all three platforms and code-tested on all
three, but the only device transfer anyone has ever run on real hardware was a
Kindle, on a Mac. Windows drive enumeration has never run against a real
reader, and a tolino cannot be detected on Windows at all: it is identified by
the name of its volume, and a Windows drive root carries none. Converting is
the part that is well tested everywhere; sending is not.
````

Finally, in the **Device commands** subsection, extend the existing hardware caveat paragraph so it ends with one more sentence:

```markdown
On Linux and Windows this caveat is stronger still: no device of any kind has
been connected to Screepub on either operating system. See
[Linux and Windows](#linux-and-windows-command-line) above.
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 799 + 83, `tsc` clean. Then run the tag-time check the release workflow will run, against the new notes:

```bash
grep -niE 'sha-?256|[0-9a-f]{64}' docs/releases/0.6.0.md && echo "FAIL: a tag would be rejected" || echo "ok"
wc -w docs/releases/0.6.0.md   # must be <= 350
```
Expected: `ok`, and a word count under 350.

- [ ] **Step 5: Commit**

```bash
git add docs/releases/0.6.0.md README.md tests/release-notes.test.ts tests/release-artifacts.test.ts
git commit -m "$(cat <<'EOF'
Say the unsigned Windows build and the unproven hardware out loud

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

## Final Verification

The spec's six acceptance criteria, as checkable steps.

- [ ] **1. `bun tools/build-cli.ts` produces all three artifacts plus `SHA256SUMS`, and fails loudly if any is missing, empty or the wrong format**

Run the tool by hand, outside the repo:

```bash
V="$(bun --print "require('./package.json').version")"
OUT="$(mktemp -d)"
bun tools/build-cli.ts --version "$V" --out "$OUT"
ls -l "$OUT"
cat "$OUT/SHA256SUMS"
file "$OUT"/*/screepub "$OUT"/*/screepub.exe
(cd "$OUT" && sha256sum -c SHA256SUMS)
```
Expected: three archives plus `SHA256SUMS`; `file` reports `ELF 64-bit … x86-64`, `ELF 64-bit … ARM aarch64` and `PE32+ … x86-64`; `sha256sum -c` prints three `OK` lines.

Now prove it refuses a build it should refuse, from the command line:

```bash
bun tools/build-cli.ts --version 99.0.0 --out "$OUT" ; echo "exit=$?"
bun tools/build-cli.ts --version "$V" --out "$OUT" --only darwin-arm64 ; echo "exit=$?"
rm -rf "$OUT"
```
Expected: the first prints the package.json mismatch and exits non-zero; the second names `darwin-arm64` and exits non-zero. **No macOS target can be built by this tool at all.**

The eight artifact-rejection branches (missing, 0-byte, small, wrong architecture, not a binary, missing archive, archive without the member, member without the executable bit) cannot be provoked from the command line, because a re-run simply recompiles a good artifact over the damaged one. They are covered by `bun test tests/build-cli.test.ts -t verifyArtifact`, and the production floors rejecting a truncated **real** binary is covered by `bun test tests/build-cli-e2e.test.ts -t "not vacuous"`. Run both and confirm they pass.

- [ ] **2a. The macOS artifacts, the DMG and the tap bump are byte-unchanged**

```bash
sha256sum app/release.sh app/build-lib.sh tools/bump-tap.sh tools/check-tap.sh
```
Expected, exactly:

```
5d1e1e729711a1b5c70e9f2300359bbea0481287ee5c94c6e7a28eff39ef7670  app/release.sh
424938158445b127389bed7bc6367c564254e0859a7e5c85c0a8fa3e13d841b9  app/build-lib.sh
f6245bcd9884ceeb0584f827b22e53c680252d1006fe491b64dc53dadf2a7f68  tools/bump-tap.sh
f1f1b1a430bd1b6b90c5c31dc2d1af5ada975a78a7ac7f84c8db0788d41db0ab  tools/check-tap.sh
```

- [ ] **2b. Nothing under `app/`, the tap scripts or the tap workflow is modified**

```bash
git diff --stat main -- app/ tools/bump-tap.sh tools/check-tap.sh .github/workflows/tap-freshness.yml
```
Expected: no output.

- [ ] **2c. The macOS half of `release.yml` is unchanged**

```bash
git diff main -- .github/workflows/release.yml | grep '^-' | grep -v '^---'
```
Expected: no output. Every hunk in that file is an addition; if a line was removed or altered inside `checks`, `release`, `tap` or `tap-check`, stop and restore it.

- [ ] **3. The release workflow uploads the new artifacts beside the old ones**

Run: `bun test tests/release-artifacts.test.ts`
Expected: pass. Then read the graph back from the file itself:

```bash
bun -e 'const w=Bun.YAML.parse(require("fs").readFileSync(".github/workflows/release.yml","utf8"));
for (const [k,v] of Object.entries(w.jobs)) console.log(k, "<-", JSON.stringify(v.needs ?? null), v["runs-on"]);'
```
Expected: `checks`, `release`, `tap`, `tap-check` exactly as before, plus `cross-cli <- "checks"` (ubuntu), `smoke-linux-x64 <- "cross-cli"` (ubuntu), `smoke-windows-x64 <- "cross-cli"` (windows-latest), `cross-upload <- ["release","smoke-linux-x64","smoke-windows-x64"]` (ubuntu). Confirm by eye that the `release` job's `gh release upload` still lists the DMG and the two macOS tarballs and nothing else.

- [ ] **4. linux-x64 and windows-x64 are smoke-tested on their own OS**

Run: `bun test tests/smoke-cli.test.ts tests/build-cli-e2e.test.ts`
Expected: pass, including the real host-binary smoke run (or its printed skip on a Mac). The per-OS half rides on the next tag by construction — a Windows binary cannot be executed here. What is verifiable now is that both jobs exist, run `tools/smoke-cli.ts`, each name their own archive, and that `cross-upload` waits for both; the test above asserts all four.

Confirm the two `--json` invocations the smoke tool depends on still behave, using this machine's own build:

```bash
OUT="$(mktemp -d)"
bun tools/build-cli.ts --version "$(bun --print "require('./package.json').version")" \
  --out "$OUT" --only "$(bun -e 'console.log(require("./tools/build-cli").hostTarget()?.id ?? "linux-x64")')"
bun tools/smoke-cli.ts --binary "$OUT"/*/screepub
rm -rf "$OUT"
```
Expected: `smoke: … converts, reports <version>, and lists devices`. On a Mac this step has no host target and is skipped.

- [ ] **5. The full existing suite passes, `tsc` is clean, nothing under `app/` is modified**

Run: `bun test && bunx tsc --noEmit && git status --short app/`
Expected: all pass (799 pre-existing + ~83 new), 3 skipped as before, `tsc` silent, `git status` empty.

Then confirm the committed fixtures are untouched and still reproduce:

```bash
bun test tests/fixture-stability.test.ts
git status --short tests/fixtures/
```
Expected: pass, and no output.

And confirm no artifact escaped into the tree:

```bash
git status --short
find . -name 'screepub-cli-*' -not -path './node_modules/*' -not -path './.git/*'
du -sh .git
```
Expected: `git status` shows only intended source files; `find` prints nothing; `.git` has not grown by hundreds of megabytes.

- [ ] **6. README and release notes state the unsigned-Windows and unproven-hardware limits**

Run: `bun test tests/release-artifacts.test.ts tests/release-notes.test.ts`
Expected: pass. Then read both by eye — a test can check that the word "SmartScreen" appears; only a person can check that the paragraph around it is honest and does not read as an apology or a dismissal.

Finally, run the tag-time notes checks that `release.yml` will run, before a tag exists to run them:

```bash
grep -niE 'sha-?256|[0-9a-f]{64}' docs/releases/0.6.0.md && echo "FAIL" || echo "ok"
wc -w docs/releases/0.6.0.md
iconv -f UTF-8 -t UTF-8 docs/releases/0.6.0.md >/dev/null && echo "utf8 ok"
```
Expected: `ok`, a word count at or under 350, `utf8 ok`.

- [ ] **7. The two tools really are the logic, and the YAML really is thin**

```bash
grep -c 'run:' .github/workflows/release.yml
grep -n 'bun build --compile' .github/workflows/*.yml
```
Expected: the second command prints **nothing**. If a `bun build --compile` has appeared in YAML, the build matrix has started leaking back into a place no test can reach, and the spec's central mitigation is gone.
