import { describe, test, expect } from 'bun:test';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

/**
 * THE REFERENCE SWEEP — piece F1, acceptance criterion 2, of
 * docs/superpowers/specs/2026-09-14-retire-swiftui-design.md.
 *
 * app/ is the shipping SwiftUI Mac app and it is FROZEN, not deleted (see
 * app/README-FROZEN.md). One day piece F3 deletes it. The point of this
 * test is that when that day comes, the full blast radius is already
 * written down in tools/app-references.json — with, for each file, what
 * breaks — instead of being discovered one red CI job at a time.
 *
 * So this test answers two questions, and the first one is the hard one.
 *
 * ── WHAT COUNTS AS A REFERENCE ────────────────────────────────────────
 *
 * The obvious sweep — grep the tree for the literal string `app/` — is
 * wrong in both directions, and both were measured in this tree before
 * this comment was written:
 *
 *   MISSES. .github/workflows/weekly-toolchain.yml is, in its entirety,
 *   `swift run -c release kit-check` on macos-15 with `working-directory:
 *   app`. There is no slash. A grep for `app/` does not see an entire
 *   macOS workflow that F3 must delete — and that workflow is absent from
 *   the design spec's own reference table for exactly this reason.
 *   Likewise `swift build`, `Screepub-macOS.dmg` and the bare
 *   `Export.swift`-style provenance comments in src/.
 *
 *   FALSE POSITIVES. .github/workflows/release.yml says `path: app` and
 *   `cd app` in its app-bundles and app-upload jobs — and that `app` is
 *   the TAURI bundle staging directory, nothing to do with this one.
 *   tests/release-artifacts.test.ts:379 asserts `/cd app/` against that
 *   same Tauri job. A sweep that flagged those would be telling the next
 *   person to delete working code, which is how a guard earns its own
 *   deletion.
 *
 * The definition used here is therefore NOT textual proximity to "app".
 * A reference is:
 *
 *   an occurrence, in a tracked file outside app/, of a token that names
 *   the SwiftUI app itself — its directory, a Swift source file, a Swift
 *   build command, or a release artifact that only it produces.
 *
 * which is the PATTERNS list below. `cd app` is deliberately NOT in it:
 * it cannot be told apart from the Tauri staging directory by pattern, and
 * a pattern that needs a human to adjudicate every hit is a checklist
 * wearing a test's clothes. The one real `cd app`-shaped reference,
 * `working-directory: app`, is anchored to end-of-line and matches only
 * the five Swift steps.
 *
 * ── WHAT IS NOT SWEPT, AND WHY ────────────────────────────────────────
 *
 * Dated records. docs/superpowers/ (specs, plans, run logs), docs/adr/ and
 * docs/releases/ describe what was decided or shipped on a given date.
 * Deleting app/ does not make a 2026-08-04 plan wrong; rewriting one to
 * hide that the Mac app existed would be worse than a dangling path. Four
 * plans there edit app/Sources/KitCheck/main.swift BY LINE NUMBER, and the
 * spec is explicit that the sweep must exclude them "or it will fight them
 * forever".
 *
 * docs/retired-coverage.md joined them at F3. It is the gate-2 record of
 * what the Swift app's checks covered and what became of each, so its whole
 * job is to name Swift sources and kit-check line ranges. Once the
 * directory is gone it is a dated record like the plans, not a live claim,
 * and rewriting it to avoid the names would erase what it records.
 *
 * This test and tools/app-references.json are also excluded: they name
 * every token in the list by construction, so counting them measures the
 * sweep, not the tree.
 *
 * ── THE SHAPE OF THE PIN ──────────────────────────────────────────────
 *
 * The FILE SET is exact — a file that starts referring to app/ without a
 * row fails, and so does a row whose file no longer refers to app/ at all
 * (a stale row is a lie about the blast radius). The PER-FILE COUNT is a
 * CEILING, matching tests/frozen-app.test.ts: references may fall freely,
 * because every one that falls is progress toward F3, but a file cannot
 * quietly grow new ones. Exact counts would fail on an unrelated copy edit
 * to README.md, and a guard that fails for reasons nobody cares about is a
 * guard that gets deleted.
 *
 * Would a plausible wrong implementation pass? The non-vacuity block below
 * is the answer: a scanner that matched nothing, a pattern list that had
 * quietly lost its teeth, or an inventory of empty annotations would all
 * satisfy set equality against themselves. So the anchors, the categories
 * and the prose are asserted independently.
 */

const REPO = new URL('..', import.meta.url).pathname;
const INVENTORY_PATH = join(REPO, 'tools', 'app-references.json');

/** Dated records (see above), the directory itself, and the sweep's own files. */
const NOT_SWEPT = [
  /^app\//,
  /^docs\/superpowers\//,
  /^docs\/adr\//,
  /^docs\/releases\//,
  /^docs\/retired-coverage\.md$/,
  /^tools\/app-references\.json$/,
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
    what: 'a path into the directory',
  },
  {
    name: 'workdir',
    // Anchored to end of line: this is the YAML form that has no slash and
    // that a naive sweep misses entirely. `cd app` is excluded on purpose.
    re: /working-directory:[ \t]*app[ \t]*(?=\n|$)/gm,
    what: 'a workflow step that runs inside app/',
  },
  { name: 'swift-symbol', re: /ScreepubKit|ScreepubApp|KitCheck|kit-check|KFXKit/g, what: 'a Swift module or the kit-check executable' },
  { name: 'swift-file', re: /\b[A-Za-z][A-Za-z0-9-]*\.swift\b/g, what: 'a Swift source file that lives in app/' },
  { name: 'swift-build', re: /swift (?:build|run)|build-app\.sh|build-lib\.sh/g, what: 'a command that builds the Swift app' },
  // There was a fifth rule here, `swift-artifact`, matching Screepub.app and
  // Screepub-macOS. At the identifier release the window took both names
  // (its bundle is Screepub.app, its image Screepub-macOS.dmg), so neither
  // says "the Swift app" any more. The Swift app's own artifacts live under
  // app/dist/, which the path rule above still catches.
];

type Row = { refs: number; category: string; breaks: string };
const inventory = JSON.parse(readFileSync(INVENTORY_PATH, 'utf8')) as {
  $comment: string;
  files: Record<string, Row>;
};

const CATEGORIES = ['build', 'test', 'artifact', 'doc', 'note', 'freeze'];

/** Every file the repository is about to be responsible for: tracked, PLUS
 *  untracked-and-not-ignored.
 *
 *  `--others --exclude-standard` is not tidiness, it closes a hole that let a
 *  file through on 2026-09-21. A bare `git ls-files` lists only TRACKED
 *  files, so a brand-new one is invisible to this sweep until it is staged —
 *  and a brand-new file is the single likeliest kind to need an inventory
 *  row. desktop/ui/feedback.js, a port of Feedback.swift, passed a full green
 *  suite and then failed the moment it was committed, because `git add` is
 *  what made it visible here. The author read the green run as the answer,
 *  which it honestly was for the tree as git then knew it.
 *
 *  Demonstrated rather than assumed: one file, identical contents, nothing
 *  else changed — untracked it is not flagged, staged it is.
 *
 *  `--exclude-standard` keeps .gitignore honoured, so build output, the
 *  2.5 GB target/ and the gitignored real-script fixtures stay out. What is
 *  added is exactly the set someone is about to commit. */
function tracked(): string[] {
  const proc = Bun.spawnSync(
    ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
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

/** path -> reference count, for everything swept. */
function scan(): Map<string, number> {
  const found = new Map<string, number>();
  for (const f of tracked()) {
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
  'tools/app-references.json is the BLAST RADIUS of deleting app/ — the list',
  'piece F3 works through, with what breaks for each file. It is a pin, not a',
  'cache: adding a row is how you record a new coupling, and you owe the row a',
  'category and a sentence saying what breaks.',
  '',
  'If you just ADDED a reference to the frozen Mac app, the first question is',
  'whether you should have. app/ is frozen (app/README-FROZEN.md); new work',
  'belongs in src/ and desktop/. If the reference is legitimate — a doc that',
  'must describe the shipping app, a test that pins its artifacts — add or',
  'update its row.',
  '',
  'If you REMOVED the last reference in a file, delete its row. Shrinking is',
  'the direction this test never fights; a row for a file that no longer',
  'refers to app/ overstates the work F3 still has to do.',
  '',
  'Read this file\'s header comment before widening the patterns: `cd app` and',
  '`path: app` in release.yml are the TAURI staging directory and must not be',
  'swept.',
].join('\n');

function fail(title: string, lines: string[]): never {
  throw new Error([title, ...lines.map((l) => '  ' + l), FIX].join('\n'));
}

describe('the reference sweep: everything outside app/ that names it', () => {
  const found = scan();

  test('every file that references app/ has a row in the inventory', () => {
    const unlisted = [...found.keys()].filter((f) => !(f in inventory.files)).sort();
    if (unlisted.length) {
      fail('Files reference the frozen Mac app but are not in the inventory.', [
        ...unlisted.map((f) => {
          const text = readFileSync(join(REPO, f), 'utf8');
          const kinds = PATTERNS.filter((p) => (text.match(p.re) ?? []).length).map((p) => p.what);
          return `${f} — ${found.get(f)} reference(s): ${kinds.join('; ')}`;
        }),
      ]);
    }
    expect(unlisted).toEqual([]);
  });

  test('no row in the inventory is stale', () => {
    const stale = Object.keys(inventory.files)
      .filter((f) => !found.has(f))
      .sort();
    if (stale.length) {
      fail('The inventory lists files that no longer reference the frozen Mac app.', [
        ...stale.map((f) => (existsSync(join(REPO, f)) ? `${f} — file exists, but no reference left in it` : `${f} — file is gone`)),
      ]);
    }
    expect(stale).toEqual([]);
  });

  test('no file has grown new references to app/', () => {
    // A ceiling, not an equality: see the header. Falling counts are F3
    // making progress and must never be a failing build.
    const grown: string[] = [];
    for (const [f, n] of found) {
      const row = inventory.files[f];
      if (!row) continue; // reported above
      if (n > row.refs) grown.push(`${f}: ${row.refs} reference(s) when pinned, ${n} now (+${n - row.refs})`);
    }
    if (grown.length) fail('New references into the frozen Mac app.', grown);
    expect(grown).toEqual([]);
  });

  test('every row says what would break, in words a stranger can act on', () => {
    // The inventory's value is the `breaks` column. A row with a category
    // and no reason is a filename, which the scanner could have produced by
    // itself — and would make the whole file a cache rather than a decision.
    const bad: string[] = [];
    for (const [f, row] of Object.entries(inventory.files)) {
      if (!CATEGORIES.includes(row.category)) bad.push(`${f}: category "${row.category}" is not one of ${CATEGORIES.join(', ')}`);
      if (typeof row.breaks !== 'string' || row.breaks.trim().length < 40) bad.push(`${f}: "breaks" must explain the consequence, not restate the path`);
      if (!Number.isInteger(row.refs) || row.refs < 1) bad.push(`${f}: refs must be a positive integer`);
    }
    if (bad.length) fail('Inventory rows are incomplete.', bad);
    expect(bad).toEqual([]);
  });
});

describe('the sweep is not vacuous', () => {
  test('a file that is not committed yet is still swept', () => {
    // The hole that let desktop/ui/feedback.js through on 2026-09-21. A bare
    // `git ls-files` lists only TRACKED files, so a brand-new file — the
    // likeliest kind to need a row — was invisible here until `git add` made
    // it visible, which meant a genuinely green suite went red at the commit
    // and not before.
    //
    // Asserted against the flags rather than by writing a file into the
    // repository mid-suite, because a test that creates one and crashes
    // leaves it behind for the next run to trip over.
    const source = readFileSync(join(REPO, 'tests', 'app-references.test.ts'), 'utf8');
    expect(source).toContain("'--others'");
    expect(source).toContain("'--exclude-standard'");
    // And the sweep must still see the ordinary case, or the flags above
    // could be satisfied by a list that is somehow empty.
    expect(tracked()).toContain('tools/app-references.json');
  });

  test('the patterns actually match the forms that a naive grep misses', () => {
    // If someone weakens PATTERNS, set equality against a regenerated
    // inventory would still be green. These are the forms that motivated
    // the pattern list, asserted directly.
    expect(countIn('      - run: swift run -c release kit-check\n        working-directory: app\n')).toBeGreaterThanOrEqual(3);
    expect(countIn('Mirrors EbookConvert.swift exactly')).toBe(1);
    expect(countIn('see `app/release.sh`')).toBe(1);
    // The Swift app's own artifacts are still caught where they live.
    expect(countIn('app/dist/Screepub-macOS.dmg')).toBe(1);
  });

  test('the names the window took over at the identifier release are not references', () => {
    // Screepub.app is the window's bundle and Screepub-macOS.dmg its image
    // since the handover. Counting them would tell people to delete the
    // window's own download links.
    expect(countIn('download/Screepub-macOS.dmg')).toBe(0);
    expect(countIn('/Applications/Screepub.app')).toBe(0);
  });

  test('the patterns do not match the Tauri staging directory', () => {
    // The false-positive half. If either of these ever counts, the sweep
    // starts telling people to delete working release machinery.
    expect(countIn('          path: app\n')).toBe(0);
    expect(countIn('          cd app\n          sha256sum *.deb\n')).toBe(0);
    expect(countIn('pattern: app-bundles-*\n')).toBe(0);
    expect(countIn('working-directory: desktop/src-tauri\n')).toBe(0);
  });

  // There was an anchors test here: the inventory had to keep naming
  // theme-colors.ts, ci.yml, weekly-toolchain.yml and THIRD-PARTY-NOTICES.md
  // and hold more than 40 rows, because before F3 an emptied inventory
  // agreeing with an emptied scan was the failure mode. At F3 emptying it is
  // the job, so the anchors went with the first rows F3 resolved. What still
  // stops a scan that sees nothing from passing is the two tests either side
  // of this note: the patterns must match the forms they exist for, and the
  // swept slice of the tree must be the whole tree.

  test('the sweep covers the whole tree, not an empty slice of it', () => {
    // A broken NOT_SWEPT list, a failed git ls-files, or a cwd mistake all
    // show up here rather than as a mysteriously green sweep.
    const files = tracked().filter((x) => !NOT_SWEPT.some((r) => r.test(x)));
    expect(files.length).toBeGreaterThan(150);
    expect(files).toContain('README.md');
    expect(files).toContain('.github/workflows/weekly-toolchain.yml');
    expect(files.some((x) => x.startsWith('app/'))).toBe(false);
    expect(files.some((x) => x.startsWith('docs/superpowers/'))).toBe(false);
    expect(files).not.toContain('docs/retired-coverage.md');
  });
});

describe('docs/retired-coverage.md — the coverage that bun test does not have', () => {
  const DOC = join(REPO, 'docs', 'retired-coverage.md');
  const text = readFileSync(DOC, 'utf8');

  /**
   * F1 acceptance criterion 3: every unreplaced kit-check section is listed
   * with an explicit decision. This is the mechanism that makes gate 2
   * checkable rather than remembered — the spec's words.
   *
   * The sections are named here, in the test, rather than read out of the
   * document: a test that derived its expectations from the document under
   * test would pass against an empty document. Each id below is one row of
   * the spec's "What was measured" item 2, with the check count that item
   * measured — so dropping a section, or quietly shrinking one, fails here
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
    // `port` or `accept-loss` — the spec's two words. A section listed with
    // neither is the "we'll decide later" that gate 2 exists to prevent.
    const rows = text.split('\n').filter((l) => l.startsWith('| `'));
    expect(rows.length).toBeGreaterThanOrEqual(SECTIONS.length);
    for (const row of rows) {
      expect(row, `no port/accept-loss decision in: ${row.slice(0, 60)}`).toMatch(/`port`|`accept-loss`/);
    }
  });

  test('the decisions are marked as recommendations, not as settled', () => {
    // The standing correction in this program is overclaiming. Nobody in
    // this repository has the standing to decide that a shipping feature is
    // gone; the document's job is to make deciding cheap, and it must say
    // so where a skimmer sees it.
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
