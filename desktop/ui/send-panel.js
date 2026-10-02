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
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';
import { newIssueUrl, osLabel } from './feedback.js';
import { RELEASE } from './notes.js';
import * as D from './device.js';
import * as R from './send-routes.js';

let ctx = null;
let host = null;
/** The part redrawn between cards, calm and missing. */
let body = null;
let statusLine = null;
let unsubscribe = null;
let showing = false;
/** 'cards' | 'calm' | 'missing', or null until the reader's device is known
 *  (nothing is drawn before then, so the cards never flash up for someone
 *  who already chose). */
let mode = null;

const platform = () => navigator.userAgentData?.platform ?? navigator.platform;
const devicesNow = () => ctx.devices?.current?.() ?? null;

export function mountPanel(node, context) {
  unsubscribe?.();
  host = node;
  ctx = context;
  mode = null;
  statusLine = el('p', { class: 'caption send-status', role: 'status' }, '');
  body = el('div', { class: 'panel-body' });
  clear(host);
  host.append(body, statusLine);
  unsubscribe = ctx.devices?.subscribe?.(onDevices) ?? null;
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
  if (mode === 'cards') markCards(list);
}

// ------------------------------------------------------------------- calm

function drawCalm() {
  const kind = ctx.state.readerDevice;
  return el('div', { class: 'calm' },
    D.unproven(kind, platform()) ? asterisk(kind) : null,
    el('div', { class: 'calm-ways' },
      el('button', { type: 'button', class: 'btn btn-outline', 'data-key': 'other-ways', 'aria-expanded': 'false' },
        D.OTHER_WAYS),
      el('button', {
        type: 'button', class: 'btn-quiet', 'data-key': 'different',
        onclick: () => { mode = 'cards'; draw(); },
      }, D.DIFFERENT)));
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
