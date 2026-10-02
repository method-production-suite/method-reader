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
