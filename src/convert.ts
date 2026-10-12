// Orchestration: PDF/Fountain input → parsed screenplay → Fountain text →
// fountain-js tokens → XHTML chapters → EPUB bytes.
import { createHash } from 'node:crypto';
import { parseFountain } from './fountain/parse';
import { extractText, type ImageStats } from './parser/extract';
import { isFirstRealPage, pageParagraphs } from './parser/first-page';
import { parseLines } from './parser/index';
import type { ParsedScreenplay } from './parser/types';
import { extractTitleMeta, toFountain, type TitleMeta } from './fountain/serialize';
import { tokensToBody, tokensToPreviewHtml } from './epub/html';
import { buildEpub, type BookImage, type BookMeta } from './epub/build';
import { resolveFormatOptions, type FormatOptions } from './options';
import { MOBI_CARRIES_PICTURES, mobiArt, tokensToMobiHtml } from './mobi/html';
import { buildMobi } from './mobi/writer';
import { isSafeImageFile } from './fountain/notes';
import { isPng } from './parser/png';

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

// The packager's picture shape, documented where it is defined (src/epub/build.ts).
export type { BookImage };

export interface ConvertOptions {
  title?: string;
  author?: string;
  /** convert even when the document doesn't look like a screenplay */
  force?: boolean;
  /** also build a MOBI 6 for dependency-free USB sideload */
  mobi?: boolean;
  /**
   * Whether that MOBI carries the pictures and the cover. Defaults to
   * MOBI_CARRIES_PICTURES (src/mobi/html.ts), the safety gate that keeps it
   * text-only until a device probe passes; only the probe and tests set it.
   */
  mobiPictures?: boolean;
  /** formatting knobs (partial; merged over defaults) */
  format?: Partial<FormatOptions>;
  /**
   * Called as the conversion advances, so a caller can show determinate
   * progress instead of a spinner. `fraction` is 0..1 across the whole
   * pipeline, not just the current stage. While pages are read, `reading`
   * says which page has been reached and how many there are, for the
   * window's "Reading page N of M" (spec decision 17).
   */
  onProgress?: (stage: ConvertStage, fraction: number, reading?: { page: number; pages: number }) => void;
  /**
   * Called at most once, as soon as the first page that is not a title page
   * has been read, with that page's words as plain paragraphs (spec decision
   * 17). A PDF only; never called if no page qualifies.
   */
  onFirstPage?: (page: number, paragraphs: string[]) => void;
}

/** The stages a caller can be told about, in the order they occur. A
 * document that places no picture never reaches 'pictures': its pages hand
 * straight over to 'render'. */
export type ConvertStage = 'parse' | 'pictures' | 'render';

/**
 * Where the pictures and the book begin on the bar, as fractions of the whole
 * job; the pages fill everything before the first.
 *
 * Reading the pages dominates wall-clock on a script without pictures, which
 * is most of them, so it owns the largest share. Decoding the pictures
 * dominates on a picture-heavy script (a 60-picture document spends about
 * 15 s of its 17 s conversion there), so it gets a share of its own and the
 * bar keeps moving through it. The shares are fixed before the text pass has
 * seen whether there are any pictures, so a script without them steps
 * straight from 0.6 to 0.85 as its pages finish: a jump forward, never a
 * wait. Rendering is comparatively fixed-cost and keeps the tail it has
 * always had.
 */
const PICTURES_START = 0.6;
const RENDER_START = 0.85;

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
   * EPUB and the MOBI show the ones its notes place when `includeImages` is
   * on, but the folder is the copy every later render reads, so these are
   * carried with the option off too. Always empty on Fountain input, which
   * has no PDF to carry them out of: the pictures it renders came from its
   * folder. */
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
 * own renderer packages, which depends on `includeImages`. A warning is
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

/**
 * The books for one `.fountain`, with the pictures its notes may show.
 *
 * `images` is every picture the caller holds. The renderer is told their
 * NAMES, and shows a figure only for a note whose picture is among them
 * (rendersImage); the packager is handed the BYTES, and ships only what the
 * body showed. Both come from this one array, which is what keeps buildEpub's
 * "shown but not given" throw out of reach: no name can be in the set without
 * its bytes being in the list. The reader preview is handed the same array as
 * a map, so the window shows the figures the book does and no others, and so
 * is the MOBI: `mobiArt` asks the EPUB's question of the same names, and its
 * order is both the markup's record numbers and the order the bytes go in.
 */
async function renderBooks(
  fountainText: string,
  meta: BookMeta,
  wantMobi: boolean,
  format: FormatOptions,
  images: readonly BookImage[],
  mobiPictures: boolean = MOBI_CARRIES_PICTURES,
) {
  const { tokens } = parseFountain(fountainText);
  // The book's identity is its CONTENT, not the moment of rendering. The
  // app rewrites the library EPUB on every options change; a fresh
  // urn:uuid each time would hand readers a "new" book to re-index on
  // every tweak. The .fountain is already the cache boundary, so its
  // text is the identity.
  const withId: BookMeta = {
    identifier: `urn:screepub:${createHash('sha256').update(fountainText).digest('hex').slice(0, 32)}`,
    ...meta,
  };
  const body = tokensToBody(tokens, { format, images: new Set(images.map((img) => img.name)) });
  const epub = await buildEpub(withId, body, format, images);
  // First bytes win for a name passed twice, as they do in buildEpub, so the
  // preview and the MOBI show the picture the EPUB ships.
  const byName = new Map<string, Uint8Array>();
  for (const img of images) if (!byName.has(img.name)) byName.set(img.name, img.bytes);
  const previewHtml = tokensToPreviewHtml(tokens, format, byName);
  let mobi: Uint8Array | undefined;
  if (wantMobi) {
    // Behind the safety gate, the MOBI is told it can ship no picture at
    // all, so mobiArt lists none and picks no cover: every note renders
    // nothing, exactly as with includeImages off.
    const art = mobiArt(tokens, format, mobiPictures ? new Set(byName.keys()) : new Set<string>());
    mobi = buildMobi({
      title: meta.title,
      author: meta.author,
      html: tokensToMobiHtml(tokens, meta, format, art),
      // Every name in `order` came out of byName, so none is missing here.
      images: art.order.map((name) => byName.get(name)!),
      // The EPUB's cover, named as one of those records (registry #19b).
      coverIndex: art.coverIndex,
    });
  }
  return { epub, mobi, previewHtml };
}

/** Full pipeline for a screenplay PDF. */
export async function convertPdf(
  pdfBytes: Uint8Array,
  opts: ConvertOptions = {},
): Promise<ConvertResult> {
  const warnings: string[] = [];

  // The text only. The pictures are located on the way past and decoded
  // further down, once neither refusal can fire: both are decided by the
  // text, so a document that is going to be turned away has no business
  // paying for its pictures first (see extractText for what that cost).
  let firstPageSent = false;
  const doc = await extractText(pdfBytes, undefined, (page, pages, pageLines, pageWidth) => {
    if (opts.onFirstPage && !firstPageSent && pageLines && pageWidth && isFirstRealPage(pageLines, pageWidth)) {
      firstPageSent = true;
      opts.onFirstPage(page, pageParagraphs(pageLines));
    }
    opts.onProgress?.('parse', (page / pages) * PICTURES_START, { page, pages });
  });
  const { lines, pageCount, spacingRepairs } = doc;
  if (lines.length < pageCount * MIN_LINES_PER_PAGE) {
    throw new ScannedPdfError(pageCount, lines.length);
  }

  // Parsed WITHOUT the pictures for the guard, which is exactly what the
  // guard would see with them: `insertImages` only ever adds `image`
  // elements, so it can neither make nor unmake a scene heading or a line of
  // dialogue. `parseLines` is pure over its inputs, so this parse is also the
  // final one whenever the document keeps no pictures (the common case), and
  // parsing again with them gives what one parse with them always gave.
  const textOnly = parseLines(lines);
  const sceneCount = textOnly.scenes.length;
  const dialogueCount = textOnly.elements.filter((e) => e.type === 'dialogue').length;
  if (sceneCount === 0 && dialogueCount === 0 && !opts.force) {
    throw new NotAScreenplayError();
  }

  // Past both refusals, so now the pixels. Nothing a caller could write down
  // exists before this line, which is also what keeps a scan's pixels off the
  // caller's disk: there are none.
  const { images: rawImages, imageStats } = await doc.decodeImages((page, pages) =>
    opts.onProgress?.('pictures', PICTURES_START + (page / pages) * (RENDER_START - PICTURES_START)),
  );
  const screenplay = rawImages.length > 0 ? parseLines(lines, rawImages) : textOnly;
  // Straight off the raw images: each already carries the filename
  // `toRawImages` assigned it, and `toFountain` writes that same name into the
  // note, so nothing here has to pair anything with the element stream. Built
  // beside the parse that consumed the same array, and before the warnings,
  // which are about these.
  const images: BookImage[] = rawImages.map((img) => ({ name: img.file, bytes: img.bytes }));

  if (sceneCount === 0) {
    warnings.push('No scene headings detected: the EPUB will have no scene navigation.');
  }
  warnings.push(...imageWarnings(imageStats, images));

  const format = resolveFormatOptions(opts.format);
  const meta = resolveMeta(extractTitleMeta(screenplay.elements), opts);
  const fountainText = toFountain(screenplay, { title: meta.title, author: meta.author }, format);
  opts.onProgress?.('render', RENDER_START);
  const { epub, mobi, previewHtml } = await renderBooks(fountainText, meta, opts.mobi ?? false, format, images, opts.mobiPictures);
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
 * documented limitation (registry §8) rather than a guessed warning.
 *
 * Worded for a writer, because a writer is who reads it: the window's Tune
 * shows every engine warning under each rebuild, and the Convert page shows
 * it when a `.fountain` is dropped. No option key, file extension or format
 * name. "The script's PDF" rather than "the PDF" because the second reader
 * may never have converted one: their (CONT'D) came from whatever wrote the
 * text, and converting the PDF of that script is still what removes it. */
function stageOneWarnings(fountainText: string, format: FormatOptions): string[] {
  if (format.contdMode !== 'strip' || !CONTD_CUE.test(fountainText)) return [];
  return [
    "This book still shows (CONT'D): it is written into the script the book was made from. " +
      "Removing it takes effect when you convert the script's PDF.",
  ];
}

export interface FountainConvertOptions extends ConvertOptions {
  /**
   * The pictures this text's `[[img: ...]]` notes may show, by filename: the
   * shape `readImages` (src/library-images.ts) returns for the folder beside
   * a `.fountain`. Absent, no note shows a picture, which is the honest
   * answer for a caller with no folder to read.
   */
  images?: ReadonlyMap<string, Uint8Array>;
}

/** Pipeline for Fountain text input (stage 1 skipped). */
export async function convertFountain(
  fountainText: string,
  opts: FountainConvertOptions = {},
): Promise<ConvertResult> {
  const { tokens } = parseFountain(fountainText);
  const detected: TitleMeta = {
    title: tokens.find((t) => t.type === 'title')?.text?.replace(/<[^>]+>/g, ''),
    author: tokens.find((t) => t.type === 'author' || t.type === 'authors')?.text,
  };
  const format = resolveFormatOptions(opts.format);
  const meta = resolveMeta(detected, opts);
  // Filtered again, though the folder's reader already refuses both and
  // counts them: this is a public entry, and a map can come from anywhere.
  // Bytes that are not a PNG would be declared image/png in the manifest and
  // fail epubcheck (OPF-029, PKG-022), so they are no picture and the note
  // renders nothing. The name check repeats the allowlist rendersImage
  // applies, so `have` never holds a name no note could carry.
  const images: BookImage[] = [...(opts.images ?? [])]
    .filter(([name, bytes]) => isSafeImageFile(name) && isPng(bytes))
    .map(([name, bytes]) => ({ name, bytes }));
  const { epub, mobi, previewHtml } = await renderBooks(fountainText, meta, opts.mobi ?? false, format, images, opts.mobiPictures);

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
    // folder, which is where the ones rendered above were read from. Handing
    // them back would invite a caller to write them over themselves.
    images: [],
  };
}
