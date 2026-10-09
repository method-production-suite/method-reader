// The first real page of a script, as plain paragraphs, for the window to
// type out while the rest converts (spec decision 17). Pure, and read-only
// over RawLines: nothing here feeds classification or the parse result, so
// the parser stays format-option-free and the .fountain cannot move.
import { isBoilerplateLine } from './boilerplate';
import type { RawLine } from './types';

/** A page needs this many lines before it can be the first real page. */
export const FIRST_PAGE_MIN_LINES = 8;

/** A gap this many times the page's usual line gap starts a paragraph. */
const PARAGRAPH_GAP = 1.5;

/** Left edges this close (percent of page width) are the same indent. */
const INDENT_TOLERANCE = 1;

/** A line is centred when its middle sits this close (percent) to the page's. */
const CENTRE_BAND = 10;

/** Courier 12, the screenplay face: 10 characters to the inch. RawLine has no
 *  width, so a line's extent is estimated from its length. */
const CHAR_WIDTH_PT = 7.2;

/** The page's line gap: the smallest gap between consecutive lines that
 *  occurs at least twice (gaps within half a point count as one), so a
 *  wrapped line's gap, not a blank line's. Not simply the commonest gap: on
 *  a page of one-line speeches the blank line between blocks outnumbers the
 *  wrap, and taking it as the line gap ran every heading into its action.
 *  A gap under 40% of the commonest is not a line gap (two lines set almost
 *  level, as dual dialogue can be), so it is passed over. */
function commonGap(lines: RawLine[]): number {
  const counts = new Map<number, number>();
  for (let i = 1; i < lines.length; i++) {
    const gap = Math.round((lines[i - 1].y - lines[i].y) * 2) / 2;
    if (gap > 0) counts.set(gap, (counts.get(gap) ?? 0) + 1);
  }
  let mode = 0;
  let modeCount = 0;
  for (const [gap, count] of counts) {
    if (count > modeCount || (count === modeCount && gap < mode)) {
      mode = gap;
      modeCount = count;
    }
  }
  let best = mode;
  for (const [gap, count] of counts) {
    if (count >= 2 && gap < best && gap >= mode * 0.4) best = gap;
  }
  return best;
}

/** Joins a wrapped line onto its paragraph: with one space, or with none
 *  when the line before broke at a word's hyphen ("fourteen-" "year-old"). */
function joinLines(parts: string[]): string {
  let out = parts[0] ?? '';
  for (const next of parts.slice(1)) out += /[A-Za-z]-$/.test(out) && !/--$/.test(out) ? next : ` ${next}`;
  return out;
}

/**
 * A page's words as plain paragraphs, in reading order. A paragraph breaks
 * where the gap to the next line is more than 1.5 times the page's usual
 * line gap, or where the left edge moves (a cue sits on its speech with no
 * gap at all). Wrapped lines join with one space, or none after a word's
 * hyphen. Page marks and revision
 * slugs are dropped: they are furniture, not words. Only `text` is read,
 * never `styled`, so no emphasis marker can reach the window.
 */
export function pageParagraphs(lines: RawLine[]): string[] {
  const words = lines.filter((l) => l.text.trim() !== '' && !isBoilerplateLine(l.text));
  if (words.length === 0) return [];
  const gap = commonGap(words);
  const paragraphs: string[][] = [[words[0].text.trim()]];
  for (let i = 1; i < words.length; i++) {
    const prev = words[i - 1];
    const cur = words[i];
    const wide = gap > 0 && prev.y - cur.y > gap * PARAGRAPH_GAP;
    const moved = Math.abs(prev.indent - cur.indent) > INDENT_TOLERANCE;
    if (wide || moved) paragraphs.push([cur.text.trim()]);
    else paragraphs[paragraphs.length - 1].push(cur.text.trim());
  }
  return paragraphs.map(joinLines);
}

/**
 * True when a page reads as a title page: at least half its lines are
 * centred. Centred means the line's middle is within 10% of the page's AND
 * its left edge is its own. The second half matters: a speech runs 2.5in to
 * 6in on an 8.5in page, so dialogue and full action lines are centred too,
 * but they share their left edge with the lines around them. A title block
 * is centred line by line, so each line starts somewhere different.
 */
export function isTitleLike(lines: RawLine[], pageWidth: number): boolean {
  if (lines.length === 0) return true;
  const centred = lines.filter((l) => {
    const middle = l.indent + ((l.text.length * CHAR_WIDTH_PT) / pageWidth) * 100 / 2;
    if (Math.abs(middle - 50) > CENTRE_BAND) return false;
    return !lines.some((o) => o !== l && Math.abs(o.indent - l.indent) <= INDENT_TOLERANCE);
  });
  return centred.length * 2 >= lines.length;
}

/** The first-real-page test the engine applies to each page as it is read. */
export function isFirstRealPage(lines: RawLine[], pageWidth: number): boolean {
  return lines.length >= FIRST_PAGE_MIN_LINES && !isTitleLike(lines, pageWidth);
}
