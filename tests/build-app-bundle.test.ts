import { describe, test, expect, afterAll } from 'bun:test';
import {
  closeSync,
  ftruncateSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
  writeSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  BUNDLE_KINDS,
  MAC_ARCH,
  MAC_TARGET,
  SIGNATURE_EXT,
  UPDATER_KINDS,
  UPDATER_OVERLAY,
  assertBundleVersions,
  buildArgv,
  buildBundles,
  bundleDirFor,
  bundleListFor,
  cargoPackageVersion,
  discoverArtifact,
  kindsForOs,
  parseBundleArgs,
  updaterKindsForOs,
  verifyBundleFile,
  type BundleArch,
  type BundleKind,
  type UpdaterKind,
} from '../tools/build-app-bundle';
import { parseChecksums } from '../tools/build-cli';
import { fakeSignatureBox } from './signature-box';

const OUT = mkdtempSync(join(tmpdir(), 'screepub-bundle-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const kind = (id: string): BundleKind => BUNDLE_KINDS.find((k) => k.id === id)!;

/** A file of exactly `size` bytes that is all zeroes, made as a hole rather
 *  than as ten megabytes of allocation: the floor is 10 MB and the suite
 *  would otherwise write a quarter of a gigabyte to say so. */
function hollow(path: string, size: number): number {
  const fd = openSync(path, 'w');
  ftruncateSync(fd, size);
  return fd;
}

/** A file that passes `kind`'s magic check and clears its floor, without
 *  writing forty megabytes. */
function plausible(path: string, k: BundleKind): void {
  const size = k.floorBytes + 1024;
  const fd = hollow(path, size);
  try {
    const magic = new Uint8Array(k.magic);
    // The koly block is the LAST 512 bytes, so the trailer kinds get their
    // magic at size - 512 and nothing at the head at all.
    writeSync(fd, magic, 0, magic.length, k.magicAt === 'head' ? 0 : size - 512);
  } finally {
    closeSync(fd);
  }
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

  test('every row looks in the bundler subdirectory its own id names', () => {
    // A row whose `dir` pointed at another kind's directory would discover
    // the other kind's artifact and publish it under this name.
    for (const k of BUNDLE_KINDS) {
      expect(k.dir).toBe(k.id);
      expect(bundleDirFor(k)).toMatch(new RegExp(`bundle[/\\\\]${k.id}$`));
    }
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
      // Every kind that OS builds must actually be asked for.
      for (const k of kindsForOs(os)) expect(bundleListFor(os).split(',')).toContain(k.id);
    }
  });

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
    // The one Mac image, under the Swift app's old name ON PURPOSE (owner,
    // 2026-10-01; plan 2026-10-01-f2-handover-amendment.md): every site
    // button, README link and outside link already points at
    // releases/latest/download/Screepub-macOS.dmg. Universal through the
    // identifier release, Apple silicon only after it, and never with the
    // arch in the name: one Mac arch is built, as one Windows arch is, and
    // the parser refuses the other (see "a Mac bundle is Apple silicon
    // only" below), so no x64 image can be named at all.
    expect(kind('dmg').releasedName('0.6.0', 'arm64')).toBe('Screepub-macOS.dmg');
    expect(MAC_ARCH).toBe('arm64');
  });

  test('no published name changes when the version does', () => {
    // The property the renaming exists for, stated directly rather than
    // implied by five literals: a prerelease and a far-future release get
    // exactly the names 0.6.0 gets, and no name carries a version at all.
    for (const k of BUNDLE_KINDS) {
      for (const arch of ['x64', 'arm64'] as const) {
        const name = k.releasedName('0.6.0', arch);
        expect(k.releasedName('9.12.3-rc1', arch)).toBe(name);
        expect(name).not.toMatch(/\d+\.\d+\.\d+/);
      }
    }
  });

  test('every published name ends in the extension its own row declares', () => {
    // A name that lost its extension would still upload, and would download
    // as something the OS refuses to open.
    for (const k of BUNDLE_KINDS) {
      for (const arch of ['x64', 'arm64'] as const) {
        expect(k.releasedName('0.6.0', arch).endsWith(k.ext)).toBe(true);
      }
    }
  });

  test('the arch-bearing names change when the arch changes', () => {
    // The defect this forbids is a releasedName that ignores its arch
    // argument: an arm64 .deb published under the amd64 name installs and
    // then refuses to run.
    for (const id of ['deb', 'rpm'] as const) {
      const k = kind(id);
      expect(k.releasedName('0.6.0', 'x64')).not.toBe(k.releasedName('0.6.0', 'arm64'));
    }
    // The two that ship ONE arch carry none in the name. That is safe only
    // because each of those OSes builds one arch: the Windows leg is x86-64,
    // and a Mac bundle for anything but Apple silicon is refused.
    for (const id of ['dmg', 'nsis'] as const) {
      const k = kind(id);
      expect(k.releasedName('0.6.0', 'x64')).toBe(k.releasedName('0.6.0', 'arm64'));
    }
  });

  test('no two rows can ever produce the same published filename', () => {
    const names = BUNDLE_KINDS.flatMap((k) =>
      (['x64', 'arm64'] as const).map((a) => k.releasedName('0.6.0', a)),
    );
    // nsis and dmg ignore the arch (one arch ships for each), so each
    // contributes one name twice; everything else must be distinct.
    expect(new Set(names).size).toBe(names.length - 2);
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
    expect(
      buildArgv('macos', { target: 'aarch64-apple-darwin', config: 'tauri.transition.conf.json' }),
    ).toEqual([
      'cargo',
      'tauri',
      'build',
      '--bundles',
      'app,dmg',
      '--target',
      'aarch64-apple-darwin',
      '--config',
      'tauri.transition.conf.json',
    ]);
  });

  test('neither flag appears when it was not asked for', () => {
    expect(buildArgv('windows', {})).not.toContain('--target');
    expect(buildArgv('windows', {})).not.toContain('--config');
  });

  test('the bundle directory moves under the triple when one is given', () => {
    // `cargo tauri build --target X` writes to target/X/release/bundle/...,
    // not target/release/bundle/... Looking in the wrong one is how a build
    // "succeeds" and produces nothing.
    expect(bundleDirFor(kind('deb'))).toMatch(/target[/\\]release[/\\]bundle[/\\]deb$/);
    expect(bundleDirFor(kind('dmg'), 'aarch64-apple-darwin')).toMatch(
      /target[/\\]aarch64-apple-darwin[/\\]release[/\\]bundle[/\\]dmg$/,
    );
    expect(bundleDirFor(kind('dmg'), 'aarch64-apple-darwin')).not.toBe(bundleDirFor(kind('dmg')));
  });
});

describe('finding the file the bundler wrote', () => {
  test('it returns the one artifact with the right extension', () => {
    const dir = join(OUT, 'find-one');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'Screepub_0.6.0_aarch64.dmg'), 'x');
    // Other formats beside it are not candidates.
    writeFileSync(join(dir, 'Screepub_0.6.0_aarch64.dmg.sig'), 'x');
    expect(discoverArtifact(dir, kind('dmg'))).toBe(
      join(dir, 'Screepub_0.6.0_aarch64.dmg'),
    );
  });

  test('it ignores the temporary image bundle_dmg leaves behind on a crash', () => {
    // bundle_dmg writes rw.$$.<name>.dmg beside the real one and removes it
    // on success. After a crashed run both are there; without this filter
    // the count check below fires on a directory that has exactly one real
    // artifact, and the build fails for the wrong reason.
    const dir = join(OUT, 'find-rw');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'rw.4711.Screepub_0.6.0_x64.dmg'), 'x');
    writeFileSync(join(dir, 'Screepub_0.6.0_x64.dmg'), 'x');
    expect(discoverArtifact(dir, kind('dmg'))).toBe(join(dir, 'Screepub_0.6.0_x64.dmg'));
  });

  test('a directory holding only the leftover temporary image is still a failure', () => {
    // The crash case where the real image was never written: excluding rw.
    // must not turn "nothing was built" into a silent success.
    const dir = join(OUT, 'find-only-rw');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'rw.4711.Screepub_0.6.0_x64.dmg'), 'x');
    expect(() => discoverArtifact(dir, kind('dmg'))).toThrow(/find-only-rw/);
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
      const size = 2048;
      const fd = hollow(path, size);
      const magic = new Uint8Array(k.magic);
      writeSync(fd, magic, 0, magic.length, k.magicAt === 'head' ? 0 : size - 512);
      closeSync(fd);
      expect(() => verifyBundleFile(path, k)).toThrow(/floor/);
    }
  });

  test('an empty file is rejected for every kind', () => {
    // Zero bytes must not read back as "no magic to disagree with".
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `empty${k.ext}`);
      writeFileSync(path, new Uint8Array(0));
      expect(() => verifyBundleFile(path, k)).toThrow(/floor/);
    }
  });

  test('the wrong container is rejected for every kind', () => {
    // The realistic failure is a renamed file: the build produced an .rpm
    // and something copied it to the .deb's published name.
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `wrong${k.ext}`);
      closeSync(hollow(path, k.floorBytes + 1024)); // all zeroes
      expect(() => verifyBundleFile(path, k)).toThrow(/magic|container/i);
    }
  });

  test('one kind rejects another kind of real container', () => {
    // Cross-check the magics against each other rather than only against
    // zeroes: a row that checked a two-byte prefix shared with its
    // neighbour would pass the all-zeroes test and still be wrong.
    for (const k of BUNDLE_KINDS) {
      const path = join(OUT, `cross${k.ext}`);
      for (const other of BUNDLE_KINDS) {
        if (other.id === k.id) continue;
        plausible(path, other);
        expect(() => verifyBundleFile(path, k)).toThrow(/magic|container/i);
      }
    }
  });

  test('the DMG check reads the trailer and not the head', () => {
    // A UDIF image has no head magic at all -- its 512-byte koly trailer is
    // at the END. A head-only check would accept any large file as a DMG,
    // which is exactly the vacuous assertion this test exists to forbid.
    const k = kind('dmg');
    expect(k.magicAt).toBe('udif-trailer');
    const path = join(OUT, 'headmagic.dmg');
    const size = k.floorBytes + 1024;
    const fd = hollow(path, size);
    writeSync(fd, new Uint8Array(k.magic), 0, k.magic.length, 0); // koly at the FRONT
    closeSync(fd);
    expect(() => verifyBundleFile(path, k)).toThrow(/magic|container/i);
  });

  test('the koly block must start the trailer, not merely appear in it', () => {
    // The trailer is a fixed 512-byte structure whose first four bytes are
    // the signature; a scan of the last 512 bytes would accept a file that
    // happens to contain "koly" anywhere near the end.
    const k = kind('dmg');
    const path = join(OUT, 'lateloly.dmg');
    const size = k.floorBytes + 1024;
    const fd = hollow(path, size);
    writeSync(fd, new Uint8Array(k.magic), 0, k.magic.length, size - 64);
    closeSync(fd);
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

  test('a version key that is absent fails CLOSED, for each of the three files', () => {
    // The gate must never read "I could not find a version" as agreement.
    const dir = mkdtempSync(join(OUT, 'repo-missing-'));
    mkdirSync(join(dir, 'desktop', 'src-tauri'), { recursive: true });
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'screepub' }));
    writeFileSync(
      join(dir, 'desktop', 'src-tauri', 'Cargo.toml'),
      '[package]\nname = "screepub-desktop"\n',
    );
    writeFileSync(
      join(dir, 'desktop', 'src-tauri', 'tauri.conf.json'),
      JSON.stringify({ productName: 'Screepub' }),
    );
    expect(() => assertBundleVersions('0.6.0', dir)).toThrow(/package\.json/);
  });

  test("a dependency's version is never mistaken for the crate's", () => {
    // [package] with no version of its own, followed by a dependency that
    // has one: a lazy scan from [package] finds "2" and, for a build of
    // version 2, would pass. This is the gate failing open.
    const text =
      '[package]\nname = "screepub-desktop"\n\n[dependencies.tauri]\nversion = "2"\nfeatures = []\n';
    expect(cargoPackageVersion(text)).toBeUndefined();
    // Inline form too, which is the spelling the real manifest uses.
    expect(
      cargoPackageVersion('[package]\nname = "d"\n\n[dependencies]\ntauri = { version = "2" }\n'),
    ).toBeUndefined();
    expect(cargoPackageVersion('[package]\nversion = "0.6.0"\n\n[dependencies]\ntauri = "2"\n')).toBe(
      '0.6.0',
    );
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
      // Named, never left to the host: see "a Mac bundle is Apple silicon only".
      target: 'aarch64-apple-darwin',
    });
    // Linux and Windows build for the runner they are on, untargeted.
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'linux', 'x64').target).toBeUndefined();
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'win32', 'x64').target).toBeUndefined();
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

  test('it refuses an architecture it has no bundles for', () => {
    expect(() => parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'linux', 'ia32')).toThrow(
      /ia32/,
    );
  });

  test('--out is made absolute so the tool can be run from anywhere', () => {
    const args = parseBundleArgs(['--version', '0.6.0', '--out', 'dist'], 'linux', 'x64');
    expect(args.outDir.startsWith('/') || /^[A-Za-z]:/.test(args.outDir)).toBe(true);
    expect(args.outDir.endsWith('dist')).toBe(true);
  });

  test('--target and --config reach the args they are named for', () => {
    expect(
      parseBundleArgs(
        ['--version', '0.6.0', '--out', OUT, '--target', 'aarch64-apple-darwin', '--config', 'o.json'],
        'darwin',
        'arm64',
      ),
    ).toMatchObject({ target: 'aarch64-apple-darwin', config: 'o.json' });
  });
});

describe('a whole run, against a fake cargo', () => {
  /** A repository whose three version files agree, so the release gate can
   *  be satisfied. It never can be in the real tree on a working branch --
   *  package.json trails the crate and the config by design -- which is
   *  precisely why this half of the tool needs an injected repo to be
   *  exercised at all. */
  const agreeingRepo = (version: string): { repoDir: string; desktopDir: string } => {
    const repoDir = mkdtempSync(join(OUT, 'run-repo-'));
    const desktopDir = join(repoDir, 'desktop', 'src-tauri');
    mkdirSync(desktopDir, { recursive: true });
    writeFileSync(join(repoDir, 'package.json'), JSON.stringify({ version }));
    writeFileSync(
      join(desktopDir, 'Cargo.toml'),
      `[package]\nname = "screepub-desktop"\nversion = "${version}"\n`,
    );
    writeFileSync(
      join(desktopDir, 'tauri.conf.json'),
      JSON.stringify({ productName: 'Screepub', version }),
    );
    return { repoDir, desktopDir };
  };

  /** Stands in for `cargo tauri build`: records the argv, then writes the
   *  artifacts a real bundler would have written, under the bundler's own
   *  naming rather than the published one. */
  const fakeCargo = (dirs: BundleKind[], desktopDir: string) => {
    const calls: string[][] = [];
    const cwds: string[] = [];
    const spawn = (argv: string[], cwd: string) => {
      calls.push(argv);
      cwds.push(cwd);
      for (const k of dirs) {
        const dir = bundleDirFor(
          k,
          argv.includes('--target') ? argv[argv.indexOf('--target') + 1] : undefined,
          desktopDir,
        );
        mkdirSync(dir, { recursive: true });
        plausible(join(dir, `Screepub_0.6.0_bundler-name${k.ext}`), k);
      }
      return { exitCode: 0, stderr: '' };
    };
    return { calls, cwds, spawn };
  };

  test('it renames to the published names and writes SHA256SUMS-app', async () => {
    const repo = agreeingRepo('0.6.0');
    const { calls, cwds, spawn } = fakeCargo(kindsForOs('linux'), repo.desktopDir);
    const out = join(OUT, 'run-linux');
    const made = await buildBundles(
      { version: '0.6.0', outDir: out, os: 'linux', arch: 'arm64' },
      spawn,
      repo,
    );
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual([
      'Screepub-linux-arm64.deb',
      'Screepub-linux-aarch64.rpm',
    ]);
    // The returned paths are the files that are actually on disk.
    for (const p of made) expect(() => readFileSync(p)).not.toThrow();
    // Exactly one cargo invocation: deb and rpm come out of a single build.
    expect(calls.length).toBe(1);
    expect(calls[0]).toEqual(['cargo', 'tauri', 'build', '--bundles', 'deb,rpm']);
    // Run from the crate, not from wherever the tool was invoked.
    expect(cwds[0]).toBe(repo.desktopDir);

    const sums = parseChecksums(readFileSync(join(out, 'SHA256SUMS-app'), 'utf8'));
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-linux-aarch64.rpm',
      'Screepub-linux-arm64.deb',
    ]);
    // The digests describe the files that are actually there, not the ones
    // the bundler wrote before the rename.
    for (const [name, digest] of sums) {
      expect(digest).toBe(
        new Bun.CryptoHasher('sha256').update(readFileSync(join(out, name))).digest('hex'),
      );
    }
  });

  test('the version gate stops a run before cargo is ever spawned', async () => {
    // The real tree is exactly this shape on every working branch, so a
    // gate that ran after the build would waste ten minutes to say so.
    const repo = agreeingRepo('0.5.4');
    const { calls, spawn } = fakeCargo(kindsForOs('linux'), repo.desktopDir);
    await expect(
      buildBundles({ version: '0.6.0', outDir: join(OUT, 'run-gated'), os: 'linux', arch: 'x64' }, spawn, repo),
    ).rejects.toThrow(/0\.5\.4/);
    expect(calls.length).toBe(0);
  });

  test('the checksums file is NOT called SHA256SUMS, and names the x64 build', async () => {
    // E1's cross-upload job publishes SHA256SUMS for the three CLI
    // archives. One file overwriting the other on the release page is how a
    // download silently stops being checkable.
    const repo = agreeingRepo('0.6.0');
    const { spawn } = fakeCargo(kindsForOs('linux'), repo.desktopDir);
    const out = join(OUT, 'run-names');
    await buildBundles({ version: '0.6.0', outDir: out, os: 'linux', arch: 'x64' }, spawn, repo);
    expect(() => readFileSync(join(out, 'SHA256SUMS-app'))).not.toThrow();
    expect(() => readFileSync(join(out, 'SHA256SUMS'))).toThrow();
    // The arch actually reaches the names on a second architecture.
    const sums = parseChecksums(readFileSync(join(out, 'SHA256SUMS-app'), 'utf8'));
    expect([...sums.keys()].sort()).toEqual([
      'Screepub-linux-amd64.deb',
      'Screepub-linux-x86_64.rpm',
    ]);
  });

  test('a failing cargo fails the run and forwards its stderr', async () => {
    const repo = agreeingRepo('0.6.0');
    const spawn = () => ({ exitCode: 101, stderr: 'error: linker `cc` not found' });
    await expect(
      buildBundles(
        { version: '0.6.0', outDir: join(OUT, 'run-fail'), os: 'linux', arch: 'x64' },
        spawn,
        repo,
      ),
    ).rejects.toThrow(/linker/);
  });

  test('a cargo that exits 0 and writes nothing still fails', async () => {
    // The failure this whole tool exists for: a green exit code and an
    // empty bundle directory.
    const repo = agreeingRepo('0.6.0');
    const spawn = () => ({ exitCode: 0, stderr: '' });
    await expect(
      buildBundles(
        { version: '0.6.0', outDir: join(OUT, 'run-empty'), os: 'linux', arch: 'x64' },
        spawn,
        repo,
      ),
    ).rejects.toThrow(/deb/);
  });

  test('a cargo that writes only the deb still fails, and no rpm is published', async () => {
    // Half a Linux build is the realistic partial failure, and the .deb
    // must not go out on its own beside a checksums file that never
    // mentions the missing half.
    const repo = agreeingRepo('0.6.0');
    const { spawn } = fakeCargo([kind('deb')], repo.desktopDir);
    const out = join(OUT, 'run-half');
    await expect(
      buildBundles({ version: '0.6.0', outDir: out, os: 'linux', arch: 'x64' }, spawn, repo),
    ).rejects.toThrow(/rpm/);
    expect(() => readFileSync(join(out, 'SHA256SUMS-app'))).toThrow();
  });

  test('a truncated artifact fails the run instead of being published', async () => {
    const repo = agreeingRepo('0.6.0');
    const spawn = () => {
      const dir = bundleDirFor(kind('deb'), undefined, repo.desktopDir);
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'Screepub_0.6.0_truncated.deb'), new Uint8Array(64));
      return { exitCode: 0, stderr: '' };
    };
    const out = join(OUT, 'run-trunc');
    await expect(
      buildBundles({ version: '0.6.0', outDir: out, os: 'linux', arch: 'x64' }, spawn, repo),
    ).rejects.toThrow(/floor/);
    expect(() => readFileSync(join(out, 'Screepub-linux-amd64.deb'))).toThrow();
  });

  // ---- the updater archive ---------------------------------------------
  // Piece A's transport half. With --updater the macOS run also carries
  // out the `.app.tar.gz` the bundler makes when createUpdaterArtifacts is
  // on, and the `.sig` beside it. Both are what latest.json points at.

  const appTar = (): UpdaterKind => UPDATER_KINDS.find((k) => k.id === 'app-tar')!;

  /** A fake cargo for macOS that writes the DMG AND, when asked, the
   *  updater archive and its signature under the bundler's own names,
   *  which follow productName ("Screepub"). */
  const fakeMacCargo = (
    desktopDir: string,
    write: { tar?: boolean; sig?: boolean | string } = { tar: true, sig: true },
  ) => {
    const calls: string[][] = [];
    const spawn = (argv: string[], _cwd: string) => {
      calls.push(argv);
      const target = argv.includes('--target') ? argv[argv.indexOf('--target') + 1] : undefined;
      for (const k of kindsForOs('macos')) {
        const dir = bundleDirFor(k, target, desktopDir);
        mkdirSync(dir, { recursive: true });
        plausible(join(dir, `Screepub${k.ext}`), k);
      }
      const macos = bundleDirFor(appTar(), target, desktopDir);
      mkdirSync(macos, { recursive: true });
      const tar = join(macos, 'Screepub.app.tar.gz');
      if (write.tar) plausible(tar, appTar() as unknown as BundleKind);
      if (write.sig === true) {
        writeFileSync(`${tar}${SIGNATURE_EXT}`, fakeSignatureBox('Screepub.app.tar.gz'));
      } else if (typeof write.sig === 'string') {
        writeFileSync(`${tar}${SIGNATURE_EXT}`, write.sig);
      }
      return { exitCode: 0, stderr: '' };
    };
    return { calls, spawn };
  };
  const macRun = (out: string, updater: boolean) => ({
    version: '0.6.0',
    outDir: out,
    os: 'macos' as const,
    arch: 'arm64' as const,
    target: 'aarch64-apple-darwin',
    config: 'tauri.transition.conf.json',
    updater,
  });
  // Both variables SET. The password may be empty, for a key made without
  // one, but it must be present: tauri-cli reads its absence as "prompt
  // me", and a build spawned by this tool has no terminal to prompt on.
  // Measured 2026-09-21: "incorrect updater private key password: Device
  // not configured (os error 6)", after the whole build.
  const withKey = { TAURI_SIGNING_PRIVATE_KEY: 'not-a-real-key-but-set', TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' };

  test('--updater passes the updater overlay AFTER the transition overlay', () => {
    // Order is the whole point of it being a second file: the transition
    // overlay renames the product and this one turns the archive on, and
    // the CLI merges them in the order given. Both reach cargo.
    expect(buildArgv('macos', { target: 'aarch64-apple-darwin', config: 'tauri.transition.conf.json', updater: true })).toEqual([
      'cargo', 'tauri', 'build', '--bundles', 'app,dmg',
      '--target', 'aarch64-apple-darwin',
      '--config', 'tauri.transition.conf.json',
      '--config', UPDATER_OVERLAY,
    ]);
    expect(UPDATER_OVERLAY).toBe('tauri.updater.conf.json');
  });

  test('without --updater the overlay is never passed, so a push build needs no key', () => {
    for (const os of ['linux', 'macos', 'windows'] as const) {
      expect(buildArgv(os, {})).not.toContain(UPDATER_OVERLAY);
      expect(buildArgv(os, { updater: false })).not.toContain(UPDATER_OVERLAY);
    }
  });

  test('the parser reads --updater, and it is off by default', () => {
    expect(parseBundleArgs(['--version', '0.6.0', '--out', OUT], 'darwin', 'arm64').updater).toBe(false);
    expect(
      parseBundleArgs(['--version', '0.6.0', '--out', OUT, '--updater'], 'darwin', 'arm64').updater,
    ).toBe(true);
  });

  test('the updater archive is a macOS kind only, gzip at the head, floor like the rest', () => {
    // One row today. Linux and Windows updater artifacts exist in the
    // plugin but are not signed by this release yet; adding them is a
    // row here and a leg in release.yml, not a redesign.
    expect(UPDATER_KINDS.map((k) => k.id)).toEqual(['app-tar']);
    expect(updaterKindsForOs('macos').map((k) => k.id)).toEqual(['app-tar']);
    expect(updaterKindsForOs('linux')).toEqual([]);
    expect(updaterKindsForOs('windows')).toEqual([]);
    const k = appTar();
    expect(k.dir).toBe('macos');
    expect(k.ext).toBe('.app.tar.gz');
    expect(k.magic).toEqual([0x1f, 0x8b]);
    expect(k.magicAt).toBe('head');
    expect(k.floorBytes).toBe(kind('dmg').floorBytes);
  });

  test('the published name follows the image\'s, and it serves Apple silicon only', () => {
    const k = appTar();
    // Same rule as the image: plain, because one Mac arch is built.
    expect(k.releasedName('0.6.0', MAC_ARCH)).toBe('Screepub-macOS.app.tar.gz');
    // The plugin asks for darwin-<arch of the RUNNING binary>. Through the
    // identifier release the archive was universal and answered for both
    // darwin keys; from the release after it, darwin-aarch64 alone. An
    // Intel Mac running an earlier universal build then finds no key for
    // itself and check() rejects (desktop/ui/update.js says so in plain
    // words). darwin-x86_64 coming back here means an Intel build came back.
    expect(k.platformKeys(MAC_ARCH)).toEqual(['darwin-aarch64']);
  });

  test('the published name can be read back to its arch, which is how the manifest builder finds it', () => {
    const k = appTar();
    expect(k.archOf(k.releasedName('0.6.0', MAC_ARCH))).toBe('arm64');
    expect(k.archOf('Screepub-macOS.app.tar.gz')).toBe('arm64');
    // The per-arch names of the universal era are not this kind any more.
    expect(k.archOf('Screepub-macOS-x64.app.tar.gz')).toBeUndefined();
    expect(k.archOf('Screepub-macOS-arm64.app.tar.gz')).toBeUndefined();
    expect(k.archOf('Screepub-macOS.dmg')).toBeUndefined();
    expect(k.archOf('Screepub-macOS.app.tar.gz.sig')).toBeUndefined();
    expect(k.archOf('Screepub-macOS-riscv.app.tar.gz')).toBeUndefined();
    expect(k.archOf('Screepub-macOS-universal.app.tar.gz')).toBeUndefined();
    // The pre-F2 name is not this kind any more: a manifest built from an
    // old release's leftovers must not quietly pick it up.
    expect(k.archOf('Screepub-Desktop-macOS-universal.app.tar.gz')).toBeUndefined();
  });

  test('the verifier accepts a plausible archive and rejects a non-gzip one', () => {
    const k = appTar();
    const good = join(OUT, 'plausible.app.tar.gz');
    plausible(good, k as unknown as BundleKind);
    expect(() => verifyBundleFile(good, k)).not.toThrow();
    const bad = join(OUT, 'not-gzip.app.tar.gz');
    plausible(bad, kind('deb'));
    expect(() => verifyBundleFile(bad, k)).toThrow(/magic/);
  });

  test('with --updater the archive and its signature come out under the published names, byte for byte', async () => {
    const repo = agreeingRepo('0.6.0');
    const { calls, spawn } = fakeMacCargo(repo.desktopDir);
    const out = join(OUT, 'run-updater');
    const made = await buildBundles(macRun(out, true), spawn, repo, withKey);
    const names = made.map((p) => p.replace(/^.*[/\\]/, '')).sort();
    expect(names).toEqual([
      'Screepub-macOS.app.tar.gz',
      'Screepub-macOS.app.tar.gz.sig',
      'Screepub-macOS.dmg',
    ]);
    expect(calls[0]).toContain(UPDATER_OVERLAY);
    // The signature is the plugin's whole basis for trusting the download,
    // and latest.json carries its CONTENT. A copy that changed one byte
    // would make every install fail with "signature could not be decoded".
    expect(readFileSync(join(out, 'Screepub-macOS.app.tar.gz.sig'), 'utf8')).toBe(
      fakeSignatureBox('Screepub.app.tar.gz'),
    );
    // SHA256SUMS-app keeps naming installers only: the archive is proven by
    // its signature, and app-upload rebuilds the checksums file over
    // exactly the four installers anyway.
    const sums = parseChecksums(readFileSync(join(out, 'SHA256SUMS-app'), 'utf8'));
    expect([...sums.keys()]).toEqual(['Screepub-macOS.dmg']);
  });

  test('without --updater a macOS run ignores an archive that happens to be there', async () => {
    // The push path and a local build. Whatever a previous signed run
    // left in bundle/macos must not be swept into an unsigned release.
    const repo = agreeingRepo('0.6.0');
    const { calls, spawn } = fakeMacCargo(repo.desktopDir);
    const out = join(OUT, 'run-no-updater');
    const made = await buildBundles(macRun(out, false), spawn, repo, {});
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual(['Screepub-macOS.dmg']);
    expect(calls[0]).not.toContain(UPDATER_OVERLAY);
  });

  test('--updater without the signing key fails BEFORE cargo is spawned', async () => {
    // The CLI would fail too, after a full universal build, with "A public
    // key has been found, but no private key". Twenty minutes is a long
    // way to travel to read an environment variable.
    const repo = agreeingRepo('0.6.0');
    const { calls, spawn } = fakeMacCargo(repo.desktopDir);
    await expect(
      buildBundles(macRun(join(OUT, 'run-nokey'), true), spawn, repo, {}),
    ).rejects.toThrow(/TAURI_SIGNING_PRIVATE_KEY/);
    expect(calls.length).toBe(0);
  });

  test('--updater with the key but no password variable at all fails BEFORE cargo, and says empty is fine', async () => {
    // The variable may be empty; it may not be missing. Missing means
    // tauri-cli prompts, and there is no terminal, so the failure lands
    // after the full build as "Device not configured".
    const repo = agreeingRepo('0.6.0');
    const { calls, spawn } = fakeMacCargo(repo.desktopDir);
    await expect(
      buildBundles(macRun(join(OUT, 'run-nopw'), true), spawn, repo, {
        TAURI_SIGNING_PRIVATE_KEY: 'set',
      }),
    ).rejects.toThrow(/TAURI_SIGNING_PRIVATE_KEY_PASSWORD[\s\S]*empty/);
    expect(calls.length).toBe(0);
  });

  test('a cargo that exits 0 and writes no archive fails, naming it', async () => {
    const repo = agreeingRepo('0.6.0');
    const { spawn } = fakeMacCargo(repo.desktopDir, { tar: false, sig: false });
    await expect(
      buildBundles(macRun(join(OUT, 'run-notar'), true), spawn, repo, withKey),
    ).rejects.toThrow(/\.app\.tar\.gz/);
  });

  test('an archive with no signature beside it fails, naming the signature', async () => {
    const repo = agreeingRepo('0.6.0');
    const { spawn } = fakeMacCargo(repo.desktopDir, { tar: true, sig: false });
    await expect(
      buildBundles(macRun(join(OUT, 'run-nosig'), true), spawn, repo, withKey),
    ).rejects.toThrow(/\.sig/);
  });

  test('a signature that is not a minisign box fails, and the archive is not published', async () => {
    const repo = agreeingRepo('0.6.0');
    const { spawn } = fakeMacCargo(repo.desktopDir, { tar: true, sig: 'definitely not a signature' });
    const out = join(OUT, 'run-badsig');
    await expect(buildBundles(macRun(out, true), spawn, repo, withKey)).rejects.toThrow(
      /signature|base64|untrusted comment/i,
    );
    expect(() => readFileSync(join(out, 'Screepub-macOS.app.tar.gz'))).toThrow();
  });

  test('the macOS run passes the target triple and the overlay through', async () => {
    const repo = agreeingRepo('0.6.0');
    const { calls, spawn } = fakeCargo(kindsForOs('macos'), repo.desktopDir);
    const made = await buildBundles(
      {
        version: '0.6.0',
        outDir: join(OUT, 'run-macos'),
        os: 'macos',
        arch: 'arm64',
        target: 'aarch64-apple-darwin',
        config: 'tauri.transition.conf.json',
      },
      spawn,
      repo,
    );
    expect(calls[0]).toContain('--target');
    expect(calls[0]).toContain('aarch64-apple-darwin');
    expect(calls[0]).toContain('--config');
    expect(calls[0]).toContain('tauri.transition.conf.json');
    // And it looked under the triple: nothing was written to the untargeted
    // directory, so finding an artifact at all proves the path.
    expect(made.map((p) => p.replace(/^.*[/\\]/, ''))).toEqual(['Screepub-macOS.dmg']);
  });

  test('an Intel Mac run is refused BEFORE cargo, however the args were made', async () => {
    // The parser refuses it too; this is the same rule for a caller that
    // builds BundleArgs by hand. An x64 image would be published under the
    // one Mac name.
    const repo = agreeingRepo('0.6.0');
    const cases: [BundleArch, string | undefined][] = [
      ['x64', 'x86_64-apple-darwin'],
      ['x64', undefined],
      ['arm64', 'x86_64-apple-darwin'],
      ['arm64', 'universal-apple-darwin'],
      ['arm64', undefined],
    ];
    for (const [arch, target] of cases) {
      const { calls, spawn } = fakeCargo(kindsForOs('macos'), repo.desktopDir);
      await expect(
        buildBundles(
          { version: '0.6.0', outDir: join(OUT, `run-intel-${arch}-${target}`), os: 'macos', arch, target },
          spawn,
          repo,
        ),
      ).rejects.toThrow(/Apple silicon/);
      expect(calls.length).toBe(0);
    }
  });
});

// ── a Mac bundle is Apple silicon only ───────────────────────────────
//
// The identifier release (F2) was the last universal Mac image: the frozen
// Swift updater installs the first .dmg on a release with no architecture
// check, so F2 had to open on Intel too. From the release after it, Intel
// Macs are not supported (owner, 2026-10-01; plan
// 2026-10-01-f2-handover-amendment.md). These pins fail if an Intel or a
// universal Mac bundle can be asked for again.
describe('a Mac bundle is Apple silicon only', () => {
  test('the arch and the cargo triple are named once, here', () => {
    expect(MAC_ARCH).toBe('arm64');
    expect(MAC_TARGET).toBe('aarch64-apple-darwin');
  });

  test('a Mac build always names the triple, so the runner cannot choose it', () => {
    // An Intel runner building "for the host" would make an x64 image and
    // publish it under the one Mac name. With the triple named, cargo
    // builds Apple silicon wherever it runs.
    const args = parseBundleArgs(['--version', '0.6.0', '--out', '/o'], 'darwin', 'arm64');
    expect(args.arch).toBe('arm64');
    expect(args.target).toBe('aarch64-apple-darwin');
    expect(
      parseBundleArgs(['--version', '0.6.0', '--out', '/o', '--arch', 'arm64'], 'darwin', 'arm64').target,
    ).toBe('aarch64-apple-darwin');
  });

  test('an Intel Mac is refused, whether it is the host or asked for', () => {
    expect(() => parseBundleArgs(['--version', '0.6.0', '--out', '/o'], 'darwin', 'x64')).toThrow(
      /Apple silicon/,
    );
    expect(() =>
      parseBundleArgs(['--version', '0.6.0', '--out', '/o', '--arch', 'x64'], 'darwin', 'arm64'),
    ).toThrow(/Apple silicon/);
    // Cross-building FOR Apple silicon from an Intel Mac is still a Mac
    // build of the one arch, and is allowed.
    expect(
      parseBundleArgs(['--version', '0.6.0', '--out', '/o', '--arch', 'arm64'], 'darwin', 'x64').target,
    ).toBe('aarch64-apple-darwin');
  });

  test('a Mac --target other than the Apple silicon triple is refused', () => {
    for (const target of ['x86_64-apple-darwin', 'universal-apple-darwin']) {
      expect(() =>
        parseBundleArgs(['--version', '0.6.0', '--out', '/o', '--target', target], 'darwin', 'arm64'),
      ).toThrow(/Apple silicon/);
    }
  });

  test('universal is not an architecture any more, on any OS', () => {
    for (const [platform, host] of [['darwin', 'arm64'], ['linux', 'x64'], ['win32', 'x64']] as const) {
      expect(() =>
        parseBundleArgs(['--version', '0.6.0', '--out', '/o', '--arch', 'universal'], platform, host),
      ).toThrow(/universal/);
    }
  });
});
