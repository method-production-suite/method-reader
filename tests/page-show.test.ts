// The page writes itself (desktop/ui/page-show.js), driven through the real
// convert.js on the fake page: the engine's page line arrives, the clock is
// turned by hand, and what the reader would see is read off the frames.
// The fake has no frames of its own, so each test hands the typed page and
// the result's page a fake document when it "loads" them.
import { afterEach, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import {
  FakeDocument, FakeNode, HOLD, fakePage, settle, withGlobals, type FakePage,
} from './helpers/fake-dom';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
type Timing = {
  TYPE_MS: number; GLOW_MS: number;
  planShow(p: string[]): unknown;
  formAt(plan: unknown, bookAt: number | null, i: number): number | null;
  endAt(plan: unknown, bookAt: number | null): number | null;
};
const T = (await import(join(UI, 'page-timing.js'))) as Timing;
const S = (await import(join(UI, 'page-show.js'))) as Record<string, any>;

const PDF = '/s/Field Station.pdf';
const PAGE = [
  'EXT. RIDGE ROAD - DAY',
  'Wind comes down off the ridge in long flat gusts.',
  'DELACROIX',
  'Thirty-one.',
  'They take the bend.',
];
const pageLine = (paragraphs = PAGE) => JSON.stringify({ progress: { stage: 'page', page: 2, paragraphs } });
const converted = {
  ok: true, title: 'Field Station', author: null, pages: 18, scenes: 12, characters: 6, warnings: [],
  epubPath: '/lib/Field Station/Field Station.epub', fountainPath: '/lib/Field Station/Field Station.fountain',
  previewHtml: '<html><head><style>p{}</style></head><body><section class="scene"></section></body></html>',
};

/** A clock the test turns: frames wait for run(), later() for the time. */
function handClock() {
  let now = 1000;
  let frames: Array<() => void> = [];
  const laters: Array<{ at: number; fn: () => void }> = [];
  return {
    clock: {
      now: () => now,
      frame: (fn: () => void) => { frames.push(fn); },
      later: (fn: () => void, ms: number) => { laters.push({ at: now + ms, fn }); },
    },
    /** Moves the time on by `ms` and draws one frame. */
    advance(ms: number) {
      now += ms;
      const due = frames;
      frames = [];
      for (const fn of due) fn();
      for (const l of laters.splice(0)) {
        if (l.at <= now) l.fn();
        else laters.push(l);
      }
    },
  };
}

/** A frame's document, as the frame would load it. */
function loadFrame(frame: FakeNode, fill: (doc: FakeDocument, body: FakeNode) => void) {
  const doc = new FakeDocument();
  fill(doc, doc.body);
  frame.contentDocument = doc;
  frame.contentWindow = {
    CSSStyleSheet: class { text = ''; replaceSync(t: string) { this.text = t; } },
    getComputedStyle: () => ({ fontSize: '16px' }),
    scrollTo: () => {},
  };
  frame.fire('load');
  return doc;
}

/** The typed page's own document: an empty scene, as page-show.js writes it. */
function loadTyped(pane: FakeNode) {
  const frame = pane.querySelector('iframe.page-show-frame')!;
  return loadFrame(frame, (doc, body) => {
    body.className = 'typed-body';
    body.append(doc.createElement('section'));
    body.firstChild!.className = 'scene';
  });
}

/** The book's page, as the result frame would load it. */
function loadBook(pane: FakeNode) {
  const frame = pane.querySelector('iframe.result-page-frame')!;
  return loadFrame(frame, (doc, body) => {
    const block = (tag: string, cls: string, words: string) => {
      const n = doc.createElement(tag);
      n.className = cls;
      n.append(words);
      return n;
    };
    const scene = doc.createElement('section');
    scene.className = 'scene';
    const speech = doc.createElement('div');
    speech.className = 'dialogue-block';
    speech.append(block('p', 'character', 'DELACROIX'), block('p', 'dialogue', 'Thirty-one.'));
    scene.append(
      block('h2', 'scene-heading', 'EXT. RIDGE ROAD - DAY'),
      block('p', 'action', 'Wind comes down off the ridge in long flat gusts.'),
      speech,
      block('p', 'action', 'They take the bend.'),
    );
    body.append(scene);
  });
}

const typedText = (doc: FakeDocument) => doc.querySelector('section.scene')!.textContent;

describe('the page writes itself', () => {
  let page: FakePage | null = null;
  let copies = 0;
  afterEach(async () => { await page?.close(); page = null; });

  async function mount(state: Record<string, unknown> = {}, engine?: (args: string[]) => unknown) {
    page = fakePage({ respond: engine ?? (() => HOLD) });
    copies += 1;
    const convert = await import(`${join(UI, 'convert.js')}?show-${copies}`);
    const pane = page.pane();
    pane.id = 'surface-convert';
    const hand = handClock();
    const ctx = {
      state: { script: null, reducedMotion: false, ...state } as Record<string, any>,
      goTo: () => {},
      restoreFocus: () => {},
      scriptChanged: () => {},
      convertAgain: () => {},
      clock: hand.clock,
    };
    convert.mount(pane, ctx);
    await settle();
    const going = convert.convertPath(PDF);
    await settle();
    const call = () => page!.tauri.callsTo(PDF)[0];
    return { convert, pane, ctx, hand, going, call, tauri: page.tauri, doc: page.doc };
  }

  test('the engine’s page replaces the bar with the result’s own box, its words whole for a screen reader at once', async () => {
    const { pane, tauri, call, going } = await mount();
    expect(pane.querySelector('.meter')).not.toBeNull();
    tauri.emit('engine-line', pageLine());
    expect(pane.querySelector('.meter')).toBeNull();
    expect(pane.dataset.state).toBe('working');
    const box = pane.querySelector('.result .result-page.page-show')!;
    expect(box.getAttribute('aria-hidden')).toBe('true');
    expect(box.querySelector('iframe.page-show-frame')).not.toBeNull();
    // Whole from the first moment, outside the hidden box.
    const words = pane.querySelector('.page-show-words')!;
    expect(words.querySelectorAll('p').map((p) => p.textContent)).toEqual(PAGE);
    expect(words.closest('[aria-hidden]')).toBeNull();
    expect(pane.querySelector('.page-show-line')!.textContent).toBe(S.SHOW.starting);
    call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
    await going;
  });

  test('types on its own clock: about half the page at two seconds, all of it at four, whatever the engine is doing', async () => {
    const { pane, tauri, hand, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    const typed = loadTyped(pane);
    const all = PAGE.join('');
    hand.advance(0);
    expect(typedText(typed)).toBe('');
    hand.advance(T.TYPE_MS / 2);
    expect(typedText(typed).length).toBe(Math.floor(all.length / 2));
    expect(all.startsWith(typedText(typed))).toBe(true);
    // Plain while typed: every block is a typed paragraph.
    expect(typed.querySelectorAll('p.typed').length).toBe(PAGE.length);
    hand.advance(T.TYPE_MS / 2);
    expect(typedText(typed)).toBe(all);
    // The engine has not answered, so nothing has formed and the show waits.
    expect(typed.querySelectorAll('.page-show-glow').length).toBe(0);
    expect(pane.querySelector('iframe.page-show-frame')).not.toBeNull();
    call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
    await going;
  });

  test('the slim line says which page is being read, then that the book is being built', async () => {
    const { pane, tauri, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    const line = () => pane.querySelector('.page-show-line')!.textContent;
    tauri.emit('engine-line', '{"progress":{"stage":"parse","percent":17,"page":3,"pages":18}}');
    expect(line()).toBe('Reading page 3 of 18');
    tauri.emit('engine-line', '{"progress":{"stage":"parse","percent":11,"page":2,"pages":18}}');
    expect(line()).toBe('Reading page 3 of 18');
    tauri.emit('engine-line', '{"progress":{"stage":"render","percent":85}}');
    expect(line()).toBe(S.SHOW.building);
    expect(pane.querySelector('.page-show-line')!.getAttribute('role')).toBe('status');
    call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
    await going;
  });

  test('with the book there first, the result appears beside the page at once, and the wave runs two blocks behind', async () => {
    const { pane, tauri, hand, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    const typed = loadTyped(pane);
    const box = pane.querySelector('.result-page')!;
    call().answer(converted);
    await going;
    await settle();
    // The result's buttons are there now, working, and the page is the same box.
    expect(pane.dataset.state).toBe('done');
    expect(pane.querySelector('.result-title')!.textContent).toBe('Field Station is ready');
    expect(pane.buttons('Convert another').length).toBe(1);
    expect(pane.querySelector('.result-page')).toBe(box);
    expect(pane.querySelector('.page-show-line')).toBeNull();
    loadBook(pane);
    const plan = T.planShow(PAGE);
    const bookAt = 0;
    // Before block 0's moment, nothing has its form.
    hand.advance(T.formAt(plan, bookAt, 0)! - 1);
    expect(typed.querySelectorAll('.page-show-glow').length).toBe(0);
    hand.advance(1);
    const heading = typed.querySelector('h2.scene-heading')!;
    expect(heading.textContent).toBe(PAGE[0]);
    expect(heading.classList.contains('page-show-glow')).toBe(true);
    // The cue and its line go into the book's own dialogue block.
    hand.advance(T.formAt(plan, bookAt, 3)! - T.formAt(plan, bookAt, 0)!);
    const speech = typed.querySelector('div.dialogue-block')!;
    expect(speech.querySelectorAll('p').map((p) => p.className.split(' ')[0])).toEqual(['character', 'dialogue']);
    expect(typed.querySelectorAll('p.typed').length).toBe(1);
    // At the end the typed page lifts away, and the result's page is left.
    hand.advance(T.endAt(plan, bookAt)! - T.formAt(plan, bookAt, 3)!);
    expect(pane.querySelector('iframe.page-show-frame')!.classList.contains('page-show-gone')).toBe(true);
    hand.advance(S.FADE_MS);
    expect(pane.querySelector('iframe.page-show-frame')).toBeNull();
    expect(box.querySelector('iframe.result-page-frame')).not.toBeNull();
    expect(box.classList.contains('page-show')).toBe(false);
    expect(pane.dataset.show).toBeUndefined();
  });

  test('with the book late, the typing finishes, the wave waits for it, then sweeps', async () => {
    const { pane, tauri, hand, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    const typed = loadTyped(pane);
    hand.advance(T.TYPE_MS + 2000);
    expect(typed.querySelectorAll('.page-show-glow').length).toBe(0);
    call().answer(converted);
    await going;
    await settle();
    loadBook(pane);
    hand.advance(0);
    expect(typed.querySelectorAll('.page-show-glow').length).toBe(1);
    hand.advance(10_000);
    hand.advance(S.FADE_MS);
    expect(pane.querySelector('iframe.page-show-frame')).toBeNull();
  });

  test('a click anywhere finishes the show at once, and the click still does its work', async () => {
    const { pane, tauri, hand, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    loadTyped(pane);
    call().answer(converted);
    await going;
    await settle();
    loadBook(pane);
    hand.advance(500);
    expect(pane.querySelector('iframe.page-show-frame')).not.toBeNull();
    pane.button('Convert another').click();
    expect(pane.querySelector('iframe.page-show-frame')).toBeNull();
    expect(pane.dataset.state).toBe('idle');
  });

  test('a key before the book exists types the whole page at once, and the result comes without a wave', async () => {
    const { pane, tauri, hand, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    const typed = loadTyped(pane);
    hand.advance(300);
    page!.fireWindow('keydown', { key: 'Escape' });
    expect(typedText(typed)).toBe(PAGE.join(''));
    call().answer(converted);
    await going;
    await settle();
    loadBook(pane);
    expect(pane.querySelector('iframe.page-show-frame')).toBeNull();
    expect(pane.querySelector('iframe.result-page-frame')).not.toBeNull();
  });

  test('a refusal clears the typed page and shows the refusal as always', async () => {
    const { pane, tauri, call, going } = await mount();
    tauri.emit('engine-line', pageLine());
    call().answer({ ok: false, error: { code: 'not-screenplay', message: 'No scene headings found.' } });
    await going;
    expect(pane.dataset.state).toBe('failed');
    expect(pane.querySelector('.page-show')).toBeNull();
    expect(pane.dataset.show).toBeUndefined();
    expect(pane.querySelector('.fault-body')!.textContent).toBe('No scene headings found.');
    // Nothing is listening any more: a click is just a click.
    pane.button('Back to one').click();
    expect(pane.dataset.state).toBe('idle');
  });

  test('“Animate the page” off, or the system’s Reduce Motion, keeps today’s bar', async () => {
    for (const state of [{ animatePage: false }, { reducedMotion: true }]) {
      const { pane, tauri, call, going } = await mount(state);
      tauri.emit('engine-line', pageLine());
      expect(pane.querySelector('.page-show')).toBeNull();
      expect(pane.querySelector('.meter')).not.toBeNull();
      call().answer(converted);
      await going;
      expect(pane.dataset.state).toBe('done');
      expect(pane.querySelector('iframe.page-show-frame')).toBeNull();
      await page!.close();
      page = null;
    }
  });

  test('a page line with nothing in it keeps the bar', async () => {
    const { pane, tauri, call, going } = await mount();
    tauri.emit('engine-line', pageLine([]));
    expect(pane.querySelector('.meter')).not.toBeNull();
    call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
    await going;
  });

  test('the typewriter sounds play only when they are on', async () => {
    for (const on of [false, true]) {
      const made: string[] = [];
      const g = withGlobals();
      g.set('AudioContext', class {
        sampleRate = 8000;
        currentTime = 0;
        destination = {};
        constructor() { made.push('context'); }
        createBuffer() { return { getChannelData: () => new Float32Array(400) }; }
        createGain() { return { gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect() {} }; }
        createBiquadFilter() { return { frequency: {}, Q: {}, connect() {} }; }
        createBufferSource() { return { connect() {}, start() { made.push('key'); }, stop() {} }; }
        createOscillator() { return { frequency: {}, connect() {}, start() { made.push('ding'); }, stop() {} }; }
        close() {}
      });
      try {
        const { pane, tauri, hand, call, going } = await mount({ typewriterSounds: on });
        tauri.emit('engine-line', pageLine());
        loadTyped(pane);
        for (let t = 0; t < T.TYPE_MS; t += 16) hand.advance(16);
        if (on) {
          expect(made.filter((m) => m === 'key').length).toBeGreaterThan(20);
          expect(made).toContain('ding');
        } else {
          expect(made).toEqual([]);
        }
        call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
        await going;
        await page!.close();
        page = null;
      } finally {
        g.restore();
      }
    }
  });

  test('the well’s app-settings probe brings the two switches into the window’s state', async () => {
    const { ctx, call, going } = await mount({}, (args) => (args[0] === 'app-settings'
      ? { ok: true, library: null, animatePage: false, typewriterSounds: true }
      : HOLD));
    expect(ctx.state.animatePage).toBe(false);
    expect(ctx.state.typewriterSounds).toBe(true);
    call().answer({ ok: false, error: { code: 'internal', message: 'x' } });
    await going;
  });
});

describe('what the show decides', () => {
  test('canShow wants a page and nobody saying no', () => {
    const line = { stage: 'page', paragraphs: ['A'] };
    expect(S.canShow({}, line)).toBe(true);
    expect(S.canShow({ animatePage: true }, line)).toBe(true);
    expect(S.canShow({ animatePage: false }, line)).toBe(false);
    expect(S.canShow({ reducedMotion: true }, line)).toBe(false);
    expect(S.canShow({}, { stage: 'page', paragraphs: [] })).toBe(false);
    expect(S.canShow({}, { stage: 'parse', percent: 4 })).toBe(false);
  });

  test('readingLine falls back to the share read from an engine that names no pages', () => {
    const a = S.readingLine(null, { stage: 'parse', percent: 40 });
    expect(a.line).toBe('Reading the pages (40%)');
    expect(S.readingLine(a, { stage: 'nonsense' })).toBe(a);
    const b = S.readingLine(a, { stage: 'parse', percent: 50, page: 9, pages: 18 });
    expect(b.line).toBe('Reading page 9 of 18');
    expect(S.readingLine(b, { stage: 'parse', percent: 60 })).toBe(b);
    const c = S.readingLine(b, { stage: 'render', percent: 85 });
    expect(c.line).toBe(S.SHOW.building);
    expect(S.readingLine(c, { stage: 'parse', percent: 90, page: 18, pages: 18 })).toBe(c);
  });

  test('readingLine says the pictures are being read, between the pages and the book, and never goes back', () => {
    const a = S.readingLine(null, { stage: 'parse', percent: 50, page: 18, pages: 18 });
    const b = S.readingLine(a, { stage: 'pictures', percent: 70 });
    expect(b).toEqual({ line: S.SHOW.pictures, page: 18, stage: 'pictures' });
    // A late page line cannot walk it back to the pages.
    expect(S.readingLine(b, { stage: 'parse', percent: 55, page: 18, pages: 18 })).toBe(b);
    const c = S.readingLine(b, { stage: 'render', percent: 85 });
    expect(c.line).toBe(S.SHOW.building);
    // Nor a late pictures line back from the book.
    expect(S.readingLine(c, { stage: 'pictures', percent: 84 })).toBe(c);
  });

  test('alignBlocks pairs the typed page with the book’s blocks by their words, in order', () => {
    const book = ['TITLE', 'by Someone', 'EXT. RIDGE ROAD - DAY', 'Wind comes down.', 'DELACROIX (CONT’D)', 'Thirty-one.'];
    expect(S.alignBlocks(['EXT. RIDGE ROAD - DAY', 'Wind comes down.', 'DELACROIX', 'Thirty-one.'], book))
      .toEqual([2, 3, 4, 5]);
    // A speech the page cut off is not the whole speech; a stray line has no match.
    expect(S.alignBlocks(['Wind', 'Nowhere in the book.'], book)).toEqual([null, null]);
    expect(S.alignBlocks([], book)).toEqual([]);
    // Each later match is looked for just after the one before.
    expect(S.alignBlocks(['EXT. RIDGE ROAD - DAY', 'TITLE'], book)).toEqual([2, null]);
  });

  test('pageSwitchesFrom takes only two booleans', () => {
    expect(S.pageSwitchesFrom({ animatePage: true, typewriterSounds: false })).toEqual({ animatePage: true, typewriterSounds: false });
    expect(S.pageSwitchesFrom({ animatePage: 'yes', typewriterSounds: false })).toBeNull();
    expect(S.pageSwitchesFrom(null)).toBeNull();
  });
});

describe('the Settings page’s two switches', () => {
  let page: FakePage | null = null;
  let copies = 0;
  afterEach(async () => { await page?.close(); page = null; });

  async function mountTune(state: Record<string, unknown>, engine: (args: string[]) => unknown) {
    page = fakePage({ respond: engine });
    copies += 1;
    const tune = await import(`${join(UI, 'tune.js')}?switches-${copies}`);
    const pane = page.pane();
    const ctx = {
      state: { script: { fountainPath: '/lib/x.fountain', epubPath: null, previewHtml: '' }, reducedMotion: false, ...state } as Record<string, any>,
      goTo: () => {},
      sendBook: () => {},
      convertAgain: () => {},
    };
    tune.mount(pane, ctx);
    await tune.show();
    await settle();
    const box = (key: string) => pane.querySelector(`#${tune.PAGE_SWITCHES.options.find((o: any) => o.key === key).id}`)!;
    return { tune, pane, ctx, box, tauri: page.tauri };
  }
  const settingsAnswer = { ok: true, settings: { ...DEFAULT_FORMAT_OPTIONS }, defaults: { ...DEFAULT_FORMAT_OPTIONS }, appDefaults: { ...DEFAULT_FORMAT_OPTIONS }, preset: null, presets: [] };

  test('two labelled, described checkboxes under the foot, showing what is stored', async () => {
    const { tune, pane, box } = await mountTune({ animatePage: true, typewriterSounds: false }, (a) => (a[0] === 'settings' ? settingsAnswer : HOLD));
    const set = pane.querySelector('.tune-foot fieldset.page-switches')!;
    expect(set.querySelector('legend')!.textContent).toBe('While a script converts');
    for (const option of tune.PAGE_SWITCHES.options) {
      const input = set.querySelector(`#${option.id}`)!;
      expect(input.getAttribute('type')).toBe('checkbox');
      expect(set.querySelector(`label[for="${option.id}"]`)!.textContent).toBe(option.label);
      expect(set.querySelector(`#${input.getAttribute('aria-describedby')}`)!.textContent).toBe(option.line);
    }
    expect(box('animatePage').checked).toBe(true);
    expect(box('typewriterSounds').checked).toBe(false);
    expect(pane.querySelector('.page-switches-reduced')).toBeNull();
  });

  test('flipping one writes it through app-settings; the sounds rest while the page is still', async () => {
    const { ctx, box, tauri } = await mountTune({ animatePage: true, typewriterSounds: false }, (a) => {
      if (a[0] === 'settings') return settingsAnswer;
      if (a.includes('--set')) return { ok: true, ...JSON.parse(a[3]), ...(a[3].includes('animate') ? { typewriterSounds: false } : { animatePage: true }) };
      return HOLD;
    });
    box('typewriterSounds').checked = true;
    box('typewriterSounds').fire('change');
    await settle();
    expect(tauri.calls.some((c) => c.args.join(' ') === 'app-settings --json --set {"typewriterSounds":true}')).toBe(true);
    expect(ctx.state.typewriterSounds).toBe(true);
    box('animatePage').checked = false;
    box('animatePage').fire('change');
    await settle();
    expect(tauri.calls.some((c) => c.args.join(' ') === 'app-settings --json --set {"animatePage":false}')).toBe(true);
    expect(ctx.state.animatePage).toBe(false);
    expect(box('typewriterSounds').disabled).toBe(true);
  });

  test('a refusal puts the box back and says the engine’s sentence', async () => {
    const { pane, ctx, box } = await mountTune({ animatePage: true, typewriterSounds: false }, (a) => {
      if (a[0] === 'settings') return settingsAnswer;
      if (a.includes('--set')) return { ok: false, error: { code: 'bad-settings', message: 'The settings file is locked.' } };
      return HOLD;
    });
    box('animatePage').checked = false;
    box('animatePage').fire('change');
    await settle();
    expect(ctx.state.animatePage).toBe(true);
    expect(box('animatePage').checked).toBe(true);
    expect(pane.querySelector('.page-switches .keep-note')!.textContent).toBe('The settings file is locked.');
  });

  test('with Reduce Motion on, the page says it will not animate whatever the switch says', async () => {
    const { tune, pane } = await mountTune({ animatePage: true, typewriterSounds: false, reducedMotion: true }, (a) => (a[0] === 'settings' ? settingsAnswer : HOLD));
    expect(pane.querySelector('.page-switches-reduced')!.textContent).toBe(tune.PAGE_SWITCHES.reduced);
  });

  test('not known yet, the page asks the engine once and shows what it holds', async () => {
    const { box } = await mountTune({}, (a) => {
      if (a[0] === 'settings') return settingsAnswer;
      if (a[0] === 'app-settings') return { ok: true, animatePage: false, typewriterSounds: false };
      return HOLD;
    });
    await settle();
    expect(box('animatePage').checked).toBe(false);
  });
});
