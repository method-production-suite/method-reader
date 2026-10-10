// The route decisions behind the send panel (parity piece B, window plan
// Task W2): the pure functions in send-routes.js (moved out of the old Send
// page's send.js unchanged) that turn the engine's `routes --json` answer
// into rows, buttons, save-dialog defaults and status lines. The panel that
// draws them is driven in send-panel-ui.test.ts; the mounted half that used
// to live here, against the Send page, was re-homed there when the page went.
//
// Its own file rather than a block in desktop-ui.test.ts only so parallel
// work on that file cannot collide with this one; send-routes.js is imported
// the same dynamic way desktop-ui.test.ts imports the window's modules.
//
// The test that matters most is the round trip: the validator is checked
// against the ENGINE's real catalog (src/export/routes.ts) for every fact
// combination worth naming, so a validator stricter than the engine (the
// dangerous direction: the panel would show a failure line instead of
// routes) cannot pass on hand-written fixtures alone.
import { beforeAll, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import type { ConnectedDevice } from '../src/device/types';
import { preselected, routes, type Route, type RouteFacts } from '../src/export/routes';

const UI = join(new URL('..', import.meta.url).pathname, 'desktop', 'ui');

const EM_DASH = '\u2014';

type RouteDevice = { id: string; kind: string; name: string; volume: string | null };
type Status = { line: string; bad: boolean };
type RoutesModule = {
  NO_MESSAGE: string;
  NO_NOTE: string;
  NO_ROUTES: string;
  NO_EXTENSION: string;
  SETUP_BUTTON: string;
  SETUP_TITLE: string;
  connectedDevices: (shown: unknown) => RouteDevice[] | null;
  sameRoutes: (a: unknown, b: unknown) => boolean;
  performerFor: (route: unknown) => string | null;
  buttonLabel: (route: unknown, file?: unknown) => string;
  routeLines: (route: unknown, platform: unknown) =>
    { title: string; detail: string; where: string | null };
  kindleFileFrom: (built: unknown) => [string, Record<string, unknown>];
  kindleCheckFrom: (answer: unknown) =>
    { extension: string; fresh: boolean; builtBy: string; bookDate: number | null; beta: boolean } | null;
  handsOverKindleFile: (route: unknown) => boolean;
  buildsAhead: (file: unknown, opts?: {
    building?: boolean; busy?: boolean; triedFor?: string | null; forKindle?: boolean;
  }) => boolean;
  mayBuild: (file: unknown) => boolean;
  buildLine: (extension: unknown) => string;
  preparingPhase: (device: unknown, file?: unknown) => string;
  kindleFileIsBeta: (route: unknown, file: unknown) => boolean;
  routesFailure: (answer: unknown) => string;
  whereLine: (device: unknown) => string;
  routesFrom: (answer: unknown) => { routes: Route[]; chosen: string } | null;
  isDeviceRoute: (route: unknown) => boolean;
  saveNameFor: (epubPath: unknown, extension: string) => string;
  baseName: (path: unknown) => string;
  saveFiltersFor: (extension: string, label: unknown) => { name: string; extensions: string[] }[];
  routeNoteFrom: (answer: unknown) => [string, { detail: string }];
  emailSetupHint: (route: unknown) => { line: string; key: string } | null;
  statusFor: (
    phase: string,
    opts?: { device?: unknown; detail?: string; route?: unknown },
  ) => Status;
};

let send: RoutesModule;
beforeAll(async () => { send = (await import(join(UI, 'send-routes.js'))) as RoutesModule; });

// ------------------------------------------------------------------ facts

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const kindleTwin: ConnectedDevice = { kind: 'kindle', name: 'KINDLE2', volume: '/Volumes/KINDLE2' };
const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const tolino: ConnectedDevice = { kind: 'tolino', name: 'tolino', volume: '/Volumes/tolino' };
const rm: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };

/** Every fact combination the plan names: three platforms; Books, Amazon's
 *  app and the mail default each on and off; 0, 1 or 2 Kindles; a docked
 *  reMarkable or not. Plus one set of other volume readers, so the rows for
 *  kinds other than Kindle are walked connected as well as dimmed. */
function* everyFacts(): Generator<RouteFacts> {
  const deviceSets: ConnectedDevice[][] = [[], [kindle], [kindle, kindleTwin], [kobo, tolino]];
  for (const platform of ['darwin', 'win32', 'linux']) {
    for (const booksApp of [true, false]) {
      for (const sendToKindleApp of [true, false]) {
        for (const appleMailDefault of [true, false]) {
          for (const set of deviceSets) {
            for (const docked of [false, true]) {
              yield {
                platform,
                booksApp,
                sendToKindleApp,
                appleMailDefault,
                devices: docked ? [...set, rm] : [...set],
              };
            }
          }
        }
      }
    }
  }
}

/** Every remembered route worth trying for one list: nothing remembered,
 *  each key the list carries (so a platform-unavailable one falls back and
 *  a connect/setup one stays chosen), and a key no route carries. */
const remembered = (list: Route[]) => [undefined, 'gone', ...new Set(list.map((r) => r.key))];

/** The engine's answer as `routes --json` would print it. */
const answerFor = (list: Route[], lastRoute: string | undefined) =>
  ({ ok: true, routes: list, chosen: preselected(list, lastRoute).id });

/** One well-formed answer to break, field by field. */
const sample = () => {
  const list = routes({
    platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: false,
    devices: [kindle, rm],
  });
  return structuredClone({ ok: true, routes: list, chosen: list[0]!.id }) as unknown as {
    ok: unknown; routes: Record<string, unknown>[]; chosen: unknown;
  };
};
const at = (answer: ReturnType<typeof sample>, key: string) => {
  const i = answer.routes.findIndex((r) => r.key === key);
  expect(i, `sample has no ${key} row`).toBeGreaterThan(-1);
  return answer.routes[i]!;
};

// ------------------------------------------------------------ routesFrom

describe('routesFrom: the engine’s route list, taken whole or not at all', () => {
  test('every answer the real engine can give comes back exactly as it went in', () => {
    // The dangerous direction for a strict validator is TOO strict: one
    // rejected shape and the send panel shows a failure line where every route
    // should be. So the validator is run over the engine's own catalog for
    // every fact combination and every remembered choice, both as objects and
    // through JSON (what runEngine actually hands the window).
    let checked = 0;
    for (const facts of everyFacts()) {
      const list = routes(facts);
      for (const last of remembered(list)) {
        const answer = answerFor(list, last);
        const expected = { routes: list, chosen: answer.chosen };
        expect(send.routesFrom(answer)).toStrictEqual(expected);
        expect(send.routesFrom(JSON.parse(JSON.stringify(answer)))).toStrictEqual(expected);
        checked += 1;
      }
    }
    // The matrix really ran: 3 platforms x 8 app and mail combinations x 4
    // device sets x docked or not is 192 fact sets, each with several
    // remembered choices.
    expect(checked).toBeGreaterThan(192 * 3);
  });

  test('a connected device with a blank name still round-trips: the list is drawn, not refused', () => {
    // A drive with no label reaches the engine as '' (or spaces). The
    // validator refuses a blank title or device name by refusing the WHOLE
    // list, so the engine has to name such a row itself, by its kind.
    const blanks: ConnectedDevice[] = [
      { kind: 'kindle', name: '', volume: 'E:\\' },
      { kind: 'kobo', name: '   ', volume: 'F:\\' },
      { kind: 'remarkable', name: ' ', volume: null },
    ];
    for (const platform of ['darwin', 'win32', 'linux']) {
      const list = routes({
        platform, booksApp: true, sendToKindleApp: false, appleMailDefault: false, devices: blanks,
      });
      for (const last of remembered(list)) {
        const answer = answerFor(list, last);
        const got = send.routesFrom(JSON.parse(JSON.stringify(answer)));
        expect(got, `${platform}, remembered ${String(last)}`).toStrictEqual({ routes: list, chosen: answer.chosen });
      }
      const titles = list.filter((r) => r.device !== undefined).map((r) => r.title);
      expect(titles).toEqual(['Kindle', 'Kobo', 'reMarkable']);
    }
  });

  test('the copies are clean: rebuilt, never the engine’s objects by reference', () => {
    const answer = sample();
    const extra = structuredClone(answer) as Record<string, unknown> & typeof answer;
    extra.stray = 'kept by accident';
    extra.routes[0]!.stray = 'rides along';
    (extra.routes[0]!.device as Record<string, unknown>).serial = 'G000';
    const got = send.routesFrom(extra)!;
    expect(got).not.toBe(null);
    expect(Object.keys(got).sort()).toEqual(['chosen', 'routes']);
    expect(got.routes[0]).not.toHaveProperty('stray');
    expect(got.routes[0]!.device).not.toHaveProperty('serial');
    expect(got.routes).not.toBe(extra.routes);
    expect(got.routes[0]).not.toBe(extra.routes[0]);
    expect(got.routes[0]!.device).not.toBe(extra.routes[0]!.device);
    // Absent stays absent: no `unavailable: undefined` or `device: undefined`
    // appears on a row that never had one.
    const books = got.routes.find((r) => r.key === 'apple-books')!;
    expect(Object.keys(books).sort()).toEqual(['available', 'button', 'detail', 'id', 'key', 'title']);
  });

  test('the engine’s words are kept as it wrote them', () => {
    // Unavailable rows show the engine's fix as is; the validator checks
    // that a field is text, it does not tidy it.
    const answer = sample();
    at(answer, 'device:kobo').detail = '  plug in over USB to send  ';
    const got = send.routesFrom(answer)!;
    expect(got.routes.find((r) => r.key === 'device:kobo')!.detail)
      .toBe('  plug in over USB to send  ');
  });

  test('anything that is not an ok answer is null', () => {
    for (const answer of [null, undefined, 'routes', 7, [], true]) {
      expect(send.routesFrom(answer)).toBe(null);
    }
    for (const ok of [false, 'true', 1, undefined, null]) {
      expect(`ok ${String(ok)}: ${send.routesFrom({ ...sample(), ok })}`).toBe(`ok ${String(ok)}: null`);
    }
    // A refusal is a refusal, whatever else it carries.
    expect(send.routesFrom({ ok: false, error: { code: 'x', message: 'y' } })).toBe(null);
  });

  test('the list itself must be a list, with something in it', () => {
    for (const list of [undefined, null, 'rows', {}, 7]) {
      expect(send.routesFrom({ ...sample(), routes: list })).toBe(null);
    }
    // An empty list has nothing `chosen` can name.
    expect(send.routesFrom({ ok: true, routes: [], chosen: 'save-epub' })).toBe(null);
    for (const entry of [null, 'save-epub', 7, ['save-epub']]) {
      const answer = sample();
      (answer.routes as unknown[]).splice(1, 0, entry);
      expect(`entry ${JSON.stringify(entry)}: ${send.routesFrom(answer)}`)
        .toBe(`entry ${JSON.stringify(entry)}: null`);
    }
  });

  test('every text field is text, on every row', () => {
    for (const field of ['id', 'key', 'title', 'detail', 'button']) {
      for (const bad of [undefined, null, 7, '', '   ', ['x']]) {
        for (const key of ['device:kindle', 'apple-books', 'device:kobo']) {
          const answer = sample();
          const row = at(answer, key);
          if (bad === undefined) delete row[field];
          else row[field] = bad;
          // `chosen` names the Kindle row, so a broken id on the other two
          // rows is caught by the id check alone, not by the chosen one.
          const got = send.routesFrom(answer);
          expect(`${key}.${field} = ${JSON.stringify(bad)}: ${got}`)
            .toBe(`${key}.${field} = ${JSON.stringify(bad)}: null`);
        }
      }
    }
  });

  test('available is a boolean, and unavailable is present exactly when it is false', () => {
    for (const bad of ['true', 1, 0, null, undefined]) {
      const answer = sample();
      const row = at(answer, 'apple-books');
      if (bad === undefined) delete row.available;
      else row.available = bad;
      expect(`available = ${String(bad)}: ${send.routesFrom(answer)}`)
        .toBe(`available = ${String(bad)}: null`);
    }
    // Present on an available row: the engine's rule is broken.
    for (const why of ['connect', 'platform', 'setup', null]) {
      const answer = sample();
      at(answer, 'apple-books').unavailable = why;
      expect(`available row with unavailable ${String(why)}: ${send.routesFrom(answer)}`)
        .toBe(`available row with unavailable ${String(why)}: null`);
    }
    // Missing on an unavailable row.
    const missing = sample();
    delete at(missing, 'device:kobo').unavailable;
    expect(send.routesFrom(missing)).toBe(null);
    // A kind nobody defined.
    for (const why of ['broken', '', 'Connect', 7, null]) {
      const answer = sample();
      at(answer, 'device:kobo').unavailable = why;
      expect(`unavailable = ${String(why)}: ${send.routesFrom(answer)}`)
        .toBe(`unavailable = ${String(why)}: null`);
    }
    // And each real kind is accepted on an unavailable row.
    for (const why of ['connect', 'platform', 'setup']) {
      const answer = sample();
      at(answer, 'device:kobo').unavailable = why;
      expect(send.routesFrom(answer)).not.toBe(null);
    }
  });

  test('chosen names a row that is listed, by its id', () => {
    for (const chosen of [undefined, null, 7, '', 'gone', ['save-epub']]) {
      expect(`chosen ${JSON.stringify(chosen)}: ${send.routesFrom({ ...sample(), chosen })}`)
        .toBe(`chosen ${JSON.stringify(chosen)}: null`);
    }
    // A connected Kindle's id carries its volume. Its bare key is not an id
    // on this list, and a window that accepted it would brass nothing.
    const answer = sample();
    expect(at(answer, 'device:kindle').id).not.toBe('device:kindle');
    expect(send.routesFrom({ ...answer, chosen: 'device:kindle' })).toBe(null);
    // An unavailable row can be chosen: an unplugged Kindle you chose last
    // time stays chosen and its button waits.
    expect(send.routesFrom({ ...answer, chosen: 'device:kobo' })!.chosen).toBe('device:kobo');
  });

  test('a row or a device is an object, not a list that happens to carry the fields', () => {
    // JSON cannot make one, but the validator rebuilds from whatever it is
    // handed, and an array is not the shape the engine promises.
    const rowAsList = sample();
    const books = at(rowAsList, 'apple-books');
    rowAsList.routes[rowAsList.routes.indexOf(books)] = Object.assign([], books);
    expect(send.routesFrom(rowAsList)).toBe(null);
    const deviceAsList = sample();
    const row = at(deviceAsList, 'device:kindle');
    row.device = Object.assign([], row.device);
    expect(send.routesFrom(deviceAsList)).toBe(null);
  });

  test('two rows with one id are one row too many', () => {
    // Ids are what `chosen` names and what the page tells rows apart by;
    // a repeated one makes both ambiguous.
    const answer = sample();
    at(answer, 'save-kindle').id = 'save-epub';
    expect(send.routesFrom(answer)).toBe(null);
  });

  test('a device is well formed, and rides exactly on the rows that send to one', () => {
    for (const bad of [null, 'Kindle', 7, ['Kindle']]) {
      const answer = sample();
      at(answer, 'device:kindle').device = bad;
      expect(`device = ${JSON.stringify(bad)}: ${send.routesFrom(answer)}`)
        .toBe(`device = ${JSON.stringify(bad)}: null`);
    }
    for (const field of ['id', 'kind', 'name']) {
      for (const bad of [undefined, null, 7, '', '  ']) {
        const answer = sample();
        const device = at(answer, 'device:kindle').device as Record<string, unknown>;
        if (bad === undefined) delete device[field];
        else device[field] = bad;
        expect(`device.${field} = ${JSON.stringify(bad)}: ${send.routesFrom(answer)}`)
          .toBe(`device.${field} = ${JSON.stringify(bad)}: null`);
      }
    }
    // volume is a string or null, and it is there: JSON keeps a null.
    for (const bad of [undefined, 7, false, {}]) {
      const answer = sample();
      const device = at(answer, 'device:kindle').device as Record<string, unknown>;
      if (bad === undefined) delete device.volume;
      else device.volume = bad;
      expect(`device.volume = ${JSON.stringify(bad)}: ${send.routesFrom(answer)}`)
        .toBe(`device.volume = ${JSON.stringify(bad)}: null`);
    }
    // A docked reMarkable has no volume, and that is fine.
    expect((at(sample(), 'remarkable').device as RouteDevice).volume).toBe(null);
    expect(send.routesFrom(sample())).not.toBe(null);

    // An available device row with no device has nothing to send to: its
    // button would be a dead end.
    const bare = sample();
    delete at(bare, 'device:kindle').device;
    expect(send.routesFrom(bare)).toBe(null);
    const bareRm = sample();
    delete at(bareRm, 'remarkable').device;
    expect(send.routesFrom(bareRm)).toBe(null);
    // A device on a row that does not send to one, or on a dimmed one.
    const onBooks = sample();
    at(onBooks, 'apple-books').device = structuredClone(at(onBooks, 'device:kindle').device);
    expect(send.routesFrom(onBooks)).toBe(null);
    const onDimmed = sample();
    at(onDimmed, 'device:kobo').device = { id: '/Volumes/KOBO', kind: 'kobo', name: 'Kobo', volume: '/Volumes/KOBO' };
    expect(send.routesFrom(onDimmed)).toBe(null);
  });
});

// --------------------------------------------------------- isDeviceRoute

describe('isDeviceRoute: which rows go through export then send', () => {
  test('a volume reader of any kind, and a reMarkable', () => {
    for (const key of ['device:kindle', 'device:kobo', 'device:tolino', 'device:someday', 'remarkable']) {
      expect(`${key}: ${send.isDeviceRoute({ key })}`).toBe(`${key}: true`);
    }
  });

  test('never an app, the email, a save, or something that only looks close', () => {
    for (const key of [
      'apple-books', 'send-to-kindle', 'email-to-kindle', 'save-epub', 'save-kindle',
      'device', 'devices:kindle', 'xdevice:kindle', 'remarkable2', 'reMarkable', '',
    ]) {
      expect(`${key}: ${send.isDeviceRoute({ key })}`).toBe(`${key}: false`);
    }
    for (const route of [null, undefined, {}, { key: 7 }, 'device:kindle']) {
      expect(send.isDeviceRoute(route)).toBe(false);
    }
  });

  test('it agrees with the engine: every available device row carries a device', () => {
    for (const facts of everyFacts()) {
      for (const route of routes(facts)) {
        if (!route.available) continue;
        expect(`${route.id}: ${send.isDeviceRoute(route)}`)
          .toBe(`${route.id}: ${route.device !== undefined}`);
      }
    }
  });
});

// ----------------------------------------------------- the save dialog

describe('saveNameFor and saveFiltersFor: what the save dialog starts with', () => {
  test('the book’s own name, with the extension of the file being saved', () => {
    expect(send.saveNameFor('/Users/sam/Screepub/Library/My Script.epub', 'epub')).toBe('My Script.epub');
    expect(send.saveNameFor('/Users/sam/Screepub/Library/My Script.epub', 'kfx')).toBe('My Script.kfx');
    expect(send.saveNameFor('/lib/draft.epub', 'azw3')).toBe('draft.azw3');
    expect(send.saveNameFor('/lib/draft.epub', 'mobi')).toBe('draft.mobi');
  });

  test('the stem comes from the file name, never the folder', () => {
    expect(send.saveNameFor('/lib/v1.2/script.epub', 'kfx')).toBe('script.kfx');
    expect(send.saveNameFor('/lib/v1.2/script', 'epub')).toBe('script.epub');
    // Windows paths, because the window runs there too.
    expect(send.saveNameFor('C:\\Users\\sam\\Screepub\\Draft Two.epub', 'azw3')).toBe('Draft Two.azw3');
  });

  test('only the last extension goes; a dotted title keeps its dots', () => {
    expect(send.saveNameFor('/lib/Mr. Smith Goes.epub', 'epub')).toBe('Mr. Smith Goes.epub');
    expect(send.saveNameFor('/lib/draft.v2.epub', 'kfx')).toBe('draft.v2.kfx');
  });

  test('an extension handed over with its dot does not get a second one', () => {
    expect(send.saveNameFor('/lib/draft.epub', '.kfx')).toBe('draft.kfx');
  });

  test('no usable name still gives the dialog a name, not a bare extension', () => {
    // A default of ".epub" alone would be a hidden file on a Mac.
    for (const path of ['', '/lib/', null, undefined, 7]) {
      const name = send.saveNameFor(path, 'epub');
      expect(`${String(path)} -> ${name}`).toMatch(/ -> [^.\s][^/\\]*\.epub$/);
    }
  });

  test('one filter, named by the label, holding the one extension', () => {
    expect(send.saveFiltersFor('epub', 'EPUB')).toEqual([{ name: 'EPUB', extensions: ['epub'] }]);
    expect(send.saveFiltersFor('kfx', 'KFX for USB sideload to Kindle'))
      .toEqual([{ name: 'KFX for USB sideload to Kindle', extensions: ['kfx'] }]);
    // The dialog wants the extension bare.
    expect(send.saveFiltersFor('.azw3', 'AZW3')).toEqual([{ name: 'AZW3', extensions: ['azw3'] }]);
    expect(send.saveFiltersFor('epub', '  EPUB ')).toEqual([{ name: 'EPUB', extensions: ['epub'] }]);
  });

  test('a filter with no label is named by its extension rather than left blank', () => {
    for (const label of ['', '   ', null, undefined, 7]) {
      expect(send.saveFiltersFor('azw3', label)).toEqual([{ name: 'AZW3', extensions: ['azw3'] }]);
    }
  });
});

// ------------------------------------------------ routeNoteFrom, statusFor

describe('baseName: the file name at the end of a path', () => {
  test('either separator, and a trailing one is not a name', () => {
    expect(send.baseName('/Users/me/Books/Field Station.epub')).toBe('Field Station.epub');
    expect(send.baseName('C:\\Books\\Field Station.kfx')).toBe('Field Station.kfx');
    expect(send.baseName('/Volumes/Kindle/')).toBe('Kindle');
    expect(send.baseName('plain.epub')).toBe('plain.epub');
  });

  test('nothing to name is an empty name', () => {
    for (const path of ['', '/', null, undefined]) expect(send.baseName(path)).toBe('');
  });
});

describe('routeNoteFrom: what a route’s answer lets the status line say', () => {
  test('the engine’s note, trimmed, as a success', () => {
    const answer = { ok: true, key: 'apple-books', note: '  Added to Apple Books.  ' };
    expect(send.routeNoteFrom(answer)).toEqual(['done', { detail: 'Added to Apple Books.' }]);
    expect(send.statusFor(...send.routeNoteFrom(answer)))
      .toEqual({ line: 'Added to Apple Books.', bad: false });
  });

  test('a success with no note says what is known, and never claims more', () => {
    // 'Done.' would be a claim the window cannot back: it does not know what
    // the route did, only that the engine said ok.
    for (const note of [undefined, null, '', '   ', 7]) {
      const [phase, detail] = send.routeNoteFrom({ ok: true, key: 'save-epub', note });
      expect(phase).toBe('done');
      expect(detail.detail).toBe(send.NO_NOTE);
    }
    expect(send.NO_NOTE).not.toBe('Done.');
    expect(send.NO_NOTE.trim()).not.toBe('');
    expect(send.statusFor(...send.routeNoteFrom({ ok: true })).bad).toBe(false);
  });

  test('anything but ok is the failure path, in the engine’s words', () => {
    const refused = { ok: false, error: { code: 'x', message: '  Apple Books did not open.  ' } };
    expect(send.routeNoteFrom(refused)).toEqual(['failed', { detail: 'Apple Books did not open.' }]);
    expect(send.statusFor(...send.routeNoteFrom(refused)))
      .toEqual({ line: 'Apple Books did not open.', bad: true });
    // A refusal with no sentence gets the stand-in, still as an alarm.
    for (const answer of [{ ok: false }, null, undefined, { ok: 'true', note: 'Added.' }]) {
      const status = send.statusFor(...send.routeNoteFrom(answer));
      expect(status).toEqual({ line: send.NO_MESSAGE, bad: true });
    }
    // A note on a refusal is not a success.
    const [phase] = send.routeNoteFrom({ ok: false, note: 'Added to Apple Books.' });
    expect(phase).toBe('failed');
  });
});

describe('statusFor: the route phases, beside the device ones', () => {
  test('opening names the route being opened', () => {
    expect(send.statusFor('opening', { route: { title: 'Apple Books' } }))
      .toEqual({ line: 'Opening Apple Books…', bad: false });
    expect(send.statusFor('opening', { route: { title: 'Send to Kindle web' } }).line)
      .toBe('Opening Send to Kindle web…');
    expect(send.statusFor('opening', { route: { title: '  Apple Books ' } }).line)
      .toBe('Opening Apple Books…');
    // No title to name: still a line, and still not an alarm.
    for (const route of [undefined, null, {}, { title: '  ' }, { title: 7 }]) {
      expect(send.statusFor('opening', { route })).toEqual({ line: 'Opening…', bad: false });
    }
  });

  test('saving', () => {
    // A Kindle file building is not a status: it has its own line and bar
    // (buildLine()), and nothing on this page that only reads the book waits
    // for it.
    expect(send.statusFor('saving')).toEqual({ line: 'Saving…', bad: false });
  });

  test('done shows the detail it is handed, as good news', () => {
    expect(send.statusFor('done', { detail: 'Saved.' })).toEqual({ line: 'Saved.', bad: false });
  });

  test('the device phases say what they said before', () => {
    const device = { id: '/m/Kindle', kind: 'kindle', name: 'Kindle', volume: '/m/Kindle' };
    expect(send.statusFor('preparing', { device }).line).toBe('Getting the book ready for Kindle…');
    expect(send.statusFor('copying', { device }).line).toBe('Copying it to Kindle…');
    expect(send.statusFor('sent', { detail: 'Sent to Kindle.' })).toEqual({ line: 'Sent to Kindle.', bad: false });
    expect(send.statusFor('failed', { detail: '' })).toEqual({ line: send.NO_MESSAGE, bad: true });
    expect(send.statusFor('idle')).toEqual({ line: '', bad: false });
  });
});

// -------------------------------------------------------- emailSetupHint

describe('emailSetupHint: the one route that needs a first-time step', () => {
  const LINE = 'First time? Amazon needs your sender address approved, or it drops the email without a word.';

  test('the email row gets the hint and the key the engine opens the page by', () => {
    expect(send.emailSetupHint({ key: 'email-to-kindle', available: true }))
      .toEqual({ line: LINE, key: 'kindle-email-setup' });
    // Unavailable too: attaching the EPUB by hand needs the same approval.
    expect(send.emailSetupHint({ key: 'email-to-kindle', available: false, unavailable: 'platform' }))
      .toEqual({ line: LINE, key: 'kindle-email-setup' });
  });

  test('no other row does', () => {
    for (const key of ['apple-books', 'send-to-kindle', 'save-epub', 'save-kindle', 'device:kindle', 'remarkable', 'email']) {
      expect(`${key}: ${send.emailSetupHint({ key })}`).toBe(`${key}: null`);
    }
    for (const route of [null, undefined, {}, 'email-to-kindle']) {
      expect(send.emailSetupHint(route)).toBe(null);
    }
  });

  test('each call hands back its own object', () => {
    const first = send.emailSetupHint({ key: 'email-to-kindle' })!;
    first.line = 'changed';
    expect(send.emailSetupHint({ key: 'email-to-kindle' })!.line).toBe(LINE);
  });

  test('on every engine answer, exactly the email row carries it', () => {
    for (const facts of everyFacts()) {
      const hinted = routes(facts).filter((r) => send.emailSetupHint(r) !== null).map((r) => r.key);
      expect(hinted).toEqual(['email-to-kindle']);
    }
  });
});

// ------------------------------------------------ what the page draws (W3)

describe('connectedDevices: what is plugged in, read off the route list', () => {
  test('the device of every available device row, in the engine’s order', () => {
    const list = routes({
      platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true,
      devices: [kindle, kobo, rm],
    });
    const shown = send.routesFrom(answerFor(list, undefined))!;
    const got = send.connectedDevices(shown)!;
    expect(got.map((d) => d.kind)).toEqual(['kindle', 'kobo', 'remarkable']);
    // Exactly the device objects the rows carry, in the shape `send --device`
    // takes, so the KFX block and ctx.state.devices read what they read from
    // `devices` before.
    expect(got).toEqual(shown.routes.filter((r) => r.device).map((r) => r.device!));
    // Copies, so a reader of ctx.state.devices cannot edit the rows on screen.
    expect(got[0]).not.toBe(shown.routes[0]!.device);
  });

  test('a dimmed device row is not a connected device', () => {
    // Nothing plugged in: every device row is dimmed, and the answer is an
    // empty list, which is what the KFX block reads as "the reader is still
    // deciding" (kindleRelevant).
    const list = routes({
      platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices: [],
    });
    expect(send.connectedDevices(send.routesFrom(answerFor(list, undefined)))).toEqual([]);
  });

  test('no list on screen is not known, rather than nothing connected', () => {
    // null keeps the KFX block from flashing up before the first answer.
    for (const shown of [null, undefined, {}, { routes: 'x' }]) {
      expect(send.connectedDevices(shown)).toBe(null);
    }
  });

  test('on every engine answer, one device per available device row', () => {
    for (const facts of everyFacts()) {
      const list = routes(facts);
      const got = send.connectedDevices(send.routesFrom(answerFor(list, undefined)))!;
      expect(got.length).toBe(facts.devices.length);
    }
  });
});

describe('sameRoutes: the rows are rebuilt only when something on them changed', () => {
  const list = routes({
    platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: false,
    devices: [kindle],
  });
  const shown = () => send.routesFrom(answerFor(list, undefined))!;

  test('the same answer twice is the same list', () => {
    // The poll ticks every two seconds; a rebuild on every tick would take
    // the focus off a button someone had just tabbed to.
    expect(send.sameRoutes(shown(), shown())).toBe(true);
  });

  test('a different choice, detail, availability, title, button or order is not', () => {
    const base = shown();
    const changed = (edit: (s: ReturnType<typeof shown>) => void) => {
      const next = structuredClone(base);
      edit(next);
      return send.sameRoutes(base, next);
    };
    expect(changed((s) => { s.chosen = 'save-epub'; })).toBe(false);
    expect(changed((s) => { s.routes[1]!.detail = 'something else'; })).toBe(false);
    expect(changed((s) => { s.routes[1]!.title = 'Books'; })).toBe(false);
    expect(changed((s) => { s.routes[1]!.button = 'Open'; })).toBe(false);
    expect(changed((s) => { s.routes[0]!.id = 'device:kindle#/Volumes/KINDLE2'; })).toBe(false);
    expect(changed((s) => {
      const r = s.routes[1]!;
      r.available = false;
      (r as { unavailable?: string }).unavailable = 'setup';
    })).toBe(false);
    expect(changed((s) => { s.routes.reverse(); })).toBe(false);
    expect(changed((s) => { s.routes.pop(); })).toBe(false);
  });

  test('nothing on screen is never the same as a list', () => {
    expect(send.sameRoutes(null, shown())).toBe(false);
    expect(send.sameRoutes(shown(), null)).toBe(false);
    expect(send.sameRoutes(null, null)).toBe(false);
  });
});

describe('performerFor: which flow a row’s button runs', () => {
  test('a device or a reMarkable goes through export then send', () => {
    for (const key of ['device:kindle', 'device:kobo', 'device:someday', 'remarkable']) {
      expect(`${key}: ${send.performerFor({ key })}`).toBe(`${key}: device`);
    }
  });

  test('each save is its own flow, and every app or page is opened by the engine', () => {
    expect(send.performerFor({ key: 'save-epub' })).toBe('save-epub');
    expect(send.performerFor({ key: 'save-kindle' })).toBe('save-kindle');
    for (const key of ['apple-books', 'send-to-kindle', 'email-to-kindle']) {
      expect(`${key}: ${send.performerFor({ key })}`).toBe(`${key}: open`);
    }
    // A route this window is older than is still handed to the engine, which
    // performs it or refuses in its own words, rather than drawn as a dead
    // button.
    expect(send.performerFor({ key: 'someday-route' })).toBe('open');
  });

  test('the email row’s setup link is its own flow, and the three names for it agree', async () => {
    const hint = send.emailSetupHint({ key: 'email-to-kindle' })!;
    expect(send.performerFor({ key: hint.key })).toBe('setup');
    // The key the hint carries is the one app.js's builder sends: one page,
    // named once on each side of the boundary, checked here to match.
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.emailSetup()).toContain(hint.key);
  });

  test('a row with no key runs nothing', () => {
    for (const route of [null, undefined, {}, { key: 7 }, { key: '  ' }]) {
      expect(send.performerFor(route)).toBe(null);
    }
  });

  test('on every engine answer, every available row has a flow', () => {
    for (const facts of everyFacts()) {
      for (const route of routes(facts)) {
        if (!route.available) continue;
        const how = send.performerFor(route);
        expect(`${route.id}: ${how !== null && how !== 'setup'}`).toBe(`${route.id}: true`);
        expect(how === 'device').toBe(route.device !== undefined);
      }
    }
  });
});

describe('routeLines: what a row says', () => {
  test('a mounted reader keeps its volume line', () => {
    const list = routes({
      platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true,
      devices: [kindle, kobo, rm],
    });
    const rows = send.routesFrom(answerFor(list, undefined))!.routes;
    const [k, ko, r] = [rows[0]!, rows[1]!, rows[2]!];
    expect(send.routeLines(k, 'MacIntel')).toEqual({
      title: 'Kindle', detail: k.detail, where: '/Volumes/Kindle',
    });
    const kobos = send.routeLines(ko, 'MacIntel');
    expect(kobos.where).toBe('/Volumes/KOBOeReader');
    // A reMarkable never mounts: no volume line at all, because the engine's
    // detail ("the EPUB, over its USB connection") already says how it goes.
    expect(send.routeLines(r, 'MacIntel').where).toBe(null);
    expect(send.routeLines(r, 'MacIntel').detail).toBe(r.detail);
    // A volume that is only spaces is no volume.
    expect(send.routeLines({ ...k, device: { ...k.device!, volume: '  ' } }, 'MacIntel').where).toBe(null);
  });

  test('every other row, available or dimmed, is its title and the engine’s detail', () => {
    const list = routes({
      platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: false, devices: [],
    });
    for (const route of send.routesFrom(answerFor(list, undefined))!.routes) {
      expect(send.routeLines(route, 'MacIntel')).toEqual({
        title: route.title, detail: route.detail, where: null,
      });
    }
  });
});

describe('buttonLabel names the Kindle file on the buttons that hand one over', () => {
  // Which file a Kindle gets depends on this computer (KFX with the whole
  // toolchain, Screepub's own AZW3 everywhere else), and
  // the engine says which before anything is built (`export --check`). It
  // goes on the BUTTON, which is what hands the file over: a reader's name
  // stays the name it mounts as.
  const kfx = { extension: 'kfx', fresh: false, builtBy: 'calibre', bookDate: 1 };
  const list = routes({
    platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true,
    devices: [kindle, kindleTwin, kobo, rm],
  });
  const rows = () => send.routesFrom(answerFor(list, undefined))!.routes;
  const labelled = (file: unknown) => rows().map((r) => send.buttonLabel(r, file));

  test('Save a Kindle file and every connected Kindle’s button carry the extension; nothing else changes', () => {
    const plain = labelled(null);
    expect(plain).toEqual(rows().map((r) => r.button));
    const named = labelled(kfx);
    const changed = plain.map((label, i) => [label, named[i]]).filter(([a, b]) => a !== b);
    expect(changed).toEqual([
      ['Copy to Kindle', 'Copy to Kindle (.kfx)'],
      ['Copy to KINDLE2', 'Copy to KINDLE2 (.kfx)'],
      ['Save a Kindle file…', 'Save a Kindle file (.kfx)…'],
    ]);
    expect(labelled({ ...kfx, extension: 'azw3' })).toContain('Save a Kindle file (.azw3)…');
  });

  test('the rows that hand over a beta Kindle file (our AZW3) are the ones that get the Beta badge', () => {
    const azw3 = { extension: 'azw3', fresh: false, builtBy: 'screepub', bookDate: 1, beta: true };
    const marked = rows().filter((r) => send.kindleFileIsBeta(r, azw3)).map((r) => r.id);
    expect(marked).toEqual(['device:kindle#/Volumes/Kindle', 'device:kindle#/Volumes/KINDLE2', 'save-kindle']);
    expect(rows().filter((r) => send.kindleFileIsBeta(r, kfx))).toEqual([]);
    expect(rows().filter((r) => send.kindleFileIsBeta(r, null))).toEqual([]);
  });

  test('the rows keep their titles: a reader is named as it mounts', () => {
    for (const route of rows()) {
      expect(send.routeLines(route, 'MacIntel').title).toBe(route.title);
    }
  });

  test('which rows hand over a Kindle file: the Kindle save, and a Kindle that is plugged in', () => {
    const handing = rows().filter((r) => send.handsOverKindleFile(r)).map((r) => r.id);
    expect(handing).toEqual(['device:kindle#/Volumes/Kindle', 'device:kindle#/Volumes/KINDLE2', 'save-kindle']);
    // A dimmed Kindle row has nothing to press, and says nothing about files.
    const dimmed = send.routesFrom(answerFor(routes({
      platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices: [],
    }), undefined))!.routes.find((r) => r.key === 'device:kindle')!;
    expect(send.handsOverKindleFile(dimmed)).toBe(false);
    expect(send.handsOverKindleFile(null)).toBe(false);
  });
});

describe('building the Kindle file before anyone presses anything', () => {
  const file = (extension: string, fresh: boolean | null, builtBy = 'calibre', bookDate: number | null = 1000) =>
    ({ extension, fresh, builtBy, bookDate });
  /** A Kindle session, and no build tried yet for this book. */
  const go = { forKindle: true };

  test('out of date, Calibre builds it (KFX or AZW3), and a Kindle is in play: start now', () => {
    expect(send.buildsAhead(file('kfx', false), go)).toBe(true);
    expect(send.buildsAhead(file('azw3', false), go)).toBe(true);
  });

  test('current already, or not known yet (a page come back to before its check answers): nothing to build', () => {
    expect(send.buildsAhead(file('kfx', true), go)).toBe(false);
    expect(send.buildsAhead(file('azw3', true), go)).toBe(false);
    expect(send.buildsAhead(file('kfx', null), go)).toBe(false);
  });

  test('no Calibre: the engine’s own MOBI waits for a press, and Calibre is never started', () => {
    // The MOBI rung rewrites the library EPUB in place and takes a moment;
    // building it ahead buys nothing and holds the Settings page's saves.
    expect(send.buildsAhead(file('mobi', false, 'screepub'), go)).toBe(false);
  });

  test('not a Kindle session (a Kobo, a reMarkable, Apple Books): Kindle Previewer is never started for nobody', () => {
    expect(send.buildsAhead(file('kfx', false))).toBe(false);
    expect(send.buildsAhead(file('kfx', false), { forKindle: false })).toBe(false);
  });

  test('once per version of the book and kind of file: not again for what a build was already started for', () => {
    // Focus comes back constantly (every closed Save box, every switch of
    // app), and a build that failed leaves the file out of date: without
    // this, every return started another half-minute run.
    expect(send.buildsAhead(file('kfx', false, 'calibre', 1000), { ...go, triedFor: 'kfx@1000' })).toBe(false);
    // A Settings save rewrites the book, which moves its date.
    expect(send.buildsAhead(file('kfx', false, 'calibre', 2000), { ...go, triedFor: 'kfx@1000' })).toBe(true);
    // The KFX plugin installed after an AZW3 was built ahead: the same book,
    // a different file, and worth building.
    expect(send.buildsAhead(file('kfx', false, 'calibre', 1000), { ...go, triedFor: 'azw3@1000' })).toBe(true);
    // No date to tell versions apart by: nothing can stop a retry, so no start.
    expect(send.buildsAhead(file('kfx', false, 'calibre', null), go)).toBe(false);
  });

  test('never a second build, never while the page is busy, never on a check that did not answer', () => {
    expect(send.buildsAhead(file('kfx', false), { ...go, building: true })).toBe(false);
    expect(send.buildsAhead(file('kfx', false), { ...go, busy: true })).toBe(false);
    expect(send.buildsAhead(null, go)).toBe(false);
    expect(send.buildsAhead(undefined, go)).toBe(false);
  });

  test('a press may build unless the check said the file is current', () => {
    expect(send.mayBuild(file('kfx', true))).toBe(false);
    expect(send.mayBuild(file('kfx', false))).toBe(true);
    expect(send.mayBuild(file('mobi', false, 'screepub'))).toBe(true);
    expect(send.mayBuild(null)).toBe(true);
  });

  test('the wait before a Kindle copy is the build line when it may build, and a stat when it is current', () => {
    const kindleDevice = { kind: 'kindle', name: 'Kindle' };
    const kobo = { kind: 'kobo', name: 'Kobo' };
    expect(send.preparingPhase(kindleDevice)).toBe('building');
    expect(send.preparingPhase(kindleDevice, file('kfx', false))).toBe('building');
    expect(send.preparingPhase(kindleDevice, file('kfx', true))).toBe('preparing');
    expect(send.preparingPhase(kobo, file('kfx', false))).toBe('preparing');
  });

  test('the line says how long, by the file being built', () => {
    // Kindle Previewer's cold start is most of a KFX build: 23 s for the
    // 18-page demo, measured. Screepub's own AZW3 takes milliseconds, and
    // the MOBI well under a second.
    expect(send.buildLine('kfx')).toBe('Building the Kindle file. This takes about half a minute.');
    expect(send.buildLine('azw3')).toBe('Building the Kindle file. This takes a moment.');
    expect(send.buildLine('mobi')).toBe('Building the Kindle file. This takes a moment.');
    expect(send.buildLine(null)).toBe('Building the Kindle file…');
  });
});

describe('kindleCheckFrom: what the engine says about the Kindle file before it is built', () => {
  test('the extension, whether the file beside the book is current, and what builds it', () => {
    expect(send.kindleCheckFrom({
      ok: true, path: '/lib/s.kfx', format: 'kindle', extension: 'kfx', label: 'KFX',
      available: ['epub', 'kindle'], stages: [], fresh: false, builtBy: 'calibre', bookDate: 1727000000000.5,
    })).toEqual({ extension: 'kfx', fresh: false, builtBy: 'calibre', bookDate: 1727000000000.5, beta: false });
    // The engine says our own AZW3 is beta, and the page keeps that.
    expect(send.kindleCheckFrom({ ok: true, extension: 'azw3', fresh: true, builtBy: 'screepub', bookDate: 5, beta: true }))
      .toEqual({ extension: 'azw3', fresh: true, builtBy: 'screepub', bookDate: 5, beta: true });
  });

  test('a book date that is not a number is not known: the rest of the answer still names the file', () => {
    const good = { ok: true, extension: 'kfx', fresh: false, builtBy: 'calibre' };
    for (const bookDate of [undefined, null, 'yesterday', Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(send.kindleCheckFrom({ ...good, bookDate })?.bookDate).toBe(null);
    }
  });

  test('anything else is not known, which names nothing and builds nothing', () => {
    const good = { ok: true, extension: 'azw3', fresh: true, builtBy: 'calibre' };
    for (const bad of [
      null, {}, { ...good, ok: false }, { ok: false, error: { code: 'x', message: 'no' } },
      { ...good, extension: '' }, { ...good, extension: '../x' }, { ...good, extension: 7 },
      { ...good, fresh: 'yes' }, { ...good, fresh: undefined },
      { ...good, builtBy: 'kindle-previewer' }, { ...good, builtBy: undefined },
    ]) {
      expect(`${JSON.stringify(bad)}: ${send.kindleCheckFrom(bad)}`).toBe(`${JSON.stringify(bad)}: null`);
    }
  });
});

describe('kindleFileFrom: what the Kindle save learns from the export', () => {
  test('the extension and the engine’s label for the file it built', () => {
    expect(send.kindleFileFrom({
      ok: true, path: '/lib/s.azw3', extension: 'azw3', label: 'AZW3 for USB sideload',
    })).toEqual(['built', { extension: 'azw3', label: 'AZW3 for USB sideload' }]);
    expect(send.kindleFileFrom({ ok: true, extension: '.kfx', label: '  KFX  ' }))
      .toEqual(['built', { extension: 'kfx', label: 'KFX' }]);
    // No label: the filter falls back to the extension (saveFiltersFor).
    expect(send.kindleFileFrom({ ok: true, extension: 'mobi' }))
      .toEqual(['built', { extension: 'mobi', label: null }]);
  });

  test('a refusal is the export’s own sentence, and nothing is saved', () => {
    const refused = { ok: false, error: { code: 'export-failed', message: 'Calibre could not build it.' } };
    expect(send.kindleFileFrom(refused)).toEqual(['failed', { detail: 'Calibre could not build it.' }]);
    expect(send.kindleFileFrom({ ok: false })).toEqual(['failed', { detail: send.NO_MESSAGE }]);
    expect(send.kindleFileFrom(null)).toEqual(['failed', { detail: send.NO_MESSAGE }]);
    expect(send.statusFor(...send.kindleFileFrom(refused) as [string, { detail: string }]).bad).toBe(true);
  });

  test('a success that does not say what it built opens no dialog', () => {
    // The extension names the file in the dialog and filters it; one that is
    // missing, or is not a bare word, is not something to hand a Save box.
    for (const extension of [undefined, null, '', '  ', 7, '../x', 'az w3', '.']) {
      const [phase, detail] = send.kindleFileFrom({ ok: true, extension, label: 'AZW3' });
      expect(`${JSON.stringify(extension)}: ${phase}`).toBe(`${JSON.stringify(extension)}: failed`);
      expect(detail.detail).toBe(send.NO_EXTENSION);
    }
  });
});

describe('routesFailure: the line in place of a list the page cannot draw', () => {
  test('a refusal is the engine’s own sentence', () => {
    expect(send.routesFailure({ ok: false, error: { code: 'unreadable', message: 'Cannot read the book.' } }))
      .toBe('Cannot read the book.');
    expect(send.routesFailure({ ok: false })).toBe(send.NO_MESSAGE);
    expect(send.routesFailure(null)).toBe(send.NO_MESSAGE);
  });

  test('an ok answer in the wrong shape says so, not that the engine refused', () => {
    expect(send.routesFailure({ ok: true, routes: 'x' })).toBe(send.NO_ROUTES);
    expect(send.NO_ROUTES).not.toBe(send.NO_MESSAGE);
  });
});

describe('the words this adds', () => {
  test('carry no em dash', () => {
    const lines = [
      send.NO_NOTE,
      send.NO_ROUTES,
      send.NO_EXTENSION,
      send.SETUP_BUTTON,
      send.SETUP_TITLE,
      send.emailSetupHint({ key: 'email-to-kindle' })!.line,
      send.statusFor('opening', { route: { title: 'Apple Books' } }).line,
      send.statusFor('opening', { route: { title: send.SETUP_TITLE } }).line,
      send.statusFor('opening').line,
      send.statusFor('saving').line,
      send.buildLine('kfx'),
      send.buildLine('azw3'),
      send.buildLine(null),
    ];
    for (const line of lines) expect(line).not.toContain(EM_DASH);
  });
});
