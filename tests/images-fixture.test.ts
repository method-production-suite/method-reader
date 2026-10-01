// The image subsystem against a REAL document: tests/fixtures/images.pdf.
//
// Split from images.test.ts, which is stubs only. Two reasons, and the second
// is the one that will matter next time something breaks. A stub answers
// however it is written, so the facts that cost the most to learn — that
// `objs.has()` lies, that pdf.js renames a repeated picture into commonObjs —
// can only be pinned against a file pdf.js actually opened. And the second
// half of this file exercises extract.ts's PAGE LOOP rather than images.ts, so
// a regression there should not report against a file named for the parser
// module. Same split by concern, under a shared prefix, as
// torture.test.ts / torture-layout / torture-markup / torture-coverage.
//
// Stays the first import, like extract.test.ts. Not because pdf.mjs cannot
// load without it: on pdfjs-dist 6.3.289 a bare import loads under bun 1.3.14
// (checked 2026-10-01). It hands getDocument the statically imported worker,
// gives the modern build the DOMMatrix shim it needs for rendering, and lets
// quiet-pdfjs.ts patch console.warn before pdf.mjs is evaluated; see
// pdfjs-shims.ts.
import '../src/parser/pdfjs-shims';
import { test, expect, describe, beforeAll } from 'bun:test';
import { getDocument } from 'pdfjs-dist/build/pdf.mjs';
import {
  collectImagePlacements,
  resolveImageData,
  toRgb,
  type PdfImage,
} from '../src/parser/images';
import { extractDocument, type ExtractResult } from '../src/parser/extract';
import { parseLines } from '../src/parser/index';
import { extractTitleMeta } from '../src/fountain/serialize';
import type { ParsedScreenplay, RawImage, ScreenplayElement } from '../src/parser/types';

/** The dimensions a PNG declares in its IHDR (bytes 16..24). Read off the
 * bytes that actually ship, since `RawImage` no longer carries them beside the
 * file: this asserts the encoder's output rather than a field copied from it,
 * which is the stronger of the two. */
function pngSize(b: Uint8Array): [number, number] {
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  return [dv.getUint32(16), dv.getUint32(20)];
}

const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** Fresh bytes every time: getDocument TRANSFERS the buffer to the worker, so
 * a second load of the same Uint8Array reads a detached array. */
const fixture = (name: string) =>
  Bun.file(`tests/fixtures/${name}`).arrayBuffer().then((b) => new Uint8Array(b));

describe('the pixel side against a real PDF', () => {
  // Resolved ONCE, in a beforeAll, with the assertions split across small
  // tests below. It was one ~50-line test carrying eight assertions, ordered
  // so that the count and size checks ran BEFORE the two claims it exists for
  // — a fixture regeneration that shifted a size hid both.
  //
  // The hook ASSERTS NOTHING, for the same reason. A failing beforeAll marks
  // every test in the describe as failed and names none of them, which is the
  // masking one layer up: `img` and `rgb` are carried nullable and the tests
  // below say what they should have been.
  interface Resolved {
    key: string;
    page: number;
    hasBefore: boolean;
    hasAfter: boolean;
    /** what the PAINT declared, against what the decoded image reports */
    declared: [number, number];
    img: PdfImage | null;
    rgb: Uint8Array | null;
  }
  const seen: Resolved[] = [];

  beforeAll(async () => {
    const pdf = await getDocument({ data: await fixture('images.pdf') }).promise;
    for (let n = 1; n <= pdf.numPages; n++) {
      const page = await pdf.getPage(n);
      for (const place of collectImagePlacements(await page.getOperatorList())) {
        const hasBefore = page.objs.has(place.objId);
        const img = await resolveImageData(page.objs, page.commonObjs, place.objId);
        seen.push({
          key: place.objId,
          page: n,
          hasBefore,
          hasAfter: page.objs.has(place.objId),
          declared: [place.srcPxWidth, place.srcPxHeight],
          img,
          rgb: img && toRgb(img),
        });
      }
    }
  });

  test('every paint resolves to pixels this parser can read', () => {
    expect(seen.filter((s) => s.img === null)).toEqual([]);
    expect(seen.filter((s) => s.rgb === null)).toEqual([]);
  });

  test('the decoded image is the size its paint operator declared', () => {
    // Both halves of the same fact: the size artwork.ts judges on comes from
    // the paint, and the size png.ts encodes comes from the image.
    for (const s of seen) expect([s.img!.width, s.img!.height]).toEqual(s.declared);
  });

  test('objs.has() LIES, which is why nothing here uses the synchronous get', () => {
    // The claim no stub can make: every image above resolved, and most of them
    // reported absent a moment earlier, because pdf.js sends pixels after the
    // operator list is already complete. `has() ? get() : skip` reads that as
    // an imageless script.
    expect(seen.filter((s) => !s.hasBefore).length).toBeGreaterThan(0);
  });

  test('a REPEATED picture is renamed and parked in commonObjs', () => {
    // The logo is `img_p0_2` on sheet 1 and `g_d0_img_p1_2` from sheet 2 on,
    // and the renamed one is in NEITHER page store, even once resolved: asking
    // only page.objs stalls five seconds a sheet and then reports no
    // furniture, which reads downstream as "this script has none".
    const renamed = seen.filter((s) => s.key.startsWith('g_'));
    expect(renamed.map((s) => s.page)).toEqual([2, 3, 4, 5]);
    expect(renamed.every((s) => !s.hasAfter)).toBe(true);
  });

  test('and a page-store image DOES report has() once resolved', () => {
    // So the line above is a discriminator rather than a has() that is simply
    // false for everything.
    expect(seen.filter((s) => !s.key.startsWith('g_')).every((s) => s.hasAfter)).toBe(true);
  });

  test('the same picture through two stores carries the same pixels', () => {
    // Or the furniture rule sees five different logos and keeps them all.
    // `.at(-1)`, not `[3]`: if a pdf.js bump changed when a picture gets
    // promoted, an index would throw on undefined instead of failing on the
    // claim, and a crash names nothing.
    const last = seen.filter((s) => s.key.startsWith('g_')).at(-1)!;
    const first = seen.find((s) => s.key === 'img_p0_2')!;
    expect([...last.rgb!]).toEqual([...first.rgb!]);
  });

  test('every paint converts to exactly width x height x 3 bytes', () => {
    for (const s of seen) expect(s.rgb!.length).toBe(s.img!.width * s.img!.height * 3);
  });

  test('ten paints of six pictures, at the sizes the fixture declares', () => {
    // make-fixture.py's own table (IMAGES / SHEET_PLACEMENTS).
    expect(seen).toHaveLength(10);
    expect([...new Set(seen.map((s) => `${s.img!.width}x${s.img!.height}`))].sort()).toEqual([
      '100x50', '120x160', '16x16', '200x200', '240x180', '261x351',
    ]);
  });
});

describe('extractDocument on the image fixture', () => {
  // One extraction, shared. Same reason as above, plus a plain one: opening
  // the fixture four times to assert four things costs four page loops.
  let images: RawImage[];
  let stats: ExtractResult['imageStats'];
  let pageCount: number;

  beforeAll(async () => {
    const out = await extractDocument(await fixture('images.pdf'));
    images = out.images;
    stats = out.imageStats;
    pageCount = out.pageCount;
  });

  test('keeps the title card and the panel, drops the other four', () => {
    expect(pageCount).toBe(5);
    expect(images.length).toBe(2);
    expect(images.map((i) => i.pageNum)).toEqual([1, 2]);
  });

  test('attributes every drop to the rule the fixture built for it', () => {
    // The fixture's whole point, end to end: one picture per rule. `width` and
    // not `area` for imSmall (20x20pt from 200x200px) because the linear
    // floors are asked first, and `background` and not `furniture` for imMark
    // because it sits on one sheet. Six counts plus kept plus overCap add up
    // to the ten paints collectImagePlacements found.
    expect(stats.droppedBy).toEqual({
      pixels: 1,      // imTiny, 16x16 source pixels
      width: 1,       // imSmall, 20pt wide on a 612pt sheet
      height: 0,
      area: 0,
      furniture: 5,   // imLogo, one per sheet
      background: 1,  // imMark, full bleed under 29 text lines
    });
    expect(stats.overCap).toBe(0);
    const dropped = Object.values(stats.droppedBy).reduce((a, b) => a + b, 0);
    expect(images.length + stats.overCap + dropped + stats.failed).toBe(10);
  });

  test('loses no picture to a failed resolve, conversion or encode', () => {
    // The count that makes a TOTAL loss visible. pdf.js's `data: null` bitmap
    // path is one runtime bump from opening under Bun, and if it opens every
    // image converts to null at once — a book with no art and no error,
    // indistinguishable from a script that never had pictures.
    expect(stats.failed).toBe(0);
  });

  test('the title card covers most of sheet 1', () => {
    const card = images[0];
    expect(card.pageWidthPct).toBeCloseTo((522 / 612) * 100, 0);
    expect(card.pageHeightPct).toBeCloseTo((702 / 792) * 100, 0);
  });

  test('and sits where it was drawn, in the axis RawLine uses', () => {
    // indent is the LEFT edge as a share of the sheet; y is the TOP edge in
    // PDF user space, so a descending sort interleaves images with lines.
    // The card is 522x702pt at (45, 45), so its top edge is 747.
    const card = images[0];
    expect(card.y).toBeCloseTo(747, 1);
  });

  test('every kept image carries a decodable PNG at its source resolution', () => {
    for (const img of images) {
      expect([...img.bytes.subarray(0, 8)]).toEqual(PNG_MAGIC);
      expect(pngSize(img.bytes)[0]).toBeGreaterThan(0);
    }
    // Both are well under IMAGE_MAX_EDGE, so the downscaler passed them
    // through: these are the fixture's own declared source dimensions.
    expect(images.map((i) => pngSize(i.bytes).join('x'))).toEqual(['261x351', '240x180']);
  });

  test('names each image by page and order on that page', () => {
    expect(images.map((i) => i.file)).toEqual(['p001-01.png', 'p002-01.png']);
  });

  test('a PDF with no images returns an empty array, not undefined', async () => {
    const out = await extractDocument(await fixture('screenplay.pdf'));
    expect(out.images).toEqual([]);
    expect(out.imageStats.failed).toBe(0);
    expect(out.imageStats.overCap).toBe(0);
    expect(Object.values(out.imageStats.droppedBy).every((n) => n === 0)).toBe(true);
    // The text half is untouched by any of this, and says so: extract.test.ts
    // and the convert suite already pin what screenplay.pdf's lines are, so a
    // second assertion on them here would only be a slower copy.
    expect(out.lines.length).toBeGreaterThan(0);
  });
});

describe('parseLines merges the fixture images into the element stream', () => {
  // The check hand-built streams cannot make. tests/parser-pipeline.test.ts
  // owns the merge's cases, but every y in it was chosen by the test author —
  // so it can only show the rule is self-consistent. These are pdf.js's own
  // coordinates, out of a document laid out by tools/make-fixture.py, and the
  // question they answer is whether the two halves agree about which way up a
  // page is.
  //
  // ONE extraction and ONE parse, shared — the same policy the describe above
  // states, and for the same reason: opening the fixture per test costs a page
  // loop per test. `imageEls` rather than `images`, which is a `RawImage[]` in
  // the describe above; these are the ScreenplayElements built from those.
  let parsed: ParsedScreenplay;
  let elements: ScreenplayElement[];
  let imageEls: ScreenplayElement[];

  beforeAll(async () => {
    const out = await extractDocument(await fixture('images.pdf'));
    // Both arrays exactly as extractDocument produced them, in its order.
    parsed = parseLines(out.lines, out.images);
    elements = parsed.elements;
    imageEls = elements.filter((el) => el.type === 'image');
  });

  test('both kept pictures reach the stream, and nothing else does', () => {
    expect(imageEls.map((el) => el.img!.file)).toEqual(['p001-01.png', 'p002-01.png']);
  });

  test('an image element keeps the page and top edge it was merged on', () => {
    // The merge key, carried onto the element rather than consumed and thrown
    // away: the card is 522x702pt at (45, 45) so its top is 747, and the panel
    // is 180x135pt at (108, 561) so its top is 696. Both in the axis the text
    // elements beside them use — element 1 is the title at y=504, element 5
    // the sheet-2 heading at y=720 — which is the whole basis of the merge.
    expect(imageEls.map((el) => [el.pageNum, el.y])).toEqual([[1, 747], [2, 696]]);
  });

  test('the title card comes BEFORE the title text it was drawn behind', () => {
    // The card's top edge is y=747 and the title's first line is y=504, both in
    // PDF user space where up is the high y. Compare those two the wrong way
    // round and the book opens on its title with the card underneath.
    expect(elements[0].img?.file).toBe('p001-01.png');
    expect(elements[1].text).toBe('THE PAPER LANTERN');
  });

  test('the card is full-page and fills the column', () => {
    // 85.3% x 88.6% of the sheet is 75.6% of its area, over the 70% bar; and
    // 85.3% of the sheet is 121% of the text column, clamped back to it.
    expect(imageEls[0].img).toEqual({ file: 'p001-01.png', widthPct: 100, fullPage: true });
  });

  test('the panel lands under its scene heading and above the action', () => {
    // make-fixture.py reserves 13 rows on sheet 2 for exactly this: heading at
    // y=720, panel top at y=696, action at y=540.
    const at = elements.findIndex((el) => el.img?.file === 'p002-01.png');
    expect(elements[at - 1].text).toBe('INT. ARCHIVE ROOM - DAY');
    expect(elements[at + 1].text).toContain('holds a storyboard panel');
  });

  test('the panel is inline, at its own share of the column', () => {
    // 180pt of a 612pt sheet is 29.4%, which is 42% of the 6.0" column.
    expect(imageEls[1].img).toEqual({ file: 'p002-01.png', widthPct: 42, fullPage: false });
  });

  test('a picture on the title page is not title-page furniture', () => {
    // Asserted, not detected, and this is the consequence that makes it right:
    // isBody() in serialize.ts drops title-page elements from the book, and
    // extractTitleMeta joins the readable ones into the TITLE. Flagged true,
    // the designed title card would vanish AND the book would be called
    // "p001-01.png THE PAPER LANTERN" — the filename survives verbatim
    // because humanizeCaps leaves mixed-case text alone.
    expect(imageEls[0].isTitlePage).toBe(false);
    expect(extractTitleMeta(elements)).toEqual({
      title: 'The Paper Lantern',
      author: 'J. Q. Nobody',
    });
  });

  test('neither picture becomes a speaker or a scene', () => {
    expect(parsed.characters.map((c) => c.name)).toEqual(['WREN', 'HOLT']);
    expect(parsed.scenes.length).toBe(4);
    // elementIndex counts the images ahead of each heading: the card sits at 0,
    // so sheet 2's heading is element 5 rather than element 4.
    expect(parsed.scenes.map((s) => s.elementIndex)).toEqual([5, 14, 22, 42]);
  });
});
