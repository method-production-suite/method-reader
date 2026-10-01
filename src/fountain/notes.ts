// Fountain notes, shared by both renderers.
//
// The format says `[[...]]` is invisible in output. Before registry #18 we
// never emitted one and never stripped one either, so a hand-written note
// rendered as literal text — a spec violation nobody had hit. Stamping fmt
// notes into the .fountain makes stripping load-bearing, so it lives here,
// beside slug.ts, as the one copy both renderers import (the registry #5b
// precedent) rather than two regexes that drift.

import type { ImageRef, SizeStep } from '../parser/types';

/** Any note. Bracket-free inside, so an unterminated `[[` matches nothing and
 * is left in the text rather than eating the rest of the line. */
const NOTE = /\[\[[^\]]*\]\]/g;

/** A LEADING fmt note only: a note further in is a note about the text, not
 * a declaration about the block. */
const LEADING_FMT = /^\s*\[\[\s*fmt:([^\]]*)\]\]/;

const FAMILY_CLASS: Record<string, string> = {
  mono: 'fmt-mono',
  serif: 'fmt-serif',
  sans: 'fmt-sans',
  cursive: 'fmt-cursive',
};

const SIZE_CLASS: Record<string, string> = {
  '-1': 'fmt-minus1',
  '+1': 'fmt-plus1',
  '+2': 'fmt-plus2',
};

/**
 * Remove every note, then tidy the whitespace the removal left behind.
 *
 * Newlines survive: a lyrics or verse dialogue token arrives as ONE token with
 * its line breaks inside, and the EPUB splits on them to make paragraphs.
 * Text with no `[[` at all is returned identical, character for character —
 * which is why no existing script's output moves by a byte.
 */
export function stripNotes(text: string): string {
  if (!text.includes('[[')) return text;
  return text
    .replace(NOTE, '')
    .replace(/[^\S\n]{2,}/g, ' ')
    .replace(/^[^\S\n]+/gm, '')
    .trim();
}

/**
 * A leading `[[fmt: ...]]` as a class suffix, e.g. " fmt-sans fmt-plus2",
 * ready to append inside an existing class attribute. Unknown words are
 * ignored one by one: a malformed note yields no classes and never throws.
 */
export function fmtClasses(text: string): string {
  const m = LEADING_FMT.exec(text);
  if (!m) return '';
  let out = '';
  for (const word of m[1].trim().split(/\s+/)) {
    if (FAMILY_CLASS[word]) out += ` ${FAMILY_CLASS[word]}`;
    else if (SIZE_CLASS[word]) out += ` ${SIZE_CLASS[word]}`;
  }
  return out;
}

/** The size step of a leading fmt note, for renderers with no stylesheet. */
export function fmtSizeStep(text: string): SizeStep | undefined {
  const m = LEADING_FMT.exec(text);
  if (!m) return undefined;
  for (const word of m[1].trim().split(/\s+/)) {
    if (word === '-1' || word === '+1' || word === '+2') return word;
  }
  return undefined;
}

/**
 * A filename we are willing to put inside a zip.
 *
 * SECURITY BOUNDARY, not tidiness. The `.fountain` is a durable text artifact
 * a user can hand-edit or receive from someone else, and this string becomes
 * an entry path in the EPUB and a filename read off disk. The allowlist
 * excludes both separators, absolute paths, `%`-escapes and anything that is
 * not a lowercase `.png`. A name that fails is dropped, never repaired.
 *
 * What it does NOT contain is a rule about `..`, and it does not need one:
 * with no separator in the class, `..png` is a filename and not a path. Every
 * traversal string fails here on a character the class lacks (`/`, `\`, `%`),
 * which is the mechanism to check if this is ever loosened.
 *
 * The 64 counts the STEM. `pNNN-NN.png` is eleven characters, so the bound is
 * not a limit on what we emit — it is a limit on what we will read back.
 */
const SAFE_IMAGE_FILE = /^[A-Za-z0-9._-]{1,64}\.png$/;

/**
 * The same boundary, for names that arrive as bytes rather than as text.
 *
 * The picture folder, `<stem>.images/`, is read back by filename, and a
 * directory listing is no more trustworthy than a hand-edited `.fountain` —
 * less, since nobody typed it. Exported so the folder's reader
 * (src/library-images.ts) applies the one allowlist rather than a second
 * `endsWith('.png')` that would let `a&b.png` into an OPF manifest. Sharing
 * the regex is the registry #5b/#18 rule the rest of this module follows;
 * two copies of a security check drift exactly like two copies of a parser.
 */
export function isSafeImageFile(file: string): boolean {
  return SAFE_IMAGE_FILE.test(file);
}

/** An integer width word, e.g. `w=71`. Nothing else is a width: `w=71.5` and
 * `w=abc` simply do not match, and the default stands. */
const WIDTH_WORD = /^w=(-?\d+)$/;

/**
 * Parse the INNER text of a standalone image note (registry #19).
 *
 * Inner, because fountain-js hands a `note` token its contents with the
 * brackets already stripped — unlike `fmtClasses`, which reads a leading note
 * still embedded in an action or dialogue token's text and so matches the
 * brackets itself.
 *
 * Case-SENSITIVE on the keyword, matching LEADING_FMT above. We emit exactly
 * `img: `, and a note this does not recognize is not a defect a reader can
 * see: a standalone one is a `note` token, which both renderers' switches drop
 * unread, and a folded one goes through `stripNotes`. Either way it stays
 * invisible, exactly as the format promises — it simply renders no picture.
 */
export function parseImgNote(inner: string): ImageRef | undefined {
  const m = /^\s*img:\s*(\S+)(.*)$/.exec(inner);
  if (!m) return undefined;
  const file = m[1];
  if (!isSafeImageFile(file)) return undefined;

  // 100 is the fallback for a note with no width AND for a width we cannot
  // read. Both mean "as wide as the column", which is the direction
  // ImageRef.widthPct argues for: too large beats too small to see.
  let widthPct = 100;
  let fullPage = false;
  for (const word of m[2].trim().split(/\s+/)) {
    if (word === 'full') fullPage = true;
    const w = WIDTH_WORD.exec(word);
    // The clamp is the only guard, and it is load-bearing rather than tidy.
    // parseInt on `-?\d+` never returns NaN but it DOES return ±Infinity —
    // `w=` followed by four hundred nines is Infinity — so a finiteness test
    // would not be dead code, it would be redundant code: Math.min pins
    // Infinity to 100 and Math.max pins -Infinity to 5, which is the same
    // answer by the same route as `w=9999` and `w=-5`. The 5..100 range is
    // ImageRef.widthPct's, stated there.
    if (w) widthPct = Math.min(100, Math.max(5, parseInt(w[1], 10)));
  }
  return { file, widthPct, fullPage };
}
