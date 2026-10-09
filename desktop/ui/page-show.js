// The page writes itself (spec 2026-10-01, decisions 17 to 21). While a
// script converts, its first real page types out in the box where the
// result's first page will sit; once the book exists, each block takes its
// form two blocks behind the typing, with a brief brass glow, and the typed
// page lifts away to show the result's own page underneath it.
//
// convert.js only wires this in: it starts a show when the engine's page line
// arrives, hands it the progress lines and the book, and cancels it on a
// refusal. When it runs is page-timing.js's; what it sounds like is
// typewriter.js's. The file is in two halves, the way convert.js is: what the
// show decides is pure and exported above the line, the drawing is below.
import { el, clear, text } from './dom.js';
import { dressFrame } from './read.js';
import { resultFrame } from './result-page.js';
import { planShow, hasPage, frameAt, soundsDue, GLOW_MS } from './page-timing.js';

// ---------------------------------------------------------------- decisions

/** The words the show puts beside the page. */
export const SHOW = {
  heading: 'Converting',
  starting: 'Starting up',
  building: 'Building the book',
};

/** Whether a page line starts a show: the reader has not turned "Animate the
 *  page" off, the system is not asking for reduced motion, and there is a
 *  page in the line to type. Anything else keeps today's bar. */
export function canShow(state, line) {
  if (state?.reducedMotion === true) return false;
  if (state?.animatePage === false) return false;
  return line?.stage === 'page' && hasPage(planShow(line.paragraphs));
}

/** The two switches off an app-settings answer, or null when either is not
 *  a boolean (an engine from before the switches, or a broken answer). */
export function pageSwitchesFrom(answer) {
  const animatePage = answer?.animatePage;
  const typewriterSounds = answer?.typewriterSounds;
  if (typeof animatePage !== 'boolean' || typeof typewriterSounds !== 'boolean') return null;
  return { animatePage, typewriterSounds };
}

/** The slim line under the page, after one progress line: "Reading page N
 *  of M" while pages are read (or the share read, from an engine that does
 *  not name pages), then "Building the book". `at` is { line, page, stage }
 *  from the last call (null at first); it never goes back a page or a
 *  stage, so a late line cannot walk it backwards. */
export function readingLine(at, line) {
  const was = at ?? { line: SHOW.starting, page: 0, stage: null };
  const stage = line?.stage;
  if (stage === 'render') {
    return { line: SHOW.building, page: was.page, stage: 'render' };
  }
  if (stage !== 'parse' || was.stage === 'render') return was;
  const page = Number(line.page);
  const pages = Number(line.pages);
  if (Number.isInteger(page) && Number.isInteger(pages) && page >= 1 && page <= pages) {
    if (page < was.page) return was;
    return { line: `Reading page ${page} of ${pages}`, page, stage: 'parse' };
  }
  const percent = Number(line.percent);
  if (!Number.isFinite(percent) || was.page > 0) return was;
  const shown = Math.min(100, Math.max(0, Math.round(percent)));
  return { line: `Reading the pages (${shown}%)`, page: 0, stage: 'parse' };
}

/** A block's words reduced to what two copies of them share: letters and
 *  digits, upper case. Curly and straight quotes, spacing and punctuation
 *  all fall away. */
export function normalise(words) {
  return String(words ?? '').normalize('NFKC').toUpperCase().replace(/[^\p{L}\p{N}]+/gu, '');
}

/** Two blocks are the same block when their words match, or one is the
 *  start of the other and at least half its length: a cue the book writes
 *  as "MARA (CONT'D)" is still MARA, but a speech the page cut off halfway
 *  is not the whole speech. */
function sameBlock(a, b) {
  if (a === '' || b === '') return false;
  if (a === b) return true;
  const [short, long] = a.length <= b.length ? [a, b] : [b, a];
  return long.startsWith(short) && short.length * 2 >= long.length;
}

/** How far past the last match the next block may be found. */
const LOOK_AHEAD = 4;

/** Which of the book's blocks each typed paragraph is, by its words: an
 *  index into `blocks`, or null for a paragraph the book has no match for
 *  (that one keeps its typed form until the page lifts away). The first
 *  match may be anywhere, past a title page; each later one is looked for
 *  just after the one before, so the pairs stay in order. */
export function alignBlocks(paragraphs, blocks) {
  const want = (paragraphs ?? []).map(normalise);
  const have = (blocks ?? []).map(normalise);
  const out = [];
  let from = 0;
  let anchored = false;
  for (const words of want) {
    const to = anchored ? Math.min(have.length, from + LOOK_AHEAD) : have.length;
    let found = null;
    for (let j = from; j < to; j += 1) {
      if (sameBlock(words, have[j])) { found = j; break; }
    }
    out.push(found);
    if (found !== null) {
      from = found + 1;
      anchored = true;
    }
  }
  return out;
}

/** The stylesheet the typed page wears, around the engine's own when the
 *  book exists. The face comes first so the book's own replaces it; the
 *  rest comes after so it holds: the typed page starts where the result's
 *  scrolled page does (a line and a half down, at its first block), and a
 *  typed paragraph is plain and left-aligned, whatever the book does with
 *  it once it takes its form. `brass` is the window's own colour, by value:
 *  a custom property does not cross into the frame. */
export function typedCss(engineCss, brass) {
  // Unresolved, the glow is the text's own colour rather than a guess.
  const glow = String(brass ?? '').trim() || 'currentColor';
  return `body { font-family: "Courier Prime", "Courier New", monospace; }
${String(engineCss ?? '')}
body.typed-body { padding-top: 1.5em; }
body.typed-body section.scene { margin-top: 0; }
body.typed-body section.scene > :first-child { margin-top: 0; }
p.typed { margin: 0 0 1em 0; text-align: left; text-indent: 0; font-weight: normal; }
@keyframes page-show-glow {
  from { text-shadow: 0 0 0.5em ${glow}, 0 0 0.15em ${glow}; }
  to { text-shadow: none; }
}
.page-show-glow { animation: page-show-glow ${GLOW_MS}ms ease-out; }
`;
}

/** The typed page's own document: an empty scene, filled in once loaded. */
const TYPED_DOC = '<!doctype html><html><head></head><body class="typed-body"><section class="scene"></section></body></html>';

/** The window's colour token the glow is drawn in. */
const GLOW_TOKEN = 'brass';

/** How long the typed page takes to lift away at the end. */
export const FADE_MS = 250;

// ------------------------------------------------------------------ drawing

const realClock = {
  now: () => performance.now(),
  frame: (fn) => requestAnimationFrame(fn),
  later: (fn, ms) => setTimeout(fn, ms),
};

/** The words a block of the book's page carries, without its page mark. */
function blockWords(node) {
  const marker = node.querySelector?.('.page-marker');
  const all = String(node.textContent ?? '');
  return marker ? all.replace(String(marker.textContent ?? ''), '') : all;
}

/** Starts the show in `pane`, replacing whatever is there.
 *
 *  `name` is the file being converted; `clock` is { now, frame, later }
 *  (the window's own by default; a test hands in one it turns by hand);
 *  `sounds` is typewriter.js's object, or null for a quiet show; `onEnd`
 *  hears the show finish, by itself, by a click or key, or by cancel(). */
export function beginShow(pane, paragraphs, { name = '', clock = realClock, sounds = null, onEnd = () => {} } = {}) {
  const plan = planShow(paragraphs);
  const start = clock.now();
  let bookAt = null;
  let sources = null;
  let skipped = false;
  let over = false;
  let reading = null;
  let typedBefore = 0;
  let heard = { keyAt: null, dingAt: null };
  /** The typed frame's document once loaded, its scene, and one node per
   *  block: the typed paragraph, or the book's block once it has formed. */
  let typedDoc = null;
  let scene = null;
  let nodes = [];
  let formed = 0;
  const wrappers = new Map();
  const brass = getComputedStyle(document.documentElement).getPropertyValue(`--${GLOW_TOKEN}`);

  // ---- the page and its words

  const typedFrame = el('iframe', {
    class: 'page-show-frame',
    title: 'First page',
    sandbox: 'allow-same-origin',
    tabindex: '-1',
  });
  const box = el('div', { class: 'result-page page-show', 'aria-hidden': 'true' }, typedFrame);
  const slim = el('p', { class: 'page-show-line read-out', role: 'status', 'aria-live': 'polite' }, SHOW.starting);
  // The text is in the window for a screen reader from the first moment,
  // whole: the typing is for eyes, and the page it happens on is hidden
  // from assistive technology like the result's own page is.
  const words = el('div', { class: 'page-show-words' }, ...plan.blocks.map((b) => el('p', {}, b.text)));
  const side = el('div', { class: 'result-side' },
    el('h2', { class: 'result-title' }, SHOW.heading),
    el('p', { class: 'book-count' }, name),
    words);
  const result = el('div', { class: 'result', 'data-showing': 'on' }, box, side, slim);

  clear(pane);
  pane.dataset.state = 'working';
  pane.dataset.show = 'on';
  pane.append(result);

  typedFrame.addEventListener('load', () => {
    const doc = typedFrame.contentDocument;
    const found = doc?.querySelector?.('section.scene');
    // about:blank loads first and fires `load` too; it has no scene.
    if (!found || over) return;
    typedDoc = doc;
    scene = found;
    dressFrame(typedFrame, typedCss('', brass));
    nodes = plan.blocks.map(() => {
      const p = doc.createElement('p');
      p.className = 'typed';
      scene.append(p);
      return p;
    });
    formed = 0;
    paint();
  });
  typedFrame.setAttribute('srcdoc', TYPED_DOC);

  // ---- the clock

  function elapsed() {
    return clock.now() - start;
  }

  function paint() {
    if (over) return;
    const t = elapsed();
    const now = frameAt(plan, bookAt, t);
    const typed = skipped ? plan.total : now.typed;
    if (sounds !== null && !skipped) {
      const due = soundsDue(plan, typedBefore, typed, t, heard);
      if (due.key) sounds.key();
      if (due.ding) sounds.ding();
      heard = due;
    }
    typedBefore = typed;
    if (typedDoc !== null) {
      const shown = skipped ? plan.blocks.map((b) => b.text) : now.shown;
      for (let i = formed; i < nodes.length; i += 1) {
        if (nodes[i].textContent !== shown[i]) nodes[i].textContent = shown[i];
      }
      const due = sources === null ? 0 : now.formed;
      while (formed < due) formBlock(formed++);
    }
    if (now.done) finish({ fade: true });
  }

  function loop() {
    if (over) return;
    paint();
    if (!over) clock.frame(loop);
  }
  clock.frame(loop);

  /** Block i takes its form: the book's own block, glowing, in the typed
   *  paragraph's place; inside its dialogue block when the book has one,
   *  so the speech sits the way it will. A block the book has no match for
   *  only glows. */
  function formBlock(i) {
    const typed = nodes[i];
    const source = sources[i] ?? null;
    if (source === null) {
      typed.classList.add('page-show-glow');
      return;
    }
    const block = typedDoc.importNode(source, true);
    block.classList.add('page-show-glow');
    const parent = source.parentNode;
    if (parent && parent.localName === 'div') {
      let wrapper = wrappers.get(parent);
      if (wrapper === undefined || !wrapper.isConnected) {
        wrapper = typedDoc.importNode(parent, false);
        wrappers.set(parent, wrapper);
        scene.insertBefore(wrapper, typed);
      }
      typed.remove();
      wrapper.append(block);
    } else {
      typed.replaceWith(block);
    }
    nodes[i] = block;
  }

  // ---- the end

  function stopListening() {
    document.removeEventListener('click', onPress, true);
    removeEventListener('keydown', onPress, true);
  }

  /** Over. With `fade`, the typed page lifts away over FADE_MS; without,
   *  it goes at once. Either way the result's page is under it. */
  function finish({ fade = false } = {}) {
    if (over) return;
    over = true;
    stopListening();
    sounds?.close?.();
    delete pane.dataset.show;
    result.removeAttribute('data-showing');
    slim.remove();
    const lift = () => {
      typedFrame.remove();
      box.classList.remove('page-show');
    };
    if (fade) {
      typedFrame.classList.add('page-show-gone');
      clock.later(lift, FADE_MS);
    } else {
      lift();
    }
    onEnd();
  }

  /** A click anywhere, or any key: the show is over at once. Before the
   *  book exists that means the whole page typed, and the result as soon as
   *  it is there. */
  function onPress() {
    if (over) return;
    if (bookAt !== null) {
      finish();
      return;
    }
    skipped = true;
    stopListening();
    paint();
  }
  document.addEventListener('click', onPress, true);
  addEventListener('keydown', onPress, true);

  return {
    /** The show's box, while it is on screen. */
    box,
    /** A progress line from the engine. */
    progress(line) {
      if (over) return;
      reading = readingLine(reading, line);
      text(slim, reading.line);
    },
    /** The book exists: `side` (the result's right half) takes the place of
     *  the show's, and the result's page goes in under the typed one. False
     *  when the show is no longer on screen, and the caller draws the
     *  result itself. */
    adopt(newSide, script) {
      if (over || !box.isConnected) return false;
      side.replaceWith(newSide);
      slim.remove();
      result.removeAttribute('data-showing');
      const frame = resultFrame(script, (inside, css) => {
        if (over) return;
        const blocks = [...inside.body.querySelectorAll('h2, p')];
        const matched = alignBlocks(plan.blocks.map((b) => b.text), blocks.map(blockWords));
        sources = matched.map((j) => (j === null ? null : blocks[j]));
        if (typedDoc !== null) {
          // The book's own sheet now, so a typed paragraph is in its face.
          dressFrame(typedFrame, typedCss(css, brass));
        }
        bookAt = elapsed();
        if (skipped) finish();
        else paint();
      });
      if (frame === null) {
        sources = [];
        bookAt = elapsed();
        if (skipped) finish();
      } else {
        box.insertBefore(frame, typedFrame);
      }
      return true;
    },
    /** Over at once, as a click would. */
    finish: () => finish(),
    /** Over, and the pane is someone else's now (a refusal, another file). */
    cancel() {
      if (over) return;
      over = true;
      stopListening();
      sounds?.close?.();
      delete pane.dataset.show;
      onEnd();
    },
    /** Whether the show is still running. */
    running: () => !over,
  };
}
