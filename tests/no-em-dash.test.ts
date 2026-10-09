// No em dash in any text a person reads: the owner's rule for every string
// the window shows and every message the engine prints or puts in an answer.
// A colon or a period says the same thing plainly.
//
// Comments are out of scope, so each source is run through Bun's transpiler,
// which drops them, and what is left is code: string literals, template
// literals, regular expressions. Every em dash left must be one of the few
// below, each a dash that is DATA rather than words: the parser reading a
// screenplay's own dashes (never change how a script's dashes are read or
// rendered), and two comments inside the stylesheet the EPUB carries, which
// no reader sees. Each entry must still be found, so the list cannot go stale
// and hide a new dash behind an old excuse.
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { Glob } from 'bun';

const ROOT = new URL('..', import.meta.url).pathname;
const EM_DASH = '—';

/** file -> what the dash is, and a piece of the one transpiled line it is on. */
const ALLOWED: { file: string; why: string; on: string }[] = [
  {
    file: 'desktop/ui/read.js',
    why: 'the separator between a scene heading\'s place and time, as scripts write it',
    on: `/\\s[-–${EM_DASH}]\\s/g`,
  },
  {
    file: 'src/fountain/serialize.ts',
    why: 'dashes a script puts between a heading and its inline text',
    on: `[\\s/:–${EM_DASH}-]+`,
  },
  {
    file: 'src/parser/boilerplate.ts',
    why: 'dashes in a revision stamp on the page, as scripts print them',
    on: `[\\\\s${EM_DASH}–-]*`,
  },
  {
    file: 'src/epub/css.ts',
    why: 'a comment inside the EPUB\'s own stylesheet, not text a reader sees',
    on: `Original PDF page numbers ${EM_DASH} a marginal reference`,
  },
  {
    file: 'src/epub/css.ts',
    why: 'a comment inside the EPUB\'s own stylesheet, not text a reader sees',
    on: `docs/pagination-reference.md §2 ${EM_DASH}`,
  },
];

function sources(): string[] {
  return [
    ...new Glob('desktop/ui/*.js').scanSync(ROOT),
    ...new Glob('src/**/*.ts').scanSync(ROOT),
    // The device-request service: what it answers, the window shows.
    ...new Glob('services/**/*.ts').scanSync(ROOT),
  ].sort();
}

/** Every line of code (comments gone) in `file` that holds an em dash. */
function codeLinesWithEmDash(file: string): string[] {
  const code = readFileSync(join(ROOT, file), 'utf8');
  if (!code.includes(EM_DASH)) return [];
  const transpiler = new Bun.Transpiler({ loader: file.endsWith('.ts') ? 'ts' : 'js' });
  return transpiler.transformSync(code).split('\n').filter((line) => line.includes(EM_DASH));
}

describe('no em dash in any text a person reads', () => {
  test('the scan sees strings and not comments', () => {
    const transpiler = new Bun.Transpiler({ loader: 'ts' });
    const out = transpiler.transformSync(`// a ${EM_DASH} b\nexport const x: string = 'c ${EM_DASH} d';\n`);
    expect(out.split('\n').filter((line) => line.includes(EM_DASH))).toEqual([`export const x = "c ${EM_DASH} d";`]);
    expect(sources().length).toBeGreaterThan(50);
    expect(sources()).toContain('services/device-requests/src/worker.ts');
  });

  test('every em dash in window and engine code is data on the allow-list, and nothing else', () => {
    const found: string[] = [];
    const used = new Set<number>();
    for (const file of sources()) {
      for (const line of codeLinesWithEmDash(file)) {
        const at = ALLOWED.findIndex((allowed) => allowed.file === file && line.includes(allowed.on));
        if (at === -1) found.push(`${file}: ${line.trim()}`);
        else used.add(at);
      }
    }
    expect(found).toEqual([]);
    // Every excuse still has its dash: a stale entry would hide a new one.
    expect(ALLOWED.filter((_, i) => !used.has(i)).map((a) => `${a.file}: ${a.on}`)).toEqual([]);
  });

  test('the window’s page carries none either', () => {
    expect(readFileSync(join(ROOT, 'desktop/ui/index.html'), 'utf8').includes(EM_DASH)).toBe(false);
  });
});
