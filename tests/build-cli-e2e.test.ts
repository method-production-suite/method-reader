import { describe, test, expect, afterAll } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import {
  TARGETS,
  MAC_TARGETS,
  macTargetIdForHost,
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
      smokeCli(binaryPath(LINUX, OUT), 'tests/fixtures/screenplay.pdf', 'tests/fixtures/jpx.pdf', OUT, VERSION);
    },
    300000,
  );

  const MAC_HOST = MAC_TARGETS.find((t) => t.id === macTargetIdForHost(process.platform, process.arch));
  test.skipIf(!MAC_HOST)(
    'on a Mac: --mac compiles, SIGNS with the release flags, packages the signed bytes, and converts',
    async () => {
      // Ad-hoc (`-`) because no Developer ID is on a test machine, but
      // through the same codesign call, flags and entitlements a release
      // uses. Notarization is the one step only a release can exercise.
      const out = join(OUT, 'mac');
      const made = await buildAll({
        version: VERSION, outDir: out, only: [MAC_HOST!.id], mac: { sign: '-', notarize: false },
      });
      expect(made.map((p) => basename(p))).toEqual([MAC_HOST!.archiveName]);

      const bin = binaryPath(MAC_HOST!, out);
      expect(readBinaryFormat(bin)).toBe(MAC_HOST!.format);
      const described = Bun.spawnSync(['codesign', '-dv', bin], { stderr: 'pipe' }).stderr.toString();
      expect(described).toMatch(/flags=0x[0-9a-f]+\([^)]*runtime[^)]*\)/);
      const ents = Bun.spawnSync(['codesign', '-d', '--entitlements', '-', '--xml', bin], { stdout: 'pipe' })
        .stdout.toString();
      expect(ents).toContain('com.apple.security.cs.allow-jit');

      // The archive holds the SIGNED bytes: packaging came after signing.
      const [member] = await archiveEntries(archivePath(MAC_HOST!, out));
      const digest = (b: Uint8Array) => createHash('sha256').update(b).digest('hex');
      expect(digest(member!.data)).toBe(digest(readFileSync(bin)));

      smokeCli(bin, 'tests/fixtures/screenplay.pdf', 'tests/fixtures/jpx.pdf', out, VERSION);
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
