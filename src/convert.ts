// Orchestration: PDF/Fountain input → parsed screenplay → Fountain text →
// fountain-js tokens → XHTML chapters → EPUB bytes.
import { createHash } from 'node:crypto';
import { Fountain } from 'fountain-js';
import { extractDocument } from './parser/extract';
import { isFirstRealPage, pageParagraphs } from './parser/first-page';
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
  /**
   * Called at most once, as soon as the first page that is not a title page
   * has been read, with that page's words as plain paragraphs (spec decision
   * 17). A PDF only; never called if no page qualifies.
   */
  onFirstPage?: (page: number, paragraphs: string[]) => void;
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

  let firstPageSent = false;
  const { lines, pageCount, spacingRepairs } = await extractDocument(pdfBytes, undefined, (page, pages, pageLines, pageWidth) => {
    if (opts.onFirstPage && !firstPageSent && pageLines && pageWidth && isFirstRealPage(pageLines, pageWidth)) {
      firstPageSent = true;
      opts.onFirstPage(page, pageParagraphs(pageLines));
    }
    opts.onProgress?.('parse', (page / pages) * PARSE_SHARE);
  });
  if (lines.length < pageCount * MIN_LINES_PER_PAGE) {
    throw new ScannedPdfError(pageCount, lines.length);
  }

  const screenplay = parseLines(lines);
  const sceneCount = screenplay.scenes.length;
  const dialogueCount = screenplay.elements.filter((e) => e.type === 'dialogue').length;
  if (sceneCount === 0 && dialogueCount === 0 && !opts.force) {
    throw new NotAScreenplayError();
  }
  if (sceneCount === 0) {
    warnings.push('No scene headings detected: the EPUB will have no scene navigation.');
  }

  const format = resolveFormatOptions(opts.format);
  const meta = resolveMeta(extractTitleMeta(screenplay.elements), opts);
  const fountainText = toFountain(screenplay, { title: meta.title, author: meta.author }, format);
  opts.onProgress?.('render', PARSE_SHARE);
  const { epub, mobi, previewHtml } = await renderBooks(fountainText, meta, opts.mobi ?? false, format);
  opts.onProgress?.('render', 1);

  return { epub, mobi, previewHtml, fountainText, screenplay, meta, warnings, spacingRepairs };
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
  };
}
