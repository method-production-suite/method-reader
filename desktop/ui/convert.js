// Convert: the drop well, the bar, the result, and the refusal.
// Four states, from brand/components/drop-well.html, progress.html,
// result-card.html and failure-notice.html.
//
// The file is in two halves. Everything this surface DECIDES — what a code
// means, what the bar reads, what the result says, what a drop is — is a
// pure exported function above the line, tested directly by
// tests/desktop-ui.test.ts. Below the line is drawing: it holds no rule of
// its own, so a live run is enough to check it.
import {
  runEngine, pickScreenplay, pickFolder, onProgress, argv, FORCE_FLAG, openUrl,
} from './app.js';
import { inTurn, holders } from './book-queue.js';
import { WAITING } from './tune.js';
import { el, clear, text } from './dom.js';
import { newIssueUrl, osLabel } from './feedback.js';
import { RELEASE } from './notes.js';

// ---------------------------------------------------------------- decisions

/** The heading names the CAUSE, not the failure. "Scanned pdf, no text"
 *  tells you what to do next; a generic apology does not. Every code
 *  src/cli-errors.ts can return on the conversion path has an entry, so a
 *  real failure never renders with a blank heading. */
export const HEADINGS = {
  'scanned': 'Scanned PDF, no text',
  'not-screenplay': 'Not a screenplay',
  'password': 'Locked PDF',
  'unreadable': 'Unreadable file',
  'unsupported-type': 'Wrong kind of file',
  'bad-options': 'Bad settings',
  'usage': 'Bad settings',
  'library': 'No way into the library',
  'unwritable': 'Screepub could not save the pictures from this script',
  'internal': 'The engine did not answer',
};

/** The window's own name. On the idle screen this is the ONLY place the app
 *  says what it is: the macOS title bar is gone, and the paragraph that used
 *  to carry the name went with it. */
export const WORDMARK = 'Screepub';

/** The drop well's words, exported the way send.js and tune.js export theirs,
 *  so a test can read the copy without mounting a surface.
 *
 *  `limits` used to name TWO of the engine's four guards, a scan and a
 *  password-locked file, on the argument that both are properties a reader
 *  can check at a glance, which moved both from after the wait to before the
 *  drop. The password half was cut deliberately (2026-09-20). It is a real
 *  trade, not a tidy-up: a locked PDF is now met AFTER the conversion wait
 *  rather than before it. What makes it survivable is that the refusal still
 *  names the cause — HEADINGS above maps `password` to "Locked PDF" and
 *  prints the engine's own sentence under it. */
export const WELL = {
  call: 'Drop a screenplay PDF',
  or: 'or',
  limits: 'Needs selectable text, not a scan.',
};

/** The one-time update question under the well, in the reader's words.
 *  Nothing is sent before it is answered (update.js, shouldAsk). */
export const ASK = {
  question: 'Check for new versions once a day?',
  yes: 'Turn on',
  no: 'No thanks',
};

/** The one guard a reader can meaningfully overrule. The others describe a
 *  file the engine genuinely cannot read, and offering an override on them
 *  would be a lie dressed as a button. */
export const OVERRIDABLE = 'not-screenplay';

/** Shown only if the engine breaks its own contract and sends a failure with
 *  no sentence in it. Every real failure renders the engine's words instead. */
export const NO_MESSAGE = 'The engine refused the file without saying why.';

/** The bar before the engine's first line arrives. */
export const PROGRESS_START = { percent: 0, stage: null, label: 'starting up' };

/** What the read-out says while a conversion waits its turn on its book
 *  (book-queue.js), by what holds it: the Settings page's own words for the
 *  same wait (tune.js's WAITING, longest wait first), so a half-minute
 *  Kindle build does not read as a conversion stuck "starting up". Null when
 *  nothing does, and the conversion starts at once. */
export function waitingLabel(ahead) {
  const labels = [].concat(ahead ?? []);
  if (labels.length === 0) return null;
  const named = Object.keys(WAITING).find((label) => labels.includes(label));
  if (named !== undefined) return WAITING[named];
  if (labels.includes('save')) return 'Waiting for the settings to finish saving…';
  return 'Waiting for the book to be free…';
}

export function shortcutLabel(platform) {
  return /mac/i.test(String(platform ?? '')) ? '⌘O' : 'Ctrl+O';
}

/** What the file manager is called where this window is running.
 *
 *  The Swift app said "SHOW IN FINDER" because it only ever ran on a Mac.
 *  This one runs on three platforms, and "Finder" on Windows is not a
 *  shorter way of saying File Explorer, it is the name of a thing that is
 *  not there. An unknown platform gets the generic phrasing rather than a
 *  guess: naming the wrong file manager is worse than naming none. */
export function revealLabel(platform) {
  const said = String(platform ?? '');
  if (/mac/i.test(said)) return 'Show in Finder';
  if (/win/i.test(said)) return 'Show in File Explorer';
  return 'Show in folder';
}

/** The engine writes one refusal for both of its faces, and for the CLI it
 *  is right: "Pass --force to convert it anyway" is the next thing to type.
 *  In a window there is nothing to type it into, and the sentence lands
 *  directly above a Convert anyway button that already does it — the
 *  interface talking past itself, and the one place the two faces disagree.
 *
 *  The rule is not "reword the engine". It is narrower and it only ever
 *  fires where the window has made the sentence redundant: when the window
 *  has drawn the override, the sentence that names the flag comes out, and
 *  nothing else does. The diagnosis — the part only the engine knows — is
 *  untouched, and a refusal the window offers no button for keeps every word.
 *
 *  Nothing is ever dropped to nothing: a message that is ONLY the remedy is
 *  left whole, because a blank fault body says less than a sentence a
 *  window user cannot act on. */
export function withoutCliRemedy(message, flag) {
  const whole = String(message ?? '').trim();
  const needle = String(flag ?? '');
  if (needle === '' || !whole.includes(needle)) return whole;
  const sentences = whole.match(/[^.!?]+[.!?]*\s*/g) ?? [];
  const kept = sentences.filter((s) => !s.includes(needle)).join('').trim();
  return kept === '' ? whole : kept;
}

/** What Show in Finder says when the engine could not reveal the file.
 *  Reuses failureFor, so a reveal refusal renders through the same rule as
 *  every other one this surface shows: the engine's own sentence, and the
 *  same fallback when it broke its contract and sent none. Success has
 *  nothing to say here: the file manager showing the file already answers
 *  the question. */
export function revealFailureMessage(answer) {
  if (answer === null || answer === undefined || answer.ok) return null;
  return failureFor(answer.error).message;
}

/** What Show in Finder should say after asking the engine to reveal `path`:
 *  a refusal, through revealFailureMessage's same rule as every other one
 *  this surface shows; the message from a crash or a broken answer (a
 *  rejected `run`, the same shape runEngine throws for either one); or ''
 *  on success. `run` is runEngine, injected here so a test can drive this
 *  without a real engine and without a real file manager. */
export async function revealNote(run, path) {
  try {
    return revealFailureMessage(await run(argv.reveal(path))) ?? '';
  } catch (err) {
    return err.message;
  }
}

/** What a failing answer means for the reader. The message is the engine's
 *  own: it says what happened, and this window is not better placed to say
 *  it. The only edit is withoutCliRemedy's, and only where this window has
 *  put a button in the sentence's place. */
export function failureFor(input) {
  // The contract says an object with both fields; a failure whose error is
  // missing or malformed still has to render something a person can act on.
  const error = input === null || input === undefined ? {} : input;
  const raw = typeof error.code === 'string' ? error.code.trim() : '';
  const code = raw === '' ? 'internal' : raw;
  const message = typeof error.message === 'string' ? error.message.trim() : '';
  // Not `code in HEADINGS`: an unknown code is a file the window knows
  // nothing about, and guessing that it can be overridden is a lie.
  const canForce = code === OVERRIDABLE;
  const said = message === '' ? NO_MESSAGE : message;
  return {
    code,
    heading: HEADINGS[code] ?? HEADINGS.internal,
    message: canForce ? withoutCliRemedy(said, FORCE_FLAG) : said,
    canForce,
  };
}

/** What the refusal says about the book that was already open. A file the
 *  engine would not read produced nothing to replace it with, so it replaces
 *  nothing: Read, Tune and Send keep the script they had. Saying so is the
 *  other half — a reader who has just been refused should not have to guess
 *  whether the work they were tuning survived. */
export function stillOpenNote(script) {
  const title = typeof script?.title === 'string' ? script.title.trim() : '';
  if (title === '') return null;
  return `${title} is still open. This changed nothing about it.`;
}

/** The bar's next position, given where it already is and one progress line.
 *  The engine's percent is ALREADY the whole pipeline's: src/convert.ts's
 *  PARSE_SHARE puts page extraction at 0..85 and rendering at 85..100, and
 *  the stage name only says which of the two the reader is waiting on.
 *  (Observed: a 3,601-page script emits parse 6..85 then render 85, render
 *  100.) Re-weighting it here would discount it twice and park the bar at
 *  72% for the whole tail of the parse. So the number is taken as given,
 *  clamped, and never allowed to fall — a late line from the stage that just
 *  ended, an unknown stage and a percent that is not a number are all
 *  ignored rather than drawn. */
export function nextProgress(previous, line) {
  const stage = line?.stage;
  const percent = Number(line?.percent);
  if (stage !== 'parse' && stage !== 'render') return previous;
  if (!Number.isFinite(percent)) return previous;
  const next = Math.min(100, Math.max(0, Math.round(percent)));
  if (next < previous.percent) return previous;
  // Standing still at the same percent is still news if the stage changed:
  // the engine crosses into rendering at 85 without the bar moving.
  if (next === previous.percent && stage === previous.stage) return previous;
  return {
    percent: next,
    stage,
    label: stage === 'parse'
      ? `reading the pages (${next}%)`
      : `building the book (${next}%)`,
  };
}

/** The name at the end of a path, whichever slash the OS used. */
export function fileName(path) {
  const parts = String(path ?? '').split(/[\\/]/).filter((p) => p !== '');
  return parts.length === 0 ? String(path ?? '') : parts[parts.length - 1];
}

/** The book introducing itself. The counts are the fastest way to confirm
 *  the parse was right: a wrong character count means the cues were misread.
 *  A count the engine did not send is left out rather than printed as
 *  "undefined pages". */
export function countLine(answer) {
  const say = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}.`;
  const first = [
    Number.isFinite(answer?.pages) ? say(answer.pages, 'page') : null,
    Number.isFinite(answer?.scenes) ? say(answer.scenes, 'scene') : null,
  ].filter((p) => p !== null).join(' ');
  const second = Number.isFinite(answer?.characters)
    ? say(answer.characters, 'speaking character')
    : '';
  return [first, second].filter((p) => p !== '').join('\n');
}

/** The one object tasks 9–11 read. Everything optional is normalised here,
 *  so no later surface has to guess whether a field is missing or empty. */
export function scriptFrom(path, answer) {
  return {
    path,
    title: answer?.title ?? fileName(path),
    author: answer?.author ?? null,
    pages: answer?.pages ?? null,
    scenes: answer?.scenes ?? null,
    characters: answer?.characters ?? null,
    warnings: Array.isArray(answer?.warnings) ? answer.warnings : [],
    epubPath: answer?.epubPath ?? null,
    fountainPath: answer?.fountainPath ?? null,
    previewHtml: answer?.previewHtml ?? '',
    settings: null,
  };
}

/** Which of the dropped paths this window converts. Tauri hands over
 *  everything that was dragged; one window converts one script. */
export function droppedPath(paths) {
  if (!Array.isArray(paths)) return null;
  return paths.find((p) => typeof p === 'string' && p.trim() !== '') ?? null;
}

/** What a clean `app-settings --json` answer gives the library line to work
 *  with, or null for anything the engine's contract doesn't actually
 *  promise. The probe that calls this is a convenience, so a malformed
 *  answer draws nothing rather than a line that misreports where books go. */
export function libraryFrom(answer) {
  if (answer === null || typeof answer !== 'object' || answer.ok !== true) return null;
  const library = answer.library;
  if (library === null || typeof library !== 'object') return null;
  const { path, chosen, platformDefault, fromEnv } = library;
  if (typeof path !== 'string') return null;
  if (chosen !== null && typeof chosen !== 'string') return null;
  if (typeof platformDefault !== 'string') return null;
  if (typeof fromEnv !== 'boolean') return null;
  if (typeof answer.home !== 'string') return null;
  return { path, chosen, platformDefault, fromEnv, home: answer.home };
}

/** The library line itself. See shortenHome for the ~ rule. */
export function libraryLine(library) {
  const { path, home, fromEnv } = library;
  const shown = shortenHome(path, home);
  return fromEnv
    ? `Books are saved in ${shown}, set by SCREEPUB_LIBRARY.`
    : `Books are saved in ${shown}.`;
}

/** `path`, with a leading `home` folded to `~`, but only when it genuinely
 *  sits under home: the folder itself, or home plus a separator and more,
 *  never a sibling that merely shares the same prefix (`/Users/ann2` is not
 *  under `/Users/ann`). Checked against both separators, because `path` may
 *  have come from either OS.
 *
 *  A path carrying a backslash, or a bare drive letter, is Windows and
 *  compares case-insensitively, the way its own filesystem does; a POSIX
 *  path does not. `home` is tried both as given and with its own trailing
 *  separator stripped, so a root home (`/`, `C:\`) matches the folder
 *  itself as `~` rather than `~/` or `~\`: comparing only the stripped form
 *  is what used to produce those, because a path equal to an UNstripped
 *  home with a separator of its own never matched the stripped root. */
function shortenHome(path, home) {
  const windows = path.includes('\\') || home.includes('\\') || /^[A-Za-z]:/.test(path);
  const fold = (s) => (windows ? s.toLowerCase() : s);
  const root = home.replace(/[/\\]+$/, '');
  if (fold(path) === fold(home) || fold(path) === fold(root)) return '~';
  for (const sep of ['/', '\\']) {
    if (fold(path).startsWith(fold(`${root}${sep}`))) return `~${path.slice(root.length)}`;
  }
  return path;
}

/** Which buttons belong beside the library line. SCREEPUB_LIBRARY wins over
 *  whatever was chosen (see argv.appSettings' own doc comment), so there is
 *  nothing to offer changing while it is set. */
export function libraryActions(library) {
  if (library.fromEnv) return [];
  return library.chosen === null ? ['change'] : ['change', 'reset'];
}

/** What Change says once it has actually moved the library. Reset does not
 *  show this: going back to the default did not convert anything either. */
export const movedLine = 'New books go here. Books already converted stay where they are.';

/** The value app-settings' own set option takes for Change (a folder) or
 *  Reset (`null`, back to the default): the one thing that differs between
 *  them, so both go through the same call in buildLibrarySlot below. */
export function libraryChangeArgs(path) {
  return JSON.stringify({ libraryPath: path });
}

/** Shown when app-settings answers in a way its own contract does not
 *  promise: ok with no usable library inside, or a refusal with no sentence
 *  at all. Not failureFor's NO_MESSAGE ("the engine refused the file"):
 *  nothing was refused here, and there is no file, so this gets its own
 *  words. */
export const NO_LIBRARY_MESSAGE = 'Screepub could not confirm where books are saved.';

/** What Change or Reset should show once the engine has answered: a clean
 *  library to redraw the line from, or a message for the secondary line.
 *  libraryFrom already rejects an answer whose `ok` is not literally true,
 *  so there is nothing left for this function to check beyond what it
 *  returns. */
export function libraryAfter(answer) {
  const library = libraryFrom(answer);
  if (library !== null) return { ok: true, library };
  const message = typeof answer?.error?.message === 'string' ? answer.error.message.trim() : '';
  return { ok: false, message: message === '' ? NO_LIBRARY_MESSAGE : message };
}

/** Whether a successful Change or Reset should say movedLine under the
 *  library line: only a Change that actually put the library somewhere new.
 *  Reset never does, because going back to the default did not convert
 *  anything either, and neither does a Change that picked the folder
 *  already in use, because nothing moved. */
export function showsMovedLine(action, previousPath, library) {
  return action === 'change' && library.path !== previousPath;
}

// ------------------------------------------------------------------ drawing

/** The home folder, as the library probe last reported it: a bug report
 *  replaces it with ~ (feedback.js). Null until the probe has answered. */
let knownHome = null;
let ctx = null;
let pane = null;
let chooseButton = null;
let unlistenProgress = null;
let busy = false;
/** The library book each PDF converted in this window landed as, read off
 *  the engine's answer. Converting one of them again writes that book
 *  afresh, the same file the Settings page's save rebuilds and the Send
 *  page's send copies, so it takes its turn on it (book-queue.js). A PDF
 *  not in here has no book anything in this window has touched, so nothing
 *  can be holding it, and it converts at once. The window never works out
 *  a library path itself; the engine owns that (src/library.ts). */
const landed = new Map();

// Bumped every time drawWell rebuilds the page, so a probe or a Change/Reset
// answer that arrives after the well has been redrawn (a refusal's "Back to
// one", "Convert another", a fresh mount) can tell it no longer belongs to
// the well on screen and drop itself instead of filling a slot nobody sees.
let libraryGeneration = 0;

// The bar's width is the one value the window computes rather than declares,
// and CSP `default-src 'self'` refuses an inline style. A constructed
// stylesheet is the way in that the CSP allows; the number written into it is
// an integer 0..100 from nextProgress() and nothing else.
let fillSheet = null;
function paintFill(percent) {
  if (fillSheet === null) {
    fillSheet = new CSSStyleSheet();
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, fillSheet];
  }
  fillSheet.replaceSync(`.fill { width: ${percent}%; }`);
}

export function mount(node, context) {
  pane = node;
  ctx = context;
  drawWell();
}

export function show() {
  // The surface's first stop for a keyboard. It used to name the well's
  // button, which is only on screen in one of this surface's four states;
  // the plan in focus.js picks the first control there actually is.
  ctx.restoreFocus();
}

export function choose() {
  pickFileThenConvert();
}

/** Wired by mount-time listeners in app.js; see main.js. */
export function dragOver(on) {
  pane?.querySelector('.well')?.classList.toggle('well-targeted', on);
}

/** A file arrived from the desktop. */
export function dropPaths(paths) {
  const path = droppedPath(paths);
  if (path !== null) convertPath(path);
}

function icon() {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 26 26');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.3');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  for (const d of ['M6 3.5h9l5 5v14H6z', 'M15 3.5v5h5', 'M13 11.5v7', 'M10 15.5l3 3 3-3']) {
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', d);
    svg.append(path);
  }
  return svg;
}

/** Back to the empty state, which is this window's home: the drop well is
 *  the one screen that says what Screepub wants from you.
 *
 *  It does NOT close the script that is open. The book stays behind the well,
 *  so Read, Settings and Send stay reachable and a reader who pressed this by
 *  mistake has lost nothing. "Convert another" is an invitation, not a
 *  discard — and the engine has already done the work, so throwing it away
 *  here would be throwing away the only copy in memory. */
export function reset() {
  if (busy) return;
  drawWell();
}

/** The question, if there is one to ask. It removes itself once answered and
 *  hands the keyboard back, because the button that had it is gone. */
function askLine() {
  if (!ctx.updates?.shouldAsk()) return null;
  const answer = (on, event) => {
    // The reader could have flipped the switch in the release notes while
    // this line was still on screen: shouldAsk() would now say the
    // question is already answered, and a stale click here must not
    // overwrite that newer answer with this one.
    if (ctx.updates.shouldAsk()) ctx.updates.answer(on);
    // Found through the clicked button's own ancestor, not a variable this
    // closure would otherwise have to reach forward to.
    event.currentTarget.closest('.well-ask')?.remove();
    ctx.restoreFocus();
  };
  return el('p', { class: 'well-ask' },
    el('span', { class: 'well-ask-question' }, ASK.question),
    el('button', { type: 'button', class: 'btn-quiet', onclick: (event) => answer(true, event) }, ASK.yes),
    el('button', { type: 'button', class: 'btn-quiet', onclick: (event) => answer(false, event) }, ASK.no),
  );
}

/** The library line's one slot under the drop area: built empty by drawWell,
 *  after the update question, and filled (or refilled) in place by the
 *  probe below or by Change/Reset. This pair of paragraphs is the only DOM
 *  the feature ever owns, so nothing here ever appends a second line beside
 *  it: the drawWell/probeLibrary pair on either side of this function is
 *  what makes that true even across a redraw or a race between two probes.
 *
 *  A refusal from Change or Reset only ever replaces `secondary`
 *  (movedLine's own spot): `line` and its buttons are left exactly as the
 *  last good library drew them, so the reader can just try again. */
function buildLibrarySlot() {
  // well-library names the slot as a whole, and no stylesheet gives it a
  // rule. The README pictures hide the slot by that name, because there it
  // would name the scratch folder the pictures were taken with.
  const line = el('p', { class: 'well-ask well-library', hidden: true });
  const secondary = el('p', { class: 'well-ask well-library', hidden: true }, '');
  let buttons = [];

  function showLibrary(library) {
    const words = el('span', {}, libraryLine(library));
    buttons = libraryActions(library).map((action) => el('button', {
      type: 'button',
      class: 'btn-quiet',
      onclick: () => run(action, library),
    }, action === 'change' ? 'Change…' : 'Reset'));
    clear(line);
    line.append(words, ...buttons);
    line.hidden = false;
  }

  function showSecondary(message) {
    text(secondary, message);
    secondary.hidden = message === '';
  }

  function setBusy(on) {
    for (const button of buttons) button.disabled = on;
  }

  // Change and Reset are the same shape: ask the OS for a folder (Change
  // only), send it, and show what came back. The buttons go quiet before
  // the picker even opens and come back only in the finally, so a thrown
  // picker (or a thrown engine call) cannot leave them disabled forever,
  // and a second click on either one while the picker is open cannot start
  // a second write.
  async function run(action, library) {
    setBusy(true);
    try {
      let path = null;
      if (action === 'change') {
        path = await pickFolder({ defaultPath: library.path });
        // Cancelled, or the folder already in use: nothing to send, and
        // nothing for the secondary line to say either.
        if (path === null || path === library.path) return;
      }
      const answer = await runEngine(argv.appSettings(libraryChangeArgs(path)));
      const result = libraryAfter(answer);
      if (result.ok) {
        showLibrary(result.library);
        showSecondary(showsMovedLine(action, library.path, result.library) ? movedLine : '');
      } else {
        showSecondary(result.message);
      }
    } catch (err) {
      showSecondary(err.message);
    } finally {
      setBusy(false);
    }
  }

  return { line, secondary, showLibrary };
}

/** The library line's own probe: read-only (app-settings with nothing to
 *  set never counts toward busy, see app.js's countsTowardBusy), and a
 *  convenience rather than a requirement of the well, so a dead engine or
 *  an answer that breaks the contract leaves `slot` exactly as drawWell
 *  built it: empty. `gen` is drawWell's own generation counter, captured
 *  when the probe starts; if it no longer matches by the time the probe
 *  returns, the well has been redrawn since and `slot` is no longer the one
 *  on screen, so the answer is dropped rather than filling it anyway. */
async function probeLibrary(gen, slot) {
  let answer;
  try {
    answer = await runEngine(argv.appSettings());
  } catch {
    return;
  }
  if (gen !== libraryGeneration || pane.dataset.state !== 'idle') return;
  const library = libraryFrom(answer);
  if (library === null) return;
  knownHome = library.home;
  slot.showLibrary(library);
}

function drawWell() {
  clear(pane);
  libraryGeneration += 1;
  const gen = libraryGeneration;
  chooseButton = el('button', { type: 'button', class: 'well-btn', onclick: choose },
    `Choose PDF…  ${shortcutLabel(navigator.userAgentData?.platform ?? navigator.platform)}`);

  const well = el('div', { class: 'well' },
    el('span', { class: 'well-mark', 'aria-hidden': 'true' }, icon()),
    el('span', { class: 'well-call' }, WELL.call),
    el('span', { class: 'well-or' }, WELL.or),
    chooseButton,
    el('span', { class: 'well-limits' }, WELL.limits),
  );

  // The wordmark stands where the paragraph did. An h1 because on this screen
  // it IS the page's title: nothing above it names the app any more.
  pane.append(
    el('h1', { class: 'wordmark' }, WORDMARK),
    well,
  );
  // Not passed to append() directly: append(null) would print "null".
  const ask = askLine();
  if (ask) pane.append(ask);
  // The library line's slot, after the question: built empty here and
  // handed straight to its own probe, so this is the only place either one
  // is ever created.
  const slot = buildLibrarySlot();
  pane.append(slot.line, slot.secondary);
  pane.dataset.state = 'idle';
  // The code belongs to the refusal that set it, not to the pane. Left in
  // place it would ride along on the next success — a `data-state="done"`
  // carrying `data-error-code="not-screenplay"` is exactly the wrong thing to
  // find in a bug report pasted out of the DOM.
  delete pane.dataset.errorCode;
  probeLibrary(gen, slot);
}

async function pickFileThenConvert() {
  if (busy) return;
  // Two things this no longer does. It does not guard against a second
  // picker — app.js does, at the one place that can, so the shortcut and the
  // button cannot disagree about it. And it does not put the keyboard back
  // itself: it used to re-focus the well's button, which exists on ONE of
  // this surface's four states, so cancelling a dialog over a result or a
  // refusal left the page with no focused element at all. main.js restores
  // the focus for whatever surface is showing, for every dialog. See focus.js.
  const path = await pickScreenplay();
  if (path === null) return;
  await convertPath(path);
}

/** "Convert it again", wherever it is offered (Read's blank notice, the
 *  Settings fault screen, Send's "no book" screen): convert the open
 *  script's own source again, which takes its book's turn like any other
 *  conversion of that file, and show the progress here. A script whose
 *  source this window never knew, or a conversion already under way, has
 *  nothing new to start, so those only come to this surface. */
export function convertAgain() {
  const path = ctx.state.script?.path;
  if (busy || typeof path !== 'string' || path.trim() === '') {
    ctx.goTo('convert');
    return Promise.resolve();
  }
  return convertPath(path);
}

export async function convertPath(path, { force = false } = {}) {
  if (busy) return;
  busy = true;
  // A conversion is this surface's business, wherever it was started from: a
  // drop on the Read surface and Ctrl-O on Tune both end up here. One rule,
  // one place — and it fires on a file, never on the ASK for one.
  ctx.goTo('convert');
  const book = landed.get(path);
  // Asked before the turn is, so it names what is running now. A conversion
  // rewrites the book, so it waits for every one of them.
  const progress = drawProgress(path, waitingLabel(book === undefined ? [] : holders(book)));
  let answer;
  try {
    const converting = () => {
      progress.started();
      return runEngine(argv.convert(path, { force }));
    };
    answer = await (book === undefined ? converting() : inTurn(book, 'convert', converting));
  } catch (err) {
    // Rust could not start the engine, or the engine printed something that
    // is not its contract. Its sentence is still the most useful one there
    // is, so it is shown the same way the engine's own would be.
    drawFailure({ code: 'internal', message: err.message }, path);
    return;
  } finally {
    busy = false;
    const stop = await unlistenProgress;
    if (typeof stop === 'function') stop();
    unlistenProgress = null;
  }
  if (answer.ok) {
    if (typeof answer.epubPath === 'string') landed.set(path, answer.epubPath);
    drawResult(path, answer);
  } else {
    drawFailure(answer.error, path);
  }
}

/** The progress screen. `waiting` is waitingLabel's line while the
 *  conversion waits its book's turn, or null; the read-out shows it until
 *  started() says the engine has been asked. */
function drawProgress(path, waiting = null) {
  clear(pane);
  pane.dataset.state = 'working';
  paintFill(PROGRESS_START.percent);
  const fill = el('div', { class: 'fill' });
  const readOut = el('span', { class: 'read-out', role: 'status', 'aria-live': 'polite' },
    waiting ?? PROGRESS_START.label);
  let at = PROGRESS_START;

  unlistenProgress = onProgress((line) => {
    const moved = nextProgress(at, line);
    if (moved === at) return;
    at = moved;
    paintFill(at.percent);
    text(readOut, at.label);
  });

  pane.append(
    el('h2', { class: 'slug' }, 'Converting'),
    el('p', { class: 'work-line' },
      `The pages of ${fileName(path)} reflow themselves, one scene at a time.`),
    el('div', { class: 'meter' },
      el('div', { class: 'track' }, fill),
      readOut),
    el('p', { class: 'sign-off' }, 'Please stand by:'),
  );
  // The redraw threw away whatever had the focus. This surface has no
  // control while it works, so the plan lands on the pane itself and Tab
  // still moves from there.
  ctx.restoreFocus();
  return {
    started() {
      if (waiting !== null && at === PROGRESS_START) text(readOut, PROGRESS_START.label);
    },
  };
}

function drawResult(path, answer) {
  clear(pane);
  pane.dataset.state = 'done';
  delete pane.dataset.errorCode;
  ctx.state.script = scriptFrom(path, answer);
  const script = ctx.state.script;

  // Show in Finder asks the ENGINE now (owner decision, 2026-09-23): the
  // window's old reveal permission was fixed to the library's old path, and
  // a library that can move needs a door that moves with it. The note below
  // is built into THIS result's own DOM right here, not appended once the
  // engine answers: an answer that arrives after the reader has converted
  // another script, or opened a different book, would otherwise land on
  // whatever the pane shows by then, not on the book it was actually about,
  // and a stale refusal would survive a later success too. Every update
  // below only ever touches this one element, wherever it still is.
  const note = el('p', { class: 'caption', hidden: true }, '');
  async function showInFinder() {
    const said = await revealNote(runEngine, script.epubPath);
    text(note, said);
    note.hidden = said === '';
  }

  pane.append(
    el('div', { class: 'announce' },
      el('p', { class: 'book-title' }, script.title),
      script.author ? el('p', { class: 'book-by' }, `(by ${script.author})`) : null,
      el('p', { class: 'book-count' }, countLine(answer)),
      ...script.warnings.map((w) => el('p', { class: 'caption' }, `(${w})`)),
    ),
    el('div', { class: 'send' },
      el('button', { type: 'button', class: 'btn btn-brad', onclick: () => ctx.goTo('send') },
        'Send to a reader'),
      el('div', { class: 'asides' },
        el('button', { type: 'button', class: 'btn-quiet', onclick: () => ctx.goTo('read') },
          'Read it'),
        el('button', { type: 'button', class: 'btn-quiet', onclick: () => ctx.goTo('tune') },
          'Settings'),
        // Where the book actually is. The path is printed below, but a path
        // is something you read and then have to act on yourself; this is
        // the acting.
        el('button', {
          type: 'button',
          class: 'btn-quiet',
          onclick: showInFinder,
        }, revealLabel(navigator.userAgentData?.platform ?? navigator.platform)),
        el('button', { type: 'button', class: 'btn-quiet', onclick: reset },
          'Convert another'),
      ),
    ),
    el('p', { class: 'path-note' }, script.epubPath),
    note,
  );

  ctx.scriptChanged();
  ctx.restoreFocus();
}

function drawFailure(error, path) {
  clear(pane);
  pane.dataset.state = 'failed';
  const refusal = failureFor(error);

  const ways = el('div', { class: 'ways' });
  if (refusal.canForce) {
    ways.append(el('button', {
      type: 'button', class: 'btn btn-brad',
      onclick: () => convertPath(path, { force: true }),
    }, 'Convert anyway'));
  }
  ways.append(el('button', {
    type: 'button',
    class: 'btn btn-outline',
    onclick: () => { drawWell(); ctx.restoreFocus(); },
  }, 'Back to one'));
  // The refusal is the moment someone most wants to tell you what happened,
  // and until now it was the moment the window gave them nowhere to say it.
  // The report carries the code and the engine's own sentence, so it arrives
  // already saying what the window knew.
  ways.append(el('button', {
    type: 'button',
    class: 'btn-quiet',
    onclick: () => openUrl(newIssueUrl({
      appVersion: RELEASE.version,
      osVersion: osLabel(navigator.userAgentData?.platform ?? navigator.platform),
      context: `${refusal.code}: ${refusal.message}`,
      // feedback.js takes the home folder and file names out of the report.
      home: knownHome,
    })),
  }, 'Report a bug'));

  // The script that was already open stays open: see stillOpenNote. The
  // refusal is news about the file that was just refused, not about the book
  // in the library, and clearing the open script here used to take Read,
  // Tune and Send away from a book that was still perfectly good.
  const kept = stillOpenNote(ctx.state.script);

  // Built through el() rather than appended straight to the pane, because
  // el() drops a null child and Node.append() renders it as the literal word
  // "null". The `kept` line is absent whenever no script is open, which is
  // the COMMON case on a refusal, so this screen has been printing a stray
  // "null" under its buttons. Exactly the defect send.js's drawEmpty records
  // having shipped once already, on the same mistake, which is why it is
  // worth fixing in the same shape rather than with a conditional here.
  pane.append(el('div', { class: 'fault-body-block' },
    el('p', { class: 'smash' }, 'Smash cut to:'),
    el('h2', { class: 'fault' }, refusal.heading),
    // The engine's own sentence. The only thing the window takes out of it is
    // an instruction to type the flag the window has already drawn a button
    // for; see withoutCliRemedy.
    el('p', { class: 'fault-body' }, refusal.message),
    ways,
    kept === null ? null : el('p', { class: 'kept-note' }, kept),
    // The code is a support handle, not a sentence: it is what someone quotes
    // in a bug report, and printing it as body text under the buttons read as
    // leftover debug output. Labelled and set in the code face, it is
    // findable without pretending to be something a reader was told.
    el('p', { class: 'code-note' },
      el('span', { class: 'code-note-label' }, 'Error code'),
      el('code', { class: 'code code-chip' }, refusal.code)),
  ));
  // Also on the pane, for a bug report pasted out of the DOM and for anyone
  // reading the window with a tool rather than eyes.
  pane.dataset.errorCode = refusal.code;
  ctx.restoreFocus();
}
