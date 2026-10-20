# Rich Formatting Phase 1 (underline) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect underlines that screenplay PDFs draw as vector art, so they ride
`styledText` into the `.fountain` as `_underscores_` exactly like bold and italic
already do.

**Architecture:** One new pass in `src/parser/extract.ts`. A single walk over each
page's operator list collects flat, short, non-furniture drawn rules as
`UnderlineMark`s in PDF user space; a pure geometry predicate then marks a text
item underlined when a mark sits in a narrow band under its baseline and covers
most of its width. `joinLine`'s existing style-run machinery gains `u` as a third
style bit. No renderer production code changes at all: both EPUB and MOBI already
turn `_x_` into markup.

**Tech Stack:** Bun/TypeScript engine (bun:test), pdf.js 6.2.108 via
`pdfjs-dist/build/pdf.mjs`, python3 fixture generator (`tools/make-fixture.py`).

**Spec:** `../specs/2026-07-30-rich-formatting-design.md` §Phase 1.
**Umbrella:** `2026-07-30-formatting-umbrella.md` §Phase B. Phase A landed in
`3fb410c`; this plan is written against the post-A tree.

---

## Probe results this plan is calibrated on (2026-08-04)

The spec was de-risked against pdf.js **6.1.200**; the repo has since moved to
**6.2.108** (`95a98b5`). Re-probing confirmed the design and corrected two
details. These numbers are why the constants below are what they are.

`constructPath` still carries `[paintOp: number, packedPathData: Array,
minMax: Float32Array]`, and `minMax` is `[x0, y0, x1, y1]` in **untransformed
path space** — pdf.js emits `transform` as its own op and does not pre-apply it.

Measured over the local generator set (geometry only; no script text was read):

| PDF | paths | paint op | mark size | CTM | mark y − baseline | overlap of item |
|---|---|---|---|---|---|---|
| torture.pdf p8 | 1 | `fill` | 72.0 × 0.60 | identity | −1.7 | 100% |
| final-draft.pdf p1 | 43 | `stroke` | 168.0 × 0.00 | **`[1,0,0,-1,0,792]`** | −2.0 | 100% |
| highland.pdf p1 | 4 | `stroke` | 80.0 × 0.00 | identity | −2.1 | 100% |
| highland.pdf p6 | | `stroke` | 80.0 × 0.00 | identity | −3.0 | 100% |
| highland.pdf p8 | | `stroke` | 36.0 × 0.00 | identity | −1.5 | **28%** |
| highland.pdf p9 | | `stroke` | 36.0 × 0.00 | identity | −1.5 | **50%** |
| celtx.pdf | 0 | — | — | — | — | — |
| fade-in.pdf | 0 | — | — | — | — | — |
| chromium.pdf | 34 | `endPath` / `fill` | full-page boxes | mixed | — | — |

**Three consequences, each load-bearing:**

1. **Real underlines are STROKED, not filled.** Only the invented fixture fills a
   rect. Every real one is a zero-height stroke. The paint-op check must therefore
   *not* require a fill. It rejects exactly one op: `OPS.endPath` (`W n`, the
   clip-path spelling), which constructs geometry and paints nothing — that is
   what every full-page box in chromium.pdf and final-draft.pdf is.

2. **Final Draft draws under a y-flip CTM.** `[1,0,0,-1,0,792]` is not a rotation,
   so "skip rotated CTMs" would let it through untransformed and place the mark at
   y=287 instead of y=505 — no baseline would ever match, and Final Draft, the
   most common generator, would silently detect nothing. The rule is: transform
   **both** bbox corners and take min/max (which absorbs the sign flip), and skip
   only genuinely skewed/rotated matrices (`b` or `c` non-zero).

3. **The 60% overlap rule drops partial-item underlines.** Highland p8/p9 underline
   ~5 characters inside an 18-character text item. At item granularity the only
   choices are "mark the whole item" (wrong: `_` around 18 characters that are not
   underlined) or "drop it". This plan drops it, deliberately: a false positive
   corrupts the text of a shipped book, a false negative just preserves today's
   behavior. It is a real fidelity bound and Task 6 records it in §9d rather than
   leaving it as folklore.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/parser/extract.ts` | modify | mark collection, the matcher, item stamping, `u` in `joinLine` |
| `tests/extract.test.ts` | modify | unit tests: geometry, matcher, `u` runs and combos |
| `tests/mobi.test.ts` | modify | pinning test for the existing `_x_` → `<u>` replacement |
| `tools/make-fixture.py` | modify | three decoy rule shapes beside the real underline |
| `tools/torture-content.py` | modify | the decoy lines |
| `tests/fixtures/torture.pdf` | regenerate | bytes change (not byte-pinned; only screenplay/prose/blank are) |
| `tests/torture.test.ts` | modify | flip the "NOT detected" assertion; assert the decoys stay silent |
| `docs/formatting-options-log.md` | modify | §9d: limitation replaced by mechanism + the measured bound |
| `tools/torture-manifest.json` | modify | §9d row's `how` stops saying underlines are asserted undetected |

No renderer, serializer, options, or Swift changes in Phase 1.

---

## Task 1: mark collection and the matcher

**Files:**
- Modify: `src/parser/extract.ts`
- Test: `tests/extract.test.ts`

- [ ] **Step 1: Write the failing test**

At the TOP of `tests/extract.test.ts`, replace the existing first two import
lines with these three. `pdfjs-shims` must be imported before anything reaches
pdf.js, exactly as `src/parser/extract.ts` itself does:

```ts
import '../src/parser/pdfjs-shims';
import { describe, test, expect } from 'bun:test';
import { collectUnderlineMarks, groupItemsIntoLines, markUnderlinesItem } from '../src/parser/extract';
import { OPS } from 'pdfjs-dist/build/pdf.mjs';
```

Then append this block to the END of the file:

```ts
describe('collectUnderlineMarks', () => {
  const PAGE = 612;

  /** Build a one-op operator list drawing a path with bbox [x0,y0,x1,y1]. */
  function paths(
    entries: { bbox: [number, number, number, number]; paint?: number }[],
    prefix: { fn: number; args: unknown[] }[] = [],
  ) {
    const fnArray: number[] = prefix.map((p) => p.fn);
    const argsArray: unknown[] = prefix.map((p) => p.args);
    for (const e of entries) {
      fnArray.push(OPS.constructPath);
      argsArray.push([e.paint ?? OPS.stroke, [], Float32Array.from(e.bbox)]);
    }
    return { fnArray, argsArray };
  }

  test('a flat, short, filled rule is a mark', () => {
    // The invented fixture's shape: a 72pt x 0.6pt filled rect.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [201.6, 562.0, 273.6, 562.6], paint: OPS.fill }]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 201.6, x1: 273.6, y: 562.3 }]);
  });

  test('a zero-height STROKED rule is a mark too', () => {
    // Every real underline in the local set is a stroke with a zero-height
    // bbox, not a filled rect. Requiring a fill would detect nothing real.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [266.4, 548.5, 346.4, 548.5], paint: OPS.stroke }]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 266.4, x1: 346.4, y: 548.5 }]);
  });

  test('a tall box is not a mark', () => {
    expect(collectUnderlineMarks(paths([{ bbox: [100, 500, 300, 512] }]), PAGE)).toEqual([]);
  });

  test('a hairline shorter than 4pt is not a mark', () => {
    expect(collectUnderlineMarks(paths([{ bbox: [100, 500, 103, 500] }]), PAGE)).toEqual([]);
  });

  test('a page-wide rule is furniture, not a mark', () => {
    // Final Draft's header rules span the full measure; an underline never does.
    expect(collectUnderlineMarks(paths([{ bbox: [0, 744, 612, 744] }]), PAGE)).toEqual([]);
  });

  test('endPath constructs geometry but paints nothing', () => {
    // `W n` clip paths. chromium.pdf draws 34 of them, all full-page boxes.
    expect(
      collectUnderlineMarks(paths([{ bbox: [100, 500, 200, 500], paint: OPS.endPath }]), PAGE),
    ).toEqual([]);
  });

  test('a y-flip CTM is applied, not skipped', () => {
    // final-draft.pdf draws its underlines under [1,0,0,-1,0,792]. Untransformed
    // the mark lands at y=287; the real baseline is at 505.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.save, args: [] },
        { fn: OPS.transform, args: [1, 0, 0, -1, 0, 792] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 222, x1: 390, y: 505 }]);
  });

  test('restore pops the CTM back', () => {
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.save, args: [] },
        { fn: OPS.transform, args: [1, 0, 0, -1, 0, 792] },
        { fn: OPS.restore, args: [] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 222, x1: 390, y: 287 }]);
  });

  test('a rotated CTM is skipped rather than guessed at', () => {
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.transform, args: [0, 1, -1, 0, 0, 0] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([]);
  });

  test('a malformed operator list yields no marks instead of throwing', () => {
    expect(collectUnderlineMarks({ fnArray: [OPS.constructPath], argsArray: [null] }, PAGE)).toEqual([]);
    expect(collectUnderlineMarks(null as unknown as { fnArray: number[]; argsArray: unknown[] }, PAGE)).toEqual([]);
  });
});

describe('markUnderlinesItem', () => {
  const mark = (y: number, x0 = 100, x1 = 200) => ({ x0, x1, y });

  test('a mark 2pt under the baseline, fully covering the item, matches', () => {
    expect(markUnderlinesItem(mark(498), 100, 200, 500)).toBe(true);
  });

  test('the measured range of real generators all matches', () => {
    // -1.5 (highland p8/p9), -1.7 (torture), -2.0 (final draft), -3.0 (highland p6)
    for (const d of [-1.5, -1.7, -2.0, -3.0]) {
      expect(markUnderlinesItem(mark(500 + d), 100, 200, 500), `d=${d}`).toBe(true);
    }
  });

  test('a strikethrough sits above the baseline and does not match', () => {
    expect(markUnderlinesItem(mark(503.5), 100, 200, 500)).toBe(false);
  });

  test('a table border 6pt below the baseline does not match', () => {
    expect(markUnderlinesItem(mark(494), 100, 200, 500)).toBe(false);
  });

  test("the next line's mark does not match this line", () => {
    // 12pt line spacing: the row below's underline sits at 488 - 2.
    expect(markUnderlinesItem(mark(486), 100, 200, 500)).toBe(false);
  });

  test('a mark covering less than 60% of the item does not match', () => {
    // highland p8: a 36pt rule under a 129.5pt item. Marking the whole item
    // underlined would put _ around 13 characters that are not underlined.
    expect(markUnderlinesItem(mark(498, 100, 136), 100, 229.5, 500)).toBe(false);
  });

  test('a mark covering 60% or more does match', () => {
    expect(markUnderlinesItem(mark(498, 100, 160), 100, 200, 500)).toBe(true);
  });

  test('a zero-width item never matches', () => {
    expect(markUnderlinesItem(mark(498), 100, 100, 500)).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/extract.test.ts`
Expected: FAIL — `collectUnderlineMarks` and `markUnderlinesItem` are not exported
from `src/parser/extract.ts`.

- [ ] **Step 3: Implement**

In `src/parser/extract.ts`, change the pdf.js import on line 5 to pull in `OPS`:

```ts
import { getDocument, OPS } from 'pdfjs-dist/build/pdf.mjs';
```

Then insert this block immediately AFTER the `interface LineItems { ... }`
declaration (just above the `stampFontStyles` doc comment):

```ts
/**
 * A drawn horizontal rule that might be an underline, in PDF user space —
 * the same space text baselines live in (origin bottom-left).
 */
export interface UnderlineMark {
  x0: number;
  x1: number;
  y: number;
}

interface OpList {
  fnArray: ArrayLike<number>;
  argsArray: ArrayLike<unknown>;
}

// Geometry bounds, calibrated 2026-08-04 against the local generator set.
// Real underlines measured 0.0-0.6pt tall, 36-168pt wide, and sat 1.5-3.0pt
// below their baseline. Every constant has a decoy in the torture fixture.
const MARK_MAX_HEIGHT = 2.5;       // taller is a box or a filled bar
const MARK_MIN_WIDTH = 4;          // narrower is a bullet or a tick
const MARK_MAX_WIDTH_FRAC = 0.85;  // wider is a header/footer rule
const MARK_BELOW = 3.5;            // deepest a mark may sit under a baseline
const MARK_ABOVE = 0.5;            // a strikethrough sits far higher than this
const MARK_MIN_OVERLAP = 0.6;      // fraction of the item a mark must cover

/** a ∘ t — PDF's `cm` post-multiplies onto the current matrix. */
function concat(m: number[], t: number[]): number[] {
  return [
    m[0] * t[0] + m[2] * t[1],
    m[1] * t[0] + m[3] * t[1],
    m[0] * t[2] + m[2] * t[3],
    m[1] * t[2] + m[3] * t[3],
    m[0] * t[4] + m[2] * t[5] + m[4],
    m[1] * t[4] + m[3] * t[5] + m[5],
  ];
}

/**
 * Walk a page's operator list for drawn rules that could be underlines.
 *
 * pdf.js folds all path painting into `constructPath`, whose args are
 * [paintOp, packedPathData, minMax]. The minMax bounding box alone separates
 * an underline (flat and short) from a box (tall) or a page rule (wide), so
 * the packed path data is never decoded. If a generator ever batches several
 * underlines into one path with several subpaths, decoding that array into
 * per-subpath bboxes is the refinement — not built until a real script needs it.
 *
 * Best-effort by design, like `stampFontStyles`: any failure yields no marks,
 * which is exactly today's behavior.
 */
export function collectUnderlineMarks(opList: OpList, pageWidth: number): UnderlineMark[] {
  const marks: UnderlineMark[] = [];
  try {
    const { fnArray, argsArray } = opList;
    let m = [1, 0, 0, 1, 0, 0];
    const stack: number[][] = [];

    for (let i = 0; i < fnArray.length; i++) {
      const fn = fnArray[i];

      if (fn === OPS.save) {
        stack.push(m);
        continue;
      }
      if (fn === OPS.restore) {
        m = stack.pop() ?? [1, 0, 0, 1, 0, 0];
        continue;
      }
      if (fn === OPS.transform) {
        m = concat(m, argsArray[i] as number[]);
        continue;
      }
      if (fn !== OPS.constructPath) continue;

      const args = argsArray[i] as [number, unknown, ArrayLike<number>] | null;
      // endPath is the clip-path spelling (`W n`): it builds geometry and
      // paints nothing. Every full-page box the browser-print generator draws
      // is one. Every OTHER paint op is accepted — real underlines are
      // STROKED, not filled, so requiring a fill would detect nothing real.
      if (!args || args[0] === OPS.endPath) continue;
      const bb = args[2];
      if (!bb || bb.length < 4) continue;

      // Skew or rotation: the bbox stops describing an axis-aligned rule, and
      // screenplays do not underline on a slant. Skip rather than guess.
      if (Math.abs(m[1]) > 1e-6 || Math.abs(m[2]) > 1e-6) continue;

      // BOTH corners, then min/max. Final Draft draws its underlines under a
      // y-flip ([1,0,0,-1,0,pageHeight]); transforming one corner, or treating
      // a flip as "rotated, skip it", puts the mark hundreds of points off and
      // no baseline ever matches.
      const ax = m[0] * bb[0] + m[2] * bb[1] + m[4];
      const ay = m[1] * bb[0] + m[3] * bb[1] + m[5];
      const bx = m[0] * bb[2] + m[2] * bb[3] + m[4];
      const by = m[1] * bb[2] + m[3] * bb[3] + m[5];
      const x0 = Math.min(ax, bx);
      const x1 = Math.max(ax, bx);
      const y0 = Math.min(ay, by);
      const y1 = Math.max(ay, by);

      const width = x1 - x0;
      if (y1 - y0 > MARK_MAX_HEIGHT) continue;
      if (width < MARK_MIN_WIDTH) continue;
      if (width >= pageWidth * MARK_MAX_WIDTH_FRAC) continue;

      marks.push({ x0, x1, y: (y0 + y1) / 2 });
    }
  } catch {
    return [];
  }
  return marks;
}

/**
 * True when `mark` underlines the text item spanning [x0, x1) on `baseline`.
 * Pure geometry, so it is testable without a PDF.
 *
 * The band excludes a strikethrough (which sits mid x-height, ABOVE the
 * baseline) and the row below's own underline (12pt away at screenplay
 * spacing). The overlap floor keeps a rule under part of a run from marking
 * the whole run: item granularity means the alternative to dropping it is
 * wrapping characters that are not underlined.
 */
export function markUnderlinesItem(
  mark: UnderlineMark,
  x0: number,
  x1: number,
  baseline: number,
): boolean {
  if (mark.y > baseline + MARK_ABOVE) return false;
  if (mark.y < baseline - MARK_BELOW) return false;
  const width = x1 - x0;
  if (width <= 0) return false;
  const overlap = Math.min(x1, mark.x1) - Math.max(x0, mark.x0);
  return overlap / width >= MARK_MIN_OVERLAP;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/extract.test.ts && bunx tsc --noEmit`
Expected: PASS (all new tests plus the file's existing ones), clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/parser/extract.ts tests/extract.test.ts
git commit -m "Underline marks: the geometry that tells a rule from a box, a border and a strike"
```

## Task 2: stamp underline onto text items

**Files:**
- Modify: `src/parser/extract.ts`
- Test: `tests/torture.test.ts` (deferred to Task 5 — this task has no new test of its own)

This task is pure plumbing between two already-tested pieces, and the honest test
for it is the fixture assertion in Task 5. What it must not do is call
`getOperatorList()` twice per page.

- [ ] **Step 1: Implement**

In `src/parser/extract.ts`, replace the whole `stampFontStyles` function (its doc
comment through its closing brace) with these three functions:

```ts
/**
 * getOperatorList, best-effort. This call is ALSO what forces font resolution
 * into `page.commonObjs` — getTextContent alone does not load fonts — so
 * `stampFontStyles` depends on it having run first, and both passes share the
 * single call rather than paying for it twice.
 */
async function operatorList(page: {
  getOperatorList(): Promise<unknown>;
}): Promise<OpList | null> {
  try {
    return (await page.getOperatorList()) as OpList;
  } catch {
    return null;
  }
}

/**
 * Mark each item bold/italic from its font's PostScript name (e.g.
 * "CourierPrime-Italic"). Best-effort: an unresolved font leaves its items
 * plain, which is also what happens when `operatorList` above returned null.
 */
function stampFontStyles(
  page: { commonObjs: { get(id: string): unknown } },
  items: unknown[],
): void {
  const byFont = new Map<string, { italic: boolean; bold: boolean }>();
  for (const raw of items) {
    const item = raw as TextItem;
    if (!item.fontName || byFont.has(item.fontName)) continue;
    let flags = { italic: false, bold: false };
    try {
      const font = page.commonObjs.get(item.fontName) as { name?: string } | null;
      const name = String(font?.name ?? '');
      flags = { italic: /italic|oblique/i.test(name), bold: /bold|black|heavy/i.test(name) };
    } catch {
      // unresolved font — leave plain
    }
    byFont.set(item.fontName, flags);
  }
  for (const raw of items) {
    const item = raw as TextItem;
    const flags = item.fontName ? byFont.get(item.fontName) : undefined;
    if (flags) {
      item.italic = flags.italic;
      item.bold = flags.bold;
    }
  }
}

/**
 * Mark each item underlined when a drawn rule sits in the band below its
 * baseline. Underline is DRAWN, not selected, so unlike bold/italic it can
 * never be read from font data (registry 9d).
 */
function stampUnderlines(items: unknown[], opList: OpList, pageWidth: number): void {
  const marks = collectUnderlineMarks(opList, pageWidth);
  if (marks.length === 0) return;
  for (const raw of items) {
    const item = raw as TextItem;
    if (!item.str || !item.str.trim() || !item.transform) continue;
    const x0 = item.transform[4];
    const baseline = item.transform[5];
    if (marks.some((m) => markUnderlinesItem(m, x0, endX(item), baseline))) {
      item.underline = true;
    }
  }
}
```

In the `TextItem` interface, add the field after `bold?: boolean;`:

```ts
  /** a rule was DRAWN under this item — never readable from font data */
  underline?: boolean;
```

In `extractDocument`, replace the single line `await stampFontStyles(page, textContent.items);` with:

```ts
    const ops = await operatorList(page);
    stampFontStyles(page, textContent.items);
    if (ops) stampUnderlines(textContent.items, ops, viewport.width);
```

- [ ] **Step 2: Run the suite to verify nothing regressed**

Run: `bun test && bunx tsc --noEmit`
Expected: the WHOLE suite green, `tests/torture.test.ts` included. The stamping
is not yet observable in any output — `joinLine` does not read `underline` until
Task 3 — so `underline is NOT detected yet (registry 9d)` still passes here and
flips in Task 5. A green run is therefore not evidence this task worked.

- [ ] **Step 2b: Prove the stamping actually fires**

Because Step 2 cannot see it, check it directly. Write this to `.p1tmp/check.ts`
(a scratch dir inside the worktree, so the relative import resolves), run it,
then delete the directory:

```ts
import '../src/parser/pdfjs-shims';
import { getDocument } from 'pdfjs-dist/build/pdf.mjs';
import { collectUnderlineMarks, markUnderlinesItem } from '../src/parser/extract';

const bytes = new Uint8Array(await Bun.file('tests/fixtures/torture.pdf').arrayBuffer());
const pdf = await getDocument({ data: bytes }).promise;
for (let p = 1; p <= pdf.numPages; p++) {
  const page = await pdf.getPage(p);
  const vp = page.getViewport({ scale: 1.0 });
  const ops = await page.getOperatorList();
  const tc = await page.getTextContent();
  const marks = collectUnderlineMarks(ops as never, vp.width);
  if (!marks.length) continue;
  console.log(`page ${p}: ${marks.length} mark(s)`, JSON.stringify(marks));
  for (const it of tc.items as any[]) {
    if (!it.str?.trim()) continue;
    const x0 = it.transform[4], x1 = x0 + (it.width ?? it.str.length * 6);
    if (marks.some((m) => markUnderlinesItem(m, x0, x1, it.transform[5])))
      console.log('   UNDERLINED ->', JSON.stringify(it.str));
  }
}
```

Expected, exactly — one mark in fifteen pages, on the one item it belongs to:

```
page 8: 1 mark(s) [{"x0":201.60000610351562,"x1":273.6000061035156,"y":562.2999877929688}]
   UNDERLINED -> "underlined"
```

- [ ] **Step 3: Commit**

```bash
git add src/parser/extract.ts
git commit -m "Underline reaches text items: one operator-list walk, shared with font stamping"
```

## Task 3: joinLine learns the third style bit

**Files:**
- Modify: `src/parser/extract.ts`
- Test: `tests/extract.test.ts`

- [ ] **Step 1: Write the failing test**

In `tests/extract.test.ts`, inside `describe('inline style detection', ...)`, the
`styled` helper's flags type must widen. Replace the helper with:

```ts
  const styled = (
    str: string,
    x: number,
    y: number,
    flags: { italic?: boolean; bold?: boolean; underline?: boolean },
  ) => ({ str, transform: [1, 0, 0, 1, x, y], ...flags });
```

Then append these tests inside that same `describe` block:

```ts
  test('an underlined run gains underscores', () => {
    const lines = groupItemsIntoLines(
      [
        styled('This word is ', 110, 700, {}),
        styled('underlined', 200, 700, { underline: true }),
        styled(' with drawn art', 300, 700, {}),
      ],
      612,
      1,
    );
    expect(lines[0].text).toBe('This word is underlined with drawn art');
    expect(lines[0].styled).toBe('This word is _underlined_ with drawn art');
  });

  test('mixed marks nest with the underscore innermost', () => {
    // Not a palindrome: the close mirrors the open. Both renderers unwrap
    // stars first and underscores last, which is exactly this nesting.
    const lines = groupItemsIntoLines(
      [
        styled('a ', 110, 700, { underline: true, bold: true }),
        styled('b ', 140, 700, { underline: true, italic: true }),
        styled('c', 170, 700, { underline: true, bold: true, italic: true }),
      ],
      612,
      1,
    );
    expect(lines[0].styled).toBe('**_a_** *_b_* ***_c_***');
  });

  test('a punctuation-only underlined item never wraps alone', () => {
    const lines = groupItemsIntoLines(
      [
        styled('by Nora Vance', 110, 700, {}),
        styled(',', 189, 700, { underline: true }),
        styled(' the b-side', 195, 700, {}),
      ],
      612,
      1,
    );
    expect(lines[0].text).toBe('by Nora Vance, the b-side');
    expect(lines[0].styled).toBeUndefined();
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/extract.test.ts`
Expected: FAIL — `styled` is `undefined` for the underline cases (`joinLine`
builds its style key from bold and italic only, so an underlined run reads as
unstyled).

- [ ] **Step 3: Implement**

In `src/parser/extract.ts`, replace the `EMPHASIS_MARK` constant:

```ts
/**
 * Fountain emphasis as OPEN/CLOSE pairs. A mixed mark is not a palindrome —
 * `**_x_**` closes in the mirror order — so one string per style no longer
 * works. Canonical nesting puts the underscore innermost and the stars
 * outside, which is the order both renderers' regexes already unwrap (triple
 * stars, then double, then single, then underscore).
 *
 * Keys are built as b→i→u, so every subset of {b,i,u} appears exactly once.
 */
const EMPHASIS_MARK: Record<string, [string, string]> = {
  b: ['**', '**'],
  i: ['*', '*'],
  u: ['_', '_'],
  bi: ['***', '***'],
  bu: ['**_', '_**'],
  iu: ['*_', '_*'],
  biu: ['***_', '_***'],
};
```

In `joinLine`, extend the style key with the third bit:

```ts
    const style = hasWord
      ? `${item.bold ? 'b' : ''}${item.italic ? 'i' : ''}${item.underline ? 'u' : ''}`
      : '';
```

And in the group-wrapping loop, replace the two lines that use `mark`:

```ts
    const [open, close] = EMPHASIS_MARK[g.style] ?? ['*', '*'];
    const lead = clean.match(/^\s*/)![0];
    const trail = clean.match(/\s*$/)![0];
    styled += `${lead}${open}${core}${close}${trail}`;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/extract.test.ts && bunx tsc --noEmit`
Expected: PASS, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add src/parser/extract.ts tests/extract.test.ts
git commit -m "joinLine emits underscores, and mixed marks open and close in mirror order"
```

## Task 4: pin the MOBI underline replacement

**Files:**
- Modify: `tests/mobi.test.ts`

`src/mobi/html.ts:40` already turns `_x_` into `<u>x</u>`; it landed in the v0.4.2
merge train with no test. Phase 1 is the first thing that will actually emit those
underscores from a PDF, so the replacement stops being decorative and starts being
load-bearing. No production change here — a pin only.

- [ ] **Step 1: Write the test**

Append to `tests/mobi.test.ts`. Match the file's existing import style; if
`Fountain`, `tokensToMobiHtml` and `DEFAULT_FORMAT_OPTIONS` are already imported
at the top, do not import them twice.

```ts
describe('inline emphasis', () => {
  const html = (src: string) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToMobiHtml(tokens, { title: 'T' }, DEFAULT_FORMAT_OPTIONS);
  };

  test('underscores become <u>', () => {
    // Untested since it landed; rich-formatting phase 1 is the first thing
    // that emits these underscores from a PDF rather than a hand edit.
    expect(html('INT. A - DAY\n\nThe sign reads _DO NOT ENTER_.\n')).toContain(
      '<u>DO NOT ENTER</u>',
    );
  });

  test('mixed marks unwrap with the underscore innermost', () => {
    // The nesting joinLine emits for a bold+underlined run. Stars are
    // replaced before underscores, so this composes without a special case.
    const out = html('INT. A - DAY\n\nThe stamp is **_VOID_** now.\n');
    expect(out).toContain('<b><u>VOID</u></b>');
  });

  test('bold, italic and bold-italic keep their existing tags', () => {
    const out = html('INT. A - DAY\n\nA ***b*** and **c** and *d*.\n');
    expect(out).toContain('<b><i>b</i></b>');
    expect(out).toContain('<b>c</b>');
    expect(out).toContain('<i>d</i>');
  });
});
```

- [ ] **Step 2: Run to verify it passes immediately**

Run: `bun test tests/mobi.test.ts`
Expected: PASS. This is a pinning test for shipped behavior, so it is green on
arrival — that is correct, not a TDD violation. Confirm it is not vacuous by
temporarily deleting the `_..._` line in `src/mobi/html.ts:40`, re-running (the
first two tests must FAIL), then restoring it.

- [ ] **Step 3: Commit**

```bash
git add tests/mobi.test.ts
git commit -m "Pin the MOBI underline replacement before anything starts emitting underscores"
```

## Task 5: the fixture grows three decoys, and the assertion flips

**Files:**
- Modify: `tools/make-fixture.py`
- Modify: `tools/torture-content.py`
- Regenerate: `tests/fixtures/torture.pdf`
- Modify: `tests/torture.test.ts`

`tests/fixture-stability.test.ts` byte-pins only `screenplay`, `prose` and
`blank`. The torture fixture is deliberately outside that pin, so it may be
regenerated freely.

- [ ] **Step 1: Write the failing test**

In `tests/torture.test.ts`, replace the entire commented `EXPECTED FAIL WHEN
RICH-FORMATTING PHASE 1 LANDS` block and the `underline is NOT detected yet`
test that follows it with:

```ts
  test('a drawn underline reaches styledText as underscores', () => {
    const el = doc.elements.find((e) => e.text.includes('underlined'));
    expect(el).toBeDefined();
    // The plain text must still be clean: classification never sees markers.
    expect(el!.text).toBe('This word is underlined with drawn vector art.');
    expect(el!.styledText).toBe('This word is _underlined_ with drawn vector art.');
  });

  // Three rules the detector must REJECT, each caught by a different filter,
  // each drawn beside the real underline on the same page. A loosened
  // threshold shows up here as a stray underscore rather than as silence.
  test('a strikethrough is not an underline', () => {
    const el = doc.elements.find((e) => e.text.includes('struck through'));
    expect(el).toBeDefined();
    expect(el!.styledText ?? el!.text).not.toContain('_');
  });

  test('a table border below the text band is not an underline', () => {
    const el = doc.elements.find((e) => e.text.includes('below this cell'));
    expect(el).toBeDefined();
    expect(el!.styledText ?? el!.text).not.toContain('_');
  });

  test('a page-wide rule is furniture, not an underline', () => {
    const el = doc.elements.find((e) => e.text.includes('this whole line'));
    expect(el).toBeDefined();
    expect(el!.styledText ?? el!.text).not.toContain('_');
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/torture.test.ts`
Expected: FAIL — the three decoy tests fail on `expect(el).toBeDefined()`, because
the decoy lines do not exist in the fixture yet. (The first test now passes: Task 2
already detects the real underline.)

- [ ] **Step 3: Implement — the generator learns three decoy shapes**

In `tools/make-fixture.py`, widen the markup vocabulary. Replace the `MARKUP`
constant:

```python
# u draws a real underline; k, r and w draw DECOYS the parser must reject.
MARKUP = re.compile(r"\{(/?)([biukrw])\}")
```

Replace `font_for` (keeping its docstring's point, extended):

```python
# Styles that are DRAWN rather than selected. They never change which font a
# run uses, which mirrors how PDFs actually carry underline and is why the
# parser cannot see it from font data alone (registry 9d).
DRAWN = {"u", "k", "r", "w"}


def font_for(styles):
    """Style set -> font resource key."""
    s = set(styles) - DRAWN
    if s == {"b", "i"}:
        return "F4"
    if s == {"b"}:
        return "F2"
    if s == {"i"}:
        return "F3"
    return "F1"
```

Replace `styled_row_ops` with the version that draws all four rule shapes:

```python
def styled_row_ops(x_in, y, runs):
    """-> (text operators, drawn rectangles) for one laid-out line.

    Four rule shapes, one real and three decoys. Each decoy is rejected by a
    DIFFERENT filter in collectUnderlineMarks/markUnderlinesItem, so a
    loosened threshold shows up as a stray underscore in the emitted Fountain:

      u  real underline   0.6pt tall, run-width, 2.0pt under the baseline
      k  strikethrough    same bar at baseline + 3.5 (mid x-height) -> the
                          y-window rejects anything above the baseline
      r  table border     same bar at baseline - 6.0 -> below the window, and
                          6pt ABOVE the next row's baseline, so it is out of
                          that row's window too
      w  page-wide rule   0.5in margins (540pt = 88% of the page) at the real
                          underline offset -> only the furniture-width filter
                          saves it, over real text with 100% overlap
    """
    ops, rects = [], []
    x = x_in * PT
    for run in runs:
        text, styles = run["text"], run["styles"]
        if text:
            ops += [f"/{font_for(styles)} 12 Tf",
                    f"1 0 0 1 {x:.2f} {y:.2f} Tm", f"({esc(text)}) Tj"]
            w = len(text) * CHAR_W
            # A filled rectangle, not a stroked line. Real generators stroke;
            # filling here proves the detector accepts both paint ops.
            if "u" in styles:
                rects.append(f"0 g {x:.2f} {y - 2.0:.2f} {w:.2f} 0.6 re f")
            if "k" in styles:
                rects.append(f"0 g {x:.2f} {y + 3.5:.2f} {w:.2f} 0.6 re f")
            if "r" in styles:
                rects.append(f"0 g {x:.2f} {y - 6.0:.2f} {w:.2f} 0.6 re f")
            if "w" in styles:
                rects.append(f"0 g {0.5 * PT:.2f} {y - 2.0:.2f} "
                             f"{PAGE_W - 1.0 * PT:.2f} 0.6 re f")
        x += len(text) * CHAR_W
    return ops, rects
```

In `_torture_rows`, the `underline` field must keep meaning "a real underline",
so leave it keyed on `"u"` alone — no change to that function.

- [ ] **Step 4: Implement — the content grows the decoy lines**

In `tools/torture-content.py`, on the page-6 rich-formatting block, insert three
lines directly after the existing underline line:

```python
    ("action", "This word is {u}underlined{/u} with drawn vector art."),
    # Three drawn rules that are NOT underlines, one per rejection filter.
    # See styled_row_ops in make-fixture.py for what each one draws.
    ("action", "This phrase is {k}struck through{/k}, above the baseline."),
    ("action", "A table rules a border {r}below this cell{/r}, further down."),
    ("action", "A page-wide rule crosses {w}this whole line{/w} of text."),
```

- [ ] **Step 5: Regenerate the fixture and confirm pagination did not move**

```bash
python3 tools/make-fixture.py --emit-layout torture | python3 -c "import json,sys; p=json.load(sys.stdin); print(len(p), 'pages')"
python3 tools/make-fixture.py torture tests/fixtures/torture.pdf
```

Expected: `14 pages` both before and after the content edit (run the first command
on a stashed tree if you want the before value), and the build prints
`14 pages -> tests/fixtures/torture.pdf`. Three actions add six rows to a page
that has room for them.

If the page count changed, or `make-fixture.py` exits with an `atline:` error,
the decoys pushed page 6 over `LINES_PER_PAGE` and shifted every later anchor.
Fix it by moving the three decoy lines to just before the page's
`("pagebreak", "")` — do NOT relax the `atline` anchors, which exist to catch
exactly this.

- [ ] **Step 6: Run tests to verify they pass**

Run: `bun test && bunx tsc --noEmit`
Expected: full suite green, clean typecheck. `tests/fixture-stability.test.ts`
stays green (it does not pin the torture fixture);
`tests/torture-layout.test.ts` and `tests/torture-markup.test.ts` stay green (the
`underline` field still keys on `u` alone).

- [ ] **Step 6b: Prove the decoys are drawn, not merely absent**

Three tests asserting "no underscore appears" would pass just as well if the
decoy rectangles were never emitted. Confirm they are, and that each dies to a
different filter:

```bash
python3 - <<'PY'
import re
raw = open('tests/fixtures/torture.pdf','rb').read().decode('latin-1')
for x,y,w,h in re.findall(r'0 g ([\d.]+) ([\d.]+) ([\d.]+) ([\d.]+) re f', raw):
    print(f"x={x:>8} y={y:>8} w={w:>8} h={h}")
PY
```

Expected: exactly four rules — 72.00 wide (the real underline), 100.80 (the
strikethrough, 3.5pt HIGHER in y than its text row), 108.00 (the table border,
6pt lower), and 540.00 (the page-wide rule, 88% of 612). Then re-run the
Step 2b scratch script: it must report `4 path(s) drawn -> 3 survived the
geometry filters` on that page, with `MATCHED -> "underlined"` as the only
match. Three survivors, one match: the width filter kills the page rule, and
the y-window kills the other two.

- [ ] **Step 7: Commit**

```bash
git add tools/make-fixture.py tools/torture-content.py tests/fixtures/torture.pdf tests/torture.test.ts
git commit -m "The fixture draws three rules that are not underlines, and the assertion flips"
```

## Task 6: registry §9d and the coverage manifest

**Files:**
- Modify: `docs/formatting-options-log.md`
- Modify: `tools/torture-manifest.json`

- [ ] **Step 1: Replace §9d's limitation with the mechanism**

In `docs/formatting-options-log.md`, in `### 9d. Inline bold/italic
pass-through`, retitle the heading and replace the `- **Limitation:**` bullet.
The heading becomes:

```markdown
### 9d. Inline bold/italic/underline pass-through
```

The `- **Limitation:**` bullet is replaced by:

```markdown
- **Underline (2026-08-04):** PDFs DRAW underline as vector art rather than
  selecting a font, so it is invisible to the font-name check above. A single
  walk of each page's operator list collects drawn rules — pdf.js folds all
  painting into `constructPath`, whose `minMax` bbox alone separates a rule
  from a box — and an item is underlined when a mark sits 0.5pt above to
  3.5pt below its baseline and covers ≥ 60% of its width. Marks must be
  ≤ 2.5pt tall, ≥ 4pt wide, and < 85% of the page (header rules). The CTM is
  tracked through save/restore/transform: Final Draft draws under a y-flip
  ([1,0,0,-1,0,H]), so both bbox corners are transformed and min/maxed, and
  only genuinely skewed matrices are skipped. Every paint op counts except
  `endPath` (`W n` clip paths, which paint nothing) — real underlines are
  STROKED with a zero-height bbox, not filled. Marks nest with the underscore
  innermost (`**_x_**`), which is the order both renderers already unwrap.
- **Measured bound:** detection is per text ITEM, so a rule under only PART of
  a pdf.js run is dropped rather than applied to the whole run. Two of
  Highland's four underlines in the local set are that shape (a 36pt rule
  under a 129pt item). The trade is deliberate: a false positive puts
  underscores around text that is not underlined in a shipped book, a false
  negative just preserves the pre-2026-08-04 behavior. Fixing it means
  splitting an item at character offsets, which needs per-glyph advances.
- **Decoys (torture fixture, page 6):** a strikethrough, a table-cell border
  below the text band, and a page-wide rule sit beside the real underline,
  each rejected by a different filter, each asserted silent.
```

Extend the `- **Code:**` bullet's first path list to name the new functions:

```markdown
- **Code:** `src/parser/extract.ts` (`stampFontStyles`, `stampUnderlines`,
  `collectUnderlineMarks`, `markUnderlinesItem`, `joinLine`),
  `src/epub/html.ts` + `src/mobi/html.ts` (`inlineEmphasis`/`inline`).
```

- [ ] **Step 2: Update the coverage manifest**

In `tools/torture-manifest.json`, the `9d` row's `title` and `how` still say
underlines are asserted NOT detected. Replace that row's two fields:

```json
    "title": "Inline bold/italic/underline pass-through",
    "how": "bold, italic and bold-italic runs in action and dialogue, a run crossing a wrap boundary, and a punctuation-only styled item; plus a drawn underline asserted to reach styledText, and three drawn rules (strikethrough, table border, page-wide rule) asserted NOT to"
```

- [ ] **Step 3: Run the coverage suite**

Run: `bun test tests/torture-coverage.test.ts tests/torture.test.ts`
Expected: PASS. The manifest test only requires a row per registry entry, and
§9d keeps its number — but run it, because a mistyped heading would drop `9d`
from the registry's parsed entry list and the row would then read as an orphan.

- [ ] **Step 4: Commit**

```bash
git add docs/formatting-options-log.md tools/torture-manifest.json
git commit -m "Registry 9d: underline stops being a limitation and starts being a mechanism"
```

## Task 7: full verification sweep

- [ ] **Step 1:** `bun test`
Expected: full suite green, count ≥ 480 + the ~20 tests this plan adds.

- [ ] **Step 2:** `bunx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: committed-fixture sweep.** Convert each committed fixture and read
the CLI summary line against the recorded tables:

```bash
for f in screenplay torture; do bun src/cli.ts "tests/fixtures/$f.pdf" -o /tmp/p1-sweep/$f.epub --fountain /tmp/p1-sweep/$f.fountain --mobi; done
```

Expected: `screenplay.pdf` reports 5 pages · 5 scenes · 3 characters, unchanged —
it draws no paths at all, so any drift there is a bug in the shared refactor of
`stampFontStyles`, not in underline detection.

- [ ] **Step 4: epubcheck.**

```bash
epubcheck /tmp/p1-sweep/screenplay.epub && epubcheck /tmp/p1-sweep/torture.epub
```

Expected: 0 errors. Phase 1 emits no new markup — `_x_` already rendered as
`span.underline` — so an error here means something else broke.

- [ ] **Step 5: real-PDF sweep, by generator.** The gitignored root `/fixtures/`
set is in the MAIN checkout, not this worktree. Do a real A/B: the question is
not "are there underscores" but "what moved". Generate baseline outputs by
temporarily swapping in the pre-Phase-1 parser, then diff the `.fountain`s —
the corpus diff is this repo's tool for "does this actually matter?".

```bash
R=<repo>/fixtures
cp src/parser/extract.ts /tmp/extract.new.ts
git show eed4174:src/parser/extract.ts > src/parser/extract.ts
for f in final-draft highland celtx fade-in chromium; do bun src/cli.ts "$R/$f.pdf" -o /tmp/base/$f.epub --fountain /tmp/base/$f.fountain; done
cp /tmp/extract.new.ts src/parser/extract.ts   # MUST restore; verify with git diff
for f in final-draft highland celtx fade-in chromium; do bun src/cli.ts "$R/$f.pdf" -o /tmp/new/$f.epub --fountain /tmp/new/$f.fountain; done
for f in final-draft highland celtx fade-in chromium; do echo "$f $(diff /tmp/base/$f.fountain /tmp/new/$f.fountain | grep -c '^[<>]')"; done
```

Measured 2026-08-04 (changed `.fountain` lines; every summary line identical):

| PDF | changed | why |
|---|---|---|
| final-draft | **0** | its one mark is on page 1, the TITLE page, which never reaches body serialization |
| celtx | **0** | draws no paths at all |
| fade-in | **0** | draws no paths at all |
| highland | 8 (4 lines gained `_`) | 62 marks survived geometry across 108 pages; only 5 matched a baseline |
| chromium | 14 (7 lines gained `_`) | real browser `text-decoration: underline` |

The last row is the one worth pausing on. Its 8 matches all have mark width
EXACTLY equal to item width at a constant `dy = -1.2` — the signature of a
browser drawing a decoration under a text run, i.e. genuine underlines, not
false positives. (An earlier 10-page probe found only clip boxes there and
predicted zero; the full document says otherwise.) Highland is the reassuring
row in the other direction: 57 of its 62 geometrically-valid marks matched
nothing, so the y-window and overlap floor are doing real work against real
document furniture.

**Never let a real title, author or character name reach an assertion, a doc or a
screenshot.** Read these outputs; do not paste them anywhere.

**Never let a real title, author or character name reach an assertion, a doc or a
screenshot.** Read these outputs; do not paste them anywhere.

- [ ] **Step 6: app bundle.** The engine changed, so the sidecar must be re-embedded:

```bash
app/build-app.sh && (cd app && swift run -c release kit-check)
```

Expected: bundle builds, kit-check fully green. Phase 1 adds no option, so
kit-check has nothing new to check — this is a regression guard.

- [ ] **Step 7:** Commit anything the sweep touched. Phase 1 is complete; request
review with superpowers:requesting-code-review before starting Phase 2.

---

## Self-review against the spec

| Spec §Phase 1 requirement | Task |
|---|---|
| `collectUnderlineMarks(opList, viewport)`, CTM through save/restore/transform | 1 |
| flat ≤ 2.5pt, real ≥ 4pt, not furniture < 85% page width | 1 |
| output `{x0, x1, y}`; rotated CTMs skipped; try/catch → no underlines | 1 |
| matching: y in [baseline − 3.5, +0.5], overlap ≥ 60% | 1 |
| multi-subpath refinement deferred until a real script needs it | 1 (documented in the doc comment) |
| `TextItem` gains `underline?: boolean` | 2 |
| `joinLine` style key gains `u`; punctuation guard unchanged | 3 |
| `EMPHASIS_MARK` becomes open/close pairs, underscore innermost | 3 |
| no `serialize.ts` change; cues/slugs/parens never emit | 3 (no change needed: only dialogue/action read `styledText`) |
| EPUB unchanged; MOBI unchanged + missing pinning test | 4 |
| unit: matcher geometry, `joinLine` `u` runs and combos, MOBI pin | 1, 3, 4 |
| fixture: underline + three decoys incl. the Beat table-border shape | 5 |
| sweep: fixtures + epubcheck + real PDFs by generator | 7 |
| §9d amended: limitation replaced by mechanism | 6 |

Two spec details are deliberately changed, both because the 6.2.108 re-probe
contradicted them; the reasoning is in "Probe results" above and in §9d:
"rotated CTMs are skipped" becomes "skewed CTMs are skipped, flips are applied",
and the paint-op check accepts strokes rather than assuming fills.

One spec item is out of reach and recorded rather than silently dropped: a
Beat-generated PDF is not in the local set, so the table-border false positive is
covered by the fixture decoy only, not by a real-script check.
