// The KFX checklist ("Best Kindle quality") that Set it up opens in the send
// panel's Other ways: can this computer make KFX,
// and if not, the one thing to press for each missing piece.
//
// The checklist is the ENGINE's (src/export/kfx-setup.ts, via
// `screepub kfx-status`): which of Calibre, Kindle Previewer and the KFX
// plugin are present, what each missing one needs, and what a Kindle gets
// today. This file draws that answer and never re-decides it. It adds only
// what is about the window: when the block is worth showing, what a button
// says while it works, and what to say after.
//
// Two halves, like convert.js: pure exported decisions above the line, tested
// directly by tests/desktop-ui.test.ts, and drawing below it.
import { runEngine, argv, openUrl } from './app.js';
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';

// ---------------------------------------------------------------- decisions

export const HEADING = 'Best Kindle quality';
export const INSTALLED = 'Installed';
export const INSTALLING = 'Downloading and installing the KFX plugin. This takes a few seconds.';
export const NO_REASON = 'The KFX plugin was not installed, and nothing said why.';

const KINDS = new Set(['link', 'install', 'after', 'unavailable']);
const IDS = ['calibre', 'previewer', 'plugin'];

const isText = (value) => typeof value === 'string' && value.trim() !== '';

// Rebuilt clean rather than passed through: the engine's object is trusted
// for its SHAPE, not kept by reference, so a stray extra key it happened to
// attach never rides along into what this file draws.
function fixFrom(fix) {
  if (fix === null) return null;
  if (typeof fix !== 'object' || Array.isArray(fix) || !KINDS.has(fix.kind)) return undefined;
  if (fix.kind === 'link') {
    return isText(fix.label) && isText(fix.url) ? { kind: fix.kind, label: fix.label, url: fix.url } : undefined;
  }
  if (fix.kind === 'install') {
    return isText(fix.label) ? { kind: fix.kind, label: fix.label } : undefined;
  }
  return isText(fix.why) ? { kind: fix.kind, why: fix.why } : undefined;
}

function stepFrom(step, id) {
  if (typeof step !== 'object' || step === null || step.id !== id) return null;
  if (!isText(step.name) || typeof step.installed !== 'boolean') return null;
  const fix = fixFrom(step.fix);
  if (fix === undefined) return null;
  // The engine's rule: a fix exactly when the step is not installed.
  if ((fix === null) !== step.installed) return null;
  return { id, name: step.name, installed: step.installed, fix };
}

/** The checklist, whole, or null. A broken contract draws NOTHING: this
 *  block is advice, and "the probe answered strangely" is not something a
 *  reader can act on. */
export function checklistFrom(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  if (typeof value.ready !== 'boolean' || typeof value.possible !== 'boolean') return null;
  if (!isText(value.summary) || !Array.isArray(value.steps) || value.steps.length !== IDS.length) return null;
  const steps = IDS.map((id, i) => stepFrom(value.steps[i], id));
  if (steps.some((step) => step === null)) return null;
  return { ready: value.ready, possible: value.possible, summary: value.summary, steps };
}

/** `kfx-status --json`'s answer as a checklist, or null. */
export function setupFrom(answer) {
  return answer?.ok === true ? checklistFrom(answer) : null;
}

/** Kindle advice is for Kindles. Shown with nothing connected (the reader
 *  is deciding) or with a Kindle connected; hidden when only other readers
 *  are. An unknown list (the first poll has not answered) is not relevant
 *  YET, so the block does not flash up and vanish a moment later. */
export function kindleRelevant(devices) {
  if (!Array.isArray(devices)) return false;
  return devices.length === 0 || devices.some((device) => device?.kind === 'kindle');
}

/** Whether the block is drawn at all. A ready machine sees nothing, except
 *  right after an install, when the success line needs somewhere to stand.
 *  While an install runs, and right after it, the block stays up whatever
 *  is connected: the Installing line and then the result answer what the
 *  reader pressed, and a Kobo plugged in meanwhile does not change that. */
export function showSetup(setup, devices, justInstalled, installing = false) {
  if (setup === null || setup === undefined || !setup.possible) return false;
  if (installing === true || justInstalled === true) return true;
  if (!kindleRelevant(devices)) return false;
  return !setup.ready;
}

/** What stands to the right of one step. `busy` is true while a send, a
 *  Kindle file build (send-panel.js starts one as the result appears) or an install is
 *  running: the plugin must not be swapped under a running KFX conversion.
 *  Opening a web page is harmless at any time. */
export function controlFor(step, busy) {
  const fix = step.fix;
  if (fix === null) return { type: 'status', text: INSTALLED };
  if (fix.kind === 'link') return { type: 'link', label: fix.label, url: fix.url };
  if (fix.kind === 'install') return { type: 'install', label: fix.label, disabled: busy === true };
  return { type: 'status', text: fix.why };
}

/** What the status line says after a successful install. `ready` is the
 *  CHECKED checklist's own field, never the engine's raw `answer.setup.ready`
 *  taken on faith: afterInstall() passes the same value checklistFrom() just
 *  validated (or the previous checklist, when the fresh one did not pass),
 *  so this never claims "Kindles now get KFX." for a checklist the validator
 *  actually rejected. Removed forks are named: Screepub took out something
 *  the reader installed (usually the Swift app's copy), and that deserves
 *  saying. */
export function installedLine(version, removed, ready) {
  const parts = [`Installed the KFX plugin ${version}.`];
  if (ready === true) parts.push('Kindles now get KFX.');
  const names = Array.isArray(removed) ? removed.filter(isText) : [];
  if (names.length === 1) parts.push(`Removed an older copy: ${names[0]}.`);
  else if (names.length > 1) parts.push(`Removed older copies: ${names.join(', ')}.`);
  return parts.join(' ');
}

/** The engine's own sentence for a refusal, or a stand-in. */
export function failedLine(answer) {
  const said = answer?.error?.message;
  return isText(said) ? said.trim() : NO_REASON;
}

/** When the OS will not open a link, the reader can still get there by hand. */
export function linkFailedLine(url) {
  return `Could not open the page. It is at ${url}`;
}

/** Everything an install's answer changes, in one place: the checklist to
 *  draw, the line to say, whether it is an alarm, and whether the block
 *  stays up on a now-ready machine. A success whose checklist is broken keeps
 *  the old one rather than blanking the block under the success line. */
export function afterInstall(answer, previous) {
  if (answer?.ok === true && isText(answer.version)) {
    // Checked once, then reused for both the setup this returns and the
    // "Kindles now get KFX." sentence: the two must agree on the same
    // validated checklist, never on the engine's unchecked raw object.
    const setup = checklistFrom(answer.setup) ?? previous;
    return {
      setup,
      line: installedLine(answer.version, answer.removed, setup?.ready === true),
      bad: false,
      justInstalled: true,
    };
  }
  return { setup: previous, line: failedLine(answer), bad: true, justInstalled: false };
}

// ------------------------------------------------------------------ drawing

/** The node the block lives in, and what send-panel.js lends it. */
let host = null;
/** { isSending(), devices(), onBusy(busy), restoreFocus() } */
let hooks = null;
/** The parts of the block that outlive a redraw on one host. mountKfx()
 *  builds the frame once; a redraw refills the summary and rebuilds only the
 *  rows. The status line above all must stay the SAME node: a screen reader
 *  announces a change to a live region that already exists, and usually
 *  says nothing about one that arrives with its words already in it. */
let parts = null;
/** The last checklist the engine gave. About the MACHINE, not the script,
 *  so it survives send-panel.js redrawing the panel for a new script. */
let setup = null;
let justInstalled = false;
let installingNow = false;
let probing = false;
/** Installs begun so far. A probe already out when one began carries an
 *  answer older than the install's own, so it checks this on its way back
 *  and drops what it got if the count moved. Same shape as send-panel.js's
 *  `era`. */
let installs = 0;
let status = { line: '', bad: false };
/** Whether the checklist is on screen, between kfxShown() and kfxHidden(). */
let shown = false;

/** Draw into `node` from now on. Called from send-panel.js's openKfx(), which
 *  rebuilds the page whenever the script changes, so each call brings a
 *  fresh node and the block's frame is built into it here, once. */
export function mountKfx(node, options) {
  host = node;
  hooks = options;
  parts = {
    summary: el('p', { class: 'prose' }),
    rows: el('div', { class: 'devices' }),
    status: el('p', { class: 'caption send-status', role: 'status' }),
  };
  clear(host);
  host.append(el('p', { class: 'state-label' }, HEADING), parts.summary, parts.rows, parts.status);
  draw();
  // A script can arrive while Send already shows its no-script state. The
  // show's probe found nowhere to draw and asked nothing, so this mount is
  // the first chance to ask. Once the machine is known, a remount for a new
  // script asks nothing: the checklist is about the computer, not the script.
  if (shown && setup === null) probe();
}

/** The checklist came into view: ask the engine, and ask again whenever the
 *  window gets the focus back (the reader went to install Calibre and came
 *  back). An install still running keeps its line, since leaving the page
 *  did not stop it. */
export function kfxShown() {
  if (!installingNow) {
    justInstalled = false;
    status = { line: '', bad: false };
  }
  shown = true;
  window.addEventListener('focus', onFocus);
  probe();
}

export function kfxHidden() {
  shown = false;
  window.removeEventListener('focus', onFocus);
}

/** The device list changed; the block may now matter, or not. */
export function kfxDevicesChanged() {
  draw();
}

/** A send started or finished; the Install button follows. */
export function kfxRedraw() {
  draw();
}

/** True while an install runs, so send-panel.js can refuse to start a send. */
export function kfxInstalling() {
  return installingNow;
}

function onFocus() {
  probe();
}

async function probe() {
  if (probing || installingNow) return;
  // send-panel.js's blocked state never mounts the block (the last
  // host, if any, is detached), and an answer there would be drawn nowhere.
  // show() draws before kfxShown(), so on the page with readers the host is
  // in place by the time this runs.
  if (host === null || !host.isConnected) return;
  probing = true;
  const mine = installs;
  let next;
  try {
    next = setupFrom(await runEngine(argv.kfxStatus()));
  } catch {
    // Advice, not a feature: a probe that failed draws nothing.
    next = null;
  } finally {
    probing = false;
  }
  // An install began while this was out: its answer is the fresher one.
  if (mine !== installs) return;
  // The routine case, a reader coming back to the window with nothing
  // changed: nothing is redrawn, so the focus and the status line stay put.
  if (JSON.stringify(next) === JSON.stringify(setup)) return;
  setup = next;
  // The machine changed under the last thing said about it ("Kindles now
  // get KFX.", or a page that would not open), so that line goes too.
  justInstalled = false;
  status = { line: '', bad: false };
  draw();
}

function busy() {
  return installingNow || hooks?.isSending?.() === true;
}

function draw() {
  if (host === null || parts === null || !host.isConnected) return;
  // Rebuilding the rows throws away whatever control the keyboard stood
  // on, so where it stood is noted first and handed back after.
  const focused = host.contains(document.activeElement) ? document.activeElement : null;
  if (!showSetup(setup, hooks?.devices?.() ?? null, justInstalled, installingNow)) {
    host.hidden = true;
    if (focused !== null) giveBackFocus(null);
    return;
  }
  host.hidden = false;
  text(parts.summary, setup.summary);
  clear(parts.rows);
  parts.rows.append(...setup.steps.map(stepRow));
  say(status);
  if (focused !== null) giveBackFocus(focused.dataset?.step ?? null);
}

/** The keyboard, back where it was: the same step's control when it can
 *  still take the focus (the Get button a reader pressed before going off to
 *  a download page, found again after the re-probe on their return).
 *  Otherwise focus.js's plan, through send-panel.js, picks the page's first stop,
 *  as it does after every other redraw in this window; the Install button,
 *  for one, is disabled the moment it is pressed. */
function giveBackFocus(step) {
  const same = step === null ? null
    : [...parts.rows.querySelectorAll('button')].find((button) => button.dataset.step === step);
  if (canFocus(same)) same.focus();
  else hooks?.restoreFocus?.();
}

function stepRow(step) {
  return el('div', { class: 'device-row' },
    el('div', { class: 'device-what' }, el('p', { class: 'device-name' }, step.name)),
    control(controlFor(step, busy()), step.id),
  );
}

/** Each button carries its step as data-step, which is how a redraw finds
 *  the same step's control again. */
function control(c, step) {
  if (c.type === 'link') {
    return el('button', {
      type: 'button', class: 'btn btn-outline', 'data-step': step, onclick: () => follow(c.url),
    }, c.label);
  }
  if (c.type === 'install') {
    return el('button', {
      type: 'button', class: 'btn btn-brad', 'data-step': step, disabled: c.disabled, onclick: install,
    }, installingNow ? 'Installing…' : c.label);
  }
  return el('p', { class: 'reader-status' }, c.text);
}

async function follow(url) {
  if (await openUrl(url)) return;
  status = { line: linkFailedLine(url), bad: true };
  say(status);
}

async function install() {
  if (installingNow || hooks?.isSending?.() === true) return;
  installingNow = true;
  installs += 1;
  let outcome;
  // Everything after the flag goes inside the try, the busy hook and the
  // first draw included: only the finally clears installingNow, and a flag
  // left standing would make send-panel.js refuse every send until a restart.
  try {
    hooks?.onBusy?.(true);
    status = { line: INSTALLING, bad: false };
    draw();
    outcome = afterInstall(await runEngine(argv.kfxInstall()), setup);
  } catch (err) {
    // runEngine throws only when the engine could not run or broke its
    // contract; its message is already written for a person. A throw from
    // the busy hook or the first draw lands here too, before the engine is
    // asked anything.
    outcome = afterInstall({ ok: false, error: { message: err?.message } }, setup);
  } finally {
    installingNow = false;
    // Guarded, as update-flow.js's notify() guards its listeners: a throw
    // from the hook would end the install right here, and the lines below
    // (the new checklist, the line that says what happened) would never run.
    try {
      hooks?.onBusy?.(false);
    } catch (err) {
      console.error('kfx.js: the busy hook threw', err);
    }
  }
  setup = outcome.setup;
  justInstalled = outcome.justInstalled;
  status = { line: outcome.line, bad: outcome.bad };
  draw();
}

/** Into the one status node. Written only when the words change: the same
 *  words rewritten into a live region can be announced again on every
 *  redraw, and a send starting or ending redraws the block. */
function say(next) {
  if (parts === null) return;
  if (parts.status.textContent !== next.line) text(parts.status, next.line);
  parts.status.classList.toggle('bad', next.bad);
}
