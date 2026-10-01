import { afterAll, describe, test, expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, chmodSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, isAbsolute } from 'node:path';
import { createHash } from 'node:crypto';
import {
  detectBinaryFormat,
  readBinaryFormat,
  TARGETS,
  buildDir,
  compileOutfile,
  binaryPath,
  archivePath,
  targetIdForHost,
  hostTarget,
  parseBuildArgs,
  assertPackageVersion,
  compileArgv,
  compileTarget,
  tarGzEntries,
  zipEntries,
  archiveEntries,
  packageTarget,
  verifyArtifact,
  RELEASE_FLOORS,
  writeChecksums,
  parseChecksums,
  buildAll,
  MAC_TARGETS,
  MAC_ENTITLEMENTS,
  macTargetIdForHost,
  signArgv,
  signTarget,
  notaryCredentials,
  notarizeTarget,
  type TargetId,
  type Target,
  type Floors,
} from '../tools/build-cli';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-build-cli-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

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
  const bufSize = peOffset >= 256 ? 256 : Math.max(256, peOffset + 6);
  const b = new Uint8Array(bufSize);
  b.set([0x4d, 0x5a], 0); // 'MZ'
  new DataView(b.buffer).setUint32(0x3c, peOffset, true);
  if (peOffset < bufSize && peOffset + 6 <= bufSize) {
    b.set([0x50, 0x45, 0x00, 0x00], peOffset); // 'PE\0\0'
    new DataView(b.buffer).setUint16(peOffset + 4, machine, true);
  }
  return b;
}

function machoHeader(cputype: number): Uint8Array {
  const b = new Uint8Array(256);
  const v = new DataView(b.buffer);
  v.setUint32(0, 0xfeedfacf, true);
  v.setUint32(4, cputype, true);
  return b;
}

/** A fat (universal) Mach-O header: FAT_MAGIC big-endian, a count, then one
 *  fat_arch record per slice. Hand-built for the same reason the others are:
 *  a real universal binary is 200 MB and this is twenty bytes of ABI. */
function fatHeader(cputypes: number[], magic = 0xcafebabe): Uint8Array {
  const b = new Uint8Array(256);
  const v = new DataView(b.buffer);
  v.setUint32(0, magic, false); // big endian, always, in a fat header
  v.setUint32(4, cputypes.length, false);
  // FAT_MAGIC carries 20-byte fat_arch records; FAT_MAGIC_64 carries 32-byte
  // fat_arch_64, whose offset and size are 64-bit. Getting this stride wrong
  // would make the fixture agree with a reader that was also wrong.
  const stride = magic === 0xcafebabf ? 32 : 20;
  cputypes.forEach((cpu, i) => {
    const off = 8 + i * stride;
    v.setUint32(off, cpu, false);
  });
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


  test('a universal binary is named as such, not mistaken for one slice', () => {
    // lipo output is what a universal macOS bundle ships, and the verify
    // step compares against an EXPECTED format. Answering 'unknown' would
    // make every universal artifact fail verification; answering
    // 'macho-arm64' would let an Intel-only binary ship inside something
    // labelled universal.
    expect(detectBinaryFormat(fatHeader([0x01000007, 0x0100000c]))).toBe('macho-universal');
    expect(detectBinaryFormat(fatHeader([0x01000007, 0x0100000c], 0xcafebabf)))
      .toBe('macho-universal');
  });

  test('a fat binary missing one of the two arches is NOT universal', () => {
    // The whole reason the old updater cannot be handed per-arch DMGs. A
    // one-slice fat file is a thin binary wearing a universal wrapper, and
    // shipping it would strand exactly the users this is meant to reach.
    expect(detectBinaryFormat(fatHeader([0x0100000c]))).toBe('unknown');
    expect(detectBinaryFormat(fatHeader([0x01000007]))).toBe('unknown');
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
    const dir = mkdtempSync(join(SCRATCH, 'fmt-'));
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
    // A full build leaves a 64-119 MB binary AND a 39 MB or so archive per
    // target -- buildDir is not cleaned up -- so about 450 MB in all. A
    // default would put that somewhere nobody asked for, and the message
    // has to name the real number or it understates what it is warning about.
    expect(() => parseBuildArgs(['--version', '0.6.0'])).toThrow(/--out/);
    expect(() => parseBuildArgs(['--version', '0.6.0'])).toThrow(/450 MB/);
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
    // --sign is a real flag now (see the --mac block), so the unknown one is
    // spelled the way a hurried hand would spell it.
    expect(() => parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x', '--sing', 'x'])).toThrow();
  });

  test('a plain build has no macOS half', () => {
    expect(parseBuildArgs(['--version', '0.6.0', '--out', '/tmp/x']).mac).toBeUndefined();
  });
});

describe('assertPackageVersion', () => {
  function repoWith(version: unknown): string {
    const dir = mkdtempSync(join(SCRATCH, 'pkg-'));
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
    const dir = mkdtempSync(join(SCRATCH, 'compile-'));
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
    const dir = mkdtempSync(join(SCRATCH, 'compile-'));
    try {
      const fake = () => ({ exitCode: 1, stderr: 'error: unknown target\n' });
      expect(() => compileTarget(TARGETS[1]!, dir, fake, '/repo')).toThrow(/linux-arm64/);
      expect(() => compileTarget(TARGETS[1]!, dir, fake, '/repo')).toThrow(/unknown target/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('compileTarget (a real bun build)', () => {
  // Every other compileTarget test fakes the spawn, which proves the
  // plumbing (argv, cwd, error handling) but nothing about bun itself.
  // These two really invoke `bun build --compile` — the only tests in this
  // file that do — because task 4 exists specifically to settle, for real,
  // the two things an earlier review flagged as unverified: that bun
  // appends `.exe` only for the Windows target, and that each compiled
  // output's actual header format matches what the target table predicts.
  // A mocked spawn cannot answer either question; only a real compile can.
  test('a real cross-compiled linux-x64 binary really is ELF x86-64', () => {
    const dir = mkdtempSync(join(SCRATCH, 'realbuild-'));
    try {
      const target = TARGETS.find((t) => t.id === 'linux-x64')!;
      const out = compileTarget(target, dir);
      expect(existsSync(out)).toBe(true);
      expect(readBinaryFormat(out)).toBe(target.format);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 20000);

  test('bun appends .exe only for the windows target, and the result is a real PE binary', () => {
    const dir = mkdtempSync(join(SCRATCH, 'realbuild-'));
    try {
      const target = TARGETS.find((t) => t.id === 'windows-x64')!;
      const out = compileTarget(target, dir);
      // The fact under test: bun wrote screepub.exe, not the bare
      // 'screepub' we asked for via --outfile.
      expect(out.endsWith('.exe')).toBe(true);
      expect(existsSync(compileOutfile(target, dir))).toBe(false);
      expect(existsSync(out)).toBe(true);
      expect(readBinaryFormat(out)).toBe(target.format);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 20000);
});

describe('reading an archive back', () => {
  /** A tar.gz built by the system tar, from files with known contents,
   *  sizes and modes. Two files, one of them longer than a tar block, so
   *  the 512-byte walk is genuinely exercised rather than accidentally
   *  right for a single small entry. */
  function scratchTarGz(): { dir: string; archive: string; big: Uint8Array } {
    const dir = mkdtempSync(join(SCRATCH, 'tar-'));
    const stage = join(dir, 'stage');
    mkdirSync(stage, { recursive: true });
    const big = new Uint8Array(1500);
    for (let i = 0; i < big.length; i++) big[i] = i % 251;
    writeFileSync(join(stage, 'screepub'), big);
    chmodSync(join(stage, 'screepub'), 0o755);
    writeFileSync(join(stage, 'NOTES'), 'plain\n');
    chmodSync(join(stage, 'NOTES'), 0o644);
    const archive = join(dir, 'a.tar.gz');
    // Same COPYFILE_DISABLE as packageTarget, and for the same reason: on
    // macOS bsdtar would add a hidden `._screepub` / `._NOTES` beside each
    // real member and this fixture would stop being the two-file archive the
    // assertions below describe.
    const proc = Bun.spawnSync(['tar', '-czf', archive, '-C', stage, 'screepub', 'NOTES'], {
      env: { ...process.env, COPYFILE_DISABLE: '1' },
    });
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
    const dir = mkdtempSync(join(SCRATCH, 'zip-'));
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

describe('packageTarget', () => {
  /** Stage a small stand-in binary exactly where compileTarget would have
   *  left one, so packaging is tested without a 100 MB compile. */
  function stage(target: Target, bytes: Uint8Array): string {
    const dir = mkdtempSync(join(SCRATCH, 'pack-'));
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
    const dir = mkdtempSync(join(SCRATCH, 'verify-'));
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

describe('SHA256SUMS', () => {
  function withFiles(): { dir: string; names: string[] } {
    const dir = mkdtempSync(join(SCRATCH, 'sums-'));
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

  test('a partial rebuild keeps the entries it did not rebuild', () => {
    // The footgun this closes: `--only windows-x64` into a directory that
    // already holds a full build used to overwrite SHA256SUMS with one
    // line, leaving two published archives silently unchecked by a
    // `sha256sum -c` that passes.
    const { dir, names } = withFiles();
    try {
      writeChecksums(dir, names);
      writeFileSync(join(dir, names[0]!), 'rebuilt windows bytes');
      const text = writeChecksums(dir, [names[0]!]);
      const map = parseChecksums(text);
      expect([...map.keys()].sort()).toEqual([...names].sort());
      // The rebuilt one is re-hashed...
      expect(map.get(names[0]!)).toBe(
        createHash('sha256').update(readFileSync(join(dir, names[0]!))).digest('hex'),
      );
      // ...and so is the carried-over one: the digest comes from the file
      // on disk, never copied out of the old text, so an archive changed
      // behind our back cannot ride through on a stale line.
      expect(map.get(names[1]!)).toBe(
        createHash('sha256').update(readFileSync(join(dir, names[1]!))).digest('hex'),
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('an entry whose file is gone is dropped rather than carried', () => {
    // A line naming a file nobody can download fails `-c` for everyone who
    // did download the rest.
    const { dir, names } = withFiles();
    try {
      writeChecksums(dir, names);
      rmSync(join(dir, names[1]!));
      const map = parseChecksums(writeChecksums(dir, [names[0]!]));
      expect([...map.keys()]).toEqual([names[0]!]);
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

describe('buildAll', () => {
  test('refuses to build a version package.json does not name', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'all-'));
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
  // real assertion rather than an accident of what /tmp happens to hold:
  // nothing but this file writes inside its own fresh SCRATCH.
  const NEVER = join(SCRATCH, 'never');

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

// ---------------------------------------------------------------------------
// The macOS half. Built only with --mac, on a Mac, and signed (or explicitly
// --unsigned) before it is packaged: the cross job never passes --mac, so it
// still cannot produce an unsigned Mach-O by accident. This is the build the
// Homebrew formula serves, moved here so it can outlive the Swift app.
// ---------------------------------------------------------------------------

describe('the macOS target table', () => {
  test('is exactly one row, Apple silicon, in the formula\'s own name', () => {
    // Two rows (arm64 and x64) through the identifier release; one from the
    // release after it, when Intel Macs stopped being supported (owner,
    // 2026-10-01; plan 2026-10-01-f2-handover-amendment.md). Pinned whole,
    // so an Intel row coming back fails here.
    expect(MAC_TARGETS.map((t) => t.id)).toEqual(['macos-arm64']);
    expect(MAC_TARGETS.map((t) => t.bunTarget)).toEqual(['bun-darwin-arm64']);
    expect(MAC_TARGETS.map((t) => t.format)).toEqual(['macho-arm64']);
    expect(MAC_TARGETS.map((t) => t.packaging)).toEqual(['tar.gz']);
    expect(MAC_TARGETS.map((t) => t.binaryName)).toEqual(['screepub']);
    expect(MAC_TARGETS.map((t) => t.archiveName)).toEqual(['screepub-cli-macos-arm64.tar.gz']);
    // The name is not ours to choose: the tap's bump script hardcodes it,
    // and a rename here would leave the formula pointing at nothing.
    const bump = readFileSync('tools/bump-tap.sh', 'utf8');
    for (const t of MAC_TARGETS) expect(bump).toContain(t.archiveName);
  });

  test('an Intel binary is named for what it is, and refused by the Mac row', () => {
    // The detector still knows an x86-64 or fat Mach-O, so a binary that
    // came out Intel (a mistyped --target exits 0) fails verification
    // naming its real format rather than "unknown".
    const ARM = MAC_TARGETS[0]!;
    const intel = new Uint8Array(64);
    new DataView(intel.buffer).setUint32(0, 0xfeedfacf, true);
    new DataView(intel.buffer).setUint32(4, 0x01000007, true);
    expect(detectBinaryFormat(intel)).toBe('macho-x86-64');
    expect(detectBinaryFormat(intel)).not.toBe(ARM.format);
  });

  test('shares no id, directory or archive with the cross targets', () => {
    const cross = new Set(TARGETS.flatMap((t) => [t.id, t.archiveName, buildDir(t, '/o')]));
    for (const t of MAC_TARGETS) {
      expect(cross.has(t.id)).toBe(false);
      expect(cross.has(t.archiveName)).toBe(false);
      expect(cross.has(buildDir(t, '/o'))).toBe(false);
    }
  });

  test('macTargetIdForHost names the Mac this is, and nothing else', () => {
    expect(macTargetIdForHost('darwin', 'arm64')).toBe('macos-arm64');
    // An Intel Mac is not a host of the Mac table any more: nothing it
    // could build ships.
    expect(macTargetIdForHost('darwin', 'x64')).toBeUndefined();
    expect(macTargetIdForHost('linux', 'arm64')).toBeUndefined();
    expect(macTargetIdForHost('win32', 'x64')).toBeUndefined();
    // The cross mapping is untouched: a Mac is still no host of THAT table.
    expect(targetIdForHost('darwin', 'arm64')).toBeUndefined();
  });
});

describe('parseBuildArgs with --mac', () => {
  const BASE = ['--version', '0.6.0', '--out', '/tmp/x'];
  const ID = 'Developer ID Application: Darkwell Entertainment LLC (XSRB3D643J)';

  test('--mac --sign builds the one Mac target, signed, not notarized', () => {
    const args = parseBuildArgs([...BASE, '--mac', '--sign', ID]);
    expect(args.only).toEqual(['macos-arm64']);
    expect(args.mac).toEqual({ sign: ID, notarize: false });
  });

  test('--notarize rides on --sign', () => {
    expect(parseBuildArgs([...BASE, '--mac', '--sign', ID, '--notarize']).mac)
      .toEqual({ sign: ID, notarize: true });
  });

  test('--mac --unsigned is allowed, and says so in the args', () => {
    expect(parseBuildArgs([...BASE, '--mac', '--unsigned']).mac).toEqual({ sign: null, notarize: false });
  });

  test('--mac must say how it is signed: neither, or both, is refused', () => {
    // An unsigned Mach-O is a Gatekeeper warning for every person who
    // downloads it, so "unsigned" has to be asked for by name.
    expect(() => parseBuildArgs([...BASE, '--mac'])).toThrow(/--sign/);
    expect(() => parseBuildArgs([...BASE, '--mac'])).toThrow(/--unsigned/);
    expect(() => parseBuildArgs([...BASE, '--mac', '--sign', ID, '--unsigned'])).toThrow(/both/);
  });

  test('an unsigned binary cannot be notarized, so the pair is refused', () => {
    expect(() => parseBuildArgs([...BASE, '--mac', '--unsigned', '--notarize'])).toThrow(/notariz/);
  });

  test('the signing flags mean nothing without --mac, and are refused', () => {
    expect(() => parseBuildArgs([...BASE, '--sign', ID])).toThrow(/--mac/);
    expect(() => parseBuildArgs([...BASE, '--unsigned'])).toThrow(/--mac/);
    expect(() => parseBuildArgs([...BASE, '--notarize'])).toThrow(/--mac/);
  });

  test('--only stays inside its own table, in both directions', () => {
    expect(parseBuildArgs([...BASE, '--mac', '--unsigned', '--only', 'macos-arm64']).only)
      .toEqual(['macos-arm64']);
    // A Mac id without --mac is the cross job being handed a Mac target:
    // refused, and the message says what would make it legal.
    expect(() => parseBuildArgs([...BASE, '--only', 'macos-arm64'])).toThrow(/--mac/);
    expect(() => parseBuildArgs([...BASE, '--mac', '--unsigned', '--only', 'linux-x64']))
      .toThrow(/known targets are macos-arm64$/);
  });

  test('the Intel Mac target is gone, with or without --mac', () => {
    expect(() => parseBuildArgs([...BASE, '--mac', '--unsigned', '--only', 'macos-x64']))
      .toThrow(/unknown target "macos-x64"/);
    expect(() => parseBuildArgs([...BASE, '--only', 'macos-x64'])).toThrow(/macos-x64/);
  });
});

describe('the entitlements the CLI is signed with', () => {
  test('are exactly the three the shipped CLI has always carried', () => {
    // bun's runtime JITs; under the hardened runtime that needs these. The
    // move out of the Swift app's folder changes where the CLI is built,
    // not how it is signed, so the set is pinned whole.
    const text = readFileSync(MAC_ENTITLEMENTS, 'utf8');
    const keys = [...text.matchAll(/<key>([^<]+)<\/key>\s*<true\/>/g)].map((m) => m[1]);
    expect(keys.sort()).toEqual([
      'com.apple.security.cs.allow-jit',
      'com.apple.security.cs.allow-unsigned-executable-memory',
      'com.apple.security.cs.disable-library-validation',
    ]);
    expect(text).not.toContain('<false/>');
  });
});

describe('signing a Mac binary', () => {
  const ARM = MAC_TARGETS.find((t) => t.id === 'macos-arm64')!;
  const ID = 'Developer ID Application: Darkwell Entertainment LLC (XSRB3D643J)';
  const GOOD_DESCRIBE =
    'Executable=/tmp/out/macos-arm64/screepub\nIdentifier=screepub\n' +
    'CodeDirectory v=20500 size=1 flags=0x10000(runtime) hashes=1+2 location=embedded\n' +
    'TeamIdentifier=XSRB3D643J\n';

  function recorder(answers: Record<string, { exitCode: number; stderr: string }>) {
    const seen: string[][] = [];
    const spawn = (argv: string[]) => {
      seen.push(argv);
      const key = argv.slice(0, 2).join(' ');
      return answers[key] ?? { exitCode: 0, stderr: key === 'codesign -dv' ? GOOD_DESCRIBE : '' };
    };
    return { seen, spawn };
  }

  test('signArgv is the exact codesign invocation the shipped CLI was signed with', () => {
    // --identifier because the Swift app's release script signed a file
    // named screepub-cli-<arch>, and codesign takes the identifier from the
    // file name. This one signs a file named `screepub`, so without it the
    // binary's designated requirement would change (review, 2026-10-01).
    expect(signArgv(ARM, '/tmp/out', ID)).toEqual([
      'codesign', '--force', '--options', 'runtime', '--timestamp',
      '--entitlements', MAC_ENTITLEMENTS,
      '--identifier', 'screepub-cli-arm64',
      '--sign', ID,
      '/tmp/out/macos-arm64/screepub',
    ]);
  });

  test('signs, then verifies, then reads back the flags, in that order', () => {
    const { seen, spawn } = recorder({});
    signTarget(ARM, '/tmp/out', ID, spawn);
    expect(seen.length).toBe(3);
    expect(seen[0]).toEqual(signArgv(ARM, '/tmp/out', ID));
    expect(seen[1]).toEqual(['codesign', '--verify', '--strict', '--verbose=2', '/tmp/out/macos-arm64/screepub']);
    expect(seen[2]).toEqual(['codesign', '-dv', '/tmp/out/macos-arm64/screepub']);
  });

  test('a failed sign throws, naming the target and codesign\'s words', () => {
    const { spawn } = recorder({ 'codesign --force': { exitCode: 1, stderr: 'no identity found\n' } });
    expect(() => signTarget(ARM, '/tmp/out', ID, spawn)).toThrow(/macos-arm64/);
    expect(() => signTarget(ARM, '/tmp/out', ID, spawn)).toThrow(/no identity found/);
  });

  test('a signature that does not verify throws', () => {
    const { spawn } = recorder({ 'codesign --verify': { exitCode: 3, stderr: 'invalid signature\n' } });
    expect(() => signTarget(ARM, '/tmp/out', ID, spawn)).toThrow(/invalid signature/);
  });

  test('a signature WITHOUT the hardened runtime is refused: notarization would reject it', () => {
    const { spawn } = recorder({
      'codesign -dv': { exitCode: 0, stderr: GOOD_DESCRIBE.replace('0x10000(runtime)', '0x0(none)') },
    });
    expect(() => signTarget(ARM, '/tmp/out', ID, spawn)).toThrow(/hardened runtime/);
  });

  test('a real identity must leave a team on the binary; ad-hoc may not', () => {
    // codesign answers "TeamIdentifier=not set" for an ad-hoc signature. A
    // release asked for a Developer ID and got that: refuse. A local `-`
    // asked for ad-hoc and got it: fine.
    const adhoc = GOOD_DESCRIBE.replace('TeamIdentifier=XSRB3D643J', 'TeamIdentifier=not set');
    const { spawn } = recorder({ 'codesign -dv': { exitCode: 0, stderr: adhoc } });
    expect(() => signTarget(ARM, '/tmp/out', ID, spawn)).toThrow(/team/i);
    expect(() => signTarget(ARM, '/tmp/out', '-', spawn)).not.toThrow();
  });

  test('refuses to sign anything that is not a Mac target', () => {
    const { seen, spawn } = recorder({});
    expect(() => signTarget(TARGETS[0]!, '/tmp/out', ID, spawn)).toThrow(/linux-x64/);
    expect(seen.length).toBe(0);
  });
});

describe('notarizing a Mac binary', () => {
  const ARM = MAC_TARGETS.find((t) => t.id === 'macos-arm64')!;
  const CREDS = { keyPath: '/k/ac.p8', keyId: 'KEYID', issuer: 'ISSUER' };

  test('notaryCredentials reads the three variables the release job sets', () => {
    expect(notaryCredentials({
      AC_API_KEY_PATH: '/k/ac.p8', AC_API_KEY_ID: 'KEYID', AC_API_ISSUER_ID: 'ISSUER',
    })).toEqual(CREDS);
  });

  test('and names exactly the ones that are missing', () => {
    let message = '';
    try {
      notaryCredentials({ AC_API_KEY_ID: 'KEYID' });
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toContain('AC_API_KEY_PATH');
    expect(message).toContain('AC_API_ISSUER_ID');
    expect(message).not.toContain('AC_API_KEY_ID');
    expect(() => notaryCredentials({ AC_API_KEY_PATH: '', AC_API_KEY_ID: 'a', AC_API_ISSUER_ID: 'b' }))
      .toThrow(/AC_API_KEY_PATH/);
  });

  test('zips the binary, submits the zip and waits, then removes the zip', () => {
    const dir = mkdtempSync(join(SCRATCH, 'notary-'));
    try {
      const seen: string[][] = [];
      const spawn = (argv: string[]) => {
        seen.push(argv);
        // Stand in for ditto: make the zip it would have made.
        if (argv[0] === 'ditto') writeFileSync(argv[argv.length - 1]!, 'zip');
        return { exitCode: 0, stderr: '', stdout: argv[0] === 'xcrun' ? '{"id":"2efe2717-52ef-43a5-96dc-0797e4ca1041","message":"Processing complete","status":"Accepted"}' : '' };
      };
      mkdirSync(buildDir(ARM, dir), { recursive: true });
      notarizeTarget(ARM, dir, CREDS, spawn);
      const zip = join(dir, 'macos-arm64', 'screepub-notarize.zip');
      // --output-format json, the way tauri-macos-sign 2.3.4 asks it (the
      // code that notarizes the window every release): the verdict is read
      // from the answer, not inferred from the exit code.
      expect(seen).toEqual([
        ['ditto', '-c', '-k', binaryPath(ARM, dir), zip],
        ['xcrun', 'notarytool', 'submit', zip, '--key', '/k/ac.p8', '--key-id', 'KEYID',
          '--issuer', 'ISSUER', '--output-format', 'json', '--wait'],
      ]);
      expect(existsSync(zip)).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('an exit of 0 with a status other than Accepted is a failure, with the id to look up', () => {
    const dir = mkdtempSync(join(SCRATCH, 'notary-'));
    try {
      const answer = (stdout: string) => (argv: string[]) => {
        if (argv[0] === 'ditto') {
          writeFileSync(argv[argv.length - 1]!, 'zip');
          return { exitCode: 0, stderr: '', stdout: '' };
        }
        return { exitCode: 0, stderr: '', stdout };
      };
      mkdirSync(buildDir(ARM, dir), { recursive: true });
      const invalid = answer('{"id":"abc-123","message":"Processing complete","status":"Invalid"}');
      expect(() => notarizeTarget(ARM, dir, CREDS, invalid)).toThrow(/Invalid/);
      expect(() => notarizeTarget(ARM, dir, CREDS, invalid)).toThrow(/abc-123/);
      expect(() => notarizeTarget(ARM, dir, CREDS, invalid)).toThrow(/notarytool log/);
      // An answer that is not the JSON asked for is not a pass either.
      expect(() => notarizeTarget(ARM, dir, CREDS, answer('Processing complete'))).toThrow(/macos-arm64/);
      expect(() => notarizeTarget(ARM, dir, CREDS, answer(''))).toThrow(/macos-arm64/);
      expect(existsSync(join(dir, 'macos-arm64', 'screepub-notarize.zip'))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a rejected submission throws, names the target, and still removes the zip', () => {
    const dir = mkdtempSync(join(SCRATCH, 'notary-'));
    try {
      const spawn = (argv: string[]) => {
        if (argv[0] === 'ditto') {
          writeFileSync(argv[argv.length - 1]!, 'zip');
          return { exitCode: 0, stderr: '' };
        }
        return { exitCode: 1, stderr: 'status: Invalid\n' };
      };
      mkdirSync(buildDir(ARM, dir), { recursive: true });
      expect(() => notarizeTarget(ARM, dir, CREDS, spawn)).toThrow(/macos-arm64/);
      expect(() => notarizeTarget(ARM, dir, CREDS, spawn)).toThrow(/Invalid/);
      expect(existsSync(join(dir, 'macos-arm64', 'screepub-notarize.zip'))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('buildAll with --mac', () => {
  test('a notarizing build checks its credentials before compiling anything', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'all-mac-'));
    const saved = process.env.AC_API_KEY_PATH;
    delete process.env.AC_API_KEY_PATH;
    try {
      const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as { version: string };
      await expect(buildAll({
        version: pkg.version, outDir: dir, only: ['macos-arm64'],
        mac: { sign: 'Developer ID Application: X (T)', notarize: true },
      })).rejects.toThrow(/AC_API_KEY_PATH/);
      // Refused before the first 60 MB compile, not after it.
      expect(existsSync(buildDir(MAC_TARGETS[0]!, dir))).toBe(false);
    } finally {
      if (saved !== undefined) process.env.AC_API_KEY_PATH = saved;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a Mac id handed to buildAll without --mac is refused, not silently skipped', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'all-mac-'));
    try {
      const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as { version: string };
      await expect(buildAll({ version: pkg.version, outDir: dir, only: ['macos-arm64'] }))
        .rejects.toThrow(/--mac/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
