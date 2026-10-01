// images.ts against STUBS. Everything that needs a document pdf.js actually
// opened lives in images-fixture.test.ts, which also owns extract.ts's page
// loop: a stub answers however it is written, and a regression in the loop
// should not report against a file named for the parser module.
//
// MUST stay the first import, like extract.test.ts: pdf.mjs builds a
// `new DOMMatrix()` at module scope, so evaluating it before the shim throws
// ReferenceError on load and every test in the file fails as one.
import '../src/parser/pdfjs-shims';
import { test, expect, describe } from 'bun:test';
import { OPS } from 'pdfjs-dist/build/pdf.mjs';
import {
  collectImagePlacements,
  collectPageImages,
  dominantPageSize,
  IMAGE_BUDGET_MS,
  finishImages,
  isGeneratedImageFile,
  newImageRun,
  resolveImageData,
  toRawImages,
  toRgb,
  type EncodedCandidate,
  type OpList,
  type PdfImage,
} from '../src/parser/images';

/** The dimensions a PNG declares in its IHDR (bytes 16..24). Read off the
 * bytes that actually ship, since `RawImage` no longer carries them beside the
 * file: this asserts the encoder's output rather than a field copied from it,
 * which is the stronger of the two. */
function pngSize(b: Uint8Array): [number, number] {
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  return [dv.getUint32(16), dv.getUint32(20)];
}

type Op = [fn: number, args: unknown];

const ops = (...list: Op[]): OpList => ({
  fnArray: list.map((o) => o[0]),
  argsArray: list.map((o) => o[1]),
});

const q: Op = [OPS.save, []];
const Q: Op = [OPS.restore, []];
const cm = (...m: number[]): Op => [OPS.transform, m];
const Do = (name: string, px = 8, py = 8): Op => [OPS.paintImageXObject, [name, px, py]];
/** `Do` on a form XObject: pdf.js carries the form's /Matrix in the begin op. */
const formBegin = (matrix: number[] | null): Op => [OPS.paintFormXObjectBegin, [matrix, null]];
const formEnd: Op = [OPS.paintFormXObjectEnd, []];

/** The `q W 0 0 H X Y cm /Im Do Q` pdf.js emits for each placed image. */
function opList(entries: { name: string; ctm: number[]; px: [number, number] }[]): OpList {
  return ops(...entries.flatMap((e) => [q, cm(...e.ctm), Do(e.name, e.px[0], e.px[1]), Q]));
}

describe('collectImagePlacements', () => {
  test('reads position and rendered size from the CTM', () => {
    const list = opList([{ name: 'img_p0_1', ctm: [144, 0, 0, 108, 108, 540], px: [240, 180] }]);
    expect(collectImagePlacements(list)).toEqual([
      { objId: 'img_p0_1', x0: 108, y0: 540, x1: 252, y1: 648, srcPxWidth: 240, srcPxHeight: 180 },
    ]);
  });

  test('handles a y-flipped placement', () => {
    // Final Draft draws under a flip. Transforming one corner would put the
    // box below the page; all four corners plus min/max is what fixes it.
    const list = opList([{ name: 'a', ctm: [100, 0, 0, -80, 50, 400], px: [10, 10] }]);
    const [p] = collectImagePlacements(list);
    expect(p.y0).toBe(320);
    expect(p.y1).toBe(400);
    expect(p.x0).toBe(50);
    expect(p.x1).toBe(150);
  });

  test('restores the matrix after Q, so a later image is not offset', () => {
    const list = opList([
      { name: 'a', ctm: [10, 0, 0, 10, 500, 500], px: [4, 4] },
      { name: 'b', ctm: [20, 0, 0, 20, 5, 5], px: [4, 4] },
    ]);
    const [, b] = collectImagePlacements(list);
    expect(b.x0).toBe(5);
    expect(b.y0).toBe(5);
  });

  test('composes an inner cm inside the outer one, not beside it', () => {
    // The whole concat: a translate under a 2x scale moves 2x as far. Transpose
    // the multiply and this lands at (10, 20) — a plausible box, off the art.
    const list = ops(q, cm(2, 0, 0, 2, 0, 0), cm(1, 0, 0, 1, 10, 20), Do('a'), Q);
    const [p] = collectImagePlacements(list);
    expect([p.x0, p.y0, p.x1, p.y1]).toEqual([20, 40, 22, 42]);
  });

  test('a nested q/Q leaves the outer matrix in force', () => {
    const list = ops(
      q,
      cm(2, 0, 0, 2, 100, 100),
      q,
      cm(1, 0, 0, 1, 5, 5),
      Do('inner'),
      Q,
      Do('outer'),
      Q,
    );
    const [inner, outer] = collectImagePlacements(list);
    expect([inner.x0, inner.y0]).toEqual([110, 110]);
    expect([outer.x0, outer.y0]).toEqual([100, 100]);
  });

  test('two images in one q: the second cm composes on the first', () => {
    const list = ops(q, cm(1, 0, 0, 1, 10, 10), Do('a'), cm(1, 0, 0, 1, 10, 10), Do('b'), Q);
    const [a, b] = collectImagePlacements(list);
    expect([a.x0, a.y0]).toEqual([10, 10]);
    expect([b.x0, b.y0]).toEqual([20, 20]);
  });

  test('an image drawn with no cm at all sits on the unit square', () => {
    const list = ops(q, Do('a'), Q);
    expect(collectImagePlacements(list)).toEqual([
      { objId: 'a', x0: 0, y0: 0, x1: 1, y1: 1, srcPxWidth: 8, srcPxHeight: 8 },
    ]);
  });

  test('a rotated placement reports the page region it covers', () => {
    // Quarter turn: the box is axis-aligned and its aspect is the transpose of
    // the source's. Reported rather than skipped, because a turned storyboard
    // panel still occupies that region and still has to go somewhere.
    const list = ops(q, cm(0, 100, -50, 0, 200, 300), Do('a'), Q);
    expect(collectImagePlacements(list)).toEqual([
      { objId: 'a', x0: 150, y0: 300, x1: 200, y1: 400, srcPxWidth: 8, srcPxHeight: 8 },
    ]);
  });

  test('a slanted placement needs all four corners, not the diagonal', () => {
    // A 3-4-5 turn: the (0,0)-(1,1) diagonal spans x 80..100 while the picture
    // spans 20..160. Flips and quarter turns forgive mapping two corners; this
    // is the placement that does not.
    const list = ops(q, cm(60, 80, -80, 60, 100, 100), Do('a'), Q);
    const [p] = collectImagePlacements(list);
    expect([p.x0, p.x1]).toEqual([20, 160]);
    expect([p.y0, p.y1]).toEqual([100, 240]);
  });

  test('a zero-scale placement is reported, degenerate box and all', () => {
    // Geometry only: a collapsed box is a fact about the page, and dropping it
    // here would hide it from the filters that own that judgement.
    const list = ops(q, cm(0, 0, 0, 40, 72, 500), Do('a'), Q);
    const [p] = collectImagePlacements(list);
    expect(p.x0).toBe(72);
    expect(p.x1).toBe(72);
    expect(p.y1 - p.y0).toBe(40);
  });

  test('an unbalanced Q drops the matrix rather than carrying it forward', () => {
    // The stray Q lands AFTER the cm on purpose. With the matrix still at
    // identity the two candidate behaviours are indistinguishable and this test
    // cannot fail; here `?? IDENTITY` puts the image at the origin and `?? m`
    // leaves it at 3x on (9, 9).
    //
    // Which is a real judgement call, because we DIVERGE from pdf.js here: its
    // renderer makes restore() on an empty stack a no-op that keeps the CTM
    // (pdf.mjs, `restore()`: `if (this.stateStack.length === 0) ... return`).
    // extract.ts:190 chose otherwise and so does this walk. A viewer has to show
    // something for a corrupt content stream; a best-effort extractor does not,
    // and would rather park one image at the origin, where every size floor
    // downstream catches it, than carry a matrix it no longer trusts across
    // every image left on the page.
    const list = ops(Q, cm(3, 0, 0, 3, 9, 9), Q, Do('b'));
    const [b] = collectImagePlacements(list);
    expect([b.x0, b.y0, b.x1, b.y1]).toEqual([0, 0, 1, 1]);
  });

  test('a form XObject transforms and restores like q/cm ... Q', () => {
    // pdf.js does not expand a form into save/transform ops: the /Matrix rides
    // on paintFormXObjectBegin, and the canvas renderer saves, applies it, and
    // restores on End. Ignoring the pair puts placed art (Illustrator, InDesign)
    // at the wrong offset AND leaks the form's own transforms into the page.
    const list = ops(
      q,
      cm(2, 0, 0, 2, 0, 0),
      formBegin([1, 0, 0, 1, 10, 20]),
      Do('inside'),
      formEnd,
      Do('after'),
      Q,
    );
    const [inside, after] = collectImagePlacements(list);
    expect([inside.x0, inside.y0]).toEqual([20, 40]);
    expect([after.x0, after.y0]).toEqual([0, 0]);
  });

  test('a form with no /Matrix still saves and restores', () => {
    const list = ops(q, formBegin(null), cm(1, 0, 0, 1, 30, 30), Do('inside'), formEnd, Do('after'), Q);
    const [inside, after] = collectImagePlacements(list);
    expect([inside.x0, inside.y0]).toEqual([30, 30]);
    expect([after.x0, after.y0]).toEqual([0, 0]);
  });

  test('ignores images inside an annotation, which are markup on the script', () => {
    const list = ops(
      [OPS.beginAnnotation, ['annot1', [0, 0, 200, 100], [1, 0, 0, 1, 50, 50], [1, 0, 0, 1, 0, 0], false]],
      q,
      cm(60, 0, 0, 30, 0, 0),
      Do('stamp'),
      Q,
      [OPS.endAnnotation, []],
      q,
      cm(60, 0, 0, 30, 72, 400),
      Do('art'),
      Q,
    );
    expect(collectImagePlacements(list).map((p) => p.objId)).toEqual(['art']);
  });

  test('skips a paint whose pixel dimensions are not numbers', () => {
    const list = ops(
      [OPS.paintImageXObject, ['bad', undefined, 12]],
      [OPS.paintImageXObject, ['nan', Number.NaN, 12]],
      Do('good'),
    );
    expect(collectImagePlacements(list).map((p) => p.objId)).toEqual(['good']);
  });

  test('is best-effort: a malformed list yields no placements, never a throw', () => {
    expect(collectImagePlacements({ fnArray: [OPS.paintImageXObject], argsArray: [null] })).toEqual([]);
    expect(collectImagePlacements(null as never)).toEqual([]);
  });

  test('ignores image masks, which in screenplays are decoration', () => {
    // Args in pdf.js's real shape — an objId in `data`, with dimensions — so
    // that adding mask support would actually produce a placement here and
    // fail. A stub with null fields would be filtered by the guards above and
    // leave this passing whatever the walk did with masks.
    const list = ops(q, cm(9, 0, 0, 9, 72, 700), [
      OPS.paintImageMaskXObject,
      [{ data: 'mask_p0_1', width: 32, height: 32, interpolate: undefined, count: 1 }],
    ], Q);
    expect(collectImagePlacements(list)).toEqual([]);
  });
});

describe('toRgb', () => {
  test('passes RGB_24BPP through untouched', () => {
    const data = new Uint8ClampedArray([1, 2, 3, 4, 5, 6]);
    expect([...toRgb({ width: 2, height: 1, kind: 2, data })!]).toEqual([1, 2, 3, 4, 5, 6]);
  });

  test('reads the Uint8Array pdf.js actually sends, not only a clamped one', () => {
    // Every image in tests/fixtures/images.pdf arrives as a plain Uint8Array:
    // the direct DeviceRGB path hands back the decode stream's own buffer
    // (pdf.worker.mjs:33974). Only the general colour-space fallback allocates
    // a Uint8ClampedArray (:33784, :33794), so narrowing the field to the one a
    // design probe happens to sample is a type no real image satisfies.
    const data = new Uint8Array([9, 8, 7]);
    expect([...toRgb({ width: 1, height: 1, kind: 2, data })!]).toEqual([9, 8, 7]);
  });

  test('hands back bytes we own, not a view into pdf.js store', () => {
    // downscaleRgb returns THE SAME ARRAY for an image that already fits, so a
    // view handed out here would reach encodePng still pointing at the pixels
    // pdf.js is caching: one buffer, two owners, and one of them is a library.
    const data = new Uint8ClampedArray([1, 2, 3]);
    const rgb = toRgb({ width: 1, height: 1, kind: 2, data })!;
    rgb[0] = 200;
    expect(data[0]).toBe(1);
    expect(rgb).toBeInstanceOf(Uint8Array);
  });

  test('flattens RGBA_32BPP onto white, the page a screenplay is read on', () => {
    // The design's "Out of scope": transparent images are flattened onto white.
    // Two pixels, transparent then opaque. Dropping alpha instead — which is
    // what this test used to assert, against a comment claiming otherwise —
    // ships the first pixel as whatever RGB sat underneath it, so a logo cut
    // out over a dark backing arrives as art in a black rectangle.
    const data = new Uint8ClampedArray([10, 20, 30, 0, 40, 50, 60, 255]);
    expect([...toRgb({ width: 2, height: 1, kind: 3, data })!])
      .toEqual([255, 255, 255, 40, 50, 60]);
  });

  test('leaves a fully opaque RGBA pixel exactly as it was', () => {
    // The blend has to be an identity at a=255, not merely close to one: a
    // rounding slip here tints every photograph in the book.
    const data = new Uint8ClampedArray([1, 128, 254, 255]);
    expect([...toRgb({ width: 1, height: 1, kind: 3, data })!]).toEqual([1, 128, 254]);
  });

  test('blends a partly transparent pixel instead of stepping to one end', () => {
    // round(c * a/255 + 255 * (1 - a/255)) at half alpha. A threshold — pick
    // white under 128 and the colour above — passes both tests above and turns
    // every soft edge in a storyboard into a jagged one.
    const data = new Uint8ClampedArray([10, 20, 30, 128]);
    expect([...toRgb({ width: 1, height: 1, kind: 3, data })!]).toEqual([132, 137, 142]);
  });

  test('rounds the blend rather than letting the store truncate it', () => {
    // Deliberate values: at half alpha every channel here lands just past .5,
    // where rounding and truncation disagree. The obvious midpoint colours do
    // NOT — 10/20/30 all truncate to what they round to, so a test written on
    // them leaves Math.round unpinned. png.ts's downscaleRgb rounds for the
    // same reason and says so: a Uint8Array store truncates, and half a level
    // shaved off every pixel of every image is a uniform darkening.
    const data = new Uint8ClampedArray([1, 3, 5, 128]);
    expect([...toRgb({ width: 1, height: 1, kind: 3, data })!]).toEqual([128, 129, 130]);
  });

  test('expands GRAYSCALE_1BPP, which is packed bits and row-padded', () => {
    // One row of 9 pixels: 101010101, padded to two bytes. A naive reader
    // that ignores the row padding smears every row after the first.
    const data = new Uint8ClampedArray([0b10101010, 0b10000000]);
    const rgb = toRgb({ width: 9, height: 1, kind: 1, data })!;
    expect(rgb.length).toBe(9 * 3);
    expect([rgb[0], rgb[1], rgb[2]]).toEqual([255, 255, 255]);
    expect([rgb[3], rgb[4], rgb[5]]).toEqual([0, 0, 0]);
    expect([rgb[24], rgb[25], rgb[26]]).toEqual([255, 255, 255]);
  });

  test('restarts each 1bpp row on a byte boundary, so rows do not shear', () => {
    // The single-row test above cannot fail on a padding bug. Three rows of 3
    // pixels, one byte each: white-black-black, black-white-black,
    // black-black-white. Read as a continuous bit stream instead, rows 2 and 3
    // slide left by 5 and 10 bits and the diagonal comes out as a smear.
    const data = new Uint8Array([0b10000000, 0b01000000, 0b00100000]);
    const rgb = toRgb({ width: 3, height: 3, kind: 1, data })!;
    const white: number[] = [];
    for (let p = 0; p < 9; p++) if (rgb[p * 3] === 255) white.push(p);
    expect(white).toEqual([0, 4, 8]);
  });

  test('a 1bpp width that is a multiple of 8 has no padding to skip', () => {
    // The remainder branch is the one that advances past the pad byte; on a
    // width of 8 it must not run, or every row after the first is off by one.
    const data = new Uint8Array([0b11111111, 0b00000000]);
    const rgb = toRgb({ width: 8, height: 2, kind: 1, data })!;
    expect(rgb.length).toBe(8 * 2 * 3);
    expect(rgb[0]).toBe(255);
    expect(rgb[23]).toBe(255);
    expect(rgb[24]).toBe(0);
    expect(rgb[47]).toBe(0);
  });

  test('refuses a buffer too short for its declared size', () => {
    expect(toRgb({ width: 100, height: 100, kind: 2, data: new Uint8ClampedArray(9) })).toBeNull();
  });

  test('accepts a buffer of exactly the declared size, and refuses one byte less', () => {
    // Not hypothetical: a truncated image stream reaches us with correct
    // width/height and a short buffer, because pdf.js caps the read at what
    // actually decoded (`if (end > bufEnd) end = bufEnd`, pdf.worker.mjs
    // DecodeStream.getBytes). One byte either side of the boundary is where an
    // off-by-one in the guard lives.
    expect(toRgb({ width: 2, height: 2, kind: 2, data: new Uint8Array(12) })!.length).toBe(12);
    expect(toRgb({ width: 2, height: 2, kind: 2, data: new Uint8Array(11) })).toBeNull();
    expect(toRgb({ width: 2, height: 2, kind: 3, data: new Uint8Array(16) })!.length).toBe(12);
    expect(toRgb({ width: 2, height: 2, kind: 3, data: new Uint8Array(15) })).toBeNull();
    expect(toRgb({ width: 9, height: 2, kind: 1, data: new Uint8Array(4) })!.length).toBe(54);
    expect(toRgb({ width: 9, height: 2, kind: 1, data: new Uint8Array(3) })).toBeNull();
  });

  test('refuses an unknown kind rather than guessing', () => {
    expect(toRgb({ width: 1, height: 1, kind: 99 as never, data: new Uint8ClampedArray(3) })).toBeNull();
  });

  test('refuses a zero dimension', () => {
    expect(toRgb({ width: 0, height: 4, kind: 2, data: new Uint8Array(12) })).toBeNull();
    expect(toRgb({ width: 4, height: 0, kind: 2, data: new Uint8Array(12) })).toBeNull();
  });

  test('refuses a fractional dimension, which nothing downstream would catch', () => {
    // NOT a throw, which is the whole reason the guard has to be here. ES2021
    // dropped SameValueZero from ToIndex, so `new Uint8Array(2.5 * 3)` is seven
    // bytes rather than a RangeError, `subarray` truncates its index too, and
    // `data.length < 7.5` is false for every buffer worth having. Loosened to
    // Number.isFinite this returns 15 well-formed bytes labelled 2.5x2, and
    // png.ts's assertRgb throws on them a stage later, having documented that a
    // mismatch there can only be our own bug. pdf.js supplies the input:
    // outside the branch that prefers the decoded image's own dimensions it
    // checks the dict as `typeof width === "number" && width > 0`
    // (pdf.worker.mjs:33205).
    expect(toRgb({ width: 2.5, height: 2, kind: 2, data: new Uint8Array(64) })).toBeNull();
    expect(toRgb({ width: 2, height: Number.NaN, kind: 2, data: new Uint8Array(64) })).toBeNull();
  });

  test('refuses the bitmap shape, where pdf.js sends no pixels at all', () => {
    // `{ data: null, bitmap }` is what createImageData returns when it has an
    // ImageBitmap: through OffscreenCanvas (pdf.worker.mjs:33809), or through
    // the ungated `#getImage` (:33686) that only a missing ImageDecoder stops.
    // Both shut under Bun today. No cast needed to reach this, and that is the
    // point: the field is typed for what pdf.js actually returns, so the guard
    // is visibly load-bearing rather than visibly dead.
    expect(toRgb({ width: 4, height: 4, kind: 2, data: null })).toBeNull();
  });
});

describe('resolveImageData', () => {
  /** One grey pixel, tagged `v` so a test can say WHICH store answered. Named
   * for what it is: `png` would be a lie in a feature whose sibling module is
   * png.ts and whose later tests assert on real PNG magic. */
  const pixels = (v: number) =>
    ({ width: 1, height: 1, kind: 2 as const, data: new Uint8ClampedArray([v, v, v]) });
  const store = (answers: Record<string, PdfImage>) => ({
    has: () => false, // exactly what pdf.js reports before the stream lands
    get: (k: string, cb: (o: PdfImage) => void) => {
      if (k in answers) setTimeout(() => cb(answers[k]), 5);
    },
  });

  test('uses the async form, because objs.has() lies about pending images', () =>
    resolveImageData(store({ img_p0_1: pixels(7) }), null, 'img_p0_1', 500).then((img) => {
      expect(img?.data?.[0]).toBe(7);
    }));

  test('falls back to commonObjs, where pdf.js parks a REPEATED image', () =>
    // Found while building the fixture: a logo drawn on every sheet is
    // `img_p0_2` on page 1 and `g_d0_img_p1_2` from page 2 on, and the
    // global cache is the only store that has it. Without this fallback the
    // page-furniture rule never runs, because the furniture never resolves.
    resolveImageData(store({}), store({ g_d0_img_p1_2: pixels(9) }), 'g_d0_img_p1_2', 500)
      .then((img) => expect(img?.data?.[0]).toBe(9)));

  test('prefers the page store when both have the key', () =>
    resolveImageData(store({ k: pixels(1) }), store({ k: pixels(2) }), 'k', 500)
      .then((img) => expect(img?.data?.[0]).toBe(1)));

  test('a second answer never overwrites the first', () => {
    // The tie-break above only decides a same-tick race. The real contract is
    // that the FIRST answer wins, so a store settling later cannot swap the
    // pixels out from under a caller that has already been handed them.
    const slow = {
      has: () => false,
      get: (_k: string, cb: (o: PdfImage) => void) => setTimeout(() => cb(pixels(2)), 40),
    };
    const fast = {
      has: () => false,
      get: (_k: string, cb: (o: PdfImage) => void) => setTimeout(() => cb(pixels(1)), 1),
    };
    return resolveImageData(slow, fast, 'k', 500).then((img) => expect(img?.data?.[0]).toBe(1));
  });

  test('settles on a callback that fires synchronously', () => {
    // pdf.js always answers on a microtask (`obj.promise.then(...)`), so every
    // other test here is a setTimeout and none of them exercises a store that
    // calls back before the timer even exists.
    const now = {
      has: () => false,
      get: (_k: string, cb: (o: PdfImage) => void) => cb(pixels(3)),
    };
    return resolveImageData(now, null, 'k', 500).then((img) => expect(img?.data?.[0]).toBe(3));
  });

  test('tolerates a store that calls back twice', () => {
    const twice = {
      has: () => false,
      get: (_k: string, cb: (o: PdfImage) => void) => {
        cb(pixels(5));
        cb(pixels(6));
      },
    };
    return resolveImageData(twice, null, 'k', 500).then((img) => expect(img?.data?.[0]).toBe(5));
  });

  test('gives up after the timeout instead of hanging the conversion', () =>
    resolveImageData(store({}), store({}), 'never', 20).then((img) => expect(img).toBeNull()));

  test('an answer that arrives after the timeout is ignored, not thrown', async () => {
    // pdf.js's callback form never cancels: a key we gave up on can still be
    // resolved later and call back into a promise that has already settled.
    //
    // The wait is on the late callback ITSELF, not on a sleep long enough to
    // probably cover it. A sleep that under-waits does not fail — it passes
    // having exercised nothing, and hands the stray callback to whichever test
    // runs next.
    let delivered!: () => void;
    const landed = new Promise<void>((r) => { delivered = r; });
    const late = {
      has: () => false,
      get: (_k: string, cb: (o: PdfImage) => void) =>
        setTimeout(() => { cb(pixels(4)); delivered(); }, 40),
    };
    expect(await resolveImageData(late, null, 'k', 10)).toBeNull();
    await landed;
  });

  test('survives a store that throws', () => {
    const bad = { has: () => false, get: () => { throw new Error('boom'); } };
    return resolveImageData(bad, null, 'bad', 20).then((img) => expect(img).toBeNull());
  });

  test('still finds the image when only the fallback store throws', () =>
    resolveImageData(store({ k: pixels(4) }), { has: () => false, get: () => { throw new Error('x'); } },
      'k', 500).then((img) => expect(img?.data?.[0]).toBe(4)));
});

describe('collectPageImages', () => {
  /** A store that answers `answers` on the next tick and counts what it was
   * asked, so a test can prove the collector did NOT ask twice. */
  const store = (answers: Record<string, PdfImage>) => {
    const asked: string[] = [];
    return {
      asked,
      has: () => false,
      get: (k: string, cb: (o: PdfImage) => void) => {
        asked.push(k);
        if (k in answers) setTimeout(() => cb(answers[k]), 1);
      },
    };
  };
  /** Distinct pixels per seed, so two pictures do not hash alike by accident. */
  const pixels = (seed: number, w = 4, h = 4): PdfImage => {
    const data = new Uint8Array(w * h * 3);
    for (let i = 0; i < data.length; i++) data[i] = (i * 7 + seed) % 256;
    return { width: w, height: h, kind: 2, data };
  };
  /** The half of pdf.js's PDFObjects the collector touches. Spelled out here
   * because images.ts keeps its own copy unexported on purpose. */
  interface Store {
    has(k: string): boolean;
    get(k: string, cb: (o: PdfImage) => void): void;
  }
  const page = (s: Store) => ({ objs: s, commonObjs: null });
  const at = (name: string, x = 0, y = 0) =>
    ops(q, cm(40, 0, 0, 40, x, y), Do(name, 4, 4), Q);

  test('encodes a PNG and carries the placement geometry onto the candidate', async () => {
    const s = store({ a: pixels(1) });
    const run = newImageRun();
    const [c] = await collectPageImages(page(s), at('a', 72, 500), { pageNum: 3, textLines: 41 }, run);
    expect([...c.bytes.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    expect([c.x0, c.y0, c.x1, c.y1]).toEqual([72, 500, 112, 540]);
    expect([c.srcPxWidth, c.srcPxHeight]).toEqual([4, 4]);
    // The ENCODED size, read out of the PNG that ships rather than off a
    // field beside it. Equal to the source here because 4px is far under
    // IMAGE_MAX_EDGE, so the downscaler passed it through.
    expect(pngSize(c.bytes)).toEqual([4, 4]);
    // The two numbers a caller can transpose without the compiler noticing.
    // pageTextLines is the background rule's whole discriminator, and pageNum
    // is what the furniture rule counts distinct values of.
    expect(c.pageNum).toBe(3);
    expect(c.pageTextLines).toBe(41);
    expect(run.failed).toBe(0);
  });

  test('gives identical pixels an identical hash, whatever they are named', async () => {
    // The furniture rule's premise: a logo is frequently a different XObject
    // per sheet, and identical PIXELS are the fact worth acting on.
    const s = store({ img_p0_2: pixels(9), g_d0_img_p1_2: pixels(9), other: pixels(11) });
    const list = ops(
      q, cm(40, 0, 0, 40, 0, 0), Do('img_p0_2', 4, 4), Q,
      q, cm(40, 0, 0, 40, 0, 60), Do('g_d0_img_p1_2', 4, 4), Q,
      q, cm(40, 0, 0, 40, 0, 120), Do('other', 4, 4), Q,
    );
    const [a, b, c] = await collectPageImages(page(s), list, { pageNum: 1, textLines: 0 }, newImageRun());
    expect(a.hash).toBe(b.hash);
    expect(a.hash).not.toBe(c.hash);
  });

  test('encodes a repeated picture ONCE and shares the bytes', async () => {
    // Page furniture is by definition on every sheet, and encoding is the
    // expensive half: a 600x300 letterhead costs ~10ms a sheet to deflate,
    // which is a second of pure waste on a hundred-page script (measured
    // 2026-08-07, bun 1.3.14). A cache hit skips the resolve as well, so the
    // store is asked exactly once across both pages.
    const s = store({ logo: pixels(3) });
    const run = newImageRun();
    const one = await collectPageImages(page(s), at('logo'), { pageNum: 1, textLines: 5 }, run);
    const two = await collectPageImages(page(s), at('logo'), { pageNum: 2, textLines: 5 }, run);
    expect(s.asked).toEqual(['logo']);
    expect(two[0].bytes).toBe(one[0].bytes);
    expect(two[0].pageNum).toBe(2);
  });

  test('counts an image with no pixels behind it, and carries on', async () => {
    // Answered, but with nothing — the `obj ?? null` case in resolveImageData.
    // Instant rather than a timeout on purpose: what this test is about is
    // that one lost picture does not end the page, and the five-second wait
    // has its own test below.
    const s = store({ good: pixels(2) });
    const hollow = {
      has: () => false,
      get: (k: string, cb: (o: PdfImage) => void) =>
        k === 'nothing' ? cb(undefined as unknown as PdfImage) : s.get(k, cb),
    };
    const run = newImageRun();
    const list = ops(
      q, cm(40, 0, 0, 40, 0, 0), Do('nothing', 4, 4), Q,
      q, cm(40, 0, 0, 40, 0, 60), Do('good', 4, 4), Q,
    );
    const got = await collectPageImages(page(hollow), list, { pageNum: 1, textLines: 0 }, run);
    expect(got).toHaveLength(1);
    expect(run.failed).toBe(1);
  });

  test('counts an image whose pixels cannot be converted', async () => {
    // The bitmap shape: pdf.js answers with an ImageBitmap and no bytes. Shut
    // under Bun today, one runtime bump from open, and if it opens EVERY image
    // takes this path at once — which is the loss this count makes visible.
    const s = store({ bitmap: { width: 4, height: 4, kind: 2, data: null } });
    const run = newImageRun();
    expect(await collectPageImages(page(s), at('bitmap'), { pageNum: 1, textLines: 0 }, run))
      .toEqual([]);
    expect(run.failed).toBe(1);
  });

  test('survives a picture whose pixels disagree with its dimensions', async () => {
    // png.ts throws on a mismatch, deliberately: by then a mismatch can only
    // be our own bug, and a silently black-bottomed picture is worse than a
    // loud stop. The blast radius has to be one image rather than the book.
    //
    // NOTHING pdf.js can hand over reaches that throw today — toRgb returns
    // exactly width x height x 3 bytes or null, so the encoder's contract
    // cannot be violated from outside. Which is why the input here is a
    // buffer that LIES about its own length: it stands in for the upstream
    // bug assertRgb was written against, because the guard has to hold when
    // that bug arrives, not when a test can arrange it politely.
    const liar = {
      length: 60,                              // 4 x 5 x 3, so toRgb accepts
      subarray: () => new Uint8Array(12),      // and then hands back 4 x 1
    } as unknown as Uint8Array;
    const s = store({ ok: pixels(6) });
    const lying = {
      has: () => false,
      get: (k: string, cb: (o: PdfImage) => void) =>
        k === 'bad' ? cb({ width: 4, height: 5, kind: 2, data: liar }) : s.get(k, cb),
    };
    const run = newImageRun();
    const list = ops(
      q, cm(40, 0, 0, 40, 0, 0), Do('bad', 4, 5), Q,
      q, cm(40, 0, 0, 40, 0, 60), Do('ok', 4, 4), Q,
    );
    const got = await collectPageImages(page(lying), list, { pageNum: 1, textLines: 0 }, run);
    expect(got.map((c) => c.srcPxHeight)).toEqual([4]);
    expect(run.failed).toBe(1);
  });

  test('charges the document budget, and caps the wait at what is left of it', async () => {
    const s = store({});
    const run = newImageRun();
    run.remainingMs = 30;
    const startedAt = Date.now();
    expect(await collectPageImages(page(s), at('never'), { pageNum: 1, textLines: 0 }, run))
      .toEqual([]);
    // Both halves, because either alone passes on the wrong code. Handing
    // resolveImageData the per-image 5s instead of what the budget has left
    // still ends with the budget overdrawn — it just takes five seconds to get
    // there, which on a lookbook of sixty is the stall the budget exists to
    // stop. The bound is two orders of magnitude clear of both numbers.
    expect(Date.now() - startedAt).toBeLessThan(1000);
    expect(run.remainingMs).toBeLessThanOrEqual(0);
    expect(run.failed).toBe(1);
  });

  test('charges the ENCODE too, not only the wait', async () => {
    // The scanned-script case in one assertion. Every sheet of a 300dpi scan
    // is a distinct full-page image, so there is nothing to wait for and
    // nothing the cache can answer: the cost is all deflate, and a budget that
    // charges only the wait would let 120 sheets spend 29 seconds of CPU
    // producing 141 MB of PNG for `ScannedPdfError` to throw away.
    //
    // 1200x900 is the smallest picture whose encode is unambiguously above the
    // sub-millisecond wait this stub imposes — it costs tens of ms, so the
    // floor below is an order of magnitude clear of the wait it must not be
    // confused with.
    const s = store({ big: pixels(5, 1200, 900) });
    const run = newImageRun();
    await collectPageImages(page(s), at('big'), { pageNum: 1, textLines: 0 }, run);
    expect(IMAGE_BUDGET_MS - run.remainingMs).toBeGreaterThan(10);
  });

  test('stops ASKING once the budget is gone, and counts what it skipped', async () => {
    // Out of budget means stop asking, not ask with no timeout: a document
    // that has already spent its allowance waiting is not going to be handed
    // the rest either. The skipped ones are still lost pictures, so they are
    // still counted.
    const s = store({ a: pixels(1), b: pixels(2) });
    const run = newImageRun();
    run.remainingMs = 0;
    const list = ops(
      q, cm(40, 0, 0, 40, 0, 0), Do('a', 4, 4), Q,
      q, cm(40, 0, 0, 40, 0, 60), Do('b', 4, 4), Q,
    );
    expect(await collectPageImages(page(s), list, { pageNum: 1, textLines: 0 }, run)).toEqual([]);
    expect(s.asked).toEqual([]);
    expect(run.failed).toBe(2);
  });

  test('but still places a CACHED picture after the budget is gone', async () => {
    // The defect the cache introduced, and the one thing that distinguishes
    // skipping the placement from abandoning the page: bytes already in hand
    // cost nothing to place, so an exhausted budget must not throw away a logo
    // this document has already paid for. With a `break` here the logo goes
    // too, and is counted as a second failure — a picture reported lost while
    // its PNG sits in the cache.
    const s = store({ logo: pixels(3) });
    const run = newImageRun();
    await collectPageImages(page(s), at('logo'), { pageNum: 1, textLines: 5 }, run);

    run.remainingMs = 0;
    const list = ops(
      q, cm(40, 0, 0, 40, 0, 0), Do('fresh', 4, 4), Q,
      q, cm(40, 0, 0, 40, 0, 60), Do('logo', 4, 4), Q,
    );
    const got = await collectPageImages(page(s), list, { pageNum: 2, textLines: 5 }, run);
    expect(got).toHaveLength(1);
    expect(got[0].pageNum).toBe(2);
    // Only the uncached one is a failure, and only it was skipped.
    expect(run.failed).toBe(1);
    expect(s.asked).toEqual(['logo']);
  });

  test('a page whose operator list never arrived has no images and no failures', async () => {
    const run = newImageRun();
    expect(await collectPageImages(page(store({})), null, { pageNum: 1, textLines: 0 }, run))
      .toEqual([]);
    expect(run.failed).toBe(0);
  });
});

describe('toRawImages', () => {
  const candidate = (over: Partial<EncodedCandidate> = {}): EncodedCandidate => ({
    pageNum: 1, x0: 61.2, y0: 396, x1: 306, y1: 792,
    srcPxWidth: 200, srcPxHeight: 200, hash: 'h', pageTextLines: 0,
    bytes: new Uint8Array([1]), ...over,
  });

  test('normalizes the box against the page, in the axes RawLine uses', () => {
    const [img] = toRawImages([candidate()], 612, 792);
    expect(img.y).toBe(792);                 // TOP edge, like RawLine.y
    expect(img.pageWidthPct).toBeCloseTo(40, 6);
    expect(img.pageHeightPct).toBeCloseTo(50, 6);
    // and nothing else: no left edge and no encoded size, both of which rode
    // here unread until 2026-08-12.
    expect(Object.keys(img).sort())
      .toEqual(['bytes', 'file', 'pageHeightPct', 'pageNum', 'pageWidthPct', 'y']);
  });

  test('names by page and by order down that page', () => {
    const imgs = toRawImages(
      [candidate({ y1: 700 }), candidate({ y1: 300 }), candidate({ pageNum: 12, y1: 500 })],
      612, 792,
    );
    expect(imgs.map((i) => i.file)).toEqual(['p001-01.png', 'p001-02.png', 'p012-01.png']);
  });

  test('sorts by page ascending and y descending, whatever order it was painted', () => {
    // Task 7 merges this array against the element stream on exactly that
    // ordering, so it has to be a fact rather than a hope: a generator is free
    // to paint the bottom panel of a strip first, and page order out of
    // extract.ts is only as good as the paint order.
    const imgs = toRawImages(
      [candidate({ pageNum: 2, y1: 100 }), candidate({ pageNum: 1, y1: 200 }),
       candidate({ pageNum: 1, y1: 700 })],
      612, 792,
    );
    expect(imgs.map((i) => [i.pageNum, i.y])).toEqual([[1, 700], [1, 200], [2, 100]]);
    expect(imgs.map((i) => i.file)).toEqual(['p001-01.png', 'p001-02.png', 'p002-01.png']);
  });
});

// The predicate the CLI deletes by. A drift between the namer and this is
// silent — the sidecar prune simply stops recognizing its own files and leaves
// a stale picture in the book — so the pairing is pinned on real output,
// including the sizes `padStart` does not truncate.
describe('isGeneratedImageFile', () => {
  const candidate = (over: Partial<EncodedCandidate> = {}): EncodedCandidate => ({
    pageNum: 1, x0: 0, y0: 0, x1: 100, y1: 100,
    srcPxWidth: 200, srcPxHeight: 200, hash: 'h', pageTextLines: 0,
    bytes: new Uint8Array([1]), ...over,
  });

  test('every name toRawImages produces is one it recognizes', () => {
    const imgs = toRawImages(
      [candidate(), candidate({ y1: 90 }), candidate({ pageNum: 12 }),
       candidate({ pageNum: 1200 })],
      612, 792,
    );
    expect(imgs.map((i) => i.file)).toEqual([
      'p001-01.png', 'p001-02.png', 'p012-01.png', 'p1200-01.png',
    ]);
    for (const img of imgs) expect(isGeneratedImageFile(img.file)).toBe(true);
  });

  test('a name a person could have chosen is NOT one of ours', () => {
    // Every one of these passes notes.ts's allowlist, which is why the two
    // predicates are separate: this is the set that may be DELETED.
    for (const name of [
      'board-01.png', 'sunset.png', 'p1.png', 'A-01.png', 'P001-01.png',
      'p001-01.PNG', 'xp001-01.png', 'p001-01.png.bak', 'p00-01.png', 'p001-1.png',
    ]) {
      expect(isGeneratedImageFile(name)).toBe(false);
    }
  });

  test('no separator can hide in one, however it is spelled', () => {
    for (const name of ['../p001-01.png', 'a/p001-01.png', 'p001-01.png/x', 'p001\\-01.png']) {
      expect(isGeneratedImageFile(name)).toBe(false);
    }
  });
});

describe('finishImages', () => {
  // The document-level decisions, tested on documents no fixture has: a mixed
  // page geometry, a broken image, sixty-one keepers. extractDocument holds a
  // real PDF, so inline none of these could fail.
  const letter = { width: 612, height: 792 };
  const landscape = { width: 792, height: 612 };
  const candidate = (over: Partial<EncodedCandidate> = {}): EncodedCandidate => ({
    pageNum: 1, x0: 61.2, y0: 396, x1: 306, y1: 792,
    srcPxWidth: 200, srcPxHeight: 200, hash: 'h1', pageTextLines: 0,
    bytes: new Uint8Array([1]), ...over,
  });

  test('measures against the MAJORITY geometry, not the last page read', () => {
    // The wiring, which a uniform fixture can never pin: one landscape insert
    // at the end of a portrait script. Against the last page this image is
    // 30.9% wide and 64.7% tall; against the document it is 40% and 50%.
    const { images } = finishImages([candidate()], [letter, letter, landscape], 3, newImageRun());
    expect(images[0].pageWidthPct).toBeCloseTo(40, 6);
    expect(images[0].pageHeightPct).toBeCloseTo(50, 6);
  });

  test('carries the failure count off the run, untouched', () => {
    const run = newImageRun();
    run.failed = 7;
    expect(finishImages([], [letter], 1, run).imageStats.failed).toBe(7);
  });

  test('reports what the cap refused, which is not what the filters refused', () => {
    // Distinct hashes and distinct pages, so nothing is furniture and the only
    // thing that can drop one is MAX_IMAGES. 61 in, 60 out, 1 over.
    const many = Array.from({ length: 61 }, (_, i) =>
      candidate({ pageNum: i + 1, hash: `h${i}` }));
    const { images, imageStats } = finishImages(many, [letter], 61, newImageRun());
    expect(images).toHaveLength(60);
    expect(imageStats.overCap).toBe(1);
    expect(Object.values(imageStats.droppedBy).every((n) => n === 0)).toBe(true);
  });

  test('a document with no pages at all is refused rather than divided by', () => {
    expect(finishImages([candidate()], [], 0, newImageRun()).images).toEqual([]);
  });
});

describe('dominantPageSize', () => {
  // The image filters measure every picture against ONE page geometry, so
  // extract.ts has to pick which page it comes from. Taking the last page's —
  // which is what a loop variable does if nobody chooses — measures an entire
  // document against whatever sheet happened to be last.
  const letter = { width: 612, height: 792 };
  const landscape = { width: 792, height: 612 };

  test('a uniform document reports its one size', () => {
    expect(dominantPageSize([letter, letter, letter])).toEqual(letter);
  });

  test('a landscape insert does not redefine the document, even when last', () => {
    expect(dominantPageSize([letter, letter, letter, landscape])).toEqual(letter);
  });

  test('ties go to the first seen, so the answer is deterministic', () => {
    expect(dominantPageSize([letter, landscape])).toEqual(letter);
    expect(dominantPageSize([landscape, letter])).toEqual(landscape);
  });

  test('no pages at all is zeros, which keepArtwork refuses to judge on', () => {
    expect(dominantPageSize([])).toEqual({ width: 0, height: 0 });
  });
});
