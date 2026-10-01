// Adapted from an earlier table-read parser by the same author, reworked for
// headless Bun/Node: modern pdf.js build + DOM shims, no browser worker.
import './pdfjs-shims';
import { isCueText } from './cue';
import type { FamilyBucket, FontRun, RawImage, RawLine, SizeStep } from './types';
import { getDocument, OPS } from 'pdfjs-dist/build/pdf.mjs';
// ONE edge for the whole image feature. images.ts owns the collection and the
// join to artwork.ts's judgements; this module reads pages and lines, hands
// over what a page produced, and asks for the pile back at the end.
import {
  collectPageImages,
  finishImages,
  newImageRun,
  type EncodedCandidate,
  type ImageStats,
  type OpList,
} from './images';
// pdf.js loads its JPEG2000, JBIG2 and CCITT decoders from a directory at
// runtime rather than from its own bundle, and without one every image in
// those formats fails to decode. See pdfjs-wasm.ts.
import { pdfjsWasmDir } from './pdfjs-wasm';

export type { ImageStats } from './images';

/// Called once per page as extraction advances. `pages` is the number of
/// pages this run will actually visit (so it honors `maxPages`), which makes
/// `page / pages` a usable fraction from the very first tick.
export type PageProgress = (page: number, pages: number) => void;

/** What the pass over the pages knows as soon as it has read them: the text,
 * and nothing that needed a picture's pixels. */
export interface ExtractedText {
  /** Each line's text, indent, y position and page number. Indent is a
   * percentage of page width (0-100), which is what makes the parser work
   * across PDF generators and page sizes. */
  lines: RawLine[];
  /** The DOCUMENT's page count, which is not the number of pages this run
   * visited when `maxPages` is set. */
  pageCount: number;
  /** How many text items `repairPhantomSpaces` rewrote. */
  spacingRepairs: number;
}

/** What decoding the pictures produces. */
export interface ExtractedImages {
  /** The pictures that cleared artwork.ts, in reading order. */
  images: RawImage[];
  /** What became of the pictures, including the ones that did not make it. */
  imageStats: ImageStats;
}

/** Everything one document load produces. Named because a caller that wants
 * to talk about it should not have to spell `Awaited<ReturnType<...>>`. */
export interface ExtractResult extends ExtractedText, ExtractedImages {}

/** The text now, and the pictures only when asked for. See extractText. */
export interface PendingExtract extends ExtractedText {
  /**
   * Resolve, encode and judge the pictures the page pass located.
   *
   * Asked for at most once in effect: a second call hands back the first
   * call's promise, so a document still gets exactly ONE image run (one
   * budget, one encode cache, one failure count), however it is called.
   */
  decodeImages(): Promise<ExtractedImages>;
}

/**
 * Read a PDF in a single document load: its lines, its true page count, and
 * its pictures (see ExtractResult). extractText followed straight away by
 * its `decodeImages`, for a caller with no reason to stop in between.
 *
 * NOTE: pdf.js transfers the underlying ArrayBuffer to its worker, so the
 * caller's `pdfBytes` is detached afterwards. Pass a copy if you still need it.
 */
export async function extractDocument(
  pdfBytes: Uint8Array,
  maxPages?: number,
  onProgress?: PageProgress,
): Promise<ExtractResult> {
  const { decodeImages, ...text } = await extractText(pdfBytes, maxPages, onProgress);
  return { ...text, ...(await decodeImages()) };
}

/**
 * Read a PDF's text in a single document load, and leave its pictures for
 * later: located on the way past, decoded only if `decodeImages` is called.
 *
 * The split is for convert.ts, whose two refusals are both decided by the
 * text alone, so a document either one turns away never pays for its
 * pictures. On the local corpus (2026-10-01, best of two) that is the
 * difference between 0.09s and 3.7s to refuse a scan, and between 1.5s and
 * 16.7s to refuse a 126-picture document that is not a screenplay, most of
 * the 20s IMAGE_BUDGET_MS spent on a result nobody would see.
 *
 * Same NOTE as extractDocument: `pdfBytes` is detached afterwards.
 */
export async function extractText(
  pdfBytes: Uint8Array,
  maxPages?: number,
  onProgress?: PageProgress,
): Promise<PendingExtract> {
  // Asked BEFORE the document, and asked even for a script with no pictures in
  // it, because `wasmUrl` is a load-time option: pdf.js hands it to the worker
  // inside `docParams` and there is no later door. Null on a temp directory we
  // could not write, which is not fatal — the document still converts, minus
  // any JPEG2000 or JBIG2 picture in it — so it travels out on the stats
  // instead, where the warning can say why the pictures are missing.
  //
  // Asked PER DOCUMENT, never held in a constant here, for two reasons. The
  // folder can go away between documents (removePdfjsWasmDir, which
  // tests/pdfjs-wasm.test.ts calls mid-run) and only pdfjsWasmDir() knows to
  // make a fresh one. And every getDocument re-sets the worker's one
  // process-wide copy of this string (`WasmImage.setOptions`), while a decoder
  // that FAILED to load stays failed (its memoized module promise) until a
  // document cleanup clears it, which this engine never runs. So one open
  // without the folder, when the folder exists, would cost later documents
  // their JPEG2000 pictures as well.
  const wasmUrl = pdfjsWasmDir();
  const pdf = await getDocument({ data: pdfBytes, wasmUrl: wasmUrl ?? undefined }).promise;
  const allLines: RawLine[] = [];
  // Counted, not silent. repairPhantomSpaces edits an author's words, so the
  // number rides out with the result and the app can say so. A count that
  // suddenly jumps on a script is how anyone would notice it misfiring.
  let spacingRepairs = 0;
  const sizes: { width: number; height: number }[] = [];
  // Every page that has an operator list, kept for `decodeImages`: the page
  // (whose stores the pixels arrive in), its list (where the placements are),
  // and the two facts about it that only this pass knows. Nothing new is held
  // alive by this: pdf.js keeps every page proxy, and every operator list on
  // it, until a cleanup this engine never runs.
  const pending: {
    page: Parameters<typeof collectPageImages>[0];
    ops: OpList;
    info: { pageNum: number; textLines: number };
  }[] = [];

  const lastPage = maxPages ? Math.min(pdf.numPages, maxPages) : pdf.numPages;
  for (let pageNum = 1; pageNum <= lastPage; pageNum++) {
    const page = await pdf.getPage(pageNum);
    const viewport = page.getViewport({ scale: 1.0 });
    sizes.push({ width: viewport.width, height: viewport.height });
    const textContent = await page.getTextContent();
    const ops = await operatorList(page);
    stampFontStyles(page, textContent.items);
    if (ops) stampUnderlines(textContent.items, ops, viewport.width);
    // Before grouping, because every stage downstream reads `str` and a
    // phantom space fragments a word into two tokens for all of them.
    spacingRepairs += repairPhantomSpaces(textContent.items, ops);

    const pageLines = groupItemsIntoLines(textContent.items, viewport.width, pageNum);
    allLines.push(...pageLines);
    // The same operator list the text pass just read, kept for the pictures.
    // A null list (a page pdf.js could not build one for) gives no pictures,
    // just as the text pass above gets unresolved fonts and no underlines or
    // repairs from it. The text-line count goes with it because it is a fact
    // about THIS page, and it is what tells a designed title card apart from a
    // watermark printed behind a full page of script.
    if (ops) pending.push({ page, ops, info: { pageNum, textLines: pageLines.length } });
    // After the work, not before: a tick means "page N is done", so the
    // fraction never claims progress the parser has not actually made.
    onProgress?.(pageNum, lastPage);
  }

  // Dominant font is a DOCUMENT fact, so this runs once, after every page:
  // a chyron page set entirely in Helvetica is still a deviation from a
  // Courier script (registry #18).
  stampLineFmt(allLines);

  // AFTER every page rather than inside the loop, and only on request, which
  // is the point of this function. Safe to defer for one reason, and it is a
  // standing rule rather than a convenience: pdf.js drops an incoming image
  // once a page's operator-list intent is gone, so anything that frees a page
  // (`page.cleanup()`) before its pixels are asked for buys silence rather
  // than an error (see collectPageImages for what that costs, measured).
  // Nothing in this engine frees pages. Whoever adds it must add it after the
  // promise below has settled, never in the page loop above.
  const decode = async (): Promise<ExtractedImages> => {
    // ONE budget for the whole document, threaded through every page: the
    // per-image timeout alone would let a lookbook of damaged XObjects stall
    // for minutes. The same object also caches encoded pixels, so a logo on
    // ninety sheets is deflated twice rather than ninety times — twice because
    // pdf.js renames a picture when it promotes it on the second sheet it
    // appears on. See ImageRun.encoded.
    const run = newImageRun();
    const candidates: EncodedCandidate[] = [];
    // In page order, as the loop above used to, so the budget is spent and
    // the encode cache is filled in exactly the order it always was.
    for (const p of pending) {
      candidates.push(...(await collectPageImages(p.page, p.ops, p.info, run)));
    }
    // Which pictures are art is a DOCUMENT fact too, for the same shape of
    // reason as the font pass: repetition is only visible once every sheet has
    // been read. The page count handed over is the whole document's, not this
    // run's, which is what `keepArtwork` asks for.
    const { images, imageStats } = finishImages(candidates, sizes, pdf.numPages, run);
    // Set here rather than inside finishImages, which counts what became of the
    // pictures and knows nothing about how pdf.js was configured. Only when it
    // actually failed: absent means the decoders were in place, so a caller
    // cannot read this as "no JPEG2000 in this document".
    if (!wasmUrl) imageStats.decodersUnavailable = true;
    return { images, imageStats };
  };
  let decoded: Promise<ExtractedImages> | undefined;

  return {
    lines: allLines,
    pageCount: pdf.numPages,
    spacingRepairs,
    decodeImages: () => (decoded ??= decode()),
  };
}

interface TextItem {
  str: string;
  transform: number[];
  fontName?: string;
  italic?: boolean;
  bold?: boolean;
  /** a rule was DRAWN under this item — never readable from font data */
  underline?: boolean;
  /** coarse family bucket from the resolved PostScript name (registry #18) */
  bucket?: FamilyBucket;
  /** actual rendered width from pdf.js — much more accurate than the
   * len*6 estimate (which false-splits names like "Marlowe" → "Mar lowe") */
  width?: number;
}

const endX = (item: TextItem) => item.transform[4] + (item.width ?? item.str.length * 6);

/**
 * Undo word spaces pdf.js invented, using the glyph stream as the truth.
 *
 * pdf.js turns a wide inter-glyph gap into a space in the string it hands
 * us. Usually right; wrong on a PDF whose text has been re-encoded. A Final
 * Draft script re-saved through Quartz carried a uniform +17 tracking
 * adjustment on every glyph pair and, about once a page, one anomalous
 * ~-110 that widened a single gap by roughly 1.5pt. That is a fifth of a
 * character, far short of a space, and pdf.js called it one: "these"
 * arrived as "thes e", 94 times across 70 of 112 pages.
 *
 * Two facts make this repairable rather than guesswork, and both matter:
 *
 * A REAL space is a glyph. It appears in the run as its own entry with a
 * full cell of advance. An invented one is not in the run at all. So a
 * writer who spaced out a word deliberately keeps every space, and we never
 * have to reason about whether they meant it.
 *
 * And SCREENPLAYS ARE MONOSPACED, so the ink says how many characters there
 * really are. Every clean item on a page divides to the same cell width;
 * an item carrying a phantom is a whole cell short of its own string. That
 * is the flag, and it is why this cannot fire on a proportional font: there
 * is no grid, so no item is ever flagged. (Measured on a pitch deck: 87
 * items look short, and every one of them is a false positive. The floor on
 * `width` and the uniqueness check below are what keep that harmless, but
 * the real protection is that this only runs where a script is Courier.)
 *
 * Returns how many items it repaired.
 */
export function repairPhantomSpaces(items: unknown[], ops: OpList | null): number {
  if (!ops) return 0;
  const texts = items as TextItem[];

  // The glyph runs, keyed by their text with ALL whitespace removed, so a
  // run and the item it produced match even though their spacing differs —
  // which is the entire point. A key claimed twice is dropped: picking one
  // would be a coin flip, and a wrong repair is worse than none.
  const byKey = new Map<string, string | null>();
  for (let i = 0; i < ops.fnArray.length; i++) {
    if (ops.fnArray[i] !== OPS.showText) continue;
    // argsArray is ArrayLike<unknown> by design — the op list is a union of
    // every operator's arguments — so showText's own shape is asserted here
    // rather than widening the interface for one caller.
    const args = ops.argsArray[i] as unknown[] | undefined;
    const glyphs = args?.[0];
    if (!Array.isArray(glyphs)) continue;
    // Glyphs only. The numbers between them are positioning, and reading
    // them as anything else is the bug this function exists to undo.
    const text = glyphs
      .map((g) => (typeof g === 'number' ? '' : ((g as { unicode?: string })?.unicode ?? '')))
      .join('');
    if (!text.trim()) continue;
    const key = text.replace(/\s+/gu, '');
    byKey.set(key, byKey.has(key) ? null : text);
  }
  if (byKey.size === 0) return 0;

  // The page's cell width, as the median of width-per-character over lines
  // long enough to be representative. Median, not mean: the damaged items
  // are the outliers we are hunting and must not drag the baseline toward
  // themselves.
  const cells = texts
    .filter((t) => typeof t.width === 'number' && t.width > 0 && t.str.trim().length > 8)
    .map((t) => t.width! / t.str.length)
    .sort((a, b) => a - b);
  if (cells.length < 3) return 0;
  const cell = cells[Math.floor(cells.length / 2)]!;
  if (!(cell > 0)) return 0;

  let repaired = 0;
  for (const t of texts) {
    if (typeof t.width !== 'number' || t.width <= 0) continue;
    // 0.6 of a cell: comfortably past the rounding in a real line (measured
    // spread is under 0.02) and comfortably under the 1.0 a genuine missing
    // character would cost.
    if (t.str.length - t.width / cell < 0.6) continue;
    const truth = byKey.get(t.str.replace(/\s+/gu, ''));
    if (!truth) continue;
    // Compare TRIMMED. A glyph run usually ends with a trailing space that
    // the item does not carry, which makes the two the same length even
    // when the run has one less space in the middle — so a raw comparison
    // reads "no improvement" and skips. That is not hypothetical: it let
    // three of the four known cases through on the first cut of this, and
    // only the one run that happened to have no trailing space was fixed.
    // Leading and trailing space is safe to drop here regardless: indent
    // comes from the transform, and joinLine trims and collapses anyway.
    const fixed = truth.trim();
    // Only ever REMOVE spacing. A repair that added text would mean the two
    // strings disagreed about something other than whitespace, and this
    // function has no business fixing that.
    if (fixed.length >= t.str.trim().length) continue;
    t.str = fixed;
    repaired++;
  }
  return repaired;
}

export function groupItemsIntoLines(
  items: unknown[],
  pageWidth: number,
  pageNum: number
): RawLine[] {
  // Filter to actual text items (not marked content)
  const textItems = items.filter(
    (item): item is TextItem => 'str' in (item as Record<string, unknown>) && 'transform' in (item as Record<string, unknown>)
  );

  if (textItems.length === 0) return [];

  // Group items by Y coordinate, clustering within a small tolerance —
  // baselines drift inside one visual line (one generator prints "(CONT'D)"
  // a point above its cue name) while real line spacing is ~12pt.
  const Y_TOLERANCE = 2;
  const clusters: { y: number; items: TextItem[] }[] = [];
  for (const item of [...textItems].sort((a, b) => b.transform[5] - a.transform[5])) {
    const last = clusters[clusters.length - 1];
    if (last && last.y - item.transform[5] <= Y_TOLERANCE) {
      last.items.push(item);
    } else {
      clusters.push({ y: item.transform[5], items: [item] });
    }
  }

  // Right-margin zone where revision stars live (Final Draft puts them at
  // ~94% of page width). Star-only items there are production markup, not
  // text — left in, a bare "*" line classifies as action and resets the
  // active-character context, fragmenting revised dialogue.
  const revisionMarginX = pageWidth * 0.8;

  const lines: LineItems[] = [];
  for (const { y, items: clusterItems } of clusters) {
    // Drop empty items (marked-content markers) and right-margin revision
    // stars — both would corrupt joining/classification below.
    const lineItems = clusterItems
      .filter((item) => item.str !== '')
      .filter((item) => !(/^\*+$/.test(item.str) && item.transform[4] >= revisionMarginX));
    if (lineItems.length === 0) continue;
    lineItems.sort((a, b) => a.transform[4] - b.transform[4]);
    lines.push({ y: Math.round(y), items: lineItems });
  }

  return deinterleaveDualDialogue(lines, pageWidth, pageNum);
}

interface LineItems {
  y: number;
  items: TextItem[];
}

/**
 * A drawn horizontal rule that might be an underline, in PDF user space —
 * the same space text baselines live in (origin bottom-left).
 */
export interface UnderlineMark {
  x0: number;
  x1: number;
  y: number;
}

// Geometry bounds, calibrated 2026-08-04 against the local generator set.
// Real underlines measured 0.0-0.6pt tall, 36-168pt wide, and sat 1.5-3.0pt
// below their baseline. Every constant has a decoy in the torture fixture.
const MARK_MAX_HEIGHT = 2.5;       // taller is a box or a filled bar
const MARK_MIN_WIDTH = 4;          // narrower is a bullet or a tick
const MARK_MAX_WIDTH_FRAC = 0.85;  // wider is a header/footer rule
const MARK_BELOW = 3.5;            // deepest a mark may sit under a baseline
const MARK_ABOVE = 0.5;            // a strikethrough sits far higher than this
const MARK_MIN_OVERLAP = 0.6;      // fraction of the item a mark must cover

/** m ∘ t — PDF's `cm` post-multiplies onto the current matrix. */
function concat(m: number[], t: number[]): number[] {
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
 * Walk a page's operator list for drawn rules that could be underlines.
 *
 * pdf.js folds all path painting into `constructPath`, whose args are
 * [paintOp, packedPathData, minMax]. The minMax bounding box alone separates
 * an underline (flat and short) from a box (tall) or a page rule (wide), so
 * the packed path data is never decoded. If a generator ever batches several
 * underlines into one path with several subpaths, decoding that array into
 * per-subpath bboxes is the refinement — not built until a real script needs it.
 *
 * Best-effort by design, like `stampFontStyles`: any failure yields no marks,
 * which is exactly today's behavior.
 */
export function collectUnderlineMarks(opList: OpList, pageWidth: number): UnderlineMark[] {
  const marks: UnderlineMark[] = [];
  try {
    const { fnArray, argsArray } = opList;
    let m = [1, 0, 0, 1, 0, 0];
    const stack: number[][] = [];

    for (let i = 0; i < fnArray.length; i++) {
      const fn = fnArray[i];

      if (fn === OPS.save) {
        stack.push(m);
        continue;
      }
      if (fn === OPS.restore) {
        m = stack.pop() ?? [1, 0, 0, 1, 0, 0];
        continue;
      }
      if (fn === OPS.transform) {
        m = concat(m, argsArray[i] as number[]);
        continue;
      }
      if (fn !== OPS.constructPath) continue;

      const args = argsArray[i] as [number, unknown, ArrayLike<number>] | null;
      // endPath is the clip-path spelling (`W n`): it builds geometry and
      // paints nothing. Every full-page box the browser-print generator draws
      // is one. Every OTHER paint op is accepted — real underlines are
      // STROKED, not filled, so requiring a fill would detect nothing real.
      if (!args || args[0] === OPS.endPath) continue;
      const bb = args[2];
      if (!bb || bb.length < 4) continue;

      // Skew or rotation: the bbox stops describing an axis-aligned rule, and
      // screenplays do not underline on a slant. Skip rather than guess.
      if (Math.abs(m[1]) > 1e-6 || Math.abs(m[2]) > 1e-6) continue;

      // BOTH corners, then min/max. Final Draft draws its underlines under a
      // y-flip ([1,0,0,-1,0,pageHeight]); transforming one corner, or treating
      // a flip as "rotated, skip it", puts the mark hundreds of points off and
      // no baseline ever matches.
      const ax = m[0] * bb[0] + m[2] * bb[1] + m[4];
      const ay = m[1] * bb[0] + m[3] * bb[1] + m[5];
      const bx = m[0] * bb[2] + m[2] * bb[3] + m[4];
      const by = m[1] * bb[2] + m[3] * bb[3] + m[5];
      const x0 = Math.min(ax, bx);
      const x1 = Math.max(ax, bx);
      const y0 = Math.min(ay, by);
      const y1 = Math.max(ay, by);

      const width = x1 - x0;
      if (y1 - y0 > MARK_MAX_HEIGHT) continue;
      if (width < MARK_MIN_WIDTH) continue;
      if (width >= pageWidth * MARK_MAX_WIDTH_FRAC) continue;

      marks.push({ x0, x1, y: (y0 + y1) / 2 });
    }
  } catch {
    return [];
  }
  return marks;
}

/**
 * True when `mark` underlines the text item spanning [x0, x1) on `baseline`.
 * Pure geometry, so it is testable without a PDF.
 *
 * The band excludes a strikethrough (which sits mid x-height, ABOVE the
 * baseline) and the row below's own underline (12pt away at screenplay
 * spacing). The overlap floor keeps a rule under part of a run from marking
 * the whole run: item granularity means the alternative to dropping it is
 * wrapping characters that are not underlined.
 */
export function markUnderlinesItem(
  mark: UnderlineMark,
  x0: number,
  x1: number,
  baseline: number,
): boolean {
  if (mark.y > baseline + MARK_ABOVE) return false;
  if (mark.y < baseline - MARK_BELOW) return false;
  const width = x1 - x0;
  if (width <= 0) return false;
  const overlap = Math.min(x1, mark.x1) - Math.max(x0, mark.x0);
  return overlap / width >= MARK_MIN_OVERLAP;
}

/**
 * PostScript base name → coarse family bucket.
 *
 * Matched against the name lowercased with its subset prefix ("AAAAAB+") and
 * every non-alphanumeric stripped, so "Letter Gothic" and "LetterGothic" both
 * read as "lettergothic". Two traps this shape avoids:
 *   - bare "gothic" would swallow Century Gothic, which is a sans;
 *   - bare "roman" would swallow Helvetica-Roman and AvenirNext-Roman, where
 *     "-Roman" is the REGULAR weight, not the family. Times matches "times".
 * Weight and slope tokens (bold, black, heavy, italic, oblique) match nothing
 * here on purpose: they are style, and style is registry 9d's business.
 */
const FAMILY_PATTERNS: [RegExp, FamilyBucket][] = [
  [/courier|mono|lettergothic|prestige/, 'mono'],
  [/times|georgia|garamond|palatino|caslon|baskerville|minion/, 'serif'],
  [/script|hand|brush|comic/, 'cursive'],
];

export function familyBucket(name: string): FamilyBucket | undefined {
  const n = String(name ?? '')
    .replace(/^[A-Z]{6}\+/, '')
    .toLowerCase()
    .replace(/[^a-z0-9]/g, '');
  if (!n) return undefined;
  for (const [re, bucket] of FAMILY_PATTERNS) if (re.test(n)) return bucket;
  return 'sans';
}

// Deviation thresholds (registry #18). A step needs BOTH a ratio outside the
// band and >= 1.5pt of absolute change, so float jitter in a text matrix can
// never mint one.
const SIZE_SMALLER_RATIO = 0.85;
const SIZE_BIGGER_RATIO = 1.15;
const SIZE_BIGGEST_RATIO = 1.4;
const SIZE_MIN_DELTA = 1.5;
/** Share of a line's resolved characters that must agree on one deviation. */
const FMT_AGREEMENT = 0.8;

function sizeStep(size: number, dominant: number): SizeStep | undefined {
  if (Math.abs(size - dominant) < SIZE_MIN_DELTA) return undefined;
  const ratio = size / dominant;
  if (ratio <= SIZE_SMALLER_RATIO) return '-1';
  if (ratio > SIZE_BIGGEST_RATIO) return '+2';
  if (ratio >= SIZE_BIGGER_RATIO) return '+1';
  return undefined;
}

/** The key with the largest tally, or undefined for an empty map. */
function argmax<T>(tally: Map<T, number>): T | undefined {
  let best: T | undefined;
  let bestN = -1;
  for (const [k, n] of tally) {
    if (n > bestN) {
      best = k;
      bestN = n;
    }
  }
  return best;
}

/**
 * Stamp `fmt` on every line that deviates from the document's dominant font.
 *
 * Runs over the WHOLE document, after every page, because "dominant" is a
 * document-level fact: a chyron page set entirely in Helvetica is still a
 * deviation from a Courier script. Weighted by character count so a page of
 * one-word sans lines cannot outvote the body.
 *
 * A uniform Courier-12 screenplay produces zero fmt anywhere, by construction:
 * every run agrees with the dominant, so neither tally below is ever non-empty.
 */
export function stampLineFmt(lines: RawLine[]): void {
  const docFamily = new Map<FamilyBucket, number>();
  const docSize = new Map<number, number>();
  for (const line of lines) {
    for (const run of line.fonts ?? []) {
      docFamily.set(run.bucket, (docFamily.get(run.bucket) ?? 0) + run.chars);
      docSize.set(run.size, (docSize.get(run.size) ?? 0) + run.chars);
    }
  }
  const domFamily = argmax(docFamily);
  const domSize = argmax(docSize);
  if (domFamily === undefined || domSize === undefined || domSize <= 0) return;

  for (const line of lines) {
    const runs = line.fonts;
    if (!runs || runs.length === 0) continue;
    const total = runs.reduce((n, r) => n + r.chars, 0);
    if (total === 0) continue;

    const byFamily = new Map<FamilyBucket, number>();
    const byStep = new Map<SizeStep, number>();
    for (const run of runs) {
      if (run.bucket !== domFamily) {
        byFamily.set(run.bucket, (byFamily.get(run.bucket) ?? 0) + run.chars);
      }
      const step = sizeStep(run.size, domSize);
      if (step) byStep.set(step, (byStep.get(step) ?? 0) + run.chars);
    }
    const agreed = <T>(tally: Map<T, number>): T | undefined => {
      for (const [k, n] of tally) if (n / total >= FMT_AGREEMENT) return k;
      return undefined;
    };
    const family = agreed(byFamily);
    const size = agreed(byStep);
    if (family || size) line.fmt = { family, size };
  }
}

/**
 * getOperatorList, best-effort. This call is ALSO what forces font resolution
 * into `page.commonObjs` — getTextContent alone does not load fonts — so
 * `stampFontStyles` depends on it having run first, and both passes share the
 * single call rather than paying for it twice.
 */
async function operatorList(page: {
  getOperatorList(): Promise<unknown>;
}): Promise<OpList | null> {
  try {
    return (await page.getOperatorList()) as OpList;
  } catch {
    return null;
  }
}

/**
 * Mark each item bold/italic from its font's PostScript name (e.g.
 * "CourierPrime-Italic"). Best-effort: an unresolved font leaves its items
 * plain, which is also what happens when `operatorList` above returned null.
 */
function stampFontStyles(
  page: { commonObjs: { get(id: string): unknown } },
  items: unknown[],
): void {
  const byFont = new Map<string, { italic: boolean; bold: boolean; bucket?: FamilyBucket }>();
  for (const raw of items) {
    const item = raw as TextItem;
    if (!item.fontName || byFont.has(item.fontName)) continue;
    let flags: { italic: boolean; bold: boolean; bucket?: FamilyBucket } = {
      italic: false,
      bold: false,
    };
    try {
      const font = page.commonObjs.get(item.fontName) as { name?: string } | null;
      const name = String(font?.name ?? '');
      flags = {
        italic: /italic|oblique/i.test(name),
        bold: /bold|black|heavy/i.test(name),
        bucket: familyBucket(name),
      };
    } catch {
      // unresolved font — leave plain and unbucketed
    }
    byFont.set(item.fontName, flags);
  }
  for (const raw of items) {
    const item = raw as TextItem;
    const flags = item.fontName ? byFont.get(item.fontName) : undefined;
    if (flags) {
      item.italic = flags.italic;
      item.bold = flags.bold;
      item.bucket = flags.bucket;
    }
  }
}

/**
 * Per-run font tallies for a line's items, merging equal neighbours.
 *
 * Size comes from the text matrix's vertical scale, which is `transform[3]`
 * for the unrotated text screenplays are made of; the absolute value keeps a
 * flipped matrix from reporting a negative size. Items whose font never
 * resolved are skipped entirely, so the shares in `stampLineFmt` are over
 * resolved characters rather than being diluted by unknowns.
 */
function fontRuns(items: TextItem[]): FontRun[] | undefined {
  const out: FontRun[] = [];
  for (const item of items) {
    const chars = item.str.trim().length;
    if (!chars || !item.bucket) continue;
    const size = Math.round(Math.abs(item.transform[3]) * 10) / 10;
    const last = out[out.length - 1];
    if (last && last.bucket === item.bucket && last.size === size) last.chars += chars;
    else out.push({ bucket: item.bucket, size, chars });
  }
  return out.length > 0 ? out : undefined;
}

/**
 * Mark each item underlined when a drawn rule sits in the band below its
 * baseline. Underline is DRAWN, not selected, so unlike bold/italic it can
 * never be read from font data (registry 9d).
 */
function stampUnderlines(items: unknown[], opList: OpList, pageWidth: number): void {
  const marks = collectUnderlineMarks(opList, pageWidth);
  if (marks.length === 0) return;
  for (const raw of items) {
    const item = raw as TextItem;
    if (!item.str || !item.str.trim() || !item.transform) continue;
    const x0 = item.transform[4];
    const baseline = item.transform[5];
    if (marks.some((m) => markUnderlinesItem(m, x0, endX(item), baseline))) {
      item.underline = true;
    }
  }
}

/** Join sorted items into text: spaces at >5pt gaps, exact-overlap dedup
 * (Final Draft double-prints (MORE)/(CONT'D) furniture). */
function joinItems(items: TextItem[]): string {
  let text = '';
  let prevEndX = -1;
  let prevX = Number.NaN;
  let prevStr = '';

  for (const item of items) {
    const x = item.transform[4];
    if (item.str === prevStr && Math.abs(x - prevX) < 2) {
      continue;
    }
    if (prevEndX >= 0 && x - prevEndX > 5) {
      text += ' ';
    }
    text += item.str;
    prevEndX = endX(item);
    prevX = x;
    prevStr = item.str;
  }
  // Items carry their own padding (italic spans especially) — collapse
  // runs so the fountain matches what HTML whitespace-collapsing renders.
  return text.replace(/ {2,}/g, ' ').trim();
}

/**
 * Fountain emphasis as OPEN/CLOSE pairs. A mixed mark is not a palindrome —
 * `**_x_**` closes in the mirror order — so one string per style no longer
 * works. Canonical nesting puts the underscore innermost and the stars
 * outside, which is the order both renderers' regexes already unwrap (triple
 * stars, then double, then single, then underscore).
 *
 * Keys are built as b→i→u, so every subset of {b,i,u} appears exactly once.
 */
const EMPHASIS_MARK: Record<string, [string, string]> = {
  b: ['**', '**'],
  i: ['*', '*'],
  u: ['_', '_'],
  bi: ['***', '***'],
  bu: ['**_', '_**'],
  iu: ['*_', '_*'],
  biu: ['***_', '_***'],
};

/**
 * Join a line producing both plain text and (when fonts vary) a styled
 * variant with fountain emphasis markers. Punctuation-only items never
 * carry style — a lone italic comma must not become "*,*".
 */
function joinLine(items: TextItem[]): { text: string; styled?: string } {
  interface Run { str: string; gapBefore: boolean; style: string }
  const runs: Run[] = [];
  let prevEndX = -1;
  let prevX = Number.NaN;
  let prevStr = '';
  for (const item of items) {
    const x = item.transform[4];
    if (item.str === prevStr && Math.abs(x - prevX) < 2) continue;
    const hasWord = /[\p{L}\p{N}]{2}/u.test(item.str);
    const style = hasWord
      ? `${item.bold ? 'b' : ''}${item.italic ? 'i' : ''}${item.underline ? 'u' : ''}`
      : '';
    runs.push({ str: item.str, gapBefore: prevEndX >= 0 && x - prevEndX > 5, style });
    prevEndX = endX(item);
    prevX = x;
    prevStr = item.str;
  }

  let plain = '';
  for (const r of runs) plain += (r.gapBefore && plain ? ' ' : '') + r.str;
  plain = plain.replace(/ {2,}/g, ' ').trim();
  if (!runs.some((r) => r.style)) return { text: plain };

  const groups: { style: string; text: string }[] = [];
  for (const r of runs) {
    const piece = (r.gapBefore ? ' ' : '') + r.str;
    const last = groups[groups.length - 1];
    if (last && last.style === r.style) last.text += piece;
    else groups.push({ style: r.style, text: piece });
  }
  let styled = '';
  for (const g of groups) {
    const clean = g.text.replace(/ {2,}/g, ' ');
    const core = clean.trim();
    if (!g.style || !core) {
      styled += clean;
      continue;
    }
    const [open, close] = EMPHASIS_MARK[g.style] ?? ['*', '*'];
    const lead = clean.match(/^\s*/)![0];
    const trail = clean.match(/\s*$/)![0];
    styled += `${lead}${open}${core}${close}${trail}`;
  }
  styled = styled.replace(/ {2,}/g, ' ').trim();
  return { text: plain, styled: styled !== plain ? styled : undefined };
}

interface ClusterSplit {
  leftText: string;
  rightText: string;
  leftItems: TextItem[];
  rightItems: TextItem[];
}

/** Split a line's items at the widest internal gap into two columns, when
 * the geometry looks like side-by-side text. Reliable only on short lines
 * (cue pairs) — long body lines close the gap, see the boundary pass. */
function clusterSplit(items: TextItem[], pageWidth: number): ClusterSplit | null {
  // Drop whitespace-only items before measuring. pdf.js synthesizes a space
  // item to BRIDGE a wide horizontal jump, and that filler is exactly as wide
  // as the gap it spans: for a dual cue line it arrives as
  //   ["BUNNY" x=158 w=36]  [" " x=194 w=194]  ["CASSIUS" x=389 w=50]
  // so every adjacent gap measures zero, the largest-gap scan finds nothing,
  // and a perfectly good two-column line never anchors a dual region.
  // A whitespace item carries no text and contributes nothing to leftText or
  // rightText, so removing it can only reveal the real gap, never invent one.
  const solid = items.filter((it) => it.str.trim() !== '');
  if (solid.length < 2) return null;
  items = solid;
  let gapIdx = -1;
  let gapSize = 0;
  for (let i = 1; i < items.length; i++) {
    const gap = items[i].transform[4] - endX(items[i - 1]);
    if (gap > gapSize) {
      gapSize = gap;
      gapIdx = i;
    }
  }
  if (gapIdx < 0 || gapSize < pageWidth * 0.12) return null;
  const left = items.slice(0, gapIdx);
  const right = items.slice(gapIdx);
  if (left[0].transform[4] > pageWidth * 0.42) return null;
  if (right[0].transform[4] < pageWidth * 0.48) return null;
  const leftText = joinItems(left);
  const rightText = joinItems(right);
  if (!leftText || !rightText) return null;
  return { leftText, rightText, leftItems: left, rightItems: right };
}

// Synthetic indents for de-interleaved dual-dialogue lines: standard cue
// and dialogue zones so the classifier treats each column as an ordinary
// sequential speech.
const DUAL_CUE_INDENT = 40;
const DUAL_BODY_INDENT = 30;

/**
 * Dual dialogue prints two speeches in side-by-side columns. Y-joining
 * would interleave them into garbage, so: detect regions anchored by a
 * dual-cue line (two cue-shaped clusters), collect each column, and emit
 * the left speech followed by the right speech as normal-looking lines.
 */
function deinterleaveDualDialogue(
  lines: LineItems[],
  pageWidth: number,
  pageNum: number
): RawLine[] {
  const out: RawLine[] = [];
  let i = 0;

  while (i < lines.length) {
    const split = clusterSplit(lines[i].items, pageWidth);
    const isDualCue = split !== null && isCueText(split.leftText) && isCueText(split.rightText);

    if (!isDualCue) {
      // Shooting scripts print the scene number in BOTH margins on the
      // same row — collapse "2   2" / "12A.  12A." to a single token so
      // it classifies and attaches as one scene number.
      const joined = joinLine(lines[i].items);
      const text = joined.text.replace(/^(\d+[A-Z]?\.?)\s+\1$/, '$1');
      if (text) {
        const indent = Math.round((lines[i].items[0].transform[4] / pageWidth) * 100);
        const styled = text === joined.text ? joined.styled : undefined;
        // Dual-column lines below deliberately carry no `fonts`, and so no
        // fmt: that pass joins its columns to strings without retaining the
        // per-column items (registry #10a, #18's bounds).
        out.push({ text, indent, y: lines[i].y, pageNum, styled, fonts: fontRuns(lines[i].items) });
      }
      i++;
      continue;
    }

    // Dual region. Body lines are partitioned against a column boundary
    // rather than per-line gaps (long lines close the gap between the
    // columns). Geometry facts that shape this: cues sit ~1.2" deeper than
    // their column's text on BOTH sides, and the left column can start at
    // the action margin — so the boundary starts at rightCueX minus that
    // cue offset and refines to the right column's actual text edge as
    // soon as a body line splits cleanly on its own gap.
    let boundary = split.rightItems[0].transform[4] - pageWidth * 0.13;
    let leftBodyMinX: number | null = null;

    const ys: number[] = [lines[i].y];
    const left: string[] = [split.leftText];
    const right: string[] = [split.rightText];
    i++;
    while (i < lines.length) {
      // Whitespace-only items dropped for the same reason as in clusterSplit:
      // pdf.js's bridging space spans the WHOLE column gap, so it looks like a
      // single item straddling the boundary and would end the region on the
      // first body line. It carries no text, so nothing downstream loses
      // anything (joinItems re-inserts spaces from the surviving gaps).
      const items = lines[i].items.filter((it) => it.str.trim() !== '');

      // Refine toward the right column's true text edge — only ever move
      // LEFT: indented lines (parentheticals) start deeper in the column
      // and must not drag the boundary onto normal dialogue lines.
      //
      // THE ORDER HERE IS LOAD-BEARING, and it used to be the other way
      // round. The opening boundary only assumes the cue-to-body offset
      // (13% of the page). A right column whose body starts LEFT of that
      // guess while still running past it looks exactly like a full-width
      // line, so the straddle test below ended the region on its own first
      // body line: both cues fell back to action, and the two simultaneous
      // speeches were concatenated into one sentence. A line that splits
      // cleanly into two columns is by definition not full-width, so the
      // split gets to speak first.
      const gapSplit = clusterSplit(items, pageWidth);
      if (gapSplit) boundary = Math.min(boundary, gapSplit.rightItems[0].transform[4] - 6);

      const straddles = items.some(
        (it) => it.transform[4] < boundary - 6 && endX(it) > boundary + 6,
      );
      if (straddles) break; // full-width line — region over

      const leftItems = items.filter((it) => it.transform[4] < boundary);
      const rightItems = items.filter((it) => it.transform[4] >= boundary);
      const leftText = joinItems(leftItems);
      const rightText = joinItems(rightItems);

      if (leftText && rightText && isCueText(leftText) && isCueText(rightText)) {
        break; // next simultaneous exchange — new region anchors here
      }
      if (leftText && !rightText && isCueText(leftText)) {
        break; // a normal cue or slugline — back to single-column flow
      }
      if (leftText && leftBodyMinX !== null && leftItems[0].transform[4] < leftBodyMinX - 24) {
        break; // text left of the established column edge — not column text
      }

      if (leftText) {
        left.push(leftText);
        leftBodyMinX = Math.min(leftBodyMinX ?? Infinity, leftItems[0].transform[4]);
      }
      if (rightText) right.push(rightText);
      ys.push(lines[i].y);
      i++;
    }

    for (const column of [left, right]) {
      column.forEach((text, k) => {
        if (!text) return;
        out.push({
          text,
          indent: k === 0 ? DUAL_CUE_INDENT : DUAL_BODY_INDENT,
          y: ys[Math.min(k, ys.length - 1)],
          pageNum,
          // the right column's cue marks the pair for fountain's ^ syntax
          dualRight: column === right && k === 0 ? true : undefined,
        });
      });
    }
  }

  return out;
}

