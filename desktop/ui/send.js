// Send: every way the book can leave, and the file each one should get.
//
// The window knows nothing about devices or routes. `screepub routes` lists
// every way out in its own order (what is plugged in among them) and which
// one was used last, `screepub export` says which file a Kindle should get
// (KFX → AZW3 → MOBI, the best rung this machine can reach), `screepub send`
// moves it to a reader, and `screepub route` performs the rest: it opens
// Apple Books, Amazon or Mail, or writes a copy where the Save box said.
// Each of those is tested TypeScript shared with the CLI; this file draws
// their answers and never re-decides them. If anything here starts to look
// like it knows what a Kindle is, it belongs in src/device/.
//
// The file is in two halves, the way convert.js, read.js and tune.js are.
// Everything this surface DECIDES — which devices are offerable, which
// rung applies, what a refusal means, what "sent" is allowed to claim, and
// what is honest to say about hardware nobody here has ever plugged in — is
// a pure exported function, most of them now in send-routes.js and the rest
// above the line here, tested directly by tests/desktop-ui.test.ts and
// tests/send-routes-ui.test.ts. Below the line is drawing, which holds no
// rule of its own and rides on the live run.
import { runEngine, argv, saveDialog } from './app.js';
import { settingsFrom } from './tune.js';
import { inTurn } from './book-queue.js';
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';
import {
  HEADING, mountKfx, kfxShown, kfxHidden, kfxDevicesChanged, kfxRedraw, kfxInstalling,
} from './kfx.js';
import {
  UNPROVEN, PROVEN, platformOf, undetectable, forFormat, blockedReason, optionsJsonFor,
  artifactLine, statusFor, outcomeFor, needsSettings, preparingPhase, kindleTurn, mayBuild,
  buildsAhead, attemptKey, buildLine, routesFrom, kindleCheckFrom, saveNameFor, saveFiltersFor,
  routeNoteFrom, emailSetupHint, SETUP_BUTTON, SETUP_TITLE, performerFor, readsTheBook,
  connectedDevices, sameRoutes, routeLines, buttonLabel, kindleFileFrom, NO_ROUTES,
  routesFailure,
} from './send-routes.js';

// ---------------------------------------------------------------- decisions

/** Short form, for the standing list of readers. Same fact, column width. */
export const UNPROVEN_SHORT = 'Never run on real hardware';
export const PROVEN_LABEL = 'Verified on hardware';

/** Every reader Screepub can reach, and how — shown whether or not one is
 *  connected, because with nothing plugged in this list IS the surface. The
 *  route is stated in full: "a Kindle gets an EPUB" would be wrong in a way
 *  a reader could not discover until the book failed to appear. */
export const READERS = [
  {
    kind: 'kindle',
    name: 'Kindle',
    // The ladder in its own order, best rung first: KFX, then AZW3, then the
    // engine's MOBI. Listing it any other way would read as a preference the
    // engine does not hold.
    route: 'Over USB. Kindles never index a sideloaded EPUB, so Screepub builds a KFX, '
      + 'an AZW3 or its own MOBI, whichever is the best this computer can make, and copies '
      + 'that across instead.',
  },
  {
    kind: 'kobo',
    name: 'Kobo',
    route: 'Over USB. The EPUB, copied to the volume, where the reader indexes it when you '
      + 'eject.',
  },
  {
    kind: 'tolino',
    name: 'tolino',
    route: 'Over USB. The EPUB, into the Books folder at the root of the volume.',
  },
  {
    kind: 'remarkable',
    name: 'reMarkable',
    route: 'Over the tablet’s own USB web interface, which is why it never turns up as '
      + 'a drive. The EPUB, into the root folder.',
  },
];

/** What the standing list says in its third column. */
export function readerStatus(kind, platform) {
  const proven = kind === PROVEN.kind && platformOf(platform) === PROVEN.platform;
  return { proven, text: proven ? PROVEN_LABEL : UNPROVEN_SHORT };
}

/** The one line under the standing list. Without it, four identical red
 *  statuses in a column read as a warning wall; with it they read as a
 *  statement, which is what they are. It also carries the fact the statuses
 *  alone cannot: WHERE the one proven route was proven. */
export function provenNote(platform) {
  const where = platformOf(platform);
  if (where === PROVEN.platform) {
    return 'A Kindle over USB, on a Mac, is the one route anyone has actually run. The others '
      + 'are built and code-tested, and that is all. They are listed because Screepub will '
      + 'try, not because anyone can promise.';
  }
  // Named only when it is known. An absent navigator.platform must not be
  // reported as some particular operating system.
  const named = where === 'windows' ? 'from Windows'
    : where === 'linux' ? 'from Linux'
      : 'on this computer’s platform';
  return `Sending has ${UNPROVEN} ${named}. The one route anyone has run was a Kindle `
    + 'over USB, on a Mac. The code is the same on all three platforms; the confidence is not.';
}

/** The sentence beside a device that is actually connected right now, or
 *  null when there is nothing to warn about. Kind AND platform, for the same
 *  reason PROVEN carries both. */
export function caveatFor(device, platform) {
  const where = platformOf(platform);
  const name = typeof device?.name === 'string' && device.name.trim() !== ''
    ? device.name.trim()
    : 'this reader';
  if (device?.kind === PROVEN.kind) {
    if (where === PROVEN.platform) return null;
    return `Sending to a Kindle has ${UNPROVEN} on this computer’s platform. It has only been run on a Mac.`;
  }
  return `Sending to a ${name} has ${UNPROVEN}. It is built and code-tested; nobody has `
    + 'plugged one in.';
}

export const SEND_LABEL = 'Send to';

/** The verb stays constant through the flow: the control that says Send
 *  produces a result that says Sent. */
export function sendLabel(device) {
  return `${SEND_LABEL} ${device.name}`;
}

/** Whether this is a Kindle session, read off the route list: the route
 *  the page would choose (the one used last time, or the first that can
 *  fire) is a Kindle or Save a Kindle file, or a Kindle is plugged in now. */
export function wantsKindleFile(shown) {
  if (!Array.isArray(shown?.routes)) return false;
  if (shown.routes.some((route) => route.device?.kind === 'kindle')) return true;
  const chosen = shown.routes.find((route) => route.id === shown.chosen);
  return chosen?.key === 'save-kindle' || chosen?.key === 'device:kindle';
}

/** The button a row gets: brass for the chosen route, outline for every
 *  other one that can fire, and none at all for a row that cannot. The rows
 *  keep the engine's order whichever is chosen; the brass is what marks the
 *  choice. A chosen row that is unavailable (the Kindle you used last time,
 *  unplugged) stays where the engine put it and waits without a button. */
export function buttonClassFor(route, chosenId) {
  if (route?.available !== true) return null;
  return route.id === chosenId ? 'btn btn-brad' : 'btn btn-outline';
}

/** The folded table of every reader and how it is reached, under the list.
 *  "Nothing plugged in" used to stand above it; the dimmed device rows now
 *  say that per reader, with the fix ("plug in over USB to send"). */
export const REACH_HEADING = 'What Screepub can reach';

/** Not "nothing leaves this computer" any more: that was true when every row
 *  was a cable, and Send to Kindle and email go through Amazon. The device
 *  rows still say it, in the engine's words, where it is true. */
export const LEDE = 'Every way this book can go from here, the one you used last time, or the best one here, in brass. '
  + 'A reader plugged in over USB gets the file it can actually open.';

/** Shown until the first `routes` answer arrives. */
export const LOOKING = 'Looking for every way to send it…';

export const NO_SCRIPT = {
  slug: 'Nothing to send yet',
  line: 'Convert a script and it can go to a reader from here.',
};

// ------------------------------------------------------------------ drawing

let ctx = null;
let pane = null;
let list = null;
let statusLine = null;
let artifactNote = null;
let poll = null;
/** The route list on screen (routesFrom()'s answer), or null when none is:
 *  still looking, or a failure line in its place. */
let drawn = null;
/** Polls begun, and the newest one whose answer counts. An answer asked
 *  before one already drawn is older than what is on screen and is dropped,
 *  so a slow poll cannot put back a list a faster, later one replaced. */
let asks = 0;
let newest = 0;
/** Full asks (the reMarkable probe included) whose answer is still out. A
 *  tick of the poll while one is out asks nothing: each can take the probe's
 *  whole timeout, and piling a second on top of it would only add another
 *  engine run to wait for. */
let fullOut = 0;
/** What the engine last said about this script's Kindle file
 *  (kindleCheckFrom()), or null when it has not said, and the book it said
 *  it about: kept across a hide and a show of the same book, so the rows do
 *  not lose their file type for the moment the check takes to answer again. */
let kindle = null;
let kindleBook = null;
/** The extension the buttons on screen were labelled with, so a check that
 *  changes it redraws them and one that does not leaves them alone. */
let labelledWith = null;
/** Kindle file exports of one book, one at a time: the tail of each book's
 *  line. Two at once would run Kindle Previewer twice over the same scratch
 *  file. This is a rule about the Kindle FILE, not the book, so it is kept
 *  here and not in book-queue.js's readers and writers: a Kindle export
 *  takes its place in this line first and its turn on the book after, so
 *  one waiting here holds no turn on the book, and a Settings save that
 *  arrives meanwhile goes ahead of it and is in the file it builds. */
const kindleLines = new Map();

/** Run `work` once every Kindle export of `book` ahead of it has finished,
 *  however it finished, and hand back its own promise. With none ahead it
 *  starts in the caller's own tick. */
function inKindleLine(book, work) {
  const ahead = kindleLines.get(book);
  let run;
  if (ahead === undefined) {
    try {
      run = Promise.resolve(work());
    } catch (err) {
      run = Promise.reject(err);
    }
  } else {
    run = ahead.then(work);
  }
  const tail = run.then(() => {}, () => {});
  kindleLines.set(book, tail);
  tail.then(() => { if (kindleLines.get(book) === tail) kindleLines.delete(book); });
  return run;
}

/** A Kindle file export of `book` (or the save of one, which exports again):
 *  in the Kindle line, then on its turn on the book as kindleTurn() says. A
 *  press during a build therefore waits for that build, then finds the file
 *  it made current (the engine's freshness rule) rather than building
 *  another. */
function onKindleFile(book, args) {
  const label = kindleTurn(kindle);
  return inKindleLine(book, () => onBook(book, label, args));
}

/** Kindle file builds out now, by book: `{ runs, file }`, how many exports
 *  that may build it are out (one started as the page opened, a press queued
 *  behind it) and what the check said the file is. Kept at module level and
 *  not with the page, because a build outlives a redraw, a new script and a
 *  hidden page, and a page that comes back to its book must find it still
 *  going rather than start another. */
const builds = new Map();
/** What each book's last background build was started for (attemptKey()),
 *  so the page builds ahead once per version of a book and kind of file
 *  (buildsAhead()'s `triedFor`).
 *  At module level, as `builds` is: a hide and a show, or a new script and
 *  back, is still the same version of the book. */
const triedFor = new Map();
/** From show() to hide(): the page may build ahead, when the check and the
 *  route list both say so. Either can answer first, so each looks again
 *  when it lands (maybeBuildAhead()). */
let aheadWanted = false;
/** The build line and its bar, and the line's own node. */
let buildNote = null;
let buildWords = null;
/** A send or a route in flight (sendTo() and perform() share it). Two at
 *  once would be a genuine race and not a cosmetic one: the MOBI rung of
 *  src/export/artifact.ts REWRITES the library EPUB in place before it
 *  writes the .mobi beside it, and a save of the EPUB could copy it half
 *  written. So the whole list goes dead for the duration, and the poll stops
 *  rebuilding rows underneath it: a redraw mid-send would hand back a
 *  fresh, enabled button and let a second one start. */
let sending = false;
/** Which script this surface is showing. A send already in flight cannot be
 *  cancelled — the engine has been asked — but it must not SAY anything once
 *  the script under it has been replaced, or "Sent to Kindle." lands in the
 *  status line beside a script that was never sent. Same shape as tune.js's
 *  `era`. */
let era = 0;

/** Under a person's patience, well above the cost of a mount scan. */
const POLL_MS = 2000;

export function mount(node, context) {
  pane = node;
  ctx = context;
  draw();
}

export function scriptChanged() {
  era += 1;
  aheadWanted = false;
  drawn = null;
  draw();
}

export function show() {
  draw();
  window.addEventListener('focus', onFocus);
  // Two asks at once. The quick one leaves out the reMarkable probe, which
  // runs to its whole timeout (a second and a half) on every machine with no
  // tablet docked, so the rows are on screen as soon as the mount scan
  // answers, with the reMarkable row saying it is still checking. The full
  // one fills that row in. Asked second, the full answer is the newer one,
  // so refresh()'s ordering drops the quick answer if it lands last.
  refresh({ quick: true });
  refresh();
  // A reader plugs something in while looking at this surface; the list has
  // to notice on its own. Cleared first: a second show() without an
  // intervening hide() would otherwise leave the first interval running
  // forever with nothing holding its handle.
  clearInterval(poll);
  poll = setInterval(tick, POLL_MS);
  kfxShown();
  // What the last visit learnt is kept (the buttons keep naming the file), but
  // not whether the file is current: a Settings save may have rewritten the
  // book since, and a press before the check answers must put the build
  // line up rather than trust a send from before. Unknown, not stale, so
  // nothing is built ahead on it either.
  if (kindle !== null) kindle = { ...kindle, fresh: null };
  aheadWanted = true;
  checkKindle();
}

/** Back from somewhere else, perhaps from installing Kindle Previewer: the
 *  Kindle file may be a different kind now, so the buttons are named again.
 *  Only named: focus comes back after every Save box and every switch of
 *  app, and it never starts a build. kfx.js re-asks its checklist on the
 *  same event. */
function onFocus() {
  checkKindle({ ahead: false });
}

/** One tick of the poll: a full ask, unless the last one has not answered. */
function tick() {
  if (fullOut > 0) return;
  refresh();
}

export function hide() {
  clearInterval(poll);
  poll = null;
  aheadWanted = false;
  window.removeEventListener('focus', onFocus);
  kfxHidden();
}

function draw() {
  clear(pane);
  list = null;
  statusLine = null;
  artifactNote = null;
  buildNote = null;
  buildWords = null;
  drawn = null;
  if (ctx.state.script?.epubPath !== kindleBook) {
    kindle = null;
    kindleBook = null;
  }

  if (!ctx.state.script) {
    pane.append(
      el('h2', { class: 'slug' }, NO_SCRIPT.slug),
      el('p', { class: 'prose' }, NO_SCRIPT.line),
    );
    return;
  }

  const blocked = blockedReason(ctx.state.script);
  if (blocked !== null) {
    pane.append(
      el('h2', { class: 'fault' }, 'No book to send'),
      el('p', { class: 'fault-body' }, blocked),
      el('div', { class: 'read-ways' },
        el('button', {
          type: 'button', class: 'btn btn-outline', onclick: () => ctx.convertAgain(),
        }, 'Convert it again')),
    );
    return;
  }

  list = el('div', { class: 'devices' },
    el('p', { class: 'caption' }, LOOKING));
  statusLine = el('p', { class: 'caption send-status', role: 'status' }, '');
  // No percent to show: the engine says nothing while Kindle Previewer
  // works, so the bar sweeps rather than fills. The line is what is read
  // out; the bar is there to be seen moving.
  buildWords = el('p', { class: 'caption kindle-build-line' }, '');
  buildNote = el('div', { class: 'kindle-build', role: 'status' },
    buildWords,
    el('div', { class: 'track', 'aria-hidden': 'true' }, el('div', { class: 'build-sweep' })));
  buildNote.hidden = true;
  artifactNote = el('p', { class: 'caption send-artifact' }, '');
  artifactNote.hidden = true;
  const kfxNode = el('section', { class: 'kfx-setup', 'aria-label': HEADING });
  kfxNode.hidden = true;

  pane.append(
    el('h2', { class: 'slug' }, 'Send it'),
    el('p', { class: 'prose' }, LEDE),
    list,
    buildNote,
    statusLine,
    artifactNote,
    kfxNode,
    reach(),
  );
  // After the append: kfx.js draws only into a node that is in the page.
  showBuild();
  mountKfx(kfxNode, {
    // A build counts: the plugin must not be swapped under a KFX conversion.
    isSending: () => sending || builds.size > 0,
    // What is connected, read off the route list: the block's relevance
    // (Kindle advice is for Kindles) reads it as it read `devices`. Null
    // until the first answer, so the block does not flash up and vanish.
    devices: () => connectedDevices(drawn),
    onBusy: (on) => {
      for (const button of buttons()) button.disabled = on;
      // An install just ended: the Kindle file may be a KFX from now on,
      // and on a Kindle session it is worth starting.
      if (!on) checkKindle();
    },
    // Its redraws hand the keyboard back through the same plan as every
    // other surface's (focus.js), when a control it held is gone.
    restoreFocus: () => ctx.restoreFocus(),
  });
}

/** Every route this book can take, as `screepub routes` lists them, asked
 *  again every two seconds so a reader plugged in turns up on its own.
 *  `quick` is the page's first ask only (show()): the same list without the
 *  reMarkable probe. */
async function refresh({ quick = false } = {}) {
  if (list === null) return;
  // Rows must not be rebuilt out from under a send in progress.
  if (sending) return;
  const into = list;
  const mine = ++asks;
  if (!quick) fullOut += 1;
  let answer;
  try {
    answer = await runEngine(argv.routes(ctx.state.script.epubPath, { quick }));
  } catch (err) {
    // The same three reasons to drop an answer as below.
    if (list !== into || sending || mine <= newest) return;
    newest = mine;
    fault(err.message);
    return;
  } finally {
    if (!quick) fullOut -= 1;
  }
  // A new script's page, a send begun since, or a newer answer already
  // drawn: this one is out of date, and drawing it would undo something.
  if (list !== into || sending || mine <= newest) return;
  newest = mine;
  const shown = routesFrom(answer);
  if (shown === null) {
    fault(routesFailure(answer));
    return;
  }
  ctx.state.devices = connectedDevices(shown);
  // Nothing changed, so nothing is redrawn: a rebuild every two seconds
  // would take the focus off a button someone had just tabbed to.
  if (sameRoutes(drawn, shown) && labelledWith === (kindle?.extension ?? null)) return;
  drawn = shown;
  fillRows();
  // After the rows, not before: a KFX block that hides now while it held
  // the keyboard hands it to the page's first stop, which should be a row.
  kfxDevicesChanged();
  // The list decides whether a Kindle is in play: it may be the answer a
  // build ahead was waiting for, or a Kindle just plugged in.
  maybeBuildAhead();
}

/** The rows, from the list on screen and what the check said about the
 *  Kindle file. */
function fillRows() {
  labelledWith = kindle?.extension ?? null;
  rebuild(() => {
    for (const route of drawn.routes) list.append(routeRow(route, drawn.chosen));
  });
}

/** Ask the engine about this script's Kindle file (`export --check`): which
 *  file this computer makes, and whether the one beside the book is current.
 *  It takes its turn on the book (book-queue.js), so a save still settling
 *  on the Settings page lands first and the answer is about the book as it
 *  will be. A check that fails names nothing: the buttons keep their plain
 *  words, and a press still builds and says what went wrong in its own
 *  words. `ahead` false names the file and nothing more: the check on a
 *  focus return and the one after a build never start a build. */
async function checkKindle({ ahead = true } = {}) {
  const script = ctx.state.script;
  if (list === null || blockedReason(script) !== null) return;
  const book = script.epubPath;
  // A build already out for this book knows what it is building, and until
  // it lands the check could only say "out of date".
  const running = builds.get(book);
  if (running !== undefined) {
    if (running.file !== null) useKindle(book, running.file);
    return;
  }
  const mine = era;
  let answer = null;
  try {
    answer = await onBook(book, 'check', argv.kindleCheck(book));
  } catch {
    // As a refusal: nothing known.
  }
  if (era !== mine || list === null) return;
  useKindle(book, kindleCheckFrom(answer));
  if (ahead) maybeBuildAhead();
}

/** Start the build ahead if everything buildsAhead() asks for is so right
 *  now, on a page that is on screen with its list drawn. */
function maybeBuildAhead() {
  if (!aheadWanted || poll === null || drawn === null) return;
  const script = ctx.state.script;
  const book = script?.epubPath;
  if (kindleBook !== book) return;
  if (!buildsAhead(kindle, {
    building: builds.has(book),
    busy: sending || kfxInstalling(),
    triedFor: triedFor.get(book) ?? null,
    forKindle: wantsKindleFile(drawn),
  })) return;
  triedFor.set(book, attemptKey(kindle));
  buildInBackground(script);
}

/** What is known about this book's Kindle file, onto the buttons. */
function useKindle(book, file) {
  kindle = file;
  kindleBook = book;
  // Rows are never rebuilt under a send; the poll after it catches up.
  if (drawn !== null && !sending && labelledWith !== (kindle?.extension ?? null)) fillRows();
}

/** Build the Kindle file now, on the book's turn, so a press finds it built
 *  or building. It is an engine call like any other, so an update's restart
 *  waits for it (app.js counts every `export` that builds). A build that
 *  fails says nothing here: nobody asked for it, and a press builds again
 *  and says what went wrong in the engine's own words.
 *
 *  One that worked is followed by a check, not by taking the file as
 *  current: a Settings save that waited behind the build rebuilds the book
 *  right after it, and a press that trusted the build would then rebuild the
 *  Kindle file with no line up. The check waits behind that save and says.
 *  It never starts a build of its own: a file the engine still calls out of
 *  date straight after being built (a book dated in the future) would
 *  otherwise build again, and again, for as long as the page is open. */
function buildInBackground(script) {
  const book = script.epubPath;
  const run = track(book, kindle, onKindleFile(book, argv.export(book, {
    forFormat: 'kindle',
    fountain: script.fountainPath,
    optionsJson: optionsJsonFor(script),
  })));
  run.then((answer) => {
    if (answer?.ok === true && poll !== null) checkKindle({ ahead: false });
  }, () => {});
}

/** Count `run`, an export that may build this book's Kindle file, as a build
 *  for as long as it is out: the build line and its bar stay up until the
 *  last one for the book answers. Hands `run` back, answer and all. */
function track(book, file, run) {
  const entry = builds.get(book) ?? { runs: 0, file };
  entry.runs += 1;
  if (file !== null) entry.file = file;
  builds.set(book, entry);
  showBuild();
  kfxRedraw();
  const done = () => {
    entry.runs -= 1;
    if (entry.runs === 0 && builds.get(book) === entry) builds.delete(book);
    showBuild();
    kfxRedraw();
  };
  run.then(done, done);
  return run;
}

/** A press's own export just handed over a Kindle file for `book`: it is
 *  current as of now, so the next press is a lookup and puts up no build
 *  line for it. */
function builtNow(book) {
  if (kindleBook === book && kindle !== null) kindle = { ...kindle, fresh: true };
}

/** The build line and its bar, up while this page's book has a build out. */
function showBuild() {
  if (buildNote === null) return;
  const running = builds.get(ctx.state.script?.epubPath) ?? null;
  buildNote.hidden = running === null;
  if (running === null) return;
  const words = buildLine(running.file?.extension ?? null);
  // Only when the words change: rewritten into a live region, the same words
  // can be read out again.
  if (buildWords.textContent !== words) text(buildWords, words);
}

/** A route or a send just worked, and the engine now remembers it: ask again
 *  at once rather than at the next tick, so the brass moves to the route
 *  just used. Every poll already out was asked before the engine remembered
 *  it, so none of them may land after this one and put the brass back. */
function repoll() {
  newest = asks;
  refresh();
}

/** The list could not be drawn: the engine's sentence (or NO_ROUTES) in its
 *  place, never a list quietly short of the row that broke it. */
function fault(line) {
  drawn = null;
  rebuild(() => list.append(el('p', { class: 'fault-body' }, line)));
}

/** Refill the list. Rebuilding throws away whatever button the keyboard
 *  stood on, so where it stood is noted first and handed back after, the
 *  way kfx.js does for its own rows. */
function rebuild(fill) {
  const focused = list.contains(document.activeElement) ? document.activeElement : null;
  clear(list);
  fill();
  if (focused !== null) giveBackFocus(focused.dataset?.route ?? null);
}

/** The keyboard, back on the same route's button when it can still take the
 *  focus (a new reader plugged in above it moved it down, the brass moved to
 *  it). Otherwise focus.js's plan picks the page's first stop, as after
 *  every other redraw in this window: the reader it sent to was unplugged. */
function giveBackFocus(route) {
  const same = route === null ? null
    : buttons().find((button) => button.dataset.route === route);
  if (canFocus(same)) same.focus();
  else ctx.restoreFocus();
}

/** What Screepub can reach, and how, whatever is plugged in.
 *
 *  The table folds away. It is four readers, four honesty labels and two
 *  caveats: good information, and not what someone came to this page to
 *  find out. A <details> rather than a hand-rolled toggle: the open/shut
 *  state, the keyboard and the announcement come from the platform. It is
 *  drawn once per page, outside the list the poll rebuilds, so a reader who
 *  opened it does not have it shut on them by a reader being plugged in.
 *
 *  Folded, never dropped. Everything that made the table honest travels with
 *  it, including provenNote(), which carries the one fact the four statuses
 *  cannot: WHERE the single proven route was proven.
 *
 *  Built through el() rather than appended straight to the node, because
 *  el() drops a null child and Node.append() renders it as the word "null".
 *  That is not hypothetical: the old empty state shipped a stray "null"
 *  under the list on every platform but Windows until it was seen on screen. */
function reach() {
  const platform = navigator.platform;
  const cannot = undetectable(platform);
  return el('details', { class: 'reach' },
    el('summary', { class: 'reach-summary' }, REACH_HEADING),
    ...READERS.map((reader) => readerRow(reader, platform)),
    el('p', { class: 'caption reader-note' }, provenNote(platform)),
    cannot === null ? null : el('p', { class: 'caption device-caveat' }, cannot),
  );
}

function readerRow(reader, platform) {
  const status = readerStatus(reader.kind, platform);
  return el('div', { class: 'reader-row' },
    el('p', { class: 'device-name' }, reader.name),
    el('p', { class: `reader-status${status.proven ? '' : ' reader-untested'}` }, status.text),
    el('p', { class: 'reader-route' }, reader.route),
  );
}

/** One route: its title and the engine's detail, a device's volume line and
 *  caveat, the email row's first-time step, and a button only when it can
 *  fire. A dimmed row keeps its place and its fix; it has nothing to press.
 *  Each button carries its route's id as data-route, which is how a rebuild
 *  finds the same route's button again. */
function routeRow(route, chosen) {
  const lines = routeLines(route, navigator.platform);
  const caveat = route.device === undefined ? null : caveatFor(route.device, navigator.platform);
  const hint = emailSetupHint(route);
  const style = buttonClassFor(route, chosen);
  // The poll keeps running through a KFX install (it stops only for a send),
  // so a row it draws mid-install has its buttons born as dead as the
  // others', and the install's end brings them all back.
  const button = style === null ? null : el('button', {
    type: 'button', class: style, 'data-route': route.id, disabled: kfxInstalling(),
    onclick: () => choose(route),
  }, buttonLabel(route, kindle));
  return el('div', { class: route.available ? 'device-row' : 'device-row route-unavailable' },
    el('div', { class: 'device-what' },
      el('p', { class: 'device-name' }, lines.title),
      el('p', { class: 'route-detail' }, lines.detail),
      lines.where === null ? null : el('p', { class: 'device-where' }, lines.where),
      caveat === null ? null : el('p', { class: 'device-caveat' }, caveat),
      hint === null ? null : el('p', { class: 'caption route-hint' }, hint.line, ' ',
        el('button', {
          type: 'button', class: 'btn-quiet', 'data-route': hint.key, disabled: kfxInstalling(),
          onclick: () => keepFocus(hint.key, () => perform({ key: hint.key, title: SETUP_TITLE })),
        }, SETUP_BUTTON)),
    ),
    button,
  );
}

/** A row's button: a device through sendTo(), with its phases; every other
 *  route through perform(). */
function choose(route) {
  if (performerFor(route) === 'device') keepFocus(route.id, () => sendTo(route.device));
  else keepFocus(route.id, () => perform(route));
}

/** Runs one route and gives the keyboard back to the button that started it.
 *  A route kills every button on the list while it runs, and a dead button
 *  cannot hold the focus; a Save box takes it too, and when that closes the
 *  window's own handler puts it on the page's first live stop, which is not
 *  this row. So when the keyboard was in the list as the route started, it
 *  goes back to the same route's button once the buttons live again, found
 *  by its id (a repoll may redraw the row), or failing that to the page's
 *  plan: giveBackFocus(), as after a redraw. Success, refusal or a cancelled
 *  Save box alike.
 *
 *  Left alone when there is nothing to put back (a mouse press: WebKit does
 *  not focus a clicked button) or the reader has moved on: another script
 *  (a new page), or another surface (the list is hidden, and the plan would
 *  move the keyboard on a page the reader is not looking at). */
async function keepFocus(id, run) {
  const held = list !== null && list.contains(document.activeElement);
  const mine = era;
  await run();
  if (held && era === mine && canFocus(list)) giveBackFocus(id);
}

/** Every Send button on the surface, so a send can take the whole list out
 *  of reach rather than only the row it started from. */
function buttons() {
  return list === null ? [] : [...list.querySelectorAll('button')];
}

/** The script's own settings, fetched if this window has not seen them yet.
 *  Tune stores them on the script the moment it opens; a reader who never
 *  opened Tune has a sidecar full of settings and nothing here that knows
 *  it, and the MOBI rung would then rebuild the book with the DEFAULTS. The
 *  engine is asked rather than guessed at, and one validator — Tune's — says
 *  whether the answer is one. A read that fails changes nothing: the send
 *  goes ahead exactly as it would have, with no options to pass. */
async function ensureSettings() {
  const script = ctx.state.script;
  if (!needsSettings(script)) return;
  try {
    const answer = await runEngine(argv.settings(script.fountainPath));
    ctx.state.script.settings = settingsFrom(answer);
  } catch {
    // Unreadable settings are not a reason to refuse a send: an EPUB route
    // does not consult them at all, and the Kindle rung that does will fall
    // back to what the engine already considers this script's defaults.
  }
}

/** An engine call that reads or writes `book`, this script's library EPUB:
 *  an export (whose MOBI rung rebuilds it), a send, a save (readsTheBook()
 *  says which flows). Every one goes through here, to take its turn on the
 *  book with the Settings page's saves, which rebuild that same file in
 *  place. `label` is a row of book-queue.js's TURNS, which says whether the
 *  call reads the book (and runs beside other readers) or writes it (and
 *  runs alone), and is what the Settings page says it is waiting for. */
function onBook(book, label, args) {
  return inTurn(book, label, () => runEngine(args));
}

async function sendTo(device) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  /** Another script was opened while this send was in flight. */
  const stale = () => era !== mine;
  /** The copy happened; the engine now remembers this reader's kind. */
  let worked = false;
  for (const button of buttons()) button.disabled = true;
  artifactNote.hidden = true;
  try {
    // Inside the try, so a redraw that threw could never leave `sending` on.
    kfxRedraw();
    await ensureSettings();
    if (stale()) return;
    const script = ctx.state.script;
    // A Kindle file that may have to be built gets the build line and its
    // bar rather than a status (preparingPhase()), and waits behind a build
    // already out for the book instead of starting another (onKindleFile()):
    // the engine then finds the file that build made, current, and hands it
    // back. Every other reader takes the EPUB as it is, beside any build.
    const waiting = preparingPhase(device, kindle);
    say(waiting === 'building' ? statusFor('idle') : statusFor(waiting, { device }));

    // The library artifact is what Read previewed and what Tune rebuilds, so
    // it is the only file handed to a transfer. What the Kindle rung makes
    // of it is the engine's ladder, not this window's.
    const call = argv.export(script.epubPath, {
      forFormat: forFormat(device),
      fountain: script.fountainPath,
      optionsJson: optionsJsonFor(script),
    });
    const exporting = forFormat(device) === 'kindle'
      ? onKindleFile(script.epubPath, call)
      : onBook(script.epubPath, 'send', call);
    const built = await (waiting === 'building' ? track(script.epubPath, kindle, exporting) : exporting);

    // Nothing is copied when nothing was built: the export's refusal is the
    // whole answer, and asking `send` to move a file that does not exist
    // would replace the engine's sentence with a worse one.
    if (stale()) return;
    if (built.ok !== true) {
      say(statusFor(...outcomeFor(built, null, device)));
      return;
    }
    if (waiting === 'building') builtNow(script.epubPath);

    say(statusFor('copying', { device }));
    const sent = await onBook(script.epubPath, 'send', argv.send(built.path, device.id));
    if (stale()) return;

    const [phase, detail] = outcomeFor(built, sent, device);
    say(statusFor(phase, detail));
    if (phase !== 'sent') return;
    worked = true;
    const what = artifactLine(built);
    text(artifactNote, what);
    artifactNote.hidden = what === '';
  } catch (err) {
    // The engine's own sentence, verbatim. Every one of them is already
    // written for a person ("no reader is connected: plug one in over USB
    // and try again", "reMarkable accepts PDF and EPUB, not .mobi.") and
    // this window is not better placed to say it.
    if (!stale()) say(statusFor('failed', { device, detail: err.message }));
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    if (worked && !stale()) repoll();
    // A check that landed meanwhile was refused as busy; look again.
    if (!stale()) maybeBuildAhead();
  }
}

/** Every route that is not a device, from the press to the line that says
 *  how it went: sendTo()'s twin, under the SAME `sending` flag and the same
 *  `era` discipline, so no two routes (or a route and a send) ever run at
 *  once and no line is painted into a script that replaced this one.
 *
 *  The window never writes the file. A save asks where in the Save box and
 *  hands the chosen path to `route save-* --out`; the engine writes there.
 *  A cancelled box (null) is a change of mind: it goes straight out, asks
 *  the engine nothing more, and says nothing. */
async function perform(route) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  /** Another script was opened while this route was in flight. */
  const stale = () => era !== mine;
  const how = performerFor(route);
  /** The engine said the route worked, so it now remembers it. */
  let worked = false;
  for (const button of buttons()) button.disabled = true;
  artifactNote.hidden = true;
  try {
    // Inside the try, so a redraw that threw could never leave `sending` on.
    kfxRedraw();
    const script = ctx.state.script;
    let options = {};
    let what = '';
    if (how === 'save-epub') {
      const out = await saveDialog({
        defaultPath: saveNameFor(script.epubPath, 'epub'),
        filters: saveFiltersFor('epub', 'EPUB'),
      });
      if (stale() || out === null) return;
      options = { out };
      say(statusFor('saving'));
    } else if (how === 'save-kindle') {
      // The Kindle file is built (or found fresh) first, with this script's
      // own settings: which rung it reaches (KFX, AZW3, MOBI) is what names
      // the file in the Save box. Behind a build already out for the book it
      // waits for that build rather than starting another, and the build
      // line stays up until it is done; unless the check said the file is
      // current, its own export shows that line too.
      await ensureSettings();
      if (stale()) return;
      const settings = { fountain: script.fountainPath, optionsJson: optionsJsonFor(script) };
      say(statusFor('idle'));
      const exporting = onKindleFile(script.epubPath,
        argv.export(script.epubPath, { forFormat: 'kindle', ...settings }));
      const built = await (mayBuild(kindle) ? track(script.epubPath, kindle, exporting) : exporting);
      if (stale()) return;
      const [phase, file] = kindleFileFrom(built);
      if (phase === 'failed') {
        say(statusFor(phase, file));
        return;
      }
      builtNow(script.epubPath);
      // The wait is over; the Save box is the next thing on screen.
      say(statusFor('idle'));
      const out = await saveDialog({
        defaultPath: saveNameFor(script.epubPath, file.extension),
        // Named by purpose and type, short enough for a dialog's format menu.
        // The engine's label ("AZW3, for USB sideload to Kindle") is a
        // sentence for the page, not a menu item.
        filters: saveFiltersFor(file.extension, `Kindle file (${file.extension.toUpperCase()})`),
      });
      if (stale() || out === null) return;
      options = { out, ...settings };
      what = artifactLine(built);
      say(statusFor('saving'));
    } else {
      say(statusFor('opening', { route }));
    }
    const call = how === 'setup' ? argv.emailSetup() : argv.route(route.key, script.epubPath, options);
    // The save of a Kindle file exports it again (the engine finds it
    // current), so it keeps the Kindle file's line and turn; Save the EPUB
    // only reads the book.
    const answer = await (how === 'save-kindle' ? onKindleFile(script.epubPath, call)
      : readsTheBook(how) ? onBook(script.epubPath, 'copy', call) : runEngine(call));
    if (stale()) return;
    const [phase, detail] = routeNoteFrom(answer);
    say(statusFor(phase, detail));
    worked = phase === 'done';
    if (!worked) return;
    text(artifactNote, what);
    artifactNote.hidden = what === '';
  } catch (err) {
    // runEngine throws only when the engine could not run or broke its
    // contract, and a Save box only when the OS refused one; both messages
    // are already written for a person.
    if (!stale()) say(statusFor('failed', { detail: err.message }));
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    if (worked && !stale()) repoll();
    // A check that landed meanwhile (a Save box was up) was refused as busy,
    // and a cancelled save asks for no new list: look again now.
    if (!stale()) maybeBuildAhead();
  }
}

function say(status) {
  if (statusLine === null) return;
  text(statusLine, status.line);
  statusLine.classList.toggle('bad', status.bad);
}
