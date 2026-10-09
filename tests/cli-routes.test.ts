// `screepub routes` and `screepub route`: the ranked list of every way a
// book can leave, and the verb that performs one of them.
//
// Handlers are driven IN PROCESS with injected facts, a fake Opener that
// records argv, and a settings path inside SCRATCH. Nothing here may open an
// app, a URL or a folder, read the real mail handler, or write the real app
// settings file.
//
// The spawned-CLI tests cover refusals, --help, `routes` (which is
// read-only) and one route that succeeds, `save-epub`, which opens nothing
// and writes only inside SCRATCH. Every spawn gets SCREEPUB_CONFIG_DIR,
// SCREEPUB_LIBRARY and SCREEPUB_VOLUME_ROOTS in SCRATCH, the reMarkable
// probe pointed at a port that refuses at once, and a PATH whose first
// folder holds a fake `open`, `xdg-open` and `defaults`: each one logs its
// argv and exits 1, printing nothing else. So the mail probe reads a canned
// "could not tell" (a failure that does not say the key does not exist)
// instead of this machine's handler,
// and a refusal that ever stopped refusing would log an open here instead of
// opening anything. The last test in this file reads that log.
import { afterAll, describe, expect, test } from 'bun:test';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { CliError } from '../src/cli-errors';
import {
  routeCommand,
  routesCommand,
  routesLines,
  type RouteDeps,
  type RouteOptions,
} from '../src/cli-routes';
import type { FreshKindleArtifactOptions } from '../src/export/artifact';
import type { KfxStatus } from '../src/export/kfx';
import { emailToKindle, realOpener, sendViaAmazon, type Opener } from '../src/export/route-perform';
import { routes, type RouteFacts } from '../src/export/routes';
import { resolveCommand } from '../src/cli-devices';
import type { ConnectedDevice } from '../src/device/types';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-routes-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const ROOT = new URL('..', import.meta.url).pathname;
const EM_DASH = '\u2014';

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };

/** A readable book. The handlers only need a file that is there; nothing in
 *  this file parses it. */
function book(name = 'Script.epub'): string {
  const path = join(mkdtempSync(join(SCRATCH, 'book-')), name);
  writeFileSync(path, 'book-bytes');
  return path;
}

/** A settings file path in its own fresh folder, never created here. */
function settingsFile(): string {
  return join(mkdtempSync(join(SCRATCH, 'config-')), 'settings.json');
}

/** Facts, counted. Darwin with Books, no Amazon app, Apple Mail default,
 *  nothing plugged in, unless a test says otherwise. */
function factsOf(over: Partial<RouteFacts> = {}) {
  const counter = { calls: 0 };
  const facts = async (): Promise<RouteFacts> => {
    counter.calls += 1;
    return {
      platform: 'darwin',
      devices: [],
      booksApp: true,
      sendToKindleApp: false,
      appleMailDefault: true,
      ...over,
    };
  };
  return { counter, facts };
}

async function thrown(run: () => Promise<unknown>): Promise<CliError> {
  try {
    await run();
  } catch (err) {
    expect(err).toBeInstanceOf(CliError);
    return err as CliError;
  }
  throw new Error('expected a CliError, and nothing was thrown');
}

describe('routesCommand', () => {
  test('lists exactly the catalog for the probed facts, in its order', async () => {
    const { facts } = factsOf({ devices: [kindle] });
    const answer = await routesCommand(book(), { facts, settingsPath: settingsFile() });
    expect(answer.routes).toEqual(routes(await facts()));
  });

  test('chosen follows a remembered key', async () => {
    const path = settingsFile();
    writeFileSync(path, JSON.stringify({ lastRoute: 'save-kindle' }));
    const { facts } = factsOf();
    const answer = await routesCommand(book(), { facts, settingsPath: path });
    expect(answer.chosen).toBe('save-kindle');
  });

  test('chosen is the row id, not the key: a remembered Kindle picks the connected one', async () => {
    // A connected device's id carries its volume; its key does not. The
    // window matches rows by id, so answering the key would choose nothing.
    const path = settingsFile();
    writeFileSync(path, JSON.stringify({ lastRoute: 'device:kindle' }));
    const { facts } = factsOf({ devices: [kindle] });
    const answer = await routesCommand(book(), { facts, settingsPath: path });
    expect(answer.chosen).toBe('device:kindle#/Volumes/Kindle');
  });

  test('a remembered Kindle that is unplugged stays chosen', async () => {
    const path = settingsFile();
    writeFileSync(path, JSON.stringify({ lastRoute: 'device:kindle' }));
    const { facts } = factsOf();
    const answer = await routesCommand(book(), { facts, settingsPath: path });
    expect(answer.chosen).toBe('device:kindle');
  });

  test('falls back to the first available row when nothing is remembered', async () => {
    const { facts } = factsOf();
    const answer = await routesCommand(book(), { facts, settingsPath: settingsFile() });
    expect(answer.chosen).toBe('apple-books');
  });

  test('a lastRoute that is not a string is treated as nothing remembered', async () => {
    const path = settingsFile();
    writeFileSync(path, JSON.stringify({ lastRoute: 7 }));
    const { facts } = factsOf({ platform: 'linux', booksApp: false, appleMailDefault: false });
    const answer = await routesCommand(book(), { facts, settingsPath: path });
    expect(answer.chosen).toBe('send-to-kindle');
  });

  test('read-only: no settings file appears, and an existing one is left byte for byte', async () => {
    const absent = settingsFile();
    await routesCommand(book(), { facts: factsOf().facts, settingsPath: absent });
    expect(existsSync(absent)).toBe(false);

    const present = settingsFile();
    const text = '{"lastRoute":"save-epub","libraryPath":"/somewhere"}';
    writeFileSync(present, text);
    const before = statSync(present).mtimeMs;
    await routesCommand(book(), { facts: factsOf().facts, settingsPath: present });
    expect(readFileSync(present, 'utf8')).toBe(text);
    expect(statSync(present).mtimeMs).toBe(before);
  });

  test('an unreadable book is refused first: no probe runs', async () => {
    const { counter, facts } = factsOf();
    const err = await thrown(() =>
      routesCommand(join(SCRATCH, 'no-such.epub'), { facts, settingsPath: settingsFile() }),
    );
    expect(err.code).toBe('unreadable');
    expect(err.message).toContain('no-such.epub');
    expect(counter.calls).toBe(0);
  });

  test('a folder given as the book is unreadable too', async () => {
    const { counter, facts } = factsOf();
    const err = await thrown(() =>
      routesCommand(mkdtempSync(join(SCRATCH, 'dir-')), { facts, settingsPath: settingsFile() }),
    );
    expect(err.code).toBe('unreadable');
    expect(counter.calls).toBe(0);
  });
});

describe('routes for a person', () => {
  test('one line per route, a star on the chosen one, dimmed rows carry their fix', async () => {
    const { facts } = factsOf({ appleMailDefault: false });
    const answer = await routesCommand(book(), { facts, settingsPath: settingsFile() });
    const lines = routesLines(answer);
    expect(lines).toHaveLength(answer.routes.length);
    expect(lines.filter((l) => l.startsWith('*'))).toHaveLength(1);
    const chosen = lines.find((l) => l.startsWith('*'))!;
    expect(chosen).toContain('apple-books');
    expect(chosen).toContain('Apple Books');
    const email = lines.find((l) => l.includes('email-to-kindle'))!;
    expect(email).toContain('(dimmed: needs Apple Mail as the default mail app');
    const save = lines.find((l) => l.includes('save-epub'))!;
    expect(save).not.toContain('dimmed');
    expect(save).toContain('for email, Apple Books and most e-readers');
    for (const line of lines) expect(line).not.toContain(EM_DASH);
  });

  test('the star follows the row id: of two Kindles, only the chosen one is starred', async () => {
    // Two connected Kindles share the key device:kindle and differ only by
    // id, so a star matched by key marks neither (chosen is an id) or both.
    const path = settingsFile();
    writeFileSync(path, JSON.stringify({ lastRoute: 'device:kindle' }));
    const twin: ConnectedDevice = { kind: 'kindle', name: 'KINDLE2', volume: '/Volumes/KINDLE2' };
    const { facts } = factsOf({ devices: [kindle, twin] });
    const lines = routesLines(await routesCommand(book(), { facts, settingsPath: path }));
    const starred = lines.filter((l) => l.startsWith('*'));
    expect(starred).toHaveLength(1);
    expect(starred[0]).toContain('Kindle: over USB');
    expect(lines.find((l) => l.includes('KINDLE2'))!.startsWith(' ')).toBe(true);
  });
});

// ---- route: the performer and the handler -------------------------------

const BOOKS_NOTE = 'Added to Apple Books. It syncs to your iPhone and iPad when Books uses iCloud.';
const APP_NOTE = "Opened Amazon's Send to Kindle app with the book.";
const PAGE_NOTE = "Opened Amazon's Send to Kindle page, and the book's folder so you can drag it in.";
const MAIL_NOTE = "Opened a Mail message with the book attached. Address it to your Kindle's email address.";
const SETUP_NOTE =
  "Opened Amazon's settings page. Find your Kindle's email address there, and add the address you send from to the approved list.";
const STK_URL = 'https://www.amazon.com/sendtokindle';
const PDOC_URL = 'https://www.amazon.com/hz/mycd/myx#/home/settings/pdoc';
const TO_SEND = 'send to a reader with screepub send; route performs the others';

const noToolchain: KfxStatus = { calibre: false, previewer: false, pluginInstalled: false, ready: false };

/** Everything routeCommand can reach, faked and counted: the facts, an
 *  Opener that records argv and answers `codeFor(argv)` (0 unless told), the
 *  Kindle ladder (throws unless a test replaces it), and a settings file in
 *  its own SCRATCH folder that starts absent. */
function rig(over: {
  facts?: Partial<RouteFacts>;
  codeFor?: (argv: string[]) => number;
  stderr?: string;
  platform?: string;
  ladder?: (opts: FreshKindleArtifactOptions) => Promise<string>;
} = {}) {
  const { counter, facts } = factsOf(over.facts);
  const calls: string[][] = [];
  const open: Opener = async (argv) => {
    calls.push(argv);
    return { code: (over.codeFor ?? (() => 0))(argv), stderr: over.stderr ?? '' };
  };
  const ladder = { calls: [] as FreshKindleArtifactOptions[] };
  const settingsPath = settingsFile();
  const deps: RouteDeps = {
    facts,
    open,
    settingsPath,
    ...(over.platform === undefined ? {} : { platform: over.platform }),
    exportDeps: {
      calibreAvailable: () => over.ladder !== undefined,
      kfxStatus: async () => noToolchain,
      freshKindleArtifact: async (opts) => {
        ladder.calls.push(opts);
        if (over.ladder) return over.ladder(opts);
        throw new Error('the Kindle ladder was not expected to run');
      },
    },
  };
  return { deps, calls, counter, ladder, settingsPath };
}

function remembered(path: string): unknown {
  return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : undefined;
}

describe('route: Apple Books', () => {
  test('opens the book with Books, answers the note, and remembers it', async () => {
    const epub = book();
    const r = rig();
    const answer = await routeCommand({ key: 'apple-books', epub }, r.deps);
    expect(r.calls).toEqual([['open', '-a', 'Books', epub]]);
    expect(answer).toEqual({ key: 'apple-books', note: BOOKS_NOTE });
    expect('path' in answer).toBe(false);
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'apple-books' });
  });

  test('a book path with spaces reaches Books as one argument', async () => {
    const epub = book('My Script (draft 2).epub');
    const r = rig();
    await routeCommand({ key: 'apple-books', epub }, r.deps);
    expect(r.calls).toEqual([['open', '-a', 'Books', epub]]);
  });

  test('remembering keeps the settings file\'s other keys', async () => {
    const r = rig();
    writeFileSync(r.settingsPath, JSON.stringify({ lastRoute: 'save-epub', libraryPath: '/kept' }));
    await routeCommand({ key: 'apple-books', epub: book() }, r.deps);
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'apple-books', libraryPath: '/kept' });
  });

  test('Books failing to open is route-failed, naming Apple Books and why, and is NOT remembered', async () => {
    const r = rig({ codeFor: () => 1, stderr: 'LSOpenURLsWithRole() failed with error -10810\n' });
    writeFileSync(r.settingsPath, JSON.stringify({ lastRoute: 'save-epub' }));
    const err = await thrown(() => routeCommand({ key: 'apple-books', epub: book() }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toBe('could not open Apple Books: LSOpenURLsWithRole() failed with error -10810');
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'save-epub' });
  });

  test('a failure that printed nothing still says why: its exit code', async () => {
    const r = rig({ codeFor: () => 3 });
    const err = await thrown(() => routeCommand({ key: 'apple-books', epub: book() }, r.deps));
    expect(err.message).toBe('could not open Apple Books: it exited with code 3');
  });

  test('off a Mac it is unavailable with its fix, and nothing opens', async () => {
    const r = rig({ facts: { platform: 'linux', booksApp: false } });
    const err = await thrown(() => routeCommand({ key: 'apple-books', epub: book() }, r.deps));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toBe('On a Mac only');
    expect(r.calls).toEqual([]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test('on a Mac without Books.app (the row is hidden) it is unavailable, and nothing opens', async () => {
    const r = rig({ facts: { booksApp: false } });
    const err = await thrown(() => routeCommand({ key: 'apple-books', epub: book() }, r.deps));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toContain('Apple Books');
    expect(r.calls).toEqual([]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });
});

describe('route: Send to Kindle', () => {
  test("Amazon's app installed on a Mac: one open, with the book", async () => {
    const epub = book();
    const r = rig({ facts: { sendToKindleApp: true } });
    const answer = await routeCommand({ key: 'send-to-kindle', epub }, r.deps);
    expect(r.calls).toEqual([['open', '-a', 'Send to Kindle', epub]]);
    expect(answer).toEqual({ key: 'send-to-kindle', note: APP_NOTE });
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'send-to-kindle' });
  });

  test('no app on a Mac: open the page, THEN reveal the book', async () => {
    const epub = book();
    const r = rig({ facts: { sendToKindleApp: false } });
    const answer = await routeCommand({ key: 'send-to-kindle', epub }, r.deps);
    expect(r.calls).toEqual([['open', STK_URL], ['open', '-R', epub]]);
    expect(answer.note).toBe(PAGE_NOTE);
  });

  test("Windows: the page opens, then explorer opens the book's folder; the app is a Mac thing", async () => {
    // explorer exits 1 even when it did what was asked: not a failure. The
    // folder, not `/select,<book>`: Bun quotes that whole argument when the
    // path has a space in it, and explorer misreads the quoted form (a comma
    // in the path breaks it too), so a book called "My Script.epub" would
    // open the wrong folder.
    const epub = book('My Script, draft 2.epub');
    const r = rig({
      facts: { platform: 'win32', sendToKindleApp: true, booksApp: false, appleMailDefault: false },
      codeFor: (argv) => (argv[0] === 'explorer' ? 1 : 0),
    });
    const answer = await routeCommand({ key: 'send-to-kindle', epub }, r.deps);
    expect(r.calls).toEqual([
      ['rundll32', 'url.dll,FileProtocolHandler', STK_URL],
      ['explorer', dirname(epub)],
    ]);
    expect(answer.note).toBe(PAGE_NOTE);
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'send-to-kindle' });
  });

  test("Windows: the page failing to open IS a failure, and explorer never runs (only explorer's own code is ignored)", async () => {
    const r = rig({
      facts: { platform: 'win32', booksApp: false, appleMailDefault: false },
      codeFor: (argv) => (argv[0] === 'rundll32' ? 1 : 0),
    });
    const err = await thrown(() => routeCommand({ key: 'send-to-kindle', epub: book() }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toContain("could not open Amazon's Send to Kindle page");
    expect(r.calls).toEqual([['rundll32', 'url.dll,FileProtocolHandler', STK_URL]]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test("Linux: xdg-open the page, then the book's folder", async () => {
    const epub = book();
    const r = rig({ facts: { platform: 'linux', booksApp: false, appleMailDefault: false } });
    await routeCommand({ key: 'send-to-kindle', epub }, r.deps);
    expect(r.calls).toEqual([['xdg-open', STK_URL], ['xdg-open', dirname(epub)]]);
  });

  test('a Mac reveal that fails is still route-failed, but only after the page already opened', async () => {
    const epub = book();
    const r = rig({ codeFor: (argv) => (argv[1] === '-R' ? 1 : 0), stderr: 'no such file' });
    const err = await thrown(() => routeCommand({ key: 'send-to-kindle', epub }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toBe("could not open the book's folder: no such file");
    expect(r.calls).toEqual([['open', STK_URL], ['open', '-R', epub]]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test('the page failing on a Mac is route-failed too, and the folder is never opened', async () => {
    const r = rig({ codeFor: (argv) => (argv[1] === STK_URL ? 1 : 0) });
    const err = await thrown(() => routeCommand({ key: 'send-to-kindle', epub: book() }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toBe("could not open Amazon's Send to Kindle page: it exited with code 1");
    expect(r.calls).toEqual([['open', STK_URL]]);
  });

  test("the page's argv is issued before the folder's, and a folder opener that never resolves does not stop the page from opening", async () => {
    // Some Linux desktops' xdg-open does not return until the file manager
    // window it opened is closed (desktop/ui/app.js excludes `reveal` from
    // the busy count for the same reason), so a folder open that never
    // settles must never hold up the page. Bounded with a short race, not a
    // real wait: a bug that hung forever would hang this suite too.
    const epub = book();
    const calls: string[][] = [];
    const open: Opener = async (argv) => {
      calls.push(argv);
      if (argv[0] === 'open' && argv[1] === '-R') return new Promise(() => {});
      return { code: 0, stderr: '' };
    };
    const settled = sendViaAmazon(epub, { platform: 'darwin', sendToKindleApp: false }, open);
    const race = await Promise.race([
      settled.then(() => 'settled' as const),
      new Promise<'timeout'>((resolve) => setTimeout(() => resolve('timeout'), 50)),
    ]);
    expect(race).toBe('timeout');
    expect(calls).toEqual([['open', STK_URL], ['open', '-R', epub]]);
  });
});

describe('route: email to Kindle', () => {
  test('Apple Mail the default: opens a Mail message with the book', async () => {
    const epub = book();
    const r = rig({ facts: { appleMailDefault: true } });
    const answer = await routeCommand({ key: 'email-to-kindle', epub }, r.deps);
    expect(r.calls).toEqual([['open', '-a', 'Mail', epub]]);
    expect(answer).toEqual({ key: 'email-to-kindle', note: MAIL_NOTE });
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'email-to-kindle' });
  });

  test('another mail app the default: unavailable with the setup fix, nothing opened or remembered', async () => {
    const r = rig({ facts: { appleMailDefault: false } });
    const err = await thrown(() => routeCommand({ key: 'email-to-kindle', epub: book() }, r.deps));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toBe(
      'Needs Apple Mail as the default mail app, the one mail app the attachment survives',
    );
    expect(r.counter.calls).toBe(1);
    expect(r.calls).toEqual([]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test('off a Mac: unavailable, with the platform fix', async () => {
    const r = rig({ facts: { platform: 'win32', booksApp: false, appleMailDefault: false } });
    const err = await thrown(() => routeCommand({ key: 'email-to-kindle', epub: book() }, r.deps));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toBe('On a Mac only; save the EPUB and attach it yourself');
    expect(r.calls).toEqual([]);
  });

  test('Mail failing to open is route-failed, NOT remembered', async () => {
    const r = rig({ codeFor: () => 1, stderr: 'Unable to find application named Mail' });
    const err = await thrown(() => routeCommand({ key: 'email-to-kindle', epub: book() }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toBe('could not open Mail: Unable to find application named Mail');
    expect(existsSync(r.settingsPath)).toBe(false);
  });
});

describe("route email-to-kindle: Amazon's 50 MB limit", () => {
  // Amazon drops mail over its limit without a word to the sender, so a book
  // that is over is refused before Mail opens. Sizes are stood in through the
  // seam: no test here writes a 50 MB file.
  const tooLarge = (mb: string) =>
    `This book is too large for Kindle email. It is ${mb} MB, and Amazon takes up to 50 MB by email ` +
    'and drops anything larger without a word. Send it with the Send to Kindle app or web page instead, which take up to 200 MB.';

  test('a book over the limit is refused before Mail opens, naming Send to Kindle, and is NOT remembered', async () => {
    const epub = book();
    const r = rig();
    const sizes: string[] = [];
    const sizeOf = (path: string) => {
      sizes.push(path);
      return 63_400_000;
    };
    const err = await thrown(() => routeCommand({ key: 'email-to-kindle', epub }, { ...r.deps, sizeOf }));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toBe(tooLarge('63.4'));
    expect(err.message).not.toContain(EM_DASH);
    expect(sizes).toEqual([epub]);
    expect(r.calls).toEqual([]);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test('exactly 50 MB, counted the way Amazon counts it, still goes', async () => {
    const epub = book();
    const r = rig();
    const answer = await routeCommand({ key: 'email-to-kindle', epub }, { ...r.deps, sizeOf: () => 50_000_000 });
    expect(r.calls).toEqual([['open', '-a', 'Mail', epub]]);
    expect(answer.note).toBe(MAIL_NOTE);
  });

  test('one byte over reads as over, never as the 50 MB it is refused for', async () => {
    const r = rig();
    const err = await thrown(() =>
      routeCommand({ key: 'email-to-kindle', epub: book() }, { ...r.deps, sizeOf: () => 50_000_001 }));
    expect(err.message).toBe(tooLarge('50.1'));
    expect(r.calls).toEqual([]);
  });

  test('a whole number of megabytes is written without a decimal', async () => {
    const r = rig();
    const err = await thrown(() =>
      routeCommand({ key: 'email-to-kindle', epub: book() }, { ...r.deps, sizeOf: () => 120_000_000 }));
    expect(err.message).toBe(tooLarge('120'));
  });

  test('the performer refuses on its own, so no caller can attach a book Amazon would drop', async () => {
    const calls: string[][] = [];
    const open: Opener = async (argv) => {
      calls.push(argv);
      return { code: 0, stderr: '' };
    };
    const err = await thrown(() => emailToKindle(book(), open, () => 75_000_000));
    expect(err.code).toBe('route-unavailable');
    expect(err.message).toBe(tooLarge('75'));
    expect(calls).toEqual([]);
  });

  test('with nothing stood in, the size is the file on disk', async () => {
    // book() writes a few bytes, far under the limit, so Mail opens.
    const epub = book();
    const calls: string[][] = [];
    const open: Opener = async (argv) => {
      calls.push(argv);
      return { code: 0, stderr: '' };
    };
    expect(await emailToKindle(epub, open)).toBe(MAIL_NOTE);
    expect(calls).toEqual([['open', '-a', 'Mail', epub]]);
  });
});

describe("route kindle-email-setup: Amazon's settings page", () => {
  const cases: [string, string[]][] = [
    ['darwin', ['open', PDOC_URL]],
    ['win32', ['rundll32', 'url.dll,FileProtocolHandler', PDOC_URL]],
    ['linux', ['xdg-open', PDOC_URL]],
  ];
  for (const [platform, argv] of cases) {
    test(`${platform}: opens the page with the platform's URL opener, no book, no probe`, async () => {
      const r = rig({ platform });
      const answer = await routeCommand({ key: 'kindle-email-setup' }, r.deps);
      expect(r.calls).toEqual([argv]);
      expect(answer).toEqual({ key: 'kindle-email-setup', note: SETUP_NOTE });
      expect(r.counter.calls).toBe(0);
    });
  }

  test('is never remembered: it is a setup step, not a way out', async () => {
    const r = rig({ platform: 'darwin' });
    await routeCommand({ key: 'kindle-email-setup' }, r.deps);
    expect(existsSync(r.settingsPath)).toBe(false);
    writeFileSync(r.settingsPath, JSON.stringify({ lastRoute: 'email-to-kindle' }));
    await routeCommand({ key: 'kindle-email-setup' }, r.deps);
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'email-to-kindle' });
  });

  test('available on every platform, even where email to Kindle is not', async () => {
    const r = rig({ platform: 'linux', facts: { platform: 'linux', appleMailDefault: false } });
    const answer = await routeCommand({ key: 'kindle-email-setup' }, r.deps);
    expect(answer.note).toBe(SETUP_NOTE);
  });

  test('the page failing to open is route-failed', async () => {
    const r = rig({ platform: 'darwin', codeFor: () => 1 });
    const err = await thrown(() => routeCommand({ key: 'kindle-email-setup' }, r.deps));
    expect(err.code).toBe('route-failed');
    expect(err.message).toBe("could not open Amazon's settings page: it exited with code 1");
  });

  test('the host platform is the default', async () => {
    const r = rig();
    await routeCommand({ key: 'kindle-email-setup' }, r.deps);
    expect(r.calls).toHaveLength(1);
    expect(r.calls[0]).toEqual(
      process.platform === 'darwin'
        ? ['open', PDOC_URL]
        : process.platform === 'win32'
          ? ['rundll32', 'url.dll,FileProtocolHandler', PDOC_URL]
          : ['xdg-open', PDOC_URL],
    );
  });
});

describe('route: save a copy', () => {
  test('save-epub writes the book to --out, answers its path, and remembers it; nothing opens or is probed', async () => {
    const epub = book();
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'nested', 'Saved.epub');
    const r = rig();
    const answer = await routeCommand({ key: 'save-epub', epub, out }, r.deps);
    expect(answer).toEqual({ key: 'save-epub', path: out, note: `Saved to ${out}.` });
    expect(readFileSync(out, 'utf8')).toBe('book-bytes');
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'save-epub' });
    expect(r.calls).toEqual([]);
    expect(r.counter.calls).toBe(0);
    expect(r.ladder.calls).toEqual([]);
  });

  test("save-kindle builds with the script's fountain and settings, then copies the file to --out", async () => {
    const epub = book();
    const fountain = join(dirname(epub), 'Script.fountain');
    writeFileSync(fountain, 'INT. ROOM - DAY\n');
    const built = join(dirname(epub), 'Script.azw3');
    writeFileSync(built, 'azw3-bytes');
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'Copy.azw3');
    const r = rig({ ladder: async () => built });
    const answer = await routeCommand(
      { key: 'save-kindle', epub, out, fountain, optionsJson: '{"showSceneNumbers":true}' },
      { ...r.deps, exportDeps: { ...r.deps.exportDeps, kfxStatus: async () => ({ ...noToolchain, calibre: true }) } },
    );
    expect(answer).toEqual({ key: 'save-kindle', path: out, note: `Saved to ${out}.` });
    expect(readFileSync(out, 'utf8')).toBe('azw3-bytes');
    expect(r.ladder.calls).toHaveLength(1);
    expect(r.ladder.calls[0]!.fountainPath).toBe(fountain);
    expect(r.ladder.calls[0]!.format.showSceneNumbers).toBe(true);
    expect(remembered(r.settingsPath)).toEqual({ lastRoute: 'save-kindle' });
    expect(r.calls).toEqual([]);
    expect(r.counter.calls).toBe(0);
  });

  test("save-kindle's format defaults follow RouteDeps.settingsPath, not the production settings file", async () => {
    // settingsPath is what keeps lastRoute out of a real settings file in
    // every other test here; this proves it also keeps the format defaults
    // out of one, by writing a formatDefaults override to the SAME scratch
    // path and never mentioning appSettingsPath in exportDeps at all.
    const epub = book();
    const fountain = join(dirname(epub), 'Script.fountain');
    writeFileSync(fountain, 'INT. ROOM - DAY\n');
    const built = join(dirname(epub), 'Script.azw3');
    writeFileSync(built, 'azw3-bytes');
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'Copy.azw3');
    const r = rig({ ladder: async () => built });
    writeFileSync(r.settingsPath, JSON.stringify({ formatDefaults: { showSceneNumbers: true } }));
    const answer = await routeCommand(
      { key: 'save-kindle', epub, out, fountain },
      { ...r.deps, exportDeps: { ...r.deps.exportDeps, kfxStatus: async () => ({ ...noToolchain, calibre: true }) } },
    );
    expect(answer).toEqual({ key: 'save-kindle', path: out, note: `Saved to ${out}.` });
    expect(r.ladder.calls[0]!.format.showSceneNumbers).toBe(true);
  });

  test('save-kindle whose ladder fails is export-failed, and NOT remembered', async () => {
    const r = rig({
      ladder: async () => {
        throw new Error('Calibre stopped');
      },
    });
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'Copy.azw3');
    const err = await thrown(() => routeCommand({ key: 'save-kindle', epub: book(), out }, r.deps));
    expect(err.code).toBe('export-failed');
    expect(existsSync(out)).toBe(false);
    expect(existsSync(r.settingsPath)).toBe(false);
  });

  test("save-epub to a name that is not .epub is export's usage refusal, nothing written or remembered", async () => {
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'Copy.azw3');
    const r = rig();
    const err = await thrown(() => routeCommand({ key: 'save-epub', epub: book(), out }, r.deps));
    expect(err.code).toBe('usage');
    expect(err.message).toContain('.epub');
    expect(existsSync(out)).toBe(false);
    expect(existsSync(r.settingsPath)).toBe(false);
  });
});

describe('route: remembering never fails a route that worked', () => {
  test('a settings path that cannot be written: the route still answers its note', async () => {
    const blocker = join(mkdtempSync(join(SCRATCH, 'block-')), 'a-file');
    writeFileSync(blocker, 'x');
    const r = rig();
    const answer = await routeCommand(
      { key: 'apple-books', epub: book() },
      { ...r.deps, settingsPath: join(blocker, 'config', 'settings.json') },
    );
    expect(answer.note).toBe(BOOKS_NOTE);
    expect(r.calls).toHaveLength(1);
  });

  test('a read-only settings folder: a save still answers its path', async () => {
    const folder = mkdtempSync(join(SCRATCH, 'readonly-'));
    chmodSync(folder, 0o555);
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'Saved.epub');
    const r = rig();
    try {
      const answer = await routeCommand(
        { key: 'save-epub', epub: book(), out },
        { ...r.deps, settingsPath: join(folder, 'settings.json') },
      );
      expect(answer.path).toBe(out);
      expect(existsSync(out)).toBe(true);
    } finally {
      chmodSync(folder, 0o755);
    }
  });
});

describe('route refuses before it probes, opens, builds or remembers anything', () => {
  // Every usage refusal, each counted: zero opener calls, zero facts calls,
  // zero ladder runs, and no settings file. A refusal that moved below the
  // probe, the ladder or an open shows up here as a count, not as a pass.
  const REFUSALS: [string, (b: string) => RouteOptions, string][] = [
    ['a device key', (b) => ({ key: 'device:kindle', epub: b }), TO_SEND],
    ['another device key', (b) => ({ key: 'device:kobo', epub: b }), TO_SEND],
    ['a bare device: prefix', (b) => ({ key: 'device:', epub: b }), TO_SEND],
    ['remarkable', (b) => ({ key: 'remarkable', epub: b }), TO_SEND],
    ['an unknown key', (b) => ({ key: 'carrier-pigeon', epub: b }), 'no route is called "carrier-pigeon"'],
    ['an empty key', (b) => ({ key: '', epub: b }), 'no route is called ""'],
    ['a key in the wrong case', (b) => ({ key: 'Apple-Books', epub: b }), 'no route is called "Apple-Books"'],
    ['no book for apple-books', () => ({ key: 'apple-books' }), 'route apple-books needs the book'],
    ['no book for save-epub', () => ({ key: 'save-epub', out: '/x/Saved.epub' }), 'route save-epub needs the book'],
    ['a book for kindle-email-setup', (b) => ({ key: 'kindle-email-setup', epub: b }), 'kindle-email-setup takes no book'],
    ['--out on kindle-email-setup', () => ({ key: 'kindle-email-setup', out: '/x/Saved.epub' }), 'kindle-email-setup takes no --out'],
    ['--fountain on kindle-email-setup', () => ({ key: 'kindle-email-setup', fountain: '/x.fountain' }), 'kindle-email-setup takes no --fountain'],
    ['--options-json on kindle-email-setup', () => ({ key: 'kindle-email-setup', optionsJson: '{}' }), 'kindle-email-setup takes no --options-json'],
    ['save-epub without --out', (b) => ({ key: 'save-epub', epub: b }), 'route save-epub needs --out'],
    ['save-kindle without --out', (b) => ({ key: 'save-kindle', epub: b }), 'route save-kindle needs --out'],
    ['a relative --out', (b) => ({ key: 'save-epub', epub: b, out: 'Saved.epub' }), 'must be absolute'],
    ['a relative --out on save-kindle', (b) => ({ key: 'save-kindle', epub: b, out: 'Saved.azw3' }), 'must be absolute'],
    ['--out on apple-books', (b) => ({ key: 'apple-books', epub: b, out: '/x/Saved.epub' }), 'route apple-books takes no --out'],
    ['--out on send-to-kindle', (b) => ({ key: 'send-to-kindle', epub: b, out: '/x/Saved.epub' }), 'route send-to-kindle takes no --out'],
    ['--out on email-to-kindle', (b) => ({ key: 'email-to-kindle', epub: b, out: '/x/Saved.epub' }), 'route email-to-kindle takes no --out'],
    ['--fountain on apple-books', (b) => ({ key: 'apple-books', epub: b, fountain: '/x.fountain' }), 'route apple-books takes no --fountain'],
    ['--options-json on send-to-kindle', (b) => ({ key: 'send-to-kindle', epub: b, optionsJson: '{}' }), 'route send-to-kindle takes no --options-json'],
    ['--fountain on save-epub', (b) => ({ key: 'save-epub', epub: b, out: '/x/Saved.epub', fountain: '/x.fountain' }), 'route save-epub takes no --fountain'],
    ['--options-json on save-epub', (b) => ({ key: 'save-epub', epub: b, out: '/x/Saved.epub', optionsJson: '{}' }), 'route save-epub takes no --options-json'],
  ];

  for (const [name, options, fragment] of REFUSALS) {
    test(`${name}: usage, and nothing ran`, async () => {
      const r = rig({ ladder: async () => join(SCRATCH, 'never.azw3') });
      const err = await thrown(() => routeCommand(options(book()), r.deps));
      expect(err.code).toBe('usage');
      expect(err.message).toContain(fragment);
      expect(err.message).not.toContain(EM_DASH);
      expect(r.calls).toEqual([]);
      expect(r.counter.calls).toBe(0);
      expect(r.ladder.calls).toEqual([]);
      expect(existsSync(r.settingsPath)).toBe(false);
    });
  }

  test('an unknown key names every key route takes', async () => {
    const err = await thrown(() => routeCommand({ key: 'carrier-pigeon', epub: book() }, rig().deps));
    for (const key of ['apple-books', 'send-to-kindle', 'email-to-kindle', 'save-epub', 'save-kindle', 'kindle-email-setup']) {
      expect(err.message).toContain(key);
    }
  });

  test('a book that is not there is unreadable, before any probe or open', async () => {
    for (const key of ['apple-books', 'save-epub']) {
      const r = rig();
      const err = await thrown(() =>
        routeCommand({ key, epub: join(SCRATCH, 'ghost.epub'), out: key === 'save-epub' ? '/x/Saved.epub' : undefined }, r.deps),
      );
      expect(err.code).toBe('unreadable');
      expect(err.message).toContain('ghost.epub');
      expect(r.calls).toEqual([]);
      expect(r.counter.calls).toBe(0);
      expect(existsSync(r.settingsPath)).toBe(false);
    }
  });

  test('the device check comes before the book check: a device key with no book still points at send', async () => {
    const r = rig();
    const err = await thrown(() => routeCommand({ key: 'device:kindle' }, r.deps));
    expect(err.code).toBe('usage');
    expect(err.message).toBe(TO_SEND);
  });
});

describe('the default Opener', () => {
  // Runs `sh`, never `open`: what is under test is how a real process's exit
  // code and stderr come back, and that a missing program is an answer, not
  // a throw.
  test('returns the exit code and stderr of what it ran', async () => {
    expect(await realOpener(['sh', '-c', 'echo nope >&2; exit 3'])).toEqual({ code: 3, stderr: 'nope\n' });
    expect(await realOpener(['sh', '-c', 'exit 0'])).toEqual({ code: 0, stderr: '' });
  });

  test('a program that is not there is a non-zero code with a reason, not a throw', async () => {
    const answer = await realOpener([join(SCRATCH, 'no-such-program')]);
    expect(answer.code).not.toBe(0);
    expect(answer.stderr.length).toBeGreaterThan(0);
  });
});

// ---- the spawned CLI ----------------------------------------------------

const CONFIG = join(SCRATCH, 'config');
const LIBRARY = join(SCRATCH, 'library');
const NO_MOUNTS = mkdtempSync(join(SCRATCH, 'mounts-'));
const FAKE_BIN = mkdtempSync(join(SCRATCH, 'bin-'));
const TOOL_LOG = join(SCRATCH, 'tools.log');
for (const tool of ['open', 'xdg-open', 'defaults']) {
  const script = join(FAKE_BIN, tool);
  writeFileSync(script, `#!/bin/sh\necho "${tool} $*" >> '${TOOL_LOG}'\nexit 1\n`);
  chmodSync(script, 0o755);
}

async function runCli(args: string[], env: Record<string, string> = {}) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
    cwd: ROOT,
    env: {
      ...process.env,
      SCREEPUB_CONFIG_DIR: CONFIG,
      SCREEPUB_LIBRARY: LIBRARY,
      SCREEPUB_VOLUME_ROOTS: NO_MOUNTS,
      SCREEPUB_REMARKABLE_ENDPOINT: 'http://127.0.0.1:9',
      PATH: `${FAKE_BIN}${delimiter}${process.env.PATH ?? ''}`,
      ...env,
    },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}

/** stdout is EXACTLY one parseable object. */
function soleJson(stdout: string): any {
  const lines = stdout.trim().split('\n');
  expect(lines).toHaveLength(1);
  return JSON.parse(lines[0]!);
}

function toolLog(): string[] {
  return existsSync(TOOL_LOG) ? readFileSync(TOOL_LOG, 'utf8').trim().split('\n').filter(Boolean) : [];
}

describe('screepub routes (through the CLI)', () => {
  test('is a verb, and a file of that name still converts', () => {
    expect(resolveCommand(['routes', 'x.epub'], () => false)).toEqual({
      kind: 'verb', verb: 'routes', args: ['x.epub'],
    });
    expect(resolveCommand(['routes'], (p) => p === 'routes')).toEqual({ kind: 'convert' });
  });

  test('--json is one object: ok, every route, the save rows available, and chosen among them', async () => {
    const { stdout, exitCode } = await runCli(['routes', book(), '--json']);
    expect(exitCode).toBe(0);
    const answer = soleJson(stdout);
    expect(answer.ok).toBe(true);
    expect(Object.keys(answer).sort()).toEqual(['chosen', 'ok', 'routes']);
    const byKey = new Map(answer.routes.map((r: any) => [r.key, r]));
    expect((byKey.get('save-epub') as any).available).toBe(true);
    expect((byKey.get('save-kindle') as any).available).toBe(true);
    expect(answer.routes.map((r: any) => r.id)).toContain(answer.chosen);
    if (process.platform === 'darwin') {
      // The fake `defaults` exits 1, so the mail probe could not tell, which
      // is never Apple Mail: the row is dimmed with its setup fix. That it
      // is, proves the real handler was not the one read.
      expect((byKey.get('email-to-kindle') as any).unavailable).toBe('setup');
      expect(toolLog().some((line) => line.startsWith('defaults read'))).toBe(true);
    }
    // Read-only: no settings file was made.
    expect(existsSync(join(CONFIG, 'settings.json'))).toBe(false);
  });

  test('chosen follows the settings file under SCREEPUB_CONFIG_DIR', async () => {
    const config = mkdtempSync(join(SCRATCH, 'config-'));
    writeFileSync(join(config, 'settings.json'), JSON.stringify({ lastRoute: 'save-kindle' }));
    const { stdout, exitCode } = await runCli(['routes', book(), '--json'], {
      SCREEPUB_CONFIG_DIR: config,
    });
    expect(exitCode).toBe(0);
    expect(soleJson(stdout).chosen).toBe('save-kindle');
  });

  test('a connected Kindle under SCREEPUB_VOLUME_ROOTS is listed first', async () => {
    const mounts = mkdtempSync(join(SCRATCH, 'mounts-'));
    mkdirSync(join(mounts, 'Kindle', 'documents'), { recursive: true });
    const { stdout } = await runCli(['routes', book(), '--json'], { SCREEPUB_VOLUME_ROOTS: mounts });
    const first = soleJson(stdout).routes[0];
    expect(first.key).toBe('device:kindle');
    expect(first.device.volume).toBe(join(mounts, 'Kindle'));
  });

  test('without --json: one line per route, one star', async () => {
    const { stdout, exitCode } = await runCli(['routes', book()]);
    expect(exitCode).toBe(0);
    expect(stdout.trim().startsWith('{')).toBe(false);
    const lines = stdout.trim().split('\n');
    expect(lines.filter((l) => l.startsWith('*'))).toHaveLength(1);
    expect(stdout).toContain('save-epub');
    expect(stdout).toContain('(dimmed: ');
  });

  test('--quick answers without asking the reMarkable, and says the tablet is still being looked for', async () => {
    // A stand-in tablet on this machine that takes every request and never
    // answers, so no real network is involved: a full `routes` waits out the
    // probe's whole timeout on it, which is the wait the send panel's first
    // rows used to sit behind. The quick answer must not ask it at all.
    let asked = 0;
    const tablet = Bun.serve({
      port: 0,
      fetch: () => {
        asked += 1;
        return new Promise<Response>(() => {});
      },
    });
    try {
      const env = { SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${tablet.port}` };
      const quick = await runCli(['routes', book(), '--quick', '--json'], env);
      expect(quick.exitCode).toBe(0);
      const quickRow = soleJson(quick.stdout).routes.find((r: any) => r.key === 'remarkable');
      expect(quickRow.detail).toBe('checking whether one is docked…');
      expect(asked).toBe(0);

      // The full answer is the one that looks, and waits for it.
      const full = await runCli(['routes', book(), '--json'], env);
      expect(full.exitCode).toBe(0);
      const fullRow = soleJson(full.stdout).routes.find((r: any) => r.key === 'remarkable');
      expect(fullRow.detail).toBe('dock over USB to send');
      expect(asked).toBe(1);
    } finally {
      tablet.stop(true);
    }
  });

  test('a missing book is unreadable', async () => {
    const { stdout, exitCode } = await runCli(['routes', join(SCRATCH, 'ghost.epub'), '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unreadable');
  });

  // Every other verb's flag is refused, naming its owners:
  // tests/cli-verb-flags.test.ts checks that for every verb and every flag.

  test('takes exactly one book', async () => {
    const none = soleJson((await runCli(['routes', '--json'])).stdout);
    expect(none.error.code).toBe('usage');
    const two = await runCli(['routes', book(), book(), '--json']);
    expect(two.exitCode).toBe(1);
    expect(soleJson(two.stdout).error.code).toBe('usage');
  });

  test('--help is its own usage, not the conversion flags', async () => {
    const { stdout, exitCode } = await runCli(['routes', '--help', '--json']);
    expect(exitCode).toBe(0);
    const { usage } = soleJson(stdout);
    expect(usage).toContain('screepub routes <file.epub>');
    expect(usage).toContain('--quick');
    expect(usage).not.toContain('--mobi');
    expect(usage).not.toContain('--out');
    expect(usage).not.toContain(EM_DASH);
  });

  test('the main usage lists it', async () => {
    const { stdout } = await runCli(['--help']);
    expect(stdout).toContain('screepub routes <file.epub> [--quick] [--json]');
  });
});

describe('screepub route (through the CLI)', () => {
  // Refusals and --help ONLY (the one spawned success, a save, has its own
  // block below). No spawned run here names a key that opens
  // anything (apple-books, send-to-kindle, email-to-kindle,
  // kindle-email-setup): what those open is proven in process, above, with a
  // fake Opener. Every run below uses a device key, an unknown key, or a
  // save aimed inside SCRATCH, so even a refusal that stopped refusing could
  // at worst write a file into SCRATCH.
  const OUT = join(SCRATCH, 'never', 'Saved.epub');

  test('is a verb, and a file of that name still converts', () => {
    expect(resolveCommand(['route', 'save-epub', 'x.epub'], () => false)).toEqual({
      kind: 'verb', verb: 'route', args: ['save-epub', 'x.epub'],
    });
    expect(resolveCommand(['route'], (p) => p === 'route')).toEqual({ kind: 'convert' });
  });

  test('--help is its own usage, naming every key, kindle-email-setup included', async () => {
    const { stdout, exitCode } = await runCli(['route', '--help', '--json']);
    expect(exitCode).toBe(0);
    const { usage } = soleJson(stdout);
    expect(usage).toContain('screepub route <key> <file.epub>');
    expect(usage).toContain('screepub route kindle-email-setup');
    for (const key of ['apple-books', 'send-to-kindle', 'email-to-kindle', 'save-epub', 'save-kindle', 'kindle-email-setup']) {
      expect(usage).toContain(key);
    }
    expect(usage).toContain('--out');
    expect(usage).not.toContain('--mobi');
    expect(usage).not.toContain('--device');
    expect(usage).not.toContain(EM_DASH);
  });

  test('the main usage lists it', async () => {
    const { stdout } = await runCli(['--help']);
    expect(stdout).toContain('screepub route <key> <file.epub> [--out <path>] [--json]');
  });

  test('a device key and remarkable point at send', async () => {
    for (const key of ['device:kindle', 'remarkable']) {
      const { stdout, exitCode } = await runCli(['route', key, book(), '--json']);
      expect(exitCode).toBe(1);
      expect(soleJson(stdout).error).toEqual({ code: 'usage', message: TO_SEND });
    }
  });

  test('an unknown key is usage, naming the keys', async () => {
    const { stdout, exitCode } = await runCli(['route', 'carrier-pigeon', book(), '--json']);
    expect(exitCode).toBe(1);
    const { error } = soleJson(stdout);
    expect(error.code).toBe('usage');
    expect(error.message).toContain('save-epub');
  });

  test('no key at all is usage', async () => {
    const { stdout, exitCode } = await runCli(['route', '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
  });

  test('a second book is usage', async () => {
    const { stdout, exitCode } = await runCli(['route', 'save-epub', book(), book(), '--out', OUT, '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
    expect(existsSync(OUT)).toBe(false);
  });

  // Every other verb's flag is refused before the handler runs, naming its
  // owners: tests/cli-verb-flags.test.ts checks that for every verb and
  // every flag.

  test('a save without --out is usage naming --out', async () => {
    const { stdout, exitCode } = await runCli(['route', 'save-epub', book(), '--json']);
    expect(exitCode).toBe(1);
    const { error } = soleJson(stdout);
    expect(error.code).toBe('usage');
    expect(error.message).toContain('--out');
  });

  test('--out passes the verb and reaches the handler', async () => {
    // Not refused as a flag route does not take: a missing book is the
    // handler's own unreadable, and a relative --out its own refusal.
    const ghost = await runCli(['route', 'save-epub', join(SCRATCH, 'ghost.epub'), '--out', OUT, '--json']);
    expect(soleJson(ghost.stdout).error.code).toBe('unreadable');
    const relative = await runCli(['route', 'save-epub', book(), '--out', 'Saved.epub', '--json']);
    const { error } = soleJson(relative.stdout);
    expect(error.code).toBe('usage');
    expect(error.message).toContain('must be absolute');
  });

  test('--fountain and --options-json reach the handler, which refuses them for save-epub', async () => {
    // Refused by routeCommand, not by the verb: a verb that dropped either
    // flag on the way would let this save go ahead (into SCRATCH).
    for (const [flag, value] of [['--fountain', '/x.fountain'], ['--options-json', '{}']] as const) {
      const { stdout, exitCode } = await runCli(['route', 'save-epub', book(), '--out', OUT, flag, value, '--json']);
      expect(exitCode).toBe(1);
      expect(soleJson(stdout).error).toEqual({
        code: 'usage',
        message: `route save-epub takes no ${flag} (only save-kindle builds a file)`,
      });
    }
    expect(existsSync(OUT)).toBe(false);
  });

  test('--fountain and --options-json pass the verb for save-kindle', async () => {
    const { stdout } = await runCli([
      'route', 'save-kindle', join(SCRATCH, 'ghost.epub'), '--out', join(SCRATCH, 'never', 'Saved.azw3'),
      '--fountain', join(SCRATCH, 'ghost.fountain'), '--options-json', '{}', '--json',
    ]);
    expect(soleJson(stdout).error.code).toBe('unreadable');
  });

  test('without --json a refusal is one line on stderr, not JSON', async () => {
    const { stdout, stderr, exitCode } = await runCli(['route', 'remarkable', book()]);
    expect(exitCode).toBe(1);
    expect(stdout).toBe('');
    expect(stderr.trim()).toBe(`screepub: ${TO_SEND}`);
  });

  test('every verb that refuses --fountain and --options-json now names route as an owner too', async () => {
    const fountain = soleJson((await runCli(['devices', '--fountain', 'x', '--json'])).stdout);
    expect(fountain.error.message).toBe('devices takes no --fountain (--fountain belongs to export and route)');
    const optionsJson = soleJson((await runCli(['send', book(), '--options-json', '{}', '--json'])).stdout);
    expect(optionsJson.error.message).toBe(
      'send takes no --options-json (--options-json belongs to export and route)',
    );
    const routesFountain = soleJson((await runCli(['routes', book(), '--fountain', 'x', '--json'])).stdout);
    expect(routesFountain.error.message).toBe('routes takes no --fountain (--fountain belongs to export and route)');
  });

  test('no refusal remembered anything', () => {
    expect(existsSync(join(CONFIG, 'settings.json'))).toBe(false);
  });
});

describe('screepub route save-epub (through the CLI): the one route that works when spawned', () => {
  // The only key it is safe to let succeed in a spawned run: a save opens
  // nothing, and --out, the settings folder, the library and the mounts are
  // all inside SCRATCH. What the verb prints on success is proven here, for
  // real, rather than read out of cli.ts's source.
  function saveRun() {
    const config = mkdtempSync(join(SCRATCH, 'config-'));
    const out = join(mkdtempSync(join(SCRATCH, 'save-')), 'nested dir', 'Copy.epub');
    return { config, out, settings: join(config, 'settings.json') };
  }

  test('--json: one object, ok with the key, the path written and the note; the copy is there and remembered', async () => {
    const epub = book();
    const { config, out, settings } = saveRun();
    const { stdout, stderr, exitCode } = await runCli(
      ['route', 'save-epub', epub, '--out', out, '--json'],
      { SCREEPUB_CONFIG_DIR: config },
    );
    expect(stderr).toBe('');
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, key: 'save-epub', path: out, note: `Saved to ${out}.` });
    expect(readFileSync(out, 'utf8')).toBe('book-bytes');
    expect(JSON.parse(readFileSync(settings, 'utf8'))).toEqual({ lastRoute: 'save-epub' });
  });

  test('for a person: the note alone, one line, no JSON', async () => {
    const epub = book();
    const { config, out, settings } = saveRun();
    const { stdout, stderr, exitCode } = await runCli(
      ['route', 'save-epub', epub, '--out', out],
      { SCREEPUB_CONFIG_DIR: config },
    );
    expect(stderr).toBe('');
    expect(exitCode).toBe(0);
    expect(stdout).toBe(`Saved to ${out}.\n`);
    expect(existsSync(out)).toBe(true);
    expect(JSON.parse(readFileSync(settings, 'utf8'))).toEqual({ lastRoute: 'save-epub' });
  });
});

describe("cli.ts's route branch, in the source", () => {
  // What the branch prints on success is proven by the spawned save above;
  // the two pins that read it out of the source are gone. This one stays,
  // because it covers keys no spawned run may let through: every refusal
  // the branch makes itself comes before the handler is called, so a
  // refusal moved below it could not open Books first and refuse after.
  const START = "if (verb === 'route') {";
  const END = "// verb === 'send'";
  const REFUSAL = "fail({ code: 'usage'";

  async function branch(): Promise<string> {
    const source = await Bun.file(`${ROOT}src/cli.ts`).text();
    const start = source.indexOf(START);
    const end = start === -1 ? -1 : source.indexOf(END, start);
    expect(start, 'the route branch moved').toBeGreaterThan(-1);
    expect(end, 'the route branch lost its end marker').toBeGreaterThan(start);
    return source.slice(start, end);
  }

  test('every refusal in the branch comes before the handler is called', async () => {
    const text = await branch();
    const called = text.indexOf('routeCommand(');
    expect(called).toBeGreaterThan(-1);
    const count = (s: string) => s.split(REFUSAL).length - 1;
    // The positional check. The foreign-flag refusal, every verb's, is made
    // once in runVerb above every branch.
    expect(count(text.slice(0, called))).toBe(1);
    expect(count(text.slice(called))).toBe(0);
    const source = await Bun.file(`${ROOT}src/cli.ts`).text();
    const flagCheck = source.indexOf('foreignFlagRefusal(verb, values)');
    expect(flagCheck, 'the foreign-flag check moved out of runVerb')
      .toBeGreaterThan(source.indexOf('async function runVerb('));
    expect(flagCheck, 'the foreign-flag check moved below the route branch')
      .toBeLessThan(source.indexOf(START));
  });
});

describe('the spawned runs stayed harmless', () => {
  // Last in the file on purpose: bun runs a file's tests in order.
  test('no spawned run opened an app, a page or a folder', () => {
    expect(toolLog().filter((line) => !line.startsWith('defaults '))).toEqual([]);
  });
});
