/**
 * What separates a picture from furniture (registry #19).
 *
 * Every threshold here exists because a real PDF is full of images nobody
 * wants in a book: spacer pixels, logo glyphs, revision bullets, letterhead,
 * and watermarks printed behind the script. Carrying those would be worse
 * than carrying nothing, because the reader cannot tell which failure they
 * are looking at.
 *
 * PROVENANCE, and read this before moving anything. Every number below was
 * chosen BY EYE from the August design document on 2026-08-06, against no real
 * script: `tests/fixtures/images.pdf` was built to exercise these rules, so it
 * could only ever show they are self-consistent, never that they are right.
 *
 * AUDITED 2026-08-12 against the private corpus, and NOTHING MOVED. Seven real
 * documents — five feature screenplays, a 110-page script-plus-lookbook, and a
 * 30-page pitch deck — 207 painted placements, 83 kept, 58 refused by these
 * rules, 66 lost to the cap. Every kept picture was looked at. In the two
 * documents that BOTH carry pictures and pass the guards there were no false
 * positives and no false negatives: the lookbook's 22 keeps are its poster,
 * its reference stills and its cast portraits, and its 12 refusals are one
 * ghosted key-art plate repeated behind eleven sheets plus one 84x17 blank
 * strip, with no logo, letterhead or rule anywhere in the kept set. What the
 * audit changed is the confidence, not the constants — each note below now
 * carries the margin the corpus left it, and the two claims the corpus
 * contradicted (the width floor's independence, and "an oversized box is
 * unreachable") are corrected where they were made.
 *
 * They are still NOT equally load-bearing: MIN_HEIGHT_FRAC can barely fire at
 * all, which the corpus confirmed rather than merely predicted, and
 * FURNITURE_SHARE_MIN_PAGES is the one value here that is derived rather than
 * guessed. `droppedBy` on the verdict is what made the audit possible at all:
 * it says which rule killed a picture, which is the entire content of the
 * question "which threshold do I move".
 *
 * NOTHING here may import pdf.js, directly or through another module, and
 * tests/artwork.test.ts must never grow a `pdfjs-shims` import either. That
 * absence is the split, not a coincidence. images.ts changes when pdf.js
 * changes — a version bump, a new operator, a renamed object store — and this
 * file changes when a real script proves a threshold wrong; those two edits are
 * never in the same commit. png.ts draws the same line around itself for the
 * same reason, and states it in its own header. The practical half matters too:
 * every file that reaches pdf.js inherits an import-ORDER rule. The modern
 * build needs the DOMMatrix shim for rendering and the statically imported
 * worker, and quiet-pdfjs.ts has to patch console.warn before pdf.mjs is
 * evaluated; see pdfjs-shims.ts. The numbers below are the likeliest thing in
 * this feature to be hand-tuned after a device or corpus pass, and asserting
 * that a 20x20pt image is too small to be art should not require a shim.
 *
 * These thresholds are NOT format options (the August design allowed no size
 * options, and today's spec has "No greyscale or size controls in this
 * design"): they are documented behavior with a rationale, like the revision-star
 * and page-furniture rules. They are also written down twice — `make-fixture.py`
 * states each fixture picture's margin to the ONE threshold that governs it —
 * so tests/artwork.test.ts reproduces those six boxes and pins both copies of
 * the number at once.
 */

/**
 * Rendered size floors, as shares of the page.
 *
 * Three numbers, and only two of them can fire on a box that FITS on the
 * sheet. The width floor is meant to earn its place on its own: a tall narrow
 * strip — a sidebar rule, a vertical revision bar — clears 3% of the page area
 * while being 8% of its width, and MIN_WIDTH_FRAC would be the only thing that
 * stops it. The height floor cannot do the same, because with the area floor
 * at the same 3% a box under 3% of page height only reaches 3% of page area by
 * being WIDER than the page. It stays for the box that is: a full-bleed banner
 * overhanging the sheet. Note which way round that is — the redundancy is a
 * coincidence of two numbers happening to be equal today, not a property of
 * the rules, and lowering MIN_AREA_FRAC alone would put the height floor back
 * to work.
 *
 * 2026-08-12, and it is the first of the two things in this file the corpus
 * CONTRADICTED: neither linear floor was individually load-bearing on it. All
 * 23 width drops and both height drops were also under the area floor, so with
 * either linear check deleted the corpus's verdicts would be unchanged. The
 * tall narrow strip is arithmetic rather than an observation — real, since 8%
 * by 40% is 3.2% of a page, but no document has yet produced one. Keep both
 * floors:
 * their cost is zero (the corpus's nearest kept picture is 13.1% of page width
 * against the 10% floor, and the nearest refused is 9.6%, an empty band either
 * side) and the case they cover is one the area floor genuinely cannot see.
 * Just do not read them as pulling their weight today.
 *
 * What 3% of a page IS, since a bare fraction anchors nothing: on US Letter it
 * is 14,541 pt², about 1.9 by 1.45 inches at 4:3. Comfortably under a
 * storyboard panel, comfortably over a revision bullet. What it does NOT clear
 * is branding — the fixture's letterhead logo is 4.1% of its page and sails
 * through this floor, which is why the furniture rule exists. Read this floor
 * as aimed at decoration, never at logos, and do not raise it to try to catch
 * one: at 5% it starts eating small inline panels instead.
 *
 * That last sentence was a guess and is now a measurement, which is why this
 * floor is the tightest number in the file. The smallest picture the corpus
 * KEPT THAT A READER WANTED is 3.30% of its page (a cast portrait in the
 * lookbook, one of three on that sheet at 3.30-3.40%), and the largest the
 * AREA rule refused is 2.54%. Real art and real decoration are 0.8 of a
 * percentage point apart here, with the floor between them. 3.5% would already
 * cost three portraits; 5% is not a distant hazard.
 *
 * "That a reader wanted" is load-bearing and not a hedge, so read the two
 * populations apart before moving anything. The RAW smallest keep across the
 * corpus is lower: 3.02%, re-measured 2026-08-12, and it is design decoration
 * in the pitch deck rather than art anyone asked for — the same deck whose
 * headings are bitmaps, which no geometric rule can see and which the
 * not-a-screenplay guard refuses before any of it renders. So the floor has
 * 0.8 of a point of daylight against ART and about half a point against
 * EVERYTHING, and the second number is the one a retune actually has to clear.
 * The raw figure and the art margin are two different pairs, so quote them
 * apart.
 */
const MIN_AREA_FRAC = 0.03;      // of the page
const MIN_WIDTH_FRAC = 0.10;     // of page width
const MIN_HEIGHT_FRAC = 0.03;    // of page height

/** Source pixels required on EACH edge. Not a share of anything: the question
 * is whether there is enough picture there to be worth carrying at all, and a
 * 16x16 logo glyph is not, however large the page draws it. Judged on the
 * source size the paint operator declared rather than on what we encoded,
 * though below this floor the two are the same number anyway: downscaleRgb
 * only ever shrinks, and only past its max-edge limit.
 *
 * The loosest number here after the 2026-08-12 corpus pass, and deliberately
 * left alone: it fired exactly once in 207 placements, on an 84x17 blank
 * strip, while the smallest edge on any picture the corpus kept was 110 px.
 * Nothing lives in the band between, so there is no evidence for a number and
 * no reason to move one. */
const MIN_PIXELS = 32;

/**
 * Repetition that makes a picture furniture.
 *
 * The sibling of `suppressBoilerplate` in boilerplate.ts, which does this for
 * TEXT: a line recurring on enough distinct pages is a watermark rather than
 * dialogue. Same shape, three deliberate differences. Sameness is a hash of the
 * ENCODED PNG rather than normalized text, because a logo drawn on every page
 * is frequently a distinct XObject per page and identical pixels are the fact
 * we care about. The share is lower (25% against 40%) and combines with the
 * flat page count by OR rather than boilerplate's `Math.max`, so images are
 * caught earlier: a false positive there hides a line of a script, where a
 * false positive here loses a picture.
 *
 * FURNITURE_SHARE_MIN_PAGES belongs to the SHARE, not to the flat count, and
 * it is the one number in this file that is derived rather than chosen. The
 * share alone calls a single unrepeated picture furniture in any document of
 * four sheets or fewer — 1 of 4 is 25% — and in a one-page document it is
 * certain of it at 100%. That is a category error rather than a threshold
 * being too tight: "repeated" needs something to repeat against, so the floor
 * is 2 and could not honestly be anything else. With it in place the share's
 * whole remaining job is "in a document of eight sheets or fewer, twice is
 * enough", which is the short-document case it was added for.
 *
 * 2026-08-12: the best-evidenced rule in the file, and the one with the
 * clearest blind spot. It accounts for 19 of the corpus's 58 refusals and
 * every one was right — a key-art plate that the lookbook prints ghosted
 * behind eleven of its sheets, and a decorative picture frame on four sheets
 * of the pitch deck.
 *
 * THOSE TWO NUMBERS DO ADD UP, and the unit is why it looks otherwise: 11 and
 * 4 are SHEETS, 19 is PLACEMENTS, and the frame is painted TWICE on each of
 * its four sheets. 11 + 8 = 19. Re-measured 2026-08-12 on the deck, which is
 * still in the corpus: one hash, four distinct pages, eight candidates. Worth
 * spelling out because `droppedBy` is quoted by page count everywhere else in
 * this file and reports placements, and reading one as the other is how "19"
 * and "11 and 4" came to sit two paragraphs apart looking like a
 * contradiction. Worth knowing about that plate: the XObject is at FULL
 * opacity and the ghosting is a graphics-state alpha applied where it is
 * painted, so a book that kept it would show eleven full-strength copies of
 * the poster, not eleven faint ones. What the page looks like is not what the
 * picture is. The plate is also the case that justifies the rule existing
 * separately from the background one: ten of those eleven sheets carry fewer
 * than twenty text lines (17, 16, 4, 10, 13, 10, 9, 16, 16 and 0 against 28 on
 * the eleventh), so the background rule would have let ten copies through.
 * Nothing legitimate came near: no picture the corpus KEPT appears on more
 * than two sheets.
 *
 * THE BLIND SPOT, and it is the hash: a logo on 23 of the pitch deck's 30
 * sheets was a DIFFERENT encoded picture on every sheet — same 278x139 source
 * size, same placement, 23 distinct hashes — so this rule never saw it as
 * repeated at all and the size floors caught it instead. Hashing bytes beats
 * hashing pdf.js's object id (see ImageCandidate.hash) and both lose to a
 * producer that re-rasterizes its furniture per page. Nothing to fix here
 * without a perceptual comparison, which is a different rule and not a
 * threshold; recorded so the next person does not read the eleven-sheet
 * success as the general case.
 */
const FURNITURE_PAGES = 3;
const FURNITURE_FRAC = 0.25;
const FURNITURE_SHARE_MIN_PAGES = 2;

/**
 * Covering this much of a page THAT ALSO HOLDS this much text: a background.
 *
 * Neither half means anything alone — the text count is the only thing that
 * tells a watermark printed behind the script from a designed title card of
 * the same geometry.
 *
 * 20 lines is against the roughly 55 a US Letter screenplay page holds
 * (make-fixture.py's LINES_PER_PAGE), so read it as "more than about a third
 * of a page of text", not as a bare count. That is the number to move if this
 * rule ever misfires, and it moves in a narrow corridor: down near 5 and a
 * title card carrying a credit block reads as a background, up near 40 and a
 * watermark under a half-full page gets carried into the book.
 *
 * 2026-08-12, and the corpus's answer is one sample sitting exactly on the
 * line, so read it as support rather than as proof. The rule fired ONCE in 207
 * placements, on a full-bleed still covering 135% of a pitch-deck sheet that
 * carried exactly 20 lines of text, which is a backdrop and was rightly
 * refused. Fourteen other pictures covering 90% of a sheet or more were KEPT
 * across three documents, on sheets carrying 0 to 12 lines, and they include
 * both of the full-page plates that went on to become covers — so the text
 * half is what did the discriminating, as this note claims, rather than the
 * area half doing all the work. What no document has yet supplied is a sheet
 * of 13 to 19 lines under a full-bleed picture, which is exactly where a wrong
 * number would show. Do not move it on one sample.
 */
const BACKGROUND_AREA_FRAC = 0.90;
const BACKGROUND_TEXT_LINES = 20;

/** Art occupying this share of its page is a full-page image. It DESCRIBES a
 * kept image rather than selecting one, so no rule above consults it; it lives
 * beside the other judgements rather than beside the pdf.js walk for the same
 * reason everything else in this file does. Loose enough to catch a title card
 * inside its printer margins (the fixture's is 75.6%), tight enough that a
 * half-page storyboard strip does not qualify — which is the band, not the
 * exact value, that tests/artwork.test.ts pins. The corpus (2026-08-12) left
 * the band uncontested rather than confirming the value: its two full-page
 * plates are both over 100% of their sheet and everything else it kept out of
 * a real screenplay is under 19%, so nothing has yet landed between 19% and
 * 100% for this number to rule on. */
export const FULL_PAGE_AREA_FRAC = 0.70;

/**
 * Did this image cover enough of its sheet to be a full-page one?
 *
 * The arithmetic and not just the constant, so the ImageRef builder in index.ts
 * cannot get the units wrong: both arguments are PERCENTAGES of the page and
 * FULL_PAGE_AREA_FRAC is a FRACTION, so each is divided down before they
 * multiply. Written out once here rather than at the call site because the
 * naive `(w * h) / 100 >= FRAC * 100` happens to be equivalent and reads like a
 * mistake, and the next person to touch it would have to redo the algebra.
 *
 * Beside `columnWidthPct` for the same reason it is: the two of them are
 * everything the parser decides ABOUT a kept picture, and both are testable
 * without a pdf.js shim.
 *
 * The two parameters are named for the reader, not for the arithmetic: area is
 * a product, so swapping them is EXACTLY equivalent (IEEE 754 multiplication
 * commutes) and the call site cannot get their order wrong. Said here because
 * a mutation sweep transposing them survives, and that is the mutant being
 * equivalent rather than a test missing — do not add an assertion chasing it.
 */
export function isFullPage(pageWidthPct: number, pageHeightPct: number): boolean {
  return (pageWidthPct / 100) * (pageHeightPct / 100) >= FULL_PAGE_AREA_FRAC;
}

/**
 * The share of an 8.5" sheet a standard script's TEXT COLUMN occupies.
 *
 * 1.5" of left margin and 1.0" of right leaves 6.0", which is 70.6% of the
 * page. Not measured from any PDF: it is the format's own margin spec, the
 * same one docs/screenplay-format-reference.md states, and a script that
 * departs from it is departing from the format rather than from a threshold
 * this file guessed.
 */
const TEXT_COLUMN_FRAC = 0.706;

/**
 * The floor `columnWidthPct` clamps to, and it is NOT reachable through
 * `keepArtwork`: MIN_WIDTH_FRAC already refuses anything under 10% of the
 * page, and 10 / 0.706 is 14. It exists for a caller that has not been through
 * the filters — and because a floor is where the NaN guard below has somewhere
 * to land. Read the 5 itself in types.ts (`ImageRef.widthPct`), which states
 * both ends of the range for the renderers.
 */
const MIN_COLUMN_PCT = 5;

/**
 * Rendered width as a share of the script's TEXT COLUMN, not of the page.
 *
 * The e-book has no print margins to speak of, so the column is the honest
 * reference: an image drawn as wide as the dialogue around it should render as
 * wide as the dialogue around it. Measuring against the page instead would set
 * every picture about 30% too narrow.
 *
 * Here rather than in images.ts because remapping a page fraction to a column
 * fraction is a judgement about how a script is SET — the same kind of thing as
 * every threshold above — and not a fact pdf.js reported. It is the reason this
 * file is imported by index.ts as well as by images.ts.
 *
 * The upper clamp is ordinary rather than defensive: anything wider than the
 * column (a full-bleed title card is ~121% of it) is asked to fill the column
 * and no more. The lower one is the unreachable case described above.
 */
export function columnWidthPct(pageWidthPct: number): number {
  const ofColumn = pageWidthPct / TEXT_COLUMN_FRAC;
  // `!(x > floor)` and not `x < floor`, the house form in this file: NaN fails
  // every comparison, so the natural clamp returns NaN and the width in the
  // `[[img: ...]]` note becomes `NaN`, which is an invalid CSS length far from
  // whatever produced it. Unreachable through `keepArtwork` — its own safe-form
  // area floor already refused any box whose arithmetic was not numbers — and
  // written this way so that stays a property of THIS function too.
  if (!(ofColumn > MIN_COLUMN_PCT)) return MIN_COLUMN_PCT;
  return Math.round(Math.min(100, ofColumn));
}

/**
 * Ceiling on one document, because nothing else in the pipeline bounds the
 * size of the book.
 *
 * The arithmetic, so a maintainer facing a 90-picture lookbook can see what
 * they are trading, against the 50 MB limit on the Send-to-Kindle EMAIL route
 * (docs/device-map.md, T5 — the tightest of the delivery routes; the web
 * upload allows 200 MB).
 *
 * 2026-08-07, MEASURED, and it is worse than the August design's estimate: sixty
 * pictures out of a real 30-page pitch deck came to 16.3 MB, not the ~6 MB a
 * 100 KB panel predicts. A panel averages nearer 270 KB once it is a
 * full-bleed plate rather than a storyboard thumbnail, which is exactly what a
 * lookbook is made of. So the headroom against the email route is about 3x,
 * not the order of magnitude the estimate suggested, and the binding
 * constraint arrives near 180 pictures rather than 500.
 *
 * Sixty still stands, and the measurement is the argument FOR it rather than
 * against: this is the conservative end of a range whose upper bound is much
 * closer than anyone thought. Raise it only with a measurement, not with the
 * estimate. The reasons not to are conversion time and the picture folder,
 * plus now the email route actually being in sight.
 *
 * How often it bites, 2026-08-12: one document in seven, and that one offered
 * 126 pictures that had cleared every filter, so 66 were dropped. The only
 * real SCREENPLAY carrying art in the corpus offered 22. The cap is an
 * exception in the shape of document this feature is for, which is why its
 * warning can afford to be loud.
 */
export const MAX_IMAGES = 60;

export interface ImageCandidate {
  pageNum: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  /** SOURCE pixels as the paint operator declared them, never the encoded
   * PNG's: this is the picture's own resolution, which is what MIN_PIXELS asks
   * about. The two names sit one letter apart downstream, so the distinction
   * is spelled out here as well as in images.ts. */
  srcPxWidth: number;
  srcPxHeight: number;
  /** hash of the ENCODED bytes: the same logo is often a different objId per
   * page, and identical pixels are the fact we actually care about */
  hash: string;
  /** Text lines on this candidate's PAGE, for the background rule. Named for
   * the page and not for the image because that is what it is: a page property
   * carried per candidate, so every candidate on a sheet repeats one number.
   * Redundant rather than wrong — the collector fills a page's candidates in a
   * single loop from a single count, so they cannot disagree, and the
   * alternative is a second parameter and a lookup for a rule that fires on
   * roughly one image per corpus. */
  pageTextLines: number;
}

/**
 * Which rule refused a candidate. Ordered as the filter applies them, so the
 * first one that fires is the one reported: a 20x20pt image at 16x16 pixels is
 * `pixels`, not `area`, because that is the cheaper question and it is asked
 * first.
 */
export type DropRule =
  | 'pixels'
  | 'width'
  | 'height'
  | 'area'
  | 'furniture'
  | 'background';

const noDrops = (): Record<DropRule, number> => ({
  pixels: 0,
  width: 0,
  height: 0,
  area: 0,
  furniture: 0,
  background: 0,
});

/** What survived, how many the CAP refused, and what killed the rest. */
export interface ArtworkVerdict<T> {
  kept: T[];
  /**
   * Images that cleared every filter and lost to MAX_IMAGES alone.
   *
   * Deliberately NOT a count of everything dropped — that is `droppedBy`. The
   * filters dropping letterhead is the normal case, nearly every real script
   * has some, and a caller warning about it would warn on every conversion,
   * which is how a warning stops being read. The cap is the exceptional one,
   * and the August design asked for a warning, as today's spec does ("keeping
   * the first 60 and warning"), precisely because silent truncation
   * reads as "we carried everything" when we did not. `kept.length + overCap`
   * is what a caller needs to say "this script has 73 pictures and Screepub
   * carried 60".
   */
  overCap: number;
  /**
   * How many candidates each rule refused, keyed by the rule that fired first.
   *
   * This is the retune instrument, and it exists because a corpus diff can
   * only ever show that a picture VANISHED. "Which threshold do I move" is the
   * whole content of a retune, and without attribution answering it means
   * bisecting the constants by hand — or building a document that isolates one
   * rule, which is a luxury only a fixture has. A histogram over a real corpus
   * says directly whether the pixel floor is doing the work or the furniture
   * rule is.
   *
   * Counts CANDIDATES, which means PLACEMENTS and not sheets and not distinct
   * pictures: a logo on forty sheets is forty, and a frame painted twice on
   * each of four sheets is eight. That is the right unit for "how much of this
   * document did each rule eat", and it means the six counts plus `kept.length`
   * plus `overCap` add up to the number of candidates handed in.
   *
   * Say which unit whenever one of these numbers is written down. Every note
   * about the furniture rule is phrased in SHEETS, because that is what the
   * rule tests; this is in placements; and quoting the two side by side is how
   * a correct pair of measurements came to read as a contradiction (see
   * FURNITURE_PAGES above).
   */
  droppedBy: Record<DropRule, number>;
}

/**
 * Apply every filter, and take the first MAX_IMAGES of what is left.
 *
 * `pageCount` is the DOCUMENT's, not the run's: the furniture share has to be
 * measured against the whole script, or a five-page excerpt reads its own
 * header as art.
 *
 * `candidates` is expected in document order, and the cap trusts that: it
 * keeps a PREFIX. Preferring larger images instead would be wrong twice over —
 * the cap exists to bound the size of the book, so keeping the biggest is
 * backwards on the cap's own rationale, and a size-ranked subset illustrates
 * page 3 and page 40 and nothing between, which reads to a reader as damage
 * rather than as a limit. A prefix degrades the way a reader can understand:
 * the art stops.
 *
 * ONE page geometry for a whole document, and that is an ASSUMPTION rather
 * than a fact. The collector reads a viewport per page but hands this function
 * a single width and height — the document's MOST COMMON, as extract.ts is
 * wired (`dominantPageSize`). On a uniform script, which is nearly all of
 * them, that is exact. On a production draft carrying a LANDSCAPE storyboard
 * insert — precisely the document this feature exists for — that insert is
 * measured against a portrait page: its `wFrac` goes above 1.0, its area
 * fraction inflates past what it really covers, and the background rule can
 * then fire on a legitimate full-bleed landscape picture sitting on a
 * text-heavy sheet. Taking the majority geometry rather than any one page's
 * bounds that error to the odd sheet instead of spreading it across the
 * document, and is as far as a single width and height can go; the real fix is
 * page dimensions on `ImageCandidate` rather than parameters here, which is a
 * change to the collector as much as to this function. Named and not fixed.
 */
export function keepArtwork<T extends ImageCandidate>(
  candidates: T[],
  pageWidth: number,
  pageHeight: number,
  pageCount: number,
): ArtworkVerdict<T> {
  // Not reachable through extract.ts, which takes both from a page's own
  // viewport at scale 1. One line so that stays true for the next caller:
  // every rule below divides by these, and at zero the fractions come out
  // Infinity, which clears every FLOOR — the one way a candidate can pass a
  // size test by being unmeasurable rather than by being big. Attributed to no
  // rule, because no rule ran: this is a refusal to judge, not a judgement.
  if (!(pageWidth > 0) || !(pageHeight > 0)) {
    return { kept: [], overCap: 0, droppedBy: noDrops() };
  }

  // Distinct pages per identical picture, tallied over EVERY candidate rather
  // than only the ones that clear the size floors. Whether something is
  // furniture is a question about the picture and the document, not about one
  // placement's size: letterhead scaled down on the title page is still
  // letterhead. The August design's wording ("does not appear on 3 or more
  // pages") had no size qualifier in it, and neither does today's spec's
  // ("the same picture on 3 or more pages").
  const pagesByHash = new Map<string, Set<number>>();
  for (const c of candidates) {
    let seen = pagesByHash.get(c.hash);
    if (!seen) pagesByHash.set(c.hash, (seen = new Set<number>()));
    seen.add(c.pageNum);
  }

  /**
   * The rule that refuses this candidate, or null to keep it.
   *
   * Flat early returns rather than a predicate per rule, deliberately: half
   * the comments in here are about ORDER — which check is cheap, which one
   * must come before which — and pulling `furniture()` and `background()` out
   * into named predicates would strand every one of them at a call site with
   * nothing to attach to.
   */
  const reject = (c: T): DropRule | null => {
    // Every threshold here is written `!(value >= floor)` rather than
    // `value < floor`, and the difference is NaN: `NaN < 0.10` is false, so
    // the natural form KEEPS a candidate whose numbers are not numbers, and
    // the width that reaches the .fountain note is then `NaN`. These are
    // arithmetic over a CTM this module does not own.
    //
    // Where the safe form actually bites is not symmetric, and it is worth
    // being exact because registry #19 gets written from this file. The PIXEL
    // floor is alone: nothing else reads srcPx, so naive there means a NaN
    // reaches the encoder's dimensions unopposed. The three BOX floors share
    // one backstop — `areaFrac` is `wFrac * hFrac`, so a NaN in either linear
    // fraction poisons the area too, and the safe AREA check by itself catches
    // every NaN box. So the linear floors are safe-form for symmetry and for
    // surviving a reorder, not because either is individually load-bearing
    // today. All of them, or that reasoning has to be redone by whoever next
    // moves a line. (Mutation-checked both directions: all three naive keeps a
    // NaN box, any one of the three left safe refuses it.)
    //
    // What none of this covers: an OVERSIZED box clears every floor rather
    // than failing it, and nothing here has an upper bound to catch it,
    // because a legitimate picture may overhang the sheet and the bound is
    // therefore not a one-liner.
    //
    // Reachable on real files, not only on a degenerate CTM: the 2026-08-12
    // corpus pass found a pitch deck painting a real still inside a box 33.6x
    // the page's width, so its area fraction came out at 3357% of the sheet.
    // Nothing bad followed, and that is worth being precise about rather than
    // lucky about — the picture was art and keeping it was right, and
    // `columnWidthPct`'s upper clamp meant the book set it at the text column
    // like any other. So the missing bound costs a WRONG-BY-ANY-MEASURE box
    // its refusal, and costs a correct one nothing, which is the measured
    // reason there is no upper bound.
    if (!(c.srcPxWidth >= MIN_PIXELS) || !(c.srcPxHeight >= MIN_PIXELS)) return 'pixels';

    const wFrac = (c.x1 - c.x0) / pageWidth;
    const hFrac = (c.y1 - c.y0) / pageHeight;
    // As the product of the two linear fractions, not (w * h) / pageArea. The
    // same quantity, and it keeps all three floors comparable against the
    // fractions they are stated in.
    const areaFrac = wFrac * hFrac;
    // Both linear floors come before the area one, so an inverted box cannot
    // multiply two negative edges back into a positive area and clear it.
    if (!(wFrac >= MIN_WIDTH_FRAC)) return 'width';
    if (!(hFrac >= MIN_HEIGHT_FRAC)) return 'height';
    if (!(areaFrac >= MIN_AREA_FRAC)) return 'area';

    const pages = pagesByHash.get(c.hash)?.size ?? 1;
    if (pages >= FURNITURE_PAGES) return 'furniture';
    // Multiplied out rather than `pages / pageCount >= FURNITURE_FRAC`, which
    // needs a `Math.max(pageCount, 1)` to look safe and does not need it to BE
    // safe: `pages / 0` is Infinity, and Infinity clears the share exactly as
    // dividing by one would. A guard that protects nothing reads as if it does.
    if (pages >= FURNITURE_SHARE_MIN_PAGES && pages >= pageCount * FURNITURE_FRAC) {
      return 'furniture';
    }

    if (areaFrac >= BACKGROUND_AREA_FRAC && c.pageTextLines >= BACKGROUND_TEXT_LINES) {
      return 'background';
    }

    return null;
  };

  const droppedBy = noDrops();
  const survivors = candidates.filter((c) => {
    const rule = reject(c);
    if (rule !== null) droppedBy[rule]++;
    return rule === null;
  });

  const kept = survivors.slice(0, MAX_IMAGES);
  return { kept, overCap: survivors.length - kept.length, droppedBy };
}
