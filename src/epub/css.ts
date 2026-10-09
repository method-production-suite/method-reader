// Reflowable screenplay stylesheet, Kindle-safe, parameterized by
// FormatOptions (docs/formatting-options-log.md is the knob registry).
//
// Geometry rules (see docs/screenplay-format-reference.md for the math):
// - HORIZONTAL structure in % — Kindle strips max-width and prescribes
//   percentage side margins; % scales with screen width.
// - VERTICAL rhythm in em — scales with the reader's font size; print's
//   one-blank-Courier-line between elements = 1em.
// - No line-height/font-size overrides on body text — Enhanced Typesetting
//   owns within-paragraph spacing (the reader's own setting).
// - NO background-color on html or body, ever — the KFX converter then
//   synthesizes a wrapper block of its own and every keep in the book
//   dies silently. It is the SYNTHESIZED wrapper that breaks them, not
//   nesting: our own keeps sit two divs deep and held on device (#8b).
// - CSS value SYNTAX stays CSS-2.1-vintage — no min()/clamp()/var(), which
//   can blank a whole book on Adobe RMSDK. CSS3 PROPERTIES that degrade
//   harmlessly are fine; opacity on span.page-marker is the precedent.
//
// Break control is deliberately small: every avoid link grows the
// unbreakable chunk a renderer pushes, which reads as a bottom-of-page
// gap. The whole inventory, one line each — add nothing without a reason
// good enough to write here, and the guard test in tests/epub.test.ts
// will make you.
//     break-after: avoid   h2.scene-heading (gated: keepSceneHeadingWithScene, on)
//     break-after: avoid   p.character
//     break-after: avoid   p.parenthetical (the cue chain, registry #8b)
//     break-after: avoid   p.mini-slug (live: secondary sluglines, registry #5b)
//     break-before: avoid  p.transition
//     break-inside: avoid  table.dual-dialogue
//     break-inside: avoid  .dialogue-block (gated: keepSpeechesWhole, off)
// The cue keep is a CHAIN on those two paragraphs, not a wrapper. A
// `.keep-together` div used to carry it; the KFX converter behind
// Send-to-Kindle honors break-after on text-bearing elements (#5a, proved
// on h2.scene-heading) but not on a structural div, and does not propagate
// a last child's break-after to its parent, so the wrapper→dialogue
// boundary went ungoverned and cues stranded on device (#8b, 2026-09-14).
// Three mechanisms sit outside that list. None grows a chunk the way an
// avoid link does, because each ends a page rather than refusing to; what
// they cost is white space instead:
//     page-break-before: always  section.scene (gated: scenePageBreaks, off)
//       — up to a page of white at every scene, which is why it is opt-in.
//     page-break-before + -after: always  p.figure-full (only in a book that
//       shows a figure, registry #19) — the same shape around a plate that
//       filled its printed page, and the same cost, paid only where the
//       script itself gave a picture a page. It overrides the scene heading's
//       break-after: avoid, so a heading directly above a plate ends its page
//       alone; CSS 2.1 has no rule that keeps the two together once the plate
//       outgrows what is left of the page.
//     widows/orphans on p.action and p.dialogue (gated: printSplitMinimums,
//       on, registry #17) — bounded to about one line per seam, and the one
//       rule here whose OFF state emits a value (1) rather than nothing.
// And the column-spelling shadow rule, which re-lists the wrapper keeps
// above in -webkit-column-break-inside for engines that honor only that
// spelling. It is a second spelling of existing keeps, not a new one, but
// it does newly make those wrappers unbreakable on engines where nothing
// kept before, with the same gap cost. Its selector list is DERIVED from
// the keep set (see columnKeeps below), so it cannot drift. It re-spells
// keeps only: neither forced break above has a column spelling.
//
// The figure rules (registry #19) are in the sheet only when the body shows
// a figure, so a script without pictures gets the sheet it always got, byte
// for byte. Their reasons live here rather than beside them because every
// comment down there SHIPS, inlined into the preview besides:
//   1. The width is a CLASS, p.w5 to p.w100 in steps of five, not an inline
//      style: the reader preview is an iframe whose CSP refuses `style=`
//      attributes, and twenty short rules serve the book and the preview
//      from one markup path. An explicit width on a replaced element IS its
//      used width, so w100 is exactly the text column and nothing can
//      overflow it; no max-width is missed (and Kindle strips one anyway).
//   2. No keep on .figure, deliberately. The box holds exactly one line box,
//      so there is no break opportunity inside it to forbid, and when it is
//      taller than the page `avoid` yields. Measured 2026-08-11 on Blink at
//      6x8in: 17 seam offsets, keep against no keep, identical page counts
//      and identical text on every page. NOT because an image cannot be
//      split: it can (note 4), and docs/device-map.md §6 has KF8 honoring
//      break-inside: avoid on images.
//   3. p.figure-full's forced breaks sit on the <p>, never a wrapper, for
//      the reason the cue chain does (#8b). At the very start or end of the
//      book they have nothing to break from, and Blink suppresses both
//      (measured 2026-08-11). The plate also has NO vertical margin: after a
//      forced break a top margin is kept, so a plate within about 1em of the
//      page height no longer fit beneath it and Blink pushed it to the next
//      page, leaving a blank one in front (measured 2026-10-01 on images.pdf's
//      title card at 6x8in, and with a heading above a mid-book plate).
//      Unverified on KFX and RMSDK: a device-pass line, not a fact.
//   4. Height is unbounded. The class caps the width; the height is `auto`,
//      and max-height is no remedy, since Kindle strips it. A plate taller
//      than the page slices across pages on Blink (a 1:2 picture at w100
//      spans two at 6x8in, keep or no keep). Whether KFX and RMSDK slice,
//      scale or clip is the second device-pass line.
//
// Which renderer honors what, with sources: docs/device-map.md §6.
import type { FormatOptions } from '../options';
import { DEFAULT_FORMAT_OPTIONS } from '../options';

const FONT_STACKS: Record<FormatOptions['fontFamily'], string> = {
  courier: '"Courier Prime", "Courier New", Courier, monospace',
  serif: 'serif',
  sans: 'sans-serif',
};

const em = (v: number) => `${Number(v.toFixed(2))}em`;

/** The widths a figure class can name: html.ts rounds a note's width to the
 * nearest five inside 5..100, so these twenty steps are every class it
 * writes. tests/epub.test.ts runs the renderer over every width to hold the
 * two files to one set. */
const FIGURE_WIDTHS = Array.from({ length: 20 }, (_, i) => (i + 1) * 5);

/**
 * The stylesheet for one book.
 *
 * `hasFigures` says whether the body shows at least one picture. Each caller
 * derives it from the BookBody it is styling (`usedImages.size > 0`) rather
 * than threading it through FormatOptions: it is a fact about this book, not
 * a choice a reader makes. False by default, so every caller that does not
 * ask gets, byte for byte, the sheet a book without pictures always had.
 */
export function screenplayCss(
  o: FormatOptions,
  { hasFigures = false }: { hasFigures?: boolean } = {},
): string {
  const gap = o.elementSpacingEm;
  const sceneBreak = o.scenePageBreaks
    ? '\nsection.scene { page-break-before: always; }\n'
    : '';
  // Screenplays are traditionally ragged-right; explicit `left` overrides
  // a reader that justifies body text by default (stretchy word gaps).
  const bodyAlign = o.justifyText ? 'justify' : 'left';
  // Atomic speeches (registry #8c): opt-in, because an unbreakable block
  // taller than the space left on a page gets pushed whole — bounded
  // white space traded for never splitting a speech. `avoid` yields where
  // it cannot be honored, so a speech taller than a full page still
  // breaks bare even with this on.
  const speechKeep = o.keepSpeechesWhole
    ? '\n  page-break-inside: avoid;\n  break-inside: avoid;'
    : '';
  // Every wrapper keep the stylesheet emits, in one list, so the column
  // spelling below is DERIVED from the keep set rather than a second copy
  // of it kept in sync by hand. A keep missing from here is inert in
  // Apple Books, which honors only that spelling.
  const columnKeeps = ['table.dual-dialogue'];
  if (o.keepSpeechesWhole) columnKeeps.push('.dialogue-block');
  // The same derivation for the two directional binds. These had no column
  // spelling at all until 2026-09-14, so every keep-with-next rule in the
  // book was inert on Apple Books and the Readium family while the INSIDE
  // keeps worked — the identical silent-inertness the derived-list
  // correction was written to prevent, hiding one property along.
  const columnAfter = o.keepSceneHeadingWithScene ? ['h2.scene-heading'] : [];
  columnAfter.push('p.mini-slug', 'p.character', 'p.parenthetical');
  const columnBefore = ['p.transition'];
  // The heading keep is a CHAIN, not a wrapper: break-after on the h2
  // holds it to whatever follows, without making the whole first block
  // unbreakable (the old wrapper pushed half-page chunks; registry #5a).
  const headingKeep = o.keepSceneHeadingWithScene
    ? '\n  page-break-after: avoid;\n  break-after: avoid;'
    : '';
  // Print split minimums, the two-line rule at page edges (registry #17,
  // which carries the device support and the sources). Two facts that
  // belong at this line: CSS's own initial value is already 2, so ON is a
  // DEFENSIVE restatement meant to beat a reading system that packs
  // tighter, not an addition — and both properties inherit, so scoping to
  // these two selectors instead of body is what keeps OFF from loosening
  // p.parenthetical and p.centered as well.
  // Where it actually bites: #8b's always-on cue keep swallows the first
  // dialogue paragraph, so this fires on the TAIL paragraphs of
  // multi-paragraph speeches and on action — plus wherever that keep
  // yields, since `avoid` gives way when a wrapper outgrows the page.
  const minLines = o.printSplitMinimums ? 2 : 1;
  // The figure rules, or nothing at all. Built with its own leading newline,
  // like sceneBreak, so that when it is empty the seam between p.centered and
  // the title page is exactly the one blank line it always was.
  const figures = hasFigures
    ? `
/* A picture carried out of the source PDF (registry #19). These rules are
   only in a book that shows one. No keep, on purpose: this box holds one
   line box, so there is nothing inside it to keep together. The reasons in
   full are in src/epub/css.ts, above screenplayCss; this file is generated
   from it, so that header is not here. */
.figure {
  margin: ${em(gap)} 0;
  text-align: center;
}

/* Only the width is authored, so the height follows the aspect ratio. A
   defensive restatement of the initial value, against a reading system
   whose own stylesheet sets a height on images. */
.figure img {
  height: auto;
}

/* The width the picture had on the printed page, as a share of the text
   column to the nearest five. A class rather than an inline style, which
   the reader preview refuses. */
${FIGURE_WIDTHS.map((n) => `p.w${n} img { width: ${n}%; }`).join('\n')}

/* A plate that filled its printed page gets a page here too: before puts it
   at the top of one, after stops the next paragraph sharing what is left.
   No margin: the plate owns its page, and a top margin kept after the break
   is what pushes a page-tall plate onto the next page and leaves a blank
   one in front of it. */
p.figure-full {
  margin: 0;
  page-break-before: always;
  page-break-after: always;
}
`
    : '';

  return `
html, body {
  margin: 0;
  padding: 0;
}

body {
  font-family: ${FONT_STACKS[o.fontFamily]};
}
${sceneBreak}
h2.scene-heading {
  font-size: 1em;
  font-weight: bold;
  text-transform: uppercase;
  margin: ${em(gap * 1.6)} 0 ${em(gap)} 0;${headingKeep}
}

span.scene-number {
  font-weight: normal;
}

span.underline {
  text-decoration: underline;
}

/* Original PDF page numbers — a marginal reference, out of the flow so
   they cost no line. Horizontal in %, per the CSS invariants. Dimming is
   opacity rather than a fixed gray, which would fight dark and sepia
   themes; opacity dims against whatever the theme's text color is, and
   engines lacking it just render full strength (registry #13a). */
span.page-marker {
  float: right;
  font-size: 0.75em;
  opacity: 0.6;
  margin-left: 1%;
}

p {
  margin: 0;
  text-indent: 0;
}

p.action {
  margin: ${em(gap)} 0;
  text-align: ${bodyAlign};
  widows: ${minLines};
  orphans: ${minLines};
}

p.mini-slug {
  font-weight: bold;
  text-transform: uppercase;
  margin: ${em(gap * 1.4)} 0 ${em(gap)} 0;
  page-break-after: avoid;
  break-after: avoid;
}

/* Print geometry, reflowed: dialogue is a narrow column (58% of the body
   column in print). Cue and parenthetical indent WITHIN the column as
   percentages of it (print: cue +1.2" and parenthetical +0.6" into the
   3.5" column). */
.dialogue-block {
  margin-top: ${em(gap)};
  margin-bottom: ${em(gap)};
  margin-left: ${o.dialogueSideMarginPct}%;
  margin-right: ${o.dialogueSideMarginPct}%;${speechKeep}
}

/* A cue must never orphan from its dialogue at a page break. Centered
   alignment reads naturally at any screen width; indented reproduces
   print's fixed offsets (which only optically center on paper). */
p.character {
${o.cueAlignment === 'centered'
    ? '  text-align: center;'
    : `  margin-left: ${o.cueIndentPct}%;`}
  page-break-after: avoid;
  break-after: avoid;
}

/* Chained forward as of 2026-09-14, reversing a deliberate refusal. #5 is
   right that every avoid link grows the chunk a renderer pushes, and while
   the cue keep was a WRAPPER that argument held: the chunk already carried
   the cue and the speech, and one more link made a blank-bottom page. With
   the wrapper gone the chunk is cue + parenthetical + the orphans minimum
   (#17), about four lines, which is the same bound #5a accepted when it
   traded the heading wrapper for a chain.

   Without this link a cue and its parenthetical strand together, which is
   the defect #8b exists to prevent wearing a different hat. */
p.parenthetical {
${o.cueAlignment === 'centered'
    ? '  text-align: center;'
    : `  margin-left: ${o.parentheticalIndentPct}%;\n  margin-right: ${Math.round(o.parentheticalIndentPct / 2)}%;`}
  page-break-after: avoid;
  break-after: avoid;
}

p.dialogue {
  margin: 0;
  text-align: ${bodyAlign};
  widows: ${minLines};
  orphans: ${minLines};
}

/* Simultaneous speech: two half-width columns, kept on one page. The
   table spans full width (wider than the dialogue column) by design. */
table.dual-dialogue {
  width: 100%;
  border-collapse: collapse;
  margin: ${em(gap)} 0;
  page-break-inside: avoid;
  break-inside: avoid;
}


table.dual-dialogue td {
  width: 50%;
  vertical-align: top;
  padding: 0 2%;
}

/* A transition belongs to the shot before it: it may end a page, never
   begin one (the universal print rule, docs/pagination-reference.md §2 —
   that doc lands from branch worktree-device-map). */
p.transition {
  text-align: right;
  text-transform: uppercase;
  margin: ${em(gap)} 0;
  page-break-before: avoid;
  break-before: avoid;
}

p.centered {
  text-align: center;
  margin: ${em(gap)} 0;
}
${figures}
/* Title page */
section.titlepage {
  text-align: center;
  margin-top: 20%;
}

section.titlepage h1 {
  font-size: 1.6em;
  text-transform: uppercase;
  font-weight: bold;
  margin-bottom: 2em;
}

section.titlepage p.credit {
  margin: 0.4em 0;
}

section.titlepage p.author {
  font-size: 1.1em;
  margin: 0.4em 0;
}

/* Block font shifts the PDF carried (registry #18), gated by
   preserveFontShifts. Sizes are em, per the vertical-in-em invariant, and
   carry no line-height. A family class sits on the paragraph itself, so it
   locally overrides the body fontFamily option (#6) without !important.
   Values stay CSS-2.1-vintage: RMSDK can blank a whole book on a value
   function it cannot parse. */
.fmt-mono {
  font-family: ${FONT_STACKS.courier};
}

.fmt-serif {
  font-family: ${FONT_STACKS.serif};
}

.fmt-sans {
  font-family: ${FONT_STACKS.sans};
}

.fmt-cursive {
  font-family: "Comic Sans MS", cursive;
}

.fmt-minus1 {
  font-size: 0.85em;
}

.fmt-plus1 {
  font-size: 1.2em;
}

.fmt-plus2 {
  font-size: 1.5em;
}

/* ── the multicol spelling of every break rule above ──────────────────

   Some engines paginate by laying the book out in CSS multi-column and
   showing one column per screen, and they read ONLY the multicol
   vocabulary for fragmentation: Apple Books, whose WebKit honors nothing
   else, and the Readium family (Thorium, Kobo's phone and tablet apps).

   SEPARATE rules on purpose, and last on purpose. Separate because iBooks
   drops BOTH spellings when they share one declaration block. Last because
   these selectors repeat ones used above, and a shadow placed first would
   make ruleFor() in the tests answer with the spelling nothing else reads
   (the incident 3abeba3 is named for). Unguarded by @supports on purpose:
   the engines that need this largely predate it.

   Derived from the same gating that emits the real rules, never a second
   list kept by hand. A hand-kept copy once left the whole-speech keep
   silently inert in Apple Books while the cue keep worked. */
${columnKeeps.join(', ')} { -webkit-column-break-inside: avoid; }

${columnAfter.join(', ')} { -webkit-column-break-after: avoid; }

${columnBefore.join(', ')} { -webkit-column-break-before: avoid; }
`.trimStart();
}

export const SCREENPLAY_CSS = screenplayCss(DEFAULT_FORMAT_OPTIONS);
