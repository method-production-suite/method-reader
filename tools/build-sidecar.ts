// Compile the Screepub engine and put it where the Tauri shell looks for it.
//
//   bun tools/build-sidecar.ts --host           # this machine, ~30s
//   bun tools/build-sidecar.ts --target bun-darwin-arm64   # the Mac release leg
//   bun tools/build-sidecar.ts --target bun-windows-x64
//   bun tools/build-sidecar.ts --all            # every target (~100 MB each)
//
// This is a build input, not a release artifact: it produces no archive, no
// checksum and nothing that reaches a user. Release artifacts, the signed
// macOS CLI included, are tools/build-cli.ts's job.
//
// The Mac engine is Apple silicon only. Until the release after
// 0.9.0 this file also lipo'd an x86_64 and an arm64 engine
// into one universal sidecar, because the frozen Swift updater installs
// the first .dmg on a release with no architecture check and the window
// had to open on Intel too. Intel Macs are not supported after that
// release (owner, 2026-10-01; plan
// docs/superpowers/plans/2026-10-01-f2-handover-amendment.md), so the lipo
// is gone rather than left unreached.
//
// The binary's NAME is the whole point. Tauri resolves a bundled external
// binary by name with the Rust target triple as a suffix — a rule verified
// by running a real build and a real app on this machine, recorded in
// desktop/README.md. Get it wrong and nothing fails until the window is
// open.
//
// Two different sources answer "what is the Rust triple?", for two
// different questions. For --target/--all, it is the PINNED constant in
// tools/sidecar-targets.ts's SIDECAR_TARGETS — which has Linux musl rows
// (bun-linux-x64-musl, bun-linux-arm64-musl) alongside the glibc ones: Bun
// really does compile musl-linked Linux binaries, this was verified on this
// machine, not assumed. Cross-compiling for a machine you are not on, there
// is no toolchain to ask, and the table is cross-pinned against
// build-cli.ts's release matrix so it cannot drift silently. For --host, it
// is asked of THIS machine's own rustc (hostSidecarTarget, in
// tools/sidecar-targets.ts) rather than mapped from process.platform/
// process.arch: Node's platform/arch enum has no gnu/musl axis (glibc and
// musl Linux both report 'linux'/'x64'), so a value derived from it can
// silently name the wrong libc even when the OS and CPU guesses are right.
// hostSidecarTarget uses that same rustc answer to pick the musl BunTarget
// too, not only the filename — naming a sidecar correctly but building it
// from the glibc row is still a build that fails on a musl host, just
// loudly instead of silently. Task 1 already showed what the fully silent
// version looks like: cargo build succeeds, the sidecar bundles under the
// wrong name, and the app fails at runtime with a bare "No such file or
// directory" naming no file. Anyone running --host is about to `cargo
// build` on this same machine, so asking its own rustc is both correct and
// always available.

import { existsSync, mkdirSync, statSync } from 'node:fs';
import { join, isAbsolute, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { REPO_DIR, readBinaryFormat, type Spawn } from './build-cli';
import {
  SIDECAR_TARGETS,
  sidecarTargetFor,
  sidecarFileName,
  hostBunTarget,
  hostSidecarTarget,
  rustcHostTriple,
  type BunTarget,
  type SidecarTarget,
  type HostTriple,
} from './sidecar-targets';

/** Exactly the directory tauri.conf.json's `externalBin` points at. */
export const BINARIES_DIR = join(REPO_DIR, 'desktop', 'src-tauri', 'binaries');

/** A compiled engine embeds the whole Bun runtime: 64-119 MB. Anything an
 *  order of magnitude under that is a truncated write or a stub, not a
 *  lean build. Same reasoning, same order of magnitude, as
 *  build-cli.ts's RELEASE_FLOORS.binaryBytes. */
export const SIDECAR_FLOOR_BYTES = 20_000_000;

const realSpawn: Spawn = (argv, cwd) => {
  const proc = Bun.spawnSync(argv, { cwd, stdout: 'inherit', stderr: 'pipe' });
  return { exitCode: proc.exitCode ?? 1, stderr: proc.stderr.toString() };
};

export interface SidecarArgs {
  targets: BunTarget[];
  outDir: string;
  /** Set only when targets came from --host. The caller should then build
   *  with hostSidecarTarget(..., hostTriple) instead of
   *  sidecarTargetFor(bunTarget), so the sidecar is named for what THIS
   *  machine's rustc actually reports rather than the pinned table's
   *  assumption. --target and --all leave this undefined and keep the
   *  pinned table exactly. */
  hostTriple?: HostTriple;
}

export function parseSidecarArgs(
  argv: string[],
  platform: string = process.platform,
  arch: string = process.arch,
  hostTriple: HostTriple = rustcHostTriple,
): SidecarArgs {
  const { values } = parseArgs({
    args: argv,
    options: {
      host: { type: 'boolean', default: false },
      all: { type: 'boolean', default: false },
      target: { type: 'string', multiple: true },
      out: { type: 'string' },
    },
    strict: true,
    allowPositionals: false,
  });

  const named = (values.target ?? [])
    .flatMap((s) => s.split(','))
    .map((s) => s.trim())
    .filter(Boolean);

  let targets: BunTarget[];
  let viaHost = false;
  if (values.all) {
    targets = SIDECAR_TARGETS.map((t) => t.bunTarget);
  } else if (named.length) {
    // sidecarTargetFor throws, by name, on anything unknown.
    targets = named.map((n) => sidecarTargetFor(n).bunTarget);
  } else if (values.host) {
    targets = [hostBunTarget(platform, arch)];
    viaHost = true;
  } else {
    throw new Error(
      'build-sidecar: say which targets — --host, --target <bun-target> or --all. ' +
        `There is no default: --all costs ${SIDECAR_TARGETS.length} cross-compiles and ` +
        `about ${SIDECAR_TARGETS.length * 100} MB, and --host would build the wrong ` +
        'machine’s binary in CI.',
    );
  }

  const outDir = values.out
    ? isAbsolute(values.out)
      ? values.out
      : resolve(values.out)
    : BINARIES_DIR;

  return viaHost ? { targets, outDir, hostTriple } : { targets, outDir };
}

/** Where this target's sidecar must sit, under the name Tauri asks for. */
export function sidecarPath(target: SidecarTarget, outDir: string): string {
  return join(outDir, sidecarFileName(target));
}

/** The one invocation, as data, so it is asserted element by element.
 *  `--outfile` never carries `.exe`: bun appends it for the windows target
 *  and passing it would produce `...msvc.exe.exe`, which Tauri would not
 *  find. */
export function compileSidecarArgv(target: SidecarTarget, outDir: string): string[] {
  const full = sidecarPath(target, outDir);
  const bare = full.slice(0, full.length - target.exeSuffix.length);
  return [
    'bun',
    'build',
    '--compile',
    `--target=${target.bunTarget}`,
    'src/cli.ts',
    `--outfile=${bare}`,
  ];
}

/** Returns the path actually written. Checks that it exists rather than
 *  trusting the exit code: `bun build` can exit 0 and leave nothing where
 *  we expected it, and the only later symptom is a window that cannot find
 *  its engine. */
export function buildSidecar(
  target: SidecarTarget,
  outDir: string,
  spawn: Spawn = realSpawn,
  repoDir: string = REPO_DIR,
): string {
  mkdirSync(outDir, { recursive: true });
  const { exitCode, stderr } = spawn(compileSidecarArgv(target, outDir), repoDir);
  if (exitCode !== 0) {
    throw new Error(
      `build-sidecar: ${target.bunTarget} failed to compile (exit ${exitCode})\n${stderr.trim()}`,
    );
  }
  const path = sidecarPath(target, outDir);
  if (!existsSync(path)) {
    throw new Error(
      `build-sidecar: ${target.bunTarget} compiled but wrote nothing at ${path}. ` +
        `Tauri looks for exactly ${sidecarFileName(target)}; see desktop/README.md.`,
    );
  }
  return path;
}

/** Check what was PRODUCED. A mis-typed --target= exits 0 while producing a
 *  perfectly valid binary for the wrong machine, and a file-exists check
 *  cannot tell the difference. */
export function verifySidecar(
  target: SidecarTarget,
  outDir: string,
  floor: number = SIDECAR_FLOOR_BYTES,
): void {
  const path = sidecarPath(target, outDir);
  if (!existsSync(path)) {
    throw new Error(`build-sidecar: no sidecar at ${path} (expected ${sidecarFileName(target)})`);
  }
  const bytes = statSync(path).size;
  if (bytes < floor) {
    throw new Error(
      `build-sidecar: ${sidecarFileName(target)} is ${bytes} bytes, under the ${floor}-byte floor`,
    );
  }
  const format = readBinaryFormat(path);
  if (format !== target.format) {
    throw new Error(
      `build-sidecar: ${sidecarFileName(target)} is ${format}, expected ${target.format}`,
    );
  }
}

// LAST in the file on purpose: this runs at import time, so every const and
// function it reaches must already be initialised. (A section that once sat
// below it died with "Cannot access ... before initialization" the first
// time it ran.)
if (import.meta.main) {
  try {
    const { targets, outDir, hostTriple } = parseSidecarArgs(process.argv.slice(2));
    for (const bunTarget of targets) {
      // --host: resolve against THIS machine's rustc — which can swap in
      // a musl bunTarget the pre-parsed `targets` array above never knew
      // about, so the log below reads target.bunTarget, not the loop
      // variable. --target/--all: name it from the pinned table
      // (hostTriple is undefined, so `target` matches `bunTarget`).
      const target = hostTriple
        ? hostSidecarTarget(process.platform, process.arch, hostTriple)
        : sidecarTargetFor(bunTarget);
      console.log(`building ${target.bunTarget} → ${sidecarFileName(target)}`);
      buildSidecar(target, outDir);
      verifySidecar(target, outDir);
      console.log(`  ok: ${sidecarPath(target, outDir)}`);
    }
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  }
}
