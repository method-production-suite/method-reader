// Images carried out of a PDF: where they sit and what pixels they hold.
//
// This is the pdf.js-FACING half of image extraction, and the ORCHESTRATION:
// placements out of an operator list, pixels out of the object stores, PNGs out
// of png.ts, and finally (`finishImages`) the whole document's pile handed to
// artwork.ts and returned as `RawImage[]`. Whether a given picture is art or
// furniture is a judgement, not a fact about the file, and that judgement lives
// in artwork.ts with no pdf.js in it — the same line png.ts draws around itself
// in its own header. The two halves change for different reasons: this file
// changes when pdf.js does, artwork.ts changes when a real script proves a
// threshold wrong.
//
// extract.ts holds a single edge to this file and none to artwork.ts. It reads
// pages; it should not also be where the editorial policy is invoked.
//
// The placement walk mirrors collectUnderlineMarks in extract.ts — same
// save/restore/transform bookkeeping over the same operator list, which
// extract.ts already fetches for underline detection. The 6-element `concat` is
// deliberately a second copy rather than an import: it is a matrix multiply
// with no judgement in it to drift, which is what CLAUDE.md's one-shared-copy
// rule (registry 5b, 18) is actually about. Each copy is pinned by the suite of
// the file it lives in — extract.test.ts on the underline geometry,
// images.test.ts on the placement geometry — so breaking either is a failing
// test in that file. (An earlier version of this note claimed both suites would
// fail at once. They would not: break the copy here and extract.test.ts stays
// green. What makes the duplication safe is that arithmetic has nothing to
// drift, not a cross-check that does not exist.)
//
// The shims come first, like everywhere pdf.mjs is loaded: they hand
// getDocument the statically imported worker, supply DOMMatrix for rendering,
// and let quiet-pdfjs.ts patch console.warn before pdf.mjs is evaluated. (Not
// because a bare import dies: on pdfjs-dist 6.3.289 it loads under bun 1.3.14,
// checked 2026-10-01; see pdfjs-shims.ts.)
import './pdfjs-shims';
import { OPS } from 'pdfjs-dist/build/pdf.mjs';
import { downscaleRgb, encodePng } from './png';
// The judgements themselves stay in artwork.ts, which knows nothing about
// pdf.js and must not learn: the dependency points this way only. See
// finishImages for why the CALL to keepArtwork belongs on this side of the
// split rather than in extract.ts.
import { keepArtwork, type DropRule, type ImageCandidate } from './artwork';
import type { RawImage } from './types';

/** The shape `page.getOperatorList()` returns; the same one extract.ts walks. */
export interface OpList {
  fnArray: ArrayLike<number>;
  argsArray: ArrayLike<unknown>;
}

/** One `Do` of an image XObject, in PDF user space (y up). */
export interface ImagePlacement {
  objId: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  /** SOURCE pixels, as the paint op declares them. Named `src` because the
   * same subsystem also carries OUTPUT pixels — what the PNG ends up being
   * after the downscale — and one object holding both under near-identical
   * names is a mix-up nothing would catch. */
  srcPxWidth: number;
  srcPxHeight: number;
}

/** Handed out on an unbalanced `Q`, and pushed on the stack like any other
 * matrix, so nothing here may EVER be written through: `readonly` is what keeps
 * a future in-place edit from rewriting the saved state and this constant with
 * it. Every matrix in this file is replaced, never mutated. */
const IDENTITY: readonly number[] = [1, 0, 0, 1, 0, 0];

/** m ∘ t — PDF's `cm` post-multiplies onto the current matrix. Byte-for-byte
 * the multiply in extract.ts, operands in the same order: `t` applies FIRST,
 * inside `m`, so a translate under a 2x scale moves twice as far. */
function concat(m: ArrayLike<number>, t: ArrayLike<number>): number[] {
  return [
    m[0] * t[0] + m[2] * t[1],
    m[1] * t[0] + m[3] * t[1],
    m[0] * t[2] + m[2] * t[3],
    m[1] * t[2] + m[3] * t[3],
    m[0] * t[4] + m[2] * t[5] + m[4],
    m[1] * t[4] + m[3] * t[5] + m[5],
  ];
}

/**
 * Every image paint on a page, with the box it actually covers.
 *
 * Costs almost nothing per page: extract.ts already fetches this operator list
 * for underline detection, so the walk itself is the only new work. (The
 * pixels are not free — resolving and encoding them is the expensive part, and
 * that happens in collectPageImages, not here.)
 *
 * Two operator cases beyond `q`/`cm`/`Q` decide whether this is right on real
 * files, and each was read out of pdfjs-dist rather than assumed (6.2.108,
 * and re-checked on 6.3.289):
 *
 *   - A form XObject does NOT expand into save/transform ops. Its /Matrix rides
 *     on `paintFormXObjectBegin` and the renderer saves, applies it, and
 *     restores on End — so placed art (Illustrator, InDesign, a stamped
 *     appearance) would land at the wrong offset, and the form's own transforms
 *     would leak into the rest of the page. The `beginGroup`/`endGroup` pair a
 *     transparency group adds around it needs nothing: the renderer redirects to
 *     a scratch canvas under the same user-space mapping.
 *   - Images inside an annotation are skipped on WHAT they are, not on any
 *     difficulty placing them: a stamp, a signature or a form-field appearance
 *     is markup ON the script, never the script's art. Locating one is two
 *     concats, not a research project — `beginAnnotation` carries
 *     [id, rect, transform, matrix], and applying transform then matrix from
 *     identity lands exactly on the annotation's Rect — so whoever one day wants
 *     annotation art can have it. What they must NOT do is simply delete the
 *     skip: an appearance stream does not compose onto the CTM tracked here, so
 *     a paint that fell through would be placed by whatever the page happened to
 *     leave behind. pdf.js appends annotations AFTER the page's own operators,
 *     so the skip can never swallow page art.
 *
 * The form and annotation branches are DEFENSIVE, not load-bearing, and a
 * corpus scan is not grounds for deleting them. 2026-08-06: 549 pages across celtx,
 * chromium, fade-in, Final Draft, Highland, a pitch deck and a scanned file
 * carry no form XObjects, no annotations and no masks at all, and this walk
 * agrees with a form-blind one on every one of those pages. One PDF from a
 * generator nobody here owns is all it takes to need them, and the failure they
 * prevent is silent: art placed hundreds of points from where it was drawn.
 *
 * An image XObject always occupies the unit square, so the CTM in force at the
 * paint IS the placement. Unlike the underline walk this one does not skip
 * rotated content: a bbox stops describing a rule on a slant, but it still
 * describes the region a turned storyboard panel covers, and that panel is
 * worth keeping.
 *
 * Best-effort like the underline walk: any failure yields no images, which is
 * exactly today's behavior.
 */
export function collectImagePlacements(opList: OpList): ImagePlacement[] {
  const out: ImagePlacement[] = [];
  try {
    const { fnArray, argsArray } = opList;
    let m: readonly number[] = IDENTITY;
    const stack: (readonly number[])[] = [];
    let inAnnotation = false;

    for (let i = 0; i < fnArray.length; i++) {
      const fn = fnArray[i];

      if (fn === OPS.save) {
        stack.push(m);
        continue;
      }
      if (fn === OPS.restore) {
        m = stack.pop() ?? IDENTITY;
        continue;
      }
      if (fn === OPS.transform) {
        m = concat(m, argsArray[i] as ArrayLike<number>);
        continue;
      }
      if (fn === OPS.paintFormXObjectBegin) {
        stack.push(m);
        // A form with no /Matrix still saves and restores; only the concat is
        // conditional.
        const matrix = (argsArray[i] as [ArrayLike<number> | null, unknown] | null)?.[0];
        if (matrix) m = concat(m, matrix);
        continue;
      }
      if (fn === OPS.paintFormXObjectEnd) {
        m = stack.pop() ?? IDENTITY;
        continue;
      }
      if (fn === OPS.beginAnnotation) {
        inAnnotation = true;
        continue;
      }
      if (fn === OPS.endAnnotation) {
        inAnnotation = false;
        continue;
      }
      // Everything else falls through, and two absences are deliberate.
      // `paintImageXObjectRepeat` collapses a run of identical paints and
      // carries no pixel dimensions, but it cannot reach us: getOperatorList
      // sets the OPLIST intent, which gives the worker a NullOptimizer, so
      // only a rendering path would ever see one. And image MASKS are ignored
      // on what they are: a 1-bit stencil in a screenplay is a bullet or a
      // rule, never the art anyone wants kept.
      if (fn !== OPS.paintImageXObject || inAnnotation) continue;

      const args = argsArray[i] as [string, number, number] | null | undefined;
      if (!args || typeof args[0] !== 'string') continue;
      // pdf.js refuses to emit a paint whose Width/Height are not numbers, so
      // this only fires on a hand-built list or a future pdf.js. Worth the two
      // lines anyway: the pixel stage sizes buffers from these, where a NaN
      // becomes a corrupt image far from the operator that caused it.
      const srcPxWidth = args[1];
      const srcPxHeight = args[2];
      if (!Number.isFinite(srcPxWidth) || !Number.isFinite(srcPxHeight)) continue;
      if (srcPxWidth <= 0 || srcPxHeight <= 0) continue;

      // BOTH corners was the underline walk's hard-won lesson; for a box it is
      // all FOUR, then min/max. Final Draft draws under a y-flip, and mapping
      // only the origin puts the image off the page — the same defect, one
      // dimension richer.
      const xs: number[] = [];
      const ys: number[] = [];
      for (const [ux, uy] of [[0, 0], [1, 0], [0, 1], [1, 1]]) {
        xs.push(m[0] * ux + m[2] * uy + m[4]);
        ys.push(m[1] * ux + m[3] * uy + m[5]);
      }
      out.push({
        objId: args[0],
        x0: Math.min(...xs),
        y0: Math.min(...ys),
        x1: Math.max(...xs),
        y1: Math.max(...ys),
        srcPxWidth,
        srcPxHeight,
      });
    }
  } catch {
    return [];
  }
  return out;
}

/** What pdf.js puts in its object store for an image XObject. */
export interface PdfImage {
  width: number;
  height: number;
  /** pdf.js ImageKind: 1 GRAYSCALE_1BPP, 2 RGB_24BPP, 3 RGBA_32BPP. There is
   * no fourth — pdf.js's own `putBinaryImageData` throws `bad image kind` on
   * anything else (pdf.mjs `ImageKind`, re-checked on 6.3.289). */
  kind: 1 | 2 | 3;
  /** All THREE, and the choice is pdf.js's. The direct DeviceRGB and DeviceGray
   * paths in `PDFImage.createImageData` hand back the decode stream's own
   * bytes as a Uint8Array — which is all six pictures in
   * tests/fixtures/images.pdf — while the general colour-space fallback in the
   * same function allocates a Uint8ClampedArray. `null` is the
   * bitmap path, where the pixels are an ImageBitmap and this field is emptied
   * rather than omitted. A probe sees whichever its sample happens to use, and
   * narrowing to that one is a lie TypeScript would then enforce on every
   * caller — and it would make toRgb's guard against the null look like dead
   * code, which is how a guard gets deleted. */
  data: Uint8Array | Uint8ClampedArray | null;
}

/**
 * The half of pdf.js's `PDFObjects` this file touches.
 *
 * `has` is declared and never called, deliberately. It is here because the
 * real store has it and a caller's object literal would otherwise be rejected
 * — and because the one thing worth knowing about this type is that its
 * membership test must not be trusted. See resolveImageData.
 */
interface ObjStore {
  has(key: string): boolean;
  get(key: string, callback: (obj: PdfImage | null) => void): void;
}

/** How long to wait for one image's pixels before giving up on it. */
export const IMAGE_RESOLVE_TIMEOUT_MS = 5_000;

/**
 * Pull one image's pixels out of pdf.js's object stores.
 *
 * TWO stores, and both are load-bearing. A picture used once lives in the
 * PAGE's store; a picture pdf.js sees repeated gets promoted to the document
 * -wide `commonObjs` and renamed (`img_p0_2` on sheet one becomes
 * `g_d0_img_p1_2` from sheet two on). Asking only the page store finds
 * nothing for exactly the images the page-furniture rule exists to catch, so
 * a letterhead logo would be dropped for the wrong reason, after a five
 * second stall per page, with no error. Found by the fixture, not by reading
 * the pdf.js source.
 *
 * pdf.js's own renderer picks the store by that `g_` prefix instead
 * (`CanvasGraphics.getObject` in pdf.mjs:
 * `data.startsWith("g_") ? this.commonObjs.get(data) : ...`). We
 * race both rather than copy it: the prefix is an internal naming convention,
 * and the price of it changing under us is precisely the silent stall above,
 * where the price of racing is one extra pending map entry.
 *
 * The CALLBACK form is mandatory. `objs.has(key)` returns false for an image
 * whose data has not yet streamed from the worker, and the synchronous
 * `objs.get(key)` throws on the same condition — so the obvious
 * `has() ? get() : skip` reads as "this page has no images" on perfectly good
 * input. That mistake cost an afternoon during the design probe, and the
 * fixture still shows it: 2026-08-07, nine of the ten paints in images.pdf
 * report `has() === false` at the moment the operator list is complete, and
 * every one of them resolves. The nine is timing; the test pins the invariant.
 *
 * Asking a store for a key it does not hold is safe but not free: pdf.js
 * inserts a pending entry for whatever it is asked (`getOrInsertComputed`),
 * which a later `resolve` of the same id fills in normally. Bounded by the
 * image count and cleared with the document.
 *
 * Timed out rather than awaited forever: a damaged XObject would otherwise
 * hang a conversion with no error and no progress. Both stores race the ONE
 * timeout, because a key that is in neither should cost one wait, not two.
 * The budget is per image on purpose — pdf.js sends the pixels well after the
 * operator list, so this really is a wait for work in flight, not a retry —
 * but a caller converting a lookbook of sixty owes itself a DOCUMENT ceiling
 * too, or sixty damaged images stall for five minutes in a row. That ceiling is
 * `ImageBudget`'s job, not this function's: it holds the document-wide
 * allowance and hands what is left of it over as this `timeoutMs`.
 */
export function resolveImageData(
  objs: ObjStore,
  commonObjs: ObjStore | null,
  key: string,
  timeoutMs = IMAGE_RESOLVE_TIMEOUT_MS,
): Promise<PdfImage | null> {
  return new Promise((resolve) => {
    // First answer wins, and the losers are ordinary rather than exceptional:
    // pdf.js's callback form has no cancel, so a key we gave up on can still
    // call back later, and a late `resolve` on a settled promise is already a
    // no-op. A `settled` flag here would guard nothing and read as if it did.
    const done = (v: PdfImage | null) => {
      clearTimeout(timer);
      resolve(v);
    };
    const timer = setTimeout(() => done(null), timeoutMs);
    // Both asked at once, page store first so it wins a tie. Neither call
    // resolves for a key it does not hold, which is why this is a race and
    // not a sequence: sequencing would pay the timeout twice.
    for (const store of [objs, commonObjs]) {
      if (!store) continue;
      try {
        store.get(key, (obj) => done(obj ?? null));
      } catch {
        // Defensive, and known to be: nothing in pdf.js's CALLBACK form throws
        // for a key it does not hold — it inserts a pending entry and simply
        // never calls back, which is the whole reason for the timeout. It is
        // the SYNCHRONOUS `objs.get(key)` that throws, and that is why this
        // file never uses it. Swallowing stays right if some future store does
        // throw: the other one, or the timeout, still settles this.
      }
    }
  });
}

/**
 * Normalize any pdf.js image kind to packed 8-bit RGB, or null if we can't.
 *
 * Null rather than a throw on anything unexpected, because every input here
 * comes out of a file we did not write: a short buffer is what a TRUNCATED
 * image stream looks like from this side, pdf.js having capped the read at
 * what actually decoded rather than reporting the shortfall. png.ts's
 * assertRgb is the opposite end of the same contract — it throws, because by
 * then a mismatch could only be our own bug.
 */
export function toRgb(img: PdfImage): Uint8Array | null {
  const { width: w, height: h, kind, data } = img;
  // Integer, not merely finite, and NOT because a fractional size throws — it
  // does not. ES2021 dropped the SameValueZero check from ToIndex, so
  // `new Uint8Array(2.5 * 3)` is seven bytes and `subarray` truncates its index
  // the same way; meanwhile `data.length < px * 3` compares against a
  // FRACTIONAL threshold that no real buffer trips. A /Width of 2.5 therefore
  // sails all the way through and yields a well-formed 15-byte buffer labelled
  // 2.5x2 — and png.ts's assertRgb, which promises a mismatch there is our own
  // bug and never bad input, throws a stage later with nothing pointing back
  // here. That is the obscurity assertRgb exists to prevent. pdf.js will hand
  // one over: outside the branch that prefers the decoded image's own
  // dimensions, the `PDFImage` constructor validates the dict as
  // `typeof width === "number" && width > 0` and nothing more (re-checked on
  // 6.3.289).
  if (!Number.isInteger(w) || !Number.isInteger(h) || w < 1 || h < 1) return null;
  // `{ data: null, bitmap }` is the shape pdf.js returns when it holds an
  // ImageBitmap instead of pixels, and TWO independent doors lead there, both
  // in `PDFImage.createImageData`. Re-read on 6.3.289:
  //   - OffscreenCanvas, the expected one, gated on `isOffscreenCanvasSupported`,
  //     which `getDocument` defaults off under a Node-like runtime and
  //     `BasePdfManager` clears again against the worker's own FeatureTest.
  //   - `#getImage`, which createImageData asks unconditionally on the direct
  //     DeviceRGB/DeviceGray path, before it reads a byte. It returns a bitmap
  //     only through a JPEG stream's `getTransferableImage`, which on 6.3.289
  //     needs `JpegStream.canUseImageDecoder`: the `isImageDecoderSupported`
  //     option (off by default under a Node-like runtime) AND an `ImageDecoder`
  //     global, which Bun does not have. (The August note, on 6.2.108, found
  //     this door gated only by the missing global.)
  // Shut today through both, one runtime bump or one option from not, and a
  // property read on null the moment either opens.
  if (!data) return null;
  const px = w * h;

  // Commonest first, not in ImageKind order: RGB_24BPP is every picture in the
  // fixture and the general colour-space fallback lands on RGBA_32BPP, which
  // leaves 1bpp the rare one.
  if (kind === 2) {
    if (data.length < px * 3) return null;
    // Copied, not viewed, and the copy is the point. `data` may be a
    // Uint8ClampedArray, whose subarray is not a Uint8Array; and downscaleRgb
    // returns THE SAME ARRAY for an image that already fits (png.ts), so a
    // view handed out here would reach the encoder still pointing into
    // pdf.js's object store. Every kind leaves this function owning its bytes.
    return new Uint8Array(data.subarray(0, px * 3));
  }

  if (kind === 3) {
    if (data.length < px * 4) return null;
    // FLATTENED onto white, not dropped. png.ts emits colour type 2, which has
    // no alpha channel to carry, so a transparent pixel has to become
    // SOMETHING — and taking the stored RGB makes it whatever sat underneath,
    // frequently black. A logo cut out over a dark backing would ship as art in
    // a black rectangle, on an e-ink page. Nothing downstream would catch it:
    // artwork.ts judges size, placement and repetition, so a picture ruined
    // this way clears keepArtwork exactly as a good one does. White is the page
    // a screenplay is read on, and it is what pdf.js itself writes for a fully
    // transparent pixel when it un-premultiplies a matted mask
    // (`PDFImage.#undoPreblend`).
    //
    // The alpha is STRAIGHT, not premultiplied, so this is the ordinary blend
    // and not `c + 255 * (1 - a)`: `PDFImage.fillOpacity` writes the alpha byte
    // on its own and leaves the colour alone, and pdf.js copies these bytes
    // into a canvas ImageData unchanged (`putBinaryImageData` in pdf.mjs),
    // which is a non-premultiplied contract. All three re-checked on 6.3.289.
    //
    // The limitation, written down because a device pass is where it will
    // surface: a reader in dark mode gets a WHITE surround wherever the source
    // was transparent. Carrying it properly needs an alpha-capable encoder and
    // a renderer that honors it, which is soft-mask work neither the August
    // design nor today's spec takes on: both flatten transparency onto white.
    const out = new Uint8Array(px * 3);
    for (let i = 0; i < px; i++) {
      const a = data[i * 4 + 3];
      for (let c = 0; c < 3; c++) {
        // Rounded for png.ts's reason: a Uint8Array store truncates, and half a
        // level shaved off every pixel is a uniform darkening. a=255 comes back
        // exactly `data[i * 4 + c]`, so an opaque image is untouched.
        out[i * 3 + c] = Math.round((data[i * 4 + c] * a + 255 * (255 - a)) / 255);
      }
    }
    return out;
  }

  if (kind === 1) {
    // Packed 1-bit, and every ROW starts on a byte boundary. Ignoring that
    // padding smears each row progressively further than the last. Read out of
    // pdf.js rather than the PDF spec: the worker sizes the buffer as
    // `height * ((width * numComps * bpc + 7) >> 3)`, and its own
    // convertBlackAndWhiteToRGBA consumes `width >> 3` bytes plus one more for
    // the remainder on every row.
    const rowBytes = (w + 7) >> 3;
    if (data.length < rowBytes * h) return null;
    const out = new Uint8Array(px * 3);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        // Bits run MSB-first, so pixel 0 is bit 7 of its byte — which is the
        // `7 - (x & 7)` and not `x & 7`, the one step here a reader cannot
        // derive from the prose above. 1 is WHITE, from the same function:
        // zero maps to black and one to `nonBlackColor` (0xffffffff). A
        // /Decode that inverts has already been applied to these bytes in the
        // worker, so honoring it again here would come out as a negative.
        const bit = (data[y * rowBytes + (x >> 3)] >> (7 - (x & 7))) & 1;
        const v = bit ? 255 : 0;
        const o = (y * w + x) * 3;
        out[o] = out[o + 1] = out[o + 2] = v;
      }
    }
    return out;
  }

  return null;
}

/**
 * Long edge, in pixels, of anything we carry.
 *
 * A constant and NOT a format option, deliberately. An option here would have
 * to apply at render time to survive the app's `.fountain` cache boundary,
 * which would mean decoding a PNG as well as encoding one; applied at parse
 * time instead it would put a format option inside the parser and break that
 * invariant.
 *
 * WHAT 1400 CLEARS, and this note used to claim more: it is a size cap, not a
 * panel measurement. It refuses to carry a 4000px lookbook scan at full size,
 * which is the point, and it is comfortably over the width any figure is drawn
 * at on the 6" class a script is usually read on (a Paperwhite panel is 1236
 * across). It does NOT clear every panel Screepub ships to, and the earlier
 * claim that it did cited 1236 x 1648 while being smaller than the 1648. Two
 * places it falls short, neither of them fatal:
 *
 *   - a full-page plate is meant to fill the full width of the text column with
 *     its height following (how the renderers draw it is phase 2), so a
 *     portrait title card filling that same 1236 asks for about 1648 px of
 *     height and is handed 1400;
 *   - the 10" class is past 1400 on its SHORT edge before height enters it.
 *     docs/device-map.md §2.6 puts reMarkable at 10.3" and 226-229 PPI, which
 *     is about 1404 across; the Supernote Manta (10.7" at ~300 PPI, §2.6) and
 *     the Kindle Scribe (§2.1) are further out again.
 *
 * The trade is deliberate rather than overlooked. Each of those is a mild
 * upscale of one plate on one device, where raising the edge costs every book
 * that carries any picture at all — and it is worse to raise than it looks,
 * because the size cap that src/library-images.ts reads the picture folder
 * back under is computed from this: past about 1670 our own output stops
 * being readable back.
 */
export const IMAGE_MAX_EDGE = 1400;

/**
 * Ceiling on the time ONE DOCUMENT may spend on pictures AT ALL.
 *
 * Waiting and work both, and the "both" is load-bearing. `resolveImageData`'s
 * timeout is per image, which is the right shape for one damaged XObject and
 * the wrong shape for sixty: a lookbook whose images all fail would stall five
 * minutes with no error and no progress, which reads to the user as a hang
 * rather than as a slow conversion. But an earlier version of this constant
 * charged only the WAIT, and the case that exposes it is not a damaged file at
 * all — it is a scan. Every sheet of a 300dpi scan is a distinct full-page
 * image, so the pixels arrive instantly (nothing to wait for), the encode
 * cache never hits (no two sheets alike), and MAX_IMAGES cannot help because
 * the cap is applied to work already done. Measured 2026-08-07 (bun 1.3.14):
 * one 2550x3300 sheet of realistic paper noise is 22ms of downscale plus
 * 219ms of deflate, so 120 sheets is 29 seconds of CPU and 141 MB of encoded
 * pixels — every byte of it thrown away moments later by `ScannedPdfError`.
 * Twenty-eight seconds is twenty-eight seconds whether it went on waiting or
 * on deflating, so the budget now covers resolve, convert, downscale and
 * encode. That scan stops around sheet 82 instead of running to the end.
 *
 * The number survives charging the work because of one piece of arithmetic:
 * 60 images (MAX_IMAGES) at that same worst-case 241ms is 14.5s, inside this
 * ceiling with room to spare. Since both the cap and the budget run in
 * document order, exhausting the budget can never cost a document a picture
 * the cap would have kept — it can only stop work whose output was going to be
 * discarded anyway. A healthy script is nowhere near either number: all ten
 * paints in tests/fixtures/images.pdf spend 13-14ms of the ceiling, resolve
 * through deflate.
 *
 * What a reader gets for it: not merely "one damaged XObject cannot hang a
 * conversion", but "a document cannot spend more than twenty seconds on
 * pictures, however they fail". Placements skipped once it is gone are counted
 * in `ImageRun.failed`, so the loss is reported rather than silent.
 */
export const IMAGE_BUDGET_MS = 20_000;

/** One picture's encoded pixels: everything a candidate needs that came from
 * the image itself rather than from where it was painted.
 *
 * The encoded DIMENSIONS are not in here. They were, and they were read by
 * nothing at either end of the pipeline — the IHDR in `bytes` is where the
 * output size is actually true, and `srcPxWidth`/`srcPxHeight` on the
 * placement are what the one rule that asks about resolution (MIN_PIXELS)
 * judges. Two more numbers whose names sit one letter from those was a
 * standing invitation to read the wrong pair. */
interface EncodedImage {
  bytes: Uint8Array;
  hash: string;
}

/** A candidate with its pixels attached: what artwork.ts judges, plus what
 * survives into the book if it clears. */
export type EncodedCandidate = ImageCandidate & EncodedImage;

/**
 * What one DOCUMENT's worth of image collection carries between pages.
 *
 * All three fields are document-scoped, which is why they travel together
 * rather than as three parameters: the budget ceiling is per document (see
 * IMAGE_BUDGET_MS), the encode cache is what stops a logo on ninety sheets
 * being deflated ninety times, and the failure count is only meaningful as a
 * total. Mutable by design; `collectPageImages` is handed the same object for
 * every page.
 */
export interface ImageRun {
  /** Milliseconds left for the whole document to spend on pictures: waiting
   * for pixels AND converting, downscaling and encoding them. See
   * IMAGE_BUDGET_MS for why the work is charged and not only the wait. */
  remainingMs: number;
  /**
   * Encoded pixels by objId, so a picture placed on many sheets is resolved
   * and encoded TWICE rather than once per sheet. Twice, not once, and the
   * number is pdf.js's: it only promotes an image to the document-wide store
   * from the SECOND page it appears on (`GlobalImageCache.NUM_PAGES_THRESHOLD
   * = 2`, re-checked on 6.3.289), so a repeated picture carries a page-local id
   * on its first sheet and a `g_` id ever after. Two keys, two encodes,
   * however many sheets. Measured on tests/fixtures/images.pdf: its ten paints
   * are 7 distinct encodes rather than 10 — six pictures, of which the logo is
   * counted twice for its five sheets.
   *
   * Worth having rather than a micro-optimisation: page furniture is BY
   * DEFINITION on every sheet, encoding is the expensive half (~10ms of
   * deflate for a 600x300 letterhead), and the furniture rule throws all of it
   * away — so a hundred-page script pays twice instead of a hundred times.
   *
   * Where it helps LEAST is where the cost is highest, which is worth knowing
   * before trusting it: promotion to the global store stops once that store
   * holds ten images AND 50 MB (`GlobalImageCache.MIN_IMAGES_TO_CACHE` and
   * `MAX_BYTE_SIZE`, and its `#cacheLimitReached` needs both). A lookbook of large
   * pictures can therefore keep page-local ids throughout and miss on every
   * sheet. Correctness does not depend on any of this; only the saving does.
   *
   * A hit hands out the SAME `Uint8Array`, so two kept images of one picture
   * share one buffer — and `toRgb` already promises the bytes are ours rather
   * than a view into pdf.js's store, which this does not change. Nothing
   * downstream may MUTATE image bytes: the EPUB zip writer (phase 2) and the
   * MOBI image records (phase 3) are to copy them in, and an in-place edit
   * would rewrite the other placement too.
   *
   * Safe to key on the id: pdf.js builds a page store's ids as
   * `img_p{pageIndex}_{n}` (`PartialEvaluator.buildPaintImageXObject` over the
   * `Page`'s local id factory) and the document-wide ones as `g_{docId}_...`
   * (the same function, prefixing the `PDFDocument`'s `getDocId`), so one id
   * means one picture for the life of the document. Re-checked on 6.3.289.
   *
   * SUCCESSES only. A miss is a timing fact rather than a fact about the
   * picture — pixels arrive from the worker after the operator list that asked
   * for them, which is the whole reason resolveImageData needs a timeout — so
   * a key that has not arrived yet is not a key that never will, and caching
   * the miss would turn one slow delivery into a permanent one.
   */
  encoded: Map<string, EncodedImage>;
  /**
   * Placements that yielded no candidate: pixels that never arrived, pixels
   * we could not convert, an encode that threw, or a placement skipped
   * because the budget was gone.
   *
   * Counted rather than merely skipped, because the failures that matter are
   * total. pdf.js's `data: null` bitmap path is one runtime bump from opening
   * under Bun (see toRgb), and if it opens EVERY image converts to null at
   * once: a book that ships with no art and no error reads exactly like a
   * script that never had pictures. This is the number that tells them apart.
   *
   * Said out loud rather than merely returned: it travels on
   * `ImageStats.failed` and `imageWarnings` in convert.ts writes the sentence,
   * the same way it does for `ArtworkVerdict.overCap`.
   */
  failed: number;
}

export const newImageRun = (): ImageRun => ({
  remainingMs: IMAGE_BUDGET_MS,
  encoded: new Map(),
  failed: 0,
});

/** FNV-1a over the encoded bytes: cheap, and only ever compared, never
 * stored. 32 bits is ample for the comparison population, which is one
 * document's pictures: a collision would read two different ones as a single
 * repeated one, at about three chances in a hundred thousand even for a
 * document carrying five hundred. */
function hashBytes(b: Uint8Array): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < b.length; i++) {
    h ^= b[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16);
}

/**
 * Resolve one placement's pixels and encode them, or null if anything refused.
 *
 * The try covers every step from the pdf.js image to the PNG, and the blast
 * radius of any of them has to be ONE image rather than the whole book.
 *
 * png.ts throws by contract on a pixel/dimension mismatch, because by then a
 * mismatch could only be our own bug — unreachable through `toRgb` today,
 * which returns exactly `width * height * 3` bytes or null, and guarded anyway
 * because the day it becomes reachable is the day it is a bug. The allocation
 * is the reachable half: `toRgb` sizes buffers from numbers the FILE declares,
 * so a 1bpp XObject claiming 60000x60000 asks for 10 GB of output off 450 MB
 * of decoded input, and that is a bare `RangeError: Out of memory` (bun 1.3.14
 * allocates 4 GB and throws at 8).
 */
async function encodePlacement(
  page: { objs: ObjStore; commonObjs: ObjStore | null },
  objId: string,
  run: ImageRun,
): Promise<EncodedImage | null> {
  const startedAt = Date.now();
  try {
    const img = await resolveImageData(
      page.objs,
      page.commonObjs,
      objId,
      Math.min(IMAGE_RESOLVE_TIMEOUT_MS, run.remainingMs),
    );
    if (!img) return null;
    const rgb = toRgb(img);
    if (!rgb) return null;
    const small = downscaleRgb(rgb, img.width, img.height, IMAGE_MAX_EDGE);
    const bytes = encodePng(small.rgb, small.width, small.height);
    return { bytes, hash: hashBytes(bytes) };
  } catch {
    return null;
  } finally {
    // Charged in a `finally` so every exit pays: the wait, the conversion, the
    // downscale and the deflate, whether this placement produced a picture,
    // gave up on one, or threw. A document's ceiling is meaningless if the
    // expensive half of the work is free (IMAGE_BUDGET_MS), and an early
    // return is exactly where an uncharged path would hide.
    run.remainingMs -= Date.now() - startedAt;
  }
}

/**
 * Every image on one page, encoded and ready for the document-wide filters.
 *
 * Encoding happens here rather than after filtering because the furniture rule
 * compares ENCODED bytes: the same logo is frequently a different XObject on
 * every sheet, and identical pixels are the fact worth acting on.
 *
 * Pixels MUST be resolved while the page's operator-list intent is alive.
 * pdf.js drops an incoming image outright once `pageProxy._intentStates.size
 * === 0` (the "obj" handler in `WorkerTransport.setupMessageHandler`, pdf.mjs;
 * the code re-checked on 6.3.289, the measurement below made on 6.2.108), and
 * `page.cleanup()` is what empties that. Measured, not quoted: a `cleanup()`
 * between `getOperatorList`
 * and this call costs SIX of the ten paints in tests/fixtures/images.pdf, and
 * the four that survive are only the ones pdf.js had already parked in
 * commonObjs — so the loss is silent, partial, and would look exactly like a
 * furniture rule firing. Nothing in this engine calls cleanup today; this is
 * the note for whoever adds it as a memory optimisation.
 *
 * `info` is an object because its two fields are both plain numbers and the
 * compiler cannot catch them being swapped: `pageNum` is what the furniture
 * rule counts distinct values of, and `textLines` is the entire discriminator
 * between a designed title card and a watermark behind a full page.
 */
export async function collectPageImages(
  page: { objs: ObjStore; commonObjs: ObjStore | null },
  opList: OpList | null,
  info: { pageNum: number; textLines: number },
  run: ImageRun,
): Promise<EncodedCandidate[]> {
  const candidates: EncodedCandidate[] = [];
  if (!opList) return candidates;

  for (const p of collectImagePlacements(opList)) {
    let encoded: EncodedImage | null | undefined = run.encoded.get(p.objId);
    if (!encoded) {
      // Out of budget means stop WAITING, not stop working, and the difference
      // is the cache: a picture whose bytes are already in hand costs nothing
      // to place, so an exhausted budget must not throw away a logo this
      // document has already paid for. Only an uncached placement is skipped,
      // and it is counted, because it is a picture we failed to carry.
      //
      // `continue`, not `break`. Abandoning the rest of the page was right
      // when nothing was cached — out of budget meant nothing more could
      // arrive — and became wrong the moment a later placement could be
      // answered from memory.
      if (run.remainingMs <= 0) {
        run.failed++;
        continue;
      }
      encoded = await encodePlacement(page, p.objId, run);
      if (!encoded) {
        run.failed++;
        continue;
      }
      run.encoded.set(p.objId, encoded);
    }

    candidates.push({
      pageNum: info.pageNum,
      x0: p.x0,
      y0: p.y0,
      x1: p.x1,
      y1: p.y1,
      srcPxWidth: p.srcPxWidth,
      srcPxHeight: p.srcPxHeight,
      pageTextLines: info.textLines,
      ...encoded,
    });
  }
  return candidates;
}

/**
 * Deterministic filename inside the picture folder (`<stem>.images/`), so
 * re-converting reproduces byte for byte.
 *
 * NOT exported, and that is the whole design: `RawImage.file` is the one
 * filename anything downstream ever needs, and a second caller would mean a
 * second per-page counter to keep in step with this one. The numbering and the
 * naming are three lines apart in `toRawImages` and cannot disagree.
 */
function imageFileName(pageNum: number, indexOnPage: number): string {
  return `p${String(pageNum).padStart(3, '0')}-${String(indexOnPage).padStart(2, '0')}.png`;
}

/**
 * True for a name the line above could have produced, and nothing else.
 *
 * Exported where `imageFileName` is not, because this asks a different
 * question and a caller outside the parser genuinely needs it: whoever
 * REWRITES the picture folder (src/library-images.ts) has to know which files
 * in it are its own, and "any name a note could carry" is not that set. The
 * filename allowlist in notes.ts (`isSafeImageFile`) is
 * deliberately wide — it accepts a hand-written `board-01.png` because a
 * `.fountain` is allowed to name one — so pruning against it deletes the
 * user's pictures out of a folder they filled themselves. That is a data-loss
 * bug, and it is why the two predicates are separate rather than one.
 *
 * `\d{3,}` and `\d{2,}`, not `\d{3}` and `\d{2}`: `padStart` pads and never
 * truncates, so a 1200-page shooting script produces `p1200-01.png`. Anchored
 * at both ends, with no separator in the class, so it can only ever match a
 * bare filename.
 *
 * Three lines from the generator on purpose. The pairing is pinned by a test
 * over `toRawImages`'s real output, since a drift between these two is silent:
 * the prune simply stops recognizing its own files.
 */
export function isGeneratedImageFile(name: string): boolean {
  return /^p\d{3,}-\d{2,}\.png$/.test(name);
}

/**
 * Kept candidates as `RawImage`, in reading order and named for where they sit.
 *
 * SORTED here (page ascending, y descending — PDF user space puts the top of a
 * sheet at the high y), and not left in paint order. `insertImages` in
 * src/parser/index.ts merges this array against the element stream on
 * exactly that ordering, so it has to be a fact
 * rather than a hope: a generator is free to paint the bottom panel of a strip
 * first. The sort is after `keepArtwork`'s cap and cannot change WHICH images
 * survive, only their order; equal-y placements keep their paint order, since
 * Array.prototype.sort is stable.
 *
 * ONE page geometry for the whole document, which is an assumption rather than
 * a fact — see `keepArtwork`, which shares it and states what it costs.
 */
export function toRawImages(
  kept: EncodedCandidate[],
  pageWidth: number,
  pageHeight: number,
): RawImage[] {
  const perPage = new Map<number, number>();
  return [...kept]
    .sort((a, b) => a.pageNum - b.pageNum || b.y1 - a.y1)
    .map((c) => {
      const n = (perPage.get(c.pageNum) ?? 0) + 1;
      perPage.set(c.pageNum, n);
      return {
        file: imageFileName(c.pageNum, n),
        pageNum: c.pageNum,
        // TOP edge, so a plain descending sort interleaves images with lines.
        y: c.y1,
        pageWidthPct: ((c.x1 - c.x0) / pageWidth) * 100,
        pageHeightPct: ((c.y1 - c.y0) / pageHeight) * 100,
        bytes: c.bytes,
      };
    });
}

/**
 * What became of the pictures, for a caller that wants to say so.
 *
 * Three of the four fields are USER-FACING, and this note used to say none of
 * them were: `imageWarnings` in convert.ts turns `overCap` and `failed` into a
 * sentence each, and `decodersUnavailable` adds the one cause of a failure
 * that is ours rather than the file's. Read that function for why those two
 * losses get a warning and the ordinary ones do not.
 *
 * `droppedBy` is the one that still has no consumer, and it is kept anyway. It
 * is the per-rule histogram a retune needs — "which threshold do I move" is
 * the entire content of one — and it earned the keep on 2026-08-12, when the
 * corpus audit recorded in artwork.ts was read straight off it. Already
 * computed inside `keepArtwork`; dropping it would mean re-plumbing this
 * signature, `extract.ts` and `convert.ts` to get it back, and a count nothing
 * returns is a count nothing can test.
 */
export interface ImageStats {
  /** Placements that yielded no candidate at all — see `ImageRun.failed`. */
  failed: number;
  /**
   * True only when pdf.js's out-of-bundle image decoders could not be put
   * anywhere it could load them (pdfjs-wasm.ts), which makes every JPEG2000,
   * JBIG2 and CCITT picture in the document fail at once.
   *
   * OPTIONAL, and absent on the happy path rather than false, because this
   * says nothing about the document: it is a fact about the machine we ran
   * on. `failed` already counts the pictures; this is the one cause of a
   * failure that is OURS rather than the file's, and the only one a user
   * could act on. Nothing else in this file sets it — extract.ts does, since
   * it is the module that configures pdf.js.
   */
  decodersUnavailable?: boolean;
  /** Cleared every filter and lost to the document cap alone. */
  overCap: number;
  /** Which rule refused each dropped candidate, keyed by the first to fire. */
  droppedBy: Record<DropRule, number>;
}

/**
 * The geometry the document is mostly made of, for the rules that need ONE.
 *
 * `keepArtwork` and `toRawImages` measure every image against a single page
 * size (see `keepArtwork` for what that assumption costs). Given that, the
 * question is only which page to take it from, and the most common one is the
 * least bad answer: on a uniform script — nearly all of them — it is exact,
 * and on a production draft carrying one landscape storyboard insert it
 * confines the error to that sheet instead of measuring the whole document
 * against whichever page happened to be last. Ties go to the first seen, so
 * this is deterministic on a document with no majority at all.
 *
 * Zeros for an empty list, which `keepArtwork` refuses to judge on rather than
 * dividing by.
 */
export function dominantPageSize(
  pages: { width: number; height: number }[],
): { width: number; height: number } {
  const tally = new Map<string, { width: number; height: number; pages: number }>();
  for (const p of pages) {
    const key = `${p.width}x${p.height}`;
    const seen = tally.get(key);
    if (seen) seen.pages++;
    else tally.set(key, { width: p.width, height: p.height, pages: 1 });
  }
  let best = { width: 0, height: 0, pages: 0 };
  for (const size of tally.values()) if (size.pages > best.pages) best = size;
  return { width: best.width, height: best.height };
}

/**
 * Everything about the pictures that cannot be decided until every page has
 * been read: which geometry the whole document is measured against, which page
 * count the furniture share divides by, and how the counts add up.
 *
 * The join between the two halves of this feature, and it lives HERE rather
 * than in extract.ts for the reason the halves were split in the first place.
 * This file already orchestrates (it collects placements, resolves pixels and
 * produces `RawImage[]`); `artwork.ts` holds the judgements and knows nothing
 * about pdf.js; and putting the call to `keepArtwork` in extract.ts would give
 * the text-extraction module a second edge, straight into the editorial one.
 * Importing `keepArtwork` as a VALUE here adds no pdf.js to `artwork.ts` — the
 * dependency only ever points this way — so the shim-free invariant that split
 * guards is untouched.
 *
 * Separate from `collectPageImages` because a page cannot answer any of it,
 * and separate from `extractDocument` because that function holds a real PDF:
 * the cases worth testing here — a mixed page geometry, sixty-one keepers, a
 * picture lost on the way in — are documents no fixture has.
 *
 * Takes the `ImageRun` rather than a bare failure count: `pageCount` and
 * `failed` are both plain numbers, and adjacent bare numbers are the hazard
 * `collectPageImages` uses an object to avoid three functions up.
 */
export function finishImages(
  candidates: EncodedCandidate[],
  pageSizes: { width: number; height: number }[],
  pageCount: number,
  run: ImageRun,
): { images: RawImage[]; imageStats: ImageStats } {
  const { width, height } = dominantPageSize(pageSizes);
  const { kept, overCap, droppedBy } = keepArtwork(candidates, width, height, pageCount);
  return {
    images: toRawImages(kept, width, height),
    imageStats: { failed: run.failed, overCap, droppedBy },
  };
}
