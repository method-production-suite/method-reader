// tools/bump-tap.sh and tools/check-tap.sh, EXECUTED against a fake tap and
// a fake `gh`.
//
// release-artifacts.test.ts reads both scripts as text. That stopped being
// enough when the formula changed SHAPE: through the identifier release it
// served two tarballs from an `if Hardware::CPU.arm?` block, and from the
// release after it (Intel Macs not supported; owner, 2026-10-01; plan
// 2026-10-01-f2-handover-amendment.md) it serves one, and says
// `depends_on arch: :arm64`. bump-tap.sh has to turn the published
// two-arch formula into that, and check-tap.sh has to judge both shapes,
// each against the release it describes. Neither can be proved by grep.
//
// No network and no real tap: `gh` is a script FIRST on PATH answering
// from files, and the tap is a scratch directory. The formula fixture below
// is the published formula as read from the tap on 2026-10-01 (v0.7.4),
// with its version and digests swapped for made-up ones.
import { describe, test, expect, afterAll } from 'bun:test';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

const WORK = mkdtempSync(join(tmpdir(), 'screepub-tap-scripts-'));
afterAll(() => rmSync(WORK, { recursive: true, force: true }));
const BASH = Bun.which('bash');
if (!BASH) throw new Error('tap-scripts: no bash on PATH');
const REPO_ROOT = join(import.meta.dir, '..');

const ARM = 'screepub-cli-macos-arm64.tar.gz';
const INTEL = 'screepub-cli-macos-x64.tar.gz';
const sha = (c: string) => c.repeat(64);

/** The formula as the tap served it through the identifier release. */
const twoArchFormula = (v: string, arm: string, x64: string) => `class Screepub < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/ssandweiss/screepub"
  # Per-arch tarballs, not one universal binary.
  if Hardware::CPU.arm?
    url "https://github.com/ssandweiss/screepub/releases/download/v${v}/${ARM}"
    sha256 "${arm}"
  else
    url "https://github.com/ssandweiss/screepub/releases/download/v${v}/${INTEL}"
    sha256 "${x64}"
  end
  license "AGPL-3.0-or-later"

  livecheck do
    url "https://github.com/ssandweiss/screepub/releases/latest"
    strategy :github_latest
  end

  depends_on :macos

  def install
    bin.install "screepub"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/screepub --version")
  end
end
`;

/** The retired cask, before its retirement notice. */
const BARE_CASK = `cask "screepub" do
  version "0.7.3"
  sha256 "${sha('c')}"

  url "https://github.com/ssandweiss/screepub/releases/download/v#{version}/Screepub-macOS.dmg"
  name "Screepub"
  homepage "https://github.com/ssandweiss/screepub"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "Screepub.app"
end
`;

/** The cask as the identifier release's tap job leaves it: deprecated, on
 *  the day that job ran. */
const DEPRECATED_ON = '2026-09-30';
const CASK = BARE_CASK.replace(
  '  end\n\n  app',
  `  end\n\n  deprecate! date: "${DEPRECATED_ON}", because: "is now downloaded from the releases page and updates itself"\n\n  app`,
);

/** A `gh` that answers from files in its own directory: the newest
 *  release's tag, the tap's two files (base64, as the contents API sends
 *  them), and per-asset digests for ONE release, the way GitHub reports
 *  them ("sha256:<hex>"). A tag it does not hold gets GitHub's 404 body on
 *  stdout and exit 1, which is exactly what the scripts must not mistake
 *  for a digest. Asked for the whole asset list (a -q with no select),
 *  it answers "name sha256:<hex>" per asset. With `flaky`, every query
 *  that could reveal the Intel tarball (the list, or a lookup by that
 *  name) fails the way a transient API error does: nothing on stdout,
 *  exit 1. */
function fakeGh(
  tag: string,
  opts: { release: string; assets: Record<string, string>; formula?: string; cask?: string; flaky?: boolean },
) {
  const dir = mkdtempSync(join(WORK, `gh-${tag}-`));
  if (opts.flaky) writeFileSync(join(dir, 'flaky'), '');
  writeFileSync(join(dir, 'newest'), `v${opts.release}\n`);
  writeFileSync(join(dir, 'release'), `v${opts.release}`);
  writeFileSync(
    join(dir, 'assets.tsv'),
    Object.entries(opts.assets).map(([n, d]) => `${n}\t${d}\n`).join(''),
  );
  if (opts.formula !== undefined) writeFileSync(join(dir, 'formula.rb'), opts.formula);
  if (opts.cask !== undefined) writeFileSync(join(dir, 'cask.rb'), opts.cask);
  const script = join(dir, 'gh');
  writeFileSync(
    script,
    `#!/bin/bash
here="$(dirname "$0")"
if [ "$1" = release ] && [ "$2" = list ]; then cat "$here/newest"; exit 0; fi
if [ "$1" = api ]; then
  path="$2"
  case "$path" in
    */contents/Formula/screepub.rb) base64 < "$here/formula.rb"; exit 0 ;;
    */contents/Casks/screepub.rb) base64 < "$here/cask.rb"; exit 0 ;;
    */releases/tags/*)
      if [ "\${path##*/}" != "$(cat "$here/release")" ]; then
        echo '{"message":"Not Found","status":"404"}'; exit 1
      fi
      q=""
      while [ $# -gt 0 ]; do
        if [ "$1" = -q ]; then q="$2"; fi
        shift
      done
      name="$(printf '%s' "$q" | sed -n 's/.*select(.name == "\\([^"]*\\)").*/\\1/p')"
      if [ -e "$here/flaky" ] && { [ -z "$name" ] || [ "$name" = screepub-cli-macos-x64.tar.gz ]; }; then
        echo "gh: HTTP 502: Bad Gateway" >&2; exit 1
      fi
      if [ -z "$name" ]; then
        awk -F'\\t' '{ print $1 " sha256:" $2 }' "$here/assets.tsv"
        exit 0
      fi
      awk -F'\\t' -v n="$name" '$1 == n { print "sha256:" $2 }' "$here/assets.tsv"
      exit 0 ;;
  esac
fi
echo "fake gh: unexpected: $*" >&2
exit 1
`,
  );
  chmodSync(script, 0o755);
  return dir;
}

function tapCheckout(tag: string, formula: string, cask: string = CASK): string {
  const tap = mkdtempSync(join(WORK, `tap-${tag}-`));
  mkdirSync(join(tap, 'Formula'));
  mkdirSync(join(tap, 'Casks'));
  writeFileSync(join(tap, 'Formula', 'screepub.rb'), formula);
  writeFileSync(join(tap, 'Casks', 'screepub.rb'), cask);
  return tap;
}

const run = (script: string, args: string[], ghDir: string) =>
  spawnSync(BASH!, [join(REPO_ROOT, 'tools', script), ...args], {
    cwd: REPO_ROOT,
    env: {
      PATH: `${ghDir}:${dirname(Bun.which('awk') ?? '/usr/bin/awk')}:/usr/bin:/bin`,
      HOME: ghDir,
    },
    encoding: 'utf8',
  });

describe('tools/bump-tap.sh, run against a tap', () => {
  const ARM_SHA = sha('a');

  test('it turns the two-arch formula into one Apple silicon formula', () => {
    const gh = fakeGh('bump', { release: '0.8.0', assets: { [ARM]: ARM_SHA } });
    const tap = tapCheckout('bump', twoArchFormula('0.7.4', sha('1'), sha('2')));
    const r = run('bump-tap.sh', ['v0.8.0', tap], gh);
    expect(r.stderr).toBe('');
    expect(r.status).toBe(0);
    const formula = readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8');
    const urls = [...formula.matchAll(/^\s*url "([^"]+)"/gm)].map((m) => m[1]);
    // The download, and livecheck's own url; nothing for Intel.
    expect(urls).toEqual([
      `https://github.com/ssandweiss/screepub/releases/download/v0.8.0/${ARM}`,
      'https://github.com/ssandweiss/screepub/releases/latest',
    ]);
    expect(formula).toContain(`sha256 "${ARM_SHA}"`);
    expect(formula.match(/sha256 "/g)).toHaveLength(1);
    expect(formula).not.toContain(INTEL);
    expect(formula).not.toContain('Hardware::CPU');
    // Homebrew's spelling (ArchRequirement: arm64 is satisfied by
    // Hardware::CPU.arm64?), and before `depends_on :macos`: brew style's
    // DependencyOrder sorts dependencies by name, and "arch" < "macos".
    const arch = formula.indexOf('  depends_on arch: :arm64\n');
    const macos = formula.indexOf('  depends_on :macos\n');
    expect(arch).toBeGreaterThan(-1);
    expect(macos).toBeGreaterThan(arch);
    // The formula still has no version stanza: brew reads it from the url.
    expect(formula).not.toMatch(/^\s*version "/m);
    expect(r.stdout).toContain('tap rewritten to 0.8.0');
    // The cask: the identifier release DEPRECATED it; from the release
    // after it, it is DISABLED, in the same place. It stays pinned to the
    // last Swift release, whose updater would offer this Apple-silicon-only
    // release to an Intel Mac that installed it, so it must not install.
    const cask = readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8');
    expect(cask).not.toMatch(/^ {2}deprecate! /m);
    // The deprecation's OWN date, already past. Homebrew compares the date
    // with the user's LOCAL day (cask/dsl.rb: a date after Date.today only
    // deprecates), so a date of "today in UTC" left the cask installable
    // in the Americas for a US-evening tag.
    expect(cask).toMatch(
      new RegExp(
        `^ {2}disable! date: "${DEPRECATED_ON}", because: "is now downloaded from the releases page and updates itself"$`,
        'm',
      ),
    );
    expect(cask.replace(/^ {2}disable! [^\n]*$/m, 'NOTICE')).toBe(
      CASK.replace(/^ {2}deprecate! [^\n]*$/m, 'NOTICE'),
    );
  });

  test('a cask with no retirement notice yet gets the disable! after its livecheck block', () => {
    // Homebrew's stanza order (Cask/StanzaOrder) groups deprecate! and
    // disable! together, right after livecheck.
    const gh = fakeGh('bare', { release: '0.8.0', assets: { [ARM]: ARM_SHA } });
    const tap = tapCheckout('bare', twoArchFormula('0.7.4', sha('1'), sha('2')), BARE_CASK);
    expect(run('bump-tap.sh', ['0.8.0', tap], gh).status).toBe(0);
    const cask = readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8');
    expect(cask).toMatch(/ {2}livecheck do\n[\s\S]*?\n {2}end\n\n {2}disable! date: "[0-9-]+", because: "[^"]+"\n\n {2}app "Screepub\.app"/);
    // A fixed date, already past everywhere: the day Intel was dropped.
    // Not "today", for the time-zone reason above, and fixed so a second
    // run writes the same bytes.
    expect(cask).toContain('  disable! date: "2026-10-01", ');
  });

  test('a release whose asset list cannot be read writes nothing', () => {
    // One read of the asset list decides both the digest and whether the
    // release still serves Intel. A transient API failure must stop the
    // bump: read separately with `|| true`, a failed Intel lookup used to
    // look like "no Intel tarball" and let the identifier release be
    // bumped Apple-silicon-only.
    const gh = fakeGh('flaky', { release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('2') }, flaky: true });
    const before = twoArchFormula('0.7.4', sha('1'), sha('2'));
    const tap = tapCheckout('flaky', before);
    const r = run('bump-tap.sh', ['0.7.5', tap], gh);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain('v0.7.5');
    expect(readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8')).toBe(before);
    expect(readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8')).toBe(CASK);
  });

  test('a release that still carries the Intel tarball is refused, and nothing is written', () => {
    // The identifier release and older serve Intel through Homebrew. Bumped
    // with THIS script (from a main that already has it), the tap would
    // drop Intel brew users a release early.
    const gh = fakeGh('f2', { release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('2') } });
    const before = twoArchFormula('0.7.4', sha('1'), sha('2'));
    const tap = tapCheckout('f2', before);
    const r = run('bump-tap.sh', ['0.7.5', tap], gh);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain(INTEL);
    expect(r.stderr).toContain('git show v0.7.5:tools/bump-tap.sh');
    expect(readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8')).toBe(before);
    expect(readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8')).toBe(CASK);
  });

  test('a second run at the same version changes nothing', () => {
    // The script's own rule: bumping to the version the tap already
    // serves is an empty diff.
    const gh = fakeGh('again', { release: '0.8.0', assets: { [ARM]: ARM_SHA } });
    const tap = tapCheckout('again', twoArchFormula('0.7.4', sha('1'), sha('2')));
    expect(run('bump-tap.sh', ['0.8.0', tap], gh).status).toBe(0);
    const once = readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8');
    const caskOnce = readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8');
    expect(run('bump-tap.sh', ['0.8.0', tap], gh).status).toBe(0);
    expect(readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8')).toBe(once);
    expect(readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8')).toBe(caskOnce);
  });

  test('a release without the Apple silicon tarball writes nothing', () => {
    const gh = fakeGh('noarm', { release: '0.8.0', assets: { [INTEL]: sha('2') } });
    const before = twoArchFormula('0.7.4', sha('1'), sha('2'));
    const tap = tapCheckout('noarm', before);
    const r = run('bump-tap.sh', ['0.8.0', tap], gh);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain(ARM);
    expect(readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8')).toBe(before);
  });

  test('a tag with no release writes nothing, rather than a 404 body as a digest', () => {
    const gh = fakeGh('norelease', { release: '0.8.0', assets: { [ARM]: ARM_SHA } });
    const before = twoArchFormula('0.7.4', sha('1'), sha('2'));
    const tap = tapCheckout('norelease', before);
    expect(run('bump-tap.sh', ['0.9.9', tap], gh).status).not.toBe(0);
    expect(readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8')).toBe(before);
  });
});

describe('tools/check-tap.sh, judging the published tap', () => {
  const ARM_SHA = sha('a');

  /** What bump-tap.sh writes, read back from a real run, so the two
   *  scripts are tested against each other and not against a guess. */
  const bumpedTap = (v: string, arm: string): { formula: string; cask: string } => {
    const gh = fakeGh(`write-${v}-${arm[0]}`, { release: v, assets: { [ARM]: arm } });
    const tap = tapCheckout(`write-${v}-${arm[0]}`, twoArchFormula('0.7.4', sha('1'), sha('2')));
    expect(run('bump-tap.sh', [v, tap], gh).status).toBe(0);
    return {
      formula: readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8'),
      cask: readFileSync(join(tap, 'Casks', 'screepub.rb'), 'utf8'),
    };
  };
  const bumped = (v: string, arm: string): string => bumpedTap(v, arm).formula;
  /** The cask bump-tap.sh leaves: disabled. Made per test, not at collection. */
  const disabledCask = (): string => bumpedTap('0.8.0', sha('a')).cask;

  test('an Apple silicon release served by the tap bump-tap.sh writes is fine', () => {
    const tap = bumpedTap('0.8.0', ARM_SHA);
    const gh = fakeGh('ok', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: tap.formula, cask: tap.cask });
    const r = run('check-tap.sh', [], gh);
    expect(r.stdout).toContain('The tap serves 0.8.0');
    expect(r.stdout).toContain('retired (disabled)');
    expect(r.status).toBe(0);
  });

  test('an Apple silicon release whose cask is only deprecated is an error', () => {
    // The deprecated cask still installs the last Swift release, whose
    // updater would offer this release to an Intel Mac. It must be disabled.
    const gh = fakeGh('deponly', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', ARM_SHA), cask: CASK });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain('disable!');
  });

  test('a formula still serving an Intel tarball the release does not carry is an error', () => {
    // A brew install on an Intel Mac would download a file that is not on
    // the release page.
    const gh = fakeGh('intel', {
      release: '0.8.0',
      assets: { [ARM]: ARM_SHA },
      formula: twoArchFormula('0.8.0', ARM_SHA, sha('2')),
      cask: disabledCask(),
    });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain(INTEL);
  });

  test('an Apple silicon formula that does not SAY arm64 is an error', () => {
    // Without it brew would install an arm64 binary on an Intel Mac, where
    // it cannot run, instead of refusing with a reason.
    const formula = bumped('0.8.0', ARM_SHA).replace('  depends_on arch: :arm64\n', '');
    const gh = fakeGh('noarch', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula, cask: disabledCask() });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain('depends_on arch: :arm64');
  });

  test('a wrong Apple silicon digest is an error', () => {
    const gh = fakeGh('wrongsha', {
      release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', sha('b')), cask: disabledCask(),
    });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain(ARM);
  });

  test('the identifier release, which still carries an Intel tarball, is judged as it shipped', () => {
    // The weekly check reads the NEWEST release. Between merging this and
    // tagging the first Apple-silicon-only release, that is the identifier
    // release, served by its two-arch formula: that must stay green, both
    // digests still checked.
    const formula = twoArchFormula('0.7.5', ARM_SHA, sha('2'));
    const ok = fakeGh('f2', { release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('2') }, formula, cask: CASK });
    const okRun = run('check-tap.sh', [], ok);
    expect(okRun.status).toBe(0);
    expect(okRun.stdout).toContain('retired (deprecated)');
    // Its cask is deprecated, not disabled: a disabled cask while the
    // identifier release is newest means the tap ran ahead of the release.
    const early = fakeGh('f2-early', {
      release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('2') }, formula, cask: disabledCask(),
    });
    const earlyRun = run('check-tap.sh', [], early);
    expect(earlyRun.status).not.toBe(0);
    expect(earlyRun.stdout).toContain('deprecate!');
    const badIntel = fakeGh('f2-bad', {
      release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('3') }, formula, cask: CASK,
    });
    const r = run('check-tap.sh', [], badIntel);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain(INTEL);
  });

  test('an Apple silicon release whose cask says BOTH deprecate! and disable! is an error', () => {
    // Homebrew asks deprecated? before disabled?, so a cask carrying both
    // lines still installs.
    const both = disabledCask().replace(
      /^( {2}disable! [^\n]*)$/m,
      '  deprecate! date: "2026-09-30", because: "is now downloaded from the releases page and updates itself"\n$1',
    );
    expect(both).toMatch(/^ {2}deprecate! /m);
    expect(both).toMatch(/^ {2}disable! /m);
    const gh = fakeGh('both', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', ARM_SHA), cask: both });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toMatch(/deprecate!.*still installs|still installs.*deprecate!/);
  });

  test('a cask with no retirement notice at all is an error, before and after', () => {
    const after = fakeGh('bare-after', {
      release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', ARM_SHA), cask: BARE_CASK,
    });
    const a = run('check-tap.sh', [], after);
    expect(a.status).not.toBe(0);
    expect(a.stdout).toContain('disable!');
    const before = fakeGh('bare-before', {
      release: '0.7.5',
      assets: { [ARM]: ARM_SHA, [INTEL]: sha('2') },
      formula: twoArchFormula('0.7.5', ARM_SHA, sha('2')),
      cask: BARE_CASK,
    });
    const b = run('check-tap.sh', [], before);
    expect(b.status).not.toBe(0);
    expect(b.stdout).toContain('deprecate!');
  });
});
