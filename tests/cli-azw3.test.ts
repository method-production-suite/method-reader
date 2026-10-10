// The conversion's own --azw3: Screepub's AZW3 of the EPUB the same run
// wrote, beside the other outputs, the way --fdx and --mobi are. And
// `export --for azw3`, which now needs nothing installed.
import { afterAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildAzw3 } from '../src/azw3/writer';
import { readKf8 } from './helpers/kf8-reader';
import { maskPdbDates } from './helpers/mobi-records';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-azw3-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));
const ROOT = new URL('..', import.meta.url).pathname;
const FOUNTAIN = 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n\nJO\nA line.\n\nEXT. ROAD - NIGHT\n\nRain.\n';

const run = (args: string[]) => Bun.spawnSync(['bun', 'src/cli.ts', ...args], { cwd: ROOT, env: process.env });

describe('screepub <script> --azw3 <file>', () => {
  test('a PDF conversion writes the AZW3 of the EPUB it wrote, and names it in --json', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'pdf-'));
    const epub = join(dir, 'Converted.epub');
    const azw3 = join(dir, 'Converted.azw3');
    const proc = run([join(ROOT, 'tests', 'fixtures', 'screenplay.pdf'), '-o', epub, '--azw3', azw3, '--json']);
    expect(proc.exitCode).toBe(0);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.azw3Path).toBe(azw3);
    const bytes = new Uint8Array(readFileSync(azw3));
    // The same book the EPUB is: our writer on that EPUB, byte for byte but
    // for the PalmDB dates.
    expect(maskPdbDates(bytes)).toEqual(maskPdbDates(await buildAzw3(new Uint8Array(readFileSync(epub)))));
    expect(readKf8(bytes).ncx.length).toBe(5);
  });

  test('a Fountain conversion writes one too, and says so', () => {
    const dir = mkdtempSync(join(SCRATCH, 'fountain-'));
    const source = join(dir, 'Script.fountain');
    writeFileSync(source, FOUNTAIN);
    const azw3 = join(dir, 'Out.azw3');
    const proc = run([source, '-o', join(dir, 'Out.epub'), '--azw3', azw3]);
    expect(proc.exitCode).toBe(0);
    expect(proc.stdout.toString()).toContain(`wrote ${azw3}`);
    expect(readKf8(new Uint8Array(readFileSync(azw3))).ncx.map((n) => n.label)).toEqual(['INT. ROOM - DAY', 'EXT. ROAD - NIGHT']);
  });

  test('without --azw3 no AZW3 is written and the answer has no azw3Path', () => {
    const dir = mkdtempSync(join(SCRATCH, 'none-'));
    const source = join(dir, 'Script.fountain');
    writeFileSync(source, FOUNTAIN);
    const proc = run([source, '-o', join(dir, 'Out.epub'), '--json']);
    const answer = JSON.parse(proc.stdout.toString());
    expect('azw3Path' in answer).toBe(false);
    expect(readdirSync(dir).filter((f) => f.endsWith('.azw3'))).toEqual([]);
  });
});

describe('screepub export --for azw3', () => {
  test('makes the AZW3 beside the book with no Calibre involved, and says it is beta', () => {
    const dir = mkdtempSync(join(SCRATCH, 'export-'));
    const source = join(dir, 'Script.fountain');
    writeFileSync(source, FOUNTAIN);
    const epub = join(dir, 'Script.epub');
    expect(run([source, '-o', epub]).exitCode).toBe(0);
    const proc = run(['export', epub, '--for', 'azw3', '--json']);
    expect(proc.exitCode).toBe(0);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer).toMatchObject({ ok: true, path: join(dir, 'Script.azw3'), extension: 'azw3' });
    expect(answer.label).toContain('(beta)');
    expect(readKf8(new Uint8Array(readFileSync(answer.path))).header.version).toBe(8);
  });
});
