import { describe, test, expect } from 'bun:test';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { TARGETS, MAC_TARGETS } from '../tools/build-cli';
import { parseSidecarArgs } from '../tools/build-sidecar';
import {
  BUNDLE_KINDS,
  MAC_TARGET,
  kindsForOs,
  type BundleArch,
  type BundleOs,
} from '../tools/build-app-bundle';

const WORKFLOWS = '.github/workflows';
const read = (p: string) => readFileSync(p, 'utf8');

/** The Mac command-line tarball: what the release job uploads, and what
 *  the Homebrew formula serves. ONE since the release after the identifier
 *  release: Apple silicon only (owner, 2026-10-01; plan
 *  2026-10-01-f2-handover-amendment.md). */
const MACOS_CLI_ASSETS = ['screepub-cli-macos-arm64.tar.gz'];
/** The Intel tarball every release through the identifier release carried.
 *  Nothing may build, upload, hash or serve it again. */
const RETIRED_MACOS_CLI = 'screepub-cli-macos-x64.tar.gz';
/** Every Mac download by name. The image was the Swift app's until the
 *  identifier release and is the window's since (owner, 2026-10-01); the
 *  name did not change, on purpose. */
const MACOS_ASSETS = ['Screepub-macOS.dmg', ...MACOS_CLI_ASSETS];

describe('the macOS CLI is built apart from the cross set', () => {
  test('the cross builder emits no macOS artifact, by name or by target', () => {
    // The macOS binaries are signed and NOTARIZED on a Mac by the release
    // job (`build-cli.ts --mac`). A cross-compiled, unsigned Mach-O from the
    // Ubuntu job would quietly replace a Gatekeeper-clean download with a
    // warning screen, so the cross table carries no darwin row.
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

  test('the Mac table builds exactly the formula\'s one tarball', () => {
    // Since the CLI move these come from build-cli.ts's MAC_TARGETS, not
    // the Swift app's release script; the name is the one the tap reads.
    expect(MAC_TARGETS.map((t) => t.archiveName)).toEqual(MACOS_CLI_ASSETS);
    expect(MAC_TARGETS.map((t) => t.archiveName)).not.toContain(RETIRED_MACOS_CLI);
  });

  test('tools/bump-tap.sh serves the Apple silicon tarball only and only disables the cask', () => {
    // The Homebrew tap serves the macOS CLI. It must not learn about the
    // Linux or Windows artifacts: brew has no business installing either,
    // and tap-freshness.yml would go red on a formula it cannot audit.
    // The CASK was retired at the identifier release (deprecated) and is
    // disabled from the release after it: never bumped again, so the
    // script reads no image digest and rewrites no cask version.
    // What it does to both files is EXECUTED in tests/tap-scripts.test.ts.
    const sh = read('tools/bump-tap.sh');
    for (const name of MACOS_CLI_ASSETS) expect(sh).toContain(name);
    expect(sh).toContain('depends_on arch: :arm64');
    // Named only by the refusal to bump a release that still carries it.
    expect(sh.split(RETIRED_MACOS_CLI).length - 1).toBeLessThanOrEqual(1);
    expect(sh).not.toContain('Hardware::CPU');
    expect(sh).not.toContain('Screepub-macOS.dmg');
    expect(sh).not.toMatch(/s\|\^  version/);
    expect(sh).toContain('disable! date:');
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

interface Job {
  'runs-on'?: string;
  needs?: string | string[];
  if?: string;
  strategy?: { matrix?: { include?: Record<string, string>[] } };
  permissions?: Record<string, string>;
  steps?: { name?: string; uses?: string; run?: string; shell?: string; with?: Record<string, unknown> }[];
}
interface Workflow {
  jobs: Record<string, Job>;
}

const workflow = (file: string): Workflow =>
  Bun.YAML.parse(read(join(WORKFLOWS, file))) as Workflow;

const runText = (job: Job): string => (job.steps ?? []).map((s) => s.run ?? '').join('\n');

/** The files a job's `gh release upload` commands name, read from the
 *  commands: comment lines dropped, backslash continuations joined, flags
 *  and the "$TAG" operand skipped, paths reduced to their file name. */
const uploadedBy = (job: Job): string[] => {
  const code = runText(job).split('\n').filter((l) => !/^\s*#/.test(l)).join('\n').replace(/\\\n/g, ' ');
  const files: string[] = [];
  for (const m of code.matchAll(/gh release upload\s+([^\n]+)/g)) {
    for (const raw of m[1]!.split(/\s+/)) {
      const tok = raw.replace(/^"(.*)"$/, '$1');
      if (!tok || tok.startsWith('-') || tok === '$TAG') continue;
      files.push(tok.replace(/^.*\//, ''));
    }
  }
  return files;
};

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
    // Since the identifier release the tap waits for the release to be
    // PUBLIC (bump-tap.sh reads releases/tags/vX, which hides drafts).
    expect(needs('tap')).toEqual(['publish']);
    expect(needs('tap-check')).toEqual(['tap']);
  });

  test('the release job leaves the release as a draft; publish makes it public, last', () => {
    // Before the identifier release the release job attached the Swift
    // image and un-drafted at once. The Mac image is the window's now,
    // attached by app-upload much later; un-drafting early would leave the
    // download buttons and every Swift update check on a release with no
    // image. (Found by review 2026-10-01.)
    const release = runText(rel.jobs['release']!);
    expect(release).toContain('--draft');
    expect(release).not.toContain('--draft=false');
    const publish = rel.jobs['publish'];
    expect(publish).toBeDefined();
    expect(needs('publish').sort()).toEqual(['app-upload', 'cross-upload', 'release']);
    expect(runText(publish!)).toContain('gh release edit "$TAG" --draft=false');
    expect(publish!.permissions?.contents).toBe('write');
    // The only job that un-drafts.
    for (const [name, job] of Object.entries(rel.jobs)) {
      if (name === 'publish') continue;
      expect(`${name}: ${runText(job).includes('--draft=false')}`).toBe(`${name}: false`);
    }
  });

  test('publish runs after a failed upload job, but never after a failed release job', () => {
    // A failed bundle or smoke leg must not strand the release in draft:
    // the command-line downloads still go out. A failed RELEASE job means
    // there is nothing to publish.
    const cond = String(rel.jobs['publish']!.if ?? '');
    expect(cond).toContain('!cancelled()');
    expect(cond).toContain("needs.release.result == 'success'");
    expect(cond).not.toMatch(/\bsuccess\(\)/);
  });

  test('publish refuses any image list but exactly Screepub-macOS.dmg, or none', () => {
    // The frozen Swift updater installs the FIRST .dmg on the page, so two
    // images (or a wrongly named one) must keep the release in draft. No
    // image at all, after a failed leg, is published with a warning.
    const text = runText(rel.jobs['publish']!);
    expect(text).toContain('select(endswith(".dmg"))');
    expect(text).toMatch(/"Screepub-macOS\.dmg"\) ;;/);
    expect(text).toMatch(/""\)\s*\n\s*echo "::warning::/);
    expect(text).toMatch(/\*\)\s*\n\s*echo "::error::[^\n]*\n\s*exit 1 ;;/);
  });

  test('every job downstream of publish checks its own needs explicitly', () => {
    // A job-level `if` with no status function gets an implicit success()
    // that also looks at jobs further up the chain, so a failed bundle leg
    // would silently skip the tap. Each says what it waits for instead.
    expect(String(rel.jobs['tap']!.if)).toContain("needs.publish.result == 'success'");
    expect(String(rel.jobs['tap-check']!.if)).toContain("needs.tap.result == 'success'");
    const latest = String(rel.jobs['latest-check']!.if);
    expect(latest).toContain("needs.app-upload.result == 'success'");
    expect(latest).toContain("needs.publish.result == 'success'");
    for (const job of ['tap', 'tap-check', 'latest-check']) {
      expect(String(rel.jobs[job]!.if)).toContain('!cancelled()');
    }
  });

  test('the macOS release job uploads exactly the one CLI tarball, and no image', () => {
    // Read off the `gh release upload` commands themselves: a name in a
    // comment is not an upload. (The first version of this check grepped the
    // whole step and passed on a comment that NAMES the Swift image.)
    const text = runText(rel.jobs['release']!);
    expect(text).toContain('bun tools/build-cli.ts --version');
    expect(text).toContain('--mac --sign "$SIGN_IDENTITY" --notarize');
    expect(uploadedBy(rel.jobs['release']!).sort()).toEqual([...MACOS_CLI_ASSETS].sort());
    // The frozen Swift updater takes the first .dmg on a release; the
    // window's image (from app-upload) must be the only one.
    expect(uploadedBy(rel.jobs['release']!).some((f) => f.endsWith('.dmg'))).toBe(false);
    // And it has NOT quietly become responsible for the new ones.
    for (const t of TARGETS) expect(text).not.toContain(t.archiveName);
  });

  test('nothing in the release workflow builds, hashes or uploads an Intel Mac CLI', () => {
    // Every Intel artifact goes at the release after the identifier
    // release. Read off the whole file, comments included: no line may
    // name the x64 tarball, so a hash, an upload or a crib-sheet line for
    // it cannot come back unnoticed.
    expect(read(join(WORKFLOWS, 'release.yml'))).not.toContain(RETIRED_MACOS_CLI);
    expect(read(join(WORKFLOWS, 'release.yml'))).not.toMatch(/\bX64=|\$X64\b/);
    expect(read(join(WORKFLOWS, 'release.yml'))).not.toMatch(/formula's two tarballs/);
  });

  test('the crib sheet says what the first hand bump must change, not only the url', () => {
    // The tap a person bumps by hand first holds the identifier release's
    // two-arch formula and deprecated cask. Pasting a url and a SHA into it
    // leaves the if/else on the CPU in place and the cask installable.
    const crib = (rel.jobs['release']!.steps ?? []).find((s) => s.name === 'Tap bump crib sheet')!.run!;
    expect(crib).toContain('tools/bump-tap.sh');
    expect(crib).toContain('depends_on arch: :arm64');
    expect(crib).toMatch(/if\/else on the CPU/);
    expect(crib).toContain('disable!');
    // Keeping the deprecation's own, past, date: Homebrew compares it with
    // the user's LOCAL day, and "today" can still be tomorrow somewhere.
    expect(crib).toContain('keeping its date');
    expect(crib).not.toMatch(/today/);
  });

  test('the checks job fails a tag whose three version files disagree', () => {
    const text = runText(rel.jobs['checks']!);
    // package.json was already checked. These two are new, and between them
    // they name the bundle filename, the Info.plist, the deb Version: field
    // and the NSIS product version.
    expect(text).toContain('desktop/src-tauri/Cargo.toml');
    expect(text).toContain('desktop/src-tauri/tauri.conf.json');
    // Read from the TAGGED COMMIT, like every other assertion in that step,
    // not from the working tree: a checkout is not proof of what was tagged.
    expect(text).toMatch(/git cat-file blob "\$GITHUB_SHA:desktop\/src-tauri\/tauri\.conf\.json"/);
    expect(text).toMatch(/git cat-file blob "\$GITHUB_SHA:desktop\/src-tauri\/Cargo\.toml"/);
  });

  test('those two assertions run before any certificate is imported', () => {
    // The whole value of putting them in `checks` is that a mismatched
    // version costs twenty seconds instead of failing after notarization,
    // with a DMG already built. `release` needs `checks`, so the ordering
    // is structural rather than a matter of step order.
    expect(needs('release')).toEqual(['checks']);
    const release = runText(rel.jobs['release']!);
    expect(release).toContain('--sign "$SIGN_IDENTITY" --notarize');
    // And the version check is NOT duplicated into the signing job.
    expect(release).not.toContain('desktop/src-tauri/tauri.conf.json');
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

  test('the checksums are re-verified after the artifact round trip, before the upload', () => {
    // The files are verified and hashed in cross-cli's $RUNNER_TEMP, then
    // cross a job boundary through upload-artifact/download-artifact. The
    // smoke jobs open only their own copies and only two of the three, so
    // without this step screepub-cli-linux-arm64.tar.gz is never touched
    // again and SHA256SUMS is never checked against what it names.
    const steps = rel.jobs['cross-upload']!.steps ?? [];
    const verify = steps.findIndex((s) => /sha256sum -c SHA256SUMS/.test(s.run ?? ''));
    const upload = steps.findIndex((s) => /gh release upload/.test(s.run ?? ''));
    expect(verify).toBeGreaterThanOrEqual(0);
    expect(upload).toBeGreaterThanOrEqual(0);
    // Order is the whole point: verifying after the upload is decoration.
    expect(verify).toBeLessThan(upload);
    // SHA256SUMS names BARE filenames, so -c only resolves them with the
    // download directory as cwd.
    expect(steps[verify]!.run).toMatch(/cd cli/);
  });

  test('linux-arm64 is built and shipped but never smoke-tested', () => {
    // Stated, not hidden: no arm64 runner is assumed available, so this
    // artifact ships untested and the release notes say so. If an arm64
    // runner is ever added, this assertion is the one to delete.
    const jobs = Object.keys(rel.jobs);
    expect(jobs).not.toContain('smoke-linux-arm64');
    expect(runText(rel.jobs['cross-upload']!)).toContain('screepub-cli-linux-arm64.tar.gz');
  });

  // ---- the app bundles -------------------------------------------------
  // These are the STRUCTURAL pins: which jobs exist, what they need, what
  // their steps say. What the four bash blocks actually DO is executed,
  // with stub executables, in tests/release-app-step.test.ts.

  test('app-bundles builds on all three runners, after the checks pass', () => {
    const job = rel.jobs['app-bundles'];
    expect(job).toBeDefined();
    expect(needs('app-bundles')).toEqual(['checks']);
    // The matrix, not the file: `toContain(os)` against the whole YAML
    // would pass on the strength of some OTHER job's runner.
    const runners = (job!.strategy?.matrix?.include ?? []).map((r) => r.os);
    expect(runners.sort()).toEqual(['macos-15', 'ubuntu-latest', 'windows-latest']);
    const text = runText(job!);
    expect(text).toContain('tools/build-app-bundle.ts');
    // The TAG, so a version mismatch fails the build rather than shipping a
    // bundle that misreports itself.
    expect(text).toContain('${TAG#v}');
    const upload = (job!.steps ?? []).find((s) => (s.uses ?? '').includes('upload-artifact'));
    expect(upload).toBeDefined();
    expect(String(upload!.with?.['if-no-files-found'])).toBe('error');
  });

  test('the macOS leg builds ONE Apple silicon DMG, and nothing Intel or universal', () => {
    // One Mac row since 2026-09-20, and one ARCH since the release after
    // the identifier release (owner, 2026-10-01: "Ditch old Intel
    // completely"; plan 2026-10-01-f2-handover-amendment.md). The
    // identifier release was the last universal image, because the frozen
    // Swift updater installs the first .dmg on a release with no
    // architecture check and had to be carried across on Intel too.
    const rows = rel.jobs['app-bundles']!.strategy?.matrix?.include ?? [];
    const macs = rows.filter((r) => r.os === 'macos-15');
    expect(macs.length).toBe(1);
    expect(macs[0]!.arch).toBe('arm64');
    // Read off the text that EXECUTES, comments out: a comment recording
    // that the lipo is gone is not the lipo coming back. Anything below in
    // a command line is an Intel or universal Mac build returning.
    const commands = read(join(WORKFLOWS, 'release.yml'))
      .split('\n')
      .filter((l) => !l.trim().startsWith('#'))
      .join('\n');
    for (const gone of ['bun-darwin-x64', 'x86_64-apple-darwin', 'universal-apple-darwin', '--universal', 'lipo']) {
      expect(`${gone}: ${commands.includes(gone)}`).toBe(`${gone}: false`);
    }
    expect(commands).not.toMatch(/arch: universal|--arch universal|ARCH" = universal/);
    // The one darwin engine target, once: the Mac leg's sidecar build.
    expect(commands.split('bun-darwin-arm64').length - 1).toBe(1);
  });

  test('the cargo triple is named in the tool, not restated in the YAML', () => {
    // aarch64-apple-darwin is derived for every Mac build inside
    // tools/build-app-bundle.ts, which is also where an Intel or universal
    // Mac triple is refused. Restating it here would be a second place for
    // it to drift. Scoped to the text that EXECUTES: comments explaining
    // where the triple went are the opposite of restating it.
    const commands = runText(rel.jobs['app-bundles']!)
      .split('\n')
      .filter((l) => !l.trim().startsWith('#'))
      .join('\n');
    expect(commands).not.toContain('apple-darwin');
    expect(commands).toContain('--arch "$ARCH" --updater');
    // build-app-bundle.ts is never handed a --target; the sidecar step's
    // --target is build-sidecar's bun target, not a cargo triple.
    const build = (rel.jobs['app-bundles']!.steps ?? []).find((s) => /build-app-bundle\.ts/.test(s.run ?? ''))!;
    expect(build.run!.split('\n').filter((l) => !l.trim().startsWith('#')).join('\n')).not.toContain('--target');
  });

  test('the install line the release job writes says Apple silicon, not universal', () => {
    // The notes trailer is the one place a release page itself says what
    // the Mac download runs on.
    const text = runText(rel.jobs['release']!);
    const mac = text.split('\n').find((l) => l.includes('**Mac:**'))!;
    expect(mac).toContain('Screepub-macOS.dmg');
    expect(mac).toContain('Apple silicon');
    expect(mac).toContain('macOS 13 or later');
    expect(mac).not.toMatch(/universal|Intel/i);
  });

  test('no rustup step adds a second darwin target', () => {
    // The universal build needed x86_64-apple-darwin installed beside the
    // runner's own aarch64 one. The Apple silicon build on an Apple silicon
    // runner needs nothing added, so the step is gone, not left installing
    // an Intel target nothing uses.
    expect(runText(rel.jobs['app-bundles']!)).not.toMatch(/rustup target add/);
  });

  test('the Mac-only steps are keyed on the runner OS, not on the arch', () => {
    // While the Mac row was the only universal one, `arch == universal`
    // meant "the Mac leg". With arm64 that stops being true the day an
    // arm64 Linux row is added, so the notarization step asks which OS.
    const notarize = (rel.jobs['app-bundles']!.steps ?? []).find((s) => s.name === 'Notarize and staple the disk image')!;
    expect(String((notarize as { if?: string }).if)).toBe("runner.os == 'macOS'");
    expect(JSON.stringify(rel.jobs['app-bundles'])).not.toContain('matrix.arch ==');
  });

  test('no leg passes a config overlay that renames the product', () => {
    // Until the identifier release the macOS leg passed
    // tauri.transition.conf.json, which named the build "Screepub Desktop".
    // That file is gone: the Mac app is "Screepub" now, like the other two.
    const text = runText(rel.jobs['app-bundles']!);
    expect(text).not.toContain('tauri.transition.conf.json');
    expect(text).not.toMatch(/--config\b/);
  });

  test('the macOS leg signs and notarizes from secrets the repo already has', () => {
    const yml = read(join(WORKFLOWS, 'release.yml'));
    // tauri-bundler reads its OWN variable names; these are the translation.
    for (const v of [
      'APPLE_CERTIFICATE',
      'APPLE_CERTIFICATE_PASSWORD',
      'APPLE_SIGNING_IDENTITY',
      'APPLE_API_KEY',
      'APPLE_API_ISSUER',
      'APPLE_API_KEY_PATH',
    ]) {
      expect(yml).toContain(v);
    }
    // And no NEW secret was invented for it.
    // [A-Z0-9_], with the digits: without them P12 and P8 truncate to
    // "secrets.DEVELOPER_ID_CERT_P" and the list below can never match.
    const declared = yml.match(/secrets\.[A-Z0-9_]+/g) ?? [];
    expect([...new Set(declared)].sort()).toEqual([
      'secrets.AC_API_ISSUER_ID',
      'secrets.AC_API_KEY_ID',
      'secrets.AC_API_KEY_P8_BASE64',
      'secrets.CERT_PASSWORD',
      'secrets.DEVELOPER_ID_CERT_P12_BASE64',
      'secrets.KEYCHAIN_PASSWORD',
      'secrets.TAP_TOKEN',
      // The updater's signing key and its password (docs/release-secrets.md
      // §4). Added 2026-09-21 for piece A; the ONLY two added since the
      // list above was written.
      'secrets.TAURI_SIGNING_PRIVATE_KEY',
      'secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD',
    ]);
  });

  // ---- the updater ------------------------------------------------------
  // Piece A's transport half: the signed archive, the manifest, and the
  // alarm. docs/superpowers/plans/2026-09-21-updater-transport.md.

  test('the checks job refuses a tag whose app trusts no real updater key', () => {
    // From the TAGGED commit, like every other assertion in that job. A
    // tag with an empty pubkey ships an updater that can never accept a
    // release, which is what ADR 2026-09-21 says v0.6.1 must not do. This
    // is a tag-time gate and not a bun test on purpose: the key does not
    // exist yet, and a test that fails until the owner acts is a red main
    // for nobody's fault.
    const text = runText(rel.jobs['checks']!);
    expect(text).toContain('tools/update-signature.ts --pubkey-from-config');
    expect(text).toMatch(/git cat-file blob "\$GITHUB_SHA:desktop\/src-tauri\/tauri\.conf\.json"[^\n]*>/);
    // After bun is set up, since it is a bun tool; still before any
    // certificate is imported, since `release` needs `checks`.
    const steps = rel.jobs['checks']!.steps ?? [];
    const bun = steps.findIndex((s) => (s.uses ?? '').includes('setup-bun'));
    const gate = steps.findIndex((s) => /pubkey-from-config/.test(s.run ?? ''));
    expect(gate).toBeGreaterThan(bun);
  });

  test('the macOS leg asks for the updater archive and carries the signing key, macOS only', () => {
    const yml = read(join(WORKFLOWS, 'release.yml'));
    const text = runText(rel.jobs['app-bundles']!);
    // --updater inside the macOS branch.
    expect(text).toMatch(/if \[ "\$RUNNER_OS" = macOS \][\s\S]*--updater/);
    // The two secrets reach tauri-cli under its OWN variable names, and
    // only on macOS, in exactly the shape the APPLE_* ones already use.
    expect(yml).toMatch(/TAURI_SIGNING_PRIVATE_KEY: \$\{\{ runner\.os == 'macOS' && secrets\.TAURI_SIGNING_PRIVATE_KEY \|\| '' \}\}/);
    expect(yml).toMatch(/TAURI_SIGNING_PRIVATE_KEY_PASSWORD: \$\{\{ runner\.os == 'macOS' && secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD \|\| '' \}\}/);
    // And desktop.yml, the push path, knows nothing of either.
    const desktop = read(join(WORKFLOWS, 'desktop.yml'));
    expect(desktop).not.toContain('TAURI_SIGNING');
    expect(desktop).not.toContain('--updater');
  });

  test('app-upload builds latest.json from what arrived, LAST, and uploads archive, signature and manifest', () => {
    const job = rel.jobs['app-upload']!;
    const steps = job.steps ?? [];
    // It needs a checkout and bun now, because the manifest is built by a
    // tool rather than by YAML. The download therefore cannot land under
    // the old name, which in a checkout is the Swift app's source directory.
    expect(steps.some((s) => (s.uses ?? '').includes('actions/checkout'))).toBe(true);
    expect(steps.some((s) => (s.uses ?? '').includes('setup-bun'))).toBe(true);
    const download = steps.find((s) => (s.uses ?? '').includes('download-artifact'));
    expect(download?.with?.path).toBe('arrivals');
    const text = runText(job);
    expect(text).not.toMatch(/cd app\b/);
    expect(text).toContain('tools/build-update-manifest.ts --dir arrivals');
    expect(text).toContain('--out arrivals/latest.json');
    // The installers, the archive and its signature go up first; the
    // manifest that points at them goes up LAST, so nothing it names is
    // ever missing when it is read.
    const upload = steps.findIndex((s) => /gh release upload[\s\S]*\*\.app\.tar\.gz\.sig/.test(s.run ?? ''));
    const manifest = steps.findIndex((s) => /build-update-manifest\.ts/.test(s.run ?? ''));
    expect(upload).toBeGreaterThanOrEqual(0);
    expect(manifest).toBeGreaterThan(upload);
    expect(steps[manifest]!.run).toContain('latest.json');
    expect(steps[manifest]!.run).toMatch(/gh release upload/);
    // A prerelease gets no manifest: releases/latest never points at one.
    expect(steps[manifest]!.run).toMatch(/\*-\*/);
  });

  test('latest-check reads the manifest back from GitHub after the upload, per release', () => {
    // The tap-check pattern: a job INSIDE the release run, because a
    // `release: published` trigger elsewhere never fires for a release
    // created with the default token. A red job here means the release
    // published and the updater cannot use it.
    const job = rel.jobs['latest-check'];
    expect(job).toBeDefined();
    // And after publish: releases/latest never resolves to a draft.
    expect(needs('latest-check').sort()).toEqual(['app-upload', 'publish']);
    expect(runText(job!)).toContain('tools/check-latest.ts');
    // No --version: the question is whether the ENDPOINT serves the newest
    // release, which is what the app asks and is the right question even
    // when an old tag is being re-run.
    expect(runText(job!)).not.toContain('--version');
    expect(JSON.stringify(job)).toContain("!contains(github.ref_name, '-')");
  });

  test('the weekly freshness workflow checks the manifest as well as the tap', () => {
    const weekly = workflow('tap-freshness.yml');
    const jobs = Object.values(weekly.jobs);
    expect(jobs.some((j) => runText(j).includes('tools/check-tap.sh'))).toBe(true);
    expect(jobs.some((j) => runText(j).includes('tools/check-latest.ts'))).toBe(true);
  });

  test('the Windows leg is deliberately unsigned', () => {
    // Authenticode is a procurement problem, not an engineering one. This
    // assertion is what keeps that a decision rather than a drift.
    const yml = read(join(WORKFLOWS, 'release.yml'));
    expect(yml).not.toContain('WINDOWS_CERTIFICATE');
    expect(yml).not.toContain('signtool');
  });

  test('every bundle is smoked on the OS that built it, before any upload', () => {
    const steps = rel.jobs['app-bundles']!.steps ?? [];
    const build = steps.findIndex((s) => /build-app-bundle\.ts/.test(s.run ?? ''));
    const smoke = steps.findIndex((s) => /smoke-bundle\.ts/.test(s.run ?? ''));
    const upload = steps.findIndex((s) => (s.uses ?? '').includes('upload-artifact'));
    expect(build).toBeGreaterThanOrEqual(0);
    expect(smoke).toBeGreaterThan(build);
    expect(upload).toBeGreaterThan(smoke);
    // The upload job waits for BOTH the release job and every bundle leg.
    // If it did not wait for the smoke, the smoke would be decoration.
    expect(needs('app-upload').sort()).toEqual(['app-bundles', 'release']);
    expect(rel.jobs['app-upload']!.permissions?.contents).toBe('write');
  });

  test('the Mac DMG is smoked whole, so no notice about an untested slice', () => {
    // The universal image carried an Intel slice no runner executed, and
    // the step said so in a notice. The Apple silicon image is the
    // runner's own arch, opened and run in full like every other bundle.
    const smoke = (rel.jobs['app-bundles']!.steps ?? []).find((s) =>
      /smoke-bundle\.ts/.test(s.run ?? ''),
    )!.run!;
    expect(smoke).not.toMatch(/::notice::/);
    expect(smoke).not.toMatch(/slice/i);
    // No early exit: the old step RETURNED before the loop on the x86_64
    // leg. Nothing may skip the loop now. Matched as a command on its own
    // line, since the step's own comments talk about exiting 0.
    expect(smoke.split('\n').some((l) => l.trim() === 'exit 0')).toBe(false);
    // And the step still fails, rather than exiting 0, if the glob on any
    // leg matches nothing at all.
    expect(smoke).toContain('NOTHING WAS CHECKED');
  });

  test('every bash step in the two new jobs declares `shell: bash`', () => {
    // Without it, a `run:` block on windows-latest is executed by
    // PowerShell, where `set -euo pipefail` and `ARGS=(...)` are syntax
    // errors -- and the Windows leg is the one nobody here can try first.
    for (const step of rel.jobs['app-bundles']!.steps ?? []) {
      if (!step.run) continue;
      // The two Linux/macOS-only steps are guarded by `if:` and never
      // reach a Windows runner.
      if (/apt-get|base64 --decode/.test(step.run)) continue;
      if (/^\s*(rustup|cargo|bun install)/.test(step.run.trim())) continue;
      expect(step.shell).toBe('bash');
    }
  });

  test('the checksums are re-verified after the artifact round trip', () => {
    // Same blind spot cross-upload already names: the files are hashed in
    // one job's workspace and then cross a job boundary.
    const steps = rel.jobs['app-upload']!.steps ?? [];
    const verify = steps.findIndex((s) => /sha256sum -c SHA256SUMS-app/.test(s.run ?? ''));
    const upload = steps.findIndex((s) => /gh release upload/.test(s.run ?? ''));
    expect(verify).toBeGreaterThanOrEqual(0);
    expect(verify).toBeLessThan(upload);
    // `arrivals`, since 2026-09-21: the job checks the repository out now,
    // and the old download directory name collides with the Swift app's
    // source directory in a checkout.
    expect(steps[verify]!.run).toMatch(/cd arrivals/);
  });

  test('the app checksums file does not overwrite the CLI one', () => {
    // cross-upload publishes SHA256SUMS to the same release page. One
    // clobbering the other leaves downloads silently uncheckable.
    const text = runText(rel.jobs['app-upload']!);
    expect(text).toContain('SHA256SUMS-app');
    expect(text).not.toMatch(/SHA256SUMS(?!-app)/);
  });

  test('the release job builds the macOS CLI itself, signed and notarized, and no Swift app', () => {
    // Since the CLI move the Swift release script is not run at all: the
    // tarballs come from build-cli.ts --mac, which signs before packaging
    // and refuses to start without the notary key.
    const release = runText(rel.jobs['release']!);
    expect(release).not.toContain('app/release.sh');
    expect(release).not.toContain('app/dist/');
    expect(release).not.toMatch(/rosetta/i);
    expect(release).toMatch(/bun tools\/build-cli\.ts --version "\$\{VERSION#v\}" --out "\$RUNNER_TEMP\/cli-mac"/);
    expect(release).toContain('--mac --sign "$SIGN_IDENTITY" --notarize');
    expect(uploadedBy(rel.jobs['release']!)).not.toContain('Screepub-macOS.dmg');
    // The notes' checksums, the upload and the crib sheet all read the
    // build's --out folder: a path drifting in one of the three would kill
    // the release after both notarizations (review, 2026-10-01).
    const out = '$RUNNER_TEMP/cli-mac';
    for (const name of MACOS_CLI_ASSETS) {
      expect(release.split(`${out}/${name}`).length - 1).toBeGreaterThanOrEqual(2); // upload + crib
    }
    expect(release).toContain(`cd "${out}" && shasum -a 256`); // the notes
    // The new jobs must not touch the Swift build, the CLI or the tap.
    for (const jobName of ['app-bundles', 'app-upload']) {
      const text = runText(rel.jobs[jobName]!);
      expect(text).not.toContain('app/release.sh');
      expect(text).not.toContain('bump-tap');
      for (const name of MACOS_CLI_ASSETS) expect(text).not.toContain(name);
    }
    expect(needs('tap')).toEqual(['publish']);
    expect(needs('tap-check')).toEqual(['tap']);
    // And nothing that builds or uploads learned to wait on the bundle
    // legs: only publish (which runs after a failed leg, see its own test)
    // and the checks after it do.
    for (const jobName of ['release', 'cross-cli', 'cross-upload']) {
      expect(needs(jobName)).not.toContain('app-bundles');
      expect(needs(jobName)).not.toContain('app-upload');
    }
  });

  test('the Linux leg ships both a .deb and an .rpm', () => {
    const text = runText(rel.jobs['app-upload']!);
    expect(text).toContain('.deb');
    expect(text).toContain('.rpm');
  });
});

describe('desktop.yml bundles and smokes on every push', () => {
  // These are the STRUCTURAL pins -- which steps exist, in which order, on
  // which runners. What the two bash blocks actually DO is executed, with
  // stub executables, in tests/desktop-bundle-step.test.ts.
  const desktop = workflow('desktop.yml');
  const job = desktop.jobs['build']!;
  const steps = job.steps ?? [];
  const stepIndex = (re: RegExp): number => steps.findIndex((s) => re.test(s.run ?? ''));

  test('it still compiles on all three platforms', () => {
    // The matrix predates this piece and is not replaced by it.
    expect(JSON.stringify(job)).toContain('cargo build --locked');
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    for (const os of ['ubuntu-latest', 'macos-15', 'windows-latest']) {
      expect(yml).toContain(os);
    }
  });

  test('the generated-files diff runs BEFORE anything that rewrites Cargo.toml', () => {
    // `cargo tauri build` rewrites desktop/src-tauri/Cargo.toml with the
    // feature lists it derives from tauri.conf.json. The committed spelling
    // makes that a no-op today, but a future tauri.conf.json option would
    // make it rewrite again -- and a `git diff --exit-code` running after
    // the bundle step would then go red for a reason nobody could read.
    const diff = stepIndex(/git diff --exit-code/);
    const bundle = stepIndex(/cargo tauri build/);
    expect(diff).toBeGreaterThanOrEqual(0);
    expect(bundle).toBeGreaterThanOrEqual(0);
    expect(diff).toBeLessThan(bundle);
  });

  test('the bundle list is pinned and never the per-OS default', () => {
    // The Linux default is deb, rpm AND appimage, and the AppImage step
    // corrupts the Bun-compiled engine and fails the whole run.
    const text = runText(job);
    expect(text).toContain('--bundles');
    expect(text).not.toContain('appimage');
  });

  test('it runs smoke-bundle after building, not instead of it', () => {
    const bundle = stepIndex(/cargo tauri build/);
    const smoke = stepIndex(/tools\/smoke-bundle\.ts/);
    expect(smoke).toBeGreaterThan(bundle);
    expect(steps[smoke]!.run).toContain('--built');
  });

  test('both bash steps run under bash on every runner, Windows included', () => {
    // Without `shell: bash` a `run:` block on windows-latest is executed by
    // PowerShell, where `case ... esac` and `set -euo pipefail` are syntax
    // errors -- and the Windows leg is the one nobody here can try first.
    for (const re of [/cargo tauri build/, /tools\/smoke-bundle\.ts/]) {
      expect(steps[stepIndex(re)]!.shell).toBe('bash');
    }
  });

  test('cargo-tauri is installed at a pinned version and cached', () => {
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    // An unpinned `cargo install tauri-cli` is a four-minute build against
    // whatever crates.io served that morning.
    expect(yml).toMatch(/cargo install tauri-cli --version [0-9]/);
    expect(yml).toContain('--locked');
    expect(yml).toContain('~/.cargo/bin/cargo-tauri');
    // The cache key must name the SAME version that gets installed, or a
    // version bump is served the old binary out of the cache forever.
    const installed = /cargo install tauri-cli --version ([0-9][^ ]*) /.exec(yml)?.[1];
    expect(installed).toBeDefined();
    expect(yml).toContain(`key: tauri-cli-${installed}-`);
  });

  test('the push path imports no Developer ID certificate', () => {
    // Signing on every push is slow and exposes the secret far more widely
    // than a release does. The push bundle is unsigned on purpose.
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    for (const secret of ['APPLE_CERTIFICATE', 'DEVELOPER_ID_CERT_P12_BASE64', 'APPLE_API_KEY']) {
      expect(yml).not.toContain(secret);
    }
  });

  test('an edit to any tool this workflow runs triggers it', () => {
    // The bundle and smoke steps run code that lives outside desktop/. A
    // path filter that did not list it would let smoke-bundle.ts change
    // without the only workflow that executes it ever running.
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    const filters = yml.split('\n').filter((l) => l.trim().startsWith('paths:'));
    expect(filters.length).toBe(2);
    for (const line of filters) {
      for (const path of [
        'tools/smoke-bundle.ts',
        'tools/build-app-bundle.ts',
        'tools/bundle-archive.ts',
        'package.json',
      ]) {
        expect(line).toContain(`'${path}'`);
      }
    }
  });

  test('the workflow says, where a reader meets it, what it does not prove', () => {
    // The two admissions that must not quietly disappear: nothing here
    // launches the GUI, and build-app-bundle.ts's renaming half is not
    // exercised on the push path at all.
    const yml = read(join(WORKFLOWS, 'desktop.yml'));
    expect(yml).toContain('no runner has a display');
    expect(yml).toMatch(/build-app-bundle\.ts/);
  });
});

/** The text of `doc` from the heading `start` up to the next heading of the
 *  same level (or the end). Fails loudly when the heading is missing, so a
 *  renamed section cannot make a scoped check pass against an empty string. */
function sectionOf(doc: string, start: string): string {
  const at = doc.indexOf(`\n${start}\n`);
  expect(at, `heading "${start}" is missing`).toBeGreaterThan(-1);
  const level = start.match(/^#+ /)![0];
  const rest = doc.slice(at + start.length + 2);
  const next = rest.search(new RegExp(`^${level}`, 'm'));
  return next === -1 ? rest : rest.slice(0, next);
}

describe('the two limits are stated where a reader meets them', () => {
  const readme = read('README.md');
  const developers = read('docs/developers.md');

  test('the README says the Windows download is unsigned', () => {
    const lower = readme.toLowerCase();
    expect(lower).toContain('windows');
    expect(lower).toContain('smartscreen');
    expect(/not signed|unsigned/.test(lower)).toBe(true);
  });

  test('the developer page names every command-line archive, and the README links to it', () => {
    // The CLI downloads moved out of the README (spec 2026-09-22, part 1):
    // the README keeps one line and a link. The names are DERIVED from the
    // builder, so a renamed archive fails here rather than leaving a page
    // naming a file the release does not carry.
    for (const t of TARGETS) expect(developers).toContain(t.archiveName);
    for (const name of MACOS_ASSETS.filter((n) => n.endsWith('.tar.gz'))) {
      expect(developers).toContain(name);
    }
    expect(readme).toContain('docs/developers.md#install-the-command-line-converter');
    expect(developers).toContain('\n## Install the command-line converter\n');
  });

  test('the developer page offers no Intel Mac download, and says Homebrew is Apple Silicon only', () => {
    // Reads docs/developers.md alone: the README is another session's, and
    // still says Intel until its own branch lands. Intel Macs are not
    // supported after the identifier release (plan
    // 2026-10-01-f2-handover-amendment.md), so the page must not offer the
    // Intel tarball the release no longer carries.
    const install = sectionOf(developers, '## Install the command-line converter');
    expect(install).not.toContain(RETIRED_MACOS_CLI);
    expect(install).not.toMatch(/\| Mac, Intel \|/);
    expect(install.replace(/\s+/g, ' ')).toContain('On a Mac with Apple Silicon, Homebrew installs');
    expect(install).toContain('The Mac archive is signed and notarized');
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

describe('the app downloads are described where a reader meets them', () => {
  const readme = read('README.md');
  const developers = read('docs/developers.md');
  const notes = read('docs/releases/0.6.0.md');
  const site = read('site/index.html');
  const ledger = read('docs/verification-ledger.md');
  const rel = workflow('release.yml');
  const VERSION = '0.6.0';
  /** The one Mac download: the window's image since the identifier release. */
  const MAC_DMG = MACOS_ASSETS[0]!;

  // The names are DERIVED, not restated: release.yml's matrix says which
  // OS/arch legs run, and BUNDLE_KINDS says what each leg is named. So a
  // renamed artifact, a dropped leg or an ADDED leg (an arm64 Linux runner,
  // say) fails a documentation test rather than leaving these pages naming
  // a file the release does not carry, or silently omitting one it does.
  const published = new Map<string, BundleOs>();
  const unpublished = new Set<string>();
  for (const row of rel.jobs['app-bundles']!.strategy?.matrix?.include ?? []) {
    const os: BundleOs = row.os!.startsWith('ubuntu')
      ? 'linux'
      : row.os!.startsWith('macos')
        ? 'macos'
        : 'windows';
    for (const kind of kindsForOs(os)) {
      published.set(kind.releasedName(VERSION, row.arch as BundleArch), os);
    }
  }
  for (const kind of BUNDLE_KINDS) {
    for (const arch of ['x64', 'arm64'] as const) {
      const name = kind.releasedName(VERSION, arch);
      if (!published.has(name)) unpublished.add(name);
    }
  }
  // Names no row can build any more: the per-arch Mac images of the
  // universal era. The Mac image carries no arch now (Apple silicon only),
  // so the loop above cannot produce these, and no page may offer them.
  for (const gone of ['Screepub-macOS-arm64.dmg', 'Screepub-macOS-x64.dmg']) unpublished.add(gone);

  const NUMBER_WORD = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight'];
  const install = sectionOf(readme, '## Install');

  /** The README install table's row for `file`. */
  const row = (file: string): string => {
    const rows = install.split('\n').filter((l) => l.startsWith('|') && l.includes(`\`${file}\``));
    expect(rows).toHaveLength(1);
    return rows[0]!;
  };

  test('the derivation found the four files the release actually uploads', () => {
    // Guards every loop below: a matrix this parse did not understand would
    // make them all vacuously true. Four is what app-upload's own line-count
    // check demands: one .deb, one .rpm, ONE .dmg, one .exe.
    expect([...published.keys()].sort()).toEqual([
      'Screepub-linux-amd64.deb',
      'Screepub-linux-x86_64.rpm',
      'Screepub-macOS.dmg',
      'Screepub-windows-x64-setup.exe',
    ]);
    // And the ones no leg builds, which no page may offer.
    expect([...unpublished].sort()).toEqual([
      'Screepub-linux-aarch64.rpm',
      'Screepub-linux-arm64.deb',
      'Screepub-macOS-arm64.dmg',
      'Screepub-macOS-x64.dmg',
    ]);
  });

  test('the README install table has a row for every file the release uploads', () => {
    for (const name of published.keys()) row(name);
    row(MAC_DMG);
  });

  test('no page offers a bundle the release does not carry', () => {
    for (const text of [readme, developers, notes, site]) {
      for (const name of unpublished) expect(text).not.toContain(name);
    }
  });

  test('the README says how many files the app checksums cover', () => {
    // Spelled out, so adding a leg without touching this sentence fails here
    // rather than publishing a checksums file covering more than the page
    // admits to.
    expect(install).toContain('SHA256SUMS-app');
    expect(install).toContain(`covers these ${NUMBER_WORD[published.size]} files`);
  });

  test('the Windows warning says what to press, on both pages that offer a Windows download', () => {
    // One warning in the README's Install section now covers the installer
    // and the CLI alike; the developer page carries the same words for the
    // CLI archive. Two differently-worded warnings read as two problems.
    for (const text of [install, sectionOf(developers, '## Install the command-line converter')]) {
      // Whitespace-normalised: the pages hard-wrap, and a phrase split
      // across two lines is still the phrase a reader sees.
      const lower = text.replace(/\s+/g, ' ').toLowerCase();
      expect(lower).toContain('smartscreen');
      expect(/not signed|unsigned/.test(lower)).toBe(true);
      expect(lower).toContain('more info');
      expect(lower).toContain('run anyway');
    }
  });

  test('the README does not claim the installer works fully offline', () => {
    // NSIS's default webviewInstallMode downloads the WebView2 bootstrapper
    // when the machine has none. The install note says so, and so does the
    // privacy section's list of network touchpoints, because a reader who
    // stops there never reaches the other note.
    expect(install.toLowerCase()).toContain('webview2');
    expect(sectionOf(readme, '## Your script stays on your machine').toLowerCase()).toContain('webview2');
  });

  test('the README install statuses agree with the verification ledger', () => {
    // The ledger is the one place verification is recorded; the README's
    // status column may say no more than it. If a ledger line below goes,
    // somebody did the thing: move the README row (and the site's) up to
    // match, then update this test.
    const VERIFIED = 'Verified by a person';
    const NOT_YET = 'Built and checked automatically. Never installed by a person yet';
    const person = ledger.slice(
      ledger.indexOf('**Verified on a real machine, by a person**'),
      ledger.indexOf('**Verified only by CI'),
    );
    const nobody = ledger.slice(ledger.indexOf('**Verified by nobody**'));
    expect(nobody).toContain('Installing the `.deb`, the `.rpm` or the `.exe`');
    // History since Intel was dropped: the identifier release's universal
    // image was the last with an Intel slice, and it never ran.
    expect(nobody).toContain("The Intel SLICE of the identifier release's universal `.dmg`");
    for (const [file, os] of published) {
      if (os === 'macos') continue;
      expect(row(file)).toContain(NOT_YET);
    }
    // The Mac window: installed by a person on Apple Silicon, never on Intel.
    expect(person).toContain('/Applications');
    expect(person).toContain('Apple Silicon');
    const mac = row(MAC_DMG);
    expect(mac).toContain(VERIFIED);
    expect(mac).toContain('Apple Silicon');
    expect(mac).toContain('Intel');
    // And the README sends a reader to the ledger itself.
    expect(install).toContain('docs/verification-ledger.md');
  });

  test('the 0.6.0 notes carry the same three limits', () => {
    const lower = notes.toLowerCase();
    expect(/not signed|unsigned/.test(lower)).toBe(true);
    expect(lower).toContain('smartscreen');
    expect(lower).toContain('never been installed');
    expect(lower).toContain('tolino');
  });

  test('the 0.6.0 notes named the app downloads 0.6.0 actually published', () => {
    // HISTORY, pinned as literals on purpose: published notes are never
    // rewritten, and on 2026-09-22 the Linux and Windows names lost their
    // version. These are the four files 0.6.0 put on its release page.
    for (const name of [
      'Screepub_0.6.0_amd64.deb',
      'Screepub-0.6.0-1.x86_64.rpm',
      'Screepub-0.6.0-setup.exe',
      'Screepub-Desktop-macOS-universal.dmg',
    ]) {
      expect(notes).toContain(name);
    }
  });

  test('the download page carries the unsigned-Windows warning', () => {
    const lower = site.toLowerCase();
    expect(lower).toContain('smartscreen');
    expect(/not signed|unsigned/.test(lower)).toBe(true);
  });

  test('the site and the README offer one Mac download, the window\'s image', () => {
    // One Mac app since the identifier release. The site's three buttons and
    // the README's one kept their URL across it: the name moved to the
    // window's image, so they serve the window with no edit. The old
    // Desktop name 404s, so no page may link it, and the pages say the
    // macOS floor the window really has (13; 14 was the Swift app's).
    const LATEST_MAC = `https://github.com/ssandweiss/screepub/releases/latest/download/${MAC_DMG}`;
    expect(site).toContain(MAC_DMG);
    const buttons = [...site.matchAll(/<a class="btn[^"]*" href="([^"]+)"/g)].map((m) => m[1]);
    expect(buttons).toEqual(Array(3).fill(LATEST_MAC));
    const top = readme.slice(0, readme.indexOf('\n## '));
    expect(top).toContain(`href="${LATEST_MAC}"`);
    for (const text of [readme, site, developers]) {
      expect(text).not.toContain('Screepub-Desktop-macOS-universal.dmg');
      expect(text).not.toContain('Screepub Desktop');
      expect(text).not.toMatch(/macOS 14/);
    }
    for (const text of [install, site]) {
      expect(text.replace(/\s+/g, ' ')).toContain('macOS 13 or later');
    }
  });

  test('nothing anywhere promises an AppImage, winget or the AUR', () => {
    // All out of scope. A promise in prose is a promise.
    for (const text of [readme, developers, notes, site]) {
      const lower = text.toLowerCase();
      expect(lower).not.toContain('appimage');
      expect(lower).not.toContain('winget');
      expect(lower).not.toContain('aur ');
    }
    // The Homebrew cask is retired (deprecated at the identifier release),
    // so no page tells a reader to install it. The formula, which installs
    // the command-line converter, is still offered.
    for (const text of [readme, developers, site]) {
      expect(text).not.toContain('--cask');
    }
    expect(readme).toContain('brew install --formula ssandweiss/tap/screepub');
  });
});

describe('docs/verification-ledger.md is the one ledger of who verified what', () => {
  const doc = read('docs/verification-ledger.md');
  const person = doc.slice(
    doc.indexOf('**Verified on a real machine, by a person**'),
    doc.indexOf('**Verified only by CI'),
  );
  const ci = doc.slice(doc.indexOf('**Verified only by CI'), doc.indexOf('**Verified by nobody**'));
  const nobody = doc.slice(doc.indexOf('**Verified by nobody**'));

  test('it has all three lists, in order: a person, CI, nobody', () => {
    const p = doc.indexOf('**Verified on a real machine, by a person**');
    const c = doc.indexOf('**Verified only by CI');
    const n = doc.indexOf('**Verified by nobody**');
    expect(p).toBeGreaterThan(0);
    expect(c).toBeGreaterThan(p);
    expect(n).toBeGreaterThan(c);
  });

  test('the "by a person" list claims only the architecture that was built', () => {
    // The .deb and .rpm a person opened are aarch64. The release publishes
    // amd64 and x86_64, which no person has run. Saying "the Linux bundles"
    // without the architecture is the overstatement this ledger prevents.
    expect(person).toContain('Screepub_0.6.0_arm64.deb');
    expect(person).toContain('Screepub-0.6.0-1.aarch64.rpm');
    expect(person).not.toContain('amd64');
    // The Mac window: installed by hand, and updated from a release.
    expect(person).toContain('/Applications');
    expect(person).toContain('0.7.2 to 0.7.3');
    // Every Mac a person used is Apple Silicon, which is now the only kind
    // the Mac download is built for (Intel dropped after the identifier
    // release), so the list no longer speaks of "the ARM half".
    const flat = person.replace(/\s+/g, ' ');
    expect(flat).toContain('Each Mac that ran these is Apple Silicon, the only kind the Mac download is built for');
    expect(flat).not.toMatch(/ARM half/);
  });

  test('the "nobody" list keeps what nobody has done, and only that', () => {
    expect(nobody).not.toMatch(/`\.dmg` or `\.exe` has been\s+installed/);
    expect(nobody).toContain('.exe');
    expect(nobody).not.toContain('x86_64-apple-darwin');
    // The Intel slice never ran, and is now history: the identifier
    // release's universal image was the last to carry one.
    const intel = nobody.slice(nobody.indexOf('- The Intel SLICE'));
    const bullet = intel.slice(0, intel.indexOf('\n- ')).replace(/\s+/g, ' ');
    expect(bullet).toContain('the last Mac download to carry one');
    expect(bullet).toContain('never ran');
    expect(bullet).toContain('Apple Silicon only');
    expect(bullet).not.toMatch(/has never executed/);
  });

  test('the CI list says what release.yml does, and that the Mac image runs whole', () => {
    // release.yml has built, signed and notarized the Mac window on every tag
    // since 0.7.0; the ledger must not still say it never ran. The image is
    // Apple Silicon only after the identifier release, the runner's own
    // kind, so all of it runs and the smoke step prints no slice notice.
    expect(ci).toContain('release.yml');
    expect(ci).toMatch(/notariz/);
    expect(ci).not.toMatch(/never run|never been run|has not run|never executed/);
    const flat = ci.replace(/\s+/g, ' ');
    expect(flat).toContain('The Mac `.dmg` is built for Apple Silicon only');
    expect(flat).not.toMatch(/prints a notice/);
  });

  test('every relative link in the ledger resolves', () => {
    for (const m of doc.matchAll(/\]\(([^)#]+)(#[^)]*)?\)/g)) {
      const target = m[1]!;
      if (/^https?:/.test(target)) continue;
      expect(existsSync(join('docs', target)), target).toBe(true);
    }
  });

  test('desktop/README.md points at this ledger instead of keeping its own', () => {
    const desktop = read('desktop/README.md');
    expect(desktop).toContain('docs/verification-ledger.md');
    expect(desktop).not.toContain('Verified by nobody');
  });
});

describe('the developer docs describe the Apple silicon build', () => {
  // Intel Macs are not supported after the identifier release, so the
  // universal build (two engines lipo'd, a universal cargo target) is gone.
  // A developer doc still describing it hands a contributor commands that
  // fail. README.md and site/ are not read here: another session owns them.
  const desktop = read('desktop/README.md');

  test('desktop/README.md documents only commands the tools accept', () => {
    // Every build-sidecar line in the doc is parsed by the real parser, as
    // an Apple Silicon Mac would run it, and every cargo target it names is
    // the one build-app-bundle.ts builds a Mac bundle for.
    const sidecarLines = [...desktop.matchAll(/^\s*bun tools\/build-sidecar\.ts ([^#\n]*)/gm)].map((m) =>
      m[1]!.trim().split(/\s+/),
    );
    expect(sidecarLines.length).toBeGreaterThanOrEqual(2);
    for (const args of sidecarLines) {
      expect(() => parseSidecarArgs(args, 'darwin', 'arm64')).not.toThrow();
    }
    expect(desktop).toContain('bun tools/build-sidecar.ts --target bun-darwin-arm64');
    // An Intel Mac can still BUILD the Apple silicon bundle, but has no
    // aarch64 standard library until it is added.
    expect(desktop).toContain('rustup target add aarch64-apple-darwin');
    const cargoTargets = [...desktop.matchAll(/cargo tauri build --target (\S+)/g)].map((m) => m[1]);
    expect(cargoTargets).toEqual([MAC_TARGET]);
    for (const gone of ['--universal', 'universal-apple-darwin', 'x86_64-apple-darwin', 'lipo']) {
      expect(`${gone}: ${desktop.includes(gone)}`).toBe(`${gone}: false`);
    }
    expect(desktop).not.toMatch(/one universal `\.dmg`/);
  });

  test('CONTRIBUTING.md and the device map no longer speak of a universal Mac build', () => {
    expect(read('CONTRIBUTING.md')).not.toMatch(/universal Mac\s+build/);
    expect(read('docs/device-map.md')).not.toMatch(/universal Mac\s+build/);
  });

  test('CONTRIBUTING.md and the ledger describe the weekly cask check the same way', () => {
    // check-tap.sh wants deprecate! while a release carries the Intel
    // tarball and disable! after; both pages say so in the same words.
    const words = 'retired (deprecated, and disabled from the release after the identifier release)';
    for (const file of ['CONTRIBUTING.md', 'docs/verification-ledger.md']) {
      const flat = read(file).replace(/\s+/g, ' ');
      expect(`${file}: ${flat.includes(words)}`).toBe(`${file}: true`);
      expect(flat).not.toContain('cask still says it is deprecated');
    }
  });

  test('CONTRIBUTING.md says an Intel Mac cannot run the window, where it says to build it', () => {
    // `build-sidecar.ts --host` refuses an Intel Mac now, so the two-line
    // quick start would fail there with no word of why.
    const contributing = read('CONTRIBUTING.md');
    const at = contributing.indexOf('bun tools/build-sidecar.ts --host');
    expect(at).toBeGreaterThan(-1);
    const after = contributing.slice(at, at + 800).replace(/\s+/g, ' ');
    expect(after).toContain('An Intel Mac cannot run the window');
  });

  test('the release runbook counts one Mac CLI tarball and one SHA', () => {
    // The sections recording the 2026-08 per-arch flip are dated history
    // and keep their two-arch blocks; what is checked here is the live
    // instructions: the re-run, the by-hand bump and the bumper.
    const runbook = read('docs/release-secrets.md').replace(/\s+/g, ' ');
    expect(runbook).not.toContain('Two SHAs are needed');
    expect(runbook).not.toContain('two urls, two SHAs');
    expect(runbook).not.toContain("rewrites the formula's two urls and two SHAs");
    expect(runbook).not.toContain('after the full universal build');
    expect(runbook).not.toMatch(/`screepub-cli-macos-x64\.tar\.gz` from the `release` job/);
    expect(runbook).toContain('One SHA is needed');
    expect(runbook).toContain('depends_on arch: :arm64');
    // The first bump by hand starts from the identifier release's tap: a
    // two-arch formula and a deprecated cask.
    expect(runbook).toMatch(/if\/else on the CPU/);
    expect(runbook).toContain('disable!');
    expect(runbook).toContain('keeping its date');
    expect(runbook).not.toContain("that day's date");
  });
});
