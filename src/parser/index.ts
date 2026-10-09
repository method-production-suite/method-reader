import type {
  ParsedScreenplay,
  ScreenplayElement,
  CharacterInfo,
  SceneInfo,
  RawImage,
  RawLine,
} from './types';
import { groupBlocks } from './group';
import { classifyBlock, attachSceneNumbers } from './classify';
import { detectTitlePages } from './title-page';
import { suppressBoilerplate } from './boilerplate';
import { rescueCues } from './rescue';
// The editorial half of the image subsystem, which knows nothing about pdf.js
// — so importing it here adds no shim-ordering hazard to the parse pipeline.
// images.ts, which does load pdf.js, is deliberately NOT imported: everything
// this file needs about an image already rode out on `RawImage`.
import { columnWidthPct, isFullPage } from './artwork';

/**
 * Classify already-extracted lines into a structured screenplay — the pure
 * post-extraction pipeline, split out so callers that need the raw lines
 * (e.g. scanned-PDF detection) don't load the PDF twice.
 *
 * `images` is optional, and omitting it is not a degraded mode: a script with
 * no pictures and a caller that does not want them produce the same stream
 * they always did.
 *
 * When they ARE passed, each must carry the page and top-edge `y` that
 * `extractDocument` gave it — those are what decide where a picture lands in
 * the stream, and a picture placed by `pageNum` alone would sit at the top of
 * its sheet. Order does not matter: `insertImages` sorts this half itself.
 * What it CANNOT normalize is `lines`, which must arrive in document order,
 * as `extractDocument` returns them.
 */
export function parseLines(lines: RawLine[], images: RawImage[] = []): ParsedScreenplay {
  // Element IDs are scoped to this call via a local closure — not a
  // module-level counter — so concurrent calls (e.g. two screenplays
  // uploaded at once) never interleave or reset each other's ID sequence.
  let elementCounter = 0;
  const nextId = () => `elem${elementCounter++}`;

  // Step 2: Group into text blocks
  const blocks = groupBlocks(lines);

  // Step 3: Classify each block (stateless — passes prev element)
  let prevElement: ScreenplayElement | null = null;
  const elements: ScreenplayElement[] = [];

  for (const block of blocks) {
    const element = classifyBlock(block, prevElement, nextId);
    elements.push(element);
    // Page furniture is TRANSPARENT to classification context. A cue can be
    // the last line of a page with its speech resuming after the printed page
    // number, and letting that number reset the speaker splits the speech:
    // the continuation classifies as action and the cue is left with nothing
    // under it. Device-confirmed on a Kindle (proof sheet, speech 37).
    //
    // This is not "attach whatever follows furniture to the last speaker":
    // classifyBlock still decides by indent band, so action after a page
    // number stays action. It only stops furniture from erasing the context
    // that a genuine continuation line depends on.
    if (element.type !== 'page-number') prevElement = element;
  }

  // Step 4: Attach scene numbers to scene headings (shooting scripts)
  attachSceneNumbers(elements);

  // Step 5: Detect title pages and front matter
  const withTitlePages = detectTitlePages(elements);

  // Step 5.5: Suppress recurring page furniture (watermarks) — re-types to
  // page-number; must run before metadata so counts stay consistent.
  const pageCountForSuppression = Math.max(...withTitlePages.map((el) => el.pageNum), 1);
  const suppressed = suppressBoilerplate(withTitlePages, pageCountForSuppression);

  // Step 5.6: Rescue cues the text heuristics rejected. Placement is
  // deliberate. AFTER suppressBoilerplate, because suppression only re-types
  // elements that are still `action` — a recurring watermark that happened to
  // match a character name would otherwise be rescued to `character` first and
  // become permanently immune to suppression. BEFORE extractCharacters, so
  // rescued speeches reach the dialogue counts. It also needs `isTitlePage`,
  // which only exists after step 5.
  // Steps 4/5/5.5 are all length-preserving (attachSceneNumbers mutates in
  // place; detectTitlePages copies and reassigns by index; suppressBoilerplate
  // is a 1:1 map), so blocks[i] still pairs with suppressed[i].
  rescueCues(suppressed, blocks);

  // Step 5.7: a speech split by a page boundary is ONE speech.
  //
  // When a speech runs over a page in the source PDF, its halves arrive as
  // two dialogue elements with printed page furniture between them. Giving
  // them the same speaker (see the page-number transparency above) is not
  // enough: they still serialize as two fountain lines and render as two
  // paragraphs, so the book shows a hard break mid-sentence. Device-confirmed
  // on a Kindle.
  //
  // Registry 8's (MORE)/(CONT'D) rejoin only fires when the script PRINTS
  // that furniture. Final Draft and Highland do; celtx does not, and shows 9
  // of these seams. This is the case that rejoin cannot see.
  //
  // Must run AFTER rescueCues: steps 4 through 5.6 are length-preserving so
  // blocks[i] still pairs with suppressed[i], and this pass is the first that
  // changes the element count.
  const merged = mergeSplitSpeeches(suppressed);

  // Step 5.8: merge the pictures in where they sat on the page (registry #19).
  //
  // AFTER mergeSplitSpeeches, and both halves of that are load-bearing. Not
  // earlier, because that pass joins a page-split speech only when nothing but
  // furniture sits between its halves, and a plate at the top of the second
  // page, under the page number and above the continuation, sits exactly
  // there: merged in first, it ends the run, and one speech comes out as two
  // with the picture between them (parser-pipeline.test.ts, "a plate between
  // the halves of a page-split speech does not split it"). Earlier still would
  // also break `rescueCues`, which needs `blocks[i]` to pair with
  // `elements[i]`, and an image element has no block. Not later, because
  // `extractScenes` records an `elementIndex` into whatever array it is
  // handed, so the images have to be in the stream before the metadata passes
  // count positions in it.
  const withImages = insertImages(merged, images, nextId);

  // Step 6: Build metadata
  const characters = extractCharacters(withImages);
  const scenes = extractScenes(withImages);
  const pageCount = Math.max(...withImages.map((el) => el.pageNum), 1);

  return { elements: withImages, characters, scenes, pageCount };
}

/**
 * Join the halves of a speech that a page boundary split in two.
 *
 * Two dialogue elements merge only when they name the SAME character and
 * nothing but page furniture sits between them. Keying on the character
 * matters: a page break between two people's lines would otherwise fuse
 * their speeches into one. Anything real in between (action, a heading, a
 * new cue) ends the run, so this can only rejoin what one page turn
 * separated.
 *
 * The furniture elements are kept, not dropped: `showPageMarkers` (registry
 * 13a) re-surfaces them, and this pass has no business deciding that.
 */
function mergeSplitSpeeches(elements: ScreenplayElement[]): ScreenplayElement[] {
  const out: ScreenplayElement[] = [];
  // The last dialogue element pushed, and whether only furniture has been
  // seen since. Reset by anything that is neither.
  let openSpeech: ScreenplayElement | null = null;

  for (const el of elements) {
    if (el.type === 'page-number') {
      out.push(el);
      continue;
    }

    if (
      el.type === 'dialogue' &&
      openSpeech !== null &&
      el.character !== undefined &&
      el.character === openSpeech.character &&
      // ONLY across a page boundary. A speech may legitimately hold two
      // paragraphs separated by a blank line, and those arrive as two
      // dialogue elements on the SAME page. Merging those would destroy a
      // break the writer put there. The defect this fixes is specifically a
      // page turn cutting one paragraph in half.
      el.pageNum !== openSpeech.pageNum
    ) {
      // A source line break inside one speech is a wrap, not a paragraph:
      // join with a space so the sentence reads continuously.
      openSpeech.text = `${openSpeech.text} ${el.text}`;
      if (openSpeech.styledText !== undefined || el.styledText !== undefined) {
        openSpeech.styledText =
          `${openSpeech.styledText ?? openSpeech.text} ${el.styledText ?? el.text}`;
      }
      continue;
    }

    out.push(el);
    openSpeech = el.type === 'dialogue' ? el : null;
  }

  return out;
}

/**
 * Merge images into the element stream by where they sat on the page.
 *
 * A one-pass MERGE and not a sort of the joined result: sorting THAT would
 * need a stable comparator over two different shapes and would reorder equal-y
 * elements for nothing. The merge instead wants each input ordered by (page
 * ascending, y descending) — PDF user space puts the top of a sheet at the
 * high y — and the two halves get there by different means.
 *
 * The IMAGES half is normalized here, one line below. `toRawImages` already
 * sorts on exactly this key, so today the sort is a no-op; it is here because
 * `parseLines` is exported and a caller that builds `RawImage[]` some other
 * way would otherwise misplace pictures silently. At most MAX_IMAGES (60)
 * items, stable per spec, and applied after every filter — so it cannot change
 * WHICH images survive, only the order they are visited in.
 *
 * The ELEMENTS half cannot be normalized the same way: re-sorting classified
 * elements would fight `mergeSplitSpeeches` and the dual-dialogue
 * de-interleave, both of which place elements deliberately. So this is the
 * half carrying the assumption, and what it rests on:
 *
 *   - `groupItemsIntoLines` emits a page's lines in descending y, and
 *     `extractDocument` visits pages 1..N and concatenates. `groupBlocks`
 *     breaks on a page change and `buildBlock` takes the FIRST line's y, so
 *     blocks inherit the order.
 *   - Steps 4 through 5.6 are length-preserving and none of them reorders;
 *     `mergeSplitSpeeches` only REMOVES elements (a page-split speech is
 *     folded into its first half, which keeps that half's page and y), and a
 *     subsequence of an ordered stream is ordered.
 *
 * The ONE exception: `deinterleaveDualDialogue` emits a simultaneous exchange
 * as the left column followed by the right column, and the right column's y
 * values restart at the top of the region. An image whose y falls inside that
 * region therefore lands among the LEFT column's lines instead of after the
 * whole exchange.
 *
 * What that costs is a misplaced picture and nothing more, and the reason is
 * in serialize.ts, not here. Landing between a cue and its speech, as the
 * pinned case does, makes the shape `prepare` would demote: it turns a
 * `character` whose NEXT element is neither dialogue nor parenthetical into
 * action, which would cost the cue its `@` and the speech its speaker. Its
 * lookahead skips images, and the emitter holds the note until the speech
 * closes, so both speeches survive and the picture follows the left one.
 * tests/parser-pipeline.test.ts asserts that whole string,
 * `@BUNNY\nLeft one.\n\n[[img: p001-01.png w=71]]\n\n@CASSIUS ^\nRight one.`,
 * rather than the element types, which would read the same if the speech
 * were lost. The note does not break the pair either: fountain-js still reads
 * one dual exchange with the note inside it, and that is pinned there too.
 *
 * Pinned rather than fixed HERE, on two counts. The guard belongs where the
 * lookahead is: an image interrupting a speech is a general problem, not a
 * dual-dialogue one, and serialize.ts handles it in every case, dual or not.
 * And a picture printed INSIDE a two-column simultaneous speech is a document
 * the corpus does not contain, so what is left (the picture reading between
 * the two columns' speeches rather than after them) is worth less than a
 * second ordering key on both streams.
 */
function insertImages(
  elements: ScreenplayElement[],
  images: RawImage[],
  nextId: () => string,
): ScreenplayElement[] {
  if (images.length === 0) return elements;
  const ordered = [...images].sort((a, b) => a.pageNum - b.pageNum || b.y - a.y);

  // The name comes WITH the image: `toRawImages` in images.ts assigns it, and
  // is the only place that numbers anything. Deriving it a second time here
  // would be a second per-page counter to keep in step with that one.
  const asElement = (img: RawImage): ScreenplayElement => ({
    type: 'image',
    // The filename doubles as the element's text, so the element is never
    // empty: `toFountain` skips anything that trims to nothing, and an image
    // carries no words of its own.
    //
    // Exactly who touches this string, because an overstated record here is
    // what would stop the next person checking. The metadata passes below key
    // on `type` and never read it. `printedPageOffset` type-gates before it
    // does. `prepare` DOES reach it — `isBody` admits images, so every
    // filename is run through the `(MORE)` regex — but that regex is
    // `^\(\s*MORE\s*\)$`, which `pNNN-NN.png` cannot match. And `toFountain`
    // reads it once more, for the `if (!text) continue` emptiness guard, and
    // then writes the note from `el.img` rather than from here: the filename
    // in the `.fountain` is `img.file`, so this copy is load-bearing only as
    // "not empty".
    text: img.file,
    // Minted after every text element's, because the block loop has already
    // run. IDs are unique and nothing in the engine reads them in order.
    id: nextId(),
    pageNum: img.pageNum,
    y: img.y,
    // Never title-page furniture, and this is a decision rather than a
    // shortcut: `detectTitlePages` has already run, so an image is not being
    // classified here, it is being kept out of a classification that would
    // cost it something.
    //
    // What it costs: `extractTitleMeta` in serialize.ts joins the readable
    // title-page elements into the book's TITLE, so a flagged image would put
    // "p001-01.png" in the metadata. That is the whole reason. `isBody` does
    // not depend on this flag: it exempts images from the title-page filter
    // outright, so the designed title card survives whichever way it goes.
    isTitlePage: false,
    isReadable: true,
    img: {
      file: img.file,
      widthPct: columnWidthPct(img.pageWidthPct),
      fullPage: isFullPage(img.pageWidthPct, img.pageHeightPct),
    },
  });

  /**
   * Does this image belong before this element?
   *
   * STRICTLY greater, and the tie is not arbitrary: the two `y` are different
   * EDGES. A line's is the text item's own origin, which for text is its
   * BASELINE (extract.ts takes `transform[5]`), while an image's is its TOP
   * (`toRawImages`). So a picture whose y equals a line's y begins where that
   * line's glyphs sit and runs downward from there — it is below the line, and
   * the line goes first.
   *
   * `?? POSITIVE_INFINITY` for an element with no `y`, which is a case the
   * parser cannot produce — `classifyBlock` sets one on everything it mints —
   * and which therefore only decides what a hand-built stream would do. The
   * infinity that makes it a NON-anchor is the safer of the two: such an
   * element is skipped over and the image lands against the next element that
   * can be located. NEGATIVE_INFINITY would instead dump every remaining image
   * on the page in front of it. Untested for the same reason it is unreachable.
   */
  const goesBefore = (img: RawImage, el: ScreenplayElement): boolean =>
    img.pageNum < el.pageNum ||
    (img.pageNum === el.pageNum && img.y > (el.y ?? Number.POSITIVE_INFINITY));

  const out: ScreenplayElement[] = [];
  let i = 0;
  for (const el of elements) {
    while (i < ordered.length && goesBefore(ordered[i], el)) out.push(asElement(ordered[i++]));
    out.push(el);
  }
  // Whatever is left sat below the last element, or on a sheet past it — a
  // final full-page plate is exactly that, and it must not be dropped.
  while (i < ordered.length) out.push(asElement(ordered[i++]));
  return out;
}

// attachSceneNumbers lives in classify.ts because it shares the private
// SCENE_NUMBER pattern with the classifier that mints those markers, and
// splitting them would mean exporting the regex or keeping two of it.
//
// Not because of pdf.js: this file has none in its chain. It imports group,
// classify, title-page, boilerplate, rescue and artwork, and not one of those
// reaches pdf.mjs. extract.ts is where that hazard lives, and index.ts does
// not import it, which is the whole reason parseLines takes lines instead of
// bytes.

function extractCharacters(elements: ScreenplayElement[]): CharacterInfo[] {
  const charMap = new Map<string, { dialogueCount: number; firstAppearance: number }>();

  for (let i = 0; i < elements.length; i++) {
    const el = elements[i];
    // A centered title lands at ~38% indent — inside the cue band — and
    // classifies as a character. detectTitlePages already flagged it;
    // the roster must honor the flag or the title becomes a "speaker".
    if (el.isTitlePage) continue;
    if (el.type === 'character' && el.baseCharacter) {
      const base = el.baseCharacter;
      const existing = charMap.get(base);
      if (!existing) {
        charMap.set(base, { dialogueCount: 0, firstAppearance: i });
      }
    }
    if (el.type === 'dialogue' && el.character) {
      const base = el.character;
      const existing = charMap.get(base);
      if (existing) {
        existing.dialogueCount++;
      } else {
        charMap.set(base, { dialogueCount: 1, firstAppearance: i });
      }
    }
  }

  return [...charMap.entries()]
    .map(([name, info]) => ({
      name,
      baseName: name,
      dialogueCount: info.dialogueCount,
      firstAppearance: info.firstAppearance,
    }))
    .sort((a, b) => b.dialogueCount - a.dialogueCount);
}

function extractScenes(elements: ScreenplayElement[]): SceneInfo[] {
  let sceneNumber = 0;
  return elements
    .map((el, idx) => ({ el, idx }))
    .filter(({ el }) => el.type === 'scene')
    .map(({ el, idx }) => ({
      heading: el.text,
      sceneNumber: ++sceneNumber,
      elementIndex: idx,
      pageNum: el.pageNum,
    }));
}
