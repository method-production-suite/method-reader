// The clock of "the page writes itself" (spec 2026-10-01, decisions 17 to
// 19): which character is typed at a moment, which blocks have taken their
// form, and when the show is over. Pure: every function here takes the time
// as a number of milliseconds since the typing began, so a test can ask about
// any moment without waiting for it. page-show.js reads a real clock and
// draws what these say.
//
// The show runs on its own clock, never the engine's. A conversion takes
// milliseconds, so waiting on it would be no show at all; a long one would
// stall the typing. So the typing always takes TYPE_MS, spread evenly over
// the page's characters, and the book only decides when the wave may start.

/** How long the typing takes, whatever the script. */
export const TYPE_MS = 4000;

/** How many blocks the wave runs behind the typing. */
export const WAVE_LAG = 2;

/** The wave's pace once nothing holds it back: one block per STEP_MS, or
 *  faster on a page with many blocks, so a whole sweep never takes longer
 *  than SWEEP_MS. */
export const STEP_MS = 120;
export const SWEEP_MS = 1200;

/** How long a block's brass glow lasts once it has taken its form. */
export const GLOW_MS = 600;

/** The longest whole show when the book is there before the typing ends:
 *  the typing, a full sweep and the last glow. With a slower engine the show
 *  ends this long after the book exists at most. */
export const MAX_SHOW_MS = TYPE_MS + SWEEP_MS + GLOW_MS;

/** The gap between two key sounds: about eighteen a second, which reads as
 *  brisk typing. One sound per character would be a buzz, not a typewriter. */
export const KEY_GAP_MS = 55;

/** The shortest gap between two carriage-return dings. */
export const DING_GAP_MS = 700;

/** A page, ready to be timed: its blocks, each with its characters (code
 *  points, so an accent or an emoji is never typed in halves) and the
 *  running count of characters up to its end. Empty paragraphs are left
 *  out: there is nothing to type in them. */
export function planShow(paragraphs) {
  const blocks = [];
  let total = 0;
  for (const paragraph of Array.isArray(paragraphs) ? paragraphs : []) {
    const chars = Array.from(String(paragraph ?? '').trim());
    if (chars.length === 0) continue;
    const start = total;
    total += chars.length;
    blocks.push({ text: chars.join(''), chars, start, end: total });
  }
  return { blocks, total };
}

/** Whether there is a page to type at all. */
export function hasPage(plan) {
  return plan.total > 0;
}

/** How many characters are typed `t` ms after the typing began. */
export function typedAt(plan, t) {
  if (plan.total === 0 || !(t > 0)) return 0;
  if (t >= TYPE_MS) return plan.total;
  return Math.min(plan.total, Math.floor((t * plan.total) / TYPE_MS));
}

/** When the `count`th character lands: the inverse of typedAt. */
function typedBy(plan, count) {
  if (count <= 0) return 0;
  return Math.ceil((count * TYPE_MS) / plan.total);
}

/** How much of each block shows when `typed` characters are down. */
export function shownText(plan, typed) {
  return plan.blocks.map((b) => b.chars.slice(0, Math.max(0, Math.min(b.chars.length, typed - b.start))).join(''));
}

/** The wave's pace on this page. */
export function stepFor(plan) {
  const n = plan.blocks.length;
  return n === 0 ? STEP_MS : Math.min(STEP_MS, SWEEP_MS / n);
}

/** When block `i` takes its form, given `bookAt`, the moment the book was
 *  there to take the form from (null while it is not).
 *
 *  Two blocks behind the typing: block i forms as block i + 2 starts, which
 *  is the moment block i + 1 is finished. The last blocks, with nothing two
 *  blocks on to wait for, follow the typing's end at the wave's pace. And
 *  never before the book plus the wave's own pace for the blocks ahead of
 *  it: a book that turns up late is swept in at that pace rather than
 *  appearing all at once. */
export function formAt(plan, bookAt, i) {
  if (bookAt === null || bookAt === undefined) return null;
  const n = plan.blocks.length;
  if (i < 0 || i >= n) return null;
  const step = stepFor(plan);
  const ahead = i + WAVE_LAG - 1;
  const behindTyping = ahead < n
    ? typedBy(plan, plan.blocks[ahead].end)
    : TYPE_MS + (ahead - n + 1) * step;
  return Math.max(behindTyping, bookAt + i * step);
}

/** How many blocks have their form `t` ms in. Always a run from the top. */
export function formedAt(plan, bookAt, t) {
  let count = 0;
  for (let i = 0; i < plan.blocks.length; i += 1) {
    const at = formAt(plan, bookAt, i);
    if (at === null || at > t) break;
    count += 1;
  }
  return count;
}

/** The blocks still glowing `t` ms in. */
export function glowingAt(plan, bookAt, t) {
  const out = [];
  for (let i = 0; i < plan.blocks.length; i += 1) {
    const at = formAt(plan, bookAt, i);
    if (at !== null && at <= t && t < at + GLOW_MS) out.push(i);
  }
  return out;
}

/** When the show is over: the last block's glow done, or the typing done on
 *  a page with no block to form. Null until the book is there. */
export function endAt(plan, bookAt) {
  if (bookAt === null || bookAt === undefined) return null;
  const n = plan.blocks.length;
  if (n === 0) return bookAt;
  return formAt(plan, bookAt, n - 1) + GLOW_MS;
}

/** Everything the page shows `t` ms in, in one answer. */
export function frameAt(plan, bookAt, t) {
  const typed = typedAt(plan, t);
  const end = endAt(plan, bookAt);
  return {
    typed,
    shown: shownText(plan, typed),
    formed: formedAt(plan, bookAt, t),
    glowing: glowingAt(plan, bookAt, t),
    done: end !== null && t >= end,
  };
}

/** Which sounds are due between two frames: a key when more is typed and
 *  the last key was long enough ago; a ding when a block has just been
 *  finished and the last ding was long enough ago. `last` is
 *  { keyAt, dingAt } (null for never); the answer carries the new ones. */
export function soundsDue(plan, before, after, t, last) {
  const keyAt = last?.keyAt ?? null;
  const dingAt = last?.dingAt ?? null;
  const key = after > before && (keyAt === null || t - keyAt >= KEY_GAP_MS);
  const finished = plan.blocks.some((b) => before < b.end && b.end <= after);
  const ding = finished && (dingAt === null || t - dingAt >= DING_GAP_MS);
  return { key, ding, keyAt: key ? t : keyAt, dingAt: ding ? t : dingAt };
}
