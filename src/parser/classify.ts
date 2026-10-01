import { CHARACTER_EXTENSIONS, isCueText } from './cue';
import type { ScreenplayElement } from './types';
import type { TextBlock } from './types';
import { INDENT_RANGES } from './types';
import { isBoilerplateLine } from './boilerplate';

// Regex patterns ported from v2 spec
// group.ts starts a new block on this same test, so a heading is never
// glued to the lines above it: one copy, so the line group.ts isolates and
// the line classified below as a heading cannot drift apart. Non-global.
export const SCENE_HEADING = /^(INT\.|EXT\.|INT\.\/EXT\.|I\/E\.)/;
// Shooting-script number printed in BOTH margins of the heading row itself
// ("2 EXT. WOODS - DAY 2") — extraction joins the row into one line, so the
// pair is stripped here and attached as the scene number. The same-token
// requirement keeps false positives out.
const DUAL_MARGIN_HEADING = /^(\d{1,3}[A-Z]?)\s+(.*\S)\s+\1$/;
// Wider than SCENE_HEADING above: the openers fountain-js accepts for an
// UNFORCED heading (EST., the space forms, bare I/E). Mini-slugs serialize
// as FORCED sluglines (".LATER"), and fountain-js promotes a forced line
// back to a full scene heading — section, TOC entry and all — when its text
// also matches this shape. So a block matching it must never classify as a
// mini-slug; it falls through to action instead. The identical literal
// lives in src/fountain/slug.ts as the renderers' shared discriminator,
// and tests/epub.test.ts pins the two together and against fountain-js
// itself.
export const PRIMARY_SLUG = /^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]/i;
const TRANSITION = /^[A-Z\s]+:$/;
const PAGE_NUMBER_BARE = /^\d+\.?$/;
const PAGE_NUMBER_DASHED = /^-\s*\d+\s*-$/;
const PAGE_NUMBER_LABELED = /^(?:page|p\.)\s*\d+\.?$/i;
// Shooting script scene numbers: "2.2.", "1A.", "42.", "1-A.", "A1.", "1A", "2.2"
const SCENE_NUMBER = /^\d+[A-Z]?(?:[.\-](?:\d+[A-Z]?|[A-Z]))*\.?$/;
const PARENTHETICAL = /^\([^)]+\)$/;
const PARENTHETICAL_TRUNCATED = /^\([^)]+\)\.{3}$/;
// Cue SHAPE lives in cue.ts, imported above; these moved there with it, and
// so did CHARACTER_EXTENSIONS, which classify imports back: one copy, so the
// name stripped here cannot drift from the name cue.ts judges.

// Mini-slug shape (see isMiniSlugShaped). A slugline ends bare or on a
// colon; the last character carries most of the signal. A closing paren is
// NOT in the class: the only lines it would have admitted are whole
// parentheticals, which have to be refused anyway, and no true slug in any
// fixture ends on one.
const MINI_SLUG_TAIL = /[A-Z0-9:]$/;
const TRANSITION_TAIL = /\bTO:$/;
// PRIMARY_SLUG anchors at the start, but an unpaired shooting-script number
// pushes the opener off it ("2 EXT. WOODS - DAY 3" — the dual-margin strip
// above needs MATCHING numbers, so a mismatch leaves the heading whole).
// Stripping a leading number lets the same literal judge that line too.
const LEADING_SCENE_NUMBER = /^\d{1,3}[A-Z]?\s+/;

const ACTION_POSSESSIVE = /^[A-Z][a-z]+['\u2019]s /;
const ACTION_PRONOUNS = /^(He |She |They |It |The |A |An |His |Her |Their |Its )/i;
const ACTION_VERBS =
  /(walks|turns|looks|stands|sits|moves|enters|exits|fades|drifts|floats|pays|rises)/i;
const ACTION_CAMERA = /^(INT\.|EXT\.|FADE|CUT|ANGLE|CLOSE|WIDE|PAN|ZOOM)/;

// Age variant patterns
const AGE_PREFIX = /^(YOUNG|OLD|OLDER|YOUNGER)\s+(.+)$/;
const AGE_DECADE = /^(.+?)\s*\(\s*(TWENTIES|THIRTIES|FORTIES|FIFTIES|SIXTIES|SEVENTIES|EIGHTIES)\s*\)/i;
const AGE_NUMBER = /^(.+?)\s*[(,]\s*(?:AGE\s+)?(\d+)/i;
const AGE_STAGE = /^(.+?)\s*\(\s*(CHILD|TEEN|ADULT|ELDERLY)\s*\)/i;
const AGE_LITTLE = /^(LITTLE|BABY)\s+(.+)$/;

// Fallback counter used only when classifyBlock is called directly without an
// explicit `nextId` (e.g. unit tests exercising classification in isolation).
// Real parses — `parseLines` in index.ts, which is the only one; there is no
// `parse()` and has not been — thread their own closure-scoped counter through
// `nextId` instead, so concurrent parseLines calls never share state.
let defaultElementCounter = 0;

export function resetCounter(): void {
  defaultElementCounter = 0;
}

/**
 * Classify a text block into a screenplay element type.
 * Stateless — takes prevElement instead of tracking internal state.
 * Priority chain: page-number -> scene -> transition -> character -> parenthetical -> action -> dialogue -> mini-slug -> default
 */
export function classifyBlock(
  block: TextBlock,
  prevElement: ScreenplayElement | null,
  nextId: () => string = () => `elem${defaultElementCounter++}`,
): ScreenplayElement {
  const { text, indent } = block;
  const id = nextId();
  // fmt rides from block to element mechanically and is NEVER a classification
  // input: the parser stays format-option-free (registry #18).
  // `y` rides across mechanically like `fmt`, and for the same reason it is
  // never a classification input: it is where the block was, not what it is.
  // `yPosition` is the block's FIRST line, which extraction orders topmost-
  // first — so it is the BASELINE of the block's top line, not the block's top
  // edge, and index.ts's merge is written knowing that.
  const base: ScreenplayElement = { id, type: 'action', text, styledText: block.styledText, fmt: block.fmt, dualRight: block.dualRight, pageNum: block.pageNum, y: block.yPosition, isTitlePage: false, isReadable: true };

  // Get current character from previous element chain
  const currentCharacter = getActiveCharacter(prevElement);

  // Priority 1: Page number
  // Bare numbers (e.g. "42", "42.") are page numbers at any indent — a standalone
  // number block is almost never meaningful dialogue or action.
  // Dashed ("- 42 -") and labeled ("Page 42") formats require moderate indent.
  const trimmed = text.trim();
  if (PAGE_NUMBER_BARE.test(trimmed)) {
    return { ...base, type: 'page-number' };
  }
  if (PAGE_NUMBER_DASHED.test(trimmed) || PAGE_NUMBER_LABELED.test(trimmed)) {
    return { ...base, type: 'page-number' };
  }
  // Shooting script scene numbers (e.g., "2.2.", "1A.", "42A") — classified as
  // page-number so they're hidden. The parser pipeline attaches them to the
  // following scene heading element as `sceneNumber`.
  //
  // Carrying `sceneNumber` here is what makes attachSceneNumbers able to tell
  // these apart from ordinary page numbers LATER. It cannot re-derive the
  // difference from the text: PAGE_NUMBER_BARE above already consumed "1." and
  // "42.", and a bare number is a scene number or a page number depending only
  // on which branch classified it. Without this marker, every page that opens
  // on a scene heading donates its page number to that scene.
  if (SCENE_NUMBER.test(trimmed) && trimmed.length <= 10) {
    return { ...base, type: 'page-number', sceneNumber: trimmed };
  }

  // Priority 1.5: production-draft furniture (revision slugs, draft stamps,
  // header dates) — routed to page-number so every consumer skips it.
  if (isBoilerplateLine(trimmed)) {
    return { ...base, type: 'page-number' };
  }

  // Priority 2: Scene heading (no indent check — most reliable)
  if (SCENE_HEADING.test(text)) {
    return { ...base, type: 'scene' };
  }
  const dualMargin = trimmed.match(DUAL_MARGIN_HEADING);
  if (dualMargin && SCENE_HEADING.test(dualMargin[2])) {
    return { ...base, type: 'scene', text: dualMargin[2], sceneNumber: dualMargin[1] };
  }

  // Priority 3: Transition
  if (indent > INDENT_RANGES.TRANSITION_MIN && TRANSITION.test(text)) {
    return { ...base, type: 'transition' };
  }

  // Priority 4: Truncated character name
  if (text.endsWith('...') && indent >= INDENT_RANGES.CHARACTER_MIN && indent <= INDENT_RANGES.CHARACTER_MAX) {
    const stripped = text.replace(/\.{3}$/, '');
    if (isLikelyCharacterName(stripped, indent)) {
      const { name, baseName, ageModifier, ageValue } = extractCharacterInfo(stripped);
      return { ...base, type: 'character', text: stripped, character: name, baseCharacter: baseName, ageModifier, ageValue };
    }
  }

  // Priority 5: Character name
  if (indent >= INDENT_RANGES.CHARACTER_MIN && indent <= INDENT_RANGES.CHARACTER_MAX && isLikelyCharacterName(text, indent)) {
    const { name, baseName, ageModifier, ageValue } = extractCharacterInfo(text);
    return { ...base, type: 'character', character: name, baseCharacter: baseName, ageModifier, ageValue };
  }

  // Priority 6: Parenthetical
  if (currentCharacter && indent >= INDENT_RANGES.DIALOGUE_MIN && indent <= 40 &&
    (PARENTHETICAL.test(text) || PARENTHETICAL_TRUNCATED.test(text))) {
    return { ...base, type: 'parenthetical', character: currentCharacter };
  }

  // Priority 7: Action by pattern
  if (indent < INDENT_RANGES.ACTION_MAX && isActionByPattern(text)) {
    return { ...base, type: 'action' };
  }

  // Priority 8: Dialogue (requires active character)
  if (currentCharacter && indent >= INDENT_RANGES.DIALOGUE_MIN && indent <= INDENT_RANGES.DIALOGUE_MAX &&
    !SCENE_HEADING.test(text) && !TRANSITION.test(text)) {
    return { ...base, type: 'dialogue', character: currentCharacter };
  }

  // Priority 9: Mini-slug (secondary slugline)
  if (isMiniSlugShaped(block)) {
    return { ...base, type: 'mini-slug' };
  }

  // Priority 10: Default to action
  return { ...base, type: 'action' };
}

/**
 * Attach scene numbers from shooting scripts to the following scene heading.
 * A page-number element immediately before a scene element is a scene number
 * — but only when its text actually matches the scene-number shape (revision
 * slugs and other boilerplate also classify as page-number and must never
 * become scene numbers). Lives here (not index.ts) so it shares the private
 * SCENE_NUMBER pattern and stays unit-testable without index.ts's pdf.js
 * import, which cannot load in the bun:test environment.
 * Mutates elements in place — sets `sceneNumber` on the scene element.
 */
export function attachSceneNumbers(elements: ScreenplayElement[]): void {
  for (let i = 0; i < elements.length - 1; i++) {
    // Only a page-number element that classifyBlock MARKED as a scene number
    // is promoted. Re-testing SCENE_NUMBER here would match a bare page
    // number ("1.", "42.") just as well, because by this point the two are
    // the same type carrying the same shape of text, and every page opening
    // on a scene heading would hand its page number to that scene.
    if (
      elements[i].type === 'page-number' &&
      elements[i + 1].type === 'scene' &&
      elements[i].sceneNumber !== undefined
    ) {
      elements[i + 1].sceneNumber = elements[i].sceneNumber;
    }
  }
}

function getActiveCharacter(prevElement: ScreenplayElement | null): string | null {
  if (!prevElement) return null;
  if (prevElement.type === 'character') return prevElement.character ?? null;
  if (prevElement.type === 'dialogue' || prevElement.type === 'parenthetical') {
    return prevElement.character ?? null;
  }
  // Action/scene/transition reset character context
  return null;
}

export function isLikelyCharacterName(text: string, indent: number): boolean {
  // Geometry here, shape in cue.ts. The indent band is this caller's own
  // concern: the dual-dialogue detector has no indent to check because
  // clusterSplit settles its geometry upstream, and forcing it to invent one
  // is how a second definition of "cue" got written and drifted.
  if (indent < INDENT_RANGES.CHARACTER_MIN || indent > INDENT_RANGES.CHARACTER_MAX) return false;
  return isCueText(text);
}

function extractCharacterInfo(text: string): {
  name: string;
  baseName: string;
  ageModifier?: string;
  ageValue?: string | number;
} {
  // Strip extensions: "JACK (V.O.) (CONT'D)" -> "JACK"
  const name = text.replace(CHARACTER_EXTENSIONS, '').trim();
  const { baseName, ageModifier, ageValue } = extractAgeVariant(name);
  return { name, baseName, ageModifier, ageValue };
}

function extractAgeVariant(name: string): {
  baseName: string;
  ageModifier?: string;
  ageValue?: string | number;
} {
  let match = AGE_PREFIX.exec(name);
  if (match) return { baseName: match[2], ageModifier: match[1], ageValue: match[1].toLowerCase() };

  match = AGE_DECADE.exec(name);
  if (match) {
    const decades: Record<string, string> = {
      twenties: '20s', thirties: '30s', forties: '40s', fifties: '50s',
      sixties: '60s', seventies: '70s', eighties: '80s',
    };
    return { baseName: match[1], ageValue: decades[match[2].toLowerCase()] || match[2] };
  }

  match = AGE_NUMBER.exec(name);
  if (match) return { baseName: match[1], ageValue: parseInt(match[2], 10) };

  match = AGE_STAGE.exec(name);
  if (match) return { baseName: match[1], ageValue: match[2].toLowerCase() };

  match = AGE_LITTLE.exec(name);
  if (match) return { baseName: match[2], ageModifier: match[1], ageValue: 'child' };

  return { baseName: name };
}

/**
 * Canonical key for comparing a candidate cue against the character
 * roster: smart quotes straightened, case folded, extensions dropped.
 * Deliberately lossy — it exists to match "dev" and "MARGO’S MOM" to
 * names the script already established, not to display.
 */
export function normalizeCueName(text: string): string {
  return text
    .replace(/[‘’ʼ]/g, "'")
    .replace(/(\s*\([^)]*\))+\s*$/, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toUpperCase();
}

/**
 * Mini-slug (secondary slugline) shape test — "END OF MONTAGE",
 * "IN THE PROJECTION BOOTH", "QUICK MONTAGE:".
 *
 * Runs LAST in the chain (priority 9), so every existing veto keeps its
 * say: a candidate that reads as a cue, a speech, or action-by-pattern
 * has already returned. This is deliberate. The band it works in also
 * holds all-caps sound effects ("THUD.", "CRASH!") and all-caps action
 * sentences ("THE ENGINE STARTS."), and a promoted sound effect renders
 * as a bold heading — a visible defect — while a missed slug renders as
 * action, which is harmless. Precision beats recall here, so the rule
 * only claims blocks that nothing else objects to.
 *
 * Every guard, and why (calibrated against the five generator fixtures,
 * 2026-07-30 — 655 all-caps blocks in the action band, 92 promoted):
 */
function isMiniSlugShaped(block: TextBlock): boolean {
  const t = block.text.trim();

  // Action band, measured from the PAGE edge — where extraction puts it.
  // The old `indent < 5` assumed a margin-relative measure that no
  // generator produces (the real modal action indent is 15-18), which is
  // why PDF input produced zero mini-slugs for as long as the type existed.
  // ACTION_MAX (20) also sits structurally below DIALOGUE_MIN (25) and
  // CHARACTER_MIN (35), so this can never fire in the cue or speech band.
  if (block.indent >= INDENT_RANGES.ACTION_MAX) return false;

  // One line, bounded by blank lines — groupBlocks breaks on a >20pt Y gap,
  // so a single-line block IS a standalone beat. Prose long enough to be a
  // sentence wraps, and a wrapped block is never a heading.
  if (block.lines.length !== 1) return false;

  // Strict all-caps. Not the cue heuristic's 0.8 ratio — a slugline is
  // typed in caps, entirely. This alone rejects the "Dev CROSSES TO THE
  // COUNTER" emphasis style that some writers use for action.
  if (t !== t.toUpperCase()) return false;

  // Observed true slugs top out at 52 characters; 55 leaves a margin
  // without opening the door to a full line of caps prose.
  if (t.length < 2 || t.length > 55) return false;

  // A slugline is words. Letters must carry the line — this is what keeps
  // a WGA registration number ("WGA 1234567 555-0100") off the heading.
  const dense = t.replace(/\s/g, '');
  if ((dense.match(/[A-Z]/g) || []).length / dense.length < 0.5) return false;

  // THE discriminator. Mini-slugs end bare or on a colon; sound effects and
  // action sentences end on terminal punctuation. Across all six fixtures
  // every sound effect and shouted beat carried a "." or "!" and every true
  // slug ended bare or on ":" — the ambiguity the type invited ("SILENCE"
  // vs "END DREAM") does not actually occur in the corpus. Also rejects the
  // trailing dash of a broken-off line, the "***" of a revision note, and
  // (via the missing paren) a bare "(BEAT)" with no speaker above it.
  if (!MINI_SLUG_TAIL.test(t)) return false;

  // Fountain's own transition rule: uppercase, ends in "TO:". The format
  // says that is a transition, so it is not a slug — this is what keeps
  // "DISSOLVE TO:" and "SMASH CUT TO:" (both left-flush in real scripts)
  // out, since the transition branch above only fires at right-flush indent.
  if (TRANSITION_TAIL.test(t)) return false;

  // PRIMARY_SLUG, not SCENE_HEADING: a mini-slug serializes as a FORCED
  // slugline, and fountain-js promotes a forced line back to a full scene
  // heading — section, TOC entry and all — whenever its text also matches
  // that wider unforced shape (EST., the space forms, bare I/E). Minting one
  // here would round-trip into a scene. Tested against the line as written
  // AND with a leading shooting-script number stripped, since that number
  // pushes the opener off the anchor.
  return !PRIMARY_SLUG.test(t) && !PRIMARY_SLUG.test(t.replace(LEADING_SCENE_NUMBER, ''));
}

function isActionByPattern(text: string): boolean {
  return (
    ACTION_POSSESSIVE.test(text) ||
    ACTION_PRONOUNS.test(text) ||
    ACTION_VERBS.test(text) ||
    ACTION_CAMERA.test(text)
  );
}
