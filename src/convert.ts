// Orchestration: PDF/Fountain input → parsed screenplay → Fountain text →
// fountain-js tokens → XHTML chapters → EPUB bytes.
import { createHash } from 'node:crypto';
import { Fountain } from 'fountain-js';
import { extractDocument, type ImageStats } from './parser/extract';
import { parseLines } from './parser/index';
import type { ParsedScreenplay } from './parser/types';
import { extractTitleMeta, toFountain, type TitleMeta } from './fountain/serialize';
import { tokensToBody, tokensToPreviewHtml } from './epub/html';
import { buildEpub, type BookMeta } from './epub/build';
import { resolveFormatOptions, type FormatOptions } from './options';
import { tokensToMobiHtml } from './mobi/html';
import { buildMobi } from './mobi/writer';

/** Below this many text lines per page, the PDF has no usable text layer. */
const MIN_LINES_PER_PAGE = 3;

export class ScannedPdfError extends Error {
  constructor(pages: number, lines: number) {
    super(
      `No usable text layer (${lines} text lines across ${pages} pages). ` +
        `This looks like a scanned/image-only PDF. Run OCR on it first.`,
    );
    this.name = 'ScannedPdfError';
  }
}

export class NotAScreenplayError extends Error {
  constructor() {
    super(
      'No scene headings and no dialogue found: this does not look like a ' +
        'screenplay. Pass --force to convert it anyway.',
    );
    this.name = 'NotAScreenplayError';
  }
}

/**
 * One picture on its way out of a conversion: the bytes, and the name the
 * `.fountain` note refers to it by.
 *
 * Named, NOT pathed. convert.ts does no filesystem work and never has (it
 * imports `node:crypto` and nothing else from the platform), so where these
 * land is the caller's business: the CLI writes them into the picture folder
 * beside the `.fountain` (src/library-images.ts).
 */
export interface BookImage {
  name: string;
  bytes: Uint8Array;
}

export interface ConvertOptions {
  title?: string;
  author?: string;
  /** convert even when the document doesn't look like a screenplay */
  force?: boolean;
  /** also build a MOBI 6 for dependency-free USB sideload */
  mobi?: boolean;
  /** formatting knobs (partial; merged over defaults) */
  format?: Partial<FormatOptions>;
  /**
   * Called as the conversion advances, so a caller can show determinate
   * progress instead of a spinner. `fraction` is 0..1 across the whole
   * pipeline, not just the current stage.
   */
  onProgress?: (stage: ConvertStage, fraction: number) => void;
}

/** The stages a caller can be told about, in the order they occur. */
export type ConvertStage = 'parse' | 'render';

/**
 * Page extraction dominates wall-clock on any real script, so it owns most
 * of the bar. Rendering is comparatively fixed-cost and gets the tail.
 */
const PARSE_SHARE = 0.85;

export interface ConvertResult {
  epub: Uint8Array;
  mobi?: Uint8Array;
  previewHtml: string;
  fountainText: string;
  screenplay: ParsedScreenplay | null;
  meta: BookMeta;
  warnings: string[];
  /** Word spaces pdf.js invented that were removed against the glyph stream.
   *  Reported rather than hidden: this edits the author's text, so the
   *  number is part of the answer. Normally 0. */
  spacingRepairs: number;
  /** The pictures a PDF conversion carried, for the caller to write into the
   * folder beside the `.fountain`, whose `[[img: ...]]` notes name them. The
   * renderers do not receive pictures, so the book shows none and the folder
   * is their only copy. Always empty on Fountain input, which has no PDF to
   * carry them out of. */
  images: BookImage[];
}

/**
 * What to tell someone about the pictures that did not make it.
 *
 * Both counts are EXCEPTIONAL. The ordinary losses — a letterhead, a rule, a
 * watermark — are `droppedBy`, and warning about those would warn on nearly
 * every real script, which is how a warning stops being read. These two are
 * the ones a filmmaker cannot see for themselves:
 *
 *   - `overCap`: the conversion is missing art that cleared every filter.
 *     Silent truncation reads as "we carried everything" when we did not.
 *   - `failed`: pixels we could not get out of the PDF at all. This is the one
 *     that matters more, because a script that loses its art with no error
 *     reads exactly like a script that never had pictures. It picks up a
 *     second sentence naming the cause in the one case where the cause is
 *     ours and not the file's — see `decodersUnavailable`.
 *
 * `failed` counts PLACEMENTS, so it can name a picture that would have been
 * dropped as furniture anyway — a corrupt logo on one sheet is "1 picture
 * could not be read" even though a healthy one would never have been carried
 * either. Reporting that is the deliberate trade: the alternative is judging a
 * picture whose pixels we do not have, and the failure this exists to catch
 * (every image failing at once) is invisible any other way.
 *
 * Takes the pictures themselves rather than a count of them, because a count
 * would be a second bare number beside `stats.overCap` and `stats.failed` with
 * nothing to stop it being the wrong one — the hazard `collectPageImages`
 * spends an object to avoid. `imageWarnings(stats, stats.overCap)` would
 * compile fine against a `number` and produce a sentence wrong in both halves.
 *
 * Two things the WORDING is careful about, both learned by reading it back as
 * a filmmaker rather than as its author:
 *
 *   - "Screepub found N pictures", not "this script has N". The total is a
 *     fact about our filters, not about the document: letterhead, rules and
 *     watermarks are already excluded from it, so it matches nothing a reader
 *     can count in their own PDF, and claiming otherwise invites them to go
 *     looking for the difference.
 *   - the two numbers DO NOT ADD UP, on purpose. `failed` counts placements,
 *     some of which the filters would have dropped anyway, so "found 126, kept
 *     60" beside "3 could not be read" is not 126 = 60 + 63 + 3. Neither
 *     sentence invites the arithmetic, and neither should be edited into a
 *     single one that does.
 *
 * "Kept", not "in the book": this counts what the conversion carried out of
 * the PDF, which is what the picture folder holds. A book shows only what its
 * own renderer packages, and today that is none of them. A warning is
 * user-facing copy, so it claims the thing that is true in every state.
 *
 * Exported for its own tests. Neither count is reachable from a fixture that
 * is not deliberately damaged: `overCap` needs a 61-picture document, and
 * `failed` needs an XObject pdf.js cannot decode.
 */
export function imageWarnings(stats: ImageStats, carried: readonly BookImage[]): string[] {
  const out: string[] = [];
  const kept = carried.length;
  if (stats.overCap > 0) {
    out.push(
      `Screepub found ${kept + stats.overCap} pictures in this script. Only the first ${kept} were kept.`,
    );
  }
  if (stats.failed > 0) {
    out.push(
      stats.failed === 1
        ? '1 picture could not be read out of the PDF, so it was left out.'
        : `${stats.failed} pictures could not be read out of the PDF, so they were left out.`,
    );
    // Only ever BESIDE that sentence, never instead of it and never alone: it
    // names a cause, and a cause with nothing to explain is noise on a script
    // that never had a JPEG2000 picture in it. Worth saying when there IS
    // something to explain, because it is the one entry in this function that
    // is not about the user's file. Every other loss here means "your PDF"; a
    // temp directory we could not write means "us", and the sentence above
    // would otherwise send someone to re-export a script that was fine.
    if (stats.decodersUnavailable) {
      out.push(
        'Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read.',
      );
    }
  }
  return out;
}

function resolveMeta(detected: TitleMeta, opts: ConvertOptions): BookMeta {
  return {
    title: opts.title ?? detected.title ?? 'Untitled Screenplay',
    author: opts.author ?? detected.author,
  };
}

async function renderBooks(
  fountainText: string,
  meta: BookMeta,
  wantMobi: boolean,
  format: FormatOptions,
) {
  const { tokens } = new Fountain().parse(fountainText, true);
  // The book's identity is its CONTENT, not the moment of rendering. The
  // app rewrites the library EPUB on every options change; a fresh
  // urn:uuid each time would hand readers a "new" book to re-index on
  // every tweak. The .fountain is already the cache boundary, so its
  // text is the identity.
  const withId: BookMeta = {
    identifier: `urn:screepub:${createHash('sha256').update(fountainText).digest('hex').slice(0, 32)}`,
    ...meta,
  };
  const epub = await buildEpub(withId, tokensToBody(tokens, { format }), format);
  const previewHtml = tokensToPreviewHtml(tokens, format);
  const mobi = wantMobi
    ? buildMobi({ title: meta.title, author: meta.author, html: tokensToMobiHtml(tokens, meta, format) })
    : undefined;
  return { epub, mobi, previewHtml };
}

/** Full pipeline for a screenplay PDF. */
export async function convertPdf(
  pdfBytes: Uint8Array,
  opts: ConvertOptions = {},
): Promise<ConvertResult> {
  const warnings: string[] = [];

  const { lines, pageCount, spacingRepairs, images: rawImages, imageStats } = await extractDocument(
    pdfBytes,
    undefined,
    (page, pages) => opts.onProgress?.('parse', (page / pages) * PARSE_SHARE),
  );
  // BEFORE anything is handed back, which is what keeps a scan's pixels off
  // the caller's disk. A 13-sheet scan really does encode 13 full-page PNGs on
  // the way here (measured 2026-08-11 on the local corpus: 6.1 MB in 3.7s,
  // which is what the image budget caps for a longer one), and every byte of
  // it leaves through this throw rather than in a result somebody writes down.
  if (lines.length < pageCount * MIN_LINES_PER_PAGE) {
    throw new ScannedPdfError(pageCount, lines.length);
  }

  const screenplay = parseLines(lines, rawImages);
  // Straight off the raw images: each already carries the filename
  // `toRawImages` assigned it, and `toFountain` writes that same name into the
  // note, so nothing here has to pair anything with the element stream. Built
  // beside the parse that consumed the same array, and before the warnings,
  // which are about these.
  const images: BookImage[] = rawImages.map((img) => ({ name: img.file, bytes: img.bytes }));

  const sceneCount = screenplay.scenes.length;
  const dialogueCount = screenplay.elements.filter((e) => e.type === 'dialogue').length;
  if (sceneCount === 0 && dialogueCount === 0 && !opts.force) {
    throw new NotAScreenplayError();
  }
  if (sceneCount === 0) {
    warnings.push('No scene headings detected: the EPUB will have no scene navigation.');
  }
  warnings.push(...imageWarnings(imageStats, images));

  const format = resolveFormatOptions(opts.format);
  const meta = resolveMeta(extractTitleMeta(screenplay.elements), opts);
  const fountainText = toFountain(screenplay, { title: meta.title, author: meta.author }, format);
  opts.onProgress?.('render', PARSE_SHARE);
  const { epub, mobi, previewHtml } = await renderBooks(fountainText, meta, opts.mobi ?? false, format);
  opts.onProgress?.('render', 1);

  return { epub, mobi, previewHtml, fountainText, screenplay, meta, warnings, spacingRepairs, images };
}

/** A forced cue line still carrying a (CONT'D) — the shape serialize.ts
 * writes. Same apostrophe tolerance as its CONTD, since the marker may have
 * come from the source PDF rather than from us. */
const CONTD_CUE = /^@.*\(\s*CONT['’]?D\.?\s*\)/im;

/** Stage-1 options are consumed in fountain/serialize.ts, upstream of the
 * .fountain — the app's cache boundary. On Fountain input that stage has
 * already run, so the request cannot be honored and the caller deserves to
 * hear about it rather than get a clean exit and unchanged output.
 *
 * Only the provable case is reported. `keep` and `auto` promise no removal
 * here, and `rejoinSplitDialogue` leaves nothing to detect: serialize.ts
 * drops (MORE) markers unconditionally, so an unrejoined split is not
 * distinguishable from an ordinary same-speaker continuation. That one is a
 * documented limitation (registry §8) rather than a guessed warning. */
function stageOneWarnings(fountainText: string, format: FormatOptions): string[] {
  if (format.contdMode !== 'strip' || !CONTD_CUE.test(fountainText)) return [];
  return [
    "contdMode \"strip\" was not applied: (CONT'D) is written into the .fountain " +
      'when the PDF is read, so it cannot be removed from Fountain input. ' +
      'Re-convert the original PDF with this setting to strip it.',
  ];
}

/** Pipeline for Fountain text input (stage 1 skipped). */
export async function convertFountain(
  fountainText: string,
  opts: ConvertOptions = {},
): Promise<ConvertResult> {
  const { tokens } = new Fountain().parse(fountainText, true);
  const detected: TitleMeta = {
    title: tokens.find((t) => t.type === 'title')?.text?.replace(/<[^>]+>/g, ''),
    author: tokens.find((t) => t.type === 'author' || t.type === 'authors')?.text,
  };
  const format = resolveFormatOptions(opts.format);
  const meta = resolveMeta(detected, opts);
  const { epub, mobi, previewHtml } = await renderBooks(fountainText, meta, opts.mobi ?? false, format);

  return {
    epub,
    mobi,
    previewHtml,
    fountainText,
    screenplay: null,
    // Fountain input never went through pdf.js, so there was no invented
    // space to undo. Zero is the honest answer, not "unknown".
    spacingRepairs: 0,
    meta,
    warnings: stageOneWarnings(fountainText, format),
    // Nothing to carry: the pictures a `.fountain` names already sit in its
    // folder, and nothing reads them back for the book, since the renderers
    // do not receive pictures.
    images: [],
  };
}
