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
import { kfxSetup } from '../src/export/kfx-setup';
import { FakeNode, fakePage, HOLD, settle, type FakePage } from './helpers/fake-dom';

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
  /** restoreFocus moves the keyboard as main.js's does (the first live
   *  control in the Convert pane), rather than only counting the ask. */
  realFocus?: boolean;
  /** The panel's clock (ctx.timers), for the reMarkable's slow look. */
  timers?: unknown;
  /** The request service's fetch (ctx.fetch): never the real one here. */
  fetch?: unknown;
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
    restoreFocus: () => {
      restored += 1;
      if (o.realFocus) node.querySelectorAll('button').find((b) => !b.disabled)?.focus();
    },
    goTo: () => {}, convertAgain: () => {},
    ...(o.timers === undefined ? {} : { timers: o.timers }),
    fetch: o.fetch ?? (async () => { throw new Error('a test reached the request service without a stand-in'); }),
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
    // And said in words on the card, not only by its brass edge and dot.
    expect(p.node.querySelector('.device-card-here')!.querySelector('.device-here')!.textContent).toBe('Plugged in');
    expect(p.node.button('Kindle').querySelector('.device-here')).toBeNull();
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

  test('a card pressed from the keyboard hands it to the brass button, not the asterisk before it', async () => {
    const p = await mountPanel({ realFocus: true,
      engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: 'kobo' } : undefined) });
    const card = p.node.button('Kobo');
    card.focus();
    card.click();
    await settle();
    expect(p.brass()).not.toBeNull();
    expect(p.page.doc.activeElement).toBe(p.brass());
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

  test('a Kindle plugged in while the cards are back is copied to once Kindle is chosen again (found on a real Kindle, 2026-10-02)', async () => {
    // Kindle chosen with nothing plugged in: the web leads.
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    expect(p.brass()!.textContent).toBe('Send to Kindle web');
    // Different device, the Kindle plugged in while the cards are up (its
    // card is marked), then Kindle again: the list is asked again, and the
    // copy leads, with the Kindle listed as here in Other ways too.
    p.node.button('Different device').click();
    await p.plug([KINDLE]);
    expect(p.node.querySelectorAll('.device-card-here').length).toBe(1);
    p.node.querySelector('.device-card[data-key="kindle"]')!.click();
    await settle();
    expect(p.brass()!.textContent).toStartWith('Copy to Kindle');
    expect(p.node.visibleText).not.toContain('plug the Kindle in');
  });

  test('a card plugged in while the cards are up is marked without a redraw stealing the keyboard', async () => {
    const p = await mountPanel();
    const kindleCard = p.node.button('Kindle');
    kindleCard.focus();
    await p.plug([KINDLE]);
    expect(p.node.querySelectorAll('.device-card-here').length).toBe(1);
    expect(p.page.doc.activeElement).toBe(p.node.querySelector('.device-card[data-key="kindle"]')!);
    expect(p.node.querySelector('.device-card[data-key="kindle"]')!).toBe(kindleCard);
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
  test('a Kobo card and the Kobo calm screen carry it; pressing shows the note and its two answers', async () => {
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
    expect(note.querySelector('.asterisk-said')!.textContent)
      .toBe('Built and tested in code, never tried on a real Kobo yet. If you try it, tell us how it went.');
    expect(note.button('It worked').className).toBe('btn-quiet');
    expect(note.button('It did not work').className).toBe('btn-quiet');
    star.click();
    expect(note.hidden).toBe(true);
  });

  test('a Kindle on a Mac carries none', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    expect(p.node.querySelector('.calm-act .asterisk')).toBeNull();
  });

  test('a Kindle on Windows does, in its own words', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, platform: 'Win32' });
    expect(p.node.querySelector('.calm .asterisk-note')!.textContent)
      .toContain('never tried with a real Kindle on Windows yet');
  });
});

/** A stand-in for the request service: every post kept, each answered by
 *  `reply` (a Response, or a throw for offline), or held when it returns
 *  HOLD until the test lets it go. */
function requestService(reply: (body: any) => Response | typeof HOLD = () => okReply()) {
  const posts: { url: string; body: any }[] = [];
  const held: Array<(r: Response) => void> = [];
  const fetch = async (url: string, init: RequestInit) => {
    const body = JSON.parse(String(init.body));
    posts.push({ url, body });
    const r = reply(body);
    if (r === HOLD) return new Promise<Response>((resolve) => held.push(resolve));
    return r;
  };
  return { fetch, posts, release: async (r: Response = okReply()) => { held.shift()!(r); await settle(); } };
}
const okReply = () => new Response(JSON.stringify({ ok: true }), { status: 200 });
const refusal = (status: number, error: string) => new Response(JSON.stringify({ ok: false, error }), { status });
const offline = (): Response => { throw new TypeError('Load failed'); };
/** The app's version, as the window reads it (notes.js, which a release bump rewrites). */
const VERSION: string = (await import(join(UI, 'notes.js'))).RELEASE.version;

describe('My device isn’t here', () => {
  async function openMissing(o: Parameters<typeof mountPanel>[0] = {}) {
    const p = await mountPanel(o);
    p.node.button('My device isn’t here').click();
    await settle();
    const box = p.node.querySelector('.device-missing')!;
    return {
      ...p, box,
      field: () => box.querySelector('input')!,
      send: () => box.querySelector('[data-key="device-request-send"]')!,
      said: () => box.querySelector('.request-status')!,
    };
  }

  test('one field, labelled, up to 80 characters, its Send, and a line on what is sent', async () => {
    const m = await openMissing();
    const field = m.field();
    expect(field.getAttribute('maxlength')).toBe('80');
    const label = m.box.querySelector('label')!;
    expect(label.textContent).toBe('Which device?');
    expect(label.getAttribute('for')).toBe(field.id);
    expect(m.send().textContent).toBe('Send');
    expect(m.box.querySelector('.request-note')!.textContent)
      .toBe('Sends the device’s name, Screepub’s version and your platform. Nothing else.');
    // Nothing to send yet.
    expect(m.send().disabled).toBe(true);
    field.input('   ');
    expect(m.send().disabled).toBe(true);
    field.input('PocketBook Era');
    expect(m.send().disabled).toBe(false);
    // The EPUB and Back are still there.
    expect(m.box.button('Save the EPUB…')).not.toBeNull();
    m.box.button('Back').click();
    expect(m.cards().length).toBe(6);
  });

  test('Send posts the name, the version and the platform, waits, then thanks and clears the field', async () => {
    const service = requestService(() => HOLD);
    const m = await openMissing({ fetch: service.fetch });
    m.field().input('  PocketBook Era ');
    m.send().click();
    await settle();
    expect(service.posts).toEqual([{
      url: expect.stringMatching(/^https:\/\/screepub-device-requests\..+\.workers\.dev\/request$/),
      body: { kind: 'device', device: 'PocketBook Era', version: VERSION, platform: 'mac' },
    }]);
    // In flight: no second press.
    expect(m.send().disabled).toBe(true);
    m.send().click();
    await settle();
    expect(service.posts.length).toBe(1);
    await service.release();
    expect(m.said().textContent).toBe('Thanks. We count every request.');
    expect(m.said().className).not.toContain('bad');
    expect(m.field().value).toBe('');
    expect(m.send().disabled).toBe(true);
    // Nothing went to the engine or a browser.
    expect(m.tauri.calls.some((c) => c.args.includes('--set'))).toBe(false);
    expect(m.page.tauri.opened).toEqual([]);
  });

  test('offline: the text stays, it says it could not send, and Try again sends it again', async () => {
    let fail = true;
    const service = requestService(() => (fail ? offline() : okReply()));
    const m = await openMissing({ fetch: service.fetch });
    m.field().input('Boox Note');
    m.send().click();
    await settle();
    expect(m.field().value).toBe('Boox Note');
    expect(m.said().textContent).toBe('Could not send. Check that this computer is online, then try again.');
    expect(m.said().className).toContain('bad');
    expect(m.send().textContent).toBe('Try again');
    expect(m.send().disabled).toBe(false);
    fail = false;
    m.send().click();
    await settle();
    expect(service.posts.map((p) => p.body.device)).toEqual(['Boox Note', 'Boox Note']);
    expect(m.said().textContent).toBe('Thanks. We count every request.');
    expect(m.send().textContent).toBe('Send');
  });

  test('too many, or any refusal: the text stays and it says why', async () => {
    const service = requestService(() => refusal(429, 'Too many requests from here this hour. Try again later.'));
    const m = await openMissing({ fetch: service.fetch });
    m.field().input('Boox Note');
    m.send().click();
    await settle();
    expect(m.field().value).toBe('Boox Note');
    expect(m.said().textContent).toBe('Could not send: too many requests from this computer this hour. Try again later.');
    expect(m.send().textContent).toBe('Try again');
  });

  test('what was typed outlives going Back and coming again; the last line does not', async () => {
    const m = await openMissing({ fetch: requestService(() => offline()).fetch });
    m.field().input('Boox Note');
    m.send().click();
    await settle();
    m.box.button('Back').click();
    m.node.button('My device isn’t here').click();
    const again = m.node.querySelector('.device-missing')!;
    expect(again.querySelector('input')!.value).toBe('Boox Note');
    expect(again.querySelector('.request-status')!.textContent).toBe('');
  });
});

describe('It worked / It did not work', () => {
  const answers = (note: FakeNode) => ({
    worked: () => note.querySelector('[data-key="worked-kobo"]'),
    not: () => note.querySelector('[data-key="not-worked-kobo"]'),
    said: () => note.querySelector('.report-status')!,
  });

  test('a press posts the report and thanks; the answers go', async () => {
    const service = requestService();
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, fetch: service.fetch });
    const note = p.node.querySelector('.calm .asterisk-note')!;
    const a = answers(note);
    a.not()!.click();
    await settle();
    expect(service.posts.map((x) => x.body)).toEqual([
      { kind: 'report', route: 'kobo', worked: false, version: VERSION, platform: 'mac' },
    ]);
    expect(a.said().textContent).toBe('Thanks. We count every request.');
    expect(a.worked()).toBeNull();
    expect(a.not()).toBeNull();
    expect(p.page.tauri.opened).toEqual([]);
  });

  test('a failure says so, and Try again sends the same answer', async () => {
    let fail = true;
    const service = requestService(() => (fail ? offline() : okReply()));
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, fetch: service.fetch });
    const note = p.node.querySelector('.calm .asterisk-note')!;
    const a = answers(note);
    a.worked()!.click();
    await settle();
    expect(a.said().textContent).toBe('Could not send. Check that this computer is online, then try again.');
    expect(a.said().className).toContain('bad');
    fail = false;
    note.button('Try again').click();
    await settle();
    expect(service.posts.map((x) => x.body.worked)).toEqual([true, true]);
    expect(a.said().textContent).toBe('Thanks. We count every request.');
    expect(note.querySelector('[data-key="report-again-kobo"]')).toBeNull();
  });

  test('held: neither answer can be pressed twice', async () => {
    const service = requestService(() => HOLD);
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, fetch: service.fetch });
    const a = answers(p.node.querySelector('.calm .asterisk-note')!);
    a.worked()!.click();
    await settle();
    expect(a.worked()!.disabled).toBe(true);
    expect(a.not()!.disabled).toBe(true);
    a.not()!.click();
    await settle();
    expect(service.posts.length).toBe(1);
    await service.release();
  });

  test('a Kindle on Linux reports as kindle-linux', async () => {
    const service = requestService();
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, platform: 'Linux x86_64', fetch: service.fetch });
    p.node.querySelector('.calm .asterisk-note')!.querySelector('[data-key="worked-kindle"]')!.click();
    await settle();
    expect(service.posts[0].body).toEqual({ kind: 'report', route: 'kindle-linux', worked: true, version: VERSION, platform: 'linux' });
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
    expect(p.node.querySelector('.calm-line')!.textContent).toBe('Plug your Kobo in over USB and this becomes a Copy button.');
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
    expect(p.node.querySelector('.calm .device-here')!.textContent).toBe('Plugged in');
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
    // It worked, so the engine remembers it: the list is asked for again.
    expect(p.tauri.callsTo('routes').length).toBe(3);
  });

  test('a list that lands while a send runs is dropped, and asked for again once the send is over', async () => {
    let hold = false;
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO],
      engine: (a) => (hold && a[0] === 'routes' ? HOLD : undefined) });
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
    hold = true;
    await p.plug([]);
    const out = p.tauri.callsTo('routes').slice(-2);
    expect(out.length).toBe(2);
    // The send starts while both asks are out, then they land: dropped.
    p.brass()!.click();
    await settle();
    for (const call of out) call.answer(listed([]));
    await settle();
    const before = p.tauri.callsTo('routes').length;
    hold = false;
    // The send fails, so nothing else would ask: the dropped list is owed.
    builds(p)[0].answer({ ok: false, error: { code: 'export-failed', message: 'The book could not be read.' } });
    await settle();
    expect(p.tauri.callsTo('routes').length).toBe(before + 2);
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
  });

  test('settings read for one script land on that script, not on one opened while the read was out', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const tuned = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27, fontFamily: 'serif' };
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO], script: { settings: null } });
    const asked = p.ctx.state.script;
    p.brass()!.click();
    await settle();
    const read = p.tauri.callsTo('settings');
    expect(read.length).toBe(1);
    const other = { ...asked, title: 'Another', fountainPath: '/s/Another.fountain', settings: null };
    p.ctx.state.script = other;
    read[0].answer({ ok: true, settings: tuned });
    await settle();
    expect(asked.settings).toEqual(tuned);
    expect(other.settings).toBeNull();
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

type Mounted = Awaited<ReturnType<typeof mountPanel>>;
const isCheck = (args: string[]) => args[0] === 'export' && args.includes('--check');
/** Answer the oldest held call to `verb` (an export that builds, never a
 *  check), and hand back what it was asked. */
async function answer(p: Mounted, verb: string, value: unknown) {
  const call = p.tauri.pending().find((c) => c.args[0] === verb && !isCheck(c.args));
  if (call === undefined) throw new Error(`nothing asked the engine for ${verb}: ${p.tauri.pending().map((c) => c.args[0])}`);
  call.answer(value);
  await settle();
  return call.args;
}
async function answerNewest(p: Mounted, verb: string, value: unknown) {
  const call = p.tauri.pending().filter((c) => c.args[0] === verb && !isCheck(c.args)).at(-1);
  if (call === undefined) throw new Error(`nothing asked the engine for ${verb}`);
  call.answer(value);
  await settle();
}
/** Answer the oldest held Kindle file check. */
async function answerCheck(p: Mounted, value: unknown) {
  const call = p.tauri.pending().find((c) => isCheck(c.args));
  if (call === undefined) throw new Error(`nothing asked for a Kindle file check: ${p.tauri.pending().map((c) => c.args[0])}`);
  call.answer(value);
  await settle();
  return call.args;
}
/** Every held export that builds (not a check). */
const building = (p: Mounted) => p.tauri.pending().filter((c) => c.args[0] === 'export' && !isCheck(c.args)).length;
/** The build line while the Kindle file builds, or null when its block is
 *  hidden or off screen (the bar with it). */
function buildLine(p: Mounted) {
  const block = p.node.querySelector('.kindle-build');
  if (block === null || block.hidden) return null;
  expect(block.querySelector('.build-sweep')).not.toBeNull();
  return block.querySelector('.kindle-build-line')!.textContent;
}
/** `export --check`'s answer for a machine that makes `extension`. */
const checked = (extension: string, fresh: boolean, bookDate = 1000) => ({
  ok: true, path: `/lib/field-station/Field Station.${extension}`, format: 'kindle', extension,
  label: extension.toUpperCase(), available: ['epub', 'kindle'], stages: [], fresh,
  builtBy: extension === 'mobi' ? 'screepub' : 'calibre', bookDate,
});
const HALF_A_MINUTE = 'Building the Kindle file. This takes about half a minute.';
const kindleBuild = ['export', EPUB, '--json', '--for', 'kindle', '--fountain', FOUNTAIN,
  '--options-json', JSON.stringify({ dialogueSideMarginPct: 27 })];
const builtKfx = { ok: true, path: '/lib/field-station/Field Station.kfx', extension: 'kfx', label: 'KFX' };
const quit = { ok: false, error: { code: 'export-failed', message: 'Kindle Previewer quit.' } };
const sentKfx = { ok: true, destination: '/Volumes/Kindle/documents/Field Station.kfx' };

describe('a reMarkable docked after the result appears', () => {
  // The background poll is `devices --quick`, which never lists a
  // reMarkable, so the calm screen for one looks again on its own, slowly.
  const RM: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };
  function clock() {
    let next = 0;
    const live = new Map<number, { fn: () => void; ms: number }>();
    return {
      setInterval: (fn: () => void, ms: number) => { next += 1; live.set(next, { fn, ms }); return next; },
      clearInterval: (id: number) => { live.delete(id); },
      setTimeout: globalThis.setTimeout.bind(globalThis),
      clearTimeout: globalThis.clearTimeout.bind(globalThis),
      live: () => [...live.values()],
      async tick() { for (const t of [...live.values()]) t.fn(); await settle(); },
    };
  }
  const full = (p: Mounted) => p.tauri.callsTo('routes').filter((c) => !c.args.includes('--quick'));

  test('the calm screen asks the full list every ten seconds, and offers the tablet once it is docked', async () => {
    let docked = false;
    const timers = clock();
    const p = await mountPanel({ state: { readerDevice: 'remarkable' }, timers,
      engine: (a) => (a[0] === 'routes' ? listed(docked ? [RM] : []) : undefined) });
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(timers.live().map((t) => t.ms)).toEqual([10_000]);
    const asked = full(p).length;
    docked = true;
    await timers.tick();
    expect(full(p).length).toBe(asked + 1);
    expect(p.tauri.callsTo('routes').length).toBe(asked * 2 + 1);
    expect(p.brass()!.textContent).toBe('Upload to reMarkable');
  });

  test('the window getting the focus back asks too', async () => {
    let docked = false;
    const p = await mountPanel({ state: { readerDevice: 'remarkable' }, timers: clock(),
      engine: (a) => (a[0] === 'routes' ? listed(docked ? [RM] : []) : undefined) });
    docked = true;
    p.page.fireWindow('focus');
    await settle();
    expect(p.brass()!.textContent).toBe('Upload to reMarkable');
  });

  test('it stops when the panel is hidden, and never runs for another reader or on the cards', async () => {
    const timers = clock();
    const p = await mountPanel({ state: { readerDevice: 'remarkable' }, timers });
    p.panel.panelHidden();
    expect(timers.live()).toEqual([]);
    p.panel.panelShown();
    await settle();
    expect(timers.live().length).toBe(1);
    p.node.button('Different device').click();
    expect(timers.live()).toEqual([]);
    const kobo = clock();
    await mountPanel({ state: { readerDevice: 'kobo' }, timers: kobo });
    expect(kobo.live()).toEqual([]);
  });

  test('a send running asks nothing on the tick', async () => {
    const timers = clock();
    const p = await mountPanel({ state: { readerDevice: 'remarkable' }, devices: [RM], timers,
      engine: (a) => (a[0] === 'routes' ? listed([RM]) : undefined) });
    p.brass()!.click();
    await settle();
    const asked = p.tauri.callsTo('routes').length;
    await timers.tick();
    expect(p.tauri.callsTo('routes').length).toBe(asked);
  });
});

describe('the Kindle file, named on its button and built ahead', () => {
  test('the button that hands over a Kindle file says which file, as the engine’s check named it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    expect(p.brass()!.textContent).toBe('Copy to Kindle');
    expect(await answerCheck(p, checked('kfx', true))).toEqual(['export', EPUB, '--json', '--for', 'kindle', '--check']);
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.kfx)');
    // A Kobo's button hands over the EPUB, and is not renamed.
    const kobo = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO], check: checked('kfx', true) });
    expect(kobo.brass()!.textContent).toBe('Copy to KOBOeReader');
  });

  test('a check that lands after the button renames it, and the keyboard stays on it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    p.brass()!.focus();
    await answerCheck(p, checked('azw3', true));
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.azw3)');
    expect(p.page.doc.activeElement).toBe(p.brass());
    expect(p.restored()).toBe(0);
  });

  test('a check that fails names nothing: the button keeps its plain words', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE],
      check: { ok: false, error: { code: 'unreadable', message: 'Cannot read the book.' } } });
    expect(p.brass()!.textContent).toBe('Copy to Kindle');
    expect(p.status().textContent).toBe('');
  });

  test('the check is asked again when the window gets the focus back: a toolchain installed meanwhile renames the button', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    await answerCheck(p, checked('azw3', true));
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.azw3)');
    p.page.fireWindow('focus'); // back from installing Kindle Previewer
    await answerCheck(p, checked('kfx', true));
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.kfx)');
    p.panel.panelHidden();
    p.page.fireWindow('focus');
    await settle();
    expect(p.tauri.pending()).toEqual([]);
  });

  test('the result appearing with the Kindle file current builds nothing', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', true) });
    expect(building(p)).toBe(0);
    expect(buildLine(p)).toBe(null);
  });

  test('a Kindle chosen and the file out of date: one build starts as the result appears, with its line and bar under the brass', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE],
      check: { ok: true, path: '/x.kfx', format: 'kindle', extension: 'kfx', label: 'KFX', available: ['epub', 'kindle'], stages: [], fresh: false, builtBy: 'calibre', bookDate: 1000 } });
    await settle();
    const exports = p.tauri.callsTo('export').filter((c) => !c.args.includes('--check'));
    expect(exports.map((c) => c.args)).toEqual([['export', EPUB, '--json', '--for', 'kindle', '--fountain', FOUNTAIN, '--options-json', '{"dialogueSideMarginPct":27}']]);
    expect(p.node.querySelector('.calm .kindle-build-line')!.textContent).toBe('Building the Kindle file. This takes about half a minute.');
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.kfx)');
  });

  test('the build ahead says nothing when it is done, and a press after it may build and says so', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    expect(building(p)).toBe(0);
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    // Every button still works while it builds: nothing was pressed.
    expect(p.node.buttons().every((b) => !b.disabled)).toBe(true);
    expect(await answer(p, 'export', builtKfx)).toEqual(kindleBuild);
    expect(buildLine(p)).toBe(null);
    // Nobody asked for it, so a build that worked says nothing more. The
    // engine is asked again whether the file is current (a save may have
    // waited behind the build and changed the book), and that check never
    // starts a build of its own.
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
    expect(p.status().textContent).toBe('');
    // So a press now may build, and says so.
    p.brass()!.click();
    await settle();
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    await answer(p, 'send', sentKfx);
  });

  test('its turn comes after a save still waiting on the book, so it builds what the reader last set', async () => {
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    let free: () => void = () => {};
    const saving = inTurn(EPUB, 'save', () => new Promise<void>((resolve) => { free = resolve; }));
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    // The check itself waits its turn behind the save.
    expect(p.tauri.callsTo('export')).toEqual([]);
    free();
    await saving;
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1);
    await answer(p, 'export', builtKfx);
    await answerCheck(p, checked('kfx', true));
  });

  test('with no Calibre to build it (the engine’s MOBI), nothing starts before a press', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('mobi', false) });
    expect(building(p)).toBe(0);
    expect(buildLine(p)).toBe(null);
  });

  test('a build already running is not started again when the panel shows again; the book changing since starts the next', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1);
    p.panel.panelHidden();
    p.panel.panelShown();
    // Back on the result: no second build, and no check either (until the
    // build lands it could only say out of date).
    expect(p.tauri.pending().map((c) => c.args[0])).toEqual(['export']);
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    expect(buildLine(p)).toBe(null);
    await answerCheck(p, checked('kfx', true));
    expect(building(p)).toBe(0);

    // A setting changed on the Settings page, which rebuilt the book: a new
    // date, and the engine's check says the Kindle file is out of date.
    p.panel.panelHidden();
    p.panel.panelShown();
    await answerCheck(p, checked('kfx', false, 2000));
    expect(building(p)).toBe(1);
    await answer(p, 'export', builtKfx);
    await answerCheck(p, checked('kfx', true, 2000));
  });

  test('a build that failed is not tried again by focus or a return to the result; the next version of the book is', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1);
    await answer(p, 'export', quit);
    // Focus comes back after every Save box and every switch of app. Each
    // return asks the check again (it names the file), and none builds.
    for (let i = 0; i < 3; i += 1) {
      p.page.fireWindow('focus');
      await answerCheck(p, checked('kfx', false));
      expect(building(p)).toBe(0);
    }
    // Nor does leaving the result and coming back to the same book.
    p.panel.panelHidden();
    p.panel.panelShown();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
    expect(buildLine(p)).toBe(null);
    // A Settings save rewrote the book: a new version, built once more.
    p.panel.panelHidden();
    p.panel.panelShown();
    await answerCheck(p, checked('kfx', false, 2000));
    expect(building(p)).toBe(1);
    await answer(p, 'export', builtKfx);
    await answerCheck(p, checked('kfx', true, 2000));
  });

  test('a check refused as busy (a copy running) is looked at again when the copy ends', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO, KINDLE], check: HOLD });
    p.brass()!.click();
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1); // the Kobo's own export, for the EPUB
    await answer(p, 'export', { ok: false, error: { code: 'export-failed', message: 'No book.' } });
    expect(building(p)).toBe(1); // now the Kindle file, built ahead
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    await answerCheck(p, checked('kfx', true));
  });

  test('a check that answers after the panel is hidden starts nothing', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    p.panel.panelHidden();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
  });

  test('a Kindle plugged in after the panel is hidden starts nothing either', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('kfx', false) });
    p.panel.panelHidden();
    await p.plug([KINDLE]);
    expect(building(p)).toBe(0);
  });

  test('a Kobo reader with no Kindle in sight never starts Kindle Previewer; a Kindle plugged in does', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO], check: checked('kfx', false) });
    expect(building(p)).toBe(0);
    expect(buildLine(p)).toBe(null);
    await p.plug([KOBO, KINDLE]);
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
  });

  test('a Kindle plugged in while the cards are still up starts the build too', async () => {
    const p = await mountPanel({ check: checked('kfx', false) });
    expect(p.cards().length).toBe(6);
    expect(building(p)).toBe(0);
    await p.plug([KINDLE]);
    expect(building(p)).toBe(1);
    await answer(p, 'export', builtKfx);
  });

  test('back on the result, a press before the check answers puts the bar up, even after a press that found the file current', async () => {
    // The review's probe: a send found the file current; the book was then
    // rebuilt on the Settings page; back on the result, the press came
    // before the check could say so, and a whole KFX build ran with no line.
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    await answerCheck(p, checked('kfx', true));
    p.brass()!.click();
    await settle();
    expect(buildLine(p)).toBe(null); // current: a lookup
    expect(p.status().textContent).toBe('Getting the book ready for Kindle…');
    await answer(p, 'export', builtKfx);
    await answer(p, 'send', sentKfx);
    p.panel.panelHidden();
    p.panel.panelShown();
    // Eject holds the brass place now; the copy is pressed again from Other
    // ways, and its button still names the file while the check is out.
    expect(p.brass()!.textContent).toBe('Eject Kindle');
    const again = openWays(p).button('Copy to Kindle (.kfx)');
    again.click();
    await settle();
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    expect(p.status().textContent).toBe('');
    // The check lands mid-press: busy, so it starts nothing of its own.
    await answerCheck(p, checked('kfx', false, 2000));
    expect(building(p)).toBe(1);
    await answer(p, 'export', builtKfx);
    expect(buildLine(p)).toBe(null);
    await answer(p, 'send', sentKfx);
  });

  test('Copy to Kindle pressed mid-build waits for it, then copies what it made', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    await answerCheck(p, checked('kfx', false));
    p.brass()!.click();
    await settle();
    // No second build: the press waits behind the one running.
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    expect(p.node.buttons().every((b) => b.disabled)).toBe(true);
    await answer(p, 'export', builtKfx);
    // Its own export now runs, and finds the file that build just made (the
    // engine's freshness rule): the line stays up until that answers.
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    expect(await answer(p, 'export', builtKfx)).toEqual(kindleBuild);
    expect(buildLine(p)).toBe(null);
    expect(p.status().textContent).toBe('Copying it to Kindle…');
    expect(await answer(p, 'send', sentKfx))
      .toEqual(['send', '/lib/field-station/Field Station.kfx', '--json', '--device', '/Volumes/Kindle']);
    expect(p.status().textContent).toBe('Field Station is on your Kindle. Eject it before you unplug.');
  });

  test('a copy to a Kobo during a background build starts at once: both only read the book', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO, KINDLE], check: checked('kfx', false) });
    expect(building(p)).toBe(1);
    p.brass()!.click();
    await settle();
    expect(p.status().textContent).toBe('Getting the book ready for KOBOeReader…');
    // Two exports out at once: the build, and this one for the EPUB.
    expect(building(p)).toBe(2);
    expect(p.tauri.pending().filter((c) => c.args[0] === 'export').map((c) => c.args.slice(3, 5).join(' ')))
      .toEqual(['--for kindle', '--for epub']);
    await answerNewest(p, 'export', { ok: true, path: EPUB, extension: 'epub', label: 'EPUB' });
    await answer(p, 'send', { ok: true, destination: '/Volumes/KOBOeReader/Field Station.epub' });
    expect(p.status().textContent).toBe('Field Station is on your KOBOeReader. Eject it before you unplug.');
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
  });

  test('a Settings save during a background build waits for it: the build reads the book, the save rewrites it', async () => {
    const { inTurn, holders } = await import(join(UI, 'book-queue.js'));
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', false) });
    expect(holders(EPUB)).toEqual(['kindle']);
    let saved = false;
    const saving = inTurn(EPUB, 'save', () => { saved = true; });
    await settle();
    expect(saved).toBe(false);
    await answer(p, 'export', builtKfx);
    await saving;
    expect(saved).toBe(true);
  });

  test('a build that fails says nothing (nobody asked), and the press builds again with the line up', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: checked('kfx', false) });
    await answer(p, 'export', quit);
    expect(buildLine(p)).toBe(null);
    expect(p.status().textContent).toBe('');
    p.brass()!.click();
    await settle();
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', quit);
    expect(p.status().textContent).toBe('Kindle Previewer quit.');
    expect(buildLine(p)).toBe(null);
  });

  test('a save that waited behind the build and changed the book: the check after it says so, and a press puts the line up', async () => {
    // The gap the check after a build closes: without it, the panel would
    // take the build's file as current, and a press after the save would
    // rebuild it for half a minute with nothing on screen.
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD });
    await answerCheck(p, checked('kfx', false));
    let free: () => void = () => {};
    const saving = inTurn(EPUB, 'save', () => new Promise<void>((resolve) => { free = resolve; }));
    await answer(p, 'export', builtKfx);
    // The check waits for the save.
    expect(p.tauri.pending()).toEqual([]);
    free();
    await saving;
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
    p.brass()!.click();
    await settle();
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    expect(buildLine(p)).toBe(null);
    await answer(p, 'send', sentKfx);
  });
});

/** The Save box the panel opened, answered: a path, or null for a cancel. */
async function chooseSave(p: Mounted, path: string | null) {
  const box = p.tauri.dialogs.shift();
  if (box === undefined) throw new Error('no Save box is open');
  box.resolve(path);
  await settle();
  return box.options;
}
/** Other ways, opened. */
function openWays(p: Mounted) {
  p.node.button('Other ways').click();
  return p.node.querySelector('section.other-ways')!;
}
/** A line's words without its asterisks (each carries its own note). */
const words = (node: FakeNode): string => node.childNodes.map((k) => {
  if (k.nodeType === 3) return k.textContent;
  return k.classList.contains('asterisk-wrap') ? '' : words(k);
}).join('');
const saveRow = (p: Mounted, type: string) => p.node.querySelector(`.save-row[data-type="${type}"]`)!;
const saveButton = (p: Mounted, type: string) => saveRow(p, type).querySelector('button')!;
const notReady = { ok: true, ...kfxSetup({ calibre: true, previewer: true, pluginInstalled: false, ready: false }, 'darwin') };
const installed = {
  ok: true, version: '2.20.1', removed: [],
  setup: kfxSetup({ calibre: true, previewer: true, pluginInstalled: true, ready: true }, 'darwin'),
};
const kfxProbe = (said: unknown) => (a: string[]) => (a[0] === 'kfx-status' ? said : undefined);
const installButton = (p: Mounted) => p.node.querySelectorAll('button').find((b) => b.dataset.step === 'plugin')!;

describe('Other ways', () => {
  test('opens in place, in two groups: the device’s follow-ups lead Send it, every file type is in Save a file', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE] });
    const toggle = p.node.button('Other ways');
    expect(p.node.querySelector('section.other-ways')!.hidden).toBe(true);
    toggle.click();
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(p.node.querySelector('section.other-ways')!.hidden).toBe(false);
    const groups = p.node.querySelectorAll('.ways-group');
    expect(groups.map((g) => g.querySelector('.ways-heading')!.textContent)).toEqual(['Send it', 'Save a file']);
    const sendTitles = groups[0].querySelectorAll('.device-name').map((n) => n.textContent);
    expect(sendTitles.slice(0, 2)).toEqual(['Send to Kindle web', 'Send to Kindle email']);
    expect(sendTitles).not.toContain('Save the EPUB');
    expect(sendTitles).not.toContain('Save a Kindle file');
    // The brass one is not listed again, and nothing in the list is brass.
    expect(sendTitles).not.toContain('Kindle');
    expect(groups[0].querySelectorAll('.btn-brad')).toEqual([]);
    expect(groups[1].querySelectorAll('.save-row .device-name').map((n) => n.textContent))
      .toEqual(['EPUB', 'KFX', 'AZW3', 'MOBI', 'Fountain']);
    expect(saveRow(p, 'azw3').querySelector('.route-detail')!.textContent).toBe('Older Kindles over USB.');
    // Open across a redraw, and closed again by the same button.
    await p.plug([KINDLE, KOBO]);
    expect(p.node.querySelector('section.other-ways')!.hidden).toBe(false);
    p.node.button('Other ways').click();
    expect(p.node.querySelector('section.other-ways')!.hidden).toBe(true);
    expect(p.node.button('Other ways').getAttribute('aria-expanded')).toBe('false');
  });

  test('a Send it row runs its route, outlined, through the same flow as the brass', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' },
      engine: (a) => (a[0] === 'route' ? { ok: true, note: 'Added to Apple Books.' } : undefined) });
    const ways = openWays(p);
    const books = ways.button('Add to Apple Books');
    expect(books.className).toBe('btn btn-outline');
    books.click();
    await settle();
    expect(p.tauri.callsTo('route').map((c) => c.args)).toEqual([['route', 'apple-books', EPUB, '--json']]);
    expect(p.status().textContent).toBe('Added to Apple Books.');
  });

  test('a dimmed route keeps its fix and has no button; a tolino row carries the asterisk', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, platform: 'Win32',
      engine: (a) => (a[0] === 'routes' ? (() => {
        const list = routes({ platform: 'win32', booksApp: false, sendToKindleApp: false, appleMailDefault: false, devices: [] });
        return { ok: true, routes: list, chosen: preselected(list, undefined).id };
      })() : undefined) });
    const ways = openWays(p);
    const rows = ways.querySelectorAll('.device-row');
    const tolino = rows.find((r) => r.querySelector('.device-name')!.textContent === 'tolino')!;
    expect(tolino.className).toContain('route-unavailable');
    expect(tolino.querySelector('.route-detail')!.textContent).toContain('cannot be found on Windows');
    const star = tolino.querySelector('.asterisk-wrap')!;
    expect(tolino.querySelectorAll('button').filter((b) => !star.contains(b))).toEqual([]);
  });

  test('readers that only need plugging in or docking share one muted line, each untried one with its own asterisk', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    const ways = openWays(p);
    const group = ways.querySelectorAll('.ways-group')[0];
    const lines = group.querySelectorAll('.ways-folded');
    expect(lines.length).toBe(1);
    expect(words(lines[0])).toBe('Kindle, Kobo, tolino and reMarkable: plug in or dock over USB to send.');
    // A Kindle on a Mac is proven; the other three each keep their note and answers.
    expect(lines[0].querySelectorAll('.asterisk').map((s) => s.dataset.key))
      .toEqual(['asterisk-kobo', 'asterisk-tolino', 'asterisk-remarkable']);
    expect(lines[0].querySelector('.asterisk-note')!.textContent).toContain('never tried on a real Kobo yet');
    expect(lines[0].querySelector('[data-key="worked-tolino"]')).not.toBeNull();
    // None of them is a row of its own any more.
    const names = group.querySelectorAll('.device-row .device-name').map((n) => n.textContent);
    for (const name of ['Kindle', 'Kobo', 'tolino', 'reMarkable']) expect(names).not.toContain(name);
    // The email row keeps its first-time step: rows dimmed for other reasons stay rows.
    const email = group.querySelectorAll('.device-row').find((r) => r.querySelector('.device-name')!.textContent === 'Send to Kindle email')!;
    expect(email.querySelector('.route-hint')!.textContent).toContain('First time?');
    expect(email.button('Open Amazon’s page').className).toBe('btn-quiet');
  });

  test('a reader plugged in leaves the line for a row of its own, with its button', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    openWays(p);
    await p.plug([KOBO]);
    const group = p.node.querySelectorAll('.ways-group')[0];
    expect(words(group.querySelector('.ways-folded')!)).toBe('Kindle, tolino and reMarkable: plug in or dock over USB to send.');
    const kobo = group.querySelectorAll('.device-row').find((r) => r.querySelector('.device-name')!.textContent === 'KOBOeReader')!;
    expect(kobo.className).toBe('device-row');
    expect(kobo.button('Copy to KOBOeReader').disabled).toBe(false);
  });

  test('Save KFX: the Save box first, named for the book, then export --for kfx --out, in the Kindle line, with the build line', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    expect(builds(p)).toEqual([]);
    const options = await chooseSave(p, '/Users/ann/Desktop/Field Station.kfx');
    expect(options).toEqual({ defaultPath: 'Field Station.kfx', filters: [{ name: 'KFX', extensions: ['kfx'] }] });
    expect(builds(p).map((c) => c.args)).toEqual([[
      'export', EPUB, '--json', '--for', 'kfx', '--fountain', FOUNTAIN, '--options-json', '{"dialogueSideMarginPct":27}',
      '--out', '/Users/ann/Desktop/Field Station.kfx']]);
    expect(p.node.querySelector('.kindle-build-line')!.textContent).toBe(HALF_A_MINUTE);
    expect(p.node.buttons().filter((b) => !b.disabled)).toEqual([]);
    await answer(p, 'export', { ok: true, path: '/Users/ann/Desktop/Field Station.kfx', extension: 'kfx', label: 'KFX' });
    expect(buildLine(p)).toBe(null);
    expect(p.status().textContent).toBe('Saved Field Station.kfx.');
  });

  test('a cancelled Save box asks the engine nothing and says nothing', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    openWays(p);
    saveButton(p, 'fountain').click();
    await settle();
    expect(await chooseSave(p, null)).toEqual({ defaultPath: 'Field Station.fountain', filters: [{ name: 'Fountain', extensions: ['fountain'] }] });
    expect(builds(p)).toEqual([]);
    expect(p.status().textContent).toBe('');
    expect(p.node.buttons().every((b) => !b.disabled)).toBe(true);
  });

  test('a saved file is named on the status line; a refusal is the engine’s sentence', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    openWays(p);
    saveButton(p, 'epub').click();
    await settle();
    await chooseSave(p, '/Users/ann/Desktop/Field Station.epub');
    expect(p.status().textContent).toBe('Saving…');
    expect(await answer(p, 'export', { ok: true, path: '/Users/ann/Desktop/Field Station.epub', extension: 'epub', label: 'EPUB' }))
      .toEqual(['export', EPUB, '--json', '--for', 'epub', '--fountain', FOUNTAIN, '--options-json', '{"dialogueSideMarginPct":27}',
        '--out', '/Users/ann/Desktop/Field Station.epub']);
    expect(p.status().textContent).toBe('Saved Field Station.epub.');
    expect(p.status().classList.contains('bad')).toBe(false);

    saveButton(p, 'mobi').click();
    await settle();
    await chooseSave(p, '/Users/ann/Desktop/Field Station.mobi');
    await answer(p, 'export', { ok: false, error: { code: 'export-failed', message: 'The script could not be read.' } });
    expect(p.status().textContent).toBe('The script could not be read.');
    expect(p.status().classList.contains('bad')).toBe(true);
  });

  test('a script this window has no settings for is asked for them before the save builds', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const tuned = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27, fontFamily: 'serif' };
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, script: { settings: null },
      engine: (a) => (a[0] === 'settings' ? { ok: true, settings: tuned } : undefined) });
    openWays(p);
    saveButton(p, 'azw3').click();
    await settle();
    await chooseSave(p, '/Users/ann/Desktop/Field Station.azw3');
    const args = builds(p)[0].args;
    expect(args.slice(0, 5)).toEqual(['export', EPUB, '--json', '--for', 'azw3']);
    expect(JSON.parse(args[args.indexOf('--options-json') + 1])).toEqual(tuned);
    expect(buildLine(p)).toBe('Building the Kindle file. This takes a moment.');
    await answer(p, 'export', { ok: true, path: '/Users/ann/Desktop/Field Station.azw3', extension: 'azw3', label: 'AZW3' });
  });

  test('MOBI and Fountain are greyed for a book made without a .fountain', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, script: { fountainPath: null } });
    openWays(p);
    for (const type of ['mobi', 'fountain']) {
      expect(saveRow(p, type).className).toContain('route-unavailable');
      expect(saveRow(p, type).querySelector('button')).toBeNull();
      expect(saveRow(p, type).querySelector('.route-fix')!.textContent)
        .toBe('This book was made without a .fountain, so there is none to work from.');
    }
  });

  test('KFX greyed with Set it up when the ladder stops at AZW3; Set it up shows the checklist in place', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('azw3', true), engine: kfxProbe(notReady) });
    openWays(p);
    const row = saveRow(p, 'kfx');
    expect(row.className).toContain('route-unavailable');
    expect(row.querySelector('.route-fix')!.textContent).toBe('This computer cannot make one yet.');
    expect(row.buttons().map((b) => b.textContent)).toEqual(['Set it up']);
    expect(row.button('Set it up').className).toBe('btn-quiet');
    // AZW3 stays live: Calibre is there.
    expect(saveButton(p, 'azw3').textContent).toBe('Save…');
    // Not on the page, and the machine not asked, until it is wanted.
    expect(p.node.querySelector('section.kfx-setup')).toBeNull();
    expect(p.tauri.callsTo('kfx-status')).toEqual([]);
    row.button('Set it up').click();
    await settle();
    expect(p.tauri.callsTo('kfx-status').length).toBe(1);
    const block = p.node.querySelector('section.kfx-setup')!;
    expect(block.hidden).toBe(false);
    const { HEADING } = await import(join(UI, 'kfx.js'));
    expect(block.getAttribute('aria-label')).toBe(HEADING);
    expect(block.querySelector('.state-label')!.textContent).toBe(HEADING);
    expect(block.querySelectorAll('.device-row').length).toBeGreaterThan(0);
    // Closing Other ways puts it away, and it asks nothing more.
    p.node.button('Other ways').click();
    expect(p.node.querySelector('section.kfx-setup')).toBeNull();
    p.page.fireWindow('focus');
    await settle();
    expect(p.tauri.callsTo('kfx-status').length).toBe(1);
  });

  test('with no Calibre, AZW3 is greyed too, with the same Set it up', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('mobi', true) });
    openWays(p);
    for (const type of ['kfx', 'azw3']) expect(saveRow(p, type).button('Set it up')).toBeDefined();
    expect(saveButton(p, 'mobi').textContent).toBe('Save…');
  });

  test('the KFX checklist is asked one probe at a time, on Set it up and on focus, and not once the panel is hidden', async () => {
    // Re-homed from desktop-ui's KFX checklist on the Send page.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('azw3', true), engine: kfxProbe(HOLD) });
    openWays(p);
    saveRow(p, 'kfx').button('Set it up').click();
    await settle();
    const probes = () => p.tauri.callsTo('kfx-status');
    expect(probes().length).toBe(1);
    p.page.fireWindow('focus');
    p.page.fireWindow('focus');
    await settle();
    expect(probes().length).toBe(1);
    probes()[0].answer({ ok: false });
    await settle();
    p.page.fireWindow('focus');
    await settle();
    expect(probes().length).toBe(2);
    probes()[1].answer({ ok: false });
    await settle();
    p.panel.panelHidden();
    p.page.fireWindow('focus');
    await settle();
    expect(probes().length).toBe(2);
  });

  test('the KFX installer runs only from its button, and an install and a send never overlap', async () => {
    // Re-homed from desktop-ui's KFX checklist on the Send page. It downloads
    // third-party code and writes into the reader's Calibre.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('azw3', true), engine: kfxProbe(notReady) });
    openWays(p);
    saveRow(p, 'kfx').button('Set it up').click();
    await settle();
    p.page.fireWindow('focus');
    await settle();
    expect(p.tauri.callsTo('kfx-install')).toEqual([]);
    const books = () => p.node.querySelector('section.other-ways')!.button('Add to Apple Books');
    installButton(p).click();
    await settle();
    expect(p.tauri.callsTo('kfx-install').length).toBe(1);
    // The busy hook takes every other button out of reach while it runs.
    expect(books().disabled).toBe(true);
    expect(p.brass()!.disabled).toBe(true);
    books().click();
    await settle();
    expect(p.tauri.callsTo('route')).toEqual([]);
    p.tauri.callsTo('kfx-install')[0].answer({ ok: false, error: { code: 'kfx-install-failed', message: 'No network.' } });
    await settle();
    expect(books().disabled).toBe(false);

    // And the other way round: a route running holds the installer off.
    books().click();
    await settle();
    expect(p.tauri.callsTo('route').length).toBe(1);
    installButton(p).click();
    await settle();
    expect(p.tauri.callsTo('kfx-install').length).toBe(1);
    p.tauri.callsTo('route')[0].answer({ ok: true, note: 'Added.' });
    await settle();
  });

  test('the KFX plugin cannot be installed under a running Kindle build (re-homed)', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('azw3', false), engine: kfxProbe(notReady) });
    expect(building(p)).toBe(1);
    openWays(p);
    saveRow(p, 'kfx').button('Set it up').click();
    await settle();
    expect(installButton(p).disabled).toBe(true);
    await answer(p, 'export', { ok: true, path: '/lib/field-station/Field Station.azw3', extension: 'azw3' });
    expect(installButton(p).disabled).toBe(false);
  });

  test('a KFX install that ends asks the check again, so the brass and the KFX row follow', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], check: HOLD,
      engine: (a) => (a[0] === 'kfx-status' ? notReady : a[0] === 'kfx-install' ? installed : undefined) });
    await answerCheck(p, checked('azw3', true));
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.azw3)');
    openWays(p);
    saveRow(p, 'kfx').button('Set it up').click();
    await settle();
    installButton(p).click();
    await settle();
    await answerCheck(p, checked('kfx', true));
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.kfx)');
    expect(saveButton(p, 'kfx').textContent).toBe('Save…');
    // The checklist stays up to say it worked.
    expect(p.node.querySelector('section.kfx-setup')!.hidden).toBe(false);
  });

  test('an AZW3 built ahead, then the KFX plugin installed: exactly one KFX build ahead', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD,
      engine: (a) => (a[0] === 'kfx-status' ? notReady : a[0] === 'kfx-install' ? installed : undefined) });
    await answerCheck(p, checked('azw3', false));
    expect(building(p)).toBe(1);
    await answer(p, 'export', { ok: true, path: '/lib/field-station/Field Station.azw3', extension: 'azw3' });
    await answerCheck(p, checked('azw3', true));
    expect(building(p)).toBe(0);
    openWays(p);
    saveRow(p, 'kfx').button('Set it up').click();
    await settle();
    installButton(p).click();
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    // It fails; a focus return does not try it again.
    await answer(p, 'export', quit);
    p.page.fireWindow('focus');
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
  });

  test('a check refused as busy (a Save box open) is looked at again when the save ends', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    openWays(p);
    saveButton(p, 'epub').click();
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0); // the Save box is up: busy
    await chooseSave(p, null);
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    await answerCheck(p, checked('kfx', true));
  });

  test('Save the EPUB during a background build starts at once: both only read the book', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', false) });
    expect(building(p)).toBe(1);
    openWays(p);
    saveButton(p, 'epub').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.epub');
    expect(p.status().textContent).toBe('Saving…');
    expect(building(p)).toBe(2);
    await answerNewest(p, 'export', { ok: true, path: '/Users/me/Desktop/Field Station.epub', extension: 'epub' });
    expect(p.status().textContent).toBe('Saved Field Station.epub.');
    // The build is still going, and still says so.
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
  });

  test('Save KFX pressed mid-build waits for that build, and the line stays up until it is done', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', false) });
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    // No second build: the press waits behind the one running.
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    expect(p.status().textContent).toBe('');
    expect(p.node.buttons().every((b) => b.disabled)).toBe(true);
    await answer(p, 'export', builtKfx);
    // Its own export now runs, and finds the file that build just made: the
    // line stays up until that answers.
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    expect((await answer(p, 'export', { ...builtKfx, path: '/Users/me/Desktop/Field Station.kfx' })).slice(3, 5))
      .toEqual(['--for', 'kfx']);
    expect(buildLine(p)).toBe(null);
    expect(p.status().textContent).toBe('Saved Field Station.kfx.');
  });

  test('a press finds the file current: no build line', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('kfx', true) });
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    expect(building(p)).toBe(1);
    expect(buildLine(p)).toBe(null);
    await answer(p, 'export', builtKfx);
  });

  test('back on the result, Save KFX pressed before the check answers puts the bar up, even after a press that found the file current', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', true) });
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    expect(buildLine(p)).toBe(null);
    await answer(p, 'export', builtKfx);
    p.tauri.respond((a) => (isCheck(a) ? HOLD : a[0] === 'routes' ? listed([]) : HOLD));
    p.panel.panelHidden();
    p.panel.panelShown();
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answerCheck(p, checked('kfx', false, 2000));
    await answer(p, 'export', builtKfx);
    expect(buildLine(p)).toBe(null);
  });

  test('Save MOBI builds beside the book without rewriting it, so it starts beside a copy', async () => {
    const { inTurn, holders } = await import(join(UI, 'book-queue.js'));
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, check: checked('mobi', false) });
    let copied: () => void = () => {};
    const copying = inTurn(EPUB, 'copy', () => new Promise<void>((resolve) => { copied = resolve; }));
    openWays(p);
    saveButton(p, 'mobi').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.mobi');
    expect(building(p)).toBe(1);
    expect(holders(EPUB)).toEqual(['copy', 'kindle']);
    expect(buildLine(p)).toBe('Building the Kindle file. This takes a moment.');
    await answer(p, 'export', { ok: true, path: '/Users/me/Desktop/Field Station.mobi', extension: 'mobi' });
    copied();
    await copying;
  });

  test('a build that fails says nothing (nobody asked), and Save KFX builds again with the line up', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: checked('kfx', false) });
    await answer(p, 'export', quit);
    expect(buildLine(p)).toBe(null);
    expect(p.status().textContent).toBe('');
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', quit);
    expect(p.status().textContent).toBe('Kindle Previewer quit.');
    expect(buildLine(p)).toBe(null);
  });

  test('a save that waited behind the build and changed the book: Save KFX after it puts the line up', async () => {
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, check: HOLD });
    await answerCheck(p, checked('kfx', false));
    let free: () => void = () => {};
    const saving = inTurn(EPUB, 'save', () => new Promise<void>((resolve) => { free = resolve; }));
    await answer(p, 'export', builtKfx);
    free();
    await saving;
    await settle();
    await answerCheck(p, checked('kfx', false));
    expect(building(p)).toBe(0);
    openWays(p);
    saveButton(p, 'kfx').click();
    await settle();
    await chooseSave(p, '/Users/me/Desktop/Field Station.kfx');
    expect(buildLine(p)).toBe(HALF_A_MINUTE);
    await answer(p, 'export', builtKfx);
    expect(buildLine(p)).toBe(null);
  });

  test('the keyboard goes back to the Save button after its Save box is cancelled', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    openWays(p);
    saveButton(p, 'epub').focus();
    saveButton(p, 'epub').click();
    await settle();
    // The Save box has the focus; when it closes, the window's own handler
    // would put it on the page's first live stop.
    p.page.doc.body.focus();
    await chooseSave(p, null);
    expect(p.page.doc.activeElement).toBe(saveButton(p, 'epub'));
    expect(p.restored()).toBe(0);
  });

  test('Show in Finder and the book’s path sit at the foot of Save a file', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    const reveal = p.page.doc.createElement('button');
    reveal.textContent = 'Show in Finder';
    const path = p.page.doc.createElement('p');
    path.textContent = EPUB;
    p.panel.mountPanel(p.node, p.ctx, { footer: [reveal, path] });
    await settle();
    openWays(p);
    const save = p.node.querySelectorAll('.ways-group')[1];
    expect(save.childNodes.slice(-2)).toEqual([reveal, path]);
    // Still there after a redraw.
    await p.plug([KOBO]);
    expect(p.node.querySelectorAll('.ways-group')[1].childNodes.slice(-2)).toEqual([reveal, path]);
  });
});

describe('after a copy over USB', () => {
  const ON_IT = 'Field Station is on your KOBOeReader. Eject it before you unplug.';
  async function copied() {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO],
      engine: (a) => (a[0] === 'export' && !a.includes('--check') ? { ok: true, path: EPUB, label: 'EPUB' }
        : a[0] === 'send' ? { ok: true, destination: '/Volumes/KOBOeReader/Field Station.epub' } : undefined) });
    p.brass()!.click();
    await settle();
    return p;
  }

  test('Eject <name> becomes the brass button, with the reminder', async () => {
    const p = await copied();
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
    expect(p.node.querySelectorAll('.calm .btn-brad').length).toBe(1);
    expect(p.status().textContent).toBe(ON_IT);
    // The copy is still there to press again, in Other ways.
    expect(openWays(p).button('Copy to KOBOeReader').className).toBe('btn btn-outline');
  });

  test('a copy that lands after another book’s result came up still leaves Eject and its line', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO],
      engine: (a) => (a[0] === 'export' && !a.includes('--check') ? { ok: true, path: EPUB, label: 'EPUB' } : undefined) });
    p.brass()!.click();
    await settle();
    const send = p.tauri.callsTo('send');
    expect(send.length).toBe(1);
    // Another script converted while the copy ran.
    p.ctx.state.script = { ...p.ctx.state.script, title: 'Other', epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    send[0].answer({ ok: true, destination: '/Volumes/KOBOeReader/Field Station.epub' });
    await settle();
    expect(p.ctx.state.sendResult.eject.volume).toBe('/Volumes/KOBOeReader');
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
    expect(p.status().textContent).toBe(ON_IT);
  });

  test('Eject asks the engine for that volume only; done, the button goes and the line says so', async () => {
    const p = await copied();
    p.brass()!.click();
    await settle();
    expect(p.status().textContent).toBe('Ejecting KOBOeReader…');
    expect(p.brass()!.disabled).toBe(true);
    const call = p.tauri.callsTo('eject')[0];
    expect(call.args).toEqual(['eject', '/Volumes/KOBOeReader', '--json']);
    call.answer({ ok: true, ejected: '/Volumes/KOBOeReader' });
    await settle();
    expect(p.tauri.callsTo('eject').length).toBe(1);
    expect(p.status().textContent).toBe('Your KOBOeReader is ejected. You can unplug it now.');
    // The poll still lists it until it goes.
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
    expect(p.brass()!.disabled).toBe(false);
  });

  test('a reader that will not eject says why and keeps the button', async () => {
    const p = await copied();
    p.brass()!.click();
    await settle();
    p.tauri.callsTo('eject')[0].answer({ ok: false, error: { code: 'eject-failed', message: 'could not eject /Volumes/KOBOeReader: a file is open' } });
    await settle();
    expect(p.status().textContent).toBe('could not eject /Volumes/KOBOeReader: a file is open');
    expect(p.status().classList.contains('bad')).toBe(true);
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
    expect(p.brass()!.disabled).toBe(false);
  });

  test('the line and the button are drawn again on return (spec 13)', async () => {
    const p = await copied();
    p.panel.panelHidden();
    p.panel.mountPanel(p.node, p.ctx);
    p.panel.panelShown();
    await settle();
    expect(p.ctx.state.sendResult.eject.volume).toBe('/Volumes/KOBOeReader');
    expect(p.status().textContent).toBe(ON_IT);
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
  });

  test('unplugged without ejecting: the button goes, the line stays', async () => {
    const p = await copied();
    await p.plug([]);
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(p.status().textContent).toBe(ON_IT);
    expect(p.ctx.state.sendResult.eject).toBe(null);
  });

  test('a copy started from the heads-up while My device isn’t here is up still ends on Eject (spec 12)', async () => {
    const p = await mountPanel({ devices: [KINDLE],
      engine: (a) => (a[0] === 'export' && !a.includes('--check') ? { ok: true, path: '/lib/f/F.azw3', extension: 'azw3', label: 'AZW3' }
        : a[0] === 'send' ? { ok: true, destination: '/Volumes/Kindle/documents/F.azw3' }
          : a.includes('--set') ? { ok: true, readerDevice: 'kindle' } : undefined) });
    p.node.button('My device isn’t here').click();
    await settle();
    await p.panel.copyTo(dev(KINDLE));
    await settle();
    expect(p.brass()!.textContent).toBe('Eject Kindle');
    expect(p.status().textContent).toBe('Field Station is on your Kindle. Eject it before you unplug.');
    expect(p.ctx.state.readerDevice).toBe('kindle');
  });

  test('a reader already chosen is kept: a Kindle copied to from a Kobo reader’s cards still shows Eject', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KINDLE],
      engine: (a) => (a[0] === 'export' && !a.includes('--check') ? { ok: true, path: '/lib/f/F.azw3', extension: 'azw3', label: 'AZW3' }
        : a[0] === 'send' ? { ok: true, destination: '/Volumes/Kindle/documents/F.azw3' } : undefined) });
    p.node.button('Different device').click();
    await settle();
    await p.panel.copyTo(dev(KINDLE));
    await settle();
    expect(p.brass()!.textContent).toBe('Eject Kindle');
    expect(p.ctx.state.readerDevice).toBe('kobo');
    expect(p.tauri.callsTo('app-settings').filter((c) => c.args.includes('--set'))).toEqual([]);
  });

  test('a different book keeps the pending Eject and its line (spec 13), and ejecting it there clears it', async () => {
    const p = await copied();
    p.ctx.state.script = { ...p.ctx.state.script, title: 'Other', epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    p.panel.panelShown();
    await settle();
    expect(p.status().textContent).toBe(ON_IT);
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
    p.brass()!.click();
    await settle();
    expect(p.tauri.callsTo('eject')[0].args).toEqual(['eject', '/Volumes/KOBOeReader', '--json']);
    p.tauri.callsTo('eject')[0].answer({ ok: true, ejected: '/Volumes/KOBOeReader' });
    await settle();
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
  });

  test('a different book does not inherit a line with no Eject behind it', async () => {
    const p = await copied();
    await p.plug([]);
    p.ctx.state.script = { ...p.ctx.state.script, epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    expect(p.status().textContent).toBe('');
    expect(p.ctx.state.sendResult).toBe(null);
  });
});

// Re-homed from send-routes-ui.test.ts's mounted block, which drove the
// Send page (send.js) before it was deleted: every behaviour of that page
// the panel still has, driven here on the panel. The route list is held
// where a test needs to answer it by hand, quick and full apart.
describe('the route list, as the Send page used to be driven', () => {
  const RM: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };
  const held = (a: string[]) => (a[0] === 'routes' || a[0] === 'route' ? HOLD : undefined);
  const routeCalls = (p: Mounted, book = EPUB) => p.tauri.callsTo('routes').filter((c) => c.args[1] === book);
  /** Answer every held routes call for `book` with `value`, oldest first. */
  async function answerAll(p: Mounted, value: unknown, book = EPUB) {
    for (const call of p.tauri.pending().filter((c) => c.args[0] === 'routes' && c.args[1] === book)) call.answer(value);
    await settle();
  }
  const row = (p: Mounted | FakeNode, title: string): FakeNode => {
    const within: FakeNode = p instanceof FakeNode ? p : p.node;
    return within.querySelectorAll('.device-row')
      .find((r) => r.querySelector('.device-name')!.textContent === title)!;
  };

  test('a fresh result: the keyboard the page parked while the list was out goes to the brass button when it lands (found live, 2026-10-02)', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], engine: held, realFocus: true });
    // convert.js hands the keyboard to the page's plan after drawing the
    // result: with no brass yet, the plan's first stop is Other ways.
    p.ctx.restoreFocus();
    expect(p.page.doc.activeElement).toBe(p.node.button('Other ways'));
    await answerAll(p, listed([KINDLE]));
    expect(p.brass()!.textContent).toStartWith('Copy to Kindle');
    expect(p.page.doc.activeElement).toBe(p.brass());
  });

  test('a fresh result: a reader who moved the keyboard themselves before the list lands is left there', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE], engine: held, realFocus: true });
    p.ctx.restoreFocus();
    p.node.button('Different device').focus();
    await answerAll(p, listed([KINDLE]));
    expect(p.page.doc.activeElement).toBe(p.node.button('Different device'));
  });

  test('the quick answer is drawn at once, and the reMarkable row fills in when the full one lands', async () => {
    // The quick answer leaves out the reMarkable probe, which with no tablet
    // docked runs to its whole timeout with nothing on screen.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    openWays(p);
    await answer(p, 'routes', listed([KOBO], { remarkableChecked: false }));
    expect(p.tauri.pending().filter((c) => c.args[0] === 'routes').length).toBe(1);
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
    expect(row(p, 'reMarkable').textContent).toContain('checking whether one is docked…');
    expect(row(p, 'reMarkable').className).toContain('route-unavailable');
    await answer(p, 'routes', listed([KOBO, RM]));
    expect(row(p, 'reMarkable').className).toBe('device-row');
    expect(row(p, 'reMarkable').button('Upload to reMarkable').disabled).toBe(false);
  });

  test('a full answer that lands before the quick one is not replaced by it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    await answerNewest(p, 'routes', listed([KOBO, RM]));
    await answer(p, 'routes', listed([KOBO], { remarkableChecked: false }));
    expect(p.node.buttons('Upload to reMarkable').length).toBe(1);
    expect(p.node.textContent).not.toContain('checking whether one is docked');
  });

  test('a list in a shape the window cannot read is its own line, never a short list, until a good one comes', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    const broken = listed([]);
    (broken.routes[0] as { title: unknown }).title = '';
    await answerAll(p, broken);
    expect(p.node.querySelector('.calm .fault-body')!.textContent)
      .toBe('The engine listed the ways to send this book in a shape this window cannot read.');
    expect(p.brass()).toBeNull();
    expect(openWays(p).querySelectorAll('.device-row')).toEqual([]);
    await p.plug([]);
    await answerAll(p, listed([]));
    expect(p.node.querySelector('.calm .fault-body')).toBeNull();
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
  });

  test('the same list again redraws nothing, so the keyboard stays put', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    openWays(p);
    const save = saveButton(p, 'epub');
    save.focus();
    const asked = p.tauri.callsTo('routes').length;
    await p.plug([]);
    expect(p.tauri.callsTo('routes').length).toBe(asked + 2);
    expect(save.isConnected).toBe(true);
    expect(p.page.doc.activeElement).toBe(save);
    expect(p.restored()).toBe(0);
  });

  test('when the focused route is gone, the brass button in its place takes the keyboard', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO] });
    p.brass()!.focus();
    await p.plug([]);
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(p.page.doc.activeElement).toBe(p.brass());
    expect(p.restored()).toBe(0);
  });

  test('with no brass button drawn yet, the page’s own plan takes the keyboard', async () => {
    const p = await mountPanel({ realFocus: true,
      engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: 'kobo' } : held(a)) });
    const card = p.node.button('Kobo');
    card.focus();
    card.click();
    await settle();
    expect(p.brass()).toBeNull();
    expect(p.restored()).toBe(1);
    // The plan's pick: the first control on the calm screen, its asterisk.
    expect(p.page.doc.activeElement).toBe(p.node.querySelector('.calm .asterisk'));
  });

  for (const kind of ['Kobo', 'Kindle']) {
    test(`a ${kind} card pressed from the keyboard: the brass button takes it from the plan once the list lands`, async () => {
      const p = await mountPanel({ realFocus: true,
        engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: kind.toLowerCase() } : held(a)) });
      const card = p.node.button(kind);
      card.focus();
      card.click();
      await settle();
      expect(p.brass()).toBeNull();
      const parked = p.page.doc.activeElement;
      expect(parked !== null && p.node.contains(parked)).toBe(true);
      // The quick list lands, then the full one: each redraw keeps it owed.
      await answer(p, 'routes', listed([], { remarkableChecked: false }));
      expect(p.page.doc.activeElement).toBe(p.brass());
      await answer(p, 'routes', listed([]));
      expect(p.page.doc.activeElement).toBe(p.brass());
      expect(p.restored()).toBe(1);
    });
  }

  test('a reader who moves the keyboard before the list lands is left where they went', async () => {
    const p = await mountPanel({ realFocus: true,
      engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: 'kobo' } : held(a)) });
    const card = p.node.button('Kobo');
    card.focus();
    card.click();
    await settle();
    p.node.button('Other ways').focus();
    await answerAll(p, listed([]));
    expect(p.brass()).not.toBeNull();
    expect(p.page.doc.activeElement).toBe(p.node.button('Other ways'));
  });

  test('a list that was out when a route began lands without touching the panel; the ask after the route does', async () => {
    // A rebuild mid-route would hand back fresh, enabled buttons and let a
    // second route (or a send) start while the first is still running.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    await answerAll(p, listed([]));
    await p.plug([KOBO]);
    openWays(p).button('Add to Apple Books').click();
    await answerAll(p, listed([KOBO]));
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(p.node.buttons().every((b) => b.disabled)).toBe(true);
    await answer(p, 'route', { ok: true, note: 'Added to Apple Books.' });
    expect(p.node.buttons().every((b) => !b.disabled)).toBe(true);
    await answerAll(p, listed([KOBO]));
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
  });

  test('an older list that lands after the ask a route made cannot put the old brass back', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    await answerAll(p, listed([KOBO]));
    await p.plug([]); // out before the route below starts...
    openWays(p).button('Add to Apple Books').click();
    await answer(p, 'route', { ok: true, note: 'Added to Apple Books.' });
    await answerNewest(p, 'routes', listed([]));
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    await answerAll(p, listed([KOBO])); // ...and back last
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
  });

  test('an answer about the last book is not drawn on the next one’s panel', async () => {
    // The routes answer is about a BOOK: a refusal for the old one ("cannot
    // read it") would stand under the new one's title as if it were its.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: held });
    p.ctx.state.script = { ...p.ctx.state.script, epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    await answerAll(p, { ok: false, error: { code: 'unreadable', message: 'Cannot read the book.' } });
    expect(p.node.querySelector('.calm .fault-body')).toBeNull();
    expect(p.node.querySelector('.calm-line')!.textContent).toBe('Looking for every way to send it…');
    expect(routeCalls(p, '/lib/other/Other.epub').length).toBe(2);
    await answerAll(p, listed([]), '/lib/other/Other.epub');
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
  });

  test('a route from Send it: every button dead while it runs, the engine’s note after, then the list again', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: (a) => (a[0] === 'route' ? HOLD : undefined) });
    openWays(p).button('Add to Apple Books').click();
    expect(p.status().textContent).toBe('Opening Apple Books…');
    expect(p.node.buttons().every((b) => b.disabled)).toBe(true);
    const asked = p.tauri.callsTo('routes').length;
    const args = await answer(p, 'route', { ok: true, key: 'apple-books', note: 'Added to Apple Books.' });
    expect(args).toEqual(['route', 'apple-books', EPUB, '--json']);
    expect(p.status().textContent).toBe('Added to Apple Books.');
    expect(p.status().classList.contains('bad')).toBe(false);
    expect(p.node.buttons().every((b) => !b.disabled)).toBe(true);
    expect(p.tauri.callsTo('routes').length).toBe(asked + 1);
  });

  test('a book replaced mid-route hears nothing about it, and asks for no list for it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: (a) => (a[0] === 'route' ? HOLD : undefined) });
    openWays(p).button('Add to Apple Books').click();
    p.ctx.state.script = { ...p.ctx.state.script, epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    await answer(p, 'route', { ok: true, note: 'Added to Apple Books.' });
    expect(p.status().textContent).toBe('');
    expect(routeCalls(p).length).toBe(2);
  });

  test('only a save waits for the book: a route that opens another program asks at once', async () => {
    // Apple Books, Send to Kindle and Mail hand the book's path to another
    // program and return; the engine reads nothing of the book. A save
    // copies the book itself, so it waits for whatever holds the book.
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, engine: (a) => (a[0] === 'route' ? HOLD : undefined) });
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    let free: () => void = () => {};
    const holding = inTurn(EPUB, 'save', () => new Promise<void>((resolve) => { free = resolve; }));
    try {
      openWays(p).button('Add to Apple Books').click();
      expect(p.tauri.callsTo('route').length).toBe(1);
      await answer(p, 'route', { ok: true, note: 'Added to Apple Books.' });
      saveButton(p, 'epub').click();
      await settle();
      await chooseSave(p, '/Users/me/Desktop/Field Station.epub');
      expect(builds(p)).toEqual([]);
    } finally {
      free();
      await holding;
    }
    await settle();
    expect(builds(p).map((c) => c.args.slice(0, 5))).toEqual([['export', EPUB, '--json', '--for', 'epub']]);
  });

  test('the email row’s first-time link asks the engine to open Amazon’s page, and the keyboard comes back to it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, engine: (a) => (a[0] === 'route' ? HOLD : undefined) });
    const link = row(openWays(p), 'Send to Kindle email').button('Open Amazon’s page');
    link.focus();
    link.click();
    expect(p.status().textContent).toBe('Opening Amazon’s page…');
    p.page.doc.body.focus();
    expect(await answer(p, 'route', { ok: false, error: { code: 'open-failed', message: 'No browser.' } }))
      .toEqual(['route', 'kindle-email-setup', '--json']);
    expect(p.status().textContent).toBe('No browser.');
    expect(p.page.doc.activeElement).toBe(row(p, 'Send to Kindle email').button('Open Amazon’s page'));
    row(p, 'Send to Kindle email').button('Open Amazon’s page').click();
    await answer(p, 'route', { ok: true, note: 'Opened Amazon’s page.' });
    expect(p.status().textContent).toBe('Opened Amazon’s page.');
  });

  test('a dimmed email row still offers the first-time link, and no send button', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' },
      engine: (a) => (a[0] === 'routes' ? listed([], { appleMailDefault: false }) : undefined) });
    const email = row(openWays(p), 'Send to Kindle email');
    expect(email.className).toContain('route-unavailable');
    expect(email.querySelectorAll('button').map((b) => b.textContent)).toEqual(['Open Amazon’s page']);
  });

  test('another page or another book meanwhile leaves the keyboard where it is', async () => {
    // Nothing to take it from when the reader has moved on.
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, engine: (a) => (a[0] === 'route' ? HOLD : undefined) });
    p.brass()!.focus();
    p.brass()!.click();
    p.node.hidden = true; // the reader went to Read meanwhile
    await answer(p, 'route', { ok: false, error: { code: 'x', message: 'No.' } });
    expect(p.restored()).toBe(0);
    p.node.hidden = false;

    p.brass()!.focus();
    p.brass()!.click();
    p.ctx.state.script = { ...p.ctx.state.script, epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.node, p.ctx);
    await settle();
    const before = p.restored();
    await answer(p, 'route', { ok: false, error: { code: 'x', message: 'No.' } });
    expect(p.restored()).toBe(before);
  });

  test('Send it: every other route in the engine’s order, outlined, the engine’s detail as written, dimmed ones with no button', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE, KOBO, RM] });
    const said = listed([KINDLE, KOBO, RM]);
    const ways = openWays(p);
    const rows = ways.querySelectorAll('.ways-group')[0].querySelectorAll('.device-row');
    const titles = rows.map((r) => r.querySelector('.device-name')!.textContent);
    // The Kindle's follow-ups lead; the rest keep the engine's order; the
    // brass one and the two saves are not listed.
    const rest = said.routes.filter((r) => !['save-epub', 'save-kindle', 'send-to-kindle', 'email-to-kindle'].includes(r.key)
      && r.device?.kind !== 'kindle');
    expect(titles).toEqual(['Send to Kindle web', 'Send to Kindle email', ...rest.map((r) => r.title)]);
    for (const r of rows) {
      const route = said.routes.find((x) => x.title === r.querySelector('.device-name')!.textContent)!;
      expect(r.querySelector('.route-detail')!.textContent).toBe(route.detail);
      const own = r.querySelectorAll('button').filter((b) => b.dataset.key === route.id);
      if (route.available) {
        expect(r.className).toBe('device-row');
        expect(own.map((b) => b.className)).toEqual(['btn btn-outline']);
        expect(own[0].textContent).toBe(route.button);
      } else {
        expect(r.className).toBe('device-row route-unavailable');
        expect(own).toEqual([]);
      }
    }
    // A mounted reader keeps its volume line; a docked reMarkable, which
    // never mounts, is described once, in the engine's words.
    expect(row(p, 'KOBOeReader').querySelector('.device-where')!.textContent).toBe('/Volumes/KOBOeReader');
    expect(row(p, 'reMarkable').querySelectorAll('.route-detail').map((n) => n.textContent))
      .toEqual(['the EPUB, over its USB connection']);
    expect(row(p, 'reMarkable').querySelector('.device-where')).toBeNull();
  });
});
