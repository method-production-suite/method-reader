// JPEG2000: the format pdf.js cannot decode out of its own bundle.
//
// pdf.js 6.x keeps its JPX, JBIG2 and CCITT decoders OUTSIDE pdf.worker.mjs and
// loads them at runtime from the directory named by the `wasmUrl` option.
// Without one, every image in those formats fails to decode — and a picture
// that will not decode is indistinguishable, downstream, from a picture the
// script never had. A real 110-page script-plus-lookbook lost all 34 of its
// pictures exactly that way, with `imageCount: none` and one warning.
//
// So this file is the guard on src/parser/pdfjs-wasm.ts, and it is deliberately
// end to end: the unit assertions below (the three decoder filenames, the null
// on failure) cannot see the thing that actually broke, which is pdf.js finding
// the files. Only opening a document with a JPX image in it can.
//
// Stays the first import, like extract.test.ts. Not because pdf.mjs cannot
// load without it: on pdfjs-dist 6.3.289 a bare import loads under bun 1.3.14
// (checked 2026-10-01). It hands getDocument the statically imported worker,
// gives the modern build the DOMMatrix shim it needs for rendering, and lets
// quiet-pdfjs.ts patch console.warn before pdf.mjs is evaluated; see
// pdfjs-shims.ts.
import '../src/parser/pdfjs-shims';
import { test, expect, describe, beforeAll, afterAll } from 'bun:test';
import { getDocument } from 'pdfjs-dist/build/pdf.mjs';
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { collectImagePlacements, resolveImageData, toRgb } from '../src/parser/images';
import { DECODER_FILES, materializeDecoders, pdfjsWasmDir } from '../src/parser/pdfjs-wasm';
import { extractDocument, type ExtractResult } from '../src/parser/extract';
import type { RawImage } from '../src/parser/types';

// One scratch folder for the whole file, removed at the end: the rule in
// temp-hygiene.test.ts, which did not exist when this file was written.
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-jpx-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** The dimensions a PNG declares in its IHDR (bytes 16..24). Read off the
 * bytes that actually ship, since `RawImage` no longer carries them beside the
 * file: this asserts the encoder's output rather than a field copied from it,
 * which is the stronger of the two. */
function pngSize(b: Uint8Array): [number, number] {
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  return [dv.getUint32(16), dv.getUint32(20)];
}

/** Fresh bytes every time: getDocument TRANSFERS the buffer to the worker. */
const fixture = (name: string) =>
  Bun.file(`tests/fixtures/${name}`).arrayBuffer().then((b) => new Uint8Array(b));

/**
 * tools/make-fixture.py's `rgb_gradient`, transcribed.
 *
 * A SECOND copy of a formula, which the codebase otherwise refuses — and this
 * is the case that earns it. The point of the assertion is that the pixels
 * pdf.js hands back are the ones the fixture's author drew, so both sides have
 * to be able to say what those are INDEPENDENTLY. Importing the truth from the
 * generator (were that possible across the language boundary) would make the
 * test pass on any pair of matching wrong answers. Eight-pixel blocks varying
 * in both axes, so a transposed or reversed row is visible here.
 */
function rgbGradient(w: number, h: number, seed: number): Uint8Array {
  const px = new Uint8Array(w * h * 3);
  let i = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      px[i++] = (Math.floor(x / 8) * 37 + seed) % 256;
      px[i++] = (Math.floor(y / 8) * 53 + seed) % 256;
      px[i++] = (seed * 3) % 256;
    }
  }
  return px;
}

// make-fixture.py's JPX_GRADIENT.
const JPX_W = 64;
const JPX_H = 48;
const JPX_SEED = 71;

describe('the decoder directory', () => {
  test('carries exactly the filenames pdf.js will ask for', () => {
    // pdf.js concatenates these onto wasmUrl (`WasmImage._filename` and
    // `._noWasmFilename`, re-checked on 6.3.289; tests/pdfjs-wasm.test.ts reads
    // them out of the installed worker). A name that drifts is a decoder that
    // ships and is never found, which fails silently.
    expect(DECODER_FILES.map(([name]) => name)).toEqual([
      'openjpeg_nowasm_fallback.js',
      'jbig2.wasm',
      'jbig2_nowasm_fallback.js',
    ]);
  });

  test('materializes those files, under those names, into the directory given', () => {
    const base = mkdtempSync(join(SCRATCH, 'decoders-'));
    const dir = materializeDecoders(() => base);
    expect(dir).toBe(`${base}/`);
    expect(readdirSync(base).sort()).toEqual([...DECODER_FILES.map(([n]) => n)].sort());
  });

  test('ends in the slash pdf.js requires, since it just concatenates', () => {
    // getFactoryUrlProp THROWS on a url without a trailing slash (pdf.mjs), so
    // getting this wrong would not be a missing picture, it would be every
    // conversion failing to open its document.
    expect(pdfjsWasmDir()!.endsWith('/')).toBe(true);
  });

  test('returns null rather than throwing when it cannot make a directory', () => {
    // The branch that keeps a full or read-only temp directory from being the
    // reason a screenplay does not become a book at all.
    expect(materializeDecoders(() => {
      throw new Error('no temp directory');
    })).toBeNull();
  });
});

describe('a JPEG2000 picture through the whole parser', () => {
  let images: RawImage[];
  let stats: ExtractResult['imageStats'];
  let decoded: Uint8Array | null = null;

  beforeAll(async () => {
    const out = await extractDocument(await fixture('jpx.pdf'));
    images = out.images;
    stats = out.imageStats;

    // The pixels as pdf.js hands them over, before png.ts encodes them. Read
    // separately so the pixel assertion below is about the DECODER and not
    // about our encoder, which has its own suite.
    const pdf = await getDocument({ data: await fixture('jpx.pdf'), wasmUrl: pdfjsWasmDir()! })
      .promise;
    for (let n = 1; n <= pdf.numPages && !decoded; n++) {
      const page = await pdf.getPage(n);
      for (const place of collectImagePlacements(await page.getOperatorList())) {
        const img = await resolveImageData(page.objs, page.commonObjs, place.objId);
        if (img) decoded = toRgb(img);
      }
    }
  });

  test('reaches the book instead of vanishing', () => {
    // The whole defect in one line: before the decoder directory this was 0
    // kept and 1 failed, on a fixture whose only picture clears every filter.
    expect(images.length).toBe(1);
    expect(stats.failed).toBe(0);
  });

  test('at the resolution the codestream declares', () => {
    expect(pngSize(images[0].bytes)).toEqual([JPX_W, JPX_H]);
  });

  test('and reports the decoders as available, so no warning claims otherwise', () => {
    expect(stats.decodersUnavailable).toBeUndefined();
  });

  test('with the pixels the fixture drew, byte for byte', () => {
    // CORRECT, not merely present. The codestream is lossless (opj_compress's
    // default), so this is an equality and not a tolerance — a decoder that
    // half-worked, or one that read the components in the wrong order, would
    // still produce a picture of the right size that passed every test above.
    expect(decoded).not.toBeNull();
    expect([...decoded!]).toEqual([...rgbGradient(JPX_W, JPX_H, JPX_SEED)]);
  });
});

// The OTHER half: what a filmmaker is told when the decoders cannot be put
// anywhere pdf.js can load them.
//
// In a SUBPROCESS, and it has to be: the directory is materialized once per
// process and the answer is memoized (including the failure), so a test that
// broke the temp directory in THIS process would either come too late or poison
// every test after it. Pointing TMPDIR at a regular file is what makes it fail
// — `mkdtemp` under a non-directory is ENOTDIR on every platform, where an
// unwritable directory depends on who is running the suite.
describe('when the decoders cannot be unpacked', () => {
  const ROOT = new URL('..', import.meta.url).pathname;
  let json: { ok: boolean; imageCount?: number; warnings: string[] };
  let exitCode = -1;

  beforeAll(async () => {
    const notADir = join(SCRATCH, 'tmpdir-is-a-file');
    writeFileSync(notADir, 'not a directory');
    const proc = Bun.spawn(
      ['bun', `${ROOT}src/cli.ts`, `${ROOT}tests/fixtures/jpx.pdf`,
       '-o', join(SCRATCH, 'out.epub'), '--no-fountain', '--json'],
      { stdout: 'pipe', stderr: 'pipe', env: { ...process.env, TMPDIR: notADir } },
    );
    const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    exitCode = code;
    json = JSON.parse(out);
  });

  test('the script still becomes a book', () => {
    // The picture is lost; the conversion is not. A temp directory we cannot
    // write must never be the reason a screenplay fails to convert at all.
    expect(exitCode).toBe(0);
    expect(json.ok).toBe(true);
  });

  test('and the loss is reported with the cause, which is ours and not the PDF', () => {
    expect(json.imageCount).toBeUndefined();
    expect(json.warnings).toEqual([
      '1 picture could not be read out of the PDF, so it was left out.',
      'Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read.',
    ]);
  });
});
