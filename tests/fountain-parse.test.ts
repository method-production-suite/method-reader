import { describe, test, expect } from 'bun:test';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parseFountain } from '../src/fountain/parse';
import { fountainToFdx } from '../src/fdx/fdx';

// The first speech is the right half of a dual exchange with nothing on its
// left. fountain-js lexes bottom-up and keeps "the last line was dual" in a
// static, so this script ends its parse with the flag still set.
const SRC = [
  'INT. A - DAY',
  '',
  'ANNE ^',
  'Hello there.',
  '',
  'She sits.',
  '',
  'BOB',
  'Last words.',
  '',
].join('\n');

describe('parseFountain', () => {
  test('each parse is independent of the one before it', () => {
    const first = parseFountain(SRC).tokens;
    const second = parseFountain(SRC).tokens;
    expect(second).toEqual(first);
    expect(second.filter((t) => t.type === 'dialogue').map((t) => t.text)).toContain('Last words.');
  });

  test('the FDX keeps its last speech on a second conversion', () => {
    const first = fountainToFdx(SRC);
    const second = fountainToFdx(SRC);
    expect(second).toBe(first);
    expect(second).toContain('Last words.');
  });

  test('nothing in src/ parses Fountain except through parseFountain', () => {
    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) walk(path);
        else if (path.endsWith('.ts') && !path.endsWith(join('fountain', 'parse.ts'))) {
          if (/new Fountain\s*\(/.test(readFileSync(path, 'utf8'))) offenders.push(path);
        }
      }
    };
    walk(join(import.meta.dir, '..', 'src'));
    expect(offenders).toEqual([]);
  });
});
