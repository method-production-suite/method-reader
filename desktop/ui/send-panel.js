// The send panel: the right-hand half of Convert's result (spec 2026-10-01,
// decisions 2 to 10, 12, 13). It starts from the reader's device. First the
// cards ("What do you read on?"), asked once and stored app-wide; then the
// calm screen, one brass button for the way that fits right now; Other ways
// for everything else. The flows (the Kindle file built ahead, a copy to a
// reader, a route the engine performs, a save) are send.js's, re-homed; the
// rules are device.js's and send-routes.js's. Nothing here decides anything.
//
// What has to outlive a redraw lives on the window's state, not here:
// ctx.state.readerDevice, ctx.state.sendResult (the line and the Eject
// button), ctx.state.devices. What has to outlive a book (Kindle builds and
// their line) stays module-level, as it was in send.js.
import { runEngine, argv, openUrl } from './app.js';
import { settingsFrom } from './tune.js';
import { inTurn } from './book-queue.js';
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';
import { newIssueUrl, osLabel } from './feedback.js';
import { RELEASE } from './notes.js';
import { kfxRedraw, kfxInstalling } from './kfx.js';
import * as D from './device.js';
import * as R from './send-routes.js';

let ctx = null;
let host = null;
/** The part redrawn between cards, calm and missing. */
let body = null;
let statusLine = null;
let artifactNote = null;
let unsubscribe = null;
let showing = false;
/** 'cards' | 'calm' | 'missing', or null until the reader's device is known
 *  (nothing is drawn before then, so the cards never flash up for someone
 *  who already chose). */
let mode = null;
/** The book the panel was last mounted for: a different one is a new era. */
let mountedBook = null;
/** The route list for this book (routesFrom()'s answer), or null while it is
 *  on its way or could not be read; `routesFault` is the line said then. */
let drawn = null;
let routesFault = null;
/** The book whose route list this panel has asked for since it was mounted.
 *  The calm screen asks on its first draw; after that only a change in what
 *  is plugged in asks again, never a timer. */
let askedFor = null;
/** A list that changed while a send held the panel: asked for once it ends. */
let owed = false;
/** Asks begun, and the newest one whose answer counts. An answer asked
 *  before one already drawn is older than what is on screen and is dropped,
 *  so a slow answer cannot put back a list a faster, later one replaced. */
let asks = 0;
let newest = 0;
/** What the engine last said about this script's Kindle file
 *  (kindleCheckFrom()), or null when it has not said, and the book it said
 *  it about: kept across a hide and a show of the same book, so the buttons
 *  do not lose their file type for the moment the check takes to answer
 *  again. */
let kindle = null;
let kindleBook = null;
/** The extension the brass button was labelled with, so a check that
 *  changes it redraws it and one that does not leaves it alone. */
let labelledWith = null;
/** Kindle file exports of one book, one at a time: the tail of each book's
 *  line. Two at once would run Kindle Previewer twice over the same scratch
 *  file. This is a rule about the Kindle FILE, not the book, so it is kept
 *  here and not in book-queue.js's readers and writers: a Kindle export
 *  takes its place in this line first and its turn on the book after, so
 *  one waiting here holds no turn on the book, and a Settings save that
 *  arrives meanwhile goes ahead of it and is in the file it builds. */
const kindleLines = new Map();
/** Kindle file builds out now, by book: `{ runs, file }`, how many exports
 *  that may build it are out (one started as the result appeared, a press
 *  queued behind it) and what the check said the file is. Kept at module
 *  level and not with the panel, because a build outlives a redraw, a new
 *  script and a hidden page, and a panel that comes back to its book must
 *  find it still going rather than start another. */
const builds = new Map();
/** The build line and its bar, and the line's own node. Built once per
 *  mount and moved into each calm screen, so the live region stays the
 *  same node across redraws. */
let buildNote = null;
let buildWords = null;
/** A send or a route in flight (sendTo() and perform() share it). Two at
 *  once would be a genuine race and not a cosmetic one: the MOBI rung of
 *  src/export/artifact.ts REWRITES the library EPUB in place before it
 *  writes the .mobi beside it, and a save of the EPUB could copy it half
 *  written. So every button goes dead for the duration, and nothing redraws
 *  underneath it: a redraw mid-send would hand back a fresh, enabled button
 *  and let a second one start. */
let sending = false;
/** Which book this panel is showing. A send already in flight cannot be
 *  cancelled (the engine has been asked) but it must not SAY anything once
 *  the book under it has been replaced, or "Sent to Kindle." lands in the
 *  status line beside a script that was never sent. Same shape as tune.js's
 *  `era`. */
let era = 0;

const platform = () => navigator.userAgentData?.platform ?? navigator.platform;
const devicesNow = () => ctx.devices?.current?.() ?? null;

export function mountPanel(node, context) {
  unsubscribe?.();
  host = node;
  ctx = context;
  mode = null;
  const book = ctx.state.script?.epubPath ?? null;
  if (book !== mountedBook) {
    era += 1;
    mountedBook = book;
    drawn = null;
    routesFault = null;
  }
  if (book !== kindleBook) {
    kindle = null;
    kindleBook = null;
  }
  askedFor = null;
  statusLine = el('p', { class: 'caption send-status', role: 'status' }, '');
  artifactNote = el('p', { class: 'caption send-artifact' }, '');
  artifactNote.hidden = true;
  // No percent to show: the engine says nothing while Kindle Previewer
  // works, so the bar sweeps rather than fills. The line is what is read
  // out; the bar is there to be seen moving.
  buildWords = el('p', { class: 'caption kindle-build-line' }, '');
  buildNote = el('div', { class: 'kindle-build', role: 'status' },
    buildWords,
    el('div', { class: 'track', 'aria-hidden': 'true' }, el('div', { class: 'build-sweep' })));
  buildNote.hidden = true;
  body = el('div', { class: 'panel-body' });
  clear(host);
  host.append(body, statusLine, artifactNote);
  unsubscribe = ctx.devices?.subscribe?.(onDevices) ?? null;
  showBuild();
  start();
}

async function start() {
  const mine = host;
  const blocked = R.blockedReason(ctx.state.script);
  if (blocked !== null) {
    drawBlocked(blocked);
    return;
  }
  if (ctx.state.readerDevice === undefined) {
    let answer = null;
    try {
      answer = await runEngine(argv.appSettings());
    } catch {
      // As none: the cards are asked, and nothing is written.
    }
    if (host !== mine || !host.isConnected) return;
    ctx.state.readerDevice = D.readerDeviceFrom(answer);
  }
  mode = ctx.state.readerDevice === null ? 'cards' : 'calm';
  draw();
}

export function panelShown() {
  showing = true;
  // What is plugged in changed while the panel was away: ask again.
  if (mode === 'calm' && askedFor !== mountedBook) draw();
}

export function panelHidden() {
  showing = false;
}

/** The book cannot be sent (no EPUB on disk): send.js's words for it. */
function drawBlocked(blocked) {
  clear(body);
  body.append(
    el('h2', { class: 'fault' }, 'No book to send'),
    el('p', { class: 'fault-body' }, blocked),
    el('div', { class: 'read-ways' },
      el('button', {
        type: 'button', class: 'btn btn-outline', onclick: () => ctx.convertAgain(),
      }, 'Convert it again')),
  );
}

/** Redraw the part that changes. The keyboard goes back to the control with
 *  the same data-key when there still is one. */
function draw() {
  if (body === null || mode === null) return;
  const focused = host.contains(document.activeElement) ? document.activeElement : null;
  const key = focused?.dataset?.key ?? null;
  clear(body);
  if (mode === 'cards') body.append(drawCards());
  else if (mode === 'missing') body.append(missingPanel());
  else body.append(drawCalm());
  if (focused !== null && !body.contains(focused)) giveBackFocus(key);
  // The calm screen's first draw for this book asks for its route list: a
  // quick answer first, then the full one.
  if (mode === 'calm' && askedFor !== mountedBook) {
    askedFor = mountedBook;
    refresh({ quick: true });
    refresh();
  }
}

/** The keyboard, back on the control with this data-key when it can still
 *  take the focus. Otherwise focus.js's plan picks the page's first stop, as
 *  after every other redraw in this window. */
function giveBackFocus(key) {
  const same = key === null ? null
    : buttons().find((button) => button.dataset.key === key);
  if (canFocus(same)) same.focus();
  else ctx.restoreFocus();
}

/** Every button in the panel, so a send can take all of them out of reach. */
function buttons() {
  return host === null ? [] : [...host.querySelectorAll('button')];
}

function say(status) {
  if (statusLine === null) return;
  text(statusLine, status.line);
  statusLine.classList.toggle('bad', status.bad);
}

// ------------------------------------------------------------------ cards

function drawCards() {
  const cards = D.cardsFor(platform(), devicesNow());
  return el('div', { class: 'panel-cards' },
    el('h3', { class: 'panel-question' }, D.QUESTION),
    el('div', { class: 'device-cards' }, ...cards.map(cardSlot)));
}

/** One card, and its asterisk beside it (never inside it: a button inside a
 *  button is not a button anyone can reach). */
function cardSlot(card) {
  const button = el('button', {
    type: 'button', class: 'device-card', 'data-key': card.kind,
    onclick: () => (card.kind === D.MISSING.kind ? showMissing() : chooseDevice(card.kind)),
  },
  el('span', { class: 'device-card-name' }, card.name),
  card.sub === null ? null : el('span', { class: 'device-card-sub' }, card.sub));
  markCard(button, card.here, card.name);
  return el('div', { class: 'device-card-slot' }, button, card.unproven ? asterisk(card.kind) : null);
}

/** A card's "plugged in": the brass edge, the dot, and the same said to a
 *  screen reader. Toggled in place, so a reader plugged in while the cards
 *  are up does not rebuild the button under the keyboard. */
function markCard(button, here, name) {
  button.classList.toggle('device-card-here', here);
  const dot = button.querySelector('.device-dot');
  if (here && dot === null) button.append(el('span', { class: 'device-dot', 'aria-hidden': 'true' }));
  if (!here && dot !== null) dot.remove();
  if (here) button.setAttribute('aria-label', `${name}, plugged in`);
  else button.removeAttribute('aria-label');
}

function markCards(list) {
  for (const card of D.cardsFor(platform(), list)) {
    const button = body.querySelector(`.device-card[data-key="${card.kind}"]`);
    if (button !== null) markCard(button, card.here, card.name);
  }
}

/** A card pressed: the calm screen at once, the choice stored app-wide
 *  behind it. A store the engine refuses still answers this session, and the
 *  status line says why it will be asked again next time. */
async function chooseDevice(kind) {
  ctx.state.readerDevice = kind;
  mode = 'calm';
  draw();
  say({ line: '', bad: false });
  try {
    const answer = await runEngine(argv.readerDevice(kind));
    if (answer?.ok !== true) say({ line: R.failureMessage(answer), bad: true });
  } catch (err) {
    say({ line: err.message, bad: true });
  }
}

function showMissing() {
  mode = 'missing';
  draw();
}

/** What is plugged in changed (device-watch.js, through ctx.devices). */
function onDevices(list) {
  ctx.state.devices = list;
  if (sending) {
    // The copy finishes first; the list is asked for when it does.
    owed = true;
    return;
  }
  if (mode === 'cards') {
    markCards(list);
    return;
  }
  if (mode !== 'calm') return;
  if (!showing) {
    // Asked for again when the calm screen is next drawn.
    askedFor = null;
    return;
  }
  refresh({ quick: true });
  refresh();
}

// ------------------------------------------------------------------- calm

/** One brass button for the way that fits this reader right now, its lines
 *  under it, then Other ways and Different device. Which way is brass is
 *  device.js's offerFor(); this only draws it. */
function drawCalm() {
  const kind = ctx.state.readerDevice;
  const offer = D.offerFor({ device: kind, platform: platform(), shown: drawn, kindle });
  labelledWith = kindle?.extension ?? null;
  const brass = offer?.brass ?? null;
  const lines = routesFault === null ? offer?.lines ?? [] : [];
  return el('div', { class: 'calm' },
    el('div', { class: 'calm-act' },
      brass === null ? null : el('button', {
        type: 'button', class: 'btn btn-brad', 'data-key': brass.route.id,
        disabled: sending || kfxInstalling(), onclick: () => choose(brass.route),
      }, brass.label),
      brass !== null && R.isDeviceRoute(brass.route)
        ? el('span', { class: 'device-dot', 'aria-hidden': 'true' }) : null,
      offer?.unproven ? asterisk(kind) : null),
    buildNote,
    routesFault === null ? null : el('p', { class: 'fault-body' }, routesFault),
    ...lines.map((line) => el('p', { class: 'caption calm-line' }, line)),
    el('div', { class: 'calm-ways' },
      el('button', { type: 'button', class: 'btn btn-outline', 'data-key': 'other-ways', 'aria-expanded': 'false' },
        D.OTHER_WAYS),
      el('button', {
        type: 'button', class: 'btn-quiet', 'data-key': 'different',
        onclick: () => { mode = 'cards'; draw(); },
      }, D.DIFFERENT)));
}

/** Every route this book can take, as `screepub routes` lists them. `quick`
 *  leaves out the reMarkable probe, which runs to its whole timeout (a
 *  second and a half) on every machine with no tablet docked, so the brass
 *  is on screen as soon as the mount scan answers. The full one is asked
 *  second, so its answer is the newer one, and the ordering below drops the
 *  quick answer if it lands last. */
async function refresh({ quick = false } = {}) {
  if (ctx === null || R.blockedReason(ctx.state.script) !== null) return;
  // Nothing is redrawn out from under a send in progress.
  if (sending) {
    owed = true;
    return;
  }
  const mine = ++asks;
  const myEra = era;
  let answer;
  try {
    answer = await runEngine(argv.routes(ctx.state.script.epubPath, { quick }));
  } catch (err) {
    // The same three reasons to drop an answer as below.
    if (era !== myEra || sending || mine <= newest) return;
    newest = mine;
    fault(err.message);
    return;
  }
  // A new book's panel, a send begun since, or a newer answer already
  // drawn: this one is out of date, and drawing it would undo something.
  if (era !== myEra || sending || mine <= newest) return;
  newest = mine;
  const shown = R.routesFrom(answer);
  if (shown === null) {
    fault(R.routesFailure(answer));
    return;
  }
  // Nothing changed, so nothing is redrawn: a rebuild would take the focus
  // off a button someone had just tabbed to.
  if (routesFault === null && R.sameRoutes(drawn, shown) && labelledWith === (kindle?.extension ?? null)) return;
  drawn = shown;
  routesFault = null;
  if (mode === 'calm') draw();
}

/** The list could not be read: the engine's sentence (or NO_ROUTES) in the
 *  brass button's place, never a brass button picked from half a list. */
function fault(line) {
  drawn = null;
  routesFault = line;
  if (mode === 'calm') draw();
}

/** A route or a send just worked, and the engine now remembers it: ask again
 *  at once. Every ask already out was asked before the engine remembered
 *  it, so none of them may land after this one. */
function repoll() {
  newest = asks;
  refresh();
}

/** What follows a send or a route: a list owed while it ran is asked for
 *  (for whatever book is showing now), or one that worked asks again. */
function afterFlow(worked, stale) {
  if (owed) {
    owed = false;
    newest = asks;
    refresh({ quick: true });
    refresh();
  } else if (worked && !stale) {
    repoll();
  }
}

/** The brass button: a reader through sendTo(), with its phases; Save the
 *  EPUB through its Save box; every other route through perform(). */
function choose(route) {
  const how = R.performerFor(route);
  if (how === 'device') keepFocus(route.id, () => sendTo(route.device));
  else if (how === 'save-epub') keepFocus(route.id, () => saveFile('epub'));
  else keepFocus(route.id, () => perform(route));
}

/** Runs one flow and gives the keyboard back to the button that started it.
 *  A flow kills every button in the panel while it runs, and a dead button
 *  cannot hold the focus; a Save box takes it too, and when that closes the
 *  window's own handler puts it on the page's first live stop, which is not
 *  this button. So when the keyboard was in the panel as the flow started,
 *  it goes back to the same route's button once the buttons live again,
 *  found by its key (a redraw may have rebuilt it), or failing that to the
 *  page's plan: giveBackFocus(), as after a redraw. Success, refusal or a
 *  cancelled Save box alike.
 *
 *  Left alone when there is nothing to put back (a mouse press: WebKit does
 *  not focus a clicked button) or the reader has moved on: another book (a
 *  new era), or another page (the panel is hidden, and the plan would move
 *  the keyboard on a page the reader is not looking at). */
async function keepFocus(id, run) {
  const held = host !== null && host.contains(document.activeElement);
  const mine = era;
  await run();
  if (held && era === mine && canFocus(host)) giveBackFocus(id);
}

/** The script's own settings, fetched if this window has not seen them yet.
 *  Tune stores them on the script the moment it opens; a reader who never
 *  opened Tune has a sidecar full of settings and nothing here that knows
 *  it, and the MOBI rung would then rebuild the book with the DEFAULTS. The
 *  engine is asked rather than guessed at, and one validator (Tune's) says
 *  whether the answer is one. A read that fails changes nothing: the send
 *  goes ahead exactly as it would have, with no options to pass. */
async function ensureSettings() {
  const script = ctx.state.script;
  if (!R.needsSettings(script)) return;
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
  const label = R.kindleTurn(kindle);
  return inKindleLine(book, () => onBook(book, label, args));
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

/** The build line and its bar, up while this panel's book has a build out. */
function showBuild() {
  if (buildNote === null) return;
  const running = builds.get(ctx.state.script?.epubPath) ?? null;
  buildNote.hidden = running === null;
  if (running === null) return;
  const words = R.buildLine(running.file?.extension ?? null);
  // Only when the words change: rewritten into a live region, the same words
  // can be read out again.
  if (buildWords.textContent !== words) text(buildWords, words);
}

async function sendTo(device) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  /** Another book was opened while this send was in flight. */
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
    const waiting = R.preparingPhase(device, kindle);
    say(waiting === 'building' ? R.statusFor('idle') : R.statusFor(waiting, { device }));

    // The library artifact is what Read previewed and what Tune rebuilds, so
    // it is the only file handed to a transfer. What the Kindle rung makes
    // of it is the engine's ladder, not this window's.
    const call = argv.export(script.epubPath, {
      forFormat: R.forFormat(device),
      fountain: script.fountainPath,
      optionsJson: R.optionsJsonFor(script),
    });
    const exporting = R.forFormat(device) === 'kindle'
      ? onKindleFile(script.epubPath, call)
      : onBook(script.epubPath, 'send', call);
    const built = await (waiting === 'building' ? track(script.epubPath, kindle, exporting) : exporting);

    // Nothing is copied when nothing was built: the export's refusal is the
    // whole answer, and asking `send` to move a file that does not exist
    // would replace the engine's sentence with a worse one.
    if (stale()) return;
    if (built.ok !== true) {
      say(R.statusFor(...R.outcomeFor(built, null, device)));
      return;
    }
    if (waiting === 'building') builtNow(script.epubPath);

    say(R.statusFor('copying', { device }));
    const sent = await onBook(script.epubPath, 'send', argv.send(built.path, device.id));
    if (stale()) return;

    const [phase, detail] = R.outcomeFor(built, sent, device);
    if (phase !== 'sent') {
      say(R.statusFor(phase, detail));
      return;
    }
    worked = true;
    // On a reader that mounts: "<Title> is on your <name>. Eject it before
    // you unplug." The engine's own line for one that does not.
    say(D.copiedState(script.title, device, sent));
    const what = R.artifactLine(built);
    text(artifactNote, what);
    artifactNote.hidden = what === '';
  } catch (err) {
    // The engine's own sentence, verbatim. Every one of them is already
    // written for a person ("no reader is connected: plug one in over USB
    // and try again", "reMarkable accepts PDF and EPUB, not .mobi.") and
    // this window is not better placed to say it.
    if (!stale()) say(R.statusFor('failed', { device, detail: err.message }));
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    afterFlow(worked, stale());
  }
}

/** Every route that is not a device or a save, from the press to the line
 *  that says how it went: sendTo()'s twin, under the SAME `sending` flag and
 *  the same `era` discipline, so no two routes (or a route and a send) ever
 *  run at once and no line is painted beside a book that replaced this one.
 *  The engine opens Apple Books, Amazon or Mail; the window opens nothing. */
async function perform(route) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  /** Another book was opened while this route was in flight. */
  const stale = () => era !== mine;
  const how = R.performerFor(route);
  /** The engine said the route worked, so it now remembers it. */
  let worked = false;
  for (const button of buttons()) button.disabled = true;
  artifactNote.hidden = true;
  try {
    // Inside the try, so a redraw that threw could never leave `sending` on.
    kfxRedraw();
    const script = ctx.state.script;
    say(R.statusFor('opening', { route }));
    const call = how === 'setup' ? argv.emailSetup() : argv.route(route.key, script.epubPath);
    // A route that opens another program reads nothing of the book, and
    // must not hold it (readsTheBook()).
    const answer = await (R.readsTheBook(how) ? onBook(script.epubPath, 'copy', call) : runEngine(call));
    if (stale()) return;
    const [phase, detail] = R.routeNoteFrom(answer);
    say(R.statusFor(phase, detail));
    worked = phase === 'done';
  } catch (err) {
    // runEngine throws only when the engine could not run or broke its
    // contract; its message is already written for a person.
    if (!stale()) say(R.statusFor('failed', { detail: err.message }));
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    afterFlow(worked, stale());
  }
}

// ---------------------------------------------------------------- missing

/** "My device isn't here", for now: a sentence, a door to a GitHub issue
 *  (the one URL the window may already open), the EPUB as the fallback, and
 *  Back. Part 4 of the spec replaces the inside of this function with the
 *  request field and its Send. It stores nothing. */
function missingPanel() {
  return el('div', { class: 'device-missing' },
    el('p', { class: 'prose' }, D.MISSING_LINE),
    el('div', { class: 'calm-ways' },
      el('button', {
        type: 'button', class: 'btn btn-outline', 'data-key': 'missing-tell',
        onclick: () => openUrl(newIssueUrl({
          appVersion: RELEASE.version, osVersion: osLabel(platform()),
          context: 'A reader Screepub does not list yet: ',
        })),
      }, 'Tell us on GitHub'),
      el('button', { type: 'button', class: 'btn-quiet', 'data-key': 'save-epub', onclick: () => saveFile('epub') },
        'Save the EPUB…'),
      el('button', { type: 'button', class: 'btn-quiet', 'data-key': 'missing-back', onclick: () => { mode = 'cards'; draw(); } },
        'Back')));
}

/** Save one file type where the Save box says. Filled in with Other ways'
 *  Save a file group. */
function saveFile(type) {
  void type;
}

// --------------------------------------------------------------- asterisk

/** The mark on what nobody has run on that hardware: pressing pins the note
 *  open, hovering shows it. The note's last two words are a button that
 *  opens a report. */
function asterisk(kind) {
  const said = D.asteriskNote(kind, platform());
  if (said === null) return null;
  const note = el('p', { class: 'caption asterisk-note', hidden: true }, said.replace(/ tell us\.$/, ' '),
    el('button', { type: 'button', class: 'btn-quiet', 'data-key': `tell-${kind}`, onclick: tellUs(kind) }, 'tell us'),
    '.');
  let pinned = false;
  const star = el('button', {
    type: 'button', class: 'asterisk', 'aria-expanded': 'false', 'data-key': `asterisk-${kind}`,
    'aria-label': `About ${D.nameOf(kind)}: never tried on real hardware`,
    onclick: () => {
      pinned = !pinned;
      note.hidden = !pinned;
      star.setAttribute('aria-expanded', String(pinned));
    },
    onmouseenter: () => { note.hidden = false; },
    onmouseleave: () => { note.hidden = !pinned; },
  }, '*');
  return el('span', { class: 'asterisk-wrap' }, star, note);
}

/** A report on a reader nobody here has tried: whether it worked. */
function tellUs(kind) {
  return () => openUrl(newIssueUrl({
    appVersion: RELEASE.version,
    osVersion: osLabel(platform()),
    context: `Sending to a ${D.nameOf(kind)} on ${osLabel(platform())}: it worked / it did not work (delete one).`,
  }));
}
