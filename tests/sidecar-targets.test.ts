import { describe, test, expect } from 'bun:test';
import {
  SIDECAR_TARGETS,
  SIDECAR_BASENAME,
  sidecarFileName,
  sidecarTargetFor,
  hostBunTarget,
  hostSidecarTarget,
  parseRustcHost,
} from '../tools/sidecar-targets';
import { TARGETS } from '../tools/build-cli';

describe('the triple map', () => {
  test('every Bun desktop target maps to exactly one Rust triple', () => {
    // Pinned whole, not sampled. These strings are not decorative: Tauri
    // resolves the sidecar by this exact filename suffix (Task 1), so a
    // typo here produces a window that cannot find its engine rather than
    // anything that fails to build.
    expect(
      Object.fromEntries(SIDECAR_TARGETS.map((t) => [t.bunTarget, t.rustTriple])),
    ).toEqual({
      'bun-linux-x64': 'x86_64-unknown-linux-gnu',
      'bun-linux-arm64': 'aarch64-unknown-linux-gnu',
      'bun-windows-x64': 'x86_64-pc-windows-msvc',
      // Apple silicon only: Intel Macs are not supported after the
      // identifier release (plan 2026-10-01-f2-handover-amendment.md), so
      // there is no x86_64-apple-darwin engine to name.
      'bun-darwin-arm64': 'aarch64-apple-darwin',
      // Bun genuinely ships musl-linked Linux targets (verified on this
      // machine with `file`, not assumed); triples checked against
      // `rustc --print target-list | grep musl`.
      'bun-linux-x64-musl': 'x86_64-unknown-linux-musl',
      'bun-linux-arm64-musl': 'aarch64-unknown-linux-musl',
    });
  });

  test('no two targets share a triple, and no triple is empty', () => {
    // A copy-paste that left two rows with the same triple would overwrite
    // one sidecar with the other's binary — same filename, wrong machine,
    // and it would run fine on the developer's own box.
    const triples = SIDECAR_TARGETS.map((t) => t.rustTriple);
    expect(new Set(triples).size).toBe(SIDECAR_TARGETS.length);
    for (const t of triples) expect(t.length).toBeGreaterThan(0);
  });

  test('each target names the binary format its output must have', () => {
    expect(
      Object.fromEntries(SIDECAR_TARGETS.map((t) => [t.bunTarget, t.format])),
    ).toEqual({
      'bun-linux-x64': 'elf-x86-64',
      'bun-linux-arm64': 'elf-aarch64',
      'bun-windows-x64': 'pe-x86-64',
      'bun-darwin-arm64': 'macho-arm64',
      // Same ELF machine type as their glibc siblings: libc doesn't change
      // the CPU architecture the header records.
      'bun-linux-x64-musl': 'elf-x86-64',
      'bun-linux-arm64-musl': 'elf-aarch64',
    });
  });

  test('only the Windows target carries .exe', () => {
    for (const t of SIDECAR_TARGETS) {
      expect(t.exeSuffix).toBe(t.bunTarget === 'bun-windows-x64' ? '.exe' : '');
    }
  });
});

describe('sidecarTargetFor', () => {
  test('answers for every known target', () => {
    for (const t of SIDECAR_TARGETS) {
      expect(sidecarTargetFor(t.bunTarget).rustTriple).toBe(t.rustTriple);
    }
  });

  test('an unknown target throws, and never falls back to a default', () => {
    // The failure this exists to prevent: a silent default would build the
    // host's binary, name it for a machine it cannot run on, and the
    // mistake would only surface on someone else's computer.
    for (const bad of ['bun-linux-riscv64', 'bun-darwin-arm', 'linux-x64', '', 'bun-windows-arm64']) {
      expect(() => sidecarTargetFor(bad)).toThrow(/unknown bun target/i);
    }
  });

  test('the error names the target it was given and the ones it knows', () => {
    expect(() => sidecarTargetFor('bun-linux-riscv64')).toThrow(/bun-linux-riscv64/);
    expect(() => sidecarTargetFor('bun-linux-riscv64')).toThrow(/bun-linux-x64/);
  });
});

describe('sidecarFileName', () => {
  test('is the basename, a hyphen, the triple, and the platform suffix', () => {
    // The single most important string in this piece. Task 1 observed the
    // rule on a real build; this pins the observation.
    const names = SIDECAR_TARGETS.map(sidecarFileName);
    expect(names).toEqual([
      'screepub-engine-x86_64-unknown-linux-gnu',
      'screepub-engine-aarch64-unknown-linux-gnu',
      'screepub-engine-x86_64-pc-windows-msvc.exe',
      'screepub-engine-aarch64-apple-darwin',
      'screepub-engine-x86_64-unknown-linux-musl',
      'screepub-engine-aarch64-unknown-linux-musl',
    ]);
  });

  test('every name begins with the basename the Rust asks for', () => {
    // desktop/src-tauri/src/sidecar.rs passes SIDECAR_BASENAME to
    // sidecar(); a name that did not start with it would never resolve.
    for (const t of SIDECAR_TARGETS) {
      expect(sidecarFileName(t).startsWith(`${SIDECAR_BASENAME}-`)).toBe(true);
    }
  });

  test('no name contains a path separator', () => {
    for (const t of SIDECAR_TARGETS) {
      expect(sidecarFileName(t)).not.toContain('/');
      expect(sidecarFileName(t)).not.toContain('\\');
    }
  });
});

describe('hostBunTarget', () => {
  test('maps the platform/arch pairs a developer or runner can be on', () => {
    expect(hostBunTarget('linux', 'x64')).toBe('bun-linux-x64');
    expect(hostBunTarget('linux', 'arm64')).toBe('bun-linux-arm64');
    expect(hostBunTarget('win32', 'x64')).toBe('bun-windows-x64');
    expect(hostBunTarget('darwin', 'arm64')).toBe('bun-darwin-arm64');
  });

  test('an Intel Mac is not a host, and the error says why', () => {
    expect(() => hostBunTarget('darwin', 'x64')).toThrow(/no sidecar target/i);
    expect(() => hostBunTarget('darwin', 'x64')).toThrow(/Apple silicon/);
  });

  test('an unsupported host throws rather than guessing', () => {
    expect(() => hostBunTarget('linux', 'ia32')).toThrow(/no sidecar target/i);
    expect(() => hostBunTarget('freebsd', 'x64')).toThrow(/no sidecar target/i);
    expect(() => hostBunTarget('win32', 'arm64')).toThrow(/no sidecar target/i);
  });

  test('this machine is a supported host', () => {
    const t = hostBunTarget(process.platform, process.arch);
    expect(SIDECAR_TARGETS.map((x) => x.bunTarget)).toContain(t);
  });
});

describe('hostSidecarTarget', () => {
  test('a musl resolver selects BOTH the musl Bun target and the musl-suffixed filename', () => {
    // The finding from fix round 1: --host must ask the toolchain, because
    // Node's platform/arch cannot tell a glibc host from a musl one. Fix
    // round 2's completion: getting only the filename right and still
    // building from the glibc row is still broken — just loudly, since
    // SIDECAR_TARGETS has no musl row to build from otherwise. Both halves
    // have to come from the same resolved answer.
    const musl = () => 'x86_64-unknown-linux-musl';
    const target = hostSidecarTarget('linux', 'x64', musl);
    expect(target.bunTarget).toBe('bun-linux-x64-musl');
    expect(target.rustTriple).toBe('x86_64-unknown-linux-musl');
    expect(sidecarFileName(target)).toBe('screepub-engine-x86_64-unknown-linux-musl');
    expect(target.exeSuffix).toBe('');
    expect(target.format).toBe('elf-x86-64');
  });

  test('the same swap happens on arm64', () => {
    const musl = () => 'aarch64-unknown-linux-musl';
    const target = hostSidecarTarget('linux', 'arm64', musl);
    expect(target.bunTarget).toBe('bun-linux-arm64-musl');
    expect(sidecarFileName(target)).toBe('screepub-engine-aarch64-unknown-linux-musl');
    expect(target.format).toBe('elf-aarch64');
  });

  test('a gnu (glibc) resolver keeps the plain Bun target — no unconditional swap', () => {
    const gnu = () => 'x86_64-unknown-linux-gnu';
    const target = hostSidecarTarget('linux', 'x64', gnu);
    expect(target.bunTarget).toBe('bun-linux-x64');
  });

  test('the musl swap is Linux-only: a darwin resolver has no musl sibling to swap to', () => {
    // musl is a Linux libc distinction; muslSiblingOf is undefined for
    // darwin/windows targets, so an odd resolver answer there just names
    // the file — it cannot select a target that doesn't exist.
    const weird = () => 'aarch64-apple-darwin';
    const target = hostSidecarTarget('darwin', 'arm64', weird);
    expect(target.bunTarget).toBe('bun-darwin-arm64');
  });

  test('a resolver that fails (rustc missing or broken) propagates, never falls back to a guess', () => {
    const broken = (): string => {
      throw new Error('rustc: command not found');
    };
    expect(() => hostSidecarTarget('linux', 'x64', broken)).toThrow(/rustc: command not found/);
  });

  test('an unsupported host throws before the resolver is even asked', () => {
    const neverCalled = (): string => {
      throw new Error('should not have been called');
    };
    expect(() => hostSidecarTarget('freebsd', 'x64', neverCalled)).toThrow(/no sidecar target/i);
  });

  test('on this machine, the real resolver agrees with an independent call to rustc', () => {
    // Proof, not a mock: ask rustc separately from the implementation and
    // require agreement, rather than re-running the same regex on itself.
    const proc = Bun.spawnSync(['rustc', '-vV'], { stdout: 'pipe', stderr: 'pipe' });
    if ((proc.exitCode ?? 1) !== 0) return; // no rustc on this runner; nothing to compare
    const expected = /^host:\s*(\S+)/m.exec(proc.stdout.toString())?.[1];
    expect(expected).toBeDefined();
    expect(hostSidecarTarget().rustTriple).toBe(expected as string);
    // Two real rustc runs, on purpose: sharing one answer would make the
    // check agree with itself. On a warm machine they take well under a
    // second, but on a cold CI runner the first rustc (through the rustup
    // proxy) has pushed the pair past bun's default 5 s: the engine job
    // failed exactly that way on 2026-09-24 and passed on a rerun. A minute
    // absorbs a cold start and still fails a rustc that has actually hung.
  }, 60_000);
});

describe('parseRustcHost', () => {
  test('reads the triple that follows "host:"', () => {
    expect(
      parseRustcHost('rustc 1.98.1 (48a229cea)\nhost: aarch64-unknown-linux-gnu\nrelease: 1.98.1\n'),
    ).toBe('aarch64-unknown-linux-gnu');
  });

  test('rejects output with no host line, rather than guessing', () => {
    expect(() => parseRustcHost('rustc 1.98.1\nrelease: 1.98.1\n')).toThrow(/host/i);
  });

  test('rejects empty output', () => {
    expect(() => parseRustcHost('')).toThrow(/host/i);
  });
});

describe('the two target tables agree', () => {
  test('every release target in build-cli.ts has a sidecar triple', () => {
    // The drift this catches: someone adds a target to the release matrix
    // and the desktop app silently stops shipping an engine for it. The
    // reverse is allowed — darwin is a sidecar target and not a CROSS
    // release target, deliberately, because signed macOS artifacts are
    // built on a Mac by build-cli.ts --mac.
    const sidecarBunTargets = new Set<string>(SIDECAR_TARGETS.map((t) => t.bunTarget));
    for (const t of TARGETS) {
      expect(`${t.id} has a sidecar triple: ${sidecarBunTargets.has(t.bunTarget)}`).toBe(
        `${t.id} has a sidecar triple: true`,
      );
    }
  });

  test('where both tables name a format, they name the same one', () => {
    for (const t of TARGETS) {
      expect(sidecarTargetFor(t.bunTarget).format).toBe(t.format);
    }
  });
});
