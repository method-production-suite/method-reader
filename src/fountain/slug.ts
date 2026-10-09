// Stage-2 mini-slug classification, shared by the renderers that walk
// fountain-js tokens (EPUB and MOBI, and src/fdx/, where a mini-slug
// becomes a Shot rather than a numbered Scene Heading). This is the stage-2 counterpart to
// the parser's own PRIMARY_SLUG in ../parser/classify.ts: that copy
// classifies from PDF geometry before the .fountain file exists, this one
// classifies from fountain-js tokens after it. The two stay deliberately
// separate rather than sharing one module, because the .fountain text is
// the boundary between stage 1 (PDF → parsed elements) and stage 2
// (fountain-js tokens → rendered output) — see CLAUDE.md's architecture
// notes and classify.ts's own comment above PRIMARY_SLUG.
import type { Token } from 'fountain-js';

/**
 * The openers fountain-js accepts for an UNFORCED scene heading (its own
 * `rules.scene_heading`, first alternative). A `scene_heading` token whose
 * text fails this could only have come from the forced `.SLUG` form —
 * which is how serialize.ts writes the parser's mini-slug elements, and how
 * screenwriters write secondary sluglines in Fountain by hand. Those are
 * micro-headings INSIDE a scene: bold uppercase, no section, no TOC entry.
 *
 * Note the asymmetry this creates for hand-written Fountain: a dot-forced
 * `.BLACK` renders as a mini-slug, not a scene. Screepub owns the dot-force
 * as its mini-slug carrier; the trade is recorded in docs/fountain-input.md's
 * divergence table and registry #5b.
 *
 * `classify.ts` carries the same literal (it must not mint a mini-slug this
 * would promote back to a heading) and `tests/epub.test.ts` pins the pair
 * to each other AND to fountain-js's own tokenizer.
 */
export const PRIMARY_SLUG = /^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]/i;

export function isMiniSlug(t: Token): boolean {
  return t.type === 'scene_heading' && !PRIMARY_SLUG.test(t.text ?? '');
}
