// Our AZW3, read by a second reader that is not Calibre's: KindleUnpack
// (github.com/kevinhendricks/KindleUnpack), the plain-Python unpacker the
// MobileRead community uses on Amazon's own books. It shares no code with
// our writer or with Calibre's reader, and it trusts different fields: it
// rebuilds the files from the skeleton and fragment tables, reads the NCX
// into a toc.ncx, and finds pictures and the cover from the first-resource
// field and EXTH 201.
//
// Runs only where KindleUnpack is: SCREEPUB_KINDLEUNPACK names its folder
// (the one holding lib/kindleunpack.py), and python3 must be on the PATH.
// Skips otherwise, as in CI. KindleUnpack never touches Calibre; each book
// is unpacked in its own scratch folder, and python3 runs isolated (-I), so
// nothing beside the runner script it is handed is imported from the
// working folder.
import { afterAll, describe, expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildAzw3 } from '../src/azw3/writer';
import { readEpub } from '../src/azw3/epub';
import { viewOfCalibreEpub, viewOfOurEpub } from './helpers/calibre-read';
import { fixtureEpub, type FixtureName } from './helpers/fixture-epubs';

const KU = process.env.SCREEPUB_KINDLEUNPACK ?? '';
const python = Bun.which('python3');
const available = KU !== '' && python !== null && existsSync(join(KU, 'lib', 'kindleunpack.py'));
const withKu = available ? test : test.skip;

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-azw3-ku-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

// The runner: KindleUnpack's own unpackBook, epub 3 out, nothing dumped.
const RUNNER = join(SCRATCH, 'ku_run.py');
writeFileSync(RUNNER, [
  'import sys',
  'sys.path.insert(0, sys.argv[1])',
  'from lib.kindleunpack import unpackBook',
  "unpackBook(sys.argv[2], sys.argv[3], epubver='3')",
  '',
].join('\n'));

function unpack(azw3: string, out: string): { code: number; log: string } {
  const p = Bun.spawnSync([python!, '-I', RUNNER, KU, azw3, out], { cwd: SCRATCH, env: process.env, stdout: 'pipe', stderr: 'pipe' });
  return { code: p.exitCode ?? -1, log: `${p.stdout.toString()}\n${p.stderr.toString()}` };
}

describe('KindleUnpack reads our AZW3', () => {
  for (const name of ['field-station', 'images', 'torture', 'screenplay'] as FixtureName[]) {
    withKu(`${name}: the text, the TOC, the metadata and the pictures come back, with no complaint`, async () => {
      const dir = mkdtempSync(join(SCRATCH, `${name}-`));
      const epub = await fixtureEpub(name);
      const azw3 = join(dir, `${name}.azw3`);
      writeFileSync(azw3, await buildAzw3(epub));
      const out = join(dir, 'out');
      const run = unpack(azw3, out);
      expect(run.code, run.log).toBe(0);
      expect(run.log).not.toMatch(/Traceback|Error|incorrect|mismatch/);

      const ours = await readEpub(epub);
      // Its one warning is its own: with no pictures, a book's
      // first-resource field is the null index, as Calibre writes it too,
      // and KindleUnpack then looks for pictures among the index records.
      const warnings = run.log.split('\n').filter((l) => /warning/i.test(l));
      if (ours.images.length > 0) expect(warnings).toEqual([]);
      else for (const w of warnings) expect(w).toMatch(/does not contain a recognised resource/);

      const rebuilt = join(out, 'mobi8', `${name}.epub`);
      expect(existsSync(rebuilt), readdirSync(join(out, 'mobi8')).join(', ')).toBe(true);
      // KindleUnpack's EPUB is OPF + NCX, the shape the Calibre reader's
      // view already reads; its own cover page carries no text.
      const theirs = await viewOfCalibreEpub(new Uint8Array(readFileSync(rebuilt)));
      const want = await viewOfOurEpub(epub);
      expect(theirs.title).toBe(want.title);
      expect(theirs.authors).toEqual(want.authors);
      expect(theirs.toc).toEqual(want.toc);
      expect(theirs.blocks).toEqual(want.blocks);
      expect(theirs.text).toBe(want.text);
      const sorted = (a: Uint8Array[]) => a.map((b) => Buffer.from(b).toString('base64')).sort();
      expect(sorted(theirs.images)).toEqual(sorted(want.images));
      expect(theirs.cover === null ? null : Buffer.from(theirs.cover).toString('base64'))
        .toBe(want.cover === null ? null : Buffer.from(want.cover).toString('base64'));
      // The ASIN it reads is the book's own identifier.
      const opf = readFileSync(join(out, 'mobi8', 'OEBPS', 'content.opf'), 'utf8');
      expect(opf).toContain(`<meta name="ASIN" content="${ours.identifier!.split(':').at(-1)}" />`);
      expect(opf).toContain('<meta name="cdeType" content="EBOK" />');
    }, 60_000);
  }
});
