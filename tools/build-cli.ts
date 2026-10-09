// Compile, package and VERIFY the CLI artifacts.
//
// A Bun script and not shell, because a build matrix in bash is exactly the
// platform-locked tooling the cross-platform ADR is retiring, and because a
// release tool nobody can run locally is a release tool nobody can debug.
//
// Two tables, never mixed. TARGETS is the cross-compiled Linux and Windows
// set the cross job builds on Ubuntu. MAC_TARGETS is the Apple silicon
// tarball the Homebrew formula serves, and it is reachable ONLY with --mac,
// which also demands --sign <identity> or an explicit --unsigned: an unsigned
// Mach-O is a Gatekeeper warning for everyone who downloads it, so the cross
// job (which never passes --mac) still cannot produce one by accident. The
// Mac build signs each binary BEFORE packaging it, so the tarball holds the
// signed bytes, and can notarize it (--notarize; a bare Mach-O cannot be
// stapled, so notarization is recorded with Apple and checked online). This
// is the build the Swift app's release script used to do, moved here so it
// outlives the Swift app; plan 2026-10-01-f2-handover-amendment.md.
//
//   bun tools/build-cli.ts --version 0.6.0 --out dist/
//   bun tools/build-cli.ts --version 0.6.0 --out dist/ --only windows-x64
//   bun tools/build-cli.ts --version 0.6.0 --out dist/ --mac --unsigned
//   bun tools/build-cli.ts --version 0.6.0 --out dist/ --mac \
//     --sign "Developer ID Application: ... (TEAMID)" --notarize
//       # --notarize reads AC_API_KEY_PATH, AC_API_KEY_ID, AC_API_ISSUER_ID

import {
  closeSync,
  openSync,
  readSync,
  readFileSync,
  mkdirSync,
  chmodSync,
  rmSync,
  writeFileSync,
  existsSync,
  statSync,
} from 'node:fs';
import { join, isAbsolute, resolve, basename } from 'node:path';
import { parseArgs } from 'node:util';
import { createHash } from 'node:crypto';

export type BinaryFormat =
  | 'elf-x86-64'
  | 'elf-aarch64'
  | 'pe-x86-64'
  | 'macho-arm64'
  | 'macho-x86-64'
  | 'macho-universal'
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
  // Fat Mach-O headers are big-endian regardless of the slices inside them.
  const u32be = (o: number): number =>
    o >= 0 && o + 4 <= head.length ? view.getUint32(o, false) : -1;

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

  // Universal (fat) Mach-O. The header is big-endian ALWAYS, whatever the
  // slices inside are, which is why this needs its own reader. FAT_MAGIC
  // carries 20-byte fat_arch records; FAT_MAGIC_64 carries 32-byte
  // fat_arch_64, whose offset and size fields are 64-bit.
  //
  // Both arches are REQUIRED to answer 'macho-universal'; a one-slice fat
  // file is a thin binary wearing a universal wrapper. Nothing is BUILT
  // universal any more (Intel Macs are not supported after 0.9.0), and no row names this format or 'macho-x86-64'. They stay
  // in the detector so a binary that came out wrong is refused naming
  // what it really is, rather than "unknown".
  const beMagic = u32be(0);
  if (beMagic === 0xcafebabe || beMagic === 0xcafebabf) {
    const stride = beMagic === 0xcafebabf ? 32 : 20;
    const count = u32be(4);
    // A sane bound: a real fat binary has a handful of slices, and this also
    // stops a Java class file (same 0xcafebabe magic, different meaning)
    // from being read as a thousand arches.
    if (count < 1 || count > 16) return 'unknown';
    const cpus = new Set<number>();
    for (let i = 0; i < count; i++) {
      const cpu = u32be(8 + i * stride);
      if (cpu < 0) return 'unknown';
      cpus.add(cpu);
    }
    if (cpus.has(0x01000007) && cpus.has(0x0100000c)) return 'macho-universal';
    return 'unknown';
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

export type MacTargetId = 'macos-arm64';
export type TargetId = 'linux-x64' | 'linux-arm64' | 'windows-x64' | MacTargetId;

export interface Target {
  id: TargetId;
  /** The value for `bun build --compile --target=`. */
  bunTarget: string;
  /** What the compiled output must be. Checked, never assumed. */
  format: BinaryFormat;
  packaging: 'tar.gz' | 'zip';
  /** The name the binary has INSIDE the archive, so `tar -xzf` and
   *  `Expand-Archive` both drop a runnable `screepub` in the cwd, which is
   *  also what the Homebrew formula's `bin.install` expects of the macOS
   *  tarballs. */
  binaryName: string;
  archiveName: string;
}

/** The cross set. No darwin row, ever: those live in MAC_TARGETS, behind
 *  --mac. See the file header. */
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

/** The Homebrew formula's one download: Apple silicon. Through the
 *  identifier release there was an x64 row beside it; from the release
 *  after it Intel Macs are not supported (owner, 2026-10-01; plan
 *  docs/superpowers/plans/2026-10-01-f2-handover-amendment.md), and the
 *  formula says `depends_on arch: :arm64`. Still a table, so the cross
 *  set's machinery serves it unchanged. The archive name is
 *  tools/bump-tap.sh's, unchanged. */
export const MAC_TARGETS: readonly Target[] = [
  {
    id: 'macos-arm64',
    bunTarget: 'bun-darwin-arm64',
    format: 'macho-arm64',
    packaging: 'tar.gz',
    binaryName: 'screepub',
    archiveName: 'screepub-cli-macos-arm64.tar.gz',
  },
];

function isMacTarget(target: Target): boolean {
  return MAC_TARGETS.some((t) => t.id === target.id);
}

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
 *  suite. macOS answers `undefined` on purpose: the CROSS table has no
 *  macOS row, so a Mac has no host target in it. The Mac table has its own
 *  mapping, macTargetIdForHost. */
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

/** The Mac table's own host mapping, kept apart from targetIdForHost so that
 *  one still answers "nothing" on a Mac: the cross set has no Mac host. An
 *  Intel Mac answers nothing here too: nothing it could build ships. */
export function macTargetIdForHost(platform: string, arch: string): MacTargetId | undefined {
  if (platform === 'darwin' && arch === 'arm64') return 'macos-arm64';
  return undefined;
}

/** The repo root, from this file's location: the tool is run from anywhere
 *  and must still find package.json and src/cli.ts. */
export const REPO_DIR = join(import.meta.dir, '..');

/** A release version, MAJOR.MINOR.PATCH with an optional prerelease: the
 *  one rule every release tool checks its --version against. */
export const VERSION_RE = /^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$/;

/** How a --mac build is signed. `sign` is the codesign identity, `-` for
 *  ad-hoc, or null for an explicit --unsigned. */
export interface MacSigning {
  sign: string | null;
  notarize: boolean;
}

export interface BuildArgs {
  version: string;
  outDir: string;
  only: TargetId[];
  /** Present only for a --mac build. */
  mac?: MacSigning;
}

export function parseBuildArgs(argv: string[]): BuildArgs {
  const { values } = parseArgs({
    args: argv,
    options: {
      version: { type: 'string' },
      out: { type: 'string' },
      only: { type: 'string', multiple: true },
      mac: { type: 'boolean' },
      sign: { type: 'string' },
      unsigned: { type: 'boolean' },
      notarize: { type: 'boolean' },
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
      'build-cli: --out <dir> is required. There is no default: a full build leaves a ' +
        '64-119 MB binary AND a 39 MB or so archive per target, about 450 MB in all, ' +
        'and a default would write that somewhere you did not ask for.',
    );
  }

  let mac: MacSigning | undefined;
  if (values.mac) {
    if (values.sign !== undefined && values.unsigned) {
      throw new Error('build-cli: --sign and --unsigned were both given; a binary is one or the other');
    }
    if (values.sign === undefined && !values.unsigned) {
      throw new Error(
        'build-cli: --mac needs --sign <identity> (or `-` for ad-hoc) or an explicit --unsigned. ' +
          'An unsigned Mach-O is a Gatekeeper warning for everyone who downloads it, so it has ' +
          'to be asked for by name.',
      );
    }
    if (values.notarize && values.unsigned) {
      throw new Error('build-cli: --notarize needs a signed binary; Apple will not notarize an unsigned one');
    }
    mac = { sign: values.sign ?? null, notarize: values.notarize ?? false };
  } else if (values.sign !== undefined || values.unsigned || values.notarize) {
    throw new Error('build-cli: --sign, --unsigned and --notarize only apply to a --mac build');
  }

  const known = (mac ? MAC_TARGETS : TARGETS).map((t) => t.id);
  const requested = (values.only ?? [])
    .flatMap((s) => s.split(','))
    .map((s) => s.trim())
    .filter(Boolean);
  for (const id of requested) {
    if (!mac && MAC_TARGETS.some((t) => t.id === id)) {
      throw new Error(`build-cli: "${id}" is a macOS target; those build only with --mac`);
    }
    if (!known.includes(id as TargetId)) {
      throw new Error(`build-cli: unknown target "${id}"; known targets are ${known.join(', ')}`);
    }
  }

  return {
    version,
    outDir: isAbsolute(values.out) ? values.out : resolve(values.out),
    only: (requested.length ? requested : known) as TargetId[],
    ...(mac ? { mac } : {}),
  };
}

/** `screepub --version` reports package.json's version, inlined at compile
 *  time; the --version flag changes nothing about the binary. So the flag's
 *  job is to be CHECKED, on the cross and the --mac builds alike: a binary
 *  that disagrees with its tag lies about itself in every bug report it
 *  ever appears in. */
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

export interface SpawnResult {
  exitCode: number;
  stderr: string;
  /** Only the callers that read an answer (notarytool) look at this. */
  stdout?: string;
}
export type Spawn = (argv: string[], cwd: string) => SpawnResult;

const realSpawn: Spawn = (argv, cwd) => {
  // A missing program (codesign on Linux, say) throws rather than exiting;
  // answered as a failed run so the caller's message names the target.
  try {
    const proc = Bun.spawnSync(argv, { cwd, stdout: 'pipe', stderr: 'pipe' });
    return { exitCode: proc.exitCode ?? 1, stderr: proc.stderr.toString(), stdout: proc.stdout.toString() };
  } catch (err) {
    return { exitCode: 127, stderr: `${argv[0]}: ${(err as Error).message}` };
  }
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

/** The entitlements the CLI has always been signed with: bun's runtime JITs,
 *  and under the hardened runtime that needs allow-jit and its two
 *  companions. Moved, not changed, from the Swift app's folder. */
export const MAC_ENTITLEMENTS = join(REPO_DIR, 'tools', 'macos', 'screepub-cli.entitlements');

/** The exact codesign call the shipped CLI is signed with today:
 *  hardened runtime and a secure timestamp, both of which notarization
 *  requires. `identity` may be `-` for an ad-hoc signature (local runs and
 *  the e2e test); `--timestamp` is accepted and ignored there. */
export function signArgv(
  target: Target,
  outDir: string,
  identity: string,
  entitlements: string = MAC_ENTITLEMENTS,
): string[] {
  return [
    'codesign', '--force', '--options', 'runtime', '--timestamp',
    '--entitlements', entitlements,
    // The identifier the shipped CLI has always had. The Swift app's
    // release script signed a file named screepub-cli-<arch>, and codesign
    // takes the identifier from the file name; this signs one named
    // `screepub`, so it says so, or the designated requirement changes.
    '--identifier', `screepub-cli-${target.id.replace(/^macos-/, '')}`,
    '--sign', identity,
    binaryPath(target, outDir),
  ];
}

/** Sign, then check what the signature actually says rather than trusting
 *  codesign's exit code: it must verify strictly, carry the hardened runtime
 *  (notarization rejects a binary without it, and only after the upload), and
 *  for any identity but ad-hoc, name a team. */
export function signTarget(
  target: Target,
  outDir: string,
  identity: string,
  spawn: Spawn = realSpawn,
  repoDir: string = REPO_DIR,
): void {
  if (!isMacTarget(target)) {
    throw new Error(`build-cli: ${target.id} is not a macOS target and is never codesigned`);
  }
  const bin = binaryPath(target, outDir);
  const run = (argv: string[], what: string): string => {
    const { exitCode, stderr } = spawn(argv, repoDir);
    if (exitCode !== 0) {
      throw new Error(`build-cli: ${target.id}: ${what} failed (exit ${exitCode})\n${stderr.trim()}`);
    }
    return stderr;
  };
  run(signArgv(target, outDir, identity), 'codesign');
  run(['codesign', '--verify', '--strict', '--verbose=2', bin], 'codesign --verify');
  // codesign -dv writes its description to stderr.
  const described = run(['codesign', '-dv', bin], 'codesign -dv');
  if (!/flags=0x[0-9a-f]+\([^)]*\bruntime\b[^)]*\)/.test(described)) {
    throw new Error(
      `build-cli: ${target.id}: signed WITHOUT the hardened runtime; notarization would reject it\n${described.trim()}`,
    );
  }
  if (identity !== '-' && !/^TeamIdentifier=(?!not set)\S+/m.test(described)) {
    throw new Error(
      `build-cli: ${target.id}: signed with "${identity}" but the binary names no team ` +
        `(an ad-hoc signature?)\n${described.trim()}`,
    );
  }
}

export interface NotaryCredentials {
  keyPath: string;
  keyId: string;
  issuer: string;
}

/** The App Store Connect API key the release job already writes for
 *  notarization. Read up front so a missing one fails before any compile. */
export function notaryCredentials(env: Record<string, string | undefined> = process.env): NotaryCredentials {
  const names = ['AC_API_KEY_PATH', 'AC_API_KEY_ID', 'AC_API_ISSUER_ID'] as const;
  const missing = names.filter((n) => !env[n]);
  if (missing.length) {
    throw new Error(`build-cli: --notarize needs ${missing.join(' and ')} in the environment`);
  }
  return { keyPath: env.AC_API_KEY_PATH!, keyId: env.AC_API_KEY_ID!, issuer: env.AC_API_ISSUER_ID! };
}

/** notarytool takes an archive, not a bare Mach-O, so the signed binary
 *  rides in a zip that exists only for the submission. A bare Mach-O cannot
 *  be stapled; notarization is recorded with Apple and Gatekeeper checks it
 *  online. The zip is removed whatever happens. */
export function notarizeTarget(
  target: Target,
  outDir: string,
  creds: NotaryCredentials,
  spawn: Spawn = realSpawn,
  repoDir: string = REPO_DIR,
): void {
  const zip = join(buildDir(target, outDir), 'screepub-notarize.zip');
  try {
    const run = (argv: string[], what: string): string => {
      const { exitCode, stderr, stdout } = spawn(argv, repoDir);
      if (exitCode !== 0) {
        throw new Error(`build-cli: ${target.id}: ${what} failed (exit ${exitCode})\n${stderr.trim()}`);
      }
      return stdout ?? '';
    };
    run(['ditto', '-c', '-k', binaryPath(target, outDir), zip], 'ditto');
    // The verdict is READ, not inferred from the exit code: --output-format
    // json and status "Accepted", exactly as tauri-macos-sign 2.3.4 does
    // for the window every release (its NotarytoolSubmitOutput: id, status,
    // message). Anything else, including an answer that is not that JSON,
    // fails here with the id needed for `xcrun notarytool log`.
    const answer = run(
      ['xcrun', 'notarytool', 'submit', zip, '--key', creds.keyPath, '--key-id', creds.keyId,
        '--issuer', creds.issuer, '--output-format', 'json', '--wait'],
      'notarytool submit',
    );
    let verdict: { id?: string; status?: string; message?: string };
    try {
      verdict = JSON.parse(answer);
    } catch {
      throw new Error(`build-cli: ${target.id}: notarytool's answer was not JSON:\n${answer.trim() || '<empty>'}`);
    }
    if (verdict.status !== 'Accepted') {
      throw new Error(
        `build-cli: ${target.id}: notarization ${verdict.status ?? 'returned no status'} ` +
          `(${verdict.message ?? 'no message'}); see \`xcrun notarytool log ${verdict.id ?? '<no id>'}\``,
      );
    }
    console.log(`   ${target.id}: notarized, ${verdict.id}`);
  } finally {
    rmSync(zip, { force: true });
  }
}

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

/** Walk a tar we already hold in memory. Split out of tarGzEntries so a
 *  caller holding decompressed bytes -- a .deb's data.tar.gz member, an
 *  .rpm's payload -- does not need a temporary file to read them.
 *
 *  Regular files only, and only the 100-byte `name` field: the ustar
 *  `prefix` field and GNU 'L' long-name records are not joined on, because
 *  nothing we package reaches a 100-byte path. A path that did would come
 *  back truncated rather than as an error. */
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

/** Walk the tar ourselves. `tar -tzvf` answers in a listing format that
 *  differs between GNU tar and bsdtar; the header is 512 fixed bytes and
 *  gunzip is already in the runtime. */
export function tarGzEntries(archivePath: string): ArchiveEntry[] {
  return tarEntries(Bun.gunzipSync(readFileSync(archivePath)));
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
    // COPYFILE_DISABLE: macOS ships bsdtar, which preserves a file's extended
    // attributes by writing a SECOND, hidden member beside it named `._name`.
    // A tarball built on a Mac therefore hands the user a stray `._screepub`
    // next to the binary. GNU tar ignores this variable entirely, so setting
    // it is safe on the Linux runner that actually cuts releases — this is
    // about the archive being the same artifact wherever it was built.
    const proc = Bun.spawnSync(
      ['tar', '-czf', out, '-C', buildDir(target, outDir), target.binaryName],
      { stdout: 'pipe', stderr: 'pipe', env: { ...process.env, COPYFILE_DISABLE: '1' } },
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

export function sha256File(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

/** The format `sha256sum -c` and `shasum -a 256 -c` both parse: lowercase
 *  hex, exactly two spaces, a bare filename, a trailing newline. Sorted, so
 *  two runs of the same build produce the same file. Returns the text as
 *  well as writing it, so callers can assert on it without re-reading.
 *
 *  MERGES with an existing SHA256SUMS rather than overwriting it. Chosen
 *  over refusing-to-write because the partial file is the surprise, not the
 *  build: `--only windows-x64` into a directory that already holds a full
 *  build used to leave a ONE-LINE checksums file beside three archives —
 *  which `sha256sum -c` then passes, with two downloads silently unchecked.
 *  Refusing would have made the hand-run path need a flag to do the obvious
 *  thing. CI never passes --only (both callers are asserted free of it), so
 *  this only ever bites the hand-run path, which is the one with nobody
 *  reviewing the output.
 *
 *  Carried-over names are RE-HASHED from the files on disk, never copied
 *  out of the old text: a digest that no longer matches its file is the one
 *  failure this file exists to catch. A named file that is gone is dropped,
 *  loudly, rather than left behind to fail `-c` for a download nobody
 *  published. */
export function writeChecksums(
  outDir: string,
  archiveNames: string[],
  fileName: string = 'SHA256SUMS',
): string {
  const path = join(outDir, fileName);
  const carried: string[] = [];
  if (existsSync(path)) {
    for (const name of parseChecksums(readFileSync(path, 'utf8')).keys()) {
      if (archiveNames.includes(name)) continue;
      if (existsSync(join(outDir, name))) carried.push(name);
      else console.warn(`build-cli: ${name} is named in ${fileName} but no longer in ${outDir}; dropping it`);
    }
  }

  const text = [...archiveNames, ...carried]
    .sort()
    .map((name) => `${sha256File(join(outDir, name))}  ${name}\n`)
    .join('');
  writeFileSync(path, text);
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

/** Compile, package and verify one target at a time, verifying BEFORE
 *  moving on: a broken target then fails at the target that broke, not at
 *  a checksum three minutes later. */
export async function buildAll(args: BuildArgs, floors: Floors = RELEASE_FLOORS): Promise<string[]> {
  assertPackageVersion(args.version);
  const stray = args.mac ? [] : args.only.filter((id) => MAC_TARGETS.some((t) => t.id === id));
  if (stray.length) {
    throw new Error(`build-cli: ${stray.join(', ')} build only with --mac`);
  }
  // Before any compile: a missing notary key found after a 60 MB build is
  // a release that died late for a reason it could have named first.
  const creds = args.mac?.notarize ? notaryCredentials() : undefined;
  mkdirSync(args.outDir, { recursive: true });

  const targets = (args.mac ? MAC_TARGETS : TARGETS).filter((t) => args.only.includes(t.id));
  const names: string[] = [];
  for (const target of targets) {
    console.log(`── ${target.id} (${target.bunTarget})`);
    compileTarget(target, args.outDir);
    if (args.mac) {
      // Signed BEFORE packaging, so the tarball holds the signed bytes.
      if (args.mac.sign !== null) {
        signTarget(target, args.outDir, args.mac.sign);
        if (creds) notarizeTarget(target, args.outDir, creds);
      } else {
        console.warn(`   ${target.id}: UNSIGNED, as asked; Gatekeeper will refuse a downloaded copy`);
      }
    }
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
