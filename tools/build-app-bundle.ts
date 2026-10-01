// Build, name and VERIFY the installable app bundles.
//
//   bun tools/build-app-bundle.ts --version 0.6.0 --out dist/
//   bun tools/build-app-bundle.ts --version 0.6.0 --out dist/ \
//     --arch arm64 --updater
//       # the release's macOS leg: ALSO the updater archive and its
//       # signature. Needs TAURI_SIGNING_PRIVATE_KEY (a path or the key
//       # text) and TAURI_SIGNING_PRIVATE_KEY_PASSWORD (set, and "" for a
//       # key without one) in the environment.
//
// A Mac bundle is Apple silicon ONLY, built with --target
// aarch64-apple-darwin wherever it runs. The identifier release was the
// last universal image (the frozen Swift updater installs the first .dmg on
// a release with no architecture check, so that release had to open on
// Intel too); from the release after it Intel Macs are not supported
// (owner, 2026-10-01; plan
// docs/superpowers/plans/2026-10-01-f2-handover-amendment.md). An x64 or
// universal Mac bundle is refused rather than built, because it would be
// published under the one Mac name.
//
// A Bun script and not workflow YAML, following tools/build-cli.ts exactly:
// YAML can only be tested by cutting a release, and a release tool nobody
// can run locally is a release tool nobody can debug. It is also how the
// Linux arm64 .deb gets made by hand if no arm64 runner is available.
//
// It never signs anything. On macOS, tauri-bundler signs and notarizes from
// the APPLE_* environment variables the release workflow sets; there is no
// codesign call in this file.
// The updater archive's minisign signature is likewise tauri-cli's own,
// from TAURI_SIGNING_PRIVATE_KEY: this file only checks the key is SET
// before spending a build, and that what came out is a signature.

import {
  closeSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { REPO_DIR, VERSION_RE, writeChecksums, type Spawn } from './build-cli';
import { parseSignatureBox } from './update-signature';

export type BundleOs = 'linux' | 'macos' | 'windows';
export type BundleArch = 'x64' | 'arm64';

/** The one architecture a Mac bundle is built for, and the cargo triple
 *  that builds it. Named here and nowhere else: release.yml passes
 *  `--arch arm64` and never a triple. See the header for why it is one. */
export const MAC_ARCH: BundleArch = 'arm64';
export const MAC_TARGET = 'aarch64-apple-darwin';

/** What every artifact this tool handles has in common: a container it
 *  can be checked against, and a scale it must clear. */
export interface ArtifactShape {
  id: string;
  ext: string;
  /** Container magic, checked rather than assumed. */
  magic: readonly number[];
  /** Where that magic sits. A UDIF image's `koly` block is the LAST 512
   *  bytes of the file; a DMG has no header magic at all. */
  magicAt: 'head' | 'udif-trailer';
  floorBytes: number;
}

export interface BundleKind extends ArtifactShape {
  id: 'deb' | 'rpm' | 'dmg' | 'nsis';
  os: BundleOs;
  /** The directory under `target/[<triple>/]release/bundle/`. */
  dir: string;
  /** The stable published filename. Deliberately ours, not the bundler's:
   *  tauri names the DMG after productName, the version and the arch
   *  (`Screepub_0.8.0_aarch64.dmg`), and a page linking to
   *  releases/latest/download/<name> needs a name that never changes. */
  releasedName(version: string, arch: BundleArch): string;
}

/** Every bundle carries the ~102 MB engine — the real arm64 .deb and .rpm
 *  measure ~44.3 MB compressed — so 10 MB is an unambiguous floor. Same
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
    // No version in the name, on purpose (spec 2026-09-22, part 4): pages
    // link to releases/latest/download/<name>, which a versioned name breaks
    // at every release. The version is still inside the package.
    releasedName: (_v, arch) => `Screepub-linux-${arch === 'x64' ? 'amd64' : 'arm64'}.deb`,
  },
  {
    id: 'rpm',
    os: 'linux',
    dir: 'rpm',
    ext: '.rpm',
    magic: RPM_MAGIC,
    magicAt: 'head',
    floorBytes: FLOOR,
    releasedName: (_v, arch) => `Screepub-linux-${arch === 'x64' ? 'x86_64' : 'aarch64'}.rpm`,
  },
  {
    // The Mac image takes the Swift app's old name ON PURPOSE, from the
    // identifier release on (owner, 2026-10-01): every page and outside
    // link already points at releases/latest/download/Screepub-macOS.dmg,
    // and the Swift job no longer publishes an image of its own, so this is
    // the one .dmg the frozen Swift updater can find. Only Apple silicon is
    // built (MAC_ARCH), so, like the Windows row, the arch is not in the
    // name; parseBundleArgs and buildBundles refuse any other Mac arch.
    id: 'dmg',
    os: 'macos',
    dir: 'dmg',
    ext: '.dmg',
    magic: KOLY_MAGIC,
    magicAt: 'udif-trailer',
    floorBytes: FLOOR,
    releasedName: () => 'Screepub-macOS.dmg',
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
    releasedName: () => 'Screepub-windows-x64-setup.exe',
  },
];

export const DESKTOP_DIR = join(REPO_DIR, 'desktop', 'src-tauri');

export function kindsForOs(os: BundleOs): BundleKind[] {
  return BUNDLE_KINDS.filter((k) => k.os === os);
}

// ── the updater archive ──────────────────────────────────────────────
//
// With bundle.createUpdaterArtifacts on, tauri-bundler ALSO writes the
// thing the in-app updater downloads: on macOS a `.tar.gz` of the .app,
// beside the .app under bundle/macos/. tauri-cli then signs it and writes
// the minisign signature beside that as `<archive>.sig`. Neither is an
// installer: nobody double-clicks them, latest.json points at them.
//
// The flag lives in an OVERLAY and not in tauri.conf.json, because the CLI
// fails any bundle that asks for updater artifacts without the private
// key, and the push workflow and every local build have no key. --updater
// passes the overlay and, afterwards, insists the archive and its
// signature exist and are what they claim to be.

export const UPDATER_OVERLAY = 'tauri.updater.conf.json';
export const SIGNATURE_EXT = '.sig';
const GZIP_MAGIC = [0x1f, 0x8b];

/** The key the plugin looks up in latest.json: `<os>-<arch>` of the
 *  RUNNING binary (tauri-plugin-updater 2.12.0, updater.rs, `get_urls`,
 *  which also tries `<os>-<arch>-app` first). Apple silicon only: the
 *  universal archive of the identifier release answered for darwin-x86_64
 *  too, and the release after it does not, so an Intel Mac running that
 *  older build finds no key for itself and check() rejects. The window
 *  says so in plain words (desktop/ui/update.js). */
export type PlatformKey = 'darwin-aarch64';

export interface UpdaterKind extends ArtifactShape {
  id: 'app-tar';
  os: BundleOs;
  dir: string;
  releasedName(version: string, arch: BundleArch): string;
  /** The inverse of releasedName, for a tool that only has the filename:
   *  the manifest builder runs in a job that never saw the build. */
  archOf(releasedName: string): BundleArch | undefined;
  platformKeys(arch: BundleArch): PlatformKey[];
}

export const UPDATER_KINDS: readonly UpdaterKind[] = [
  {
    // One row. Linux and Windows updater artifacts exist in the plugin but
    // this release does not sign them; adding one is a row here, a key in
    // the manifest builder, and the secret on that leg in release.yml.
    id: 'app-tar',
    os: 'macos',
    dir: 'macos',
    ext: '.app.tar.gz',
    magic: GZIP_MAGIC,
    magicAt: 'head',
    floorBytes: FLOOR,
    // Named like the image beside it: plain, one Mac arch.
    releasedName: () => 'Screepub-macOS.app.tar.gz',
    archOf: (name) => (name === 'Screepub-macOS.app.tar.gz' ? MAC_ARCH : undefined),
    platformKeys: () => ['darwin-aarch64'],
  },
];

export function updaterKindsForOs(os: BundleOs): UpdaterKind[] {
  return UPDATER_KINDS.filter((k) => k.os === os);
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

export function buildArgv(
  os: BundleOs,
  opts: { target?: string; config?: string; updater?: boolean },
): string[] {
  const argv = ['cargo', 'tauri', 'build', '--bundles', bundleListFor(os)];
  if (opts.target) argv.push('--target', opts.target);
  if (opts.config) argv.push('--config', opts.config);
  // AFTER any other overlay: the CLI takes --config repeatedly and merges
  // them in the order given, so this one lands on top of whatever renamed
  // the product rather than under it.
  if (opts.updater) argv.push('--config', UPDATER_OVERLAY);
  return argv;
}

/** `cargo tauri build --target X` writes to `target/X/release/bundle/...`,
 *  not `target/release/bundle/...`. Looking in the wrong one is how a build
 *  "succeeds" and produces nothing.
 *
 *  `desktopDir` is a parameter and not only the constant so a test can point
 *  a whole run at a temporary tree instead of writing fake artifacts into
 *  the repo's own `target/`, where they would be found by the NEXT run. */
export function bundleDirFor(
  kind: { dir: string },
  target?: string,
  desktopDir: string = DESKTOP_DIR,
): string {
  return target
    ? join(desktopDir, 'target', target, 'release', 'bundle', kind.dir)
    : join(desktopDir, 'target', 'release', 'bundle', kind.dir);
}

/** The one file the bundler wrote, found rather than reconstructed.
 *
 *  Reconstructing tauri's own filename would mean two places agreeing about
 *  a third party's format string -- and productName, which is half of it,
 *  is exactly what a config overlay can change (the macOS build carried one
 *  until the identifier release). So: glob for the
 *  extension and insist on EXACTLY ONE match, which is what turns a glob
 *  into a fact. `rw.` is excluded by name because bundle_dmg leaves
 *  `rw.$$.<name>.dmg` behind when it dies partway. */
export function discoverArtifact(dir: string, kind: ArtifactShape): string {
  if (!existsSync(dir)) {
    throw new Error(
      `build-app-bundle: ${kind.id}: no bundle directory at ${dir}. cargo tauri build ` +
        'reported success and produced nothing there.',
    );
  }
  const found = readdirSync(dir)
    .filter((n) => n.endsWith(kind.ext) && !n.startsWith('rw.'))
    .sort();
  if (found.length !== 1) {
    throw new Error(
      `build-app-bundle: ${kind.id}: expected exactly one ${kind.ext} in ${dir}, found ` +
        `${found.length}${found.length ? ` (${found.join(', ')})` : ''}. A leftover from an ` +
        'earlier version is the usual cause; clear the directory and rebuild.',
    );
  }
  return join(dir, found[0]!);
}

/** Read `n` bytes at a position, never the whole file: a real bundle is
 *  ~44 MB and this inspects at most eight bytes of it. */
function bytesAt(path: string, n: number, position: number): Uint8Array {
  const fd = openSync(path, 'r');
  try {
    const buf = new Uint8Array(n);
    const read = readSync(fd, buf, 0, n, position);
    return buf.subarray(0, read);
  } finally {
    closeSync(fd);
  }
}

/** Check what was PRODUCED. `cargo tauri build` can exit 0 and leave a
 *  truncated image, and a copy step can put the wrong container behind the
 *  right name.
 *
 *  There is deliberately no shared file-list assertion across kinds. An
 *  earlier note here said the real .deb and .rpm of one build do not carry
 *  the same payload -- deb four icon sizes, rpm one, binaries of different
 *  sizes. That was measured on a STALE pair and did not reproduce: the
 *  2026-09-14 build ships the same nine files at the same sizes in both,
 *  differing only in entry order and the rpm's `./` prefix. The assertion
 *  still does not exist, for the weaker and more durable reason: nothing
 *  makes two different bundlers stay in step, so the only thing true of
 *  every kind is its container and its scale. */
export function verifyBundleFile(path: string, kind: ArtifactShape): void {
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
      ? bytesAt(path, 512, bytes - 512)
      : bytesAt(path, kind.magic.length, 0);
  const ok = window.length >= kind.magic.length && kind.magic.every((b, i) => window[i] === b);
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

/** The crate's own version: the `version` key of the `[package]` TABLE and
 *  nothing else. A naive "first version after [package]" scan runs straight
 *  past the end of the table into `[dependencies]` when [package] has no
 *  version of its own, and then reports a dependency's `version = "2"` as
 *  the crate's -- a gate that fails OPEN. The table is cut at the next
 *  header first, so a missing key answers undefined and the gate fails. */
export function cargoPackageVersion(text: string): string | undefined {
  const header = /^[ \t]*\[package\][ \t]*\r?$/m.exec(text);
  if (!header) return undefined;
  const rest = text.slice(header.index + header[0].length);
  const next = /^[ \t]*\[/m.exec(rest);
  const table = next ? rest.slice(0, next.index) : rest;
  return /^[ \t]*version[ \t]*=[ \t]*"([^"]*)"/m.exec(table)?.[1];
}

/** package.json is what the ENGINE reports, Cargo.toml is the crate, and
 *  tauri.conf.json names the bundle file, the Info.plist, the deb Version:
 *  and the NSIS product version. A release in which they disagree ships an
 *  installer whose About line contradicts its own filename -- which is
 *  exactly what a real 0.6.0 bundle around a 0.5.4 engine did on
 *  2026-09-14. This is a RELEASE-tool gate, not a `bun test` assertion: the
 *  split is a normal working-branch state (package.json is 0.5.4 while the
 *  crate and the config are already 0.6.0) and a test that failed on every
 *  branch would be deleted within a week. */
export function assertBundleVersions(version: string, repoDir: string = REPO_DIR): void {
  const pkg = (
    JSON.parse(readFileSync(join(repoDir, 'package.json'), 'utf8')) as { version?: string }
  ).version;
  const cargo = cargoPackageVersion(
    readFileSync(join(repoDir, 'desktop', 'src-tauri', 'Cargo.toml'), 'utf8'),
  );
  const conf = (
    JSON.parse(readFileSync(join(repoDir, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8')) as {
      version?: string;
    }
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
  /** Also produce the updater archive and its signature. Off by default:
   *  it needs TAURI_SIGNING_PRIVATE_KEY, which only a release has. */
  updater?: boolean;
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
  hostArch: string = process.arch,
): BundleArgs {
  const { values } = parseArgs({
    args: argv,
    options: {
      version: { type: 'string' },
      out: { type: 'string' },
      target: { type: 'string' },
      // process.arch says what THIS machine is. --arch names another, which
      // on a Mac is only ever how an Intel Mac asks for the Apple silicon
      // bundle it can cross-build, once `rustup target add
      // aarch64-apple-darwin` has installed that target's standard library.
      arch: { type: 'string' },
      config: { type: 'string' },
      updater: { type: 'boolean', default: false },
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
        '~44 MB and a Linux run writes two of them plus a checksums file.',
    );
  }
  const arch = values.arch ?? hostArch;
  if (arch === 'universal') {
    throw new Error(
      'build-app-bundle: "universal" is not an architecture any more. The identifier release ' +
        'was the last universal Mac image; a Mac bundle is Apple silicon only (--arch arm64). ' +
        'See docs/superpowers/plans/2026-10-01-f2-handover-amendment.md.',
    );
  }
  if (arch !== 'x64' && arch !== 'arm64') {
    throw new Error(`build-app-bundle: no app bundle is defined for architecture "${arch}"`);
  }
  const os = osForPlatform(platform);
  // The Mac triple is always NAMED, never left to the host: an Intel
  // machine building "for itself" would make an x64 image and publish it
  // under the one Mac name. assertAppleSiliconMac refuses any other.
  const target = values.target ?? (os === 'macos' ? MAC_TARGET : undefined);
  assertAppleSiliconMac(os, arch, target);

  return {
    version,
    outDir: resolve(values.out),
    os,
    arch,
    target,
    config: values.config,
    updater: values.updater ?? false,
  };
}

/** A Mac bundle is Apple silicon, built for the Apple silicon triple, or it
 *  is refused. Called by the parser AND by buildBundles, so a caller that
 *  builds BundleArgs by hand meets the same rule. No-op off macOS. */
export function assertAppleSiliconMac(os: BundleOs, arch: BundleArch, target: string | undefined): void {
  if (os !== 'macos') return;
  if (arch !== MAC_ARCH || target !== MAC_TARGET) {
    throw new Error(
      `build-app-bundle: a Mac bundle is built for Apple silicon only (--arch ${MAC_ARCH}, ` +
        `target ${MAC_TARGET}), and this asked for arch ${arch}, target ${target ?? '<the host>'}. ` +
        'Intel Macs are not supported after the identifier release ' +
        '(docs/superpowers/plans/2026-10-01-f2-handover-amendment.md), and an Intel or universal ' +
        'image would be published under the one Mac name, Screepub-macOS.dmg. An Intel Mac can ' +
        'still cross-build the Apple silicon bundle: rustup target add aarch64-apple-darwin, ' +
        'then --arch arm64.',
    );
  }
}

const realSpawn: Spawn = (argv, cwd) => {
  const proc = Bun.spawnSync(argv, { cwd, stdout: 'inherit', stderr: 'pipe' });
  return { exitCode: proc.exitCode ?? 1, stderr: proc.stderr.toString() };
};

/** Where a run reads the repo and writes its build tree. Parameters, not
 *  constants, so the rename-and-checksum half can be exercised by a test:
 *  the version gate refuses to run in the repo on any working branch, which
 *  would otherwise leave this whole function untested until a release. */
export interface BundleDirs {
  repoDir?: string;
  desktopDir?: string;
}

/** One cargo run per OS, then discover, verify, rename and checksum.
 *
 *  Verifying BEFORE the rename means a broken artifact fails naming the
 *  bundler's own file, which is the one a person can go and look at. */
export async function buildBundles(
  args: BundleArgs,
  spawn: Spawn = realSpawn,
  dirs: BundleDirs = {},
  env: Record<string, string | undefined> = process.env,
): Promise<string[]> {
  const repoDir = dirs.repoDir ?? REPO_DIR;
  const desktopDir = dirs.desktopDir ?? join(repoDir, 'desktop', 'src-tauri');
  assertAppleSiliconMac(args.os, args.arch, args.target);
  assertBundleVersions(args.version, repoDir);
  if (args.updater && !env.TAURI_SIGNING_PRIVATE_KEY) {
    // tauri-cli would fail on exactly this, AFTER the whole Mac build,
    // with "A public key has been found, but no private key". Twenty
    // minutes is a long way to travel to read one variable.
    throw new Error(
      'build-app-bundle: --updater needs TAURI_SIGNING_PRIVATE_KEY in the environment, and ' +
        'it is not set. See docs/release-secrets.md §4.',
    );
  }
  if (args.updater && env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD === undefined) {
    // SET, not necessarily non-empty. tauri-cli reads an absent password
    // variable as "prompt me", and a build spawned from here has no
    // terminal to prompt on: measured 2026-09-21 as "incorrect updater
    // private key password: Device not configured (os error 6)", again
    // after the whole build. release.yml always sets it, to '' when there
    // is no such secret, which is exactly what a password-less key wants.
    throw new Error(
      'build-app-bundle: --updater needs TAURI_SIGNING_PRIVATE_KEY_PASSWORD SET in the ' +
        'environment, empty if the key has no password (TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""). ' +
        'Unset, tauri-cli tries to prompt for one and there is no terminal here.',
    );
  }
  mkdirSync(args.outDir, { recursive: true });

  const argv = buildArgv(args.os, {
    target: args.target,
    config: args.config,
    updater: args.updater,
  });
  console.log(`── ${argv.join(' ')}`);
  const { exitCode, stderr } = spawn(argv, desktopDir);
  if (exitCode !== 0) {
    throw new Error(
      `build-app-bundle: cargo tauri build failed (exit ${exitCode})\n${stderr.trim()}`,
    );
  }

  const names: string[] = [];
  for (const kind of kindsForOs(args.os)) {
    const built = discoverArtifact(bundleDirFor(kind, args.target, desktopDir), kind);
    verifyBundleFile(built, kind);
    const name = kind.releasedName(args.version, args.arch);
    const dest = join(args.outDir, name);
    copyFileSync(built, dest);
    // Verified again AFTER the copy: a full disk truncates silently.
    verifyBundleFile(dest, kind);
    console.log(`   ${basename(built)} → ${name}  ${statSync(dest).size} bytes  ok`);
    names.push(name);
  }

  // The updater archive and its signature, only when asked. They are NOT
  // in SHA256SUMS-app: the archive is proven by its signature, and
  // app-upload rebuilds that file over exactly the installers anyway.
  const extras: string[] = [];
  if (args.updater) {
    for (const kind of updaterKindsForOs(args.os)) {
      const built = discoverArtifact(bundleDirFor(kind, args.target, desktopDir), kind);
      verifyBundleFile(built, kind);
      const sig = `${built}${SIGNATURE_EXT}`;
      if (!existsSync(sig)) {
        throw new Error(
          `build-app-bundle: ${kind.id}: ${basename(built)} has no ${basename(sig)} beside it. ` +
            'tauri-cli writes the signature right after bundling, so a missing one means ' +
            `signing did not run. Was --config ${UPDATER_OVERLAY} passed?`,
        );
      }
      // Read as text, and checked BEFORE the archive is copied out: a
      // signature that is not one means nothing here may be published.
      const sigText = readFileSync(sig, 'utf8');
      try {
        parseSignatureBox(sigText);
      } catch (err) {
        throw new Error(
          `build-app-bundle: ${kind.id}: ${basename(sig)} is not a minisign signature: ` +
            (err as Error).message,
        );
      }
      const name = kind.releasedName(args.version, args.arch);
      const dest = join(args.outDir, name);
      copyFileSync(built, dest);
      verifyBundleFile(dest, kind);
      const sigDest = `${dest}${SIGNATURE_EXT}`;
      copyFileSync(sig, sigDest);
      // Byte for byte. latest.json will carry this text, and the plugin
      // refuses an install over a signature that decodes wrongly.
      if (readFileSync(sigDest, 'utf8') !== sigText) {
        throw new Error(`build-app-bundle: ${kind.id}: ${basename(sigDest)} did not copy intact`);
      }
      console.log(
        `   ${basename(built)} → ${name} (+${SIGNATURE_EXT})  ${statSync(dest).size} bytes  ok`,
      );
      extras.push(name, `${name}${SIGNATURE_EXT}`);
    }
  }

  // NOT "SHA256SUMS": release.yml's cross-upload job already publishes a
  // file by that name for the three CLI archives, onto the same release.
  writeChecksums(args.outDir, names, 'SHA256SUMS-app');
  return [...names, ...extras].map((n) => join(args.outDir, n));
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
