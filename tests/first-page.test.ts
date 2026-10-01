// While a script converts, the window types out its first real page as plain
// paragraphs (spec decision 17). Under --progress the engine sends that page
// once, on stderr, as soon as it has read it. These tests pin the two pure
// judgements (where paragraphs break, what counts as a title page) and the
// line the CLI writes for the invented Field Station feature.
import { afterAll, describe, test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isTitleLike, pageParagraphs } from '../src/parser/first-page';
import type { RawLine } from '../src/parser/types';

const ROOT = new URL('..', import.meta.url).pathname;
const PDF = join(ROOT, 'tests', 'fixtures', 'field-station.pdf');
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-first-page-'));

afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const line = (text: string, y: number, indent = 18): RawLine => ({ text, y, indent, pageNum: 2 });

describe('pageParagraphs', () => {
  test('a gap of twice the line gap starts a new paragraph; a wrapped line joins with one space', () => {
    const lines = [
      line('EXT. RIDGE ROAD - DAY', 720),
      line('Wind comes down off the ridge in long flat gusts, pushing', 696),
      line('the grass one way and then, for no reason, the other.', 684),
      line('A road climbs the ridge.', 660),
      line('Nobody has driven it.', 648),
    ];
    expect(pageParagraphs(lines)).toEqual([
      'EXT. RIDGE ROAD - DAY',
      'Wind comes down off the ridge in long flat gusts, pushing the grass one way and then, for no reason, the other.',
      'A road climbs the ridge. Nobody has driven it.',
    ]);
  });

  test('a change of indent with no gap still starts a new paragraph, so a cue never runs into its speech', () => {
    const lines = [
      line('He counts.', 564),
      line('DELACROIX', 540, 44),
      line('Thirty-one. Thirty-two.', 528, 29),
      line('Thirty-three.', 516, 29),
    ];
    expect(pageParagraphs(lines)).toEqual(['He counts.', 'DELACROIX', 'Thirty-one. Thirty-two. Thirty-three.']);
  });

  test('a page mark is page furniture, not words to type', () => {
    const lines = [
      line('2.', 756, 87),
      line('EXT. FIELD STATION - DAY', 720),
      line('The hut door opens before', 696),
      line('they reach it.', 684),
    ];
    expect(pageParagraphs(lines)).toEqual(['EXT. FIELD STATION - DAY', 'The hut door opens before they reach it.']);
  });

  test('an empty page has no paragraphs', () => {
    expect(pageParagraphs([])).toEqual([]);
  });
});

describe('isTitleLike', () => {
  const W = 612;
  // Each centred line has its own left edge: the text is centred, so its
  // start moves with its length.
  const centred = (text: string, y: number): RawLine => {
    const widthPct = (text.length * 7.2 / W) * 100;
    return line(text, y, 50 - widthPct / 2);
  };

  test('true for a mostly centred 6-line page', () => {
    const lines = [
      centred('FIELD STATION', 504),
      centred('Written by', 468),
      centred('A. Writer', 444),
      centred('Based on the novel', 408),
      centred('by Someone Else', 396),
      line('hello@example.com', 96),
    ];
    expect(isTitleLike(lines, W)).toBe(true);
  });

  test('false for a left-aligned 30-line page', () => {
    const lines = Array.from({ length: 30 }, (_, i) =>
      line('Wind comes down off the ridge in long flat gusts, pushing', 720 - i * 12));
    expect(isTitleLike(lines, W)).toBe(false);
  });

  // Dialogue sits 2.5in to 6in on an 8.5in page, so a speech IS geometrically
  // centred. Shared left edges are what separate it from a title block.
  test('false for a page of cues and speeches, which sit on the page centre but share their left edges', () => {
    const lines: RawLine[] = [];
    let y = 720;
    for (let i = 0; i < 6; i++) {
      lines.push(line('TEO', (y -= 24), 44));
      lines.push(line('It sings in a north wind. One long', (y -= 12), 29));
      lines.push(line('note. In a west wind it is two.', (y -= 12), 29));
    }
    expect(isTitleLike(lines, W)).toBe(false);
  });
});

describe('--progress sends the first real page once', () => {
  async function run(args: string[]) {
    const proc = Bun.spawn(['bun', join(ROOT, 'src', 'cli.ts'), ...args], { stdout: 'pipe', stderr: 'pipe' });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    return { stdout, stderr, exitCode };
  }

  test('Field Station: page 2, opening on its first scene heading, plain words only', async () => {
    const withProgress = await run([PDF, '--json', '--progress', '-o', join(SCRATCH, 'with.epub')]);
    expect(withProgress.exitCode).toBe(0);
    const events = withProgress.stderr.trim().split('\n').filter(Boolean).map((l) => JSON.parse(l).progress);
    const pages = events.filter((e) => e.stage === 'page');
    expect(pages.length).toBe(1);
    const [first] = pages;

    // Page 1 is the title page; the first page after it is page 2.
    expect(first.page).toBe(2);

    // The scene heading the Read page shows first is the .fountain's first one.
    const answer = JSON.parse(withProgress.stdout);
    const fountain = readFileSync(answer.fountainPath, 'utf8');
    const firstHeading = fountain.split('\n').find((l) => /^(INT|EXT)[. ]/.test(l));
    expect(first.paragraphs[0]).toBe(firstHeading);
    expect(first.paragraphs.length).toBeGreaterThan(3);
    for (const p of first.paragraphs) {
      expect(typeof p).toBe('string');
      expect(p).not.toContain('**');
      expect(p).not.toContain('_');
      expect(p).not.toContain('[[');
      expect(p).not.toContain('\n');
    }

    // The page line arrives in the parse stage, before the bar reaches the end.
    const pageAt = events.findIndex((e) => e.stage === 'page');
    expect(events.slice(pageAt + 1).some((e) => e.stage === 'parse')).toBe(true);

    // stdout is the app's contract: same shape, same answer, with or without.
    const without = await run([PDF, '--json', '-o', join(SCRATCH, 'without.epub')]);
    expect(without.exitCode).toBe(0);
    expect(without.stderr).toBe('');
    const plain = JSON.parse(without.stdout);
    expect(Object.keys(answer).sort()).toEqual(Object.keys(plain).sort());
    expect({ ...answer, output: '', fountainPath: '', epubPath: '' })
      .toEqual({ ...plain, output: '', fountainPath: '', epubPath: '' });
  }, 120000);
});
