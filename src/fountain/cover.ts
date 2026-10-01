// The book's cover, chosen once for every renderer.
//
// Third module of the slug.ts / notes.ts kind: which picture is the face of
// the book is not a rendering decision, and two copies of the rule would let
// the EPUB and the MOBI of one script disagree about it.

import type { Token } from 'fountain-js';
import type { FormatOptions } from '../options';
import { rendersImage } from './notes';

/** The picture a book wears as its cover, and the note that placed it. */
export interface Cover {
  /** The picture's filename, as its note names it. */
  file: string;
  /**
   * The note itself. A renderer that leaves the cover out of its flow leaves
   * out THIS note, not every note naming the same file: a hand-written script
   * may show its card again in act three, and it is asking for it there.
   */
  token: Token;
}

/**
 * The designed title card, if this script opens with one: the cover.
 *
 * The rule is about POSITION, never page number. Walking the body in order,
 * the cover is a picture note that:
 * - is the first thing the body would show. Notes that draw nothing are
 *   passed over (a hand-written note, a picture the option leaves out, one
 *   whose file is missing or whose name the allowlist refuses), and so is the
 *   title page block, which is not body content. ANY other token ahead of
 *   it means no cover: a line of text, a heading of either kind, and also
 *   the tokens that draw nothing but are still not notes, such as a page
 *   marker, a synopsis, a section or a forced page break. The renderer opens
 *   the "Opening" section for those, so a card behind one would sit in a
 *   section the cover then left empty. Our own writer never puts one there;
 *   a hand-written file that does keeps its card in the flow, as before
 *   covers.
 * - renders, by `rendersImage`'s answer, so the picker can never choose a
 *   picture the book cannot ship. That covers includeImages, the allowlist,
 *   and `have`, the pictures the caller holds bytes for.
 * - is full-page. The first picture decides: a smaller picture ahead of a
 *   full one means this script has no designed title card, because the book
 *   would open on the smaller one.
 *
 * A script with no heading at all can still have a cover: "before the first
 * heading" is the whole document then.
 *
 * Off unless promoteCoverImage and includeImages are both on. The second
 * is rendersImage's own gate, stated here as well because a cover with
 * pictures off is no cover at all, whatever the first note says.
 */
export function pickCover(
  tokens: readonly Token[],
  format: FormatOptions,
  have: ReadonlySet<string>,
): Cover | undefined {
  if (!format.promoteCoverImage || !format.includeImages) return undefined;
  for (const t of tokens) {
    if (t.is_title) continue;
    if (t.type !== 'note') return undefined;
    const img = rendersImage(t.text ?? '', format.includeImages, have);
    if (!img) continue;
    return img.fullPage ? { file: img.file, token: t } : undefined;
  }
  return undefined;
}
