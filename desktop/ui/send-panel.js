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
import { runEngine, argv, openUrl, saveDialog } from './app.js';
import { settingsFrom } from './tune.js';
import { inTurn } from './book-queue.js';
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';
import { newIssueUrl, osLabel } from './feedback.js';
import { RELEASE } from './notes.js';
import {
  HEADING as KFX_HEADING, mountKfx, kfxShown, kfxHidden, kfxRedraw, kfxInstalling,
} from './kfx.js';
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
 *  is plugged in asks again, never a timer (except for a reMarkable reader:
 *  lookForRemarkable()). */
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
/** What each book's last background build was started for (attemptKey()),
 *  so the panel builds ahead once per version of a book and kind of file
 *  (buildsAhead()'s `triedFor`). At module level, as `builds` is: a hide and
 *  a show, or a new script and back, is still the same version of the book. */
const triedFor = new Map();
/** From panelShown() to panelHidden(): the panel may build ahead, when the
 *  check says so and this is a Kindle session. Either can be known first, so
 *  each looks again when it changes (maybeBuildAhead()). */
let aheadWanted = false;
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
/** Other ways, open or shut. Kept across the calm screen's redraws (a
 *  plug-in redraws it), shut again for each new mount. */
let waysOpen = false;
/** A card was pressed from the keyboard before the brass button existed (it
 *  waits on the route list): where the page's plan parked the focus then.
 *  The brass takes it when it is drawn, unless the reader has moved it on. */
let brassOwed = null;
/** The KFX checklist's place (kfx.js draws into it), built once per mount
 *  and moved into each redraw like the build line, so kfx.js keeps drawing
 *  into the same node. `kfxOpen` is whether Set it up put it on screen. */
let kfxNode = null;
let kfxOpen = false;
/** What convert.js hands in to sit at the foot of Save a file (Show in
 *  Finder, the book's path). Moved into each redraw, never rebuilt here. */
let footer = [];
/** How often the calm screen for a reMarkable asks the full route list
 *  again. The background poll is `devices --quick`, which never probes for
 *  one, so a tablet docked after the result appeared would otherwise never
 *  be offered. Slow on purpose: the probe can run to its whole timeout. */
export const REMARKABLE_LOOK_MS = 10_000;
/** That clock while it runs, and the timers it was started on. */
let remarkableTimer = null;
let remarkableClock = null;

const PLUGGED_IN = 'Plugged in';
const platform = () => navigator.userAgentData?.platform ?? navigator.platform;
const devicesNow = () => ctx.devices?.current?.() ?? null;

export function mountPanel(node, context, { footer: under = [] } = {}) {
  unsubscribe?.();
  host = node;
  ctx = context;
  mode = null;
  footer = under;
  waysOpen = false;
  // A fresh result owes the brass button the keyboard too: convert.js
  // parks it on the page's first stop before the route list has drawn one.
  brassOwed = { plan: true };
  lookForRemarkable();
  closeKfx();
  kfxNode = el('section', { class: 'kfx-setup', 'aria-label': KFX_HEADING });
  kfxNode.hidden = true;
  const book = ctx.state.script?.epubPath ?? null;
  if (book !== mountedBook) {
    era += 1;
    mountedBook = book;
    drawn = null;
    routesFault = null;
    aheadWanted = false;
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
  // What the last send said about this book, and its Eject button, drawn
  // again (spec decision 13). Another book's line is forgotten, but never a
  // reader still waiting to be ejected: its button and its line (naming the
  // book that was copied) come along until it is ejected or unplugged.
  const kept = ctx.state.sendResult ?? null;
  if (kept !== null && kept.book === book) show(kept);
  else if (kept?.eject) {
    ctx.state.sendResult = { book, line: kept.eject.line ?? '', bad: false, eject: kept.eject };
    show(ctx.state.sendResult);
  } else ctx.state.sendResult = null;
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
    mode = ctx.state.readerDevice === null ? 'cards' : 'calm';
    // While the engine answered, the result parked the keyboard on the
    // first control it had, the quiet links under this panel. The panel is
    // the work, so the plan picks again now that it is drawn (the panel
    // comes first). A reader who moved on in the meantime is left alone.
    const parked = document.activeElement;
    draw();
    const links = host.parentNode?.querySelector?.('.asides');
    if (parked !== null && links?.contains?.(parked) && parked === links.querySelector('button')) ctx.restoreFocus();
    return;
  }
  mode = ctx.state.readerDevice === null ? 'cards' : 'calm';
  draw();
}

export function panelShown() {
  showing = true;
  lookForRemarkable();
  // What is plugged in changed while the panel was away: ask again.
  if (mode === 'calm' && askedFor !== mountedBook) draw();
  // Removed first: a second show without a hide must not listen twice.
  window.removeEventListener('focus', onFocus);
  window.addEventListener('focus', onFocus);
  // What the last visit learnt is kept (the button keeps naming the file),
  // but not whether the file is current: a Settings save may have rewritten
  // the book since, and a press before the check answers must put the build
  // line up rather than trust a send from before. Unknown, not stale, so
  // nothing is built ahead on it either.
  if (kindle !== null) kindle = { ...kindle, fresh: null };
  aheadWanted = true;
  checkKindle();
  // The checklist left open asks the machine again, as it does on a focus.
  if (kfxOpen) kfxShown();
}

export function panelHidden() {
  showing = false;
  lookForRemarkable();
  aheadWanted = false;
  window.removeEventListener('focus', onFocus);
  if (kfxOpen) kfxHidden();
}

/** A copy to `device`, a reader the watcher listed (the Kindle heads-up's
 *  Copy): the same flow as a card's brass button, with no route row needed. */
export function copyTo(device) {
  if (host === null || R.blockedReason(ctx.state.script) !== null) return Promise.resolve();
  return sendTo(device);
}

/** Back from somewhere else, perhaps from installing Kindle Previewer: the
 *  Kindle file may be a different kind now, so the button is named again.
 *  Only named: focus comes back after every Save box and every switch of
 *  app, and it never starts a build. */
function onFocus() {
  checkKindle({ ahead: false });
  // A reMarkable docked while the reader was in another app.
  if (remarkableWanted() && !sending) refresh();
}

/** Ask the engine about this script's Kindle file (`export --check`): which
 *  file this computer makes, and whether the one beside the book is current.
 *  It takes its turn on the book (book-queue.js), so a save still settling
 *  on the Settings page lands first and the answer is about the book as it
 *  will be. A check that fails names nothing: the button keeps its plain
 *  words, and a press still builds and says what went wrong in its own
 *  words. `ahead` false names the file and nothing more: the check on a
 *  focus return and the one after a build never start a build. */
async function checkKindle({ ahead = true } = {}) {
  const script = ctx?.state.script;
  if (host === null || R.blockedReason(script) !== null) return;
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
  if (era !== mine) return;
  useKindle(book, R.kindleCheckFrom(answer));
  if (ahead) maybeBuildAhead();
}

/** Start the build ahead if everything buildsAhead() asks for is so right
 *  now, on a panel that is on screen. A Kindle session is the reader saying
 *  Kindle, or one plugged in now (device.js's kindleSession()). */
function maybeBuildAhead() {
  if (!aheadWanted || !showing) return;
  const script = ctx.state.script;
  const book = script?.epubPath;
  if (kindleBook !== book) return;
  if (!R.buildsAhead(kindle, {
    building: builds.has(book),
    busy: sending || kfxInstalling(),
    triedFor: triedFor.get(book) ?? null,
    forKindle: D.kindleSession(ctx.state.readerDevice, devicesNow()),
  })) return;
  triedFor.set(book, R.attemptKey(kindle));
  buildInBackground(script);
}

/** What is known about this book's Kindle file, onto the brass button. */
function useKindle(book, file) {
  kindle = file;
  kindleBook = book;
  // Nothing is redrawn under a send; the next change catches up.
  if (mode === 'calm' && !sending && labelledWith !== (kindle?.extension ?? null)) draw();
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
 *  otherwise build again, and again, for as long as the panel is open. */
function buildInBackground(script) {
  const book = script.epubPath;
  const run = track(book, kindle, onKindleFile(book, argv.export(book, {
    forFormat: 'kindle',
    fountain: script.fountainPath,
    optionsJson: R.optionsJsonFor(script),
  })));
  run.then((answer) => {
    if (answer?.ok === true && showing) checkKindle({ ahead: false });
  }, () => {});
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
  // Still where the page's plan parked it (after a card press, or on a fresh
  // result): the brass button is owed the keyboard. Anywhere else, the
  // reader moved it. A fresh result's debt waits while the keyboard is still
  // outside the panel: the plan has not parked it yet.
  let owed;
  if (brassOwed?.plan) {
    const stop = buttons().find((button) => canFocus(button)) ?? null;
    owed = mode === 'calm' && focused !== null && focused === stop;
    if (focused !== null && !owed) brassOwed = null;
  } else {
    owed = brassOwed !== null && document.activeElement === brassOwed.at;
    if (!owed) brassOwed = null;
  }
  clear(body);
  if (mode === 'cards') body.append(drawCards());
  else if (mode === 'missing') body.append(missingPanel());
  else body.append(drawCalm());
  if (owed) payBrassOwed(focused, key);
  else if (focused !== null && !body.contains(focused)) giveBackFocus(key);
  // The calm screen's first draw for this book asks for its route list: a
  // quick answer first, then the full one.
  if (mode === 'calm' && askedFor !== mountedBook) {
    askedFor = mountedBook;
    refresh({ quick: true });
    refresh();
  }
  lookForRemarkable();
}

/** Whether the slow look for a docked reMarkable should run: the calm
 *  screen, on screen, for a reader who said reMarkable. */
function remarkableWanted() {
  return showing && host !== null && mode === 'calm' && ctx?.state.readerDevice === 'remarkable';
}

/** Start or stop the slow look, as remarkableWanted() now says. A tick
 *  during a send asks nothing (and owes nothing): the next tick asks. */
function lookForRemarkable() {
  const wanted = remarkableWanted();
  if (wanted && remarkableTimer === null) {
    remarkableClock = ctx.timers ?? globalThis;
    remarkableTimer = remarkableClock.setInterval(() => {
      if (remarkableWanted() && !sending) refresh();
    }, REMARKABLE_LOOK_MS);
  } else if (!wanted && remarkableTimer !== null) {
    remarkableClock.clearInterval(remarkableTimer);
    remarkableTimer = null;
  }
}

/** The keyboard, back on the control with this data-key when it can still
 *  take the focus. Gone (a card pressed, so the cards are gone too): the
 *  brass button, the one thing the reader came for, rather than the asterisk
 *  that sits before it. Otherwise focus.js's plan picks the page's first
 *  stop, as after every other redraw in this window. */
function giveBackFocus(key) {
  const same = key === null ? null
    : buttons().find((button) => button.dataset.key === key);
  const brass = body?.querySelector('.btn-brad') ?? null;
  brassOwed = null;
  if (canFocus(same)) same.focus();
  else if (canFocus(brass)) brass.focus();
  else {
    ctx.restoreFocus();
    if (mode === 'calm') brassOwed = { at: document.activeElement };
  }
}

/** A redraw while the brass button is owed the keyboard (draw() has checked
 *  the reader has not moved it). Drawn at last: it takes it. Not yet (the
 *  quick list may still say nothing): the keyboard goes back to the control
 *  the plan parked it on, redrawn, and the brass stays owed from there, so a
 *  parked spot inside the panel (the asterisk, Other ways) does not swallow
 *  the debt the way a same-key return otherwise would. */
function payBrassOwed(focused, key) {
  const brass = mode === 'calm' ? body.querySelector('.btn-brad') : null;
  if (canFocus(brass)) {
    brassOwed = null;
    brass.focus();
    return;
  }
  if (focused !== null && !body.contains(focused)) giveBackFocus(key);
  brassOwed = mode === 'calm' ? { at: document.activeElement } : null;
}

/** Every button in the panel, so a send can take all of them out of reach. */
function buttons() {
  return host === null ? [] : [...host.querySelectorAll('button')];
}

/** The status line, kept on the window's state with the book it is about,
 *  so a return to the result draws it again. `eject` is kept as it was
 *  unless the call says otherwise, whichever book it was copied from: a
 *  save after a copy does not make the reader safe to unplug. */
function say(status) {
  if (statusLine === null) return;
  const before = ctx.state.sendResult;
  const kept = before?.eject ?? null;
  ctx.state.sendResult = {
    book: mountedBook, line: status.line, bad: status.bad,
    eject: status.eject === undefined ? kept : status.eject,
  };
  show(status);
}

function show(status) {
  text(statusLine, status.line);
  statusLine.classList.toggle('bad', status.bad);
}

/** The reader the last copy left mounted, while it still needs ejecting,
 *  whichever book was copied to it. */
function ejectPending() {
  return ctx.state.sendResult?.eject ?? null;
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
/** "Plugged in", said in words beside the green dot: the brass edge and
 *  the dot alone did not read as plugged in (owner, on a real Kindle). */
function pluggedIn() {
  return el('span', { class: 'device-here', 'aria-hidden': 'true' },
    el('span', { class: 'device-dot' }), PLUGGED_IN);
}

function markCard(button, here, name) {
  button.classList.toggle('device-card-here', here);
  const mark = button.querySelector('.device-here');
  if (here && mark === null) button.append(pluggedIn());
  if (!here && mark !== null) mark.remove();
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
  // A card always asks what is plugged in now, never a list from before the
  // cards came back: that list once offered "plug it in" for a Kindle that
  // was (found on a real Kindle, 2026-10-02).
  askedFor = null;
  drawn = null;
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
  // Unplugged without ejecting: the button goes, the line stays.
  const eject = ejectPending();
  if (eject !== null && !D.ejectStillThere(eject, list)) {
    ctx.state.sendResult = { ...ctx.state.sendResult, eject: null };
    if (mode === 'calm' && !sending) draw();
  }
  if (sending) {
    // The copy finishes first; the list is asked for when it does.
    owed = true;
    return;
  }
  // A Kindle just plugged in makes this a Kindle session.
  maybeBuildAhead();
  if (mode === 'cards') {
    markCards(list);
    // The route list drawn before the cards came back predates this plug-in:
    // the calm screen asks again when a card brings it back.
    askedFor = null;
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
  // After a copy over USB, Eject takes the brass place (spec decision 12),
  // and the copy it replaced is listed in Other ways for a second press.
  const eject = ejectPending();
  const brass = eject === null ? offer?.brass ?? null : null;
  const lines = routesFault === null ? offer?.lines ?? [] : [];
  return el('div', { class: 'calm' },
    el('div', { class: 'calm-act' },
      eject === null ? null : el('button', {
        type: 'button', class: 'btn btn-brad', 'data-key': 'eject',
        disabled: sending || kfxInstalling(), onclick: () => keepFocus('eject', ejectNow),
      }, D.ejectLabel(eject)),
      brass === null ? null : el('button', {
        type: 'button', class: 'btn btn-brad', 'data-key': brass.route.id,
        disabled: sending || kfxInstalling(), onclick: () => choose(brass.route),
      }, brass.label),
      brass !== null && R.isDeviceRoute(brass.route) ? pluggedIn() : null,
      offer?.unproven ? asterisk(kind) : null),
    buildNote,
    routesFault === null ? null : el('p', { class: 'fault-body' }, routesFault),
    ...lines.map((line) => el('p', { class: 'caption calm-line' }, line)),
    el('div', { class: 'calm-ways' },
      el('button', {
        type: 'button', class: 'btn btn-outline', 'data-key': 'other-ways', 'aria-expanded': String(waysOpen),
        onclick: toggleWays,
      }, D.OTHER_WAYS),
      el('button', {
        type: 'button', class: 'btn-quiet', 'data-key': 'different',
        onclick: () => { mode = 'cards'; draw(); },
      }, D.DIFFERENT)),
    drawOtherWays(offer === null || eject === null ? offer : { ...offer, brass: null }));
}

// ------------------------------------------------------------- other ways

/** Open or shut Other ways in place: no redraw, so the keyboard stays on
 *  the toggle. Shutting it puts the KFX checklist away too. */
function toggleWays() {
  waysOpen = !waysOpen;
  const section = body.querySelector('section.other-ways');
  if (section !== null) section.hidden = !waysOpen;
  body.querySelector('[data-key="other-ways"]')?.setAttribute('aria-expanded', String(waysOpen));
  if (!waysOpen) closeKfx();
}

/** Every way there is, in two groups (spec decision 10). Send it: the
 *  offer's follow-ups first, then every other route the engine lists, dimmed
 *  ones with their fix (device.js's sendItRows()); outlined, never brass.
 *  Save a file: every file type Screepub makes, each with who it is for;
 *  then the KFX checklist when Set it up opened it, then convert.js's foot. */
function drawOtherWays(offer) {
  const section = el('section', { class: 'other-ways', 'aria-label': D.OTHER_WAYS },
    el('div', { class: 'ways-group' },
      el('h3', { class: 'ways-heading' }, D.SEND_IT),
      ...D.sendItRows(drawn, offer).map(routeRow)),
    el('div', { class: 'ways-group' },
      el('h3', { class: 'ways-heading' }, D.SAVE_A_FILE),
      ...D.saveRows(ctx.state.script, kindle).map(saveRow),
      kfxOpen ? kfxNode : null,
      ...footer));
  section.hidden = !waysOpen;
  return section;
}

/** One route, as send.js drew it: its title, the engine's detail, where a
 *  mounted reader is, the email row's first-time step, and its button, or
 *  none when it is dimmed. The asterisk beside a reader nobody has tried. */
function routeRow(route) {
  const lines = R.routeLines(route, platform());
  const hint = R.emailSetupHint(route);
  const kind = D.kindOfRoute(route);
  const button = route.available ? el('button', {
    type: 'button', class: 'btn btn-outline', 'data-key': route.id,
    disabled: sending || kfxInstalling(), onclick: () => choose(route),
  }, R.buttonLabel(route, kindle)) : null;
  return el('div', { class: route.available ? 'device-row' : 'device-row route-unavailable' },
    el('div', { class: 'device-what' },
      el('p', { class: 'device-name' }, lines.title),
      kind === null ? null : asterisk(kind),
      el('p', { class: 'route-detail' }, lines.detail),
      lines.where === null ? null : el('p', { class: 'device-where' }, lines.where),
      hint === null ? null : el('p', { class: 'caption route-hint' }, hint.line, ' ',
        el('button', {
          type: 'button', class: 'btn-quiet', 'data-key': hint.key, disabled: sending || kfxInstalling(),
          onclick: () => keepFocus(hint.key, () => perform({ key: hint.key, title: R.SETUP_TITLE })),
        }, R.SETUP_BUTTON)),
    ),
    button,
  );
}

/** One file type: who it is for, and Save…, or why not and, for a Kindle
 *  file the tools cannot make yet, Set it up. */
function saveRow(row) {
  const key = `save-file-${row.type}`;
  const act = row.available
    ? el('button', {
      type: 'button', class: 'btn btn-outline', 'data-key': key,
      disabled: sending || kfxInstalling(), onclick: () => keepFocus(key, () => saveFile(row.type)),
    }, 'Save…')
    : null;
  return el('div', { class: row.available ? 'save-row' : 'save-row route-unavailable', 'data-type': row.type },
    el('div', { class: 'device-what' },
      el('p', { class: 'device-name' }, row.label),
      el('p', { class: 'route-detail' }, row.who),
      row.fix === null ? null : el('p', { class: 'caption route-fix' }, row.fix),
      row.setup ? el('button', {
        type: 'button', class: 'btn-quiet', 'data-key': `setup-${row.type}`, onclick: openKfx,
      }, D.SET_IT_UP) : null),
    act);
}

/** Set it up: kfx.js's checklist, in place under Save a file. */
function openKfx() {
  if (kfxOpen) return;
  kfxOpen = true;
  draw();
  // After the draw: kfx.js draws only into a node that is in the page.
  mountKfx(kfxNode, {
    // A build counts: the plugin must not be swapped under a KFX conversion.
    isSending: () => sending || builds.size > 0,
    // The reader asked for it, so it is always relevant here.
    devices: () => [],
    onBusy: (on) => {
      for (const button of buttons()) if (!kfxNode.contains(button)) button.disabled = on;
      // An install just ended: the Kindle file may be a KFX from now on, and
      // on a Kindle session it is worth starting.
      if (!on) checkKindle();
    },
    restoreFocus: () => ctx.restoreFocus(),
  });
  kfxShown();
}

/** The checklist put away: off screen, and no longer asking on a focus. */
function closeKfx() {
  if (!kfxOpen) return;
  kfxOpen = false;
  kfxHidden();
  kfxNode?.remove();
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
  maybeBuildAhead();
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
 *  (for whatever book is showing now), or one that worked asks again. A
 *  check that landed meanwhile was refused as busy, so the build ahead is
 *  looked at again too. */
function afterFlow(worked, stale) {
  if (owed) {
    owed = false;
    newest = asks;
    refresh({ quick: true });
    refresh();
  } else if (worked && !stale) {
    repoll();
  }
  if (!stale) maybeBuildAhead();
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
function track(book, file, run, extension = file?.extension ?? null) {
  const entry = builds.get(book) ?? { runs: 0, file, extension };
  entry.runs += 1;
  if (file !== null) entry.file = file;
  if (extension !== null) entry.extension = extension;
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
  const words = R.buildLine(running.extension ?? null);
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
    // you unplug.", with Eject in the brass place. The engine's own line for
    // one that does not.
    const copied = D.copiedState(script.title, device, sent);
    // The eject carries its own line, so it can be said again beside the
    // next book while the reader is still waiting to be ejected.
    say(copied.eject === null ? copied : { ...copied, eject: { ...copied.eject, line: copied.line } });
    // Eject is drawn only on the calm screen (spec decision 12: a USB copy
    // ends with it), and the heads-up can start a copy while the cards or
    // "My device isn't here" are up. So the panel goes calm; and with no
    // reader chosen yet, this one is chosen, stored as a card press stores
    // it, quietly: the copy's line is what the reader needs to see.
    if (copied.eject !== null && mode !== 'calm') {
      if ((ctx.state.readerDevice ?? null) === null && D.KINDS.includes(device.kind)) {
        ctx.state.readerDevice = device.kind;
        runEngine(argv.readerDevice(device.kind)).catch(() => {});
      }
      mode = 'calm';
    }
    draw();
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

/** Eject the reader the last copy left mounted (`screepub eject`, which
 *  refuses any volume it would not list as a reader). Under the same
 *  `sending` flag as every other flow, and on no book's turn: it touches no
 *  book. Done or unplugged, the button goes; refused, it stays, with the
 *  engine's sentence. */
async function ejectNow() {
  const eject = ejectPending();
  if (eject === null || sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  const stale = () => era !== mine;
  for (const button of buttons()) button.disabled = true;
  try {
    kfxRedraw();
    say({ line: `Ejecting ${eject.name}…`, bad: false });
    const answer = await runEngine(argv.eject(eject.volume));
    if (stale()) return;
    say(D.ejectOutcome(answer, eject));
  } catch (err) {
    if (!stale()) say({ line: err.message, bad: true });
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    if (!stale() && mode === 'calm') draw();
    afterFlow(false, stale());
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

/** Save one file type where the Save box says. The type is the extension,
 *  so the box comes first; the engine builds (or finds) the file and writes
 *  it there (`export --for <type> --out`). The window never writes a file.
 *  A Kindle file waits in the Kindle line behind any build of this book, and
 *  puts the build line up unless the check said that very file is current;
 *  EPUB and Fountain only copy. Under the same `sending` flag and `era`
 *  discipline as sendTo() and perform(). */
async function saveFile(type) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  const stale = () => era !== mine;
  for (const button of buttons()) button.disabled = true;
  artifactNote.hidden = true;
  try {
    kfxRedraw();
    await ensureSettings();
    if (stale()) return;
    const script = ctx.state.script;
    const book = script.epubPath;
    const row = D.SAVE_TYPES.find((t) => t.type === type);
    const out = await saveDialog({
      defaultPath: R.saveNameFor(book, type), filters: R.saveFiltersFor(type, row.label),
    });
    if (stale() || out === null) return;
    const call = argv.export(book, {
      forFormat: type, fountain: script.fountainPath, optionsJson: R.optionsJsonFor(script), out,
    });
    let answer;
    if (D.saveTurn(type) === 'kindle') {
      say(R.statusFor('idle'));
      const same = kindleBook === book && kindle?.extension === type;
      const run = inKindleLine(book, () => onBook(book, 'kindle', call));
      answer = await (same && !R.mayBuild(kindle) ? run : track(book, same ? kindle : null, run, type));
      if (answer?.ok === true && same) builtNow(book);
    } else {
      say(R.statusFor('saving'));
      answer = await onBook(book, 'copy', call);
    }
    if (stale()) return;
    say(answer?.ok === true
      ? { line: `Saved ${R.baseName(answer.path)}.`, bad: false }
      : R.statusFor('failed', { detail: R.failureMessage(answer) }));
  } catch (err) {
    if (!stale()) say(R.statusFor('failed', { detail: err.message }));
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
    afterFlow(false, stale());
  }
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
