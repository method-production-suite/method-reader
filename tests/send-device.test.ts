import { beforeAll, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import type { ConnectedDevice } from '../src/device/types';
import { preselected, routes } from '../src/export/routes';
import { READER_DEVICES } from '../src/settings/app';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
let d: any;
let r: any;
beforeAll(async () => {
  d = await import(join(UI, 'device.js'));
  r = await import(join(UI, 'send-routes.js'));
});

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const tolino: ConnectedDevice = { kind: 'tolino', name: 'tolino', volume: '/Volumes/tolino' };
const rm: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };
const PLATFORMS = { darwin: 'MacIntel', win32: 'Win32', linux: 'Linux x86_64' } as const;

/** The engine's list as the window has it (routesFrom), for these facts. */
const shownFor = (platform: keyof typeof PLATFORMS, devices: ConnectedDevice[], extra = {}) => {
  const list = routes({ platform, booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices, ...extra });
  return r.routesFrom({ ok: true, routes: list, chosen: preselected(list, undefined).id });
};
const kfx = { extension: 'kfx', fresh: false, builtBy: 'calibre', bookDate: 1 };

describe('the cards: what do you read on', () => {
  test('five readers and the way out, in the spec’s order', () => {
    const cards = d.cardsFor('MacIntel', []);
    expect(cards.map((c: any) => c.name)).toEqual(
      ['Kindle', 'iPhone, iPad or Mac', 'Kobo', 'tolino', 'reMarkable', 'My device isn’t here']);
    expect(cards[1].sub).toBe('Apple Books');
  });
  test('every card but the last is a kind the engine will store', () => {
    expect(d.cardsFor('MacIntel', []).slice(0, -1).map((c: any) => c.kind)).toEqual([...READER_DEVICES]);
  });
  test('a reader the computer can see is marked here; an Apple card never is', () => {
    const here = d.cardsFor('MacIntel', [{ id: '/Volumes/KOBOeReader', ...kobo }])
      .filter((c: any) => c.here).map((c: any) => c.kind);
    expect(here).toEqual(['kobo']);
    expect(d.cardsFor('MacIntel', null).some((c: any) => c.here)).toBe(false);
  });
});

describe('the asterisk', () => {
  test('a Kindle on a Mac and Apple Books carry none; everything else does', () => {
    expect(d.unproven('kindle', 'MacIntel')).toBe(false);
    expect(d.unproven('apple', 'Win32')).toBe(false);
    for (const p of ['Win32', 'Linux x86_64', undefined]) expect(d.unproven('kindle', p)).toBe(true);
    for (const k of ['kobo', 'tolino', 'remarkable']) expect(d.unproven(k, 'MacIntel')).toBe(true);
  });
  test('it says the spec’s sentence, by name, and is true for a Kindle off a Mac', () => {
    expect(d.asteriskNote('kobo', 'MacIntel'))
      .toBe('Built and tested in code, never tried on a real Kobo yet. If you try it, tell us how it went.');
    expect(d.asteriskNote('kindle', 'Win32'))
      .toBe('Built and tested in code, never tried with a real Kindle on Windows yet. If you try it, tell us how it went.');
    expect(d.asteriskNote('kindle', 'MacIntel')).toBe(null);
  });
});

describe('offerFor: the one brass button', () => {
  test('a Kindle plugged in: Copy leads, naming the file; then Send to Kindle, then email', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', [kindle]), kindle: kfx });
    expect(o.brass.label).toBe('Copy to Kindle (.kfx)');
    expect(o.next.map((a: any) => a.route.key)).toEqual(['send-to-kindle', 'email-to-kindle']);
    expect(o.lines).toEqual([]);
    expect(o.unproven).toBe(false);
  });
  test('no Kindle plugged in: Send to Kindle leads, then email, then "plug it in"', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', []), kindle: kfx });
    expect(o.brass.route.key).toBe('send-to-kindle');
    expect(o.next.map((a: any) => a.route.key)).toEqual(['email-to-kindle']);
    expect(o.lines).toEqual([d.WEB_LINE, d.PLUG_KINDLE]);
  });
  test('no Kindle plugged in and Send to Kindle out of reach: email leads, before Save the EPUB (spec 6)', () => {
    const shown = shownFor('darwin', []);
    shown.routes = shown.routes.map((route: any) => (route.key === 'send-to-kindle'
      ? { ...route, available: false, unavailable: 'platform' } : route));
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown });
    expect(o.brass.route.key).toBe('email-to-kindle');
    expect(o.next).toEqual([]);
    expect(o.lines).toEqual([d.PLUG_KINDLE]);
    // Neither: the EPUB, as before.
    shown.routes = shown.routes.map((route: any) => (route.key === 'email-to-kindle'
      ? { ...route, available: false, unavailable: 'setup' } : route));
    expect(d.offerFor({ device: 'kindle', platform: 'MacIntel', shown }).brass.route.key).toBe('save-epub');
  });
  test('Amazon’s app, when the engine says it is installed, is the same lead under its own name', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', [], { sendToKindleApp: true }) });
    expect(o.brass.label).toBe('Send to Kindle app');
    // The app takes the book itself: there is nothing to drag.
    expect(o.lines).toEqual([d.PLUG_KINDLE]);
  });
  test('Apple: Add to Apple Books on a Mac, Save the EPUB elsewhere with its line', () => {
    expect(d.offerFor({ device: 'apple', platform: 'MacIntel', shown: shownFor('darwin', []) }).brass.route.key)
      .toBe('apple-books');
    const win = d.offerFor({ device: 'apple', platform: 'Win32', shown: shownFor('win32', []) });
    expect(win.brass.route.key).toBe('save-epub');
    expect(win.lines).toEqual([d.APPLE_ELSEWHERE]);
  });
  test('Kobo, tolino, reMarkable: their own row when connected, the EPUB and a "plug it in" line when not', () => {
    expect(d.offerFor({ device: 'kobo', platform: 'MacIntel', shown: shownFor('darwin', [kobo]) }).brass.label)
      .toBe('Copy to KOBOeReader');
    const away = d.offerFor({ device: 'kobo', platform: 'MacIntel', shown: shownFor('darwin', []) });
    expect(away.brass.route.key).toBe('save-epub');
    expect(away.lines).toEqual(['Plug your Kobo in over USB and this becomes a Copy button.']);
    expect(d.offerFor({ device: 'remarkable', platform: 'MacIntel', shown: shownFor('darwin', [rm]) }).brass.route.key)
      .toBe('remarkable');
    expect(d.offerFor({ device: 'tolino', platform: 'Win32', shown: shownFor('win32', []) }).lines)
      .toEqual([r.undetectable('Win32')]);
    expect(away.unproven).toBe(true);
  });
  test('before the list arrives it waits rather than guessing', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: null });
    expect(o.waiting).toBe(true);
    expect(o.brass).toBe(null);
  });
  test('on every list the engine can give, for every device: one brass, and it can fire', () => {
    for (const platform of ['darwin', 'win32', 'linux'] as const) {
      for (const set of [[], [kindle], [kobo, tolino], [kindle, rm]]) {
        for (const device of READER_DEVICES) {
          const o = d.offerFor({ device, platform: PLATFORMS[platform], shown: shownFor(platform, set) });
          expect(`${platform} ${device}: ${o.brass?.route.available}`).toBe(`${platform} ${device}: true`);
          expect(o.brass.label.trim()).not.toBe('');
          for (const a of o.next) expect(a.route.available).toBe(true);
        }
      }
    }
  });
});

describe('Other ways', () => {
  test('Send it: the device’s own follow-ups first, then every other route in the engine’s order, no saves, no brass', () => {
    const shown = shownFor('darwin', [kindle]);
    const offer = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown });
    const rows = d.sendItRows(shown, offer);
    expect(rows.slice(0, 2).map((x: any) => x.key)).toEqual(['send-to-kindle', 'email-to-kindle']);
    expect(rows.some((x: any) => x.key === 'save-epub' || x.key === 'save-kindle')).toBe(false);
    expect(rows.some((x: any) => x.id === offer.brass.route.id)).toBe(false);
    // Every non-save route is somewhere: brass or a row.
    const all = shown.routes.filter((x: any) => !x.key.startsWith('save-')).map((x: any) => x.id);
    expect(new Set([offer.brass.route.id, ...rows.map((x: any) => x.id)])).toEqual(new Set(all));
  });
  const folded = (items: any[]) => items.filter((x) => x.fold).map((x) => x.fold);
  const kept = (items: any[]) => items.filter((x) => x.route).map((x) => x.route.title);

  test('Send it folds the readers that only need plugging in or docking into one line, where the first was', () => {
    const shown = shownFor('darwin', []);
    const offer = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown });
    const items = d.foldUnplugged(d.sendItRows(shown, offer));
    expect(folded(items)).toEqual([{
      kinds: ['kindle', 'kobo', 'tolino', 'remarkable'],
      names: ['Kindle', 'Kobo', 'tolino', 'reMarkable'],
      tail: ': plug in or dock over USB to send.',
      line: 'Kindle, Kobo, tolino and reMarkable: plug in or dock over USB to send.',
    }]);
    // Nothing else is folded, and nothing is lost: what can run stays a row.
    expect(kept(items)).toEqual(['Send to Kindle email', 'Apple Books']);
    const rows = d.sendItRows(shown, offer);
    const first = rows.findIndex((x: any) => x.key.startsWith('device:') || x.key === 'remarkable');
    expect(items.findIndex((x: any) => x.fold)).toBe(first);
  });

  test('only plug-ins say plug in; a plugged-in reader is never folded; one alone stays a row', () => {
    const shown = shownFor('darwin', [kindle, rm]);
    const offer = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown });
    const items = d.foldUnplugged(d.sendItRows(shown, offer));
    expect(folded(items).map((f: any) => f.line)).toEqual(['Kobo and tolino: plug in over USB to send.']);
    expect(kept(items)).toContain('reMarkable');
    const one = shownFor('darwin', [kindle, kobo, rm]);
    const alone = d.foldUnplugged(d.sendItRows(one, d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: one })));
    expect(folded(alone)).toEqual([]);
    expect(kept(alone)).toContain('tolino');
  });

  test('dimmed for any other reason stays a row: email needing setup, a tolino on Windows, a reMarkable still being looked for', () => {
    const win = shownFor('win32', []);
    const items = d.foldUnplugged(d.sendItRows(win, d.offerFor({ device: 'kobo', platform: 'Win32', shown: win })));
    expect(folded(items).map((f: any) => f.line)).toEqual(['Kindle, Kobo and reMarkable: plug in or dock over USB to send.']);
    expect(kept(items)).toEqual(expect.arrayContaining(['tolino', 'Send to Kindle email', 'Apple Books']));
    const looking = shownFor('darwin', [], { remarkableChecked: false });
    const still = d.foldUnplugged(d.sendItRows(looking, d.offerFor({ device: 'kobo', platform: 'MacIntel', shown: looking })));
    expect(kept(still)).toContain('reMarkable');
    expect(folded(still).map((f: any) => f.names)).toEqual([['Kindle', 'Kobo', 'tolino']]);
  });

  test('Save a file: six types, each with who it is for', () => {
    const rows = d.saveRows({ fountainPath: '/l/a.fountain' }, kfx);
    expect(rows.map((x: any) => x.type)).toEqual(['epub', 'kfx', 'azw3', 'mobi', 'fountain', 'fdx']);
    expect(rows.find((x: any) => x.type === 'fdx')).toMatchObject({ label: 'FDX' });
    expect(rows.every((x: any) => x.available && x.who.length > 0)).toBe(true);
  });
  test('KFX greys out with "set it up" when the ladder stops short of it; AZW3 when there is no Calibre', () => {
    const azw3 = d.saveRows({ fountainPath: '/l/a.fountain' }, { ...kfx, extension: 'azw3' });
    expect(azw3.find((x: any) => x.type === 'kfx')).toMatchObject({ available: false, setup: true });
    const none = d.saveRows({ fountainPath: '/l/a.fountain' }, { ...kfx, extension: 'mobi', builtBy: 'screepub' });
    expect(none.find((x: any) => x.type === 'azw3')).toMatchObject({ available: false, setup: true });
    // Not known yet: the engine refuses in its own words if it must.
    expect(d.saveRows({ fountainPath: '/l/a.fountain' }, null).every((x: any) => x.available)).toBe(true);
  });
  test('MOBI, Fountain and FDX need the .fountain', () => {
    const rows = d.saveRows({ fountainPath: null }, kfx);
    expect(rows.filter((x: any) => !x.available).map((x: any) => x.type)).toEqual(['mobi', 'fountain', 'fdx']);
    expect(rows.find((x: any) => x.type === 'fdx')).toMatchObject({ fix: d.NO_FOUNTAIN, setup: false });
  });
  test('the Kindle-file types take the Kindle line; the rest only copy', () => {
    expect(['epub', 'kfx', 'azw3', 'mobi', 'fountain', 'fdx'].map(d.saveTurn))
      .toEqual(['copy', 'kindle', 'kindle', 'kindle', 'copy', 'copy']);
  });
});

describe('after a copy: Eject', () => {
  const dev = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  test('a copy to a volume ends with the reminder and the button', () => {
    expect(d.copiedState('Field Station', dev, { ok: true, destination: '/Volumes/Kindle/documents/x' })).toEqual({
      line: 'Field Station is on your Kindle. Eject it before you unplug.', bad: false,
      eject: { id: dev.id, name: 'Kindle', volume: '/Volumes/Kindle' },
    });
    expect(d.ejectLabel({ name: 'Kindle' })).toBe('Eject Kindle');
  });
  test('a reMarkable upload has nothing to eject', () => {
    const tab = { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null };
    expect(d.copiedState('Field Station', tab, { ok: true, uploaded: true }).eject).toBe(null);
  });
  test('ejected, gone already, or refused: the refusal keeps the button', () => {
    const e = { id: dev.id, name: 'Kindle', volume: dev.volume };
    expect(d.ejectOutcome({ ok: true, ejected: dev.volume }, e))
      .toEqual({ line: 'Your Kindle is ejected. You can unplug it now.', bad: false, eject: null });
    expect(d.ejectOutcome({ ok: false, error: { code: 'unknown-device', message: 'x' } }, e))
      .toEqual({ line: 'Your Kindle is not plugged in any more.', bad: false, eject: null });
    expect(d.ejectOutcome({ ok: false, error: { code: 'eject-failed', message: 'could not eject /Volumes/Kindle: busy' } }, e))
      .toEqual({ line: 'could not eject /Volumes/Kindle: busy', bad: true, eject: e });
  });
  test('the button goes once the poll no longer lists the volume; an unknown list keeps it', () => {
    const e = { id: dev.id, name: 'Kindle', volume: dev.volume };
    expect(d.ejectStillThere(e, [dev])).toBe(true);
    expect(d.ejectStillThere(e, [])).toBe(false);
    expect(d.ejectStillThere(e, null)).toBe(true);
  });
});

describe('the heads-up', () => {
  const k1 = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  const opts = { open: true, onResult: false };
  test('the first list only seeds: a Kindle already there when the book opened is not news', () => {
    expect(d.headsUpFor(null, [k1], opts)).toEqual({ seen: [k1.id], show: null });
  });
  test('a Kindle that arrives is shown once; unplugged and back, once more', () => {
    expect(d.headsUpFor([], [k1], opts).show).toEqual(k1);
    expect(d.headsUpFor([k1.id], [k1], opts).show).toBe(null);
    const out = d.headsUpFor([k1.id], [], opts);
    expect(d.headsUpFor(out.seen, [k1], opts).show).toEqual(k1);
  });
  test('never with no book open, on the result screen, or for another kind', () => {
    expect(d.headsUpFor([], [k1], { open: false, onResult: false }).show).toBe(null);
    expect(d.headsUpFor([], [k1], { open: true, onResult: true }).show).toBe(null);
    expect(d.headsUpFor([], [{ ...k1, kind: 'kobo' }], opts).show).toBe(null);
  });
  test('its words, and how long it stays', () => {
    expect(d.headsUpLine('Field Station')).toBe('Kindle plugged in. Copy Field Station?');
    expect(d.HEADS_UP_MS).toBe(8000);
  });
});

describe('the words this adds', () => {
  test('carry no em dash, and none is blank', () => {
    const words = [d.QUESTION, d.WEB_LINE, d.PLUG_KINDLE, d.APPLE_ELSEWHERE, d.LOOKING, d.MISSING.name,
      d.MISSING_LINE, d.OTHER_WAYS, d.DIFFERENT, d.SEND_IT, d.SAVE_A_FILE, d.SEND_BOOK,
      ...d.SAVE_TYPES.flatMap((t: any) => [t.label, t.who]),
      ...['kindle', 'kobo', 'tolino', 'remarkable'].map((k) => d.asteriskNote(k, 'Win32')),
      ...['kobo', 'tolino', 'remarkable'].map((k) => d.plugLine(k, 'MacIntel'))];
    for (const w of words) {
      expect(typeof w).toBe('string');
      expect(w.trim()).not.toBe('');
      expect(w).not.toContain('—');
    }
  });
  test('readerDeviceFrom: a stored kind, or nothing', () => {
    expect(d.readerDeviceFrom({ ok: true, readerDevice: 'tolino' })).toBe('tolino');
    expect(d.readerDeviceFrom({ ok: true, readerDevice: null })).toBe(null);
    expect(d.readerDeviceFrom({ ok: true, readerDevice: 'nook' })).toBe(null);
    expect(d.readerDeviceFrom({ ok: false })).toBe(null);
  });
});

describe('kindleSession: whether the Kindle file is built ahead', () => {
  const k1 = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  test('the reader said Kindle', () => {
    expect(d.kindleSession('kindle', [])).toBe(true);
    expect(d.kindleSession('kindle', null)).toBe(true);
  });
  test('another reader chosen, but a Kindle is plugged in', () => {
    expect(d.kindleSession('kobo', [k1])).toBe(true);
  });
  test('another reader chosen, and no Kindle anywhere', () => {
    expect(d.kindleSession('kobo', [])).toBe(false);
    expect(d.kindleSession('kobo', null)).toBe(false);
    expect(d.kindleSession(null, [{ ...k1, kind: 'kobo' }])).toBe(false);
  });
});
