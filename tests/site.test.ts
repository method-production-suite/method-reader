// site/index.html, part 2 of the README and site redesign
// (docs/superpowers/specs/2026-09-22-readme-site-screens-design.md).
//
// Two changes, each pinned to its source rather than restated:
//
// - "Drag and drop" shows the real window, the same drop-and-result pair
//   tools/capture-screens.ts takes for the README, instead of a drawing. The
//   pictures are the capture tool's own site outputs, sized to the files it
//   wrote, and live under site/ because the Pages workflow deploys site/ and
//   nothing else.
// - "Every download" links each file by its version-free name through
//   /releases/latest/download/, with a status per file. The names come from
//   release.yml's matrix and BUNDLE_KINDS, the way release-artifacts.test.ts
//   derives them, and the statuses follow docs/verification-ledger.md, the one ledger of who
//   has verified what.
import { describe, expect, test } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { join, normalize, sep } from 'node:path';
import { BUNDLE_KINDS, kindsForOs, type BundleArch, type BundleOs } from '../tools/build-app-bundle';
import { SHOTS, outputsFor } from '../tools/capture/shots';

const ROOT = new URL('..', import.meta.url).pathname;
const SITE_DIR = join(ROOT, 'site');
const read = (p: string) => readFileSync(join(ROOT, p), 'utf8');
const site = read('site/index.html');

/** The markup of the first `<section class="<cls> ...">`, up to its close. */
function section(cls: string): string {
  const start = site.search(new RegExp(`<section class="${cls}[ "]`));
  expect(start).toBeGreaterThan(-1);
  return site.slice(start, site.indexOf('</section>', start));
}

/** A PNG's pixel size, from its IHDR chunk. */
function pngSize(path: string): { width: number; height: number } {
  const b = readFileSync(path);
  expect(b.subarray(1, 4).toString('latin1')).toBe('PNG');
  return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
}

/** Every attribute value in `html` for `name`, in order. */
function attrs(html: string, name: string): string[] {
  return [...html.matchAll(new RegExp(`\\s${name}="([^"]*)"`, 'g'))].map((m) => m[1]!);
}

describe('"Drag and drop" shows the real window', () => {
  const drop = section('drop');
  const imgs = [...drop.matchAll(/<img\b[^>]*>/g)].map((m) => m[0]);
  // What the capture tool writes for the site, in its own shot order.
  const captured = SHOTS.flatMap((shot) => outputsFor(shot, 'light'))
    .filter((p) => p.startsWith('site/'))
    .map((p) => p.slice('site/'.length));

  test('it shows the capture tool\'s site pictures, drop first, then the result', () => {
    expect(captured).toEqual(['img/drop-light.png', 'img/result-light.png']);
    expect(imgs.map((img) => attrs(img, 'src')[0])).toEqual(captured);
  });

  test('each picture is sized to its file, which the tool takes at 2x', () => {
    for (const img of imgs) {
      const { width, height } = pngSize(join(SITE_DIR, attrs(img, 'src')[0]!));
      expect(attrs(img, 'width')).toEqual([String(width / 2)]);
      expect(attrs(img, 'height')).toEqual([String(height / 2)]);
    }
  });

  test('each picture says what it shows', () => {
    for (const img of imgs) {
      const [alt] = attrs(img, 'alt');
      expect(alt?.length ?? 0).toBeGreaterThan(30);
    }
    expect(new Set(imgs.map((img) => attrs(img, 'alt')[0])).size).toBe(imgs.length);
  });

  test('the section keeps its words', () => {
    expect(drop).toContain('INT. THE APP - ONE STEP');
    expect(drop).toContain('<h2>Drag and drop.</h2>');
    expect(drop).toContain(
      'It\'s that simple. Fine tune anything you like afterwards, and see exactly what the device gets.',
    );
  });

  test('the drawing and its animation are gone', () => {
    for (const gone of ['id="well"', 'pdficon', 'DROP A SCREENPLAY HERE', 'runDrop', '.well{', 'dropTimer']) {
      expect(site).not.toContain(gone);
    }
  });
});

describe('the page only uses files that ship with it', () => {
  test('every local src and href resolves to a file inside site/', () => {
    // The Pages workflow deploys site/ alone, so a picture referenced from
    // assets/ (where the README's copies live) would 404 on screepub.com.
    const css = [...site.matchAll(/url\(([^)]+)\)/g)].map((m) => m[1]!.replace(/^["']|["']$/g, ''));
    const local = [...attrs(site, 'src'), ...attrs(site, 'href'), ...css].filter(
      (v) => !/^(https?:|mailto:|#|data:)/.test(v),
    );
    // The collector sees the pictures, the favicon and the self-hosted fonts.
    expect(local).toEqual(expect.arrayContaining(['img/drop-light.png', 'img/result-light.png', 'favicon.svg']));
    expect(local.filter((v) => v.startsWith('fonts/')).length).toBeGreaterThanOrEqual(6);
    for (const ref of local) {
      const path = normalize(join(SITE_DIR, ref.split(/[?#]/)[0]!));
      expect(path.startsWith(SITE_DIR + sep)).toBe(true);
      expect(existsSync(path)).toBe(true);
    }
  });
});

describe('"Every download" links version-free names, each with a status', () => {
  const downloads = section('downloads');
  const LATEST = 'https://github.com/ssandweiss/screepub/releases/latest/download/';

  // The words the README's install table uses too (spec part 1, item 9), so
  // the two pages say the same thing about the same file.
  const VERIFIED = 'Verified by a person';
  const NOT_YET = 'Built and checked automatically. Never installed by a person yet';

  // The window bundles, derived like release-artifacts.test.ts: the matrix
  // says which legs run, BUNDLE_KINDS what each leg's file is called.
  const rel = Bun.YAML.parse(read('.github/workflows/release.yml')) as {
    jobs: Record<string, { strategy?: { matrix?: { include?: { os?: string; arch?: string }[] } } }>;
  };
  const bundles = new Map<string, BundleOs>();
  for (const row of rel.jobs['app-bundles']!.strategy?.matrix?.include ?? []) {
    const os: BundleOs = row.os!.startsWith('ubuntu') ? 'linux' : row.os!.startsWith('macos') ? 'macos' : 'windows';
    for (const kind of kindsForOs(os)) bundles.set(kind.releasedName('0.0.0', row.arch as BundleArch), os);
  }
  // One Mac download since the identifier release: the window's image,
  // which the derivation above already finds.
  const MAC_DMG = 'Screepub-macOS.dmg';
  const files = [...bundles.keys()];

  /** The table row that offers `file`. */
  const row = (file: string): string => {
    const rows = [...downloads.matchAll(/<tr\b[\s\S]*?<\/tr>/g)].map((m) => m[0]);
    const mine = rows.filter((r) => r.includes(`href="${LATEST}${file}"`));
    expect(mine).toHaveLength(1);
    return mine[0]!;
  };

  test('the derivation found the four window bundles the release uploads', () => {
    // Guards the loops below against a matrix this parse misread.
    expect([...bundles.keys()].sort()).toEqual([
      'Screepub-linux-amd64.deb',
      'Screepub-linux-x86_64.rpm',
      'Screepub-macOS.dmg',
      'Screepub-windows-x64-setup.exe',
    ]);
    // And that the names really carry no version: the same file at two
    // versions has the same name, which is what makes /latest/download/ work.
    for (const kind of BUNDLE_KINDS) {
      expect(kind.releasedName('1.2.3', 'x64')).toBe(kind.releasedName('4.5.6', 'x64'));
    }
  });

  test('every file, the Mac app\'s included, is a link to the latest release by its own name', () => {
    for (const file of files) row(file);
  });

  test('no download link on the page carries a version', () => {
    const links = attrs(site, 'href').filter((h) => h.includes('/releases/'));
    expect(links.length).toBeGreaterThan(files.length);
    for (const link of links) {
      expect(link).not.toMatch(/\d+\.\d+\.\d+/);
      expect(link).not.toContain('/releases/download/');
    }
  });

  test('every file has a status', () => {
    for (const file of files) {
      const r = row(file);
      expect(r.includes(VERIFIED) || r.includes(NOT_YET)).toBe(true);
    }
  });

  test('the statuses follow docs/verification-ledger.md', () => {
    const ledger = read('docs/verification-ledger.md');
    const nobody = ledger.slice(ledger.indexOf('**Verified by nobody**'));
    expect(ledger.indexOf('**Verified by nobody**')).toBeGreaterThan(-1);
    // If one of these ledger lines goes, someone has done the thing: move
    // that file's row on the site up to match, then update this test.
    expect(nobody).toContain('Installing the `.deb`, the `.rpm` or the `.exe`');
    expect(nobody).toContain("The Intel SLICE of the identifier release's universal `.dmg`");
    for (const [file, os] of bundles) {
      if (os === 'macos') continue;
      expect(row(file)).toContain(NOT_YET);
    }
    // Installed by hand, and updated from a release, on an Apple Silicon
    // Mac. That is the evidence behind "verified by a person"; if the ledger
    // ever loses it, the claim goes too. Intel Macs are not
    // offered the app, so the row has nothing to say about them.
    const person = ledger.slice(
      ledger.indexOf('**Verified on a real machine, by a person**'),
      ledger.indexOf('**Verified only by CI'),
    );
    expect(person).toContain('dragged to `/Applications`');
    expect(person).toContain('0.7.2 to 0.7.3');
    expect(person).toMatch(/Each Mac that ran\s+these is Apple Silicon/);
    // The page sends a reader to the ledger itself.
    expect(downloads).toContain('docs/verification-ledger.md');
    const mac = row(MAC_DMG);
    expect(mac).toContain(VERIFIED);
    expect(mac).toContain('Apple Silicon');
    // Apple silicon only from the release after the identifier one: the row
    // no longer mentions Intel at all.
    expect(mac).not.toContain('Intel');
    // The pre-identifier-release name 404s now; no page may link it.
    expect(site).not.toContain('Screepub-Desktop-macOS-universal.dmg');
  });

  test('the sentence the status column replaced is gone', () => {
    // It said nobody had opened the window on anything but Linux, which
    // stopped being true at gate 1b on 2026-09-20.
    expect(downloads).not.toContain('anything but Linux');
    expect(downloads).not.toContain('Nobody has installed the new window yet');
  });
});

describe('no em dash anywhere on the site', () => {
  test('the page and the share card carry none, in any spelling', () => {
    for (const file of ['site/index.html', 'site/og-card.html']) {
      expect(read(file)).not.toMatch(/—|&mdash;|&#8212;|&#x2014;/i);
    }
  });
});
