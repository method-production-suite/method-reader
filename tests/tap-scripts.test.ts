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

const CASK = `cask "screepub" do
  version "0.7.3"
  sha256 "${sha('c')}"

  url "https://github.com/ssandweiss/screepub/releases/download/v#{version}/Screepub-macOS.dmg"
  name "Screepub"
  homepage "https://github.com/ssandweiss/screepub"

  livecheck do
    url :url
    strategy :github_latest
  end

  deprecate! date: "2026-10-01", because: "is now downloaded from the releases page and updates itself"

  app "Screepub.app"
end
`;

/** A `gh` that answers from files in its own directory: the newest
 *  release's tag, the tap's two files (base64, as the contents API sends
 *  them), and per-asset digests for ONE release, the way GitHub reports
 *  them ("sha256:<hex>"). A tag it does not hold gets GitHub's 404 body on
 *  stdout and exit 1, which is exactly what the scripts must not mistake
 *  for a digest. */
function fakeGh(tag: string, opts: { release: string; assets: Record<string, string>; formula?: string; cask?: string }) {
  const dir = mkdtempSync(join(WORK, `gh-${tag}-`));
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
  const bumped = (v: string, arm: string): string => {
    const gh = fakeGh(`write-${v}`, { release: v, assets: { [ARM]: arm } });
    const tap = tapCheckout(`write-${v}`, twoArchFormula('0.7.4', sha('1'), sha('2')));
    expect(run('bump-tap.sh', [v, tap], gh).status).toBe(0);
    return readFileSync(join(tap, 'Formula', 'screepub.rb'), 'utf8');
  };

  test('an Apple silicon release served by the formula bump-tap.sh writes is fine', () => {
    const gh = fakeGh('ok', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', ARM_SHA), cask: CASK });
    const r = run('check-tap.sh', [], gh);
    expect(r.stdout).toContain('The tap serves 0.8.0');
    expect(r.status).toBe(0);
  });

  test('a formula still serving an Intel tarball the release does not carry is an error', () => {
    // A brew install on an Intel Mac would download a file that is not on
    // the release page.
    const gh = fakeGh('intel', {
      release: '0.8.0',
      assets: { [ARM]: ARM_SHA },
      formula: twoArchFormula('0.8.0', ARM_SHA, sha('2')),
      cask: CASK,
    });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain(INTEL);
  });

  test('an Apple silicon formula that does not SAY arm64 is an error', () => {
    // Without it brew would install an arm64 binary on an Intel Mac, where
    // it cannot run, instead of refusing with a reason.
    const formula = bumped('0.8.0', ARM_SHA).replace('  depends_on arch: :arm64\n', '');
    const gh = fakeGh('noarch', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula, cask: CASK });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain('depends_on arch: :arm64');
  });

  test('a wrong Apple silicon digest is an error', () => {
    const gh = fakeGh('wrongsha', {
      release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', sha('b')), cask: CASK,
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
    expect(run('check-tap.sh', [], ok).status).toBe(0);
    const badIntel = fakeGh('f2-bad', {
      release: '0.7.5', assets: { [ARM]: ARM_SHA, [INTEL]: sha('3') }, formula, cask: CASK,
    });
    const r = run('check-tap.sh', [], badIntel);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain(INTEL);
  });

  test('a cask with no deprecation is still an error', () => {
    const cask = CASK.replace(/\n {2}deprecate! [^\n]*\n/, '\n');
    const gh = fakeGh('nodeprecate', { release: '0.8.0', assets: { [ARM]: ARM_SHA }, formula: bumped('0.8.0', ARM_SHA), cask });
    const r = run('check-tap.sh', [], gh);
    expect(r.status).not.toBe(0);
    expect(r.stdout).toContain('deprecate!');
  });
});
