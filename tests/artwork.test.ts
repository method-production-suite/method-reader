// The editorial rules, against hand-built candidates.
//
// There is no `pdfjs-shims` import here and there must never be one. That
// absence is the point of the split: artwork.ts holds judgements about what a
// picture IS, so asserting that a 20x20pt image is too small to be art should
// never depend on module evaluation order, a DOMMatrix shim, or a pdf.js
// version. If a change to this file ever seems to need pdf.js, the change
// belongs in images.ts.
import { test, expect, describe } from 'bun:test';
import {
  columnWidthPct,
  FULL_PAGE_AREA_FRAC,
  isFullPage,
  keepArtwork,
  type DropRule,
  type ImageCandidate,
} from '../src/parser/artwork';

const PAGE_W = 612;
const PAGE_H = 792;

/**
 * A candidate that passes every filter, so each test can spoil one thing.
 *
 * Deliberately imPanel's geometry from tools/make-fixture.py (180x135pt at
 * 240x180px, 5.0% of the page) and not a knife-edge box: with only tenths of a
 * point of headroom every "keeps" test below would fail at once on an area
 * retune, and none of the failures would name the area floor. The boundaries
 * belong to the tests that are about boundaries.
 *
 * TRAP in the default `hash`. Two calls to `candidate()` are two copies of the
 * same picture, so on different pages they are FURNITURE at any `pageCount` of
 * 8 or under, and at any `pageCount` from three pages up. Any test that wants
 * two unrelated images has to give at least one its own hash — which is what
 * `lookbook()` does and why it takes the trouble.
 */
function candidate(over: Partial<ImageCandidate> = {}): ImageCandidate {
  return {
    pageNum: 2,
    x0: 108, y0: 561, x1: 288, y1: 696,   // 180 x 135 pt
    srcPxWidth: 240, srcPxHeight: 180,
    hash: 'panel',
    pageTextLines: 30,
    ...over,
  };
}

const hashes = (c: ImageCandidate[]) => c.map((x) => x.hash);

/** Every rule at zero, for the cases where nothing was refused by a rule. */
const NO_DROPS: Record<DropRule, number> = {
  pixels: 0, width: 0, height: 0, area: 0, furniture: 0, background: 0,
};

/** The rules that fired, as a plain list, so an assertion reads like a
 * sentence instead of an object literal with five zeroes in it. */
const firedRules = (droppedBy: Record<DropRule, number>): [DropRule, number][] =>
  (Object.entries(droppedBy) as [DropRule, number][]).filter(([, n]) => n > 0);

/** `n` distinct pictures, one per sheet, each a point WIDER than the last so
 * document order and size order disagree. Every one still clears every floor:
 * widths run 180pt up to 180 + n - 1, all of them well inside the sheet. */
const lookbook = (n: number): ImageCandidate[] =>
  Array.from({ length: n }, (_, i) => candidate({ pageNum: i + 1, hash: `p${i}`, x1: 288 + i }));

test('nothing in the module graph reaches pdf.js, and nothing may', async () => {
  // The split is only real if something enforces it, and prose in a header is
  // not something. Bundling is the check that answers for the WHOLE graph:
  // grepping this one file for import lines would miss a module that reaches
  // pdf.js two hops away, and probing a global at runtime would only ever see
  // whichever side effect that particular route happens to leave behind.
  // What this DOESN'T catch, verified by mutation rather than assumed: an
  // `import type`, which Bun.build erases, and a named import nothing uses,
  // which it tree-shakes. Both are the right line to draw — neither is a
  // runtime dependency. A pdf.js symbol that is actually called shows up as
  // three megabytes.
  //
  // For scale, if this ever fails: artwork.ts bundles to about 1.5KB today and
  // images.ts to 3MB, essentially all of it pdf.js.
  //
  // Resolved from this file rather than the cwd, so the test does not depend
  // on where `bun test` was invoked. And note what the assertion rests on:
  // Bun strips comments, WITHOUT which artwork.ts's own header would match the
  // pattern — it names `pdf.mjs` and `pdfjs-shims` in prose. If this ever goes
  // red with no import to blame, check that first.
  const entry = `${import.meta.dir}/../src/parser/artwork.ts`;
  const built = await Bun.build({ entrypoints: [entry], target: 'bun' });
  expect(built.success).toBe(true);
  const code = await built.outputs[0].text();
  expect(code).not.toMatch(/pdfjs|pdf\.mjs|pdf\.worker/);
});

describe('keepArtwork', () => {
  test('keeps an ordinary storyboard panel', () => {
    const { kept, overCap } = keepArtwork([candidate()], PAGE_W, PAGE_H, 5);
    expect(hashes(kept)).toEqual(['panel']);
    expect(overCap).toBe(0);
  });

  test('an empty document is empty, not undefined', () => {
    expect(keepArtwork([], PAGE_W, PAGE_H, 5)).toEqual({ kept: [], overCap: 0, droppedBy: NO_DROPS });
  });

  test('drops anything below the pixel floor', () => {
    // Rendered large but only 16x16 real pixels: upscaled to mush.
    const { kept } = keepArtwork([candidate({ srcPxWidth: 16, srcPxHeight: 16 })], PAGE_W, PAGE_H, 5);
    expect(kept).toEqual([]);
  });

  test('drops anything rendered too small to be art', () => {
    const tiny = candidate({ x1: 128, y1: 581 }); // 20 x 20 pt
    expect(keepArtwork([tiny], PAGE_W, PAGE_H, 5).kept).toEqual([]);
  });

  test('drops an image repeated across pages as furniture', () => {
    // Same pixels in the same corner of every sheet: a letterhead logo.
    const logo = (pageNum: number) => candidate({ pageNum, hash: 'logo' });
    const pages = [1, 2, 3, 4, 5].map(logo);
    expect(keepArtwork(pages, PAGE_W, PAGE_H, 5).kept).toEqual([]);
  });

  test('keeps a repeated image that repeats only twice', () => {
    // Two storyboard panels of the same shot is not furniture.
    const twice = [candidate({ pageNum: 2, hash: 'same' }), candidate({ pageNum: 3, hash: 'same' })];
    expect(keepArtwork(twice, PAGE_W, PAGE_H, 20).kept.length).toBe(2);
  });

  test('drops a full-bleed image sitting under a full page of text', () => {
    const watermark = candidate({ x0: 0, y0: 0, x1: PAGE_W, y1: PAGE_H, hash: 'mark', pageTextLines: 25 });
    expect(keepArtwork([watermark], PAGE_W, PAGE_H, 5).kept).toEqual([]);
  });

  test('keeps a full-bleed image on a page with almost no text', () => {
    // The designed title card. The ONLY difference from the watermark above is
    // the text count, which is the whole content of the background rule: the
    // geometry has to be identical or the test proves nothing.
    const card = candidate({ pageNum: 1, x0: 0, y0: 0, x1: PAGE_W, y1: PAGE_H, hash: 'card', pageTextLines: 0 });
    expect(hashes(keepArtwork([card], PAGE_W, PAGE_H, 5).kept)).toEqual(['card']);
  });

  test('caps the total and reports what it dropped, never silently', () => {
    const { kept, overCap } = keepArtwork(lookbook(70), PAGE_W, PAGE_H, 70);
    expect(kept.length).toBe(60);
    expect(overCap).toBe(10);
  });

  test('the cap keeps a document-order prefix, not a scattered subset', () => {
    // lookbook() grows its pictures towards the back of the document, so a cap
    // that preferred larger images would keep p69..p10 and this would fail. An
    // all-identical set would pass under either rule: Array#sort is stable.
    const { kept } = keepArtwork(lookbook(70), PAGE_W, PAGE_H, 70);
    expect(hashes(kept)).toEqual(Array.from({ length: 60 }, (_, i) => `p${i}`));
  });

  test('exactly the cap is not over it', () => {
    const { kept, overCap } = keepArtwork(lookbook(60), PAGE_W, PAGE_H, 60);
    expect(kept.length).toBe(60);
    expect(overCap).toBe(0);
  });

  test('overCap counts only the cap, never the filters', () => {
    // One good picture and nine pieces of furniture. Nothing here is over the
    // cap, and a caller must not warn the user that ten images were lost.
    const art = candidate({ pageNum: 1, hash: 'art' });
    const junk = Array.from({ length: 9 }, (_, i) =>
      candidate({ pageNum: i + 2, hash: `junk${i}`, srcPxWidth: 8, srcPxHeight: 8 }),
    );
    const { kept, overCap } = keepArtwork([art, ...junk], PAGE_W, PAGE_H, 20);
    expect(hashes(kept)).toEqual(['art']);
    expect(overCap).toBe(0);
  });
});

describe('keepArtwork boundaries', () => {
  test('exactly 32 source pixels on an edge is enough, 31 is not, either edge', () => {
    const at = candidate({ srcPxWidth: 32, srcPxHeight: 32 });
    expect(keepArtwork([at], PAGE_W, PAGE_H, 20).kept.length).toBe(1);
    // Both edges, because one clause guarding both would look identical here.
    expect(keepArtwork([candidate({ srcPxWidth: 32, srcPxHeight: 31 })], PAGE_W, PAGE_H, 20).kept)
      .toEqual([]);
    expect(keepArtwork([candidate({ srcPxWidth: 31, srcPxHeight: 32 })], PAGE_W, PAGE_H, 20).kept)
      .toEqual([]);
  });

  test('exactly 10% of page width is wide enough, a hair under is not', () => {
    // Tall enough that only the width floor is in play.
    const box = (w: number) => candidate({ x0: 0, x1: w, y0: 0, y1: PAGE_H * 0.5 });
    expect(keepArtwork([box(PAGE_W * 0.1)], PAGE_W, PAGE_H, 20).kept.length).toBe(1);
    expect(keepArtwork([box(PAGE_W * 0.1 - 0.01)], PAGE_W, PAGE_H, 20).kept).toEqual([]);
  });

  test('exactly 3% of page height is tall enough, a hair under is not', () => {
    // A banner OVERHANGING the sheet, 712pt wide on a 612pt page, and it has
    // to be: with the area floor also at 3%, a box that fits cannot be under
    // 3% of page height and still over 3% of page area, so a full-width box
    // here would fail both floors at once and prove nothing about this one.
    // Overhanging, the area is 3.49% at both sizes and only the height moves.
    const box = (h: number) => candidate({ x0: -50, x1: 662, y0: 0, y1: h });
    expect(keepArtwork([box(PAGE_H * 0.03)], PAGE_W, PAGE_H, 20).kept.length).toBe(1);
    expect(keepArtwork([box(PAGE_H * 0.03 - 0.01)], PAGE_W, PAGE_H, 20).kept).toEqual([]);
  });

  test('exactly 3% of page area is enough, a hair under is not', () => {
    // 30% of the width by 10% of the height: both linear floors are cleared by
    // a wide margin, so the area floor is the only thing this can fail.
    const box = (h: number) => candidate({ x0: 0, x1: PAGE_W * 0.3, y0: 0, y1: h });
    expect(keepArtwork([box(PAGE_H * 0.1)], PAGE_W, PAGE_H, 20).kept.length).toBe(1);
    expect(keepArtwork([box(PAGE_H * 0.1 - 0.01)], PAGE_W, PAGE_H, 20).kept).toEqual([]);
  });

  test('the background rule fires at exactly 90% of the page and 20 lines', () => {
    // A square page, and only here: 90% of 612x792 is not exactly representable
    // as a double, so on a letter sheet there is no box that sits ON this
    // threshold rather than a hair to one side of it.
    const SQ = 1000;
    const box = (h: number, pageTextLines: number) =>
      candidate({ x0: 0, x1: SQ, y0: 0, y1: h, pageTextLines, srcPxWidth: 400, srcPxHeight: 400 });
    expect(keepArtwork([box(SQ * 0.9, 20)], SQ, SQ, 20).kept).toEqual([]);
    expect(keepArtwork([box(SQ * 0.9, 19)], SQ, SQ, 20).kept.length).toBe(1);
    expect(keepArtwork([box(SQ * 0.9 - 0.01, 20)], SQ, SQ, 20).kept.length).toBe(1);
  });
});

describe('keepArtwork furniture', () => {
  test('three distinct pages is furniture even in a long script', () => {
    const on = (pages: number[]) => pages.map((p) => candidate({ pageNum: p, hash: 'logo' }));
    expect(keepArtwork(on([1, 2, 3]), PAGE_W, PAGE_H, 120).kept).toEqual([]);
    expect(keepArtwork(on([1, 2]), PAGE_W, PAGE_H, 120).kept.length).toBe(2);
  });

  test('the same picture twice on ONE sheet is one page, not two', () => {
    // A storyboard strip repeating a panel down the sheet. Counting placements
    // instead of distinct pages would read this as furniture.
    const strip = [
      candidate({ pageNum: 4, hash: 'shot', y0: 561, y1: 696 }),
      candidate({ pageNum: 4, hash: 'shot', y0: 300, y1: 435 }),
      candidate({ pageNum: 4, hash: 'shot', y0: 60, y1: 195 }),
    ];
    expect(keepArtwork(strip, PAGE_W, PAGE_H, 8).kept.length).toBe(3);
  });

  test('a quarter of the sheets is furniture once it is on two of them', () => {
    const on = (pages: number[], pageCount: number) =>
      keepArtwork(pages.map((p) => candidate({ pageNum: p, hash: 'logo' })), PAGE_W, PAGE_H, pageCount);
    expect(on([1, 2], 8).kept).toEqual([]);       // 25%
    expect(on([1, 2], 9).kept.length).toBe(2);    // 22.2%
  });

  test('one appearance is never furniture, however short the document', () => {
    // The share alone would make a lone picture furniture in any document of
    // four sheets or fewer, and certain of it in a one-page document at 100%.
    // Repetition needs something to repeat against.
    for (const pageCount of [1, 2, 3, 4]) {
      const { kept } = keepArtwork([candidate({ pageNum: 1, hash: 'art' })], PAGE_W, PAGE_H, pageCount);
      expect(hashes(kept)).toEqual(['art']);
    }
  });

  test('the repetition tally counts every placement, not only the big ones', () => {
    // A letterhead scaled down on two sheets and printed large on a third is
    // still letterhead. Tallying only the placements that clear the size floor
    // would carry the large one.
    const small = { x1: 128, y1: 581 }; // 20 x 20 pt, under every size floor
    const mixed = [
      candidate({ pageNum: 1, hash: 'logo', ...small }),
      candidate({ pageNum: 2, hash: 'logo', ...small }),
      candidate({ pageNum: 3, hash: 'logo' }),
    ];
    expect(keepArtwork(mixed, PAGE_W, PAGE_H, 40).kept).toEqual([]);
  });

  test('an image that is both furniture and background is simply gone', () => {
    const both = [1, 2, 3].map((pageNum) =>
      candidate({ pageNum, hash: 'mark', x0: 0, y0: 0, x1: PAGE_W, y1: PAGE_H, pageTextLines: 40 }),
    );
    expect(keepArtwork(both, PAGE_W, PAGE_H, 12).kept).toEqual([]);
  });
});

describe('keepArtwork on boxes that are not boxes', () => {
  test('a zero-area box is not art', () => {
    expect(keepArtwork([candidate({ x1: 108 })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
    expect(keepArtwork([candidate({ y1: 561 })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
  });

  test('an inverted box is not art, in either axis or both', () => {
    expect(keepArtwork([candidate({ x0: 288, x1: 108 })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
    expect(keepArtwork([candidate({ y0: 696, y1: 561 })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
    // Both inverted multiplies back to a POSITIVE area, so an area-only floor
    // would wave this one through.
    expect(
      keepArtwork([candidate({ x0: 288, x1: 108, y0: 696, y1: 561 })], PAGE_W, PAGE_H, 20).kept,
    ).toEqual([]);
  });

  test('a box with no number in it is not art', () => {
    // Every `<` in a threshold test is false against NaN, so the naive form of
    // each rule would KEEP this and put `w=NaN` in the .fountain.
    expect(keepArtwork([candidate({ x1: NaN })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
    expect(keepArtwork([candidate({ y0: NaN })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
    expect(keepArtwork([candidate({ srcPxWidth: NaN })], PAGE_W, PAGE_H, 20).kept).toEqual([]);
  });

  test('a page with no size yields no art rather than infinities', () => {
    // pageTextLines 0, and it has to be: divided by a zero page the fractions come
    // out Infinity, and Infinity is over the BACKGROUND area threshold as well
    // as every floor, so a candidate on a text-heavy page would be dropped by
    // the background rule and prove nothing about the guard.
    const c = candidate({ pageTextLines: 0 });
    expect(keepArtwork([c], 0, PAGE_H, 20)).toEqual({ kept: [], overCap: 0, droppedBy: NO_DROPS });
    expect(keepArtwork([c], PAGE_W, 0, 20)).toEqual({ kept: [], overCap: 0, droppedBy: NO_DROPS });
  });
});

describe('the thresholds still mean what tools/make-fixture.py says they mean', () => {
  // tests/fixtures/images.pdf is six pictures on five sheets, and make-fixture.py
  // states each one's margin to the ONE threshold that governs it. Reproducing
  // those boxes here catches a retune the moment it is made, in the file that
  // made it, rather than in an integration test that has to open a PDF — and it
  // catches the direction make-fixture.py warns about, a KEPT image quietly
  // becoming dropped, which the generator itself cannot notice.
  const LOGO = { x0: 376, y0: 36, x1: 576, y1: 136 }; // PAGE_W - 200 - 36, 36
  const fixture: ImageCandidate[] = [
    { pageNum: 1, x0: 45, y0: 45, x1: 567, y1: 747, srcPxWidth: 261, srcPxHeight: 351, hash: 'imCard', pageTextLines: 5 },
    { pageNum: 2, x0: 108, y0: 561, x1: 288, y1: 696, srcPxWidth: 240, srcPxHeight: 180, hash: 'imPanel', pageTextLines: 12 },
    { pageNum: 3, x0: 108, y0: 400, x1: 238, y1: 530, srcPxWidth: 16, srcPxHeight: 16, hash: 'imTiny', pageTextLines: 12 },
    { pageNum: 3, x0: 300, y0: 440, x1: 320, y1: 460, srcPxWidth: 200, srcPxHeight: 200, hash: 'imSmall', pageTextLines: 12 },
    { pageNum: 4, x0: 0, y0: 0, x1: 612, y1: 792, srcPxWidth: 120, srcPxHeight: 160, hash: 'imMark', pageTextLines: 29 },
    ...[1, 2, 3, 4, 5].map((pageNum) => ({
      pageNum, ...LOGO, srcPxWidth: 100, srcPxHeight: 50, hash: 'imLogo', pageTextLines: 12,
    })),
  ];

  test('two of the six survive: the title card and the panel', () => {
    const { kept, overCap } = keepArtwork(fixture, PAGE_W, PAGE_H, 5);
    expect(hashes(kept)).toEqual(['imCard', 'imPanel']);
    expect(overCap).toBe(0);
  });

  test('each of the four is dropped by the rule make-fixture.py names', () => {
    // Asked directly, now that the verdict attributes a drop. This used to
    // isolate one hash and re-run, which only worked because this fixture is
    // separable by construction — no real script is, and a corpus pass needs
    // the histogram rather than a bisection.
    const { droppedBy } = keepArtwork(fixture, PAGE_W, PAGE_H, 5);
    expect(droppedBy).toEqual({
      pixels: 1,      // imTiny, 16x16 against 32
      width: 1,       // imSmall, 3.3% against 10% — and width is asked first
      height: 0,
      area: 0,
      furniture: 5,   // imLogo, one per sheet: the count is placements
      background: 1,  // imMark, full bleed under 29 lines
    });
  });

  test('the four drops are attributed to the rule and not merely to some rule', () => {
    // Each fixture picture, repaired at exactly the threshold that governs it,
    // becomes art. That is the claim make-fixture.py's table makes, and it is
    // the one an attribution count alone cannot make: a candidate failing two
    // rules would report only the first.
    const only = (hash: string) => fixture.filter((c) => c.hash === hash);
    const survives = (cs: ImageCandidate[]) => keepArtwork(cs, PAGE_W, PAGE_H, 5).kept.length > 0;

    // imTiny clears all three size floors at 130x130pt: real pixels and it is art.
    expect(survives(only('imTiny').map((c) => ({ ...c, srcPxWidth: 200, srcPxHeight: 200 })))).toBe(true);
    // imLogo clears every SIZE floor: on one sheet, only repetition was against it.
    expect(survives(only('imLogo').slice(0, 1))).toBe(true);
    // imMark clears everything but the text count on its sheet.
    expect(survives(only('imMark').map((c) => ({ ...c, pageTextLines: 19 })))).toBe(true);
    // imSmall is the exception, and make-fixture.py says so: it fails all three
    // size floors at once, so no single repair rescues it. Widening it alone
    // moves the verdict to the next floor rather than to art, which is exactly
    // what "first rule to fire" means.
    const widened = only('imSmall').map((c) => ({ ...c, x1: c.x0 + 200 }));
    expect(survives(widened)).toBe(false);
    expect(firedRules(keepArtwork(widened, PAGE_W, PAGE_H, 5).droppedBy)).toEqual([['height', 1]]);
  });
});

describe('FULL_PAGE_AREA_FRAC', () => {
  test('stays inside the band its own comment describes', () => {
    // Exported for the ImageRef builder and not consumed in this module, so no
    // other test here constrains it: a mutation sweep found that raising it to
    // 0.75 survives everything. A BAND and not an exact pin, because this is
    // one of the numbers Task 18 is expected to move, and a test that forbids
    // a legitimate retune gets deleted rather than consulted.
    //
    // Ceiling: the fixture's own title card, 75.6% of its page. Above that the
    // one image this constant exists to promote silently stops being
    // full-page, with every suite green. Floor: a storyboard strip taking up
    // to 60% of a sheet must NOT qualify, which is the "half-page strip does
    // not count" half of the comment.
    const FIXTURE_TITLE_CARD = (522 * 702) / (612 * 792);
    expect(FULL_PAGE_AREA_FRAC).toBeLessThanOrEqual(FIXTURE_TITLE_CARD);
    expect(FULL_PAGE_AREA_FRAC).toBeGreaterThanOrEqual(0.6);
  });
});

describe('isFullPage', () => {
  // The units are the whole risk: two PERCENTAGES compared against a
  // FRACTION. Both arguments 90 must not read as 8100x the bar.

  test('the fixture title card qualifies and the fixture panel does not', () => {
    // tools/make-fixture.py: imCard is 522x702pt and imPanel 180x135pt, both on
    // a 612x792 sheet — 75.6% of the page against 5.0%.
    expect(isFullPage((522 / 612) * 100, (702 / 792) * 100)).toBe(true);
    expect(isFullPage((180 / 612) * 100, (135 / 792) * 100)).toBe(false);
  });

  test('the threshold is the area, and it is inclusive', () => {
    // 70% x 100% is exactly FULL_PAGE_AREA_FRAC. A `>` instead of `>=` would
    // read a full-bleed strip of exactly the bar as inline.
    expect(isFullPage(FULL_PAGE_AREA_FRAC * 100, 100)).toBe(true);
    expect(isFullPage(FULL_PAGE_AREA_FRAC * 100 - 0.1, 100)).toBe(false);
  });

  test('neither dimension decides alone', () => {
    // Full width and a third of the height is a banner, not a page; full
    // height and a third of the width is a sidebar.
    expect(isFullPage(100, 33)).toBe(false);
    expect(isFullPage(33, 100)).toBe(false);
  });
});

describe('columnWidthPct remaps a page share to a column share', () => {
  // The one judgement in this file that DESCRIBES a kept image rather than
  // selecting one, so keepArtwork constrains none of it. index.ts calls it for
  // every image element; these are the arithmetic's own tests.

  test('the reference is the 6.0-inch text column, not the 8.5-inch sheet', () => {
    // docs/screenplay-format-reference.md: L 1.5", R 1.0", body 6.0". Half a
    // sheet is 71% of that column, and half a COLUMN is 35% of the sheet.
    expect(columnWidthPct(50)).toBe(71);
    expect(columnWidthPct(35.3)).toBe(50);
  });

  test('an image exactly as wide as the column comes out at 100', () => {
    // 6.0 / 8.5 = 70.6%. The number the whole remap is anchored on: get the
    // fraction wrong and this is the assertion that says so.
    expect(columnWidthPct(70.6)).toBe(100);
  });

  test('anything wider than the column is clamped to it', () => {
    // The fixture's title card is 85.3% of its sheet — 121% of the column.
    expect(columnWidthPct((522 / 612) * 100)).toBe(100);
    expect(columnWidthPct(100)).toBe(100);
  });

  test('the floor holds, and keepArtwork cannot reach it', () => {
    // MIN_WIDTH_FRAC already refuses anything under 10% of the page, and
    // 10 / 0.706 is 14 — so the floor only ever answers a caller that skipped
    // the filters. Pinned anyway: nothing else in either suite touches it.
    expect(columnWidthPct(0)).toBe(5);
    expect(columnWidthPct(1)).toBe(5);
    expect(columnWidthPct(10)).toBe(14);
  });

  test('a width that is not a number lands on the floor, not on NaN', () => {
    // The house `!(x > floor)` form. A NaN through the naive clamp reaches the
    // [[img: ...]] note as `NaN`, which is an invalid CSS length a long way
    // from whatever produced it.
    expect(columnWidthPct(NaN)).toBe(5);
  });
});

describe('droppedBy attributes a drop to the rule that caused it', () => {
  const fired = (cs: ImageCandidate[], pageCount = 20) =>
    firedRules(keepArtwork(cs, PAGE_W, PAGE_H, pageCount).droppedBy);

  test('each rule reports under its own name', () => {
    expect(fired([candidate({ srcPxWidth: 16, srcPxHeight: 16 })])).toEqual([['pixels', 1]]);
    // 5% of the width by 70% of the height: 3.5% of the page, so it CLEARS the
    // area floor and width is genuinely alone. (At half the sheet tall it
    // would be 2.5% and fail area as well, reporting `width` only because
    // width is asked first — which would prove ordering, not attribution.)
    expect(fired([candidate({ x0: 0, x1: PAGE_W * 0.05, y0: 0, y1: PAGE_H * 0.7 })]))
      .toEqual([['width', 1]]);
    // Overhanging the sheet at 2.8% of its height: 3.26% of the page, so the
    // area floor is cleared and height is alone. Only an overhanging box can
    // be in this state at all — see MIN_HEIGHT_FRAC.
    expect(fired([candidate({ x0: -50, x1: 662, y0: 0, y1: PAGE_H * 0.028 })]))
      .toEqual([['height', 1]]);
    expect(fired([candidate({ x0: 0, x1: PAGE_W * 0.3, y0: 0, y1: PAGE_H * 0.1 - 0.01 })]))
      .toEqual([['area', 1]]);
    expect(fired([1, 2, 3].map((pageNum) => candidate({ pageNum, hash: 'logo' }))))
      .toEqual([['furniture', 3]]);
    expect(fired([candidate({ x0: 0, y0: 0, x1: PAGE_W, y1: PAGE_H, pageTextLines: 20 })]))
      .toEqual([['background', 1]]);
  });

  test('a candidate failing several rules reports the FIRST one', () => {
    // 20x20pt at 8x8px fails the pixel floor, both linear floors and the area
    // floor. Reported as `pixels` because that is the cheapest question and it
    // is asked first — a histogram is only readable if the attribution rule is
    // stated, and this is it.
    expect(fired([candidate({ srcPxWidth: 8, srcPxHeight: 8, x1: 128, y1: 581 })]))
      .toEqual([['pixels', 1]]);
    // Width before height: a box that is under both reports width.
    expect(fired([candidate({ x0: 0, x1: PAGE_W * 0.05, y0: 0, y1: PAGE_H * 0.01 })]))
      .toEqual([['width', 1]]);
  });

  test('every candidate is accounted for exactly once', () => {
    // The property that makes the histogram usable on a corpus: kept + over
    // the cap + one rule each for the rest, adding up to what came in. A
    // double-count or a silent skip would show here and nowhere else.
    const mixed: ImageCandidate[] = [
      ...lookbook(65),                                                    // 60 kept, 5 over cap
      candidate({ pageNum: 90, hash: 'tiny', srcPxWidth: 4, srcPxHeight: 4 }),
      candidate({ pageNum: 91, hash: 'thin', x0: 0, x1: 12, y0: 0, y1: PAGE_H * 0.5 }),
      ...[92, 93, 94].map((pageNum) => candidate({ pageNum, hash: 'logo' })),
      candidate({ pageNum: 95, hash: 'mark', x0: 0, y0: 0, x1: PAGE_W, y1: PAGE_H, pageTextLines: 40 }),
    ];
    const { kept, overCap, droppedBy } = keepArtwork(mixed, PAGE_W, PAGE_H, 96);
    const dropped = Object.values(droppedBy).reduce((a, b) => a + b, 0);
    expect(kept.length + overCap + dropped).toBe(mixed.length);
    expect(kept.length).toBe(60);
    expect(overCap).toBe(5);
    expect(droppedBy).toEqual({
      pixels: 1, width: 1, height: 0, area: 0, furniture: 3, background: 1,
    });
  });

  test('the cap is not a rule, so its victims never appear in droppedBy', () => {
    // A caller warning on `overCap` and a caller reading the histogram are
    // asking different questions; conflating them would make every lookbook
    // look like a filter problem.
    const { overCap, droppedBy } = keepArtwork(lookbook(70), PAGE_W, PAGE_H, 70);
    expect(overCap).toBe(10);
    expect(droppedBy).toEqual(NO_DROPS);
  });
});
