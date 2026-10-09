// Fountain notes, shared by both renderers (and by the FDX writer in
// src/fdx/, which strips them the same way: a note is never script text).
//
// The format says `[[...]]` is invisible in output. Before registry #18 we
// never emitted one and never stripped one either, so a hand-written note
// rendered as literal text — a spec violation nobody had hit. Stamping fmt
// notes into the .fountain makes stripping load-bearing, so it lives here,
// beside slug.ts, as the one copy both renderers import (the registry #5b
// precedent) rather than two regexes that drift.

import type { SizeStep } from '../parser/types';

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
