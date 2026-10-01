// ScreenplayElement[] → Fountain text.
// Fountain forcing syntax is used defensively (@cue, > transition, !action)
// so fountain-js re-parses our output without relying on its own guesswork.
import type { ParsedScreenplay, ScreenplayElement } from '../parser/types';
import type { FormatOptions } from '../options';
import { DEFAULT_FORMAT_OPTIONS } from '../options';

export interface TitleMeta {
  title?: string;
  author?: string;
}

const BY_LINE = /\b(written by|screenplay by|by)\b/i;
const MORE_PAREN = /^\(\s*MORE\s*\)$/i;
const CONTD = /\(\s*CONT['’]?D\.?\s*\)/i;
// Leading chars that carry Fountain meaning at line start; ! forces action.
const NEEDS_FORCE = /^[!.>=~@#]/;

/**
 * A block's font shift as a Fountain note (registry #18).
 *
 * Notes are the one channel the format guarantees is BOTH lossless through a
 * tokenizer and invisible in output, so a tool that has never heard of
 * Screepub ignores this and a reader never sees it. Emitted unconditionally:
 * the `.fountain` is the app's cache boundary, so it must be byte-stable under
 * settings flips — `preserveFontShifts` gates rendering, not serialization.
 */
function fmtNote(fmt: ScreenplayElement['fmt']): string | null {
  if (!fmt) return null;
  const parts = [fmt.family, fmt.size].filter(Boolean);
  return parts.length > 0 ? `[[fmt: ${parts.join(' ')}]]` : null;
}

/** Title-case an ALL-CAPS string; leave mixed-case text untouched. */
function humanizeCaps(text: string): string {
  if (text !== text.toUpperCase()) return text;
  return text
    .toLowerCase()
    .replace(/\b\p{L}/gu, (c) => c.toUpperCase());
}

/**
 * Pull title/author from detected title-page elements: readable elements
 * before the by-line form the title, the first readable element after it
 * (or the by-line's own remainder) is the author.
 */
export function extractTitleMeta(elements: ScreenplayElement[]): TitleMeta {
  const titlePage = elements.filter((el) => el.isTitlePage && el.isReadable);
  if (titlePage.length === 0) return {};

  const byIdx = titlePage.findIndex((el) => BY_LINE.test(el.text));
  if (byIdx === -1) {
    return { title: humanizeCaps(titlePage[0].text.trim()) };
  }

  const titleParts = titlePage.slice(0, byIdx).map((el) => el.text.trim()).filter(Boolean);
  const title = titleParts.length > 0 ? humanizeCaps(titleParts.join(' ')) : undefined;

  // Author inline in the by-line ("written by Jane Doe") or the next element.
  const byText = titlePage[byIdx].text;
  const match = BY_LINE.exec(byText)!;
  const inline = byText
    .slice(match.index + match[1].length)
    .replace(/^[\s/:–—-]+/, '')
    .trim();
  const author = inline || titlePage[byIdx + 1]?.text.trim() || undefined;

  return { title, author };
}

/**
 * An image as a Fountain note (registry #19).
 *
 * Emitted unconditionally, for the same reason `fmtNote` is: the `.fountain`
 * is the app's cache boundary, so it must be byte-stable under settings flips
 * — whether the book shows a picture is a rendering decision, never a
 * serialization one. Read back by `parseImgNote` in notes.ts, which is the
 * only thing that parses this string.
 *
 * THE WIDTH MUST BE AN INTEGER, because `WIDTH_WORD` in notes.ts reads nothing
 * else: a fractional `w=` does not match, the note parses with the default 100
 * instead, and every picture in the book silently renders full-column. Today
 * `columnWidthPct` ends in Math.round so the value already is one — which is
 * the problem, because that makes this a property of a function one module
 * away that nobody editing it would think to check. The round here is belt and
 * braces against exactly that, and it is free.
 */
function imgNote(img: NonNullable<ScreenplayElement['img']>): string {
  const w = Math.round(img.widthPct);
  return `[[img: ${img.file} w=${w}${img.fullPage ? ' full' : ''}]]`;
}

/** True when this element belongs in the Fountain body. */
function isBody(el: ScreenplayElement): boolean {
  // Images are admitted even on a title page: a picture there is the designed
  // title card, which is the one thing on that page worth keeping.
  //
  // Belt and braces, not load-bearing: `insertImages` mints every image with
  // `isTitlePage: false` already. But it does that for `extractTitleMeta`'s
  // sake — a flagged image would land in the book's TITLE — and NOT for this
  // filter's, so the two are independent rather than one justifying the other.
  // A hand-built stream, or a future pass that classifies images, meets this.
  if (el.type === 'image') return el.isReadable;
  return !el.isTitlePage && el.isReadable && el.type !== 'page-number';
}

/**
 * True when a page marker may be anchored to this element.
 *
 * A POSITIVE list, and that is the point. An exclusion list (anchor anything
 * that is not dialogue, a parenthetical or a page number) admits every type
 * added to ElementType by default, and with it the power to close an open
 * speech, since the marker branch calls closeBlock itself; `image` is exactly
 * such a type. Here a new type anchors nothing until someone says it does,
 * and the switch is exhaustive with no `default`, so adding one to
 * ElementType stops the typecheck until that decision is made.
 *
 * The rule it encodes: a marker is a navigation anchor onto a line of text a
 * reader can land on. Headings, cues, action and transitions are that.
 * Dialogue and parentheticals are not, because a page turning mid-speech must
 * not break the speech in half. An image is not, because it is not a line of
 * text — and because anchoring one would strand the note `case 'image'` is
 * waiting to defer. `page-number` never reaches here at all: isBody drops it.
 */
function anchorsPageMarker(el: ScreenplayElement): boolean {
  switch (el.type) {
    case 'scene':
    case 'character':
    case 'action':
    case 'transition':
    case 'mini-slug':
      return true;
    case 'dialogue':
    case 'parenthetical':
    case 'image':
    case 'page-number':
      return false;
  }
}

/**
 * A PICTURE IS NOT EVIDENCE ABOUT THE ELEMENTS AROUND IT.
 *
 * Both passes below decide something about a cue by looking at its neighbour,
 * and both are wrong if a plate can sit in between: an image says nothing
 * about whether a cue has a speech under it, or about whether a speech
 * continues across a page. So neighbour means "nearest element that is not an
 * image", in both directions, and these two helpers are that rule written
 * once. The seam is ordinary layout — a plate at the top of a sheet, with a
 * cue or a continuation under it.
 */
// Both walk off their end onto `undefined` rather than testing a bound: at
// i = -1 or i = length the index simply misses, which is the same answer the
// caller needs anyway ("no neighbour"). Written the same way in both
// directions so the pair reads as one rule rather than two.
const prevNonImage = (els: ScreenplayElement[], from: number): ScreenplayElement | undefined => {
  let i = from;
  while (els[i]?.type === 'image') i--;
  return els[i];
};
const nextNonImage = (els: ScreenplayElement[], from: number): ScreenplayElement | undefined => {
  let i = from;
  while (els[i]?.type === 'image') i++;
  return els[i];
};

/**
 * Pre-passes over body elements:
 * 1. drop "(MORE)" page-break markers;
 * 2. drop "(CONT'D)" cues that directly continue the previous speech
 *    (page-break splits) so the dialogue rejoins as one speech;
 * 3. demote cues with no following dialogue to action.
 */
function prepare(elements: ScreenplayElement[], rejoin: boolean): ScreenplayElement[] {
  // "(MORE)" page-break markers can classify as parenthetical, action, or
  // dialogue depending on the template's indents — drop them by text alone.
  const body = elements.filter(isBody).filter((el) => !MORE_PAREN.test(el.text.trim()));

  const merged: ScreenplayElement[] = [];
  for (const el of body) {
    // Past the plate, or a picture at the top of a sheet silently switches
    // rejoinSplitDialogue off: the repeated cue survives and the reader gets a
    // duplicated "NAME (CONT'D)" with one speech in two blocks. The dropped
    // cue leaves `dialogue | image | dialogue`, and the emitter's deferral
    // closes the loop — the block is still open when the plate arrives, so the
    // speech rejoins and the picture lands after it.
    const prev = prevNonImage(merged, merged.length - 1);
    if (
      rejoin &&
      el.type === 'character' &&
      CONTD.test(el.text) &&
      prev &&
      (prev.type === 'dialogue' || prev.type === 'parenthetical') &&
      prev.character === el.character
    ) {
      continue; // same speech continues — drop the repeated cue
    }
    merged.push(el);
  }

  return merged.map((el, i) => {
    // Past the plate for the same reason, in the other direction: a picture
    // between a cue and its speech would otherwise demote the cue, and the
    // speech under it would lose its speaker and render as loose action
    // paragraphs. Reachable rather than hypothetical — the dual-dialogue
    // ordering exception in parser-pipeline.test.ts produces exactly that
    // shape, and so does a plate under a cue at the foot of a page.
    const next = nextNonImage(merged, i + 1);
    if (el.type === 'character' && !(next && (next.type === 'dialogue' || next.type === 'parenthetical'))) {
      return { ...el, type: 'action' as const };
    }
    return el;
  });
}

/**
 * Sheet→printed-page offset: the PDF's own printed page numbers (stripped
 * as furniture but still typed 'page-number') anchor the numbering, so
 * markers match the script's pagination — the title page never counts.
 * The mode wins because margin scene numbers pollute with random offsets.
 */
function printedPageOffset(elements: ScreenplayElement[]): number | null {
  const counts = new Map<number, number>();
  for (const el of elements) {
    if (el.type !== 'page-number') continue;
    const m = /^(\d{1,3})\.?$/.exec(el.text.trim());
    if (!m) continue;
    const offset = parseInt(m[1], 10) - el.pageNum;
    counts.set(offset, (counts.get(offset) ?? 0) + 1);
  }
  let best: number | null = null;
  let bestCount = 0;
  for (const [offset, count] of counts) {
    if (count > bestCount) {
      best = offset;
      bestCount = count;
    }
  }
  return best;
}

/** An emphasis run: a delimiter, content carrying no delimiter, the same
 *  delimiter again. The parser stamps one run per source line, so runs are
 *  never nested and this stays a safe match. */
const EMPHASIS_RUN = /(\*{1,3}|_)([^*_]+?)\1/g;
/** A slash sitting at a run's edge, with any space it brought along. */
const EDGE_SLASH_LEAD = /^\s*\/\s*/;
const EDGE_SLASH_TRAIL = /\s*\/\s*$/;

/**
 * Fountain treats `/*` … `*​/` as a boneyard comment and DELETES everything
 * between them. Neither delimiter is ever written on purpose: emphasis is
 * stamped one run per source line, so a lyric's `/` separator landing at a
 * run's edge puts the delimiter hard against the slash. A 110-page script
 * lost 63% of itself to exactly this, silently, because stage 1 was
 * correct and fountain-js ate the file afterwards.
 *
 * The slash moves OUT of the run rather than getting a space beside it:
 * Fountain forbids whitespace inside the delimiters, so `*text *` stops
 * italicizing and renders literal asterisks. Moving it keeps both the
 * slash and the emphasis.
 *
 * The final sweep is the part that matters. Emphasis is cosmetic and the
 * artifact is not, so if any delimiter pair survives the rewrite, every
 * marker on the line goes and the words stay.
 */
export function disarmBoneyard(styled: string): string {
  const moved = styled.replace(EMPHASIS_RUN, (whole, delim: string, body: string) => {
    const lead = EDGE_SLASH_LEAD.test(body);
    const trail = EDGE_SLASH_TRAIL.test(body);
    if (!lead && !trail) return whole;
    let inner = body;
    if (lead) inner = inner.replace(EDGE_SLASH_LEAD, '');
    if (trail) inner = inner.replace(EDGE_SLASH_TRAIL, '');
    // A run that was nothing but a slash keeps the slash and loses the
    // emphasis it had no text to carry.
    if (!inner.trim()) return ' / ';
    return `${lead ? ' / ' : ''}${delim}${inner}${delim}${trail ? ' / ' : ''}`;
  });
  if (!moved.includes('/*') && !moved.includes('*/')) return moved;
  return moved.replace(/\*{1,3}|_/g, '');
}

/** Serialize a parsed screenplay to Fountain text. */
export function toFountain(
  screenplay: ParsedScreenplay,
  meta?: TitleMeta,
  format: FormatOptions = DEFAULT_FORMAT_OPTIONS,
): string {
  const out: string[] = [];

  if (meta?.title || meta?.author) {
    const head: string[] = [];
    if (meta.title) head.push(`Title: ${meta.title}`);
    if (meta.author) {
      head.push('Credit: Written by');
      head.push(`Author: ${meta.author}`);
    }
    out.push(head.join('\n'));
  }

  const pageOffset = format.showPageMarkers ? printedPageOffset(screenplay.elements) ?? 0 : 0;
  let lastMarkedPage = Number.NEGATIVE_INFINITY;

  // Blocks joined by blank lines; an open dialogue block accumulates
  // cue/parenthetical/dialogue lines until a non-dialogue element closes it.
  // lastSpeaker powers contdMode 'auto': same speaker continuing through
  // action gets (CONT'D); scene and transition boundaries reset it.
  let lastSpeaker: string | null = null;
  let block: string[] | null = null;
  // Image notes that arrived while a dialogue block was open, waiting for it
  // to close. See `case 'image'` for why they cannot be written in place.
  //
  // INVARIANT: empty whenever `block` is null. `case 'image'` only defers into
  // an OPEN block and writes straight to `out` otherwise, and closeBlock — the
  // one place that nulls `block` — drains this in the same breath. That is
  // what makes the flush order below correct without a second check, and what
  // stops a note escaping the scene it belongs to: every heading is pushed
  // after a closeBlock, so the drain always beats it.
  const deferredImages: string[] = [];
  const closeBlock = () => {
    if (block && block.length > 0) out.push(block.join('\n'));
    block = null;
    out.push(...deferredImages);
    deferredImages.length = 0;
  };

  for (const el of prepare(screenplay.elements, format.rejoinSplitDialogue)) {
    const text = el.text.trim();
    if (!text) continue;

    // Page markers land only at block boundaries — never inside a speech
    // (a page turning mid-speech defers its marker to the next block), and
    // never on a plate. See `anchorsPageMarker` for which types those are and
    // why the list is written the way round it is.
    if (format.showPageMarkers && anchorsPageMarker(el) && el.pageNum > lastMarkedPage) {
      const label = el.pageNum + pageOffset;
      if (label >= 1 && lastMarkedPage !== Number.NEGATIVE_INFINITY) {
        closeBlock();
        out.push(`= pg ${label}`);
      }
      lastMarkedPage = el.pageNum;
    }

    switch (el.type) {
      case 'character': {
        closeBlock();
        let cueText = text.replace(/\s+/g, ' ');
        if (format.contdMode !== 'keep') {
          cueText = cueText.replace(CONTD, '').replace(/\s+/g, ' ').trim();
          if (format.contdMode === 'auto' && el.character && el.character === lastSpeaker) {
            cueText = `${cueText} (CONT'D)`;
          }
        }
        const caret = el.dualRight && format.dualDialogue === 'sideBySide' ? ' ^' : '';
        block = [`@${cueText}${caret}`];
        lastSpeaker = el.character ?? null;
        break;
      }
      case 'parenthetical':
        // markers would break "(...)" recognition — always plain
        if (block) block.push(text);
        else out.push(text);
        break;
      case 'dialogue': {
        const line = disarmBoneyard((el.styledText ?? el.text).trim());
        const note = fmtNote(el.fmt);
        const withNote = note ? `${note} ${line}` : line;
        if (block) block.push(withNote);
        else out.push(withNote); // stray dialogue without a cue reads as action
        break;
      }
      case 'scene':
        closeBlock();
        lastSpeaker = null;
        out.push(el.sceneNumber ? `${text} #${el.sceneNumber}#` : text);
        break;
      case 'transition':
        closeBlock();
        lastSpeaker = null;
        out.push(`> ${text}`);
        break;
      case 'mini-slug':
        // A secondary slugline ("LATER", "THE KITCHEN") is a heading, and
        // Fountain's word for a heading that doesn't open INT./EXT. is the
        // forced form. Plain text would not survive the round trip: bare
        // "LATER" re-parses as action (the mini-slug styling never applied)
        // and "BACK TO:" as a transition. Stays PLAIN — emphasis markers
        // would break slug recognition, same rule as cues and parentheticals.
        //
        // A LEADING DOT is the only text the dot-force can't carry
        // (fountain-js's rule is `^\s*\.(?!\.+)`), so ".45 ON THE COUNTER"
        // falls back to forced action; every other marker rides through the
        // dot intact and keeps its heading treatment.
        //
        // Resets lastSpeaker like scene and transition do: a mini-slug is a
        // cut in time or place, so the speaker after it is starting fresh
        // and must not inherit a (CONT'D) from before it (#8a).
        closeBlock();
        lastSpeaker = null;
        out.push(/^\./.test(text) ? `!${text}` : `.${text}`);
        break;
      case 'image': {
        // Its own block with blank lines either side, which is what makes
        // fountain-js emit a standalone `note` token rather than folding it
        // into a neighbouring paragraph the way the fmt notes are folded.
        //
        // Unless a dialogue block is open, and then it WAITS. A cue alone in
        // its own paragraph is not a cue: fountain-js re-parses "@BUNNY\n\n"
        // as action and prints the "@" as literal text, so breaking a speech
        // here to keep a picture in place costs the speech AND shows a marker
        // to the reader. `prepare`'s lookahead is the other half of the same
        // decision: it stops the cue being demoted, this stops the block being
        // split. The picture lands immediately after the speech it interrupted
        // — a few lines lower than the PDF drew it, which is the whole cost.
        if (!el.img) break;
        const note = imgNote(el.img);
        if (block) deferredImages.push(note);
        else out.push(note);
        break;
      }
      default: {
        // action, and the only type left — every other member of ElementType
        // has a case above except `page-number`, which never arrives because
        // isBody drops it. It is also the only type here besides dialogue
        // allowed to carry a styled variant or an fmt note; anything else that
        // ever lands here falls back to plain and unmarked, which is what the
        // fallback is for.
        closeBlock();
        const body = el.type === 'action'
          ? disarmBoneyard((el.styledText ?? el.text).trim())
          : text;
        const forced = NEEDS_FORCE.test(body) ? `!${body}` : body;
        const note = el.type === 'action' ? fmtNote(el.fmt) : null;
        // Glued directly above with a single newline and NO blank line: that
        // is what makes fountain-js fold the note into the block's own token
        // instead of emitting a standalone one.
        out.push(note ? `${note}\n${forced}` : forced);
      }
    }
  }
  closeBlock();

  return out.join('\n\n') + '\n';
}
