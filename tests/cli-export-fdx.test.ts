// `screepub export <book.epub> --for fdx --fountain <f>`: the editable
// screenplay, built from the script's .fountain and written the way every
// other export write is, atomically. No toolchain is involved, so every
// test injects a Calibre probe that says "absent" and the real one is
// never consulted.
import { afterAll, afterEach, beforeEach, describe, expect, test } from 'bun:test';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { exportCommand, type ExportDeps } from '../src/cli-export';
import { fileExtension, formatLabel } from '../src/export/formats';
import { fountainToFdx } from '../src/fdx/fdx';
import { parseXml } from './helpers/mini-xml';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-export-fdx-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const FOUNTAIN = 'Title: Test Piece\n\nINT. ROOM - DAY\n\nMARGO\nHello *there*.\n';

let dir: string;
let epub: string;
let fountain: string;
let ladderCalls = 0;
const settingsPath = () => join(dir, 'app-settings.json');
const deps = (): ExportDeps => ({
  calibreAvailable: () => false,
  kfxStatus: async () => { throw new Error('the KFX probe must not run for an FDX'); },
  freshKindleArtifact: async () => { ladderCalls++; throw new Error('the Kindle ladder must not run for an FDX'); },
  appSettingsPath: settingsPath(),
});

beforeEach(() => {
  dir = mkdtempSync(join(SCRATCH, 'case-'));
  epub = join(dir, 'Script.epub');
  fountain = join(dir, 'Script.fountain');
  // The book is never read for an FDX, only required to exist, the same
  // contract every export has.
  writeFileSync(epub, 'not really an epub');
  writeFileSync(fountain, FOUNTAIN);
  ladderCalls = 0;
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

async function codeOf(p: Promise<unknown>): Promise<{ code?: string; message: string }> {
  try {
    await p;
  } catch (err) {
    return { code: (err as { code?: string }).code, message: (err as Error).message };
  }
  throw new Error('expected a refusal');
}

describe('formats', () => {
  test('fdx is its own extension and label, whatever the toolchain', () => {
    expect(fileExtension('fdx', { calibreAvailable: true, kfxReady: true })).toBe('fdx');
    expect(fileExtension('fdx', { calibreAvailable: false, kfxReady: false })).toBe('fdx');
    expect(formatLabel('fdx', { calibreAvailable: false, kfxReady: false })).toContain('Final Draft');
  });
});

describe('exportCommand --for fdx', () => {
  test('writes the FDX beside the book by default, built from the .fountain', async () => {
    const result = await exportCommand({ epub, for: 'fdx', fountain }, deps());
    const want = join(dir, 'Script.fdx');
    expect(result.path).toBe(want);
    expect(result.format).toBe('fdx');
    expect(result.extension).toBe('fdx');
    expect(result.stages).toEqual([]);
    expect(readFileSync(want, 'utf8')).toBe(fountainToFdx(FOUNTAIN));
    expect(parseXml(readFileSync(want, 'utf8')).name).toBe('FinalDraft');
    expect(ladderCalls).toBe(0);
  });

  test('--out writes it there instead, parents created, and nothing beside the book', async () => {
    const out = join(dir, 'nested', 'deeper', 'Saved.fdx');
    const result = await exportCommand({ epub, for: 'fdx', fountain, out }, deps());
    expect(result.path).toBe(out);
    expect(readFileSync(out, 'utf8')).toBe(fountainToFdx(FOUNTAIN));
    expect(existsSync(join(dir, 'Script.fdx'))).toBe(false);
  });

  test('--out replaces an existing file and leaves no temp file behind', async () => {
    const out = join(dir, 'Saved.fdx');
    writeFileSync(out, 'old');
    await exportCommand({ epub, for: 'fdx', fountain, out }, deps());
    expect(readFileSync(out, 'utf8')).toBe(fountainToFdx(FOUNTAIN));
    expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
  });

  test('without --fountain it is a usage error naming --fountain, and writes nothing', async () => {
    const r = await codeOf(exportCommand({ epub, for: 'fdx' }, deps()));
    expect(r.code).toBe('usage');
    expect(r.message).toContain('--fountain');
    expect(existsSync(join(dir, 'Script.fdx'))).toBe(false);
  });

  test('a missing .fountain is unreadable and names the path', async () => {
    const missing = join(dir, 'Gone.fountain');
    const r = await codeOf(exportCommand({ epub, for: 'fdx', fountain: missing }, deps()));
    expect(r.code).toBe('unreadable');
    expect(r.message).toContain(missing);
  });

  test('a missing book is unreadable, the same as every other export', async () => {
    const r = await codeOf(exportCommand({ epub: join(dir, 'nope.epub'), for: 'fdx', fountain }, deps()));
    expect(r.code).toBe('unreadable');
  });

  test('a relative --out is refused before anything is written', async () => {
    const r = await codeOf(exportCommand({ epub, for: 'fdx', fountain, out: 'Saved.fdx' }, deps()));
    expect(r.code).toBe('usage');
    expect(r.message).toContain('absolute');
  });

  test('the wrong extension on --out is refused, naming .fdx', async () => {
    const out = join(dir, 'Saved.epub');
    const r = await codeOf(exportCommand({ epub, for: 'fdx', fountain, out }, deps()));
    expect(r.code).toBe('usage');
    expect(r.message).toContain('.fdx');
    expect(existsSync(out)).toBe(false);
  });

  test('a write that fails is export-failed, naming the chosen path and not the temp file', async () => {
    // A directory where the file should go: the rename cannot replace it.
    const out = join(dir, 'Blocked.fdx');
    mkdirSync(out);
    const r = await codeOf(exportCommand({ epub, for: 'fdx', fountain, out }, deps()));
    expect(r.code).toBe('export-failed');
    expect(r.message).toContain(out);
    expect(r.message).not.toContain('.tmp');
    expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
  });

  test('--check builds nothing and says whether the FDX beside the book is current', async () => {
    const first = await exportCommand({ epub, for: 'fdx', fountain, check: true }, deps());
    expect(first.path).toBe(join(dir, 'Script.fdx'));
    expect(first.fresh).toBe(false);
    expect(existsSync(first.path)).toBe(false);

    await exportCommand({ epub, for: 'fdx', fountain }, deps());
    const now = Date.now() / 1000;
    utimesSync(fountain, now - 100, now - 100);
    utimesSync(first.path, now, now);
    expect((await exportCommand({ epub, for: 'fdx', fountain, check: true }, deps())).fresh).toBe(true);

    // The script changed after the FDX was written.
    utimesSync(fountain, now + 100, now + 100);
    expect((await exportCommand({ epub, for: 'fdx', fountain, check: true }, deps())).fresh).toBe(false);
  });

  test('the unknown --for message names all three formats', async () => {
    const r = await codeOf(exportCommand({ epub, for: 'pdf' }, deps()));
    expect(r.code).toBe('usage');
    for (const word of ['epub', 'kindle', 'fdx']) expect(r.message).toContain(word);
  });
});

describe('screepub export --for fdx (through the CLI)', () => {
  test('--json prints one object with the path, and the file is there', () => {
    const out = join(dir, 'Saved.fdx');
    const proc = Bun.spawnSync(
      ['bun', 'src/cli.ts', 'export', epub, '--for', 'fdx', '--fountain', fountain, '--out', out, '--json'],
      { env: process.env },
    );
    const stdout = proc.stdout.toString();
    expect(proc.exitCode).toBe(0);
    expect(stdout.trim().split('\n')).toHaveLength(1);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    expect(answer.path).toBe(out);
    expect(answer.format).toBe('fdx');
    expect(answer.extension).toBe('fdx');
    expect(readFileSync(out, 'utf8')).toBe(fountainToFdx(FOUNTAIN));
  });

  test('a refusal is one JSON object with a code and a message', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', epub, '--for', 'fdx', '--json'], { env: process.env });
    expect(proc.exitCode).toBe(1);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.ok).toBe(false);
    expect(answer.error.code).toBe('usage');
    expect(Object.keys(answer.error).sort()).toEqual(['code', 'message']);
  });

  test('export --help names fdx', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', '--help'], { env: process.env });
    expect(proc.stdout.toString()).toContain('fdx');
  });
});

// The conversion's own --fdx: the same FDX, written beside the other
// outputs in the same run, the way --preview-html is.
describe('screepub <script> --fdx <file>', () => {
  test('a PDF conversion writes the FDX of the .fountain it wrote, and names it in --json', () => {
    const pdf = join(new URL('..', import.meta.url).pathname, 'tests', 'fixtures', 'screenplay.pdf');
    const fdx = join(dir, 'Converted.fdx');
    const proc = Bun.spawnSync(
      ['bun', 'src/cli.ts', pdf, '-o', join(dir, 'Converted.epub'), '--fdx', fdx, '--json'],
      { env: process.env },
    );
    expect(proc.exitCode).toBe(0);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.fdxPath).toBe(fdx);
    expect(readFileSync(fdx, 'utf8')).toBe(fountainToFdx(readFileSync(answer.fountainPath, 'utf8')));
  });

  test('a Fountain conversion writes the FDX of that Fountain', () => {
    const fdx = join(dir, 'FromFountain.fdx');
    const proc = Bun.spawnSync(
      ['bun', 'src/cli.ts', fountain, '-o', join(dir, 'Out.epub'), '--fdx', fdx],
      { env: process.env },
    );
    expect(proc.exitCode).toBe(0);
    expect(proc.stdout.toString()).toContain(`wrote ${fdx}`);
    expect(readFileSync(fdx, 'utf8')).toBe(fountainToFdx(FOUNTAIN));
  });

  test('without --fdx no FDX is written and the answer has no fdxPath', () => {
    const proc = Bun.spawnSync(
      ['bun', 'src/cli.ts', fountain, '-o', join(dir, 'Out.epub'), '--json'],
      { env: process.env },
    );
    const answer = JSON.parse(proc.stdout.toString());
    expect('fdxPath' in answer).toBe(false);
    expect(readdirSync(dir).filter((f) => f.endsWith('.fdx'))).toEqual([]);
  });

  test('--help names --fdx', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', '--help'], { env: process.env });
    expect(proc.stdout.toString()).toContain('--fdx <file>');
  });
});
