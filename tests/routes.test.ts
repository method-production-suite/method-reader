// The route catalog: every way a finished book can leave Screepub, in the
// order the window draws them, plus the rule that what you chose last time
// beats any guess.
//
// Ported from the Swift Mac app's `send-menu` checks (the block from "route
// ordering" to the button-verb checks; the plan,
// docs/superpowers/plans/2026-09-23-send-routes-engine.md, gives the file and
// lines), each test titled with the Swift check's own message where it is a
// straight port. Translations:
//   booksAvailable     -> booksApp
//   canEmailToKindle   -> appleMailDefault (on darwin)
//   sendToKindleApp    -> sendToKindleApp
//   saveCopy           -> both save rows (save-epub, save-kindle)
//   remarkableDocked   -> a reMarkable in `devices`
//   storage keys       -> kebab case ('appleBooks' is 'apple-books', ...)
// Every check runs on darwin unless it is about platforms.
//
// NOT ported: the three `SendToKindle.legacyStoredAddress` checks. The
// window cannot read the Swift app's UserDefaults, and a Mail compose opened
// with `open -a Mail <file>` cannot be pre-addressed, so there is nothing to
// honor. The two reMarkable `inputIsPDF` checks become one: the window
// always uploads the EPUB, so the detail names the EPUB.
import { describe, expect, test } from 'bun:test';
import type { DeviceSummary } from '../src/cli-devices';
import { DEVICE_DISPLAY_NAMES, type ConnectedDevice, type DeviceKind } from '../src/device/types';
import {
  DEVICE_KINDS,
  preselected,
  routes,
  type Route,
  type RouteFacts,
} from '../src/export/routes';

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const kindleTwin: ConnectedDevice = { kind: 'kindle', name: 'KINDLE2', volume: '/Volumes/KINDLE2' };
const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const tolino: ConnectedDevice = { kind: 'tolino', name: 'tolino', volume: '/Volumes/tolino' };
const rm: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };

/** The Swift defaults: Books installed, no Amazon app, Apple Mail not the
 *  default, nothing plugged in, on a Mac. */
function facts(over: Partial<RouteFacts> = {}): RouteFacts {
  return {
    platform: 'darwin',
    devices: [],
    booksApp: true,
    sendToKindleApp: false,
    appleMailDefault: false,
    ...over,
  };
}

const find = (list: Route[], key: string) => list.find((r) => r.key === key);
const keys = (list: Route[]) => list.map((r) => r.key);

const EM_DASH = '\u2014';

describe('route ordering (ported)', () => {
  test('a plugged-in Kindle is the default route', () => {
    const first = routes(facts({ devices: [kindle] }))[0]!;
    expect(first.key).toBe('device:kindle');
    expect(first.device?.volume).toBe('/Volumes/Kindle');
  });

  test('no device -> Apple Books leads, being local and instant', () => {
    expect(routes(facts({ booksApp: true }))[0]!.key).toBe('apple-books');
  });

  test('no device and no Books -> Send to Kindle leads', () => {
    expect(routes(facts({ booksApp: false }))[0]!.key).toBe('send-to-kindle');
  });

  test('reMarkable never arrives as a volume device', () => {
    const list = routes(facts({ devices: [rm] }));
    expect(list[0]!.key).toBe('remarkable');
    expect(list.some((r) => r.key === 'device:remarkable')).toBe(false);
    expect(list.filter((r) => r.key === 'remarkable')).toHaveLength(1);
  });

  test('a docked reMarkable outranks Books', () => {
    expect(routes(facts({ devices: [rm], booksApp: true }))[0]!.key).toBe('remarkable');
  });

  test('a plugged-in volume still wins over a docked reMarkable', () => {
    expect(routes(facts({ devices: [kindle, rm] }))[0]!.key).toBe('device:kindle');
    // Whatever order the device list happens to arrive in.
    expect(routes(facts({ devices: [rm, kindle] }))[0]!.key).toBe('device:kindle');
  });

  // Save is the floor: some route is always offered, whatever is connected.
  for (const books of [true, false]) {
    for (const mail of [true, false]) {
      const r = routes(facts({ booksApp: books, appleMailDefault: mail }));
      test(`routes never empty (books:${books} mail:${mail})`, () => {
        expect(r.length).toBeGreaterThan(0);
      });
      test(`Save is always offered (books:${books} mail:${mail})`, () => {
        expect(find(r, 'save-epub')).toBeDefined();
        expect(find(r, 'save-kindle')).toBeDefined();
      });
      test(`no duplicate routes (books:${books} mail:${mail})`, () => {
        expect(new Set(r.map((x) => x.id)).size).toBe(r.length);
      });
    }
  }

  test("email route listed but unavailable when Apple Mail can't attach", () => {
    expect(find(routes(facts({ appleMailDefault: false })), 'email-to-kindle')?.available).toBe(false);
  });

  test("email placeholder's detail names the fix", () => {
    expect(find(routes(facts({ appleMailDefault: false })), 'email-to-kindle')?.detail).toContain(
      'Apple Mail',
    );
  });

  test('email route sendable when Apple Mail is the default', () => {
    expect(find(routes(facts({ appleMailDefault: true })), 'email-to-kindle')?.available).toBe(true);
  });

  test('Books route hidden when Books is absent', () => {
    expect(find(routes(facts({ booksApp: false })), 'apple-books')).toBeUndefined();
  });
});

describe('a remembered choice outranks the ordering heuristic (ported)', () => {
  const allRoutes = routes(facts({ devices: [kindle], booksApp: true }));

  test('first run falls back to the ordering', () => {
    expect(preselected(allRoutes, undefined).key).toBe('device:kindle');
  });

  test('a remembered choice wins over a plugged-in device', () => {
    expect(preselected(allRoutes, 'send-to-kindle').key).toBe('send-to-kindle');
  });

  test('remembered Apple Books survives a connected Kindle', () => {
    expect(preselected(allRoutes, 'apple-books').key).toBe('apple-books');
  });

  // A remembered route that is structurally gone (Books not installed) must
  // not strand the user.
  test('a structurally absent remembered route falls back instead of vanishing', () => {
    const noBooks = routes(facts({ booksApp: false }));
    expect(preselected(noBooks, 'apple-books').key).toBe('send-to-kindle');
  });

  test('device key is by kind, not volume path', () => {
    expect(find(allRoutes, 'device:kindle')?.key).toBe('device:kindle');
    expect(allRoutes[0]!.key).toBe('device:kindle');
    expect(allRoutes[0]!.id).toBe('device:kindle#/Volumes/Kindle');
  });
});

describe('the menu is a catalog, not a status display (ported)', () => {
  const bare = routes(facts());

  for (const kindName of ['Kindle', 'Kobo', 'tolino', 'reMarkable']) {
    test(`${kindName} is listed while disconnected, flagged unavailable`, () => {
      expect(bare.some((r) => r.title === kindName && !r.available)).toBe(true);
    });
  }

  test('the first route is always sendable', () => {
    expect(bare[0]!.available).toBe(true);
  });

  test('unavailable routes sink below every available one', () => {
    const leading = bare.findIndex((r) => !r.available);
    expect(bare.slice(0, leading).length).toBe(bare.filter((r) => r.available).length);
  });

  test("every placeholder's detail says how to make it available", () => {
    for (const r of bare.filter((x) => !x.available)) {
      expect(r.detail.includes('USB') || r.detail.includes('Apple Mail')).toBe(true);
    }
  });

  const withKobo = routes(facts({ devices: [kobo] }));

  test('a connected Kobo replaces its placeholder rather than joining it', () => {
    expect(withKobo.filter((r) => r.key === 'device:kobo')).toHaveLength(1);
  });

  test('the connected Kobo row is sendable', () => {
    expect(find(withKobo, 'device:kobo')?.available).toBe(true);
  });

  // Two same-kind devices must stay individually addressable: keyed by kind
  // alone, the second Kindle's row collides with the first and a send aimed
  // at it lands on the first one's volume.
  const twins = routes(facts({ devices: [kindle, kindleTwin] }));

  test('route ids stay unique with two same-kind devices connected', () => {
    expect(new Set(twins.map((r) => r.id)).size).toBe(twins.length);
  });

  test("the second device's row carries the second device's volume", () => {
    expect(twins.find((r) => r.title === 'KINDLE2')?.device?.volume).toBe('/Volumes/KINDLE2');
  });

  test('a remembered kindle with twins connected resolves to a sendable row', () => {
    expect(preselected(twins, 'device:kindle').available).toBe(true);
  });

  test('send-to-kindle button says app when the app will launch', () => {
    expect(find(routes(facts({ sendToKindleApp: true })), 'send-to-kindle')?.button).toBe(
      'Send to Kindle app',
    );
  });

  test('send-to-kindle button says web when the browser uploader fires', () => {
    expect(find(routes(facts({ sendToKindleApp: false })), 'send-to-kindle')?.button).toBe(
      'Send to Kindle web',
    );
  });

  test('reMarkable detail names the EPUB (the window always uploads the EPUB)', () => {
    const detail = find(routes(facts({ devices: [rm] })), 'remarkable')?.detail;
    expect(detail).toContain('EPUB');
    expect(detail).not.toContain('PDF');
  });

  // A remembered device stays chosen while unplugged, but a first run never
  // guesses at something that isn't there.
  test('a remembered Kobo stays chosen while unplugged', () => {
    expect(preselected(bare, 'device:kobo').key).toBe('device:kobo');
  });

  test('...and is flagged unavailable so the view can hold SEND', () => {
    expect(preselected(bare, 'device:kobo').available).toBe(false);
  });

  test('first run never preselects an unavailable route', () => {
    expect(preselected(bare, undefined).available).toBe(true);
  });
});

describe('the button reads the route its own verb (ported)', () => {
  test("device route's button verb is Copy, named for the device", () => {
    expect(routes(facts({ devices: [kindle] }))[0]!.button).toBe('Copy to Kindle');
  });

  test("reMarkable route's button verb is Upload", () => {
    expect(routes(facts({ devices: [rm] }))[0]!.button).toBe('Upload to reMarkable');
  });

  test("Books route's button verb is Add", () => {
    expect(routes(facts())[0]!.button).toBe('Add to Apple Books');
  });

  test("web route's button carries its mechanism, since the pair exists", () => {
    expect(routes(facts({ booksApp: false }))[0]!.button).toBe('Send to Kindle web');
  });

  test("email route's button carries its mechanism", () => {
    expect(find(routes(facts({ appleMailDefault: true })), 'email-to-kindle')?.button).toBe(
      'Send to Kindle email',
    );
  });

  test('web route title names the mechanism', () => {
    expect(find(routes(facts()), 'send-to-kindle')?.title).toBe('Send to Kindle web');
  });

  test('email route title names the mechanism', () => {
    expect(find(routes(facts({ appleMailDefault: true })), 'email-to-kindle')?.title).toBe(
      'Send to Kindle email',
    );
  });

  test("save routes' buttons stay ellipsis actions", () => {
    const list = routes(facts());
    expect(find(list, 'save-epub')?.button).toBe('Save the EPUB…');
    expect(find(list, 'save-kindle')?.button).toBe('Save a Kindle file…');
  });
});

describe('the whole list, row by row', () => {
  const DEVICE_DETAIL = 'over USB, offline, nothing leaves this computer';
  const SAVE_EPUB: Route = {
    id: 'save-epub',
    key: 'save-epub',
    title: 'Save the EPUB',
    detail: 'for email, Apple Books and most e-readers',
    button: 'Save the EPUB…',
    available: true,
  };
  const SAVE_KINDLE: Route = {
    id: 'save-kindle',
    key: 'save-kindle',
    title: 'Save a Kindle file',
    detail: 'for copying to a Kindle by hand',
    button: 'Save a Kindle file…',
    available: true,
  };
  const placeholder = (kind: DeviceKind, name: string): Route => ({
    id: `device:${kind}`,
    key: `device:${kind}`,
    title: name,
    detail: 'plug in over USB to send',
    button: `Copy to ${name}`,
    available: false,
    unavailable: 'connect',
  });

  test('a Mac with everything: every available row, in Swift order, then the rest', () => {
    const list = routes(
      facts({ devices: [kindle, rm], booksApp: true, sendToKindleApp: true, appleMailDefault: true }),
    );
    expect(list).toStrictEqual([
      {
        id: 'device:kindle#/Volumes/Kindle',
        key: 'device:kindle',
        title: 'Kindle',
        detail: DEVICE_DETAIL,
        button: 'Copy to Kindle',
        available: true,
        device: { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' },
      },
      {
        id: 'remarkable',
        key: 'remarkable',
        title: 'reMarkable',
        detail: 'the EPUB, over its USB connection',
        button: 'Upload to reMarkable',
        available: true,
        device: { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null },
      },
      {
        id: 'apple-books',
        key: 'apple-books',
        title: 'Apple Books',
        detail: 'syncs to your iPhone and iPad',
        button: 'Add to Apple Books',
        available: true,
      },
      {
        id: 'send-to-kindle',
        key: 'send-to-kindle',
        title: 'Send to Kindle app',
        detail: 'via Amazon, the best-looking Kindle result',
        button: 'Send to Kindle app',
        available: true,
      },
      {
        id: 'email-to-kindle',
        key: 'email-to-kindle',
        title: 'Send to Kindle email',
        detail: 'a Mail message with the book attached',
        button: 'Send to Kindle email',
        available: true,
      },
      SAVE_EPUB,
      SAVE_KINDLE,
      placeholder('kobo', 'Kobo'),
      placeholder('tolino', 'tolino'),
    ]);
  });

  test('a bare Mac without Apple Mail: placeholders in order, email last with its setup fix', () => {
    expect(routes(facts({ booksApp: false }))).toStrictEqual([
      {
        id: 'send-to-kindle',
        key: 'send-to-kindle',
        title: 'Send to Kindle web',
        detail: 'via Amazon, the best-looking Kindle result',
        button: 'Send to Kindle web',
        available: true,
      },
      SAVE_EPUB,
      SAVE_KINDLE,
      placeholder('kindle', 'Kindle'),
      placeholder('kobo', 'Kobo'),
      placeholder('tolino', 'tolino'),
      {
        id: 'remarkable',
        key: 'remarkable',
        title: 'reMarkable',
        detail: 'dock over USB to send',
        button: 'Upload to reMarkable',
        available: false,
        unavailable: 'connect',
      },
      {
        id: 'email-to-kindle',
        key: 'email-to-kindle',
        title: 'Send to Kindle email',
        detail: 'needs Apple Mail as the default mail app, the one mail app the attachment survives',
        button: 'Send to Kindle email',
        available: false,
        unavailable: 'setup',
      },
    ]);
  });

  const APPLE_BOOKS_OFF_MAC: Route = {
    id: 'apple-books',
    key: 'apple-books',
    title: 'Apple Books',
    detail: 'on a Mac only',
    button: 'Add to Apple Books',
    available: false,
    unavailable: 'platform',
  };
  const EMAIL_OFF_MAC: Route = {
    id: 'email-to-kindle',
    key: 'email-to-kindle',
    title: 'Send to Kindle email',
    detail: 'on a Mac only; save the EPUB and attach it yourself',
    button: 'Send to Kindle email',
    available: false,
    unavailable: 'platform',
  };

  test('Windows with nothing plugged in: tolino cannot be found there at all', () => {
    expect(routes(facts({ platform: 'win32', booksApp: false }))).toStrictEqual([
      {
        id: 'send-to-kindle',
        key: 'send-to-kindle',
        title: 'Send to Kindle web',
        detail: 'via Amazon, the best-looking Kindle result',
        button: 'Send to Kindle web',
        available: true,
      },
      SAVE_EPUB,
      SAVE_KINDLE,
      placeholder('kindle', 'Kindle'),
      placeholder('kobo', 'Kobo'),
      {
        id: 'device:tolino',
        key: 'device:tolino',
        title: 'tolino',
        detail: 'cannot be found on Windows: a Windows drive carries no volume name',
        button: 'Copy to tolino',
        available: false,
        unavailable: 'platform',
      },
      {
        id: 'remarkable',
        key: 'remarkable',
        title: 'reMarkable',
        detail: 'dock over USB to send',
        button: 'Upload to reMarkable',
        available: false,
        unavailable: 'connect',
      },
      APPLE_BOOKS_OFF_MAC,
      EMAIL_OFF_MAC,
    ]);
  });

  test('Linux with a Kobo plugged in', () => {
    expect(keys(routes(facts({ platform: 'linux', devices: [kobo] })))).toEqual([
      'device:kobo',
      'send-to-kindle',
      'save-epub',
      'save-kindle',
      'device:kindle',
      'device:tolino',
      'remarkable',
      'apple-books',
      'email-to-kindle',
    ]);
  });
});

describe('three kinds of unavailable, on the right rows', () => {
  test('a Mac without Apple Mail as the default: email is setup', () => {
    expect(find(routes(facts({ appleMailDefault: false })), 'email-to-kindle')?.unavailable).toBe(
      'setup',
    );
  });

  for (const platform of ['linux', 'win32']) {
    test(`${platform}: Apple Books and email are platform, whatever the probes said`, () => {
      // Facts claiming the Mac-only apps are present must not make a Mac-only
      // route available elsewhere: the catalog owns the platform rule.
      for (const on of [false, true]) {
        const list = routes(
          facts({ platform, booksApp: on, appleMailDefault: on, sendToKindleApp: on }),
        );
        const books = find(list, 'apple-books');
        const email = find(list, 'email-to-kindle');
        expect(books?.available).toBe(false);
        expect(books?.unavailable).toBe('platform');
        expect(books?.detail).toBe('on a Mac only');
        expect(email?.available).toBe(false);
        expect(email?.unavailable).toBe('platform');
        expect(email?.detail).toBe('on a Mac only; save the EPUB and attach it yourself');
        // Amazon's Mac app is not Windows' route: off a Mac it is the web.
        expect(find(list, 'send-to-kindle')?.title).toBe('Send to Kindle web');
      }
    });

    test(`${platform}: with nothing connected, Send to Kindle leads`, () => {
      expect(routes(facts({ platform }))[0]!.key).toBe('send-to-kindle');
    });
  }

  test('an unplugged Kindle is connect', () => {
    for (const platform of ['darwin', 'linux', 'win32']) {
      const row = find(routes(facts({ platform })), 'device:kindle');
      expect(row?.available).toBe(false);
      expect(row?.unavailable).toBe('connect');
    }
  });

  test('tolino is platform on win32 and connect everywhere else', () => {
    expect(find(routes(facts({ platform: 'win32' })), 'device:tolino')?.unavailable).toBe(
      'platform',
    );
    expect(find(routes(facts({ platform: 'darwin' })), 'device:tolino')?.unavailable).toBe(
      'connect',
    );
    expect(find(routes(facts({ platform: 'linux' })), 'device:tolino')?.unavailable).toBe(
      'connect',
    );
  });

  test('a tolino that is somehow connected on win32 is a device row, not a placeholder', () => {
    const list = routes(facts({ platform: 'win32', devices: [tolino] }));
    expect(list.filter((r) => r.key === 'device:tolino')).toHaveLength(1);
    expect(find(list, 'device:tolino')?.available).toBe(true);
  });

  test('an undocked reMarkable is connect', () => {
    const row = find(routes(facts()), 'remarkable');
    expect(row?.available).toBe(false);
    expect(row?.unavailable).toBe('connect');
  });

  test('a reMarkable not looked for yet (routes --quick) says so, rather than telling you to dock it', () => {
    // The quick answer skips the probe so the send panel can draw at once; a
    // tablet that IS docked must not be told to dock while the full answer
    // is still on its way. Still dimmed, still connect: nothing to press yet.
    const row = find(routes(facts({ remarkableChecked: false })), 'remarkable');
    expect(row).toEqual({
      id: 'remarkable',
      key: 'remarkable',
      title: 'reMarkable',
      detail: 'checking whether one is docked…',
      button: 'Upload to reMarkable',
      available: false,
      unavailable: 'connect',
    });
    expect(find(routes(facts({ remarkableChecked: true })), 'remarkable')?.detail).toBe('dock over USB to send');
    expect(find(routes(facts()), 'remarkable')?.detail).toBe('dock over USB to send');
  });
});

describe('row identity and the device a row carries', () => {
  test('a connected device row carries the id `send --device` accepts', () => {
    const row = routes(facts({ devices: [kobo] }))[0]!;
    const summary: DeviceSummary | undefined = row.device;
    expect(summary).toStrictEqual({
      id: '/Volumes/KOBOeReader',
      kind: 'kobo',
      name: 'KOBOeReader',
      volume: '/Volumes/KOBOeReader',
    });
    expect(row.title).toBe('KOBOeReader');
    expect(row.button).toBe('Copy to KOBOeReader');
  });

  test('a device with a blank name is named by its kind: title, button and the device it carries', () => {
    // A drive with no label (Windows reads that as ''), or one of spaces,
    // would otherwise be a row with no title, and the window rejects a
    // blank title by rejecting the WHOLE list, save rows and all.
    const cases: [ConnectedDevice, string][] = [
      [{ kind: 'kindle', name: '', volume: 'E:\\' }, 'Kindle'],
      [{ kind: 'kobo', name: '   ', volume: '/Volumes/   ' }, 'Kobo'],
      [{ kind: 'tolino', name: '\t\n', volume: '/media/x' }, 'tolino'],
    ];
    for (const [device, name] of cases) {
      const row = routes(facts({ devices: [device] }))[0]!;
      expect(row.title).toBe(name);
      expect(row.button).toBe(`Copy to ${name}`);
      expect(row.device!.name).toBe(name);
      // Only the words change: the row still addresses that volume.
      expect(row.device!.id).toBe(device.volume!);
      expect(row.id).toBe(`device:${device.kind}#${device.volume}`);
    }
  });

  test('a docked reMarkable with a blank name carries its kind name too', () => {
    const blank: ConnectedDevice = { kind: 'remarkable', name: ' ', volume: null };
    const row = routes(facts({ devices: [blank] })).find((r) => r.key === 'remarkable')!;
    expect(row.title).toBe('reMarkable');
    expect(row.device!.name).toBe('reMarkable');
  });

  test('a name that is not blank is kept exactly as the device gave it', () => {
    const spaced: ConnectedDevice = { kind: 'kindle', name: ' My Kindle ', volume: '/Volumes/ My Kindle ' };
    const row = routes(facts({ devices: [spaced] }))[0]!;
    expect(row.title).toBe(' My Kindle ');
    expect(row.button).toBe('Copy to  My Kindle ');
    expect(row.device!.name).toBe(' My Kindle ');
  });

  test('a device with no volume falls back to its id for the row id', () => {
    const odd: ConnectedDevice = { kind: 'kobo', name: 'Kobo', volume: null };
    expect(routes(facts({ devices: [odd] }))[0]!.id).toBe('device:kobo#kobo');
  });

  test('placeholders carry no device', () => {
    for (const r of routes(facts()).filter((x) => !x.available)) {
      expect(r.device).toBeUndefined();
    }
  });

  test('two Kindles are two rows, in the order the devices arrived', () => {
    const list = routes(facts({ devices: [kindle, kindleTwin] }));
    expect(list.slice(0, 2).map((r) => r.id)).toEqual([
      'device:kindle#/Volumes/Kindle',
      'device:kindle#/Volumes/KINDLE2',
    ]);
    expect(list.filter((r) => r.key === 'device:kindle')).toHaveLength(2);
  });

  test('DEVICE_KINDS is every volume-mounted kind types.ts knows, in its order', () => {
    const fromTypes = (Object.keys(DEVICE_DISPLAY_NAMES) as DeviceKind[]).filter(
      (k) => k !== 'remarkable',
    );
    expect([...DEVICE_KINDS]).toEqual(fromTypes);
    expect([...DEVICE_KINDS]).toEqual(['kindle', 'kobo', 'tolino']);
  });
});

describe('preselected', () => {
  const twins = routes(facts({ devices: [kindle, kindleTwin] }));

  test('a remembered kind picks the FIRST row of that kind', () => {
    expect(preselected(twins, 'device:kindle').id).toBe('device:kindle#/Volumes/Kindle');
  });

  test('a remembered key matches keys, never row ids', () => {
    // An id is not a key: a stored volume path must not pick a row.
    expect(preselected(twins, 'device:kindle#/Volumes/KINDLE2').id).toBe(
      'device:kindle#/Volumes/Kindle',
    );
  });

  test('an unknown remembered key falls back to the first available row', () => {
    const list = routes(facts({ booksApp: false }));
    expect(preselected(list, 'saveCopy').key).toBe('send-to-kindle');
  });

  test('a remembered save row is chosen over a connected device', () => {
    const list = routes(facts({ devices: [kindle] }));
    expect(preselected(list, 'save-kindle').key).toBe('save-kindle');
    expect(preselected(list, 'save-epub').key).toBe('save-epub');
  });

  // A route that can never work on this system is structurally absent, as a
  // Mac without Books.app is: remembering it must not strand the person on a
  // dimmed row. It falls back exactly as a route that is not listed does.
  // connect and setup are states a person can change, so those stay chosen.
  for (const platform of ['linux', 'win32']) {
    test(`a remembered Apple Books on ${platform} falls back to the first available row`, () => {
      const list = routes(facts({ platform }));
      const chosen = preselected(list, 'apple-books');
      expect(chosen.key).toBe('send-to-kindle');
      expect(chosen.available).toBe(true);
    });

    test(`a remembered email route on ${platform} falls back too`, () => {
      expect(preselected(routes(facts({ platform })), 'email-to-kindle').key).toBe('send-to-kindle');
    });
  }

  test('a remembered Apple Books on linux with a Kindle plugged in falls back to the Kindle', () => {
    const list = routes(facts({ platform: 'linux', devices: [kindle] }));
    expect(preselected(list, 'apple-books').id).toBe('device:kindle#/Volumes/Kindle');
  });

  test('a remembered tolino on win32 falls back: it cannot be found there', () => {
    const chosen = preselected(routes(facts({ platform: 'win32' })), 'device:tolino');
    expect(chosen.key).toBe('send-to-kindle');
    expect(chosen.available).toBe(true);
  });

  test('a remembered tolino on darwin stays chosen while unplugged (connect)', () => {
    const chosen = preselected(routes(facts({ platform: 'darwin' })), 'device:tolino');
    expect(chosen.key).toBe('device:tolino');
    expect(chosen.unavailable).toBe('connect');
  });

  test('a remembered Kobo on darwin stays chosen while unplugged (connect)', () => {
    const chosen = preselected(routes(facts({ platform: 'darwin' })), 'device:kobo');
    expect(chosen.key).toBe('device:kobo');
    expect(chosen.available).toBe(false);
    expect(chosen.unavailable).toBe('connect');
  });

  test('a remembered email route on darwin without Apple Mail stays chosen (setup)', () => {
    const chosen = preselected(routes(facts({ appleMailDefault: false })), 'email-to-kindle');
    expect(chosen.key).toBe('email-to-kindle');
    expect(chosen.available).toBe(false);
    expect(chosen.unavailable).toBe('setup');
  });

  test('across the platforms, a remembered row is kept unless it is platform-unavailable', () => {
    for (const platform of ['darwin', 'linux', 'win32']) {
      const list = routes(facts({ platform, booksApp: true }));
      for (const r of list) {
        const chosen = preselected(list, r.key);
        if (r.unavailable === 'platform') {
          expect({ platform, key: r.key, chosen: chosen.key, available: chosen.available }).toEqual({
            platform,
            key: r.key,
            chosen: list.find((x) => x.available)!.key,
            available: true,
          });
        } else {
          expect({ platform, key: r.key, chosen: chosen.key }).toEqual({ platform, key: r.key, chosen: r.key });
        }
      }
    }
  });

  test('with no available row at all, the first row', () => {
    const allDim = routes(facts()).map((r) => ({ ...r, available: false }));
    expect(preselected(allDim, undefined)).toBe(allDim[0]!);
  });

  test('the first AVAILABLE row, not merely the first, when nothing is remembered', () => {
    const list = routes(facts());
    const reordered = [...list.filter((r) => !r.available), ...list.filter((r) => r.available)];
    expect(preselected(reordered, undefined).key).toBe('apple-books');
  });
});

describe('the whole matrix', () => {
  const kindleSets: ConnectedDevice[][] = [[], [kindle], [kindle, kindleTwin]];
  const otherSets: ConnectedDevice[][] = [[], [kobo, tolino]];
  const cases: { label: string; facts: RouteFacts }[] = [];
  for (const platform of ['darwin', 'linux', 'win32']) {
    for (const booksApp of [false, true]) {
      for (const sendToKindleApp of [false, true]) {
        for (const appleMailDefault of [false, true]) {
          for (const kindles of kindleSets) {
            for (const others of otherSets) {
              for (const docked of [false, true]) {
                const devices = [...kindles, ...others, ...(docked ? [rm] : [])];
                cases.push({
                  label: `${platform} books:${booksApp} app:${sendToKindleApp} mail:${appleMailDefault} devices:${devices.map((d) => d.name).join('+') || 'none'}`,
                  facts: { platform, devices, booksApp, sendToKindleApp, appleMailDefault },
                });
              }
            }
          }
        }
      }
    }
  }

  test(`covers ${cases.length} combinations`, () => {
    expect(cases.length).toBe(3 * 2 * 2 * 2 * 3 * 2 * 2);
  });

  test('ids are unique in every list', () => {
    for (const c of cases) {
      const list = routes(c.facts);
      expect({ label: c.label, unique: new Set(list.map((r) => r.id)).size }).toEqual({
        label: c.label,
        unique: list.length,
      });
    }
  });

  test('no em dash in any title, detail or button', () => {
    for (const c of cases) {
      for (const r of routes(c.facts)) {
        for (const text of [r.title, r.detail, r.button]) {
          expect({ label: c.label, text, dash: text.includes(EM_DASH) }).toEqual({
            label: c.label,
            text,
            dash: false,
          });
        }
      }
    }
  });

  test('every unavailable row says why and how; every available row carries no reason', () => {
    for (const c of cases) {
      for (const r of routes(c.facts)) {
        if (r.available) {
          expect(r.unavailable).toBeUndefined();
          expect(Object.hasOwn(r, 'unavailable')).toBe(false);
        } else {
          expect(['connect', 'platform', 'setup']).toContain(r.unavailable!);
          expect(r.detail.trim().length).toBeGreaterThan(0);
        }
        expect(r.title.length).toBeGreaterThan(0);
        expect(r.button.length).toBeGreaterThan(0);
      }
    }
  });

  test('both save rows are always present and available (the floor)', () => {
    for (const c of cases) {
      const list = routes(c.facts);
      expect(find(list, 'save-epub')?.available).toBe(true);
      expect(find(list, 'save-kindle')?.available).toBe(true);
    }
  });

  test('available rows always come first, and the first row is always sendable', () => {
    for (const c of cases) {
      const list = routes(c.facts);
      const firstDim = list.findIndex((r) => !r.available);
      const tail = firstDim === -1 ? [] : list.slice(firstDim);
      expect(list[0]!.available).toBe(true);
      expect(tail.every((r) => !r.available)).toBe(true);
      expect(preselected(list, undefined).available).toBe(true);
    }
  });

  test('every physical destination is listed exactly as often as it should be', () => {
    for (const c of cases) {
      const list = routes(c.facts);
      for (const kind of DEVICE_KINDS) {
        const connected = c.facts.devices.filter((d) => d.kind === kind).length;
        const rows = list.filter((r) => r.key === `device:${kind}`);
        // A connected kind never also gets a placeholder; an unconnected one
        // gets exactly one.
        expect({ label: c.label, kind, rows: rows.length }).toEqual({
          label: c.label,
          kind,
          rows: Math.max(connected, 1),
        });
        expect(rows.every((r) => r.available === connected > 0)).toBe(true);
      }
      expect(list.filter((r) => r.key === 'remarkable')).toHaveLength(1);
      expect(list.some((r) => r.key === 'device:remarkable')).toBe(false);
    }
  });
});
