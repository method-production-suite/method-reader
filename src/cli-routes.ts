// The `routes` and `route` verbs: every way a finished book can leave
// Screepub, best first, with the one to choose already chosen; and the verb
// that performs one of the routes that are not a reader over USB. The ORDER
// is src/export/routes.ts's, the probing src/export/route-facts.ts's, the
// opening src/export/route-perform.ts's and the saving cli-export.ts's; this
// file chooses nothing, it only checks the request and carries their answers
// out. Handlers RETURN values and never print: cli.ts owns stdout, the same
// split as cli-export.ts and cli-kfx.ts.
import { statSync } from 'node:fs';
import { CliError, errorMessage } from './cli-errors';
import { exportCommand, type ExportDeps } from './cli-export';
import { routeFacts } from './export/route-facts';
import {
  addToAppleBooks,
  emailToKindle,
  openKindleEmailSettings,
  realOpener,
  sendViaAmazon,
  type Opener,
} from './export/route-perform';
import { preselected, routes, type Route, type RouteFacts } from './export/routes';
import { appSettingsPath, readAppSettings, writeAppSettings } from './settings/app';

/** Injectable seams, defaulted to the real thing, so a test can hand in any
 *  machine's facts and keep the settings file inside its own scratch folder. */
export interface RoutesDeps {
  /** default: routeFacts() on this machine */
  facts?: () => Promise<RouteFacts>;
  /** default: appSettingsPath(), which SCREEPUB_CONFIG_DIR overrides */
  settingsPath?: string;
}

export interface RoutesAnswer {
  routes: Route[];
  /** The id (not the key) of the row to choose first: a connected device's
   *  id carries its volume, and the window matches rows by id. */
  chosen: string;
}

/** The book must be a file that is there, checked before anything is probed:
 *  a typo must not pay for the reMarkable probe's timeout to be reported.
 *  Same check and code as `export`. */
export function requireBook(epub: string): void {
  let isFile = false;
  try {
    isFile = statSync(epub).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    throw new CliError('unreadable', `cannot read the book: ${epub}`);
  }
}

/** Read-only: reads the settings file, never writes it. The send panel asks
 *  this whenever what is plugged in changes, and a read that wrote would race the one
 *  write that matters, `route` or `send` remembering a choice that worked.
 *
 *  The file is shared and hand-editable, so `lastRoute` may hold anything.
 *  preselected matches it against row keys with `===`, so a value that is
 *  not one of them (a number, a key from a newer build) falls back exactly
 *  as nothing remembered does. */
export async function routesCommand(epub: string, deps: RoutesDeps = {}): Promise<RoutesAnswer> {
  requireBook(epub);
  const list = routes(await (deps.facts ?? (() => routeFacts()))());
  const { lastRoute } = readAppSettings(deps.settingsPath ?? appSettingsPath());
  return { routes: list, chosen: preselected(list, lastRoute).id };
}

/** `routes` for a person: one line per row, `*` beside the chosen one, the
 *  key a person would type for `screepub route`, and a dimmed row's fix in
 *  place of its mechanism. */
export function routesLines(answer: RoutesAnswer): string[] {
  const width = Math.max(0, ...answer.routes.map((r) => r.key.length));
  return answer.routes.map((r) => {
    const mark = r.id === answer.chosen ? '*' : ' ';
    const detail = r.available ? r.detail : `(dimmed: ${r.detail})`;
    return `${mark} ${r.key.padEnd(width)}  ${r.title}: ${detail}`;
  });
}

// ---- route -----------------------------------------------------------------

/** The routes this verb opens a program for, and the name each is refused by
 *  when its row is not listed at all (Apple Books on a Mac without Books). */
const PERFORMED: Record<string, string> = {
  'apple-books': 'Apple Books',
  'send-to-kindle': "Amazon's Send to Kindle",
  'email-to-kindle': 'Mail',
};

/** The two routes that write a copy where --out says, and nothing else. */
const SAVES = new Set(['save-epub', 'save-kindle']);

/** Not a route in the catalog: the email row's one-time setup. It opens
 *  Amazon's settings page, needs no book, works on every platform, and is
 *  never remembered (it is a step toward a route, not a way out). */
const EMAIL_SETUP = 'kindle-email-setup';

/** Every key `route` takes, in the order its --help lists them. */
export const ROUTE_KEYS = [...Object.keys(PERFORMED), ...SAVES, EMAIL_SETUP];

export interface RouteDeps extends RoutesDeps {
  /** default: realOpener, which spawns the program */
  open?: Opener;
  /** Handed to exportCommand for the two saves. Its own appSettingsPath, if
   *  given, wins; otherwise `settingsPath` (above) covers it too, so one
   *  injected path keeps BOTH lastRoute and the format defaults out of the
   *  production settings file, rather than only the first. */
  exportDeps?: ExportDeps;
  /** kindle-email-setup's platform; the others read it off the facts.
   *  default: process.platform */
  platform?: string;
}

export interface RouteOptions {
  key: string;
  /** Every key but kindle-email-setup needs exactly one book. */
  epub?: string;
  /** The two saves only: an absolute path, as a save dialog hands back. */
  out?: string;
  /** save-kindle only: what a MOBI rebuild needs. */
  fountain?: string;
  optionsJson?: string;
}

export interface RouteAnswer {
  key: string;
  /** Where a save wrote the copy. Absent for every other route. */
  path?: string;
  /** The sentence the window shows. */
  note: string;
}

/** Remember a route that just worked, as the one to choose next time.
 *
 *  Never fails whatever called it. The book is already in Books, on its way
 *  to Amazon, or on the reader; a settings file that cannot be written (a
 *  read-only folder, a full disk) must not turn that into an error the
 *  person would answer by doing it all again. Shared with `send`. */
export function rememberRoute(key: string, settingsPath?: string): void {
  try {
    writeAppSettings({ lastRoute: key }, settingsPath ?? appSettingsPath());
  } catch {
    // Deliberately ignored: see above.
  }
}

/** `route` never performs a device row (those are refused), so it asks for
 *  the facts without the device scan: no mount walk, and no waiting out the
 *  reMarkable probe's timeout to open Apple Books. */
const factsWithoutDevices = () => routeFacts({ devices: async () => [] });

function leadingCapital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Every flag this key cannot act on, refused rather than ignored: a flag
 *  that silently did nothing teaches the caller it did something. */
function refuseFlags(options: RouteOptions, who: string, allowed: { out: boolean; build: boolean }): void {
  if (!allowed.out && options.out !== undefined) {
    throw new CliError('usage', `${who} takes no --out (only save-epub and save-kindle write a file)`);
  }
  if (!allowed.build) {
    if (options.fountain !== undefined) {
      throw new CliError('usage', `${who} takes no --fountain (only save-kindle builds a file)`);
    }
    if (options.optionsJson !== undefined) {
      throw new CliError('usage', `${who} takes no --options-json (only save-kindle builds a file)`);
    }
  }
}

/** Perform one route that is not a reader over USB, and remember it when it
 *  worked.
 *
 *  Order, and the reason for it: everything that can be refused from the
 *  request alone (the key, the book, which flags this key takes) is refused
 *  BEFORE any probe, build or open, so a mistyped command never opens
 *  anything and never waits on anything to be told so. Then availability,
 *  from the same catalog the send panel draws (so the engine never performs a
 *  row the page shows dimmed); then the work; then, on success only, the
 *  memory.
 *
 *  The saves skip the probe: the catalog lists both as available on every
 *  platform, and a copy to disk should not wait on anything else. */
export async function routeCommand(options: RouteOptions, deps: RouteDeps = {}): Promise<RouteAnswer> {
  const { key, epub } = options;
  const open = deps.open ?? realOpener;

  if (key.startsWith('device:') || key === 'remarkable') {
    throw new CliError('usage', 'send to a reader with screepub send; route performs the others');
  }
  if (!ROUTE_KEYS.includes(key)) {
    throw new CliError('usage', `no route is called "${key}": route takes ${ROUTE_KEYS.join(', ')}`);
  }

  if (key === EMAIL_SETUP) {
    if (epub !== undefined) {
      throw new CliError('usage', `${EMAIL_SETUP} takes no book: it opens Amazon's settings page`);
    }
    refuseFlags(options, EMAIL_SETUP, { out: false, build: false });
    let note: string;
    try {
      note = await openKindleEmailSettings(deps.platform ?? process.platform, open);
    } catch (err) {
      throw new CliError('route-failed', errorMessage(err));
    }
    return { key, note };
  }

  if (epub === undefined) {
    throw new CliError('usage', `route ${key} needs the book: screepub route ${key} <file.epub>`);
  }
  requireBook(epub);

  const save = SAVES.has(key);
  if (save && options.out === undefined) {
    throw new CliError('usage', `route ${key} needs --out: the absolute path to save the copy to`);
  }
  refuseFlags(options, `route ${key}`, { out: save, build: key === 'save-kindle' });

  if (save) {
    // exportCommand refuses a relative --out, and a name whose extension is
    // not the file's, before it builds or copies anything.
    const result = await exportCommand(
      {
        epub,
        for: key === 'save-kindle' ? 'kindle' : 'epub',
        fountain: options.fountain,
        optionsJson: options.optionsJson,
        out: options.out,
      },
      { ...deps.exportDeps, appSettingsPath: deps.exportDeps?.appSettingsPath ?? deps.settingsPath },
    );
    rememberRoute(key, deps.settingsPath);
    return { key, path: result.path, note: `Saved to ${result.path}.` };
  }

  const facts = await (deps.facts ?? factsWithoutDevices)();
  const row = routes(facts).find((r) => r.key === key);
  if (row === undefined) {
    throw new CliError('route-unavailable', `${PERFORMED[key]} is not installed on this computer`);
  }
  if (!row.available) {
    throw new CliError('route-unavailable', leadingCapital(row.detail));
  }

  let note: string;
  try {
    if (key === 'apple-books') note = await addToAppleBooks(epub, open);
    else if (key === 'send-to-kindle') note = await sendViaAmazon(epub, facts, open);
    else note = await emailToKindle(epub, open);
  } catch (err) {
    throw new CliError('route-failed', errorMessage(err));
  }
  rememberRoute(key, deps.settingsPath);
  return { key, note };
}
