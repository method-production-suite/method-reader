// The clock of "the page writes itself" (desktop/ui/page-timing.js): every
// answer is a function of the time, so these ask about moments directly
// rather than waiting for them.
import { describe, expect, test } from 'bun:test';
import { join } from 'node:path';

// desktop/ui is plain .js with no declarations, so the module is imported
// the dynamic way the other window tests import theirs, and its surface is
// spelled out here.
type Block = { text: string; chars: string[]; start: number; end: number };
type Plan = { blocks: Block[]; total: number };
type Last = { keyAt: number | null; dingAt: number | null } | null;
type Timing = {
  TYPE_MS: number; WAVE_LAG: number; STEP_MS: number; SWEEP_MS: number; GLOW_MS: number;
  MAX_SHOW_MS: number; KEY_GAP_MS: number; DING_GAP_MS: number;
  planShow(paragraphs: unknown): Plan;
  hasPage(plan: Plan): boolean;
  typedAt(plan: Plan, t: number): number;
  shownText(plan: Plan, typed: number): string[];
  stepFor(plan: Plan): number;
  formAt(plan: Plan, bookAt: number | null, i: number): number | null;
  formedAt(plan: Plan, bookAt: number | null, t: number): number;
  glowingAt(plan: Plan, bookAt: number | null, t: number): number[];
  endAt(plan: Plan, bookAt: number | null): number | null;
  frameAt(plan: Plan, bookAt: number | null, t: number): {
    typed: number; shown: string[]; formed: number; glowing: number[]; done: boolean;
  };
  soundsDue(plan: Plan, before: number, after: number, t: number, last: Last): {
    key: boolean; ding: boolean; keyAt: number | null; dingAt: number | null;
  };
};
const T = (await import(join(import.meta.dir, '..', 'desktop', 'ui', 'page-timing.js'))) as Timing;
const {
  DING_GAP_MS, GLOW_MS, KEY_GAP_MS, MAX_SHOW_MS, STEP_MS, SWEEP_MS, TYPE_MS, WAVE_LAG,
  endAt, formAt, formedAt, frameAt, glowingAt, hasPage, planShow, shownText, soundsDue,
  stepFor, typedAt,
} = T;

const PAGE = [
  'EXT. RIDGE ROAD - DAY',
  'Wind comes down off the ridge in long flat gusts.',
  'DELACROIX',
  'Thirty-one.',
  'They take the bend.',
  'MARA',
  'You don’t have to do that out loud.',
];

/** Every whole millisecond from 0 to `to`. */
const moments = (to: number, by = 1) => Array.from({ length: Math.floor(to / by) + 1 }, (_, i) => i * by);

describe('planShow', () => {
  test('counts characters as code points, so nothing is typed in halves', () => {
    const plan = planShow(['Café 🎬', 'Noël']);
    expect(plan.blocks.map((b) => b.chars.length)).toEqual([6, 4]);
    expect(plan.total).toBe(10);
    expect(plan.blocks.map((b) => [b.start, b.end])).toEqual([[0, 6], [6, 10]]);
  });

  test('leaves out empty paragraphs, and anything that is not a list is no page', () => {
    expect(planShow(['', '  ', 'A']).blocks.map((b) => b.text)).toEqual(['A']);
    for (const nothing of [null, undefined, 'text', 3, {}]) {
      expect(hasPage(planShow(nothing))).toBe(false);
    }
    expect(hasPage(planShow(['A']))).toBe(true);
  });
});

describe('the typing', () => {
  const plan = planShow(PAGE);

  test('takes TYPE_MS whatever the length of the page', () => {
    for (const page of [['A'], PAGE, [Array(5000).fill('x').join('')]]) {
      const p = planShow(page);
      expect(typedAt(p, TYPE_MS - 1)).toBeLessThan(p.total);
      expect(typedAt(p, TYPE_MS)).toBe(p.total);
      expect(typedAt(p, TYPE_MS * 10)).toBe(p.total);
    }
  });

  test('starts at nothing, never goes back, and spreads evenly over the time', () => {
    expect(typedAt(plan, 0)).toBe(0);
    expect(typedAt(plan, -50)).toBe(0);
    let before = 0;
    for (const t of moments(TYPE_MS)) {
      const now = typedAt(plan, t);
      expect(now).toBeGreaterThanOrEqual(before);
      // Even: the count is the share of the time gone, to the character.
      expect(Math.abs(now - (t / TYPE_MS) * plan.total)).toBeLessThan(1);
      before = now;
    }
    expect(typedAt(plan, TYPE_MS / 2)).toBe(Math.floor(plan.total / 2));
  });

  test('shows each block up to what is typed, in order', () => {
    const at = plan.blocks[1].start + 4;
    const shown = shownText(plan, at);
    expect(shown[0]).toBe(PAGE[0]);
    expect(shown[1]).toBe(PAGE[1].slice(0, 4));
    expect(shown.slice(2).every((s) => s === '')).toBe(true);
    expect(shownText(plan, plan.total)).toEqual(PAGE);
  });
});

describe('the wave', () => {
  const plan = planShow(PAGE);
  const n = plan.blocks.length;
  /** When block i is finished being typed. */
  const typedEnd = (i: number) => Math.ceil((plan.blocks[i].end * TYPE_MS) / plan.total);

  test('waits for the book: no block forms, and the show has no end, while it is not there', () => {
    for (const t of moments(TYPE_MS * 3, 7)) {
      expect(formedAt(plan, null, t)).toBe(0);
      expect(frameAt(plan, null, t).done).toBe(false);
    }
    expect(endAt(plan, null)).toBeNull();
    expect(formAt(plan, null, 0)).toBeNull();
  });

  test('with the book there first, runs two blocks behind the typing', () => {
    for (let i = 0; i < n - WAVE_LAG + 1; i += 1) {
      // Block i forms the moment block i + 1 is typed out, as block i + 2
      // begins: never before.
      expect(formAt(plan, 0, i)).toBe(Math.max(typedEnd(i + WAVE_LAG - 1), i * stepFor(plan)));
    }
    for (const t of moments(TYPE_MS, 5)) {
      const typed = typedAt(plan, t);
      const typingIn = plan.blocks.findIndex((b) => typed < b.end);
      const formed = formedAt(plan, 0, t);
      if (typingIn >= 0) expect(formed).toBeLessThanOrEqual(Math.max(0, typingIn - WAVE_LAG + 1));
    }
  });

  test('forms the last blocks after the typing ends, one step apart, and ends with their glow', () => {
    const step = stepFor(plan);
    expect(formAt(plan, 0, n - 1)).toBe(TYPE_MS + step);
    expect(endAt(plan, 0)).toBe(TYPE_MS + step + GLOW_MS);
    expect(frameAt(plan, 0, endAt(plan, 0)! - 1).done).toBe(false);
    expect(frameAt(plan, 0, endAt(plan, 0)!).done).toBe(true);
  });

  test('with the book late, waits for it and then sweeps at the wave’s pace', () => {
    const bookAt = TYPE_MS + 2500;
    const step = stepFor(plan);
    expect(formedAt(plan, bookAt, bookAt - 1)).toBe(0);
    for (let i = 0; i < n; i += 1) expect(formAt(plan, bookAt, i)).toBe(bookAt + i * step);
    expect(endAt(plan, bookAt)).toBe(bookAt + (n - 1) * step + GLOW_MS);
  });

  test('with the book arriving mid-typing, sweeps the blocks it owes rather than forming them at once', () => {
    const bookAt = typedEnd(4);
    const step = stepFor(plan);
    expect(formedAt(plan, bookAt, bookAt)).toBe(1);
    expect(formAt(plan, bookAt, 1)).toBe(bookAt + step);
    expect(formAt(plan, bookAt, 2)).toBe(bookAt + 2 * step);
  });

  test('forms blocks top to bottom, never un-forms one, and glows each for GLOW_MS', () => {
    for (const bookAt of [0, 900, TYPE_MS, TYPE_MS + 4000]) {
      let before = 0;
      for (const t of moments(TYPE_MS + 8000, 3)) {
        const formed = formedAt(plan, bookAt, t);
        expect(formed).toBeGreaterThanOrEqual(before);
        before = formed;
        for (const i of glowingAt(plan, bookAt, t)) {
          expect(i).toBeLessThan(formed);
          expect(t - formAt(plan, bookAt, i)!).toBeLessThan(GLOW_MS);
        }
      }
      expect(before).toBe(n);
    }
  });

  test('a long page sweeps faster, so the sweep never takes longer than SWEEP_MS', () => {
    const long = planShow(Array.from({ length: 60 }, (_, i) => `Line ${i}`));
    expect(stepFor(long)).toBe(SWEEP_MS / 60);
    expect(stepFor(plan)).toBe(STEP_MS);
    const bookAt = TYPE_MS + 100;
    expect(formAt(long, bookAt, 59)! - bookAt).toBeLessThanOrEqual(SWEEP_MS);
  });

  test('the whole show is never longer than about six seconds plus what the engine still needs', () => {
    expect(MAX_SHOW_MS).toBeLessThanOrEqual(6000);
    for (const page of [['A'], PAGE, Array.from({ length: 200 }, () => 'x')]) {
      const p = planShow(page);
      for (const bookAt of [0, 1, 1000, TYPE_MS - 1, TYPE_MS, TYPE_MS + 1, 9000, 60000]) {
        const extra = Math.max(0, bookAt - TYPE_MS);
        expect(endAt(p, bookAt)!).toBeLessThanOrEqual(MAX_SHOW_MS + extra);
      }
    }
  });

  test('a page with nothing on it ends the moment the book is there', () => {
    const empty = planShow([]);
    expect(endAt(empty, 300)).toBe(300);
    expect(frameAt(empty, 300, 300)).toEqual({ typed: 0, shown: [], formed: 0, glowing: [], done: true });
  });
});

describe('the sounds', () => {
  const plan = planShow(PAGE);

  test('a key when more is typed, no more often than KEY_GAP_MS', () => {
    let last = { keyAt: null as number | null, dingAt: null as number | null };
    let keys = 0;
    let before = 0;
    for (const t of moments(TYPE_MS, 16)) {
      const after = typedAt(plan, t);
      const due = soundsDue(plan, before, after, t, last);
      if (due.key) {
        if (last.keyAt !== null) expect(t - last.keyAt).toBeGreaterThanOrEqual(KEY_GAP_MS);
        keys += 1;
      }
      last = due;
      before = after;
    }
    expect(keys).toBeGreaterThan(TYPE_MS / (KEY_GAP_MS * 2));
    expect(keys).toBeLessThanOrEqual(Math.ceil(TYPE_MS / KEY_GAP_MS) + 1);
  });

  test('nothing typed, no key; a finished block, a ding, no more often than DING_GAP_MS', () => {
    expect(soundsDue(plan, 5, 5, 100, null).key).toBe(false);
    const end = plan.blocks[0].end;
    expect(soundsDue(plan, end - 1, end, 100, null).ding).toBe(true);
    expect(soundsDue(plan, end - 1, end, 100, { keyAt: null, dingAt: 100 - DING_GAP_MS + 1 }).ding).toBe(false);
    expect(soundsDue(plan, end, end + 1, 100, null).ding).toBe(false);
  });
});
