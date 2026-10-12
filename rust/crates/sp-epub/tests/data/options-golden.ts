// TRANSITIONAL TypeScript, deleted at cutover with the engine: the
// TypeScript renderer's answers for every oracle dump under every option
// set below, as hashes, for rust/crates/sp-epub/tests/options_matrix.rs.
//
// The oracle dumps are made with the default options only. These sets are
// the ones the TypeScript tests (epub, epub-images, options, css-rules)
// exercise, plus values chosen to hit the arithmetic edges (toFixed ties,
// Math.round halves, fractional percentages). Every set renders the dump's
// own script.fountain with its picture folder, exactly as convert.ts's
// renderBooks does, with the clock frozen at the dump's SOURCE_DATE_EPOCH.
//
//   bun --preload ./tools/oracle/freeze-clock.ts \
//     rust/crates/sp-epub/tests/data/options-golden.ts
//
// writes tests/data/options-golden.json (committed fixtures) and, when the
// checkout has real dumps, rust/oracle-real/sp-epub-options.json (gitignored).
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { Fountain } from 'fountain-js';
import JSZip from 'jszip';
import { buildEpub, type BookImage } from '../../../../../src/epub/build';
import { tokensToBody, tokensToPreviewHtml } from '../../../../../src/epub/html';
import { resolveFormatOptions } from '../../../../../src/options';

const REPO = resolve(import.meta.dir, '../../../../..');

export const OPTION_SETS: Record<string, Record<string, unknown>> = {
  defaults: {},
  'scene-breaks': { scenePageBreaks: true },
  'whole-speeches-loose-headings': { keepSpeechesWhole: true, keepSceneHeadingWithScene: false },
  'tight-justified': { elementSpacingEm: 0.6, justifyText: true },
  'indented-serif': { cueAlignment: 'indented', fontFamily: 'serif' },
  'loose-no-pictures': { printSplitMinimums: false, includeImages: false },
  'no-font-shifts': { preserveFontShifts: false },
  'no-cover': { promoteCoverImage: false },
  'no-title-page': { includeTitlePage: false },
  'scene-numbers': { showSceneNumbers: true },
  geometry: { cueAlignment: 'indented', dialogueSideMarginPct: 25, cueIndentPct: 40, parentheticalIndentPct: 12 },
  'odd-geometry': {
    cueAlignment: 'indented', dialogueSideMarginPct: 12.5, cueIndentPct: 33.3,
    parentheticalIndentPct: 13, elementSpacingEm: 0.7,
  },
  'spacing-max-sans': { elementSpacingEm: 2, fontFamily: 'sans' },
  'spacing-min': { elementSpacingEm: 0.4 },
  'spacing-ties': { elementSpacingEm: 0.625 },
  'spacing-quarter': { elementSpacingEm: 1.25, parentheticalIndentPct: 17 },
  'clamped': { elementSpacingEm: 9, dialogueSideMarginPct: -4, cueIndentPct: 99, parentheticalIndentPct: 41, cueAlignment: 'indented' },
  'everything-on': {
    scenePageBreaks: true, keepSpeechesWhole: true, showSceneNumbers: true, justifyText: true,
  },
  'serializer-only': {
    dualDialogue: 'sequential', rejoinSplitDialogue: false, contdMode: 'strip', showPageMarkers: false,
  },
  'convert-geometry': { dialogueSideMarginPct: 28, scenePageBreaks: true },
};

const sha256 = (b: Uint8Array | string) => createHash('sha256').update(b).digest('hex');

/** Each entry's method from its local header (0 stored, 8 deflated). */
function methods(bytes: Uint8Array): Map<string, number> {
  const out = new Map<string, number>();
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i + 30 <= bytes.length && dv.getUint32(i, true) === 0x04034b50;) {
    const nameLen = dv.getUint16(i + 26, true);
    const name = new TextDecoder().decode(bytes.subarray(i + 30, i + 30 + nameLen));
    out.set(name, dv.getUint16(i + 8, true));
    i += 30 + nameLen + dv.getUint16(i + 28, true) + dv.getUint32(i + 18, true);
  }
  return out;
}

async function answers(dir: string) {
  const fountainText = readFileSync(join(dir, 'script.fountain'), 'utf8');
  const answer = JSON.parse(readFileSync(join(dir, 'convert.json'), 'utf8'));
  const picturesDir = join(dir, 'images');
  const images: BookImage[] = existsSync(picturesDir)
    ? readdirSync(picturesDir).sort().map((name) => ({
      name, bytes: new Uint8Array(readFileSync(join(picturesDir, name))),
    }))
    : [];
  const { tokens } = new Fountain().parse(fountainText, true);
  const meta = {
    identifier: `urn:screepub:${sha256(fountainText).slice(0, 32)}`,
    title: answer.title,
    author: answer.author,
  };
  const out: Record<string, unknown> = {};
  for (const [name, partial] of Object.entries(OPTION_SETS)) {
    const format = resolveFormatOptions(partial);
    const body = tokensToBody(tokens, { format, images: new Set(images.map((i) => i.name)) });
    const epub = await buildEpub(meta, body, format, images);
    const zip = await JSZip.loadAsync(epub);
    const stored = methods(epub);
    const entries: [string, boolean, string][] = [];
    for (const [entry, file] of Object.entries(zip.files)) {
      entries.push([entry, stored.get(entry) === 0, sha256(await file.async('uint8array'))]);
    }
    const byName = new Map(images.map((i) => [i.name, i.bytes]));
    const preview = tokensToPreviewHtml(tokens, format, byName);
    out[name] = { entries, preview: sha256(preview) };
  }
  return out;
}

async function folder(root: string): Promise<Record<string, unknown>> {
  const out: Record<string, unknown> = {};
  for (const d of readdirSync(root).sort()) {
    const dir = join(root, d);
    if (!existsSync(join(dir, 'book.epub'))) continue;
    out[d] = await answers(dir);
  }
  return out;
}

const golden = { optionSets: OPTION_SETS, fixtures: await folder(join(REPO, 'rust/oracle')) };
writeFileSync(join(import.meta.dir, 'options-golden.json'), `${JSON.stringify(golden, null, 1)}\n`);
const real = join(REPO, 'rust/oracle-real');
if (existsSync(real)) {
  writeFileSync(
    join(real, 'sp-epub-options.json'),
    `${JSON.stringify({ optionSets: OPTION_SETS, fixtures: await folder(real) })}\n`,
  );
}
