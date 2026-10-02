// What sending decides, with no drawing in it: the engine's route list and
// Kindle-file answers checked and read, the status line's words, the save
// box's defaults. Moved out of send.js unchanged when the Send page became
// the result screen's panel (spec 2026-10-01); send-panel.js draws with it,
// device.js builds the device model on top of it. If anything here starts to
// look like it knows what a Kindle is, it belongs in src/device/.

/** The project's own stated limit, on the surface rather than buried in a
 *  README: one Kindle, on one Mac, is the whole of what has ever been
 *  plugged in. Everything else — every other vendor, and the same Kindle on
 *  Linux or Windows — is built and code-tested and nothing more. Saying so
 *  is the difference between a promise and a claim. */
export const UNPROVEN = 'never been tested on real hardware';

/** The one combination anyone has actually run. Not a set of kinds: the
 *  platform is half of it. Windows drive enumeration has never met a real
 *  reader either, so a Kindle on Windows is exactly as unproven as a Kobo. */
export const PROVEN = { kind: 'kindle', platform: 'mac' };

export const KINDS = ['kindle', 'kobo', 'tolino', 'remarkable'];

/** Something a person could read: a string with more than spaces in it. */
export const isText = (value) => typeof value === 'string' && value.trim() !== '';

/** Which of the three this window is standing on. navigator.userAgentData is
 *  absent on WebKitGTK and navigator.platform is deprecated, so both can be
 *  undefined in the same window; an unknown platform is not a Mac, which is
 *  the safe way round — it under-claims rather than over-claims. */
export function platformOf(hint) {
  const said = String(hint ?? '');
  if (/mac/i.test(said)) return 'mac';
  if (/win/i.test(said)) return 'windows';
  if (/linux|x11|bsd/i.test(said)) return 'linux';
  // Not 'linux': under-claiming hardware support is right, but naming the
  // wrong operating system is not. A window that could not tell would
  // otherwise print "never tested from Linux" on a Mac.
  return 'unknown';
}

/** A reader Screepub could not find even if it were plugged in. tolino is
 *  identified by the NAME of its volume and a Windows drive root carries
 *  none, so on Windows it is not undetected-so-far, it is undetectable — and
 *  a person hunting for a missing row deserves to be told that rather than
 *  left to conclude their cable is bad. Null everywhere else. */
export function undetectable(platform) {
  if (platformOf(platform) !== 'windows') return null;
  return 'A tolino cannot be found on Windows at all: it is identified by the name of its '
    + 'volume, and a Windows drive root carries none. Windows drive detection as a whole has '
    + `${UNPROVEN}.`;
}

/** Which rung of `screepub export` this device is asking for. A Kindle never
 *  indexes a sideloaded EPUB, so handing it the EPUB would be a known-broken
 *  path dressed up as success; everything else takes the book as built.
 *  Which FILE the Kindle rung then produces is the engine's ladder to walk,
 *  not this window's. */
export function forFormat(device) {
  return device?.kind === 'kindle' ? 'kindle' : 'epub';
}

/** Where the file will land, in words. The volume path is the useful fact
 *  for anything that mounts; a reMarkable has no path because it never
 *  mounts, and printing its kind there would read as a shrug. */
export function whereLine(device) {
  if (typeof device?.volume === 'string' && device.volume.trim() !== '') return device.volume;
  return 'reached over USB, without mounting as a drive';
}

/** Why nothing can be sent for this script yet, or null when it can. The
 *  library artifact is the source of truth: it is what Read previewed and
 *  what Tune rebuilds, and it is the only file this surface is willing to
 *  hand to a transfer. */
export function blockedReason(script) {
  if (script === null || script === undefined) return 'Convert a script first.';
  const epub = typeof script.epubPath === 'string' ? script.epubPath.trim() : '';
  if (epub === '') {
    return 'Screepub has no book on disk for this script, so there is nothing to copy across. '
      + 'Converting it again puts one in the library.';
  }
  return null;
}

/** This script's own tuning, as the engine takes it — never the defaults. A
 *  rebuild with the defaults would silently hand over a book in formatting
 *  the reader never chose, and it would do it without a word on screen. */
export function optionsJsonFor(script) {
  const settings = script?.settings;
  if (typeof settings !== 'object' || settings === null || Array.isArray(settings)) return null;
  if (Object.keys(settings).length === 0) return null;
  return JSON.stringify(settings);
}

const SENT_LABEL = 'Sent to';

/** What the surface is allowed to claim once the engine says it is done.
 *  Two shapes, because `send` reports two: a volume gets a destination path
 *  and the eject warning that goes with it (a Kobo indexes on unplug and a
 *  yanked cable loses the book), while a reMarkable reports an upload and
 *  has no path to name. A malformed success is still a success — the engine
 *  said ok — but it is not allowed to invent a path it was not given. */
export function sentLine(device, answer) {
  const destination = typeof answer?.destination === 'string' ? answer.destination.trim() : '';
  if (answer?.uploaded === true || destination === '') {
    return `${SENT_LABEL} ${device.name}.`;
  }
  return `${SENT_LABEL} ${device.name}: ${destination}. Eject the volume before you unplug it.`;
}

/** What actually went across, in the engine's own words. `label` is
 *  src/export/formats.ts's, so which rung was reached is stated by the code
 *  that chose it. A missing label falls back to the file itself rather than
 *  to a guess about the ladder. */
export function artifactLine(built) {
  const label = typeof built?.label === 'string' ? built.label.trim() : '';
  if (label !== '') return label;
  const path = typeof built?.path === 'string' ? built.path.trim() : '';
  return path === '' ? '' : path;
}

/** Shown only if the engine breaks its own contract and refuses with no
 *  sentence in it. Every real failure renders the engine's words instead —
 *  they are already written for a person. */
export const NO_MESSAGE = 'The engine refused to send without saying why.';

/** The one place that decides what the status line says, so a line drawn at
 *  the start of a send and a line drawn at the end of one cannot disagree.
 *  `bad` is false for every phase but failure: a new attempt after a failed
 *  one has to clear the alarm, not inherit it.
 *
 *  The route phases sit beside the device ones: `opening` names the route
 *  (its title, the engine's word for it), and `saving` is the dialog's copy.
 *  A Kindle file building is not a status: it has its own line and bar
 *  (buildLine()), because one started when the page opened outlives any
 *  press. `done` is a route's own sentence, as `sent` is a device's. */
export function statusFor(phase, { device = null, detail = '', route = null } = {}) {
  const name = device?.name ?? 'the reader';
  if (phase === 'failed') {
    const said = typeof detail === 'string' ? detail.trim() : '';
    return { line: said === '' ? NO_MESSAGE : said, bad: true };
  }
  if (phase === 'sent' || phase === 'done') return { line: detail, bad: false };
  if (phase === 'preparing') return { line: `Getting the book ready for ${name}…`, bad: false };
  if (phase === 'copying') return { line: `Copying it to ${name}…`, bad: false };
  if (phase === 'opening') {
    const title = isText(route?.title) ? route.title.trim() : '';
    return { line: title === '' ? 'Opening…' : `Opening ${title}…`, bad: false };
  }
  if (phase === 'saving') return { line: 'Saving…', bad: false };
  return { line: '', bad: false };
}

/** The engine's own sentence for a refusal, or a readable stand-in when it
 *  breaks its contract and refuses without one. */
export function failureMessage(answer) {
  const said = typeof answer?.error?.message === 'string' ? answer.error.message.trim() : '';
  return said === '' ? NO_MESSAGE : said;
}

/** What the two answers add up to, as the pair statusFor takes.
 *
 *  This is the decision the whole surface turns on, so it is HERE and not in
 *  the drawing: whether a transfer happened. `ok !== true` is the engine's
 *  only word for "it did not", and a surface that failed to check it would
 *  print "Sent to Kindle." over a refusal — sentLine cannot tell, because a
 *  refusal carries no destination and that is also the shape a reMarkable
 *  upload takes. Both answers are checked, and the export's first: a Kindle
 *  whose file could not be built was never copied anywhere. */
export function outcomeFor(built, sent, device) {
  if (built?.ok !== true) return ['failed', { device, detail: failureMessage(built) }];
  if (sent?.ok !== true) return ['failed', { device, detail: failureMessage(sent) }];
  return ['sent', { device, detail: sentLine(device, sent) }];
}

/** Whether this script's settings still have to be fetched before an export.
 *  Tune stores them the moment it opens; a reader who never opened Tune has
 *  a sidecar full of settings and nothing here that knows it, and the rung
 *  that consults them would rebuild the book with the DEFAULTS. */
export function needsSettings(script) {
  if (script === null || script === undefined) return false;
  if (script.settings !== null && script.settings !== undefined) return false;
  const fountain = typeof script.fountainPath === 'string' ? script.fountainPath.trim() : '';
  return fountain !== '';
}

/** Which phase the wait is in before the copy starts. The Kindle rung can
 *  take half a minute of Kindle Previewer, and 'building' puts up the build
 *  line and its bar for it; the EPUB rung is a stat, and so is a Kindle file
 *  the engine's check (`file`, kindleCheckFrom()'s) says is current. Saying
 *  "building" for one that builds nothing would be theatre. */
export function preparingPhase(device, file = null) {
  return forFormat(device) === 'kindle' && mayBuild(file) ? 'building' : 'preparing';
}

/** The turn a Kindle file export takes on its book (book-queue.js's TURNS
 *  says what each does to it). Calibre builds KFX and AZW3 from the EPUB and
 *  writes them beside it: 'kindle', a reader, which runs beside a save of the
 *  EPUB or a copy to a Kobo. The engine's own MOBI rung rewrites the EPUB in
 *  place before it writes the .mobi: 'kindle-mobi', a writer, which runs
 *  alone. With no check to go by (`file` null) the page cannot rule the MOBI
 *  rung out, so it takes the book alone too. */
export function kindleTurn(file) {
  return file?.builtBy === 'calibre' ? 'kindle' : 'kindle-mobi';
}

/** Whether a press's `export --for kindle` may have to build the file, and
 *  so shows the build line and bar while it runs: always, unless the
 *  engine's check said the file beside the book is current, when the export
 *  only finds it. */
export function mayBuild(file) {
  return file?.fresh !== true;
}

/** Whether the send panel starts building the Kindle file as the result appears,
 *  before anyone presses anything, so a press finds it built or building.
 *  Every condition, in one place:
 *  - the engine's check (kindleCheckFrom()) says the file is out of date:
 *    not current, and not merely unknown;
 *  - Calibre is what builds it (KFX, AZW3): a machine with no Calibre never
 *    has it started, and the engine's own MOBI rewrites the library EPUB in
 *    place in a moment, so building it ahead buys nothing;
 *  - a Kindle is in play (`forKindle`, wantsKindleFile()): a Kobo or a
 *    reMarkable session must not start Kindle Previewer for nobody;
 *  - once per version of the book and kind of file: never again for the
 *    attemptKey() a build was already started for (`triedFor`), so a build
 *    that failed is not retried every time the window gets the focus back,
 *    while a KFX wanted after an AZW3 was built ahead (the KFX plugin just
 *    installed) still is; a file with no book date to tell versions apart by
 *    is never started;
 *  - not while one is running for the book (`building`), nor while a send,
 *    a route or a KFX install is (`busy`). */
export function buildsAhead(file, {
  building = false, busy = false, triedFor = null, forKindle = false,
} = {}) {
  if (file === null || file === undefined) return false;
  if (file.fresh !== false || file.builtBy !== 'calibre') return false;
  if (typeof file.bookDate !== 'number' || attemptKey(file) === triedFor) return false;
  return forKindle === true && !building && !busy;
}

/** What one build ahead is an attempt at: this kind of file, from this
 *  version of the book. */
export function attemptKey(file) {
  return `${file?.extension}@${file?.bookDate}`;
}

/** The line over the moving bar while the Kindle file builds, by the file
 *  being built. Kindle Previewer's cold start is most of a KFX build: 23 s
 *  for the 18-page demo, measured, and the owner saw 30 to 40 on a longer
 *  script. Calibre's AZW3 and the engine's MOBI took under a second there.
 *  With no check to go by, it says only what it is doing. */
export function buildLine(extension) {
  if (extension === 'kfx') return 'Building the Kindle file. This takes about half a minute.';
  if (isText(extension)) return 'Building the Kindle file. This takes a moment.';
  return 'Building the Kindle file…';
}

// Routes: every way the book can leave, as `screepub routes` lists them.
//
// The catalog is the ENGINE's (src/export/routes.ts): which routes exist
// here, their order, their words, which one is chosen, and why a dimmed one
// cannot fire. What follows only checks that answer's shape and says what
// the window does with each row. It never ranks, renames or hides one; if
// anything here starts to decide which route is better, it belongs there.

/** Why a listed row cannot fire, in the engine's three words. */
const UNAVAILABLE = new Set(['connect', 'platform', 'setup']);

/** A connected device as a row carries it, rebuilt clean, or null. An entry
 *  with no id cannot be addressed by `send` at all, and one with no name has
 *  nothing to put on its row. A kind this window has never heard of is KEPT:
 *  the engine is the authority on what a device is, and a future vendor must
 *  not be invisible here just because this file is older than it; it gets
 *  the unproven caveat, which is the truth. `volume` has to be THERE (a
 *  string, or null for a reMarkable, which JSON keeps), because a missing one
 *  means the engine changed its shape. */
function routeDeviceFrom(device) {
  if (typeof device !== 'object' || device === null || Array.isArray(device)) return null;
  if (!isText(device.id) || !isText(device.kind) || !isText(device.name)) return null;
  if (typeof device.volume !== 'string' && device.volume !== null) return null;
  return { id: device.id, kind: device.kind, name: device.name, volume: device.volume };
}

/** One row, rebuilt clean, or null. The engine's rules, each one checked:
 *  `unavailable` exactly when the row is not available, and a `device`
 *  exactly on an available row that sends to one (a device row with no
 *  device has nothing to hand to `send`; a device on any other row means
 *  the list is not the shape the engine promises). */
function routeFrom(route) {
  if (typeof route !== 'object' || route === null || Array.isArray(route)) return null;
  const { id, key, title, detail, button, available } = route;
  if (![id, key, title, detail, button].every(isText)) return null;
  if (typeof available !== 'boolean') return null;
  if (available && route.unavailable !== undefined) return null;
  if (!available && !UNAVAILABLE.has(route.unavailable)) return null;
  const sendsToDevice = available && isDeviceRoute(route);
  if ((route.device !== undefined) !== sendsToDevice) return null;
  const clean = { id, key, title, detail, button, available };
  if (!available) clean.unavailable = route.unavailable;
  if (sendsToDevice) {
    const device = routeDeviceFrom(route.device);
    if (device === null) return null;
    clean.device = device;
  }
  return clean;
}

/** `routes --json`'s answer as `{ routes, chosen }`, or null.
 *
 *  Taken whole or not at all, the way kfx.js takes its checklist: a list
 *  with one broken row is not drawn with that row missing, because the rows
 *  ARE the page and a quietly shorter list reads as a route that does not
 *  exist. Null sends the page to the engine's failure line instead. The
 *  copies are rebuilt field by field, so nothing the engine happened to
 *  attach rides along into the drawing, and the engine's words are kept
 *  exactly as written (an unavailable row's detail is its fix, shown as is).
 *  Ids are unique and `chosen` is one of them: an id is what the page tells
 *  rows apart by and what gets the brass button. */
export function routesFrom(answer) {
  if (answer?.ok !== true || !Array.isArray(answer.routes)) return null;
  const list = answer.routes.map(routeFrom);
  if (list.some((route) => route === null)) return null;
  const ids = new Set(list.map((route) => route.id));
  if (ids.size !== list.length) return null;
  if (!ids.has(answer.chosen)) return null;
  return { routes: list, chosen: answer.chosen };
}

/** Whether a row goes through `export` then `send`, the device flow with its
 *  own phases: a volume reader of any kind (a kind this file has never heard
 *  of included, as routeDeviceFrom() keeps one), or a docked reMarkable. */
export function isDeviceRoute(route) {
  const key = route?.key;
  if (typeof key !== 'string') return false;
  return key.startsWith('device:') || key === 'remarkable';
}

/** Whether a row's button hands over the Kindle file, whose type depends on
 *  this computer (KFX, AZW3 or MOBI): Save a Kindle file, and a Kindle that
 *  is plugged in. A dimmed Kindle row carries no device and has nothing to
 *  press. */
export function handsOverKindleFile(route) {
  if (route?.key === 'save-kindle') return true;
  const device = route?.device;
  return typeof device === 'object' && device !== null && forFormat(device) === 'kindle';
}

/** What `export --for kindle --check` says about this script's Kindle file
 *  before anything is built: `{ extension, fresh, builtBy }`, or null when
 *  the answer is not one (a refusal included), which names nothing on the
 *  rows. `fresh` is the engine's staleness rule, `builtBy` what makes the
 *  file when it is not: 'calibre' for KFX and AZW3, 'screepub' for the
 *  engine's own MOBI; `bookDate` when the EPUB was last written. */
export function kindleCheckFrom(answer) {
  if (answer?.ok !== true) return null;
  const extension = typeof answer.extension === 'string' ? bareExtension(answer.extension.trim()) : '';
  if (!/^[A-Za-z0-9]+$/.test(extension)) return null;
  if (typeof answer.fresh !== 'boolean') return null;
  if (answer.builtBy !== 'calibre' && answer.builtBy !== 'screepub') return null;
  // When the book was last written: which version of it this answer is
  // about. Not known is not a reason to drop the rest of the answer; it only
  // means the page will not build ahead on it (buildsAhead()).
  const bookDate = Number.isFinite(answer.bookDate) ? answer.bookDate : null;
  return { extension, fresh: answer.fresh, builtBy: answer.builtBy, bookDate };
}

/** Used only when a path has no name in it at all, so the dialog never
 *  starts on a bare ".epub", which a Mac would hide. */
const SAVE_FALLBACK = 'Screenplay';

const bareExtension = (extension) => String(extension ?? '').replace(/^\./, '');

/** The save dialog's default name: the book's own stem with the extension of
 *  the file being saved. The stem comes from the FILE name (a folder called
 *  "v1.2" must not lend it a dot), with either separator, because the window
 *  runs on Windows too. Only the last extension goes, so a title with dots
 *  in it keeps them. */
export function saveNameFor(epubPath, extension) {
  const path = typeof epubPath === 'string' ? epubPath : '';
  const file = path.split(/[/\\]/).pop() ?? '';
  const dot = file.lastIndexOf('.');
  const stem = dot > 0 ? file.slice(0, dot) : file;
  return `${stem === '' ? SAVE_FALLBACK : stem}.${bareExtension(extension)}`;
}

/** The save dialog's one filter. The label is the engine's name for the
 *  file; a filter with no label is named by its extension rather than left
 *  as a blank line in the dialog. */
export function saveFiltersFor(extension, label) {
  const bare = bareExtension(extension);
  const name = isText(label) ? label.trim() : bare.toUpperCase();
  return [{ name, extensions: [bare] }];
}

/** Stand-in for a route the engine says worked but did not describe. It
 *  claims exactly what is known. "Done." would claim more: the window does
 *  not know what the route did, only that the engine said ok. */
export const NO_NOTE = 'The engine said it worked, without saying what it did.';

/** What a route's answer (`route --json`) adds up to, as the pair statusFor
 *  takes, the way outcomeFor() does for a device. `ok !== true` is the
 *  failure path, in the engine's words, whatever else the answer carries: a
 *  note on a refusal is not a success. */
export function routeNoteFrom(answer) {
  if (answer?.ok !== true) return ['failed', { detail: failureMessage(answer) }];
  return ['done', { detail: isText(answer.note) ? answer.note.trim() : NO_NOTE }];
}

/** The email route's first-time step. Amazon throws away mail from a sender
 *  it has not been told to accept, and says nothing, so a first send that
 *  "worked" never arrives. The Swift app offered a guide to Amazon's
 *  Personal Document Settings page, where the Kindle's address lives and the
 *  approved list is edited; this is that offer. `key` is what the engine
 *  opens the page by: the window names no URL. Shown on the email row
 *  whether or not it can fire here, because attaching the EPUB by hand
 *  needs the same approval. */
export function emailSetupHint(route) {
  if (route?.key !== 'email-to-kindle') return null;
  return {
    line: 'First time? Amazon needs your sender address approved, or it drops the email without a word.',
    key: EMAIL_SETUP,
  };
}

/** The key the email row's setup link is performed by: the same one
 *  app.js's `argv.emailSetup()` hands the engine. */
const EMAIL_SETUP = 'kindle-email-setup';

/** The email row's setup link, and the name it is opened by on the status
 *  line ("Opening Amazon’s page…"). */
export const SETUP_BUTTON = 'Open Amazon’s page';

export const SETUP_TITLE = 'Amazon’s page';

/** Which flow a row's button runs. A device row (a reader over USB, or a
 *  docked reMarkable) goes through `export` then `send`, sendTo() below,
 *  with its own phases. Each save is its own flow, because each asks where
 *  first. The email row's setup link opens Amazon's page. Every other key
 *  is a route the ENGINE performs (it opens Books, Amazon's app or page, or
 *  Mail), including one this window is older than: the engine performs it
 *  or refuses in its own words, which beats a button that does nothing. */
export function performerFor(route) {
  if (isDeviceRoute(route)) return 'device';
  const key = route?.key;
  if (!isText(key)) return null;
  if (key === 'save-epub' || key === 'save-kindle') return key;
  if (key === EMAIL_SETUP) return 'setup';
  return 'open';
}

/** Whether a flow's engine call reads or writes this script's library EPUB,
 *  and so has to take the book's turn (onBook(), below). A device send and a
 *  save do: they export or copy the book itself. A route that opens another
 *  program (Books, Mail, Amazon's app or page) does not: the engine hands
 *  that program the book's path and returns, reading nothing of it. It also
 *  must not hold the book, because the Settings page's saves wait for
 *  whatever does, and on Linux the Send to Kindle route waits on xdg-open
 *  for the book's folder, which can last until the reader closes the file
 *  manager. Amazon's setup page needs no book at all. */
export function readsTheBook(how) {
  return how === 'device' || how === 'save-epub' || how === 'save-kindle';
}

/** What is plugged in, read off the route list: the device of every row
 *  that carries one (routesFrom() lets a device ride only on an available
 *  device row), in the engine's order, as copies. This is what
 *  `ctx.state.devices` and the KFX block's relevance read, as they read
 *  `devices` before routes. Null when there is no list, which the KFX block
 *  takes as "not known yet" rather than "nothing connected". */
export function connectedDevices(shown) {
  if (!Array.isArray(shown?.routes)) return null;
  return shown.routes.filter((route) => route.device !== undefined)
    .map((route) => ({ ...route.device }));
}

/** Whether the list on screen is still the list the engine just described.
 *  The poll runs every two seconds; rebuilding the rows on every tick would
 *  steal the focus out from under anyone tabbing to a button and flicker the
 *  row they were about to press. Everything a row draws counts (id,
 *  availability, detail, title, button, device) and so does which one is
 *  chosen, since that is where the brass goes. Both lists are routesFrom()'s
 *  clean copies, built field by field in one order, so comparing them as
 *  text compares exactly what is drawn. */
export function sameRoutes(a, b) {
  if (!Array.isArray(a?.routes) || !Array.isArray(b?.routes)) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

/** What a row says: its title and the engine's detail (the mechanism when it
 *  can fire, the fix when it cannot), and on a connected device row that
 *  mounts, the volume the file lands on. A docked reMarkable never mounts
 *  and gets no such line: the engine's detail already says how it is reached, and whereLine()'s stand-in beside
 *  it said the same thing twice. The title is the route's own, and a
 *  reader's is the name it mounts as: the Kindle file's type goes on the
 *  button (buttonLabel()), which is what hands the file over. */
export function routeLines(route, platform) {
  const device = route?.device ?? null;
  return {
    title: route?.title,
    detail: route?.detail,
    where: device !== null && isText(device.volume) ? whereLine(device) : null,
  };
}

/** What a row's button says: the engine's words, and on a button that hands
 *  over the Kindle file, its type, because which file that is depends on
 *  this computer: "Copy to Kindle (.kfx)", "Save a Kindle file (.kfx)…".
 *  Before the ellipsis a button that opens a Save box ends with. `file` is
 *  kindleCheckFrom()'s answer; with none the words stand alone rather than
 *  guessing. */
export function buttonLabel(route, file = null) {
  const words = route?.button;
  if (file === null || file === undefined || !handsOverKindleFile(route)) return words;
  const type = `(.${file.extension})`;
  return words.endsWith('…') ? `${words.slice(0, -1)} ${type}…` : `${words} ${type}`;
}

/** Stand-in when the export says it built the Kindle file but not what kind
 *  of file it is, so there is nothing to name the Save box's file by. */
export const NO_EXTENSION = 'The engine built the Kindle file without saying what kind of file it is.';

/** What "Save a Kindle file" learns from `export --for kindle`, as a pair
 *  like outcomeFor()'s: the extension and the engine's label, to name the
 *  Save box's file and filter, or the failure line. The export decides the
 *  rung (KFX, AZW3 or MOBI); this only reads which one it reached. An
 *  extension that is not a bare word opens no dialog at all. */
export function kindleFileFrom(built) {
  if (built?.ok !== true) return ['failed', { detail: failureMessage(built) }];
  const extension = typeof built.extension === 'string' ? bareExtension(built.extension.trim()) : '';
  if (!/^[A-Za-z0-9]+$/.test(extension)) return ['failed', { detail: NO_EXTENSION }];
  return ['built', { extension, label: isText(built.label) ? built.label.trim() : null }];
}

/** Shown in place of the list when `routes` said ok in a shape routesFrom()
 *  will not draw. Not NO_MESSAGE: the engine did not refuse anything. */
export const NO_ROUTES = 'The engine listed the ways to send this book in a shape this window cannot read.';

/** The line in place of the list: the engine's own sentence for a refusal,
 *  or NO_ROUTES for an ok answer that could not be drawn. */
export function routesFailure(answer) {
  return answer?.ok === true ? NO_ROUTES : failureMessage(answer);
}

/** The file name at the end of a path, either separator. */
export function baseName(path) {
  const parts = String(path ?? '').split(/[\\/]/).filter((p) => p !== '');
  return parts.length === 0 ? '' : parts[parts.length - 1];
}
