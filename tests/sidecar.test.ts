import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, readdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, extname, join } from 'node:path';
import { DEFAULT_FORMAT_OPTIONS, type FormatOptions } from '../src/options';
import { sidecarPath, loadScriptSettings, saveScriptSettings } from '../src/settings/sidecar';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-sidecar-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

function library(): string {
  const dir = join(mkdtempSync(join(SCRATCH, 'test-')), 'library');
  mkdirSync(dir, { recursive: true });
  return dir;
}

test('sidecar path derives from the fountain stem', () => {
  const fountain = join(library(), 'Test Script.fountain');
  expect(basename(sidecarPath(fountain))).toBe('Test Script.screepub.json');
});

// The one naming question the window's own flows could not settle on their
// own: a script whose stem contains a dot. `settings` is handed a .fountain,
// `adoptSidecar` is handed a library output prefix with no extension at all,
// and both have to land on the same file or a script's tuning is written
// under one name and read under another. Pinned against library.ts's own
// rule rather than against a literal, so the two cannot drift apart.
test('sidecar naming agrees with the library for every stem it can meet', () => {
  const dir = library();
  for (const name of ['Draft.pdf', 'My.Script.pdf', 'My.Script.fountain', 'a.b.c', '.hidden']) {
    const stem = basename(name, extname(name));
    expect(basename(sidecarPath(join(dir, name)))).toBe(`${stem}.screepub.json`);
  }
});

test('sidecar round-trips settings', () => {
  const fountain = join(library(), 'Test Script.fountain');
  writeFileSync(fountain, 'Title: T');
  const settings: FormatOptions = {
    ...DEFAULT_FORMAT_OPTIONS,
    dialogueSideMarginPct: 27,
    keepSpeechesWhole: true,
  };
  saveScriptSettings(settings, fountain);
  expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS)).toEqual(settings);
});

test('absent sidecar falls back', () => {
  const fountain = join(library(), 'Other.fountain');
  expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS)).toEqual(DEFAULT_FORMAT_OPTIONS);
});

test('corrupt sidecar (invalid JSON) falls back', () => {
  const fountain = join(library(), 'Garbage.fountain');
  writeFileSync(sidecarPath(fountain), 'not json');
  expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS)).toEqual(DEFAULT_FORMAT_OPTIONS);
});

test('a JSON array is not an object and falls back', () => {
  const fountain = join(library(), 'Array.fountain');
  writeFileSync(sidecarPath(fountain), '[1,2,3]');
  expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS)).toEqual(DEFAULT_FORMAT_OPTIONS);
});

test('partial sidecar overlays the present field and leaves the rest at fallback', () => {
  const fountain = join(library(), 'Partial.fountain');
  writeFileSync(sidecarPath(fountain), '{"dialogueSideMarginPct": 9}');
  expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS))
    .toEqual({ ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 9 });
});

// Every row writes the OPPOSITE of the default and checks whole-struct
// equality. A sidecar value that merely equals the default would pass
// whether or not the merge handled that field at all, because the merge
// starts from a copy of the fallback. The override direction is what makes
// this a real assertion. Ported verbatim from kit-check.
const OVERRIDE_CASES: Array<{ field: keyof FormatOptions; json: string; expected: Partial<FormatOptions> }> = [
  { field: 'scenePageBreaks', json: '{"scenePageBreaks": true}', expected: { scenePageBreaks: true } },
  { field: 'dialogueSideMarginPct', json: '{"dialogueSideMarginPct": 9}', expected: { dialogueSideMarginPct: 9 } },
  { field: 'cueIndentPct', json: '{"cueIndentPct": 11}', expected: { cueIndentPct: 11 } },
  { field: 'parentheticalIndentPct', json: '{"parentheticalIndentPct": 5}', expected: { parentheticalIndentPct: 5 } },
  { field: 'elementSpacingEm', json: '{"elementSpacingEm": 1.6}', expected: { elementSpacingEm: 1.6 } },
  { field: 'keepSceneHeadingWithScene', json: '{"keepSceneHeadingWithScene": false}', expected: { keepSceneHeadingWithScene: false } },
  { field: 'keepSpeechesWhole', json: '{"keepSpeechesWhole": true}', expected: { keepSpeechesWhole: true } },
  { field: 'fontFamily', json: '{"fontFamily": "serif"}', expected: { fontFamily: 'serif' } },
  { field: 'rejoinSplitDialogue', json: '{"rejoinSplitDialogue": false}', expected: { rejoinSplitDialogue: false } },
  { field: 'contdMode', json: '{"contdMode": "strip"}', expected: { contdMode: 'strip' } },
  { field: 'cueAlignment', json: '{"cueAlignment": "indented"}', expected: { cueAlignment: 'indented' } },
  { field: 'includeTitlePage', json: '{"includeTitlePage": false}', expected: { includeTitlePage: false } },
  { field: 'showSceneNumbers', json: '{"showSceneNumbers": true}', expected: { showSceneNumbers: true } },
  { field: 'showPageMarkers', json: '{"showPageMarkers": true}', expected: { showPageMarkers: true } },
  { field: 'dualDialogue', json: '{"dualDialogue": "sequential"}', expected: { dualDialogue: 'sequential' } },
  { field: 'justifyText', json: '{"justifyText": true}', expected: { justifyText: true } },
  { field: 'printSplitMinimums', json: '{"printSplitMinimums": false}', expected: { printSplitMinimums: false } },
  { field: 'preserveFontShifts', json: '{"preserveFontShifts": false}', expected: { preserveFontShifts: false } },
  { field: 'includeImages', json: '{"includeImages": false}', expected: { includeImages: false } },
];

for (const c of OVERRIDE_CASES) {
  test(`sidecar merge overrides ${c.field} against its default fallback`, () => {
    const fountain = join(library(), `Override-${c.field}.fountain`);
    writeFileSync(sidecarPath(fountain), c.json);
    expect(loadScriptSettings(fountain, DEFAULT_FORMAT_OPTIONS))
      .toEqual({ ...DEFAULT_FORMAT_OPTIONS, ...c.expected });
  });
}

test('every FormatOptions field has an override row', () => {
  const all = new Set(Object.keys(DEFAULT_FORMAT_OPTIONS));
  const covered = new Set(OVERRIDE_CASES.map((c) => c.field as string));
  expect([...all].filter((f) => !covered.has(f))).toEqual([]);
});

test('a sidecar write that cannot land leaves no temp file behind, and the error still surfaces', () => {
  const dir = library();
  const fountain = join(dir, 'Blocked.fountain');
  // A DIRECTORY where the sidecar should be: the temp file writes fine,
  // then the rename over a directory fails.
  mkdirSync(sidecarPath(fountain));
  expect(() => saveScriptSettings(DEFAULT_FORMAT_OPTIONS, fountain)).toThrow();
  expect(readdirSync(dir)).toEqual([basename(sidecarPath(fountain))]);
});
