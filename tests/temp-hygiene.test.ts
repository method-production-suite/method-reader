// No test file leaves anything behind in the system temp folder.
//
// The rule, for every file in tests/:
//
//   1. tmpdir() appears on exactly one line, at the top level:
//        const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-<file>-'));
//   2. A top-level afterAll in the same file removes that folder:
//        afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));
//   3. Every other mkdtemp in the file makes its folder INSIDE that one:
//        mkdtempSync(join(SCRATCH, 'volume-'))
//
// A file that needs no temp space at all has none of the three.
//
// Why a rule about the SOURCE and not a count of the temp folder: the temp
// folder is shared. Another worktree's suite, or a real screenshot capture,
// can add or remove entries at the same moment, so a before/after count
// reports their work as ours or hides ours behind theirs. And why this rule
// in particular: about 23,000 screepub-* folders piled up in $TMPDIR from
// helpers that made a fresh folder per test and never removed it. One
// folder per file, removed in afterAll, is the shape that cannot do that.
//
// What this cannot see: a PROCESS a test starts that writes to the temp
// folder on its own account. Kindle Previewer did exactly that through the
// real-toolchain export test (fixed in src/export/kfx.ts, pinned in
// export-kfx.test.ts); the repo's own tools are pinned by
// tool-temp-folders.test.ts, which gives each one a TMPDIR to be judged by.
// Nor the engine's own pdf.js decoder folder, made once per process for
// whichever test opens a document first: the third bunfig.toml preload,
// tests/remove-pdfjs-decoders.ts, removes it, and pdfjs-wasm.test.ts pins that.
import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const TESTS = new URL('.', import.meta.url).pathname;

/** Files allowed more tmpdir() lines than the scratch line, and why. Each
 *  one READS tmpdir() and writes nothing there itself. Keep this short. */
const READS_TMPDIR: Record<string, string> = {
  'capture-chrome.test.ts': 'asserts that launch() puts its Chrome profile under tmpdir()',
};

const SCRATCH_LINE = /^const (\w+) = mkdtempSync\(join\(tmpdir\(\), '[\w-]+'\)\);$/;
const MKDTEMP = /\bmkdtemp(?:Sync)?\(/;

/** What is wrong with one test file's source, as sentences. Empty is clean. */
function tempProblems(source: string, extraReads = 0): string[] {
  const code = source
    .split('\n')
    .filter((line) => !/^\s*(\/\/|\/\*|\*)/.test(line));
  const usesTmpdir = code.filter((line) => /\btmpdir\(\)/.test(line));
  const usesMkdtemp = code.filter((line) => MKDTEMP.test(line));
  if (usesTmpdir.length === 0 && usesMkdtemp.length === 0) return [];

  const scratch = code.filter((line) => SCRATCH_LINE.test(line));
  if (scratch.length !== 1) {
    return [
      `needs exactly one top-level "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-...-'));" ` +
        `(found ${scratch.length})`,
    ];
  }
  const name = SCRATCH_LINE.exec(scratch[0]!)![1]!;
  const problems: string[] = [];

  for (const line of usesTmpdir) {
    if (line !== scratch[0] && extraReads-- <= 0) {
      problems.push(`uses tmpdir() outside its scratch folder: ${line.trim()}`);
    }
  }
  for (const line of usesMkdtemp) {
    if (line !== scratch[0] && !line.includes(`mkdtempSync(join(${name}, `)) {
      problems.push(`makes a temp folder outside ${name}: ${line.trim()}`);
    }
  }
  const removes = `rmSync(${name}, { recursive: true, force: true })`;
  if (!topLevelAfterAlls(code).some((block) => block.includes(removes))) {
    problems.push(`never removes ${name}: add "afterAll(() => ${removes});"`);
  }
  return problems;
}

/** The text of each top-level afterAll(...) statement: from its line to the
 *  first unindented line that closes a call, which is the same line for a
 *  one-liner and the `});` for a block. */
function topLevelAfterAlls(code: string[]): string[] {
  const blocks: string[] = [];
  for (let i = 0; i < code.length; i++) {
    if (!code[i]!.startsWith('afterAll(')) continue;
    let j = i;
    while (j < code.length - 1 && !/^\S.*\);\s*$/.test(code[j]!)) j++;
    blocks.push(code.slice(i, j + 1).join('\n'));
  }
  return blocks;
}

describe('temp hygiene', () => {
  // This file is skipped: the samples below are the rule's own test cases.
  const files = readdirSync(TESTS).filter(
    (f) => f.endsWith('.ts') && f !== 'temp-hygiene.test.ts',
  );

  test('every test file keeps its temp folders inside one scratch folder it removes', () => {
    const report: string[] = [];
    for (const file of files) {
      const extra = READS_TMPDIR[file] === undefined ? 0 : 1;
      for (const problem of tempProblems(readFileSync(join(TESTS, file), 'utf8'), extra)) {
        report.push(`${file}: ${problem}`);
      }
    }
    expect(report).toEqual([]);
  });

  test('every exception still names a file that reads tmpdir()', () => {
    // An exception outliving its reason would quietly widen the rule.
    for (const file of Object.keys(READS_TMPDIR)) {
      expect(files).toContain(file);
      expect(readFileSync(join(TESTS, file), 'utf8')).toMatch(/\btmpdir\(\)/);
    }
  });

  // The rule, run against sources written to break it, so a regex that
  // stops matching anything shows up here and not as a silent pass above.
  const GOOD = [
    "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));",
    'afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));',
    "const dir = mkdtempSync(join(SCRATCH, 'volume-'));",
  ].join('\n');

  test('the pattern itself passes', () => {
    expect(tempProblems(GOOD)).toEqual([]);
    expect(tempProblems("test('no temp at all', () => {});")).toEqual([]);
  });

  test('a per-test folder straight under tmpdir() fails', () => {
    const src = `${GOOD}\n  const dir = mkdtempSync(join(tmpdir(), 'screepub-test-'));`;
    expect(tempProblems(src).join('\n')).toContain('outside its scratch folder');
  });

  test('a fixed file name under tmpdir() fails', () => {
    const src = `${GOOD}\n  const out = join(tmpdir(), 'screepub-read-surface.epub');`;
    expect(tempProblems(src).join('\n')).toContain('outside its scratch folder');
  });

  test('a scratch folder that is never removed fails', () => {
    const src = "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));";
    expect(tempProblems(src).join('\n')).toContain('never removes SCRATCH');
  });

  test('removing it per test instead of in afterAll fails', () => {
    const src = [
      "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));",
      'afterEach(() => rmSync(SCRATCH, { recursive: true, force: true }));',
    ].join('\n');
    expect(tempProblems(src).join('\n')).toContain('never removes SCRATCH');
  });

  test('an afterAll block that does more than remove it passes', () => {
    const src = [
      "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));",
      'afterAll(() => {',
      '  unlock(SCRATCH);',
      '  rmSync(SCRATCH, { recursive: true, force: true });',
      '});',
    ].join('\n');
    expect(tempProblems(src)).toEqual([]);
  });

  test('an rmSync of it inside a test body does not count as the afterAll', () => {
    const src = [
      "const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));",
      'afterAll(() => server.stop());',
      "test('x', () => {",
      '  rmSync(SCRATCH, { recursive: true, force: true });',
      '});',
    ].join('\n');
    expect(tempProblems(src).join('\n')).toContain('never removes SCRATCH');
  });

  test('a scratch folder made inside a test or helper fails', () => {
    const src = [
      "  const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-good-'));",
      'afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));',
    ].join('\n');
    expect(tempProblems(src).join('\n')).toContain('exactly one top-level');
  });

  test('two scratch folders in one file fail', () => {
    const src = `${GOOD}\nconst OTHER = mkdtempSync(join(tmpdir(), 'screepub-other-'));`;
    expect(tempProblems(src).join('\n')).toContain('found 2');
  });

  test('a mkdtemp somewhere other than the scratch folder fails', () => {
    const src = `${GOOD}\n  const dir = mkdtempSync('/tmp/screepub-x-');`;
    expect(tempProblems(src).join('\n')).toContain('outside SCRATCH');
  });

  test('a commented-out example does not count', () => {
    const src = `${GOOD}\n// mkdtempSync(join(tmpdir(), 'screepub-test-'))`;
    expect(tempProblems(src)).toEqual([]);
  });

  test('an allowed read is allowed once, not twice', () => {
    const read = "  expect(p).toStartWith(join(tmpdir(), 'x-'));";
    expect(tempProblems(`${GOOD}\n${read}`, 1)).toEqual([]);
    expect(tempProblems(`${GOOD}\n${read}\n${read}`, 1)).toHaveLength(1);
  });
});
