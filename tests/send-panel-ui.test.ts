// The send panel (desktop/ui/send-panel.js), mounted on the fake page and
// pressed: the right-hand half of Convert's result (spec 2026-10-01). It is
// mounted standalone here, into a node the test owns, with a stand-in for
// the window's device watcher (ctx.devices) that the test plugs readers into.
// The pure rules it draws are tested in send-device.test.ts and
// send-routes-ui.test.ts; this file is what a person sees and presses.
import { afterEach, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import type { ConnectedDevice } from '../src/device/types';
import { preselected, routes } from '../src/export/routes';
import { fakePage, HOLD, settle, type FakePage } from './helpers/fake-dom';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
const EPUB = '/lib/field-station/Field Station.epub';
const FOUNTAIN = '/lib/field-station/Field Station.fountain';
type Dev = { id: string; kind: string; name: string; volume: string | null };
const dev = (d: ConnectedDevice): Dev => ({ id: d.volume ?? 'remarkable', kind: d.kind, name: d.name, volume: d.volume });
const KINDLE: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const KOBO: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const listed = (devices: ConnectedDevice[], extra = {}) => {
  const list = routes({ platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices, ...extra });
  return { ok: true, routes: list, chosen: preselected(list, undefined).id };
};

let page: FakePage | null = null;
let hide: (() => void) | null = null;
let copies = 0;
afterEach(async () => { hide?.(); hide = null; await page?.close(); page = null; });

async function mountPanel(o: {
  readerDevice?: string | null; devices?: ConnectedDevice[] | null; check?: unknown;
  script?: Record<string, unknown>; engine?: (args: string[]) => unknown; platform?: string;
  state?: Record<string, unknown>;
} = {}) {
  // One page at a time: a second mount in one test puts the first away, so
  // its globals go back in the right order.
  hide?.();
  hide = null;
  await page?.close();
  let plugged = o.devices === undefined ? [] : o.devices;
  page = fakePage({
    platform: o.platform ?? 'MacIntel',
    respond: (args) => {
      const own = o.engine?.(args);
      if (own !== undefined) return own;
      if (args[0] === 'app-settings' && !args.includes('--set')) return { ok: true, readerDevice: o.readerDevice ?? null };
      if (args[0] === 'routes') return listed(plugged ?? []);
      if (args[0] === 'export' && args.includes('--check')) return o.check ?? { ok: false };
      if (args[0] === 'kfx-status') return { ok: false };
      return HOLD;
    },
  });
  copies += 1;
  const panel = await import(`${join(UI, 'send-panel.js')}?panel-${copies}`);
  const node = page.pane();
  node.id = 'surface-convert';
  const subs = new Set<(l: unknown) => void>();
  let restored = 0;
  const ctx: any = {
    state: {
      script: { title: 'Field Station', path: '/s/Field Station.pdf', epubPath: EPUB, fountainPath: FOUNTAIN,
        settings: { dialogueSideMarginPct: 27 }, ...o.script },
      devices: [], readerDevice: undefined, sendResult: null, ...o.state,
    },
    restoreFocus: () => { restored += 1; },
    goTo: () => {}, convertAgain: () => {},
    devices: {
      current: () => (plugged === null ? null : plugged.map(dev)),
      subscribe: (fn: (l: unknown) => void) => { subs.add(fn); return () => subs.delete(fn); },
    },
  };
  panel.mountPanel(node, ctx);
  panel.panelShown();
  hide = () => panel.panelHidden();
  await settle();
  const p = page;
  return {
    panel, node, ctx, tauri: p.tauri, page: p, restored: () => restored,
    async plug(next: ConnectedDevice[]) { plugged = next; for (const fn of subs) fn(next.map(dev)); await settle(); },
    cards: () => node.querySelectorAll('.device-card').map((c) => c.querySelector('.device-card-name')!.textContent),
    brass: () => node.querySelector('.calm .btn-brad'),
    status: () => node.querySelector('.send-status')!,
  };
}

describe('the cards', () => {
  test('first time: the question, six cards, and a plugged-in reader’s card marked', async () => {
    const p = await mountPanel({ devices: [KOBO] });
    expect(p.node.querySelector('.panel-question')!.textContent).toBe('What do you read on?');
    expect(p.cards()).toEqual(['Kindle', 'iPhone, iPad or Mac', 'Kobo', 'tolino', 'reMarkable', 'My device isn’t here']);
    const here = p.node.querySelectorAll('.device-card-here').map((c) => c.querySelector('.device-card-name')!.textContent);
    expect(here).toEqual(['Kobo']);
    // Said to a screen reader too, not only drawn.
    expect(p.node.querySelector('.device-card-here')!.getAttribute('aria-label')).toBe('Kobo, plugged in');
    // It asked the engine once, read-only, which reader this is.
    expect(p.tauri.callsTo('app-settings').map((c) => c.args)).toEqual([['app-settings', '--json']]);
  });

  test('choosing a card stores it app-wide and shows the calm screen, and the cards are not asked again', async () => {
    const p = await mountPanel({ engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: 'kobo' } : undefined) });
    p.node.button('Kobo').click();
    await settle();
    expect(p.tauri.callsTo('app-settings').at(-1)!.args).toEqual(['app-settings', '--json', '--set', '{"readerDevice":"kobo"}']);
    expect(p.ctx.state.readerDevice).toBe('kobo');
    expect(p.node.querySelector('.calm')).not.toBeNull();
    expect(p.node.querySelector('.device-cards')).toBeNull();
    // A later result reads it from the window's state, not the engine.
    const again = await mountPanel({ state: { readerDevice: 'kobo' } });
    expect(again.tauri.callsTo('app-settings')).toEqual([]);
    expect(again.node.querySelector('.calm')).not.toBeNull();
  });

  test('a store the engine refuses still answers this session, and says why', async () => {
    const p = await mountPanel({ engine: (a) => (a.includes('--set') ? { ok: false, error: { code: 'bad-settings', message: 'the settings file is read-only' } } : undefined) });
    p.node.button('Kindle').click();
    await settle();
    expect(p.node.querySelector('.calm')).not.toBeNull();
    expect(p.status().textContent).toBe('the settings file is read-only');
    expect(p.status().classList.contains('bad')).toBe(true);
  });

  test('a reader the settings file does not know is asked for again, not guessed', async () => {
    const p = await mountPanel({ readerDevice: 'palmpilot' });
    expect(p.ctx.state.readerDevice).toBe(null);
    expect(p.cards().length).toBe(6);
  });

  test('Different device brings the cards back, and stores nothing until one is chosen', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    p.node.button('Different device').click();
    expect(p.cards().length).toBe(6);
    expect(p.tauri.callsTo('app-settings')).toEqual([]);
  });

  test('My device isn’t here: the placeholder, which stores nothing', async () => {
    const p = await mountPanel();
    p.node.button('My device isn’t here').click();
    const box = p.node.querySelector('.device-missing')!;
    expect(box.querySelector('p')!.textContent).toBe('Screepub does not know it yet. Tell us which one and we will look at adding it. Until then, the EPUB works on most readers.');
    box.button('Tell us on GitHub').click();
    await settle();
    expect(p.page.tauri.opened[0]).toStartWith('https://github.com/ssandweiss/screepub/issues/new?');
    box.button('Back').click();
    expect(p.cards().length).toBe(6);
    expect(p.tauri.calls.some((c) => c.args.includes('--set'))).toBe(false);
  });

  test('a card plugged in while the cards are up is marked without a redraw stealing the keyboard', async () => {
    const p = await mountPanel();
    const kindleCard = p.node.button('Kindle');
    kindleCard.focus();
    await p.plug([KINDLE]);
    expect(p.node.querySelectorAll('.device-card-here').length).toBe(1);
    expect(p.page.doc.activeElement).toBe(p.node.button('Kindle'));
    expect(p.node.button('Kindle')).toBe(kindleCard);
    // And unmarked when it goes.
    await p.plug([]);
    expect(p.node.querySelectorAll('.device-card-here').length).toBe(0);
    expect(kindleCard.getAttribute('aria-label')).toBe(null);
    expect(kindleCard.querySelector('.device-dot')).toBeNull();
  });

  test('no book on disk: the panel says so and offers to convert it again, and asks nothing', async () => {
    const p = await mountPanel({ script: { epubPath: null } });
    expect(p.node.querySelector('.fault')!.textContent).toBe('No book to send');
    expect(p.node.querySelector('.device-cards')).toBeNull();
    expect(p.tauri.callsTo('app-settings')).toEqual([]);
  });
});

describe('the asterisk', () => {
  test('a Kobo card and the Kobo calm screen carry it; pressing shows the note; tell us opens an issue', async () => {
    const cards = await mountPanel();
    const slot = cards.node.button('Kobo').parentNode!;
    expect(slot.querySelector('.asterisk')).not.toBeNull();
    expect(cards.node.button('Kindle').parentNode!.querySelector('.asterisk')).toBeNull();

    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    const star = p.node.querySelector('.calm .asterisk')!;
    const note = p.node.querySelector('.calm .asterisk-note')!;
    expect(note.hidden).toBe(true);
    star.click();
    expect(note.hidden).toBe(false);
    expect(star.getAttribute('aria-expanded')).toBe('true');
    expect(note.textContent).toContain('never tried on a real Kobo yet');
    expect(note.textContent).toEndWith('If it works for you, tell us.');
    note.button('tell us').click();
    await settle();
    expect(p.page.tauri.opened.length).toBe(1);
    star.click();
    expect(note.hidden).toBe(true);
  });

  test('a Kindle on a Mac carries none', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    expect(p.node.querySelector('.calm .asterisk')).toBeNull();
  });

  test('a Kindle on Windows does, in its own words', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, platform: 'Win32' });
    expect(p.node.querySelector('.calm .asterisk-note')!.textContent)
      .toContain('never tried with a real Kindle on Windows yet');
  });
});

/** Every export that builds (not a check). */
const builds = (p: { tauri: FakePage['tauri'] }) =>
  p.tauri.callsTo('export').filter((c) => !c.args.includes('--check'));

describe('the calm screen', () => {
  test('a Kobo not plugged in: Save the EPUB is brass, with its line; Other ways and Different device beside it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    await settle();
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(p.node.querySelector('.calm-line')!.textContent).toBe('Plug your Kobo in over USB and this becomes Copy to Kobo.');
    expect(p.node.querySelectorAll('.calm .btn-brad').length).toBe(1);
    expect(p.node.button('Other ways').className).toContain('btn-outline');
    expect(p.node.button('Different device').className).toContain('btn-quiet');
  });

  test('it asks routes quick then full, and again when what is plugged in changes, never on a timer', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    expect(p.tauri.callsTo('routes').map((c) => c.args)).toEqual([['routes', EPUB, '--json', '--quick'], ['routes', EPUB, '--json']]);
    expect(p.node.querySelector('.device-dot')).toBeNull();
    await p.plug([KOBO]);
    expect(p.tauri.callsTo('routes').length).toBe(4);
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
    expect(p.node.querySelector('.device-dot')).not.toBeNull();
    // Nothing changed since: nothing more is asked.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(p.tauri.callsTo('routes').length).toBe(4);
  });

  test('while the list is on its way it says so, and a list it cannot read is said in its place', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: (a) => (a[0] === 'routes' ? HOLD : undefined) });
    expect(p.node.querySelector('.calm-line')!.textContent).toBe('Looking for every way to send it…');
    expect(p.brass()).toBeNull();
    for (const call of p.tauri.callsTo('routes')) call.answer({ ok: false, error: { code: 'unreadable', message: 'The book is gone.' } });
    await settle();
    expect(p.node.querySelector('.calm .fault-body')!.textContent).toBe('The book is gone.');
  });

  test('Copy to a Kobo: this script’s settings, export, then send, and the line says so', async () => {
    // Re-homed from desktop-ui's "a device send reads this script's settings
    // first, exports with them, then copies what was built".
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const tuned = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27, fontFamily: 'serif' };
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO], script: { settings: null } });
    p.brass()!.click();
    await settle();
    // Not read yet, so read now: an export without them would rebuild the
    // book with the engine's defaults.
    const read = p.tauri.callsTo('settings');
    expect(read.map((c) => c.args)).toEqual([['settings', FOUNTAIN, '--json']]);
    expect(builds(p)).toEqual([]);
    read[0].answer({ ok: true, settings: tuned });
    await settle();
    expect(builds(p).length).toBe(1);
    const args = builds(p)[0].args;
    expect(args.slice(0, 5)).toEqual(['export', EPUB, '--json', '--for', 'epub']);
    expect(JSON.parse(args[args.indexOf('--options-json') + 1])).toEqual(tuned);
    expect(p.status().textContent).toBe('Getting the book ready for KOBOeReader…');
    builds(p)[0].answer({ ok: true, path: EPUB, extension: 'epub', label: 'EPUB' });
    await settle();
    expect(p.status().textContent).toBe('Copying it to KOBOeReader…');
    p.tauri.callsTo('send')[0].answer({ ok: true, destination: '/Volumes/KOBOeReader/Field Station.epub' });
    await settle();
    expect(p.tauri.callsTo('send').map((c) => c.args)).toEqual([['send', EPUB, '--json', '--device', '/Volumes/KOBOeReader']]);
    expect(p.status().textContent).toBe('Field Station is on your KOBOeReader. Eject it before you unplug.');
    expect(p.status().classList.contains('bad')).toBe(false);
  });

  test('a refused export copies nothing and says the engine’s sentence', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO] });
    p.brass()!.click();
    await settle();
    builds(p)[0].answer({ ok: false, error: { code: 'export-failed', message: 'The book could not be read.' } });
    await settle();
    expect(p.tauri.callsTo('send')).toEqual([]);
    expect(p.status().textContent).toBe('The book could not be read.');
    expect(p.status().classList.contains('bad')).toBe(true);
    expect(p.brass()!.disabled).toBe(false);
  });

  test('while a copy runs every button is dead and a plug-in redraws nothing; a new script hears nothing of it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO] });
    const before = p.node.buttons();
    p.brass()!.click();
    await settle();
    expect(before.every((b) => b.disabled)).toBe(true);
    // A list that changes mid-copy does not rebuild fresh, live buttons
    // under it: the MOBI rung rewrites the book, so a second send is a race.
    const asked = p.tauri.callsTo('routes').length;
    await p.plug([KOBO, KINDLE]);
    expect(p.node.buttons()).toEqual(before);
    expect(p.tauri.callsTo('routes').length).toBe(asked);

    p.ctx.state.script = { title: 'Second', fountainPath: '/lib/second/second.fountain', epubPath: '/lib/second/second.epub', settings: {} };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    builds(p).find((c) => c.args[1] === EPUB)!.answer({ ok: true, path: EPUB, extension: 'epub', label: 'EPUB' });
    await settle();
    expect(p.tauri.callsTo('send')).toEqual([]);
    expect(p.status().textContent).toBe('');
    // The new book's own list is asked for once the old copy lets go.
    expect(p.tauri.callsTo('routes').filter((c) => c.args[1] === '/lib/second/second.epub').length).toBe(2);
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
  });

  test('Send to Kindle (not plugged in): the engine opens it, no turn on the book, and the drag line is under it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, engine: (a) => (a[0] === 'route' ? { ok: true, note: 'Opened Amazon.' } : undefined) });
    expect(p.node.querySelectorAll('.calm-line').map((n) => n.textContent)).toEqual([
      'Amazon’s Send to Kindle opens beside the book’s folder. Drag the book into it: Screepub cannot upload it for you.',
      'Or plug the Kindle in over USB and copy it across.']);
    const { holders } = await import(join(UI, 'book-queue.js'));
    p.brass()!.click();
    expect(holders(EPUB)).toEqual([]);
    await settle();
    expect(p.tauri.callsTo('route').map((c) => c.args)).toEqual([['route', 'send-to-kindle', EPUB, '--json']]);
    expect(p.status().textContent).toBe('Opened Amazon.');
    // It worked, so the engine remembers it: the list is asked for again.
    expect(p.tauri.callsTo('routes').length).toBe(3);
  });

  test('a refused route is the engine’s sentence as an alarm, and asks for nothing more', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, engine: (a) => (a[0] === 'route' ? { ok: false, error: { code: 'open-failed', message: 'No browser.' } } : undefined) });
    p.brass()!.click();
    await settle();
    expect(p.status().textContent).toBe('No browser.');
    expect(p.status().classList.contains('bad')).toBe(true);
    expect(p.tauri.callsTo('routes').length).toBe(2);
  });

  test('the keyboard goes back to the brass button after a route, found again after a redraw', async () => {
    // Re-homed from send-routes-ui's "the keyboard goes back to the route's
    // button after a refusal, and after a success".
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    const brass = p.brass()!;
    brass.focus();
    brass.click();
    // A dead button holds nothing: the browser moves the focus off it.
    p.page.doc.body.focus();
    p.tauri.callsTo('route')[0].answer({ ok: false, error: { code: 'open-failed', message: 'No browser.' } });
    await settle();
    expect(p.page.doc.activeElement).toBe(p.brass());

    p.brass()!.click();
    p.page.doc.body.focus();
    p.tauri.callsTo('route')[1].answer({ ok: true, note: 'Opened Amazon.' });
    await settle();
    expect(p.page.doc.activeElement).toBe(p.brass());
    // A Kobo plugged in redraws the calm screen; the keyboard stays on the
    // same route's button, a new node now.
    const old = p.brass();
    await p.plug([KOBO]);
    expect(p.brass()).not.toBe(old);
    expect(p.page.doc.activeElement).toBe(p.brass());
    expect(p.restored()).toBe(0);
  });

  test('a mouse press leaves the keyboard where it is', async () => {
    // Nothing to put back when the keyboard was not in the panel.
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    const other = p.page.doc.createElement('button');
    p.page.doc.body.append(other);
    other.focus();
    p.brass()!.click();
    p.tauri.callsTo('route')[0].answer({ ok: false, error: { code: 'x', message: 'No.' } });
    await settle();
    expect(p.page.doc.activeElement).toBe(other);
    expect(p.restored()).toBe(0);
  });
});
