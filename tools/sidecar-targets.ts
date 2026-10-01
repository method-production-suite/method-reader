// Which Rust target triple corresponds to each Bun compile target, and what
// the sidecar file must therefore be called.
//
// This is the one fact in piece C that a mistake hides rather than reveals.
// Tauri resolves a bundled external binary by name, with the Rust triple as
// a filename suffix; the rule was VERIFIED by running a real build and a
// real app on this machine (see desktop/README.md, "How Tauri finds the
// engine"), not read out of documentation. A wrong triple here compiles
// cleanly, bundles cleanly, and fails at runtime on someone else's computer.
//
// Two tables exist on purpose. tools/build-cli.ts's TARGETS is the cross
// RELEASE matrix and holds no darwin row, because a macOS artifact must
// come signed and notarized (build-cli.ts --mac, on a Mac). A sidecar is not
// a release artifact: it is an input to a build, so darwin belongs here.
// The same reasoning covers the two Linux musl rows below: nobody ships a
// musl release artifact, but a contributor's dev machine can be a musl
// host, so the sidecar table carries them and the release table does not.
// tests/sidecar-targets.test.ts pins that the release table is a subset of
// this one.
//
// darwin is Apple silicon ONLY. Intel Macs are not supported after the
// identifier release (owner, 2026-10-01; plan
// docs/superpowers/plans/2026-10-01-f2-handover-amendment.md): the window
// was universal through that release, so the frozen Swift updater could
// carry Intel installs across, and the release after it builds one slice.
// An x86_64-apple-darwin row here would build an engine no release ships
// and no runner tests.

import type { BinaryFormat } from './build-cli';

export type BunTarget =
  | 'bun-linux-x64'
  | 'bun-linux-x64-musl'
  | 'bun-linux-arm64'
  | 'bun-linux-arm64-musl'
  | 'bun-windows-x64'
  | 'bun-darwin-arm64';

export interface SidecarTarget {
  bunTarget: BunTarget;
  /** The suffix Tauri requires on the sidecar's filename. */
  rustTriple: string;
  /** What `bun build --compile` actually writes for this target. */
  exeSuffix: '' | '.exe';
  /** What the compiled binary must be. Checked, never assumed. */
  format: BinaryFormat;
}

/** The name the Rust passes to `sidecar()`. Every file below starts with it. */
export const SIDECAR_BASENAME = 'screepub-engine';

export const SIDECAR_TARGETS: readonly SidecarTarget[] = [
  {
    bunTarget: 'bun-linux-x64',
    rustTriple: 'x86_64-unknown-linux-gnu',
    exeSuffix: '',
    format: 'elf-x86-64',
  },
  {
    bunTarget: 'bun-linux-arm64',
    rustTriple: 'aarch64-unknown-linux-gnu',
    exeSuffix: '',
    format: 'elf-aarch64',
  },
  {
    bunTarget: 'bun-windows-x64',
    rustTriple: 'x86_64-pc-windows-msvc',
    exeSuffix: '.exe',
    format: 'pe-x86-64',
  },
  {
    bunTarget: 'bun-darwin-arm64',
    rustTriple: 'aarch64-apple-darwin',
    exeSuffix: '',
    format: 'macho-arm64',
  },
  {
    // Bun genuinely ships musl-linked Linux targets, verified on this
    // machine by compiling with each and confirming the ELF interpreter
    // with `file` (/lib/ld-musl-x86_64.so.1, not glibc's loader) — this is
    // not a glibc binary wearing a different name. Triple spelling checked
    // against `rustc --print target-list | grep musl`.
    bunTarget: 'bun-linux-x64-musl',
    rustTriple: 'x86_64-unknown-linux-musl',
    exeSuffix: '',
    format: 'elf-x86-64',
  },
  {
    bunTarget: 'bun-linux-arm64-musl',
    rustTriple: 'aarch64-unknown-linux-musl',
    exeSuffix: '',
    format: 'elf-aarch64',
  },
];

const KNOWN = SIDECAR_TARGETS.map((t) => t.bunTarget).join(', ');

export function sidecarTargetFor(bunTarget: string): SidecarTarget {
  const found = SIDECAR_TARGETS.find((t) => t.bunTarget === bunTarget);
  if (!found) {
    throw new Error(
      `build-sidecar: unknown bun target "${bunTarget}"; known targets are ${KNOWN}`,
    );
  }
  return found;
}

export function sidecarFileName(target: SidecarTarget): string {
  return `${SIDECAR_BASENAME}-${target.rustTriple}${target.exeSuffix}`;
}

/** Which (glibc-family) target this machine is, from platform/arch alone.
 *  Throws rather than guessing: a guess would produce a sidecar named for a
 *  machine it cannot run on. This cannot by itself tell a musl Linux host
 *  from a glibc one — process.platform/process.arch has no libc axis, both
 *  report 'linux'/'x64' — so hostSidecarTarget below asks the real
 *  toolchain and swaps to the musl sibling when that answer says musl. */
export function hostBunTarget(platform: string, arch: string): BunTarget {
  if (platform === 'linux' && arch === 'x64') return 'bun-linux-x64';
  if (platform === 'linux' && arch === 'arm64') return 'bun-linux-arm64';
  if (platform === 'win32' && arch === 'x64') return 'bun-windows-x64';
  if (platform === 'darwin' && arch === 'arm64') return 'bun-darwin-arm64';
  if (platform === 'darwin') {
    throw new Error(
      `build-sidecar: no sidecar target for ${platform}/${arch}. The Mac window is built for ` +
        'Apple silicon only; Intel Macs are not supported after the identifier release ' +
        '(docs/superpowers/plans/2026-10-01-f2-handover-amendment.md). To build the Apple ' +
        'silicon engine from here anyway, name it: --target bun-darwin-arm64.',
    );
  }
  throw new Error(
    `build-sidecar: no sidecar target for ${platform}/${arch}; known targets are ${KNOWN}`,
  );
}

/** A resolver for "what Rust triple is this machine", injected the same way
 *  build-cli.ts and build-sidecar.ts already inject `Spawn`: a real
 *  implementation by default, a fake one in tests. */
export type HostTriple = () => string;

/** Pull the triple out of `rustc -vV`'s `host:` line. Exported so a
 *  malformed capture can be tested without spawning a real process. */
export function parseRustcHost(output: string): string {
  const match = /^host:\s*(\S+)/m.exec(output);
  if (!match) {
    throw new Error(
      `build-sidecar: \`rustc -vV\` printed no "host:" line to read a triple from:\n${output.trim()}`,
    );
  }
  return match[1]!;
}

/** The real resolver: ask the toolchain that will actually run `cargo
 *  build` what ITS host is, instead of mapping process.platform/
 *  process.arch (see hostSidecarTarget for why). Anyone invoking --host is
 *  about to `cargo build`, so rustc is definitionally present — there is no
 *  silent fallback here for a missing or broken one, because a --host build
 *  that cannot ask rustc has no way left to get the one fact this file
 *  exists to get right. */
export function rustcHostTriple(): string {
  let proc: { exitCode: number | null; stdout: Uint8Array; stderr: Uint8Array };
  try {
    proc = Bun.spawnSync(['rustc', '-vV'], { stdout: 'pipe', stderr: 'pipe' });
  } catch (err) {
    throw new Error(
      `build-sidecar: could not run \`rustc -vV\` to resolve the host triple (${
        err instanceof Error ? err.message : String(err)
      })`,
    );
  }
  if ((proc.exitCode ?? 1) !== 0) {
    throw new Error(
      `build-sidecar: \`rustc -vV\` exited ${proc.exitCode} while resolving the host triple: ` +
        Buffer.from(proc.stderr).toString().trim(),
    );
  }
  return parseRustcHost(Buffer.from(proc.stdout).toString());
}

/** The musl-linked sibling of a glibc Linux BunTarget, or undefined for
 *  anything else (Windows and darwin have no libc axis to swap on). Bun
 *  really does ship these — bun-linux-x64-musl and bun-linux-arm64-musl —
 *  confirmed by compiling with each --target and checking the resulting
 *  ELF's interpreter with `file`; they are genuinely musl-linked binaries,
 *  not glibc ones under a different name. */
function muslSiblingOf(bunTarget: BunTarget): BunTarget | undefined {
  if (bunTarget === 'bun-linux-x64') return 'bun-linux-x64-musl';
  if (bunTarget === 'bun-linux-arm64') return 'bun-linux-arm64-musl';
  return undefined;
}

/** The SidecarTarget for THIS machine, named — and, on Linux, COMPILED —
 *  for what a same-machine `cargo build` will actually look for.
 *
 *  Two things can be wrong about a Linux host's default guess, and both
 *  come from the same root cause: process.platform/process.arch has no
 *  gnu/musl axis, so hostBunTarget alone cannot tell a musl host from a
 *  glibc one. Fixed here by asking the toolchain instead:
 *
 *   1. The RUST TRIPLE — the part Tauri uses to resolve the sidecar's
 *      filename — is asked of rustc, not read off the pinned
 *      SIDECAR_TARGETS row for the platform/arch guess.
 *   2. The BUN TARGET actually compiled — bun-linux-x64 vs.
 *      -x64-musl — is swapped to the musl sibling when that same rustc
 *      answer says musl, via muslSiblingOf.
 *
 *  Getting only #1 right and not #2 was fix round 1's gap: the sidecar
 *  would be NAMED correctly for a musl host but built from the pinned
 *  glibc row, which has no musl row to fall back to — so the build failed
 *  outright instead of shipping a binary that silently couldn't run.
 *  Getting neither right is the original failure Task 1 found: cargo
 *  build succeeds, the sidecar bundles under the wrong name, and the app
 *  fails at runtime with a bare "No such file or directory" naming no file
 *  at all.
 *
 *  Cross-compiled targets (--target / --all in build-sidecar.ts) keep the
 *  pinned table exactly, musl rows included: you cannot ask rustc about a
 *  triple it is not installed for, so there --target=bun-linux-x64-musl
 *  must be named explicitly. */
export function hostSidecarTarget(
  platform: string = process.platform,
  arch: string = process.arch,
  hostTriple: HostTriple = rustcHostTriple,
): SidecarTarget {
  // Throws on an unsupported host BEFORE the resolver ever runs.
  const glibcGuess = hostBunTarget(platform, arch);
  const rustTriple = hostTriple();
  const musl = muslSiblingOf(glibcGuess);
  const bunTarget = musl && rustTriple.includes('musl') ? musl : glibcGuess;
  const base = sidecarTargetFor(bunTarget);
  return { ...base, rustTriple };
}
