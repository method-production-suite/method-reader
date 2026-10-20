# Rich Formatting Phase 2 (font family/size) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Carry block-level font family and size shifts — text-message inserts,
chyrons, letters, on-screen title cards — from the PDF through the `.fountain`
artifact and into EPUB/MOBI rendering.

**Architecture:** The parser measures each line's font mix, compares it to the
document's dominant (family bucket, size) weighted by character count, and stamps
an `fmt` on lines whose characters overwhelmingly agree on one deviation. Blocks
inherit `fmt` only when every line agrees, and an `fmt` change breaks a block.
`serialize.ts` emits `[[fmt: ...]]` notes — always, regardless of any option, so
the `.fountain` is stable under settings flips and the app cache re-renders
without re-parsing the PDF. Both renderers strip every `[[...]]` note (Fountain
says notes are invisible; today they render as literal text, which this fixes)
and, when `preserveFontShifts` is on, turn a leading fmt note into CSS classes.

**Tech Stack:** Bun/TypeScript engine (bun:test), fountain-js 1.2.4, jszip EPUB3,
hand-built MOBI 6 dialect, SwiftPM app (`kit-check`, no XCTest), python3 fixture
generator.

**Spec:** `../specs/2026-07-30-rich-formatting-design.md` §Phase 2.
**Umbrella:** `2026-07-30-formatting-umbrella.md` §Phase C.
**Depends on:** `2026-08-04-rich-formatting-phase1.md` (merged). Phase A
(`3fb410c`) already gave `tokensToMobiHtml` a `FormatOptions` parameter — extend
it, do not re-plumb it — and the `printSplitMinimums` commits
(`ce65963`, `445d3a1`, `5630525`) are the worked example for the knob ritual.

---

## Probe results this plan is calibrated on (2026-08-04)

**Font size is `transform[3]`, confirmed** across all six local PDFs. **Subset
prefixes are real** (`AAAAAB+CourierFinalDraft`, `WXPDAA+CourierNewPSMT`) and
must be stripped before matching.

Fonts pdf.js reports, by character count over the first 8 pages:

| PDF | fonts (chars @ size) | dominant | deviations Phase 2 will find |
|---|---|---|---|
| torture.pdf | Courier 2334@12, -Bold 105@12, -BoldOblique 20@12, -Oblique 11@12 | mono 12 | none (uniform by construction) |
| final-draft.pdf | CourierFinalDraft 8977@12 (+italic 67, +48) | mono 12 | none |
| highland.pdf | CourierPrime 5972@12, -Bold 151@12 | mono 12 | none |
| celtx.pdf | courierprime 7216@12 | mono 12 | none |
| **fade-in.pdf** | CourierNewPSMT 6902@12, **AvenirNext-Bold 105@12**, **Scream 14@36**, CourierScreenplay 12@12, CourierNewPS-ItalicMT 12@12 | mono 12 | **sans**, and **+2** (36/12 = 3.0) |

So the spec's claim that a uniform Courier screenplay produces zero fmt holds for
four of the five real scripts, and `fade-in.pdf` is a genuine live case for both
arms of the detector. Bold/italic name tokens are style, not family, and
`AvenirNext-Bold` correctly buckets to `sans`.

**One spec correction: drop bare `roman` from the serif matcher.** The spec maps
`times|georgia|garamond|roman → serif`. But `-Roman` is the standard PostScript
name for a REGULAR weight — `Helvetica-Roman`, `AvenirNext-Roman`,
`Frutiger-Roman` — so matching it buckets sans faces as serif, which is the same
class of mistake the spec already warns about for bare "gothic". `Times-Roman`
and `TimesNewRomanPSMT` both match `times` already, so dropping `roman` costs
nothing and removes the whole false-positive class.

**fountain-js 1.2.4 note handling, re-verified** (both placements the spec
requires):

```
action   "[[fmt: sans]]\nCHYRON: EVERY NAME HERE IS INVENTED."
dialogue "[[fmt: sans]] A text message, in the phone."
```

The action note keeps a **newline** between itself and the body inside one token,
so note-stripping must tidy the whitespace it leaves behind without eating the
newlines a lyrics/verse dialogue token carries.

---

## Scope bound recorded up front

**Dual-dialogue lines carry no fmt.** `deinterleaveDualDialogue` builds its two
columns as joined strings and does not retain the per-column `TextItem`s, so
attaching font stats there means restructuring the delicate column-partition pass
(registry #10a). A simultaneous exchange set in a deviant face is vanishingly
rare, and the block-level design already excludes inline runs. Task 9 records
this in the registry as a bound rather than leaving it as folklore.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/parser/types.ts` | modify | `FamilyBucket`, `SizeStep`, `Fmt`, `FontRun`; `fmt` on RawLine/TextBlock/ScreenplayElement |
| `src/parser/extract.ts` | modify | family bucketing, per-line font runs, document dominant, `fmt` stamping |
| `src/parser/group.ts` | modify | `fmt` breaks blocks; `buildBlock` sets it only on unanimity |
| `src/parser/classify.ts` | modify | one field, carried mechanically |
| `src/fountain/serialize.ts` | modify | `[[fmt: ...]]` note emission, always on |
| `src/fountain/notes.ts` | **create** | note stripping + fmt parsing, shared by both renderers (the `slug.ts` precedent, registry #5b) |
| `src/epub/html.ts` | modify | strip notes; leading fmt note → classes |
| `src/epub/css.ts` | modify | seven `fmt-*` rules |
| `src/mobi/html.ts` | modify | strip notes; size steps → `<font size>`; family dropped |
| `src/options.ts`, `format-defaults.json` | modify | `preserveFontShifts`, default true |
| app Swift (4 files) | modify | the knob ritual |
| `tools/make-fixture.py`, `tools/torture-content.py` | modify | a Helvetica resource and three size shifts |
| `docs/formatting-options-log.md`, `tools/torture-manifest.json` | modify | registry #18 + its coverage row |

---

## Task 1: family bucketing

**Files:**
- Modify: `src/parser/types.ts`
- Modify: `src/parser/extract.ts`
- Test: `tests/extract.test.ts`

- [ ] **Step 1: Write the failing test**

Add `familyBucket` to the import from `../src/parser/extract` at the top of
`tests/extract.test.ts`, then append:

```ts
describe('familyBucket', () => {
  test('screenplay monospace faces bucket as mono', () => {
    for (const n of ['Courier', 'CourierPrime-Bold', 'AAAAAB+CourierFinalDraft',
                     'WXPDAA+CourierNewPSMT', 'LetterGothic', 'Prestige Elite',
                     'Andale Mono']) {
      expect(familyBucket(n), n).toBe('mono');
    }
  });

  test('Century Gothic is a sans, not a mono', () => {
    // The trap the spec calls out: match the JOINED name "lettergothic",
    // never bare "gothic".
    expect(familyBucket('CenturyGothic')).toBe('sans');
  });

  test('serif faces bucket as serif', () => {
    for (const n of ['Times-Roman', 'TimesNewRomanPSMT', 'Georgia', 'Garamond',
                     'Palatino-Roman']) {
      expect(familyBucket(n), n).toBe('serif');
    }
  });

  test('a -Roman weight suffix does not make a sans into a serif', () => {
    // "-Roman" is PostScript for the REGULAR weight. Times matches on "times"
    // anyway, so bare "roman" buys nothing and misbuckets these.
    for (const n of ['Helvetica-Roman', 'AvenirNext-Roman', 'Frutiger-Roman']) {
      expect(familyBucket(n), n).toBe('sans');
    }
  });

  test('handwriting faces bucket as cursive', () => {
    for (const n of ['BrushScriptMT', 'BradleyHandITC', 'Comic Sans MS']) {
      expect(familyBucket(n), n).toBe('cursive');
    }
  });

  test('everything else is sans', () => {
    for (const n of ['AvenirNext-Bold', 'Helvetica', 'Scream', 'Arial-BoldMT']) {
      expect(familyBucket(n), n).toBe('sans');
    }
  });

  test('style tokens never decide the bucket', () => {
    expect(familyBucket('Courier-BoldOblique')).toBe('mono');
    expect(familyBucket('Georgia-Italic')).toBe('serif');
  });

  test('an empty or missing name has no bucket', () => {
    expect(familyBucket('')).toBeUndefined();
    expect(familyBucket('AAAAAB+')).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/extract.test.ts`
Expected: FAIL — `familyBucket` is not exported.

- [ ] **Step 3: Implement**

In `src/parser/types.ts`, add above `export interface ScreenplayElement`:

```ts
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
```

In `src/parser/extract.ts`, add `Fmt`, `FamilyBucket`, `FontRun`, `SizeStep` to
the type import from `./types`, and add this block just above `stampFontStyles`:

```ts
/**
 * PostScript base name → coarse family bucket.
 *
 * Matched against the name lowercased with its subset prefix ("AAAAAB+") and
 * every non-alphanumeric stripped, so "Letter Gothic" and "LetterGothic" both
 * read as "lettergothic". Two traps this shape avoids:
 *   - bare "gothic" would swallow Century Gothic, which is a sans;
 *   - bare "roman" would swallow Helvetica-Roman and AvenirNext-Roman, where
 *     "-Roman" is the REGULAR weight, not the family. Times matches "times".
 * Weight and slope tokens (bold, black, heavy, italic, oblique) match nothing
 * here on purpose: they are style, and style is registry 9d's business.
 */
const FAMILY_PATTERNS: [RegExp, FamilyBucket][] = [
  [/courier|mono|lettergothic|prestige/, 'mono'],
  [/times|georgia|garamond|palatino|caslon|baskerville|minion/, 'serif'],
  [/script|hand|brush|comic/, 'cursive'],
];

export function familyBucket(name: string): FamilyBucket | undefined {
  const n = String(name ?? '')
    .replace(/^[A-Z]{6}\+/, '')
    .toLowerCase()
    .replace(/[^a-z0-9]/g, '');
  if (!n) return undefined;
  for (const [re, bucket] of FAMILY_PATTERNS) if (re.test(n)) return bucket;
  return 'sans';
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/extract.test.ts && bunx tsc --noEmit`
Expected: PASS, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/parser/types.ts src/parser/extract.ts tests/extract.test.ts
git commit -m "Font names bucket into four families, with the gothic and Roman traps disarmed"
```

## Task 2: the document dominant and per-line fmt

**Files:**
- Modify: `src/parser/extract.ts`
- Modify: `src/parser/types.ts`
- Test: `tests/extract.test.ts`

- [ ] **Step 1: Write the failing test**

Add `stampLineFmt` to the extract import, then append to `tests/extract.test.ts`:

```ts
describe('stampLineFmt', () => {
  const line = (runs: { bucket: string; size: number; chars: number }[]) =>
    ({ text: 'x', indent: 10, y: 700, pageNum: 1, fonts: runs }) as never;

  const body = (n: number) => line([{ bucket: 'mono', size: 12, chars: n }]);

  test('a uniform document produces no fmt anywhere', () => {
    const lines = [body(500), body(500), body(500)];
    stampLineFmt(lines);
    expect(lines.map((l) => l.fmt)).toEqual([undefined, undefined, undefined]);
  });

  test('a line in another family gets a family fmt', () => {
    const shifted = line([{ bucket: 'sans', size: 12, chars: 30 }]);
    const lines = [body(500), shifted, body(500)];
    stampLineFmt(lines);
    expect(shifted.fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('the three size steps come off the ratio, both directions', () => {
    const smaller = line([{ bucket: 'mono', size: 10, chars: 30 }]);   // 0.83
    const bigger = line([{ bucket: 'mono', size: 15, chars: 30 }]);    // 1.25
    const biggest = line([{ bucket: 'mono', size: 18, chars: 30 }]);   // 1.50
    const lines = [body(500), smaller, bigger, biggest, body(500)];
    stampLineFmt(lines);
    expect(smaller.fmt).toEqual({ family: undefined, size: '-1' });
    expect(bigger.fmt).toEqual({ family: undefined, size: '+1' });
    expect(biggest.fmt).toEqual({ family: undefined, size: '+2' });
  });

  test('a shift in both arms reports both', () => {
    const both = line([{ bucket: 'sans', size: 18, chars: 30 }]);
    const lines = [body(500), both];
    stampLineFmt(lines);
    expect(both.fmt).toEqual({ family: 'sans', size: '+2' });
  });

  test('float jitter under 1.5pt never fires', () => {
    // 12.9/12 = 1.075, under the ratio floor anyway; 13.4/12 = 1.117 is also
    // under it. The absolute delta is the belt to the ratio's braces.
    const jittery = line([{ bucket: 'mono', size: 13.4, chars: 30 }]);
    const lines = [body(500), jittery];
    stampLineFmt(lines);
    expect(jittery.fmt).toBeUndefined();
  });

  test('a ratio between the steps is not a step', () => {
    // 13.5/12 = 1.125: bigger than jitter, smaller than +1's 1.15 floor.
    const between = line([{ bucket: 'mono', size: 13.5, chars: 30 }]);
    stampLineFmt([body(500), between]);
    expect(between.fmt).toBeUndefined();
  });

  test('a line only half in the deviant font gets nothing', () => {
    const mixed = line([
      { bucket: 'mono', size: 12, chars: 20 },
      { bucket: 'sans', size: 12, chars: 20 },
    ]);
    stampLineFmt([body(500), mixed]);
    expect(mixed.fmt).toBeUndefined();
  });

  test('80% agreement is enough', () => {
    const mostly = line([
      { bucket: 'mono', size: 12, chars: 2 },
      { bucket: 'sans', size: 12, chars: 8 },
    ]);
    stampLineFmt([body(500), mostly]);
    expect(mostly.fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('the dominant is by character weight, not by line count', () => {
    // Many short sans lines lose to one long mono block.
    const sansLines = Array.from({ length: 20 }, () =>
      line([{ bucket: 'sans', size: 12, chars: 5 }]));
    const monoLine = line([{ bucket: 'mono', size: 12, chars: 900 }]);
    stampLineFmt([...sansLines, monoLine]);
    expect(monoLine.fmt).toBeUndefined();
    expect(sansLines[0].fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('lines with no resolved fonts are left alone', () => {
    const bare = { text: 'x', indent: 10, y: 700, pageNum: 1 } as never;
    stampLineFmt([body(500), bare]);
    expect((bare as { fmt?: unknown }).fmt).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/extract.test.ts`
Expected: FAIL — `stampLineFmt` is not exported.

- [ ] **Step 3: Implement**

In `src/parser/types.ts`, add to `RawLine` after `dualRight`:

```ts
  /** per-run font tallies, consumed by the document-dominant pass */
  fonts?: FontRun[];
  /** block-level font shift relative to the document's dominant font */
  fmt?: Fmt;
```

and to `TextBlock` after `dualRight`:

```ts
  fmt?: Fmt;
```

and to `ScreenplayElement` after `dualRight`:

```ts
  /** block-level font shift, serialized as a [[fmt: ...]] note (registry #18) */
  fmt?: Fmt;
```

In `src/parser/extract.ts`, add below `familyBucket`:

```ts
// Deviation thresholds (registry #18). A step needs BOTH a ratio outside the
// band and >= 1.5pt of absolute change, so float jitter in a text matrix can
// never mint one.
const SIZE_SMALLER_RATIO = 0.85;
const SIZE_BIGGER_RATIO = 1.15;
const SIZE_BIGGEST_RATIO = 1.4;
const SIZE_MIN_DELTA = 1.5;
/** Share of a line's resolved characters that must agree on one deviation. */
const FMT_AGREEMENT = 0.8;

function sizeStep(size: number, dominant: number): SizeStep | undefined {
  if (Math.abs(size - dominant) < SIZE_MIN_DELTA) return undefined;
  const ratio = size / dominant;
  if (ratio <= SIZE_SMALLER_RATIO) return '-1';
  if (ratio > SIZE_BIGGEST_RATIO) return '+2';
  if (ratio >= SIZE_BIGGER_RATIO) return '+1';
  return undefined;
}

/** The key with the largest tally, or undefined for an empty map. */
function argmax<T>(tally: Map<T, number>): T | undefined {
  let best: T | undefined;
  let bestN = -1;
  for (const [k, n] of tally) {
    if (n > bestN) {
      best = k;
      bestN = n;
    }
  }
  return best;
}

/**
 * Stamp `fmt` on every line that deviates from the document's dominant font.
 *
 * Runs over the WHOLE document, after every page, because "dominant" is a
 * document-level fact: a chyron page set entirely in Helvetica is still a
 * deviation from a Courier script. Weighted by character count so a page of
 * one-word sans lines cannot outvote the body.
 *
 * A uniform Courier-12 screenplay produces zero fmt anywhere, by construction:
 * every run agrees with the dominant, so neither tally below is ever non-empty.
 */
export function stampLineFmt(lines: RawLine[]): void {
  const docFamily = new Map<FamilyBucket, number>();
  const docSize = new Map<number, number>();
  for (const line of lines) {
    for (const run of line.fonts ?? []) {
      docFamily.set(run.bucket, (docFamily.get(run.bucket) ?? 0) + run.chars);
      docSize.set(run.size, (docSize.get(run.size) ?? 0) + run.chars);
    }
  }
  const domFamily = argmax(docFamily);
  const domSize = argmax(docSize);
  if (domFamily === undefined || domSize === undefined || domSize <= 0) return;

  for (const line of lines) {
    const runs = line.fonts;
    if (!runs || runs.length === 0) continue;
    const total = runs.reduce((n, r) => n + r.chars, 0);
    if (total === 0) continue;

    const byFamily = new Map<FamilyBucket, number>();
    const byStep = new Map<SizeStep, number>();
    for (const run of runs) {
      if (run.bucket !== domFamily) {
        byFamily.set(run.bucket, (byFamily.get(run.bucket) ?? 0) + run.chars);
      }
      const step = sizeStep(run.size, domSize);
      if (step) byStep.set(step, (byStep.get(step) ?? 0) + run.chars);
    }
    const agreed = <T>(tally: Map<T, number>): T | undefined => {
      for (const [k, n] of tally) if (n / total >= FMT_AGREEMENT) return k;
      return undefined;
    };
    const family = agreed(byFamily);
    const size = agreed(byStep);
    if (family || size) line.fmt = { family, size };
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/extract.test.ts && bunx tsc --noEmit`
Expected: PASS, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/parser/types.ts src/parser/extract.ts tests/extract.test.ts
git commit -m "Lines learn how far they sit from the document's own font, by character weight"
```

## Task 3: wire font runs through extraction

**Files:**
- Modify: `src/parser/extract.ts`
- Test: `tests/extract.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `tests/extract.test.ts`:

```ts
describe('font runs on extracted lines', () => {
  const sized = (str: string, x: number, y: number, size: number, bucket?: string) =>
    ({ str, transform: [size, 0, 0, size, x, y], bucket });

  test('a line records its per-run bucket and size, by trimmed length', () => {
    const lines = groupItemsIntoLines(
      [sized('CHYRON: ', 110, 700, 12, 'mono'), sized('LIVE', 170, 700, 18, 'sans')],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([
      { bucket: 'mono', size: 12, chars: 7 },
      { bucket: 'sans', size: 18, chars: 4 },
    ]);
  });

  test('adjacent runs with the same bucket and size merge', () => {
    const lines = groupItemsIntoLines(
      [sized('one ', 110, 700, 12, 'mono'), sized('two', 150, 700, 12, 'mono')],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([{ bucket: 'mono', size: 12, chars: 6 }]);
  });

  test('an item whose font never resolved contributes nothing', () => {
    const lines = groupItemsIntoLines(
      [sized('seen', 110, 700, 12, 'mono'), sized('unseen', 160, 700, 12, undefined)],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([{ bucket: 'mono', size: 12, chars: 4 }]);
  });

  test('a negative vertical scale still reports a positive size', () => {
    const flipped = { str: 'flipped', transform: [12, 0, 0, -12, 110, 700], bucket: 'mono' };
    expect(groupItemsIntoLines([flipped], 612, 1)[0].fonts).toEqual([
      { bucket: 'mono', size: 12, chars: 7 },
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/extract.test.ts`
Expected: FAIL — `lines[0].fonts` is `undefined`.

- [ ] **Step 3: Implement**

In `src/parser/extract.ts`, add `bucket?: FamilyBucket;` to the `TextItem`
interface after `underline?: boolean;`, with the comment:

```ts
  /** coarse family bucket from the resolved PostScript name (registry #18) */
  bucket?: FamilyBucket;
```

In `stampFontStyles`, widen the cache and stamp the bucket. The map declaration
and the two loops become:

```ts
  const byFont = new Map<string, { italic: boolean; bold: boolean; bucket?: FamilyBucket }>();
  for (const raw of items) {
    const item = raw as TextItem;
    if (!item.fontName || byFont.has(item.fontName)) continue;
    let flags: { italic: boolean; bold: boolean; bucket?: FamilyBucket } = {
      italic: false,
      bold: false,
    };
    try {
      const font = page.commonObjs.get(item.fontName) as { name?: string } | null;
      const name = String(font?.name ?? '');
      flags = {
        italic: /italic|oblique/i.test(name),
        bold: /bold|black|heavy/i.test(name),
        bucket: familyBucket(name),
      };
    } catch {
      // unresolved font — leave plain and unbucketed
    }
    byFont.set(item.fontName, flags);
  }
  for (const raw of items) {
    const item = raw as TextItem;
    const flags = item.fontName ? byFont.get(item.fontName) : undefined;
    if (flags) {
      item.italic = flags.italic;
      item.bold = flags.bold;
      item.bucket = flags.bucket;
    }
  }
```

Add this helper next to `joinLine`:

```ts
/**
 * Per-run font tallies for a line's items, merging equal neighbours.
 *
 * Size comes from the text matrix's vertical scale, which is `transform[3]`
 * for the unrotated text screenplays are made of; the absolute value keeps a
 * flipped matrix from reporting a negative size. Items whose font never
 * resolved are skipped entirely, so the shares in `stampLineFmt` are over
 * resolved characters rather than being diluted by unknowns.
 */
function fontRuns(items: TextItem[]): FontRun[] | undefined {
  const out: FontRun[] = [];
  for (const item of items) {
    const chars = item.str.trim().length;
    if (!chars || !item.bucket) continue;
    const size = Math.round(Math.abs(item.transform[3]) * 10) / 10;
    const last = out[out.length - 1];
    if (last && last.bucket === item.bucket && last.size === size) last.chars += chars;
    else out.push({ bucket: item.bucket, size, chars });
  }
  return out.length > 0 ? out : undefined;
}
```

In `deinterleaveDualDialogue`, the normal-line emit gains the field. Replace the
`out.push({ text, indent, ... })` call in the `if (!isDualCue)` branch with:

```ts
        out.push({ text, indent, y: lines[i].y, pageNum, styled, fonts: fontRuns(lines[i].items) });
```

Leave the dual-column emit alone: those lines carry no `fonts`, and therefore no
fmt (see "Scope bound recorded up front").

Finally, in `extractDocument`, after the page loop and before the `return`:

```ts
  // Dominant font is a DOCUMENT fact, so this runs once, after every page:
  // a chyron page set entirely in Helvetica is still a deviation from a
  // Courier script (registry #18).
  stampLineFmt(allLines);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green. No committed fixture has a second face or size, so
nothing else moves.

- [ ] **Step 5: Commit**

```bash
git add src/parser/extract.ts tests/extract.test.ts
git commit -m "Extraction carries font runs, and the document decides what its own font is"
```

## Task 4: blocks inherit fmt, and an fmt change breaks a block

**Files:**
- Modify: `src/parser/group.ts`
- Modify: `src/parser/classify.ts`
- Test: `tests/parser.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `tests/parser.test.ts`. If `groupBlocks` and `RawLine` are not already
imported there, add `import { groupBlocks } from '../src/parser/group';` and
`import type { Fmt, RawLine } from '../src/parser/types';`:

```ts
describe('font shifts and block boundaries', () => {
  const ln = (text: string, y: number, fmt?: Fmt): RawLine =>
    ({ text, indent: 10, y, pageNum: 1, fmt });

  test('a block takes the fmt every one of its lines agrees on', () => {
    const blocks = groupBlocks([
      ln('A chyron line', 700, { family: 'sans' }),
      ln('and its second line', 688, { family: 'sans' }),
    ]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].fmt).toEqual({ family: 'sans' });
  });

  test('lines that disagree leave the block unmarked', () => {
    const blocks = groupBlocks([
      ln('A chyron line', 700, { family: 'sans' }),
      ln('and a plain one', 688, undefined),
    ]);
    // They also do not merge: the fmt change is a block break.
    expect(blocks).toHaveLength(2);
    expect(blocks[0].fmt).toEqual({ family: 'sans' });
    expect(blocks[1].fmt).toBeUndefined();
  });

  test('an fmt change breaks a block that would otherwise merge', () => {
    // Same indent, same page, 12pt apart: without the fmt these are one block.
    const blocks = groupBlocks([
      ln('plain action', 700),
      ln('SHOUTED IN ANOTHER FACE', 688, { family: 'sans' }),
      ln('plain again', 676),
    ]);
    expect(blocks.map((b) => b.text)).toEqual([
      'plain action',
      'SHOUTED IN ANOTHER FACE',
      'plain again',
    ]);
  });

  test('a size change breaks a block just like a family change', () => {
    const blocks = groupBlocks([
      ln('plain action', 700),
      ln('INSERT: THE CARD', 688, { size: '+1' }),
    ]);
    expect(blocks).toHaveLength(2);
    expect(blocks[1].fmt).toEqual({ size: '+1' });
  });

  test('lines with the same fmt still merge normally', () => {
    const blocks = groupBlocks([
      ln('one', 700, { family: 'sans', size: '+1' }),
      ln('two', 688, { family: 'sans', size: '+1' }),
    ]);
    expect(blocks).toHaveLength(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/parser.test.ts`
Expected: FAIL — `blocks[0].fmt` is `undefined` and the merge cases produce one
block instead of the expected split.

- [ ] **Step 3: Implement**

In `src/parser/group.ts`, add `Fmt` to the type import from `./types`, then add
this helper above `shouldBreak`:

```ts
/** Two fmts are the same shift. Both undefined counts as the same. */
function sameFmt(a: Fmt | undefined, b: Fmt | undefined): boolean {
  return a?.family === b?.family && a?.size === b?.size;
}
```

In `shouldBreak`, add this immediately before the `// Scene headings always
start new blocks` check:

```ts
  // A font shift is a block boundary (registry #18): a chyron glued to the
  // action above it must isolate, or the block would carry no fmt at all and
  // the shift would vanish.
  if (!sameFmt(line.fmt, prevLine.fmt)) return true;
```

In `buildBlock`, add the unanimity rule after the `styledText` const:

```ts
  // Block-level only, by design: every line must agree, or the block carries
  // no shift. shouldBreak above already splits on disagreement, so this is
  // the belt to that brace rather than a second policy.
  const first = lines[0].fmt;
  const fmt = lines.every((l) => sameFmt(l.fmt, first)) ? first : undefined;
```

and add `fmt,` to the returned object, right after `styledText,`.

In `src/parser/classify.ts`, carry it mechanically — the `base` element becomes:

```ts
  const base: ScreenplayElement = { id, type: 'action', text, styledText: block.styledText, fmt: block.fmt, dualRight: block.dualRight, pageNum: block.pageNum, isTitlePage: false, isReadable: true };
```

`fmt` is NEVER a classification input: the parser stays format-option-free and
classification still runs on plain text only.

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/parser/group.ts src/parser/classify.ts tests/parser.test.ts
git commit -m "A font shift isolates its block, and the block carries the shift only if unanimous"
```

## Task 5: serialize fmt notes

**Files:**
- Modify: `src/fountain/serialize.ts`
- Test: `tests/serialize.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `tests/serialize.test.ts`, matching the file's existing helper for
building elements (if it has one, use it; the shape below is self-contained):

```ts
describe('font-shift notes', () => {
  const el = (
    type: ScreenplayElement['type'],
    text: string,
    extra: Partial<ScreenplayElement> = {},
  ): ScreenplayElement =>
    ({ id: text, type, text, pageNum: 1, isTitlePage: false, isReadable: true, ...extra });

  const fountain = (elements: ScreenplayElement[]) =>
    toFountain({ elements, characters: [], scenes: [], pageCount: 1 });

  test('an action note is glued directly above its block, no blank line', () => {
    const out = fountain([
      el('scene', 'INT. NEWSROOM - DAY'),
      el('action', 'CHYRON: BREAKING.', { fmt: { family: 'sans' } }),
    ]);
    expect(out).toContain('[[fmt: sans]]\nCHYRON: BREAKING.');
  });

  test('a dialogue note leads the line, space separated', () => {
    const out = fountain([
      el('character', 'WREN', { character: 'WREN' }),
      el('dialogue', 'Read it back.', { fmt: { family: 'sans' } }),
    ]);
    expect(out).toContain('@WREN\n[[fmt: sans]] Read it back.');
  });

  test('family and size ride one note, family first', () => {
    const out = fountain([el('action', 'A SIGN.', { fmt: { family: 'sans', size: '+2' } })]);
    expect(out).toContain('[[fmt: sans +2]]\nA SIGN.');
  });

  test('a size-only shift emits the size alone', () => {
    const out = fountain([el('action', 'INSERT: THE CARD.', { fmt: { size: '+1' } })]);
    expect(out).toContain('[[fmt: +1]]\nINSERT: THE CARD.');
  });

  test('an element with no fmt emits no note', () => {
    expect(fountain([el('action', 'Plain action.')])).not.toContain('[[');
  });

  test('only action and dialogue carry notes', () => {
    // Markers in a cue, parenthetical or slug break recognition — the same
    // rule styledText already lives under (registry 9d).
    const out = fountain([
      el('scene', 'INT. A - DAY', { fmt: { family: 'sans' } }),
      el('character', 'WREN', { character: 'WREN', fmt: { family: 'sans' } }),
      el('parenthetical', '(beat)', { fmt: { family: 'sans' } }),
      el('dialogue', 'Hello.', { character: 'WREN' }),
      el('transition', 'CUT TO:', { fmt: { size: '+1' } }),
      el('mini-slug', 'LATER', { fmt: { size: '+1' } }),
    ]);
    expect(out).not.toContain('[[');
  });

  test('the note survives the forced-action prefix', () => {
    // "!" forcing must stay at the head of the TEXT, not of the note.
    const out = fountain([el('action', '.45 ON THE TABLE', { fmt: { size: '+1' } })]);
    expect(out).toContain('[[fmt: +1]]\n!.45 ON THE TABLE');
  });

  test('notes are emitted regardless of any option', () => {
    // The .fountain is the app's cache boundary: it must be stable under
    // settings flips, so the knob gates RENDERING only.
    const elements = [el('action', 'CHYRON.', { fmt: { family: 'sans' } })];
    const a = toFountain({ elements, characters: [], scenes: [], pageCount: 1 },
      undefined, resolveFormatOptions({ preserveFontShifts: false }));
    const b = toFountain({ elements, characters: [], scenes: [], pageCount: 1 },
      undefined, resolveFormatOptions({ preserveFontShifts: true }));
    expect(a).toBe(b);
    expect(a).toContain('[[fmt: sans]]');
  });
});
```

Ensure `resolveFormatOptions` and the `ScreenplayElement` type are imported at
the top of the file.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/serialize.test.ts`
Expected: FAIL — no `[[fmt: ...]]` appears in any output. The last test also
fails to compile until Task 7 adds `preserveFontShifts`; run this task's other
tests now and re-run that one after Task 7.

- [ ] **Step 3: Implement**

In `src/fountain/serialize.ts`, add below the `NEEDS_FORCE` constant:

```ts
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
```

In the `case 'dialogue'` block, replace its body with:

```ts
      case 'dialogue': {
        const line = (el.styledText ?? el.text).trim();
        const note = fmtNote(el.fmt);
        const withNote = note ? `${note} ${line}` : line;
        if (block) block.push(withNote);
        else out.push(withNote); // stray dialogue without a cue reads as action
        break;
      }
```

In the `default:` (action) block, replace its body with:

```ts
      default: {
        // action — the only type left, and the only one here besides dialogue
        // allowed to carry a styled variant or an fmt note (a future type
        // falls back to plain and unmarked).
        closeBlock();
        const body = el.type === 'action' ? (el.styledText ?? el.text).trim() : text;
        const forced = NEEDS_FORCE.test(body) ? `!${body}` : body;
        const note = el.type === 'action' ? fmtNote(el.fmt) : null;
        // Glued directly above with a single newline and NO blank line: that
        // is what makes fountain-js fold the note into the block's own token
        // instead of emitting a standalone one.
        out.push(note ? `${note}\n${forced}` : forced);
      }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/serialize.test.ts && bunx tsc --noEmit`
Expected: PASS for every test except the last (`notes are emitted regardless of
any option`), which needs Task 7's option and goes green there.

- [ ] **Step 5: Commit**

```bash
git add src/fountain/serialize.ts tests/serialize.test.ts
git commit -m "Font shifts serialize as [[fmt: ...]] notes, always, so the cache stays stable"
```

## Task 6: the shared notes module

**Files:**
- Create: `src/fountain/notes.ts`
- Test: `tests/notes.test.ts` (create)

Both renderers need identical note handling. `src/fountain/slug.ts` is the
established precedent for a discriminator both renderers import (registry #5b);
this follows it rather than duplicating regexes.

- [ ] **Step 1: Write the failing test**

Create `tests/notes.test.ts`:

```ts
import { describe, test, expect } from 'bun:test';
import { fmtClasses, fmtSizeStep, stripNotes } from '../src/fountain/notes';

describe('stripNotes', () => {
  test('a leading note and the newline it sat on both go', () => {
    expect(stripNotes('[[fmt: sans]]\nCHYRON: LIVE.')).toBe('CHYRON: LIVE.');
  });

  test('an inline note and its extra space go', () => {
    expect(stripNotes('[[fmt: sans]] Read it back.')).toBe('Read it back.');
  });

  test('a note in the middle of a line goes', () => {
    // Pre-existing wart this fixes: any hand-written note used to render as
    // literal text, which the Fountain spec says is wrong.
    expect(stripNotes('He leaves [[check this]] quickly.')).toBe('He leaves quickly.');
  });

  test('newlines inside a multi-line token survive', () => {
    // A lyrics/verse dialogue token carries its own line breaks.
    expect(stripNotes('[[fmt: +1]] one\ntwo\nthree')).toBe('one\ntwo\nthree');
  });

  test('text with no notes is returned untouched, character for character', () => {
    const s = '  double  spaced  and\n  indented  ';
    expect(stripNotes(s)).toBe(s);
  });

  test('an unterminated bracket is left alone rather than eating the line', () => {
    expect(stripNotes('He said [[ and stopped')).toBe('He said [[ and stopped');
  });
});

describe('fmtClasses', () => {
  test('a family note yields its class', () => {
    expect(fmtClasses('[[fmt: sans]]\nX')).toBe(' fmt-sans');
  });

  test('all four families and all three sizes map', () => {
    for (const [word, cls] of [['mono', 'fmt-mono'], ['serif', 'fmt-serif'],
                               ['sans', 'fmt-sans'], ['cursive', 'fmt-cursive'],
                               ['-1', 'fmt-minus1'], ['+1', 'fmt-plus1'],
                               ['+2', 'fmt-plus2']]) {
      expect(fmtClasses(`[[fmt: ${word}]]\nX`), word).toBe(` ${cls}`);
    }
  });

  test('family and size together yield both classes', () => {
    expect(fmtClasses('[[fmt: sans +2]]\nX')).toBe(' fmt-sans fmt-plus2');
  });

  test('only a LEADING note counts', () => {
    expect(fmtClasses('Text first [[fmt: sans]]')).toBe('');
  });

  test('a non-fmt note yields no classes', () => {
    expect(fmtClasses('[[a production note]]\nX')).toBe('');
  });

  test('malformed content is ignored word by word, never thrown', () => {
    expect(fmtClasses('[[fmt: teal +9 sans]]\nX')).toBe(' fmt-sans');
    expect(fmtClasses('[[fmt:]]\nX')).toBe('');
    expect(fmtClasses('[[fmt: ]]\nX')).toBe('');
  });
});

describe('fmtSizeStep', () => {
  test('reads the size word out of a leading note', () => {
    expect(fmtSizeStep('[[fmt: sans +2]]\nX')).toBe('+2');
    expect(fmtSizeStep('[[fmt: -1]]\nX')).toBe('-1');
  });

  test('a family-only note has no size step', () => {
    expect(fmtSizeStep('[[fmt: sans]]\nX')).toBeUndefined();
  });

  test('no note at all has no size step', () => {
    expect(fmtSizeStep('Plain action.')).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/notes.test.ts`
Expected: FAIL — `src/fountain/notes.ts` does not exist.

- [ ] **Step 3: Implement**

Create `src/fountain/notes.ts`:

```ts
// Fountain notes, shared by both renderers.
//
// The format says `[[...]]` is invisible in output. Before registry #18 we
// never emitted one and never stripped one either, so a hand-written note
// rendered as literal text — a spec violation nobody had hit. Stamping fmt
// notes into the .fountain makes stripping load-bearing, so it lives here,
// beside slug.ts, as the one copy both renderers import (the registry #5b
// precedent) rather than two regexes that drift.

import type { SizeStep } from '../parser/types';

/** Any note. Non-greedy and bracket-free inside, so an unterminated `[[`
 * matches nothing and is left in the text rather than eating the rest. */
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/notes.test.ts && bunx tsc --noEmit`
Expected: PASS, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/fountain/notes.ts tests/notes.test.ts
git commit -m "One copy of note stripping and fmt parsing, beside slug.ts where it belongs"
```

## Task 7: the preserveFontShifts option

**Files:**
- Modify: `src/options.ts`
- Modify: `format-defaults.json`
- Test: `tests/options.test.ts`

- [ ] **Step 1: Write the failing test**

In `tests/options.test.ts`, inside `describe('resolveFormatOptions', ...)` after
the `printSplitMinimums` test:

```ts
  test('preserveFontShifts defaults to true and accepts a boolean', () => {
    expect(resolveFormatOptions({}).preserveFontShifts).toBe(true);
    expect(resolveFormatOptions({ preserveFontShifts: false }).preserveFontShifts).toBe(false);
    expect(
      resolveFormatOptions({ preserveFontShifts: 'yes' } as Record<string, unknown>).preserveFontShifts,
    ).toBe(true);
  });
```

On line 40, change the comment `// silently. Seventeen literals, one source of
truth.` to `// silently. Eighteen literals, one source of truth.`

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/options.test.ts`
Expected: FAIL — `preserveFontShifts` is not on the resolved type.

- [ ] **Step 3: Implement**

In `src/options.ts`, in `interface FormatOptions` after `printSplitMinimums`:

```ts
  /** render block-level font family/size shifts the PDF carried (registry
   * #18). Off renders every block in the body face; the notes stay in the
   * .fountain either way, so flipping this needs no re-parse */
  preserveFontShifts: boolean;
```

In `DEFAULT_FORMAT_OPTIONS` after `printSplitMinimums: true,`:

```ts
  preserveFontShifts: true,
```

In `resolveFormatOptions`'s return object after `printSplitMinimums`:

```ts
    preserveFontShifts: bool('preserveFontShifts'),
```

In `format-defaults.json`, the file ends:

```json
  "printSplitMinimums": true,
  "preserveFontShifts": true
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/options.test.ts tests/serialize.test.ts && bunx tsc --noEmit`
Expected: PASS, including Task 5's deferred `notes are emitted regardless of any
option` test. `kit-check` stays green for now — Swift's `JSONDecoder` ignores
the unknown 18th key until Task 10 adds the field.

- [ ] **Step 5: Commit**

```bash
git add src/options.ts format-defaults.json tests/options.test.ts
git commit -m "preserveFontShifts: the knob that gates rendering and never touches the .fountain"
```

## Task 8: EPUB renders the shifts and hides the notes

**Files:**
- Modify: `src/epub/html.ts`
- Modify: `src/epub/css.ts`
- Test: `tests/epub.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `tests/epub.test.ts` (reuse the file's existing pattern for building
tokens and calling `tokensToBody`; the helper below is self-contained):

```ts
describe('font shifts', () => {
  const render = (src: string, format = DEFAULT_FORMAT_OPTIONS) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToBody(tokens, { format }).files[0].xhtml;
  };

  test('a leading fmt note becomes classes on the action paragraph', () => {
    const out = render('INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n');
    expect(out).toContain('<p class="action fmt-sans">CHYRON: LIVE.</p>');
  });

  test('family and size both land', () => {
    const out = render('INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n');
    expect(out).toContain('<p class="action fmt-sans fmt-plus2">A SIGN.</p>');
  });

  test('every dialogue paragraph of the token gets the classes', () => {
    const out = render('INT. A - DAY\n\n@WREN\n[[fmt: mono -1]] one\ntwo\n');
    expect(out).toContain('<p class="dialogue fmt-mono fmt-minus1">one</p>');
    expect(out).toContain('<p class="dialogue fmt-mono fmt-minus1">two</p>');
  });

  test('with the knob off the note still vanishes but adds no classes', () => {
    const out = render(
      'INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n',
      resolveFormatOptions({ preserveFontShifts: false }),
    );
    expect(out).toContain('<p class="action">CHYRON: LIVE.</p>');
    expect(out).not.toContain('fmt-sans');
  });

  test('notes are invisible everywhere, knob or no knob', () => {
    // Fountain says notes never render. Before registry #18 they rendered as
    // literal text; this is the general fix, not just an fmt one.
    for (const format of [DEFAULT_FORMAT_OPTIONS,
                          resolveFormatOptions({ preserveFontShifts: false })]) {
      const out = render('INT. A - DAY\n\nHe leaves [[check this]] quickly.\n', format);
      expect(out).toContain('He leaves quickly.');
      expect(out).not.toContain('[[');
      expect(out).not.toContain('check this');
    }
  });

  test('a malformed fmt note strips clean and adds nothing', () => {
    const out = render('INT. A - DAY\n\n[[fmt: chartreuse]]\nStill fine.\n');
    expect(out).toContain('<p class="action">Still fine.</p>');
    expect(out).not.toContain('[[');
  });

  test('the stylesheet carries all seven classes, in em and CSS 2.1 syntax', () => {
    const css = screenplayCss(DEFAULT_FORMAT_OPTIONS);
    for (const cls of ['fmt-mono', 'fmt-serif', 'fmt-sans', 'fmt-cursive']) {
      expect(css, cls).toContain(`.${cls} {`);
    }
    expect(css.match(/\.fmt-minus1\s*{[^}]*}/)![0]).toContain('font-size: 0.85em');
    expect(css.match(/\.fmt-plus1\s*{[^}]*}/)![0]).toContain('font-size: 1.2em');
    expect(css.match(/\.fmt-plus2\s*{[^}]*}/)![0]).toContain('font-size: 1.5em');
    // The invariants: no max-width, no line-height, no CSS3 value functions.
    const rules = ['fmt-mono', 'fmt-serif', 'fmt-sans', 'fmt-cursive',
                   'fmt-minus1', 'fmt-plus1', 'fmt-plus2']
      .map((c) => css.match(new RegExp(`\\.${c}\\s*{[^}]*}`))![0]).join('\n');
    expect(rules).not.toContain('max-width');
    expect(rules).not.toContain('line-height');
    expect(rules).not.toMatch(/\b(min|max|clamp|var)\(/);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/epub.test.ts`
Expected: FAIL — the notes render as literal text and no `fmt-*` class or rule
exists.

- [ ] **Step 3: Implement — html.ts**

In `src/epub/html.ts`, add the import:

```ts
import { fmtClasses, stripNotes } from '../fountain/notes';
```

In `renderBlocks`, replace the first line of the token loop:

```ts
  for (const t of tokens) {
    const raw = t.text ?? '';
    // Read the shift BEFORE stripping, then strip every note from every token
    // type in one place. Notes are invisible by spec; this is also the general
    // fix for hand-written ones, which used to render as literal text.
    const fmt = format.preserveFontShifts ? fmtClasses(raw) : '';
    const text = stripNotes(raw);
```

In the `case 'action':` arm:

```ts
      case 'action':
        emit(`<p class="action${fmt}">${inlineEmphasis(escapeXml(text))}</p>\n`);
        break;
```

In the `case 'dialogue':` arm, the emit line becomes:

```ts
          if (line.trim()) emit(`<p class="dialogue${fmt}">${inlineEmphasis(escapeXml(line))}</p>\n`, 'dialogue');
```

In the `case 'lyrics':` arm:

```ts
      case 'lyrics':
        emit(`<p class="action${fmt}">${inlineEmphasis(escapeXml(text))}</p>\n`);
        break;
```

Leave `centered`, `character`, `parenthetical`, `transition` and `scene_heading`
using `text` — they now get note-stripping for free and carry no fmt, matching
the serializer, which never puts a note on them.

- [ ] **Step 4: Implement — css.ts**

In `src/epub/css.ts`, append these rules at the end of the returned stylesheet,
after the `section.titlepage` rules:

```css
/* Block-level font shifts the PDF carried (registry #18), gated by
   preserveFontShifts. Sizes are em, per the vertical-in-em invariant, and
   carry no line-height. A family class sits on the paragraph itself, so it
   locally overrides the body fontFamily option without !important. Values
   stay CSS-2.1-vintage: RMSDK can blank a whole book on a value function. */
.fmt-mono { font-family: ${FONT_STACKS.courier}; }
.fmt-serif { font-family: ${FONT_STACKS.serif}; }
.fmt-sans { font-family: ${FONT_STACKS.sans}; }
.fmt-cursive { font-family: "Comic Sans MS", cursive; }
.fmt-minus1 { font-size: 0.85em; }
.fmt-plus1 { font-size: 1.2em; }
.fmt-plus2 { font-size: 1.5em; }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green. The app preview inherits this automatically —
`tokensToPreviewHtml` calls `tokensToBody`.

- [ ] **Step 6: Commit**

```bash
git add src/epub/html.ts src/epub/css.ts tests/epub.test.ts
git commit -m "EPUB renders font shifts as classes, and notes finally stop rendering as text"
```

## Task 9: MOBI renders size steps and hides the notes

**Files:**
- Modify: `src/mobi/html.ts`
- Test: `tests/mobi.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `tests/mobi.test.ts`:

```ts
describe('font shifts', () => {
  const html = (src: string, format = DEFAULT_FORMAT_OPTIONS) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToMobiHtml(tokens, { title: 'T' }, format);
  };

  test('a size step wraps the action in a font tag', () => {
    expect(html('INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n')).toContain(
      '<p><font size="+2">A SIGN.</font></p>',
    );
  });

  test('all three steps map', () => {
    expect(html('INT. A - DAY\n\n[[fmt: -1]]\nSmall.\n')).toContain('<font size="-1">');
    expect(html('INT. A - DAY\n\n[[fmt: +1]]\nMedium.\n')).toContain('<font size="+1">');
  });

  test('dialogue size steps land too', () => {
    expect(html('INT. A - DAY\n\n@WREN\n[[fmt: +1]] Read it back.\n')).toContain(
      '<font size="+1">Read it back.</font>',
    );
  });

  test('family is dropped: MOBI 6 face support is not reliable', () => {
    const out = html('INT. A - DAY\n\n[[fmt: sans]]\nCHYRON.\n');
    expect(out).toContain('<p>CHYRON.</p>');
    expect(out).not.toContain('face=');
    expect(out).not.toContain('fmt-sans');
  });

  test('a family+size note keeps the size and drops the family', () => {
    expect(html('INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n')).toContain(
      '<p><font size="+2">A SIGN.</font></p>',
    );
  });

  test('with the knob off the note vanishes and no font tag appears', () => {
    const out = html(
      'INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n',
      resolveFormatOptions({ preserveFontShifts: false }),
    );
    expect(out).toContain('<p>A SIGN.</p>');
    expect(out).not.toContain('size="+2"');
  });

  test('notes are invisible here too', () => {
    const out = html('INT. A - DAY\n\nHe leaves [[check this]] quickly.\n');
    expect(out).toContain('He leaves quickly.');
    expect(out).not.toContain('[[');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/mobi.test.ts`
Expected: FAIL — notes render as literal text, no `<font size>` wrapper.

- [ ] **Step 3: Implement**

In `src/mobi/html.ts`, add the import:

```ts
import { fmtSizeStep, stripNotes } from '../fountain/notes';
```

Extend the file's header comment's knob inventory with one line:

```
// It also reads preserveFontShifts (#18), but only the SIZE arm: this
// dialect has <font size>, and MOBI 6 face support is too unreliable to
// carry the family, which stays EPUB territory.
```

Replace the first line of the token loop:

```ts
  for (const t of tokens.filter((t) => !t.is_title)) {
    const raw = t.text ?? '';
    // Read the shift before stripping. Every token type is stripped in one
    // place: Fountain notes are invisible by spec.
    const step = format.preserveFontShifts ? fmtSizeStep(raw) : undefined;
    const sized = (s: string) => (step ? `<font size="${step}">${s}</font>` : s);
    const text = stripNotes(raw);
```

In the `case 'action':` / `case 'lyrics':` arm:

```ts
      case 'action':
      case 'lyrics':
        closeSpeech();
        push(`<p>${sized(inline(esc(text)))}</p>`);
        break;
```

In the `case 'dialogue':` arm:

```ts
      case 'dialogue':
        if (speech) speech.push(sized(inline(esc(text)).replace(/\n/g, '<br/>')));
        break;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/mobi/html.ts tests/mobi.test.ts
git commit -m "MOBI takes the size steps its one font primitive can express, and drops the rest"
```

## Task 10: the Swift side of the knob

**Files:**
- Modify: `app/Sources/ScreepubKit/FormatSettings.swift`
- Modify: `app/Sources/ScreepubKit/ScriptSettings.swift`
- Modify: `app/Sources/ScreepubApp/AppSettings.swift`
- Test: `app/Sources/KitCheck/main.swift`

This is the ritual `printSplitMinimums` ran in `445d3a1` and `5630525`. Follow it
field for field.

- [ ] **Step 1: Write the failing checks**

In `app/Sources/KitCheck/main.swift`, next to the `printSplitMinimums` default
check (~line 220):

```swift
check(FormatSettings.defaults.preserveFontShifts == true,
      "preserveFontShifts defaults ON (block font shifts render)")
```

and at the end of the `sidecarOverrideCases` array, after the
`printSplitMinimums` entry:

```swift
    .init(field: "preserveFontShifts", json: #"{"preserveFontShifts": false}"#,
          apply: { $0.preserveFontShifts = false }),
```

- [ ] **Step 2: Run to verify it fails**

Run: `(cd app && swift run -c release kit-check)`
Expected: BUILD FAILURE — `value of type 'FormatSettings' has no member
'preserveFontShifts'`. That is this task's failing state.

- [ ] **Step 3: Implement**

`FormatSettings.swift`, all four places, each directly after its
`printSplitMinimums` sibling:

```swift
    public var preserveFontShifts: Bool
```

```swift
        printSplitMinimums: true,
        preserveFontShifts: true
```

```swift
        dualDialogue: String, justifyText: Bool, printSplitMinimums: Bool,
        preserveFontShifts: Bool
```

```swift
        self.preserveFontShifts = preserveFontShifts
```

`ScriptSettings.swift`: `var preserveFontShifts: Bool?` in
`PartialFormatSettings`, and in the merge:

```swift
        if let v = partial.preserveFontShifts { merged.preserveFontShifts = v }
```

`AppSettings.swift`: in `formatSettings()`'s returned initializer:

```swift
            preserveFontShifts: bool("fmtFontShifts", def.preserveFontShifts)
```

in `setFormatSettings`:

```swift
        d.set(s.preserveFontShifts, forKey: "fmtFontShifts")
```

and `"fmtFontShifts"` appended to `resetFormatting()`'s key array.

- [ ] **Step 4: Run to verify it passes**

Run: `(cd app && swift run -c release kit-check)`
Expected: every check green, including "canonical format-defaults.json decodes
into FormatSettings" and the defaults-equality check — Task 7 already added the
JSON key, so the three-way pin closes here.

- [ ] **Step 5: Commit**

```bash
git add app/Sources/ScreepubKit/FormatSettings.swift app/Sources/ScreepubKit/ScriptSettings.swift app/Sources/ScreepubApp/AppSettings.swift app/Sources/KitCheck/main.swift
git commit -m "FormatSettings mirrors preserveFontShifts through the sidecar and defaults trio"
```

## Task 11: the rail toggle

**Files:**
- Modify: `app/Sources/ScreepubApp/ReaderRail.swift`

- [ ] **Step 1: Add the toggle**

In the `Section("From the PDF")` block — this knob is about what the source PDF
carried, which is that group's subject — add:

```swift
                Toggle("Keep the PDF's font shifts", isOn: binding(\.preserveFontShifts))
                Text("Renders inserts, chyrons and on-screen text in the face and size the script drew them in. Off sets every block in the body font. Either way the shift stays recorded, so switching back needs no reconversion.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
```

- [ ] **Step 2: Compile**

Run: `(cd app && swift build)`
Expected: builds clean. No kit-check coverage: the rail is UI, and the binding
compiles against Task 10's field or not at all.

- [ ] **Step 3: Commit**

```bash
git add app/Sources/ScreepubApp/ReaderRail.swift
git commit -m "Reader rail: the PDF's font shifts get a toggle in the From the PDF group"
```

## Task 12: the fixture grows a second face and three sizes

**Files:**
- Modify: `tools/make-fixture.py`
- Modify: `tools/torture-content.py`
- Regenerate: `tests/fixtures/torture.pdf`
- Test: `tests/torture.test.ts`

- [ ] **Step 1: Write the failing test**

In `tests/torture.test.ts`, the `beforeAll` currently keeps `doc` and
`styledAll`. Add a serialized form beside them so note PLACEMENT is asserted
against real output. At the top add `import { toFountain } from '../src/fountain/serialize';`,
declare `let fountain: string;` beside the others, and add to `beforeAll`:

```ts
  fountain = toFountain(doc);
```

Then append this describe block:

```ts
describe('block font shifts', () => {
  test('a family shift becomes a note glued above its action block', () => {
    expect(fountain).toContain('[[fmt: sans]]\nCHYRON: EVERY NAME HERE IS INVENTED.');
  });

  test('all three size steps appear, each on its own block', () => {
    expect(fountain).toContain('[[fmt: -1]]\nA footnote, set smaller than the body.');
    expect(fountain).toContain('[[fmt: +1]]\nINSERT: THE INDEX CARD');
    expect(fountain).toContain('[[fmt: +2]]\nA SIGN, VERY LARGE INDEED');
  });

  test('a dialogue shift leads the line instead', () => {
    expect(fountain).toContain('[[fmt: sans]] A text message, in the phone');
  });

  test('the body itself carries no notes', () => {
    // A uniform Courier-12 screenplay must produce zero fmt by construction;
    // only the five deliberately-shifted blocks may have one.
    expect(fountain.match(/\[\[fmt:/g)!.length).toBe(5);
  });

  test('a shifted block never merges with the plain action around it', () => {
    const chyron = doc.elements.find((e) => e.text.startsWith('CHYRON:'));
    expect(chyron).toBeDefined();
    expect(chyron!.type).toBe('action');
    expect(chyron!.text).toBe('CHYRON: EVERY NAME HERE IS INVENTED.');
    expect(chyron!.fmt).toEqual({ family: 'sans', size: undefined });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/torture.test.ts`
Expected: FAIL — none of the shifted content exists in the fixture yet.

- [ ] **Step 3: Implement — the generator learns a face and three sizes**

In `tools/make-fixture.py`:

Add the fifth font resource. `build()` already sizes its object table off
`len(FONTS)` and gives every kind the same resource dict, so nothing else needs
touching — but the three byte-pinned fixtures WILL change bytes, and Step 5
handles that:

```python
FONTS = {"F1": "Courier", "F2": "Courier-Bold",
         "F3": "Courier-Oblique", "F4": "Courier-BoldOblique",
         "F5": "Helvetica"}
```

Widen the markup vocabulary and add the shift tables:

```python
# b/i are font styles; u/k/r/w are DRAWN rules (one real, three decoys);
# f/t/z/g are block font SHIFTS, which registry 18 reads back out of the PDF.
MARKUP = re.compile(r"\{(/?)([biukrwftzg])\}")

# Styles that are drawn rather than selected: they never change which font a
# run uses, which is why the parser cannot see underline from font data alone.
DRAWN = {"u", "k", "r", "w"}

# Block font shifts. f picks the second FACE; t/z/g pick a SIZE, chosen to
# land one on each side of every threshold in registry 18:
#   10/12 = 0.83 -> -1     15/12 = 1.25 -> +1     18/12 = 1.50 -> +2
SHIFT_SIZE = {"t": 10.0, "z": 15.0, "g": 18.0}
SHIFTS = {"f"} | set(SHIFT_SIZE)


def size_for(styles):
    for s in styles:
        if s in SHIFT_SIZE:
            return SHIFT_SIZE[s]
    return 12.0
```

Replace `font_for`:

```python
def font_for(styles):
    """Style set -> font resource key."""
    if "f" in styles:
        return "F5"                      # a family shift, not a slope or weight
    s = set(styles) - DRAWN - SHIFTS
    if s == {"b", "i"}:
        return "F4"
    if s == {"b"}:
        return "F2"
    if s == {"i"}:
        return "F3"
    return "F1"
```

In `styled_row_ops`, the text-operator line becomes size-aware:

```python
            ops += [f"/{font_for(styles)} {size_for(styles):g} Tf",
                    f"1 0 0 1 {x:.2f} {y:.2f} Tm", f"({esc(text)}) Tj"]
```

- [ ] **Step 4: Implement — the content grows five shifted blocks**

In `tools/torture-content.py`, at the end of the page-6 rich-formatting block
(after the underline and decoy lines, before the `("pagebreak", "")`):

```python
    # Block-level font shifts (registry 18). Each is a WHOLE line in the
    # deviant font: fmt is block-level by design, and an inline shift is an
    # explicit non-goal. Keep these SHORT — the layout measures every line at
    # Courier 12, so an 18pt line of 60 characters would run off the page.
    ("action", "{f}CHYRON: EVERY NAME HERE IS INVENTED.{/f}"),
    ("action", "{t}A footnote, set smaller than the body.{/t}"),
    ("action", "{z}INSERT: THE INDEX CARD{/z}"),
    ("action", "{g}A SIGN, VERY LARGE INDEED{/g}"),
    ("character", "WREN"),
    ("dialogue", "{f}A text message, in the phone's own face.{/f}"),
```

- [ ] **Step 5: Regenerate ALL FOUR fixtures and re-pin the three**

The new `/F5` resource changes every kind's resource dictionary, so the three
byte-pinned fixtures change bytes even though their content is identical. That
is exactly what `tests/fixture-stability.test.ts` is for — regenerate and commit
the new bytes, do not weaken the pin:

```bash
python3 tools/make-fixture.py screenplay tests/fixtures/screenplay.pdf
python3 tools/make-fixture.py prose      tests/fixtures/prose.pdf
python3 tools/make-fixture.py blank      tests/fixtures/blank-pages.pdf
python3 tools/make-fixture.py torture    tests/fixtures/torture.pdf
```

Expected: `5 pages`, `1 pages`, `2 pages`, `15 pages`. If the torture count
moved or `make-fixture.py` exits with an `atline:` error, the six new rows
pushed page 6 past `LINES_PER_PAGE` — move the block to its own page before the
`("pagebreak", "")` rather than relaxing an anchor.

- [ ] **Step 6: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green — `fixture-stability` passes against the regenerated
bytes, and `screenplay.pdf` still reports 5 pages · 5 scenes · 3 characters,
because a resource nothing references cannot change what parses.

- [ ] **Step 7: Commit**

```bash
git add tools/make-fixture.py tools/torture-content.py tests/fixtures/ tests/torture.test.ts
git commit -m "The fixture sets a chyron in Helvetica and three inserts at three sizes"
```

## Task 13: registry #18 and the coverage manifest

**Files:**
- Modify: `docs/formatting-options-log.md`
- Modify: `tools/torture-manifest.json`

`tests/torture-coverage.test.ts` fails the suite if a registry entry has no
manifest row, so these two land together.

- [ ] **Step 1: Add registry entry #18**

Append to `docs/formatting-options-log.md`:

```markdown
### 18. Block font shifts (option, default ON; 2026-08-04)
- **What:** `preserveFontShifts` renders block-level font family and size
  shifts the PDF carried — text-message inserts, chyrons, letters, on-screen
  title cards. The parser computes the document's dominant (family bucket,
  size) weighted by CHARACTER COUNT, and stamps a line with `fmt` when ≥ 80%
  of its resolved characters agree on one deviation. Blocks take the shift
  only when every line agrees, and an fmt change breaks a block, so a chyron
  glued to plain action isolates instead of vanishing.
- **Vocabulary:** family ∈ `mono|serif|sans|cursive`, size ∈ `-1|+1|+2`.
  Serialized as Fountain notes: `[[fmt: sans]]`, `[[fmt: +2]]`,
  `[[fmt: sans +2]]` — on its own line glued above an action block, inline
  ahead of a dialogue line. The notes channel is lossless through fountain-js
  and invisible by spec, so a tool that never heard of Screepub ignores it.
- **Thresholds:** size step from the ratio to the dominant — ≤ 0.85 is `-1`,
  1.15–1.4 is `+1`, > 1.4 is `+2` — each additionally requiring ≥ 1.5pt of
  absolute change, so float jitter in a text matrix can never mint a step.
  Size is the text matrix's vertical scale (`transform[3]`, absolute).
  Family buckets on the PostScript base name with its subset prefix stripped,
  lowercased, non-alphanumerics removed: `courier|mono|lettergothic|prestige`
  → mono, `times|georgia|garamond|palatino|caslon|baskerville|minion` →
  serif, `script|hand|brush|comic` → cursive, everything else sans. Bare
  "gothic" is excluded (Century Gothic is a sans) and so is bare "roman"
  ("-Roman" is the REGULAR weight of Helvetica-Roman and AvenirNext-Roman;
  Times matches on "times" anyway). Weight and slope tokens match nothing
  here: style is #9d's business.
- **Always serialized, knob gates RENDERING:** the `.fountain` carries the
  notes whatever the setting, because it is the app's cache boundary and must
  be byte-stable under a settings flip — flipping this re-renders without
  re-parsing the PDF.
- **Note stripping (general correction):** both renderers now strip ALL
  `[[...]]` notes from rendered text. Before this entry we emitted none and
  stripped none, so a hand-written note rendered as literal text, which the
  Fountain spec forbids. One copy in `src/fountain/notes.ts`, imported by
  both renderers (the #5b precedent).
- **Rendering:** EPUB adds `fmt-mono|fmt-serif|fmt-sans|fmt-cursive` and
  `fmt-minus1|fmt-plus1|fmt-plus2` to the block's paragraphs; sizes are
  0.85/1.2/1.5em (vertical-in-em invariant, no line-height, no max-width, no
  CSS3 value functions). A family class sits on the paragraph, so it locally
  overrides the body `fontFamily` option (#6). MOBI renders the size arm as
  `<font size="-1|+1|+2">` and DROPS family: MOBI 6 face support is not
  reliable enough to spend the markup on.
- **Bounds:** block-level only — inline (mid-line) family/size runs are an
  explicit non-goal. Dual-dialogue lines carry no fmt: the column-partition
  pass (#10a) joins its columns to strings without retaining the per-column
  items, and a simultaneous exchange in a deviant face is rare enough not to
  justify restructuring it. Embedding the PDF's actual fonts is also out of
  scope; the classes name generic stacks.
- **Uniform scripts are untouched:** a single-face, single-size screenplay
  produces zero fmt anywhere by construction, so its `.fountain` and both its
  outputs are byte-identical to before this entry. Verified against four of
  the five real generators in the local set; the fifth (Fade In) carries a
  sans face and a 36pt title face and is the live case.
- **App option:** "Keep the PDF's font shifts" (reader rail, From the PDF).
- **Device verdict: pending —** next KFX pass. Does a `+2` block render
  larger without breaking the keep around it, and does a family class survive
  the Publisher Font toggle (#6b's meta should make it hold in Apple Books)?
- **Code:** `src/parser/extract.ts` (`familyBucket`, `fontRuns`,
  `stampLineFmt`), `src/parser/group.ts`, `src/parser/classify.ts`,
  `src/fountain/serialize.ts` (`fmtNote`), `src/fountain/notes.ts`,
  `src/epub/html.ts` + `src/epub/css.ts`, `src/mobi/html.ts`,
  `src/options.ts`, app FormatSettings + ReaderRail.
```

- [ ] **Step 2: Add the manifest row**

In `tools/torture-manifest.json`, append a row (the file is a flat array):

```json
  { "entry": "18",
    "title": "Block font shifts",
    "covered": true,
    "page": 6,
    "side": "both",
    "how": "a chyron action block in Helvetica, three inserts at 10/15/18pt covering -1/+1/+2, and a dialogue line in the second face; each asserted to emit the exact [[fmt: ...]] note, and the rest of the fixture asserted to emit none" }
```

- [ ] **Step 3: Run the coverage suite**

Run: `bun test tests/torture-coverage.test.ts`
Expected: PASS — 34 rows for 34 registry entries, no orphans, no duplicates.

- [ ] **Step 4: Commit**

```bash
git add docs/formatting-options-log.md tools/torture-manifest.json
git commit -m "Registry 18: what a font shift is, what it renders as, and where it stops"
```

## Task 14: full verification sweep

- [ ] **Step 1:** `bun test`
Expected: full suite green.

- [ ] **Step 2:** `bunx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: committed-fixture sweep + epubcheck.**

```bash
for f in tests/fixtures/screenplay.pdf tests/fixtures/torture.pdf; do bun src/cli.ts "$f" --out /tmp/p2-sweep; done
epubcheck /tmp/p2-sweep/*.epub
```

Expected: `screenplay.pdf` unchanged at 5 pages · 5 scenes · 3 characters;
epubcheck 0 errors. The `fmt-*` rules are plain CSS 2.1 and draw no warning.

- [ ] **Step 4: real-PDF sweep, by generator.**

```bash
R=<repo>/fixtures
for f in "$R"/final-draft.pdf "$R"/highland.pdf "$R"/celtx.pdf "$R"/fade-in.pdf "$R"/chromium.pdf; do bun src/cli.ts "$f" --out /tmp/p2-real; done
grep -c 'fmt:' /tmp/p2-real/*.fountain
```

Expected from the probe table: `final-draft`, `highland`, `celtx` and
`chromium` emit **zero** fmt notes (single face, single size). `fade-in` emits
some — it carries AvenirNext at 12pt and a 36pt display face. Confirm the
fade-in notes land on blocks that really are set differently, and that its
page/scene/character counts have not moved. A non-zero count on any of the
other four means the dominant calculation or the 80% rule is wrong; investigate
before proceeding.

**Never let a real title, author or character name reach an assertion, a doc or a
screenshot.** Read these outputs; do not paste them anywhere.

- [ ] **Step 5: app bundle and Swift checks.**

```bash
app/build-app.sh && (cd app && swift run -c release kit-check)
```

Expected: bundle builds with the new sidecar embedded; kit-check fully green
including the new default check and the new sidecar-override case.

- [ ] **Step 6:** Commit anything the sweep touched. Then update the umbrella
plan `2026-07-30-formatting-umbrella.md`: mark Phases B and C done with their
dates, and repoint §Phase B and §Phase C at the real filenames
(`2026-08-04-rich-formatting-phase1.md` / `-phase2.md`) rather than the
`2026-07-30-` names they predicted.

- [ ] **Step 7:** Phase D (the combined device pass) is Sam-owned and stays
open: registry #18's verdict is pending, and the umbrella's Phase D checklist
already lists "underline renders on device; font shifts render and degrade
sanely" as items to settle in that session.

---

## Self-review against the spec

| Spec §Phase 2 requirement | Task |
|---|---|
| dominant (family bucket, size) over the document, weighted by char count | 2, 3 |
| size from `transform[3]`; family from the PostScript name, subset prefix stripped | 1, 3 |
| bucket lists incl. the "gothic" trap; bold/italic tokens ignored | 1 |
| line fmt at ≥ 80% char agreement; step ratios + ≥ 1.5pt absolute | 2 |
| uniform Courier-12 produces zero fmt by construction | 2 (test), 14 (real sweep) |
| `RawLine.fmt`, `TextBlock.fmt`; `buildBlock` unanimity; `shouldBreak` on fmt | 3, 4 |
| `classify.ts` carries fmt mechanically; never a classification input | 4 |
| title-page/page-number elements emit nothing | 5 (`isBody` already filters them) |
| serialization scope action + dialogue only; both placements | 5 |
| grammar `[[fmt: family]] / [size] / [family size]` | 5 |
| notes ALWAYS serialized regardless of the knob | 5 |
| both renderers strip ALL notes, before escaping | 6, 8, 9 |
| leading fmt note → classes on every paragraph of the block | 8 |
| CSS: em sizes, no line-height, no max-width; family overrides body | 8 |
| MOBI `<font size>`; family dropped | 9 |
| malformed fmt content: stripped, no classes, never crashes | 6, 8 |
| preview inherits via `tokensToPreviewHtml` | 8 (no change needed: it calls `tokensToBody`) |
| options + three-way default pin + FormatSettings + toggle | 7, 10, 11 |
| registry section: vocabulary, thresholds, always/gates split, note correction | 13 |
| fixture: second font resource + oversized insert; exact notes asserted | 12 |
| unit: mapping, thresholds, agreement, placement, knob, invisibility, MOBI wrap | 1, 2, 5, 6, 8, 9 |
| pins re-pinned; fixture sweep + epubcheck; real-PDF sweep by generator | 12, 14 |

One spec detail is deliberately changed: bare `roman` is dropped from the serif
matcher, because it is a weight token on sans faces and `times` already covers
the family. Reasoning is in "Probe results" above and in registry #18.

One scope bound is added rather than discovered later: dual-dialogue lines carry
no fmt, recorded in #18 under Bounds.
