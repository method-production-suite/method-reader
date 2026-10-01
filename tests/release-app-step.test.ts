// release-artifacts.test.ts proves the app-bundles and app-upload steps are
// TEXTUALLY present, needed by the right jobs, and in the right order. That
// is necessary but not sufficient: `toContain('smoke-bundle.ts')` would
// also pass if the loop globbed a directory nothing writes to, if the
// Mac leg quietly built an engine for whatever runner it landed on, or if
// the checksums step accepted five bundles where four were built.
//
// A workflow cannot be run by `bun test` -- it needs a GitHub runner -- but
// these four `run:` blocks are just bash, so this file pulls the ACTUAL
// step text out of the committed release.yml and executes it against a
// scratch directory with stub executables on PATH. Every `${{ }}` in these
// steps sits in `env:`, not in the script, so the text executed here is
// byte-for-byte the text a runner executes.
//
// No cargo is ever spawned: the stub directory comes FIRST on PATH and
// holds a recorder named `bun`, so the step's own command line is captured
// rather than run. HOME points into the scratch tree, so nothing touches a
// real home directory.
import { describe, test, expect, afterAll } from 'bun:test';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

const WORK = mkdtempSync(join(tmpdir(), 'screepub-release-step-'));
// Resolved from THIS process's PATH before the scripts below replace it.
const BASH = Bun.which('bash');
if (!BASH) throw new Error('release-app-step: no bash on PATH');
// The real coreutils, for `rm`, `sha256sum`, `wc` and `cat`. Stubs are
// prepended, so a stubbed name still wins over anything found here.
const COREUTILS = dirname(Bun.which('sha256sum') ?? '/usr/bin/sha256sum');
afterAll(() => rmSync(WORK, { recursive: true, force: true }));

interface Step {
  name?: string;
  run?: string;
  shell?: string;
}
interface Workflow {
  jobs: Record<string, { steps?: Step[] }>;
}

const YML = join('.github', 'workflows', 'release.yml');
const release = Bun.YAML.parse(readFileSync(YML, 'utf8')) as Workflow;

function stepNamed(job: string, name: string): string {
  const found = (release.jobs[job]?.steps ?? []).find((s) => s.name === name);
  if (!found?.run) {
    throw new Error(`release.yml: ${job} has no step named ${JSON.stringify(name)} with a run block`);
  }
  return found.run;
}

const SIDECAR_STEP = stepNamed('app-bundles', 'Build the engine sidecar');
const BUILD_STEP = stepNamed('app-bundles', "Build and verify this platform's bundles");
const NOTARIZE_STEP = stepNamed('app-bundles', 'Notarize and staple the disk image');
const SMOKE_STEP = stepNamed('app-bundles', 'Run the engine out of each bundle');
const SUMS_STEP = stepNamed('app-upload', 'Rebuild the checksums from what arrived, then verify them');
const MANIFEST_STEP = stepNamed('app-upload', 'Build the update manifest from what arrived, and publish it last');

/** A recorder script standing in for a real executable. It APPENDS one
 *  tab-joined line per invocation, so a loop's repeated calls are all
 *  visible rather than only the last. */
function stubDir(tag: string, names: string[]): { dir: string; lines: (n: string) => string[] } {
  const dir = mkdtempSync(join(WORK, `${tag}-`));
  const logPath = (n: string) => join(dir, `${n}.argv`);
  for (const n of names) {
    const script = join(dir, n);
    writeFileSync(
      script,
      `#!/bin/sh\nprintf '%s\\t' "$@" >> ${JSON.stringify(logPath(n))}\nprintf '\\n' >> ${JSON.stringify(logPath(n))}\nexit 0\n`,
    );
    chmodSync(script, 0o755);
  }
  return {
    dir,
    lines: (n) => {
      try {
        return readFileSync(logPath(n), 'utf8')
          .split('\n')
          .filter(Boolean)
          .map((l) => l.replace(/\t$/, ''));
      } catch {
        return [];
      }
    },
  };
}

const runStep = (script: string, dir: string, env: Record<string, string>) =>
  spawnSync(BASH, ['-c', script], {
    cwd: dir,
    env: { PATH: `${dir}:${COREUTILS}:/usr/bin:/bin`, HOME: dir, ...env },
    encoding: 'utf8',
  });

/** What GitHub sets RUNNER_OS to on each runner label the matrix uses. */
const runnerOs = (label: string): string =>
  label.startsWith('macos') ? 'macOS' : label.startsWith('windows') ? 'Windows' : 'Linux';

describe("release.yml's sidecar step, executed as bash", () => {
  test('the Mac leg builds the Apple silicon engine by name, never for the host', () => {
    // One slice since the release after the identifier release, and named
    // from the pinned table rather than asked of the runner: an Intel
    // runner building "for the host" would put an x86_64 engine inside the
    // one Mac image. build-app-bundle.ts names the matching cargo triple.
    const { dir, lines } = stubDir('sidecar-mac', ['bun']);
    const r = runStep(SIDECAR_STEP, dir, { RUNNER_OS: 'macOS', ARCH: 'arm64' });
    expect(r.status).toBe(0);
    expect(lines('bun')).toEqual(['tools/build-sidecar.ts\t--target\tbun-darwin-arm64']);
  });

  test('the Linux and Windows legs build for the runner they are on', () => {
    // --host, never a guessed triple: the Linux and Windows legs bundle for
    // themselves, and build-sidecar.ts asks the runner's own rustc.
    for (const os of ['Linux', 'Windows']) {
      const { dir, lines } = stubDir(`sidecar-host-${os}`, ['bun']);
      const r = runStep(SIDECAR_STEP, dir, { RUNNER_OS: os, ARCH: 'x64' });
      expect(r.status).toBe(0);
      expect(lines('bun')).toEqual(['tools/build-sidecar.ts\t--host']);
    }
  });

  test('a shape this step does not build fails loudly instead of defaulting', () => {
    // The universal Mac row and its lipo are gone (Intel Macs are not
    // supported after the identifier release), and so is any Intel Mac
    // shape. What replaced them must not be a silent `else --host`: a leg
    // that quietly built for the runner is how a foreign engine gets
    // inside a download with nothing failing until a window opens.
    for (const [os, arch] of [['macOS', 'universal'], ['macOS', 'x64'], ['Linux', 'arm64']]) {
      const { dir, lines } = stubDir(`sidecar-bogus-${os}-${arch}`, ['bun']);
      const r = runStep(SIDECAR_STEP, dir, { RUNNER_OS: os!, ARCH: arch! });
      expect(r.status).not.toBe(0);
      expect(r.stdout + r.stderr).toContain(`${os}/${arch}`);
      expect(lines('bun')).toEqual([]);
    }
  });

  test('every matrix row names a shape this step actually handles', () => {
    // The pin the old sidecar<->triple cross-check used to provide. The
    // cargo triple is not in the YAML at all: build-app-bundle.ts derives
    // aarch64-apple-darwin for every Mac build, so it lives in one place.
    const rows =
      ((Bun.YAML.parse(readFileSync(YML, 'utf8')) as unknown as {
        jobs: Record<string, { strategy?: { matrix?: { include?: Record<string, string>[] } } }>;
      }).jobs['app-bundles']?.strategy?.matrix?.include) ?? [];
    expect(rows.length).toBe(3);
    for (const row of rows) {
      const { dir } = stubDir(`sidecar-row-${row.os}`, ['bun']);
      expect(
        runStep(SIDECAR_STEP, dir, { RUNNER_OS: runnerOs(String(row.os)), ARCH: String(row.arch) }).status,
      ).toBe(0);
    }
  });
});

describe("release.yml's bundle step, executed as bash", () => {
  test('the Linux and Windows legs pass no arch and no overlay', () => {
    const { dir, lines } = stubDir('build-host', ['bun']);
    const r = runStep(BUILD_STEP, dir, { TAG: 'v0.6.0', RUNNER_OS: 'Linux', ARCH: 'x64' });
    expect(r.status).toBe(0);
    expect(lines('bun')).toEqual([
      'tools/build-app-bundle.ts\t--version\t0.6.0\t--out\tbundles',
    ]);
    // No --updater off macOS: nothing signs the Linux and Windows
    // artifacts yet, and the manifest builder refuses a signature it has
    // no row for, so this is where that stays deliberate.
    expect(lines('bun')[0]).not.toContain('--updater');
  });

  test('the macOS leg asks for an Apple silicon bundle, the updater, and no overlay', () => {
    // --arch arm64 and no --target. build-app-bundle.ts derives
    // aarch64-apple-darwin for every Mac build and refuses any other Mac
    // arch or triple, so the YAML cannot name an Intel one.
    const { dir, lines } = stubDir('build-mac', ['bun']);
    const r = runStep(BUILD_STEP, dir, { TAG: 'v0.6.0', RUNNER_OS: 'macOS', ARCH: 'arm64' });
    expect(r.status).toBe(0);
    // --updater too: the macOS leg is the one that produces the updater
    // archive and its signature, and the tool refuses to start without
    // TAURI_SIGNING_PRIVATE_KEY, so a release without the secret fails in
    // seconds rather than after a full build.
    // No --config: the transition overlay was deleted at the identifier
    // release, when the window became the Mac app under the same name.
    expect(lines('bun')).toEqual([
      'tools/build-app-bundle.ts\t--version\t0.6.0\t--out\tbundles\t--arch\tarm64\t--updater',
    ]);
    expect(lines('bun')[0]).not.toContain('--target');
  });

  test('the version is the TAG with its v stripped, prerelease suffix and all', () => {
    const { dir, lines } = stubDir('build-pre', ['bun']);
    expect(runStep(BUILD_STEP, dir, { TAG: 'v0.6.0-rc1', RUNNER_OS: 'Linux', ARCH: 'x64' }).status).toBe(0);
    expect(lines('bun')[0]).toContain('0.6.0-rc1');
    expect(lines('bun')[0]).not.toContain('v0.6.0');
  });
});

describe("release.yml's notarization step, executed as bash", () => {
  // The step that did not exist when v0.6.0 shipped, which is why v0.6.0
  // published an image Gatekeeper rejects. tauri-bundler notarizes the
  // .app and never the container: app.rs submits the bundle right after
  // signing it, and dmg/mod.rs only signs. So the ticket ended up
  // stapled to the app inside an image carrying none.
  const withDmgs = (tag: string, names: string[]) => {
    const { dir, lines } = stubDir(`notarize-${tag}`, ['xcrun', 'spctl']);
    mkdirSync(join(dir, 'bundles'), { recursive: true });
    for (const n of names) writeFileSync(join(dir, 'bundles', n), 'x');
    return { dir, lines };
  };
  const env = {
    APPLE_API_KEY_PATH: '/tmp/ac.p8',
    APPLE_API_KEY: 'KEYID',
    APPLE_API_ISSUER: 'ISSUER',
  };

  test('submit, staple, then VALIDATE, in that order, on the image', () => {
    // The validate is not decoration. `stapler staple` can exit 0 on
    // paths that leave no ticket, and an image without one is the exact
    // defect this step exists to prevent, so the result is asserted
    // rather than trusted. The Swift app's release script did the same
    // three, plus the spctl, and it was the reference.
    const { dir, lines } = withDmgs('one', ['Screepub-macOS.dmg']);
    const r = runStep(NOTARIZE_STEP, dir, env);
    expect(r.status).toBe(0);
    expect(lines('xcrun')).toEqual([
      'notarytool\tsubmit\tbundles/Screepub-macOS.dmg\t--key\t/tmp/ac.p8' +
        '\t--key-id\tKEYID\t--issuer\tISSUER\t--wait',
      'stapler\tstaple\tbundles/Screepub-macOS.dmg',
      'stapler\tvalidate\tbundles/Screepub-macOS.dmg',
    ]);
    // And Gatekeeper's own opinion, which is the thing a person meets.
    expect(lines('spctl')).toEqual([
      '-a\t-t\topen\t--context\tcontext:primary-signature\t-v\t' +
        'bundles/Screepub-macOS.dmg',
    ]);
  });

  test('no .dmg at all fails loudly rather than exiting 0 having notarized nothing', () => {
    // Same property the smoke step defends: a glob that matched nothing
    // must not read as success. If the bundle step ever stops producing
    // a DMG, this is where it is noticed.
    const { dir, lines } = withDmgs('none', ['Screepub_0.6.0_amd64.deb']);
    const r = runStep(NOTARIZE_STEP, dir, env);
    expect(r.status).not.toBe(0);
    expect(r.stdout + r.stderr).toContain('NOTHING WAS NOTARIZED');
    expect(lines('xcrun')).toEqual([]);
  });

  test('a failing notarytool stops the step instead of stapling nothing', () => {
    const { dir } = withDmgs('submitfail', ['Screepub-macOS.dmg']);
    writeFileSync(join(dir, 'xcrun'), '#!/bin/sh\nexit 1\n');
    chmodSync(join(dir, 'xcrun'), 0o755);
    const r = runStep(NOTARIZE_STEP, dir, env);
    expect(r.status).not.toBe(0);
  });

  test('a failing stapler validate fails the release', () => {
    // The case that matters most: submit and staple both "worked" and
    // the image still has no ticket. Exactly what shipped in v0.6.0,
    // arriving one step earlier.
    const { dir } = withDmgs('validatefail', ['Screepub-macOS.dmg']);
    writeFileSync(
      join(dir, 'xcrun'),
      '#!/bin/sh\ncase "$2" in validate) exit 1 ;; *) exit 0 ;; esac\n',
    );
    chmodSync(join(dir, 'xcrun'), 0o755);
    const r = runStep(NOTARIZE_STEP, dir, env);
    expect(r.status).not.toBe(0);
  });
});

describe("release.yml's smoke step, executed as bash", () => {
  const withBundles = (tag: string, names: string[]) => {
    const { dir, lines } = stubDir(`smoke-${tag}`, ['bun']);
    mkdirSync(join(dir, 'bundles'), { recursive: true });
    for (const n of names) writeFileSync(join(dir, 'bundles', n), 'x');
    return { dir, lines };
  };

  test('every bundle in the directory is opened, one smoke-bundle call each', () => {
    const { dir, lines } = withBundles('linux', [
      'Screepub_0.6.0_amd64.deb',
      'Screepub-0.6.0-1.x86_64.rpm',
      'SHA256SUMS-app',
    ]);
    const r = runStep(SMOKE_STEP, dir, { TAG: 'v0.6.0', ARCH: 'x64' });
    expect(r.status).toBe(0);
    expect(lines('bun')).toEqual([
      'tools/smoke-bundle.ts\t--bundle\tbundles/Screepub_0.6.0_amd64.deb\t--expect-version\t0.6.0',
      'tools/smoke-bundle.ts\t--bundle\tbundles/Screepub-0.6.0-1.x86_64.rpm\t--expect-version\t0.6.0',
    ]);
    // The checksums file beside them is not a bundle and was not smoked.
    expect(r.stdout).toContain('2 bundle(s)');
  });

  test('an empty bundles/ fails loudly instead of exiting 0 having checked nothing', () => {
    // The property smoke-bundle.ts defends one level down -- a check that
    // prints nothing and exits 0 is indistinguishable from a check that
    // ran. A glob matching no file must not silently satisfy this step.
    const { dir, lines } = withBundles('empty', []);
    const r = runStep(SMOKE_STEP, dir, { TAG: 'v0.6.0', ARCH: 'x64' });
    expect(r.status).not.toBe(0);
    expect(r.stdout + r.stderr).toContain('NOTHING WAS CHECKED');
    expect(lines('bun')).toEqual([]);
  });

  test('the Mac DMG is smoked like every other bundle', () => {
    // The Apple silicon image is one slice, the runner's own, so the bundle
    // that ships is one the release has opened and run in full.
    const { dir, lines } = withBundles('mac', ['Screepub-macOS.dmg']);
    const r = runStep(SMOKE_STEP, dir, { TAG: 'v0.6.0', RUNNER_OS: 'macOS', ARCH: 'arm64' });
    expect(r.status).toBe(0);
    expect(lines('bun')).toEqual([
      'tools/smoke-bundle.ts\t--bundle\tbundles/Screepub-macOS.dmg' +
        '\t--expect-version\t0.6.0',
    ]);
    expect(r.stdout).toContain('1 bundle(s)');
  });

  test('no leg prints an untested-slice notice, the Mac one included', () => {
    // The universal image used to carry an Intel slice no runner executed,
    // and the step said so. With Intel dropped there is no such slice, and
    // a notice about one would be noise that teaches readers to ignore
    // notices.
    for (const [os, arch, file] of [
      ['macOS', 'arm64', 'Screepub-macOS.dmg'],
      ['Linux', 'x64', 'Screepub_0.6.0_amd64.deb'],
    ]) {
      const { dir } = withBundles(`notice-${os}`, [file!]);
      const r = runStep(SMOKE_STEP, dir, { TAG: 'v0.6.0', RUNNER_OS: os!, ARCH: arch! });
      expect(r.status).toBe(0);
      expect(r.stdout).not.toContain('::notice::');
    }
  });

  test('a failing smoke-bundle stops the step rather than being swallowed by the loop', () => {
    const { dir } = withBundles('fail', ['Screepub_0.6.0_amd64.deb']);
    writeFileSync(join(dir, 'bun'), '#!/bin/sh\nexit 3\n');
    chmodSync(join(dir, 'bun'), 0o755);
    const r = runStep(SMOKE_STEP, dir, { TAG: 'v0.6.0', ARCH: 'x64' });
    expect(r.status).not.toBe(0);
  });
});

describe("release.yml's checksum step, executed as bash", () => {
  // `arrivals`, since 2026-09-21: app-upload now checks the repository
  // out, and the old download directory name is the Swift app's source
  // directory in a checkout.
  const withArrivals = (tag: string, names: string[]) => {
    const dir = mkdtempSync(join(WORK, `sums-${tag}-`));
    mkdirSync(join(dir, 'arrivals'), { recursive: true });
    for (const n of names) writeFileSync(join(dir, 'arrivals', n), `contents of ${n}`);
    return dir;
  };

  const FOUR = [
    'Screepub_0.6.0_amd64.deb',
    'Screepub-0.6.0-1.x86_64.rpm',
    'Screepub-macOS.dmg',
    'Screepub-0.6.0-setup.exe',
  ];

  test('it rebuilds SHA256SUMS-app over everything that arrived, and verifies it', () => {
    // Including over a stale per-leg file: each leg uploaded its own,
    // covering only its own bundles, and merge-multiple keeps one of them.
    const dir = withArrivals('ok', [...FOUR, 'SHA256SUMS-app']);
    const r = runStep(SUMS_STEP, dir, {});
    expect(r.status).toBe(0);
    const sums = readFileSync(join(dir, 'arrivals', 'SHA256SUMS-app'), 'utf8');
    for (const n of FOUR) expect(sums).toContain(n);
    // Bare filenames, so anyone downloading into their own directory can
    // run `sha256sum -c` against it.
    expect(sums).not.toContain('/');
    expect(sums.trim().split('\n').length).toBe(4);
  });

  test('a leg that failed to upload fails this step instead of publishing a short list', () => {
    // fail-fast is false, so two green legs and one red one is a real
    // shape. Without the count, the three remaining files would hash
    // cleanly and verify perfectly against themselves.
    const dir = withArrivals('short', FOUR.filter((n) => !n.endsWith('.exe')));
    const r = runStep(SUMS_STEP, dir, {});
    expect(r.status).not.toBe(0);
    // An extension nothing matched leaves the glob literal, so sha256sum
    // itself refuses before the count is ever reached.
    expect(r.stdout + r.stderr).toContain('.exe');
  });

  test('a SECOND .dmg arriving fails the step, though every extension is present', () => {
    // The count's real job now that macOS ships one bundle instead of two.
    // Every glob matches and sha256sum succeeds, so nothing else here
    // objects -- but the frozen Swift updater takes the FIRST .dmg asset on
    // a release and has no way to choose. Two DMGs on one release page is
    // the ambiguity this whole handover exists to remove, so it must not
    // be able to reach the page.
    const dir = withArrivals('twodmg', [...FOUR, 'Screepub-macOS-x64.dmg']);
    const r = runStep(SUMS_STEP, dir, {});
    expect(r.status).not.toBe(0);
    expect(r.stdout + r.stderr).toContain('expected 4 app bundles');
  });

  test('a file corrupted in transit fails the -c pass', () => {
    // Proof the verification is not a tautology. The checksums are written
    // from the files, so the only way to exercise -c is to change a file
    // between the two -- which is exactly what a truncated artifact round
    // trip does, one job earlier.
    const dir = withArrivals('corrupt', FOUR);
    const script = SUMS_STEP.replace(
      'sha256sum -c SHA256SUMS-app',
      'echo tampered > Screepub-0.6.0-setup.exe\n          sha256sum -c SHA256SUMS-app',
    );
    expect(script).toContain('echo tampered');
    const r = runStep(script, dir, {});
    expect(r.status).not.toBe(0);
    expect(r.stdout + r.stderr).toContain('FAILED');
  });
});

describe("release.yml's manifest step, executed as bash", () => {
  // The last thing app-upload does: build latest.json over the arrivals
  // and upload it, AFTER the installers and the archive it points at are
  // already on the release page. A manifest that goes up first is a
  // window in which the app is told about a file that is not there.
  const withArrivals = (tag: string) => {
    const { dir, lines } = stubDir(`manifest-${tag}`, ['bun', 'gh']);
    mkdirSync(join(dir, 'arrivals'), { recursive: true });
    return { dir, lines };
  };
  const env = { GH_TOKEN: 't', GH_REPO: 'ssandweiss/screepub' };

  test('an ordinary tag builds the manifest over arrivals/ and uploads it, from that directory', () => {
    const { dir, lines } = withArrivals('ok');
    const r = runStep(MANIFEST_STEP, dir, { ...env, TAG: 'v0.6.1' });
    expect(r.status).toBe(0);
    expect(lines('bun')).toEqual([
      'tools/build-update-manifest.ts\t--dir\tarrivals\t--version\t0.6.1\t--out\tarrivals/latest.json',
    ]);
    // Bare filename with --clobber, like every other upload in the file.
    expect(lines('gh')).toEqual(['release\tupload\tv0.6.1\tlatest.json\t--clobber']);
  });

  test('a prerelease gets no manifest and says so, and uploads nothing', () => {
    // releases/latest/download never resolves to a prerelease, and the
    // manifest builder refuses one anyway; the step must not fail a
    // release candidate for it.
    const { dir, lines } = withArrivals('pre');
    const r = runStep(MANIFEST_STEP, dir, { ...env, TAG: 'v0.7.0-rc1' });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('::notice::');
    expect(lines('bun')).toEqual([]);
    expect(lines('gh')).toEqual([]);
  });

  test('a manifest builder that refuses stops the step before anything is uploaded', () => {
    // "No signed artifact" is exit 1 from the tool. A step that shrugged
    // and uploaded whatever was there would publish a release whose
    // updater silently cannot work.
    const { dir, lines } = withArrivals('refused');
    writeFileSync(join(dir, 'bun'), '#!/bin/sh\necho "no signed artifact" >&2\nexit 1\n');
    chmodSync(join(dir, 'bun'), 0o755);
    const r = runStep(MANIFEST_STEP, dir, { ...env, TAG: 'v0.6.1' });
    expect(r.status).not.toBe(0);
    expect(lines('gh')).toEqual([]);
  });
});
