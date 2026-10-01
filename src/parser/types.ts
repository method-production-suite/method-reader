// Screenplay parser types.
// Adapted from an earlier table-read parser by the same author.

export type ElementType =
  | "scene" | "character" | "dialogue" | "action"
  | "parenthetical" | "transition" | "page-number" | "mini-slug"
  | "image";

/** Coarse font-family buckets. Deliberately four: a screenplay's own face,
 * and the three kinds of thing a chyron/insert/letter is ever set in. */
export type FamilyBucket = 'mono' | 'serif' | 'sans' | 'cursive';

/** Coarse size steps relative to the document's dominant size. */
export type SizeStep = '-1' | '+1' | '+2';

/** A block-level font shift. At least one field is always present. */
export interface Fmt {
  family?: FamilyBucket;
  size?: SizeStep;
}

/** Characters of one line set in one (bucket, size) pair. Runs whose font
 * never resolved are omitted, so shares below are over resolved characters. */
export interface FontRun {
  bucket: FamilyBucket;
  size: number;
  chars: number;
}

export interface ScreenplayElement {
  type: ElementType;
  text: string;
  id: string;
  pageNum: number;
  isTitlePage: boolean;
  isReadable: boolean;
  character?: string;
  baseCharacter?: string;
  ageModifier?: string;
  ageValue?: string | number;
  /** Scene number from shooting scripts (e.g., "2.2.", "1A."), attached to scene elements */
  sceneNumber?: string;
  /** text with fountain emphasis markers from PDF font styles (*i*, **b**) */
  styledText?: string;
  /** cue of the RIGHT column of a simultaneous (dual) exchange */
  dualRight?: boolean;
  /** block-level font shift, serialized as a [[fmt: ...]] note (registry #18) */
  fmt?: Fmt;
  /** image carried out of the PDF, serialized as an [[img: ...]] note (#19) */
  img?: ImageRef;
  /**
   * Where this element sat on its page, in PDF user space (up is the high y).
   *
   * The same axis and direction as `RawLine.y` and `RawImage.y`, which is what
   * lets `insertImages` (index.ts) merge the two streams on a plain comparison
   * instead of guessing from `pageNum` alone. NOT the same EDGE in both cases,
   * and the difference decides the tie: a text element carries the BASELINE of
   * its block's topmost line, an image element its own top edge. See
   * `goesBefore` in index.ts.
   *
   * Optional because several suites in tests/ build this interface by hand with
   * an `el()` helper that omits it — not because the parser ever leaves it out:
   * `classifyBlock` sets it on every element it mints, and the image elements
   * `insertImages` adds carry one too.
   */
  y?: number;
}

/** An image as the .fountain and both renderers see it (registry #19). */
export interface ImageRef {
  /** filename inside the sidecar folder */
  file: string;
  /**
   * Rendered width as a percentage of the script's TEXT COLUMN.
   *
   * NOT the same measure as `RawImage.pageWidthPct`, which is a share of the
   * whole sheet — this is that value remapped, and the two are named apart on
   * purpose (a standard script's column is 6.0" of an 8.5" page, so the same
   * picture is 50.8% of the page and 72% of the column). The e-book has no
   * print margins, so the column is the honest reference.
   *
   * The 5..100 range is the clamp the remap applies, not an independent fact
   * about images: 100 because nothing can be wider than the column it is
   * measured against, 5 because a picture rendered smaller than that would be
   * invisible on an e-ink panel and is better shown too large than not at all.
   */
  widthPct: number;
  /** covered at least `FULL_PAGE_AREA_FRAC` of its page in the source PDF;
   * drives the page-break treatment and cover eligibility */
  fullPage: boolean;
}

export interface CharacterInfo {
  name: string;
  baseName: string;
  dialogueCount: number;
  firstAppearance: number;
}

export interface SceneInfo {
  heading: string;
  sceneNumber: number;
  elementIndex: number;
  pageNum: number;
}

export interface ParsedScreenplay {
  elements: ScreenplayElement[];
  characters: CharacterInfo[];
  scenes: SceneInfo[];
  pageCount: number;
}

// Raw line extracted from PDF
export interface RawLine {
  text: string;
  indent: number; // 0-100 normalized percentage
  y: number;
  pageNum: number;
  /** text with fountain emphasis markers, present only when styles differ */
  styled?: string;
  /** cue line opening the right column of a dual-dialogue region */
  dualRight?: boolean;
  /** per-run font tallies, consumed by the document-dominant pass */
  fonts?: FontRun[];
  /** block-level font shift relative to the document's dominant font */
  fmt?: Fmt;
}

/** An image carried out of the PDF, positioned alongside RawLine. */
export interface RawImage {
  /**
   * Sidecar filename, assigned where the image is numbered (images.ts,
   * `toRawImages`) and never derived again.
   *
   * Deterministic from the page and the order on it, so re-converting the same
   * PDF reproduces the same `.fountain` and the same folder. It lives on the
   * raw image rather than being rebuilt at each stage because the name has to
   * be one string in three places — the `[[img: ...]]` note, the sidecar file,
   * and the manifest entry in each renderer — and a second derivation is a
   * second per-page counter to keep in step.
   */
  file: string;
  pageNum: number;
  /** TOP edge in PDF user space, same axis and direction as RawLine.y, so
   * images and lines interleave under one descending sort.
   *
   * The ONLY geometry that survives to here, and deliberately: a picture's
   * horizontal offset is not carried, because nothing places one by it. The
   * renderers centre a figure (`.figure { text-align: center }`) and size it
   * from `ImageRef.widthPct`, so a left edge would be a field with a plausible
   * meaning and no reader — which is what `indent` was until 2026-08-12. */
  y: number;
  /**
   * Rendered size on the sheet, as a percentage of PAGE width and height.
   *
   * Named for the page because there is a second width percentage one stage
   * later: `ImageRef.widthPct` is a share of the TEXT COLUMN, and index.ts
   * converts this into that. Two fields called `widthPct` meaning different
   * denominators is the mix-up `images.ts` renamed `srcPxWidth` to prevent —
   * same defect, one file over, and nothing downstream could catch it because
   * both are plausible percentages of plausible things.
   */
  pageWidthPct: number;
  pageHeightPct: number;
  /** Encoded PNG, and the only thing here that carries pixels.
   *
   * The OUTPUT dimensions are not a field beside it. They were, until
   * 2026-08-12, and nothing ever read them: the renderers work from the
   * `.fountain` and never see a `RawImage`, so there was no path to a consumer
   * — and the MOBI arm's refusal to write a pixel width (mobi/html.ts) reads
   * as contradicted by a `pxWidth` sitting two modules upstream. Anything that
   * genuinely needs them can read the IHDR, which is where they are true. */
  bytes: Uint8Array;
}

// Grouped text block
export interface TextBlock {
  lines: RawLine[];
  text: string;
  styledText?: string;
  fmt?: Fmt;
  dualRight?: boolean;
  indent: number; // first line indent
  minIndent: number;
  maxIndent: number;
  pageNum: number;
  yPosition: number;
}

// Indent ranges for element classification
export const INDENT_RANGES = {
  ACTION_MAX: 20,
  DIALOGUE_MIN: 25,
  DIALOGUE_MAX: 35,
  CHARACTER_MIN: 35,
  CHARACTER_MAX: 50,
  TRANSITION_MIN: 55,
} as const;
