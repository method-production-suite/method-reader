import { describe, test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';

function parseMarkup(s: string): { plain: string; spans: [number, number, string][] } {
  const run = spawnSync('python3', ['tools/make-fixture.py', '--parse-markup', s], {
    encoding: 'utf8',
  });
  if (run.status !== 0) throw new Error(run.stderr.trim());
  return JSON.parse(run.stdout);
}

describe('inline markup', () => {
  test('strips markers and records offsets into the PLAIN text', () => {
    const r = parseMarkup('a {b}bold{/b} c');
    expect(r.plain).toBe('a bold c');
    expect(r.spans).toEqual([[2, 6, 'b']]);
  });

  test('handles two styles over the same run', () => {
    const r = parseMarkup('{b}{i}both{/i}{/b}');
    expect(r.plain).toBe('both');
    expect(r.spans.map((s) => s[2]).sort()).toEqual(['b', 'i']);
  });

  test('text with no markup is returned unchanged', () => {
    const r = parseMarkup('plain text');
    expect(r.plain).toBe('plain text');
    expect(r.spans).toEqual([]);
  });

  // A typo buried in pages of content must fail at generation, not
  // produce a fixture that quietly tests nothing.
  test('an unclosed marker is an error, not silently dropped', () => {
    expect(() => parseMarkup('a {b}bold')).toThrow();
  });

  test('an unmatched closer is an error', () => {
    expect(() => parseMarkup('a bold{/b}')).toThrow();
  });
});

function wrapSpans(s: string, width: number): { styles: string[]; text: string }[][] {
  const run = spawnSync(
    'python3',
    ['tools/make-fixture.py', '--wrap', String(width), s],
    { encoding: 'utf8' },
  );
  if (run.status !== 0) throw new Error(run.stderr.trim());
  return JSON.parse(run.stdout);
}

describe('offset-preserving wrap', () => {
  // The case a naive wrap-the-marked-up-string approach gets wrong: it
  // would either split "{b}" across the break or count its 3 characters as
  // text width.
  test('a styled span survives a line break, styled on BOTH lines', () => {
    const lines = wrapSpans('aaa {b}bbb ccc{/b} ddd', 7);
    const bolded = lines
      .flat()
      .filter((r) => r.styles.includes('b'))
      .map((r) => r.text.trim())
      .filter(Boolean);
    expect(bolded).toEqual(['bbb', 'ccc']);
  });

  test('wrapping ignores marker characters when measuring width', () => {
    // Plain text is 11 chars: "aaa bbb ccc". At width 11 that is ONE line,
    // even though the marked-up string is 18 characters long.
    const lines = wrapSpans('aaa {b}bbb{/b} ccc', 11);
    expect(lines.length).toBe(1);
  });

  test('unstyled text wraps on word boundaries', () => {
    const lines = wrapSpans('one two three four', 8);
    const texts = lines.map((l) => l.map((r) => r.text).join(''));
    expect(texts).toEqual(['one two', 'three', 'four']);
  });

  test('adjacent characters with equal styles merge into one run', () => {
    const lines = wrapSpans('{b}bold{/b}', 20);
    expect(lines[0].length).toBe(1);
    expect(lines[0][0]).toEqual({ styles: ['b'], text: 'bold' });
  });

  test('underline is carried as a style like any other', () => {
    const lines = wrapSpans('a {u}line{/u} here', 40);
    const underlined = lines.flat().filter((r) => r.styles.includes('u'));
    expect(underlined.map((r) => r.text)).toEqual(['line']);
  });
});
