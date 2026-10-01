import { describe, test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * THE SWIFT APP STAYS RETIRED. What is left of the reference sweep from
 * piece F1 of docs/superpowers/specs/2026-09-14-retire-swiftui-design.md.
 *
 * From F1 to F3 this test pinned tools/app-references.json: every file
 * outside app/ that named the SwiftUI Mac app, with what would break when
 * it went, so the deletion's blast radius was written down before the day
 * it happened. Piece F3 worked that list to empty (acceptance criterion
 * 11), deleted the directory in one commit of its own ("F3: git rm -r
 * app/"), and deleted the empty list with it. There is no blast radius
 * left to record, so the job changed from "write it down" to "it does not
 * grow back", in two halves:
 *
 *   1. app/ holds nothing git would commit. The plans under
 *      docs/superpowers/ still edit app/Sources/... by line number, because
 *      they are history; a session that follows one would quietly recreate
 *      the directory, and this is where that stops.
 *   2. Nothing outside the dated records names the Swift app. A new
 *      reference is a new coupling to code that is not there: a comment
 *      pointing a reader at a deleted file, a doc telling someone to run a
 *      build that cannot run, a workflow step that fails on a Monday.
 *
 * ── WHAT COUNTS AS A REFERENCE ────────────────────────────────────────
 *
 * The obvious sweep, a grep for the literal string `app/`, is wrong in
 * both directions, and both were measured in this tree at F1:
 *
 *   MISSES. weekly-toolchain.yml's Swift job was `swift run -c release
 *   kit-check` with `working-directory: app`. There is no slash, so a grep
 *   for `app/` did not see a whole macOS job. Likewise `swift build` and
 *   the bare Swift-file provenance comments that src/ used to carry.
 *
 *   FALSE POSITIVES. release.yml's bundle jobs used to say `path: app` and
 *   `cd app` for the TAURI bundle staging directory (renamed `arrivals` on
 *   2026-09-21), nothing to do with the Swift app. A sweep that flagged
 *   them would have told the next person to delete working code, which is
 *   how a guard earns its own deletion.
 *
 * So a reference is NOT textual proximity to "app". It is:
 *
 *   an occurrence, in a file outside the dated records, of a token that
 *   names the Swift app itself: its directory, a Swift source file, a
 *   Swift module or the kit-check executable, or a command that builds it.
 *
 * which is the PATTERNS list below. `cd app` is deliberately not in it.
 *
 * Provenance is still allowed, in words: "the Swift app's converter", with
 * any tag before F3 (v0.7.4, say; the releases between it and F3 still
 * carry app/ too) or the porting commit for a reader who wants the
 * original. That is how F3 rewrote every comment it found, and it is what
 * the failure message below asks for.
 *
 * Which means the two Swift patterns ban Swift anywhere outside the dated
 * records, not only the old app's. That is right today, when no Swift is
 * meant to exist here. If the window ever needs a Tauri macOS plugin
 * written in Swift, these patterns will flag it, and the fix is an
 * allowance here for that plugin's directory, not a looser pattern.
 *
 * ── WHAT IS NOT SWEPT, AND WHY ────────────────────────────────────────
 *
 * Dated records. docs/superpowers/ (specs, plans, run logs), docs/adr/ and
 * docs/releases/ describe what was decided or shipped on a given date.
 * Deleting app/ did not make a 2026-08-04 plan wrong; rewriting one to hide
 * that the Mac app existed would be worse than a dangling path, and the
 * spec is explicit that the sweep must exclude them "or it will fight them
 * forever".
 *
 * docs/retired-coverage.md, since F3. It is the gate-2 record of what the
 * Swift app's checks covered and what became of each, so its whole job is
 * to name Swift sources and kit-check line ranges. With the directory gone
 * it is a dated record like the plans, and rewriting it to avoid the names
 * would erase what it records.
 *
 * docs/history/, the dated build notes and transcripts the window's docs
 * point at for how it was built. Same reasoning as the plans.
 *
 * app/ itself. The first half answers for it; sweeping the same files here
 * too would report a leftover file twice, the second time with the wrong
 * advice.
 *
 * This file, which names every token in the list by construction.
 */

const REPO = new URL('..', import.meta.url).pathname;

/** The dated records, app/ (half 1 answers for it) and this file; see above. */
const NOT_SWEPT = [
  /^app\//,
  /^docs\/superpowers\//,
  /^docs\/history\//,
  /^docs\/adr\//,
  /^docs\/releases\//,
  /^docs\/retired-coverage\.md$/,
  /^tests\/app-references\.test\.ts$/,
];

/** Binary and generated files: no prose to rot, and readFileSync('utf8') lies about them. */
const NOT_TEXT = /\.(pdf|png|jpg|jpeg|zip|ico|icns|woff2?|ttf|otf|lock)$/;

const PATTERNS: { name: string; re: RegExp; what: string }[] = [
  {
    name: 'path',
    // Not preceded by a path or word character, so desktop/app/ or `myapp/`
    // would not match; `app-bundles-*` has no slash and does not either.
    re: /(?<![A-Za-z0-9_./-])app\//g,
    what: 'a path into the deleted directory',
  },
  {
    name: 'workdir',
    // Anchored to end of line: this is the YAML form that has no slash and
    // that a naive sweep misses entirely. `cd app` is excluded on purpose.
    re: /working-directory:[ \t]*app[ \t]*(?=\n|$)/gm,
    what: 'a workflow step that runs inside the deleted directory',
  },
  { name: 'swift-symbol', re: /ScreepubKit|ScreepubApp|KitCheck|kit-check|KFXKit/g, what: 'a Swift module or the kit-check executable' },
  { name: 'swift-file', re: /\b[A-Za-z][A-Za-z0-9-]*\.swift\b/g, what: 'a Swift source file' },
  { name: 'swift-build', re: /swift (?:build|run)|build-app\.sh|build-lib\.sh/g, what: 'a command that builds the Swift app' },
  // There was a fifth rule here, `swift-artifact`, matching Screepub.app and
  // Screepub-macOS. At the identifier release the window took both names
  // (its bundle is Screepub.app, its image Screepub-macOS.dmg), so neither
  // says "the Swift app" any more. The Swift app's own artifacts lived under
  // app/dist/, which the path rule above catches.
];

/** Only what git already tracks under the given pathspecs. */
function gitTracked(...pathspecs: string[]): string[] {
  const proc = Bun.spawnSync(['git', 'ls-files', '-z', '--cached', '--', ...pathspecs], { cwd: REPO });
  if (!proc.success) throw new Error('git ls-files failed: ' + proc.stderr.toString());
  return proc.stdout.toString().split('\0').filter(Boolean);
}

/** git ls-files with the given pathspecs: TRACKED, plus untracked and not
 *  ignored.
 *
 *  `--others --exclude-standard` is not tidiness, it closes a hole that let
 *  a file through on 2026-09-21. A bare `git ls-files` lists only TRACKED
 *  files, so a brand-new one is invisible until it is staged, and a
 *  brand-new file is the likeliest kind to carry a new reference (or, for
 *  half 1, to be the directory coming back). desktop/ui/feedback.js passed
 *  a full green suite and then failed the moment it was committed, because
 *  `git add` is what made it visible here. `--exclude-standard` keeps
 *  .gitignore honoured, so build output, the 2.5 GB target/ and the
 *  gitignored real-script fixtures stay out. */
function gitFiles(...pathspecs: string[]): string[] {
  const proc = Bun.spawnSync(
    ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', ...(pathspecs.length ? ['--', ...pathspecs] : [])],
    { cwd: REPO },
  );
  if (!proc.success) throw new Error('git ls-files failed: ' + proc.stderr.toString());
  return proc.stdout.toString().split('\0').filter(Boolean);
}

function countIn(text: string): number {
  let n = 0;
  for (const p of PATTERNS) n += (text.match(p.re) ?? []).length;
  return n;
}

/** path -> reference count, for everything swept that has any. */
function scan(): Map<string, number> {
  const found = new Map<string, number>();
  for (const f of gitFiles()) {
    if (NOT_SWEPT.some((r) => r.test(f))) continue;
    if (NOT_TEXT.test(f)) continue;
    let text: string;
    try {
      text = readFileSync(join(REPO, f), 'utf8');
    } catch {
      continue;
    }
    if (text.includes('\0')) continue;
    const n = countIn(text);
    if (n > 0) found.set(f, n);
  }
  return found;
}

const FIX = [
  '',
  'The SwiftUI Mac app was deleted at piece F3. Nothing outside the dated',
  'records (docs/superpowers/, docs/adr/, docs/releases/,',
  'docs/retired-coverage.md) may point at it.',
  '',
  'If you are following a plan under docs/superpowers/ that edits app/, the',
  'plan is history: the code it edits is gone. Do the work in src/ (the',
  'engine) or desktop/ (the window).',
  '',
  'If the reference is provenance, say it in words: "the Swift app\'s',
  'converter", and for a reader who wants the original, any tag before F3',
  '(v0.7.4, say) or the commit that ported it. That is how F3 rewrote every',
  'comment it found.',
].join('\n');

function fail(title: string, lines: string[]): never {
  throw new Error([title, ...lines.map((l) => '  ' + l), FIX].join('\n'));
}

describe('the Swift app stays retired', () => {
  test('app/ holds nothing git would commit', () => {
    // IGNORED files do not count, so a Mac that built the Swift app before
    // F3 passes as it stands: its leftover build folders are named in
    // .gitignore (pinned by the last test in this block). Anything else
    // that turns up is either the directory coming back into git, or a
    // stray file git would add, and the two get different advice.
    const files = gitFiles('app');
    if (files.length) {
      const tracked = gitTracked('app');
      fail(
        tracked.length
          ? 'app/ is back in git. It was deleted at F3 and must stay gone.'
          : 'app/ holds files that git would add. If they are left over on this Mac from ' +
              'before F3 and you did not mean to add them, delete the folder: rm -rf app. ' +
              'If you are recreating the Swift app, read on.',
        (tracked.length ? tracked : files).slice(0, 20),
      );
    }
    expect(files).toEqual([]);
  });

  test('nothing outside the dated records names it', () => {
    const found = scan();
    if (found.size) {
      fail('Files name the retired Swift app.', [
        ...[...found.keys()].sort().map((f) => {
          const text = readFileSync(join(REPO, f), 'utf8');
          const kinds = PATTERNS.filter((p) => (text.match(p.re) ?? []).length).map((p) => p.what);
          return `${f}: ${found.get(f)} reference(s): ${kinds.join('; ')}`;
        }),
      ]);
    }
    expect([...found.keys()]).toEqual([]);
  });

  test('the build output the Swift app left on Macs that built it is ignored, not reported', () => {
    // Measured at review, 2026-10-01: Sam's main checkout and the
    // layout-pass worktree each hold a 407 MB app/.build/ (SwiftPM output),
    // plus app/build/ and app/dist/. Once git stops tracking app/, anything
    // there that is not ignored is "untracked": both halves above would fail
    // on it with advice about recreating the app, every run would read
    // thousands of build files, and a `git add -A` would stage them. So the
    // ignore rules must cover every folder SwiftPM and the old build scripts
    // wrote, including a nested package's own .build.
    const ignored = (path: string) =>
      Bun.spawnSync(['git', 'check-ignore', '-q', '--no-index', path], { cwd: REPO }).exitCode === 0;
    for (const p of ['app/.build/x', 'app/build/x', 'app/dist/x', 'app/Packages/KFXKit/.build/x']) {
      expect(ignored(p), `${p} is not ignored`).toBe(true);
    }
    // And no wider: a rule that hid app/ itself would blind the first half.
    expect(ignored('app/Sources/Probe.swift')).toBe(false);
    expect(ignored('app/Package.swift')).toBe(false);
  });
});

describe('the sweep is not vacuous', () => {
  // An empty scan is the PASSING state now, so a scanner that saw nothing
  // would pass too. These are what tell the two apart.

  test('a file that is not committed yet is still swept', () => {
    // The hole that let desktop/ui/feedback.js through on 2026-09-21; see
    // gitFiles. Asserted against the flags rather than by writing a file
    // into the repository mid-suite, because a test that creates one and
    // crashes leaves it behind for the next run to trip over.
    const source = readFileSync(join(REPO, 'tests', 'app-references.test.ts'), 'utf8');
    expect(source).toContain("'--others'");
    expect(source).toContain("'--exclude-standard'");
    // And the ordinary case is seen, or the flags above could be satisfied
    // by a list that is somehow empty.
    expect(gitFiles()).toContain('README.md');
  });

  test('the patterns actually match the forms that a naive grep misses', () => {
    // If someone weakens PATTERNS, an empty scan stays green. These are the
    // forms that motivated the pattern list, asserted directly.
    expect(countIn('      - run: swift run -c release kit-check\n        working-directory: app\n')).toBeGreaterThanOrEqual(3);
    expect(countIn('Mirrors EbookConvert.swift exactly')).toBe(1);
    expect(countIn('see `app/release.sh`')).toBe(1);
    expect(countIn('app/dist/Screepub-macOS.dmg')).toBe(1);
  });

  test('provenance in words, the way F3 rewrote it, is not a reference', () => {
    // The failure message tells people to write it this way, so this way
    // must pass, or the guard argues with its own advice.
    expect(countIn("matching the Swift app's converter exactly")).toBe(0);
    expect(countIn('the Swift app was deleted at piece F3; see it at tag v0.7.4')).toBe(0);
  });

  test('the names the window took over at the identifier release are not references', () => {
    // Screepub.app is the window's bundle and Screepub-macOS.dmg its image
    // since the handover. Counting them would tell people to delete the
    // window's own download links.
    expect(countIn('download/Screepub-macOS.dmg')).toBe(0);
    expect(countIn('/Applications/Screepub.app')).toBe(0);
  });

  test('the patterns do not match a staging directory that happens to be called app', () => {
    // The false-positive half. release.yml used one until 2026-09-21; if any
    // of these ever counts, the sweep starts telling people to delete
    // working release machinery.
    expect(countIn('          path: app\n')).toBe(0);
    expect(countIn('          cd app\n          sha256sum *.deb\n')).toBe(0);
    expect(countIn('pattern: app-bundles-*\n')).toBe(0);
    expect(countIn('working-directory: desktop/src-tauri\n')).toBe(0);
  });

  test('the sweep covers the whole tree, not an empty slice of it', () => {
    // A broken NOT_SWEPT list, a failed git ls-files, or a cwd mistake all
    // show up here rather than as a mysteriously green sweep.
    const files = gitFiles().filter((x) => !NOT_SWEPT.some((r) => r.test(x)));
    expect(files.length).toBeGreaterThan(150);
    expect(files).toContain('README.md');
    expect(files).toContain('CLAUDE.md');
    expect(files).toContain('.github/workflows/weekly-toolchain.yml');
    expect(files.some((x) => x.startsWith('docs/superpowers/'))).toBe(false);
    expect(files.some((x) => x.startsWith('docs/history/'))).toBe(false);
    expect(files).not.toContain('docs/retired-coverage.md');
  });
});

describe('docs/retired-coverage.md — the coverage that bun test does not have', () => {
  const DOC = join(REPO, 'docs', 'retired-coverage.md');
  const text = readFileSync(DOC, 'utf8');

  /**
   * F1 acceptance criterion 3: every unreplaced kit-check section is listed
   * with an explicit decision. This is the mechanism that made gate 2
   * checkable rather than remembered, the spec's words, and since F3 it
   * keeps the record of what the deletion gave up from being edited away.
   *
   * The sections are named here, in the test, rather than read out of the
   * document: a test that derived its expectations from the document under
   * test would pass against an empty document. Each id below is one row of
   * the spec's "What was measured" item 2, with the check count that item
   * measured, so dropping a section, or quietly shrinking one, fails here
   * rather than in a reviewer's memory.
   */
  const SECTIONS: [string, number][] = [
    ['send-menu', 45],
    ['self-update-installer', 26],
    ['updater-version-compare', 17],
    ['update-selection', 17],
    ['update-decoding', 14],
    ['update-error-descriptions', 11],
    ['engine-cancellation', 10],
    ['mail-and-books', 5],
    ['feedback-url', 5],
    ['release-notes-parsing', 21],
  ];

  test('every unreplaced kit-check section is present with its check count', () => {
    const missing: string[] = [];
    for (const [section, checks] of SECTIONS) {
      // The section must have its own heading AND state its check count on
      // that heading line: a passing mention in a sentence is not a record.
      // Anywhere in a level-2 heading: the four pure UpdateCheck sections
      // share one heading on purpose (they are one product question), and
      // splitting them into four would duplicate the prose, not clarify it.
      const heading = new RegExp(`^## .*\`${section}\`.*$`, 'm');
      const line = text.match(heading)?.[0];
      if (!line) missing.push(`${section}: no "## \`${section}\`" section heading`);
      else if (!line.includes(String(checks))) missing.push(`${section}: heading does not state its ${checks} checks — "${line}"`);
    }
    if (missing.length) fail('docs/retired-coverage.md is missing unreplaced kit-check sections.', missing);
    expect(missing).toEqual([]);
  });

  test('every section carries an explicit decision', () => {
    // `port` or `accept-loss`, the spec's two words. A section listed with
    // neither is the "we'll decide later" that gate 2 exists to prevent.
    const rows = text.split('\n').filter((l) => l.startsWith('| `'));
    expect(rows.length).toBeGreaterThanOrEqual(SECTIONS.length);
    for (const row of rows) {
      expect(row, `no port/accept-loss decision in: ${row.slice(0, 60)}`).toMatch(/`port`|`accept-loss`/);
    }
  });

  test('the decisions are marked as recommendations, not as settled', () => {
    // The standing correction in this program is overclaiming. Nobody in
    // this repository had the standing to decide that a shipping feature
    // was gone; the document's job was to make deciding cheap, and it must
    // say so where a skimmer sees it.
    expect(text.toLowerCase()).toContain('recommendation');
    expect(text).toContain('RECOMMENDED');
    expect(text.toLowerCase()).toMatch(/not (yet )?(a )?decision|awaiting|has not been decided|no decision/);
  });

  test('it says what a user loses, not only what a test loses', () => {
    // Sections are features. The spec asks for the product question, and a
    // document that only counted assertions would answer the wrong one.
    expect(text).toContain('What a user loses');
    expect(text).toContain('Cost to port');
  });

  test('it points at the spec and at the gate it serves', () => {
    expect(text).toContain('2026-09-14-retire-swiftui-design.md');
    expect(text).toContain('Gate 2');
  });
});
