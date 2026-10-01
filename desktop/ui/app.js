// The window's whole share of the work: build an argv, hand it to Rust,
// parse what comes back.
//
// This is the ONLY file that touches Tauri, and the only file that knows an
// engine flag. The Rust does not know these flags and must not learn them:
// --json is this file's responsibility, and so is deciding what the answer
// means. See ADR 2026-09-12: Rust is a window, not a brain.
// tests/desktop-shell.test.ts and tests/desktop-ui.test.ts read this file
// by name; keep every invoke() and every flag literal here.

// Resolved at call time, not at import time. `window.__TAURI__` only exists
// inside the shell, and reaching for it while the module loads would make
// this file impossible to import anywhere else — including from the test
// that runs every argv builder below and checks what it produced.
const tauri = () => window.__TAURI__;

/** The flag the window's own override button stands for. It is spelled here
 *  with every other flag, and exported because the refusal the engine writes
 *  names it in prose: the surface that has already drawn the button needs to
 *  recognise the sentence telling a reader to type it. */
export const FORCE_FLAG = '--force';

/** Every argv this window ever builds. One place, so a flag cannot be spelled
 *  two ways, and so a reviewer can read the whole contract at once. */
export const argv = {
  version: () => ['--version', '--json'],

  /** The first conversion. --progress makes the engine narrate to stderr,
   *  which the shell forwards as `engine-line`; --preview-inline puts the
   *  reader's document in the answer, because the window cannot read files;
   *  --library keeps the .epub, the .fountain and the sidecar out of whatever
   *  folder the reader happened to drag the PDF from. Where the library IS
   *  belongs to the engine (src/library.ts) — this window never names a path,
   *  and reads the ones it gets back off the answer. */
  convert: (path, { force = false, optionsJson = null } = {}) =>
    [path, '--json', '--progress', '--preview-inline', '--library',
      force ? FORCE_FLAG : null,
      optionsJson ? '--options-json' : null, optionsJson].filter((a) => a !== null),

  /** A re-render from the cached .fountain, writing the library EPUB back in
   *  place so what gets sent stays what was previewed. */
  reconvert: (fountain, epubPath, optionsJson) =>
    [fountain, '--json', '--preview-inline', '-o', epubPath,
      '--options-json', optionsJson],

  /** What is plugged in. `quick` leaves out the reMarkable probe: the window
   *  polls this every two seconds while a book is open (device-watch.js), and
   *  a docked reMarkable is learnt from the send panel's full `routes`. */
  devices: ({ quick = false } = {}) =>
    ['devices', '--json', quick ? '--quick' : null].filter((a) => a !== null),

  /** Safely remove the reader at `volume`, a path `devices` listed. The
   *  engine refuses any other disk (unknown-device). */
  eject: (volume) => ['eject', volume, '--json'],

  /** The reader the person said is theirs, app-wide (the send panel's
   *  cards). Null forgets it. */
  readerDevice: (kind) => ['app-settings', '--json', '--set', JSON.stringify({ readerDevice: kind })],

  send: (file, deviceId = null) =>
    ['send', file, '--json', deviceId ? '--device' : null, deviceId]
      .filter((a) => a !== null),

  settings: (fountain, set = null) =>
    ['settings', fountain, '--json', set ? '--set' : null, set].filter((a) => a !== null),

  export: (epub, { forFormat, fountain = null, optionsJson = null, out = null }) =>
    ['export', epub, '--json', '--for', forFormat,
      fountain ? '--fountain' : null, fountain,
      optionsJson ? '--options-json' : null, optionsJson,
      out ? '--out' : null, out].filter((a) => a !== null),

  /** Which Kindle file `export --for kindle` would hand over on this
   *  computer (KFX, AZW3 or MOBI), whether the one beside the book is
   *  current, and what builds it, with nothing built. The Send page asks it
   *  when it opens, to name the file on the rows that hand one over. */
  kindleCheck: (epub) => ['export', epub, '--json', '--for', 'kindle', '--check'],

  /** Can this computer make KFX for a Kindle? Installs nothing. */
  kfxStatus: () => ['kfx-status', '--json'],

  /** Install the KFX plugin into Calibre. Fetches it from Calibre's plugin
   *  index and writes into the user's Calibre, so it is only ever built in
   *  answer to a press of the button that says so (kfx.js). */
  kfxInstall: () => ['kfx-install', '--json'],

  /** Read the app's own settings (the library folder, the format defaults)
   *  with no `set`; write them when `set` is a JSON object string, for
   *  example '{"libraryPath":"/abs"}'. A field resets to its default by
   *  naming it null INSIDE that object, for example '{"libraryPath":null}':
   *  the engine refuses a bare "null" for --set itself. */
  appSettings: (set = null) =>
    ['app-settings', '--json', set ? '--set' : null, set].filter((a) => a !== null),

  /** Show `path` in the OS file manager, selected. The engine does the
   *  revealing now (owner decision, 2026-09-23): the window hands over a
   *  path it already has and nothing more. */
  reveal: (path) => ['reveal', path, '--json'],

  /** Every route this script can be sent by, in the engine's order, read-only
   *  (parity piece B). send.js polls this the way it used to poll `devices`.
   *  `quick` leaves out the reMarkable probe, which waits out its whole
   *  timeout when no tablet is docked, so the Send page's first rows need
   *  not wait for it; the row says it is still checking. */
  routes: (epub, { quick = false } = {}) =>
    ['routes', epub, '--json', quick ? '--quick' : null].filter((a) => a !== null),

  /** Perform one route: open an app (`apple-books`, `send-to-kindle`,
   *  `email-to-kindle`), or write a copy (`save-epub`, `save-kindle`) when
   *  `out` names where. `fountain`/`optionsJson` are only ever sent for a
   *  save that has to build the file first (the Kindle route). Same
   *  null-filter idiom as `export` above: an option left at its default
   *  never reaches the engine as a flag. */
  route: (key, epub, { out = null, fountain = null, optionsJson = null } = {}) =>
    ['route', key, epub, '--json',
      out ? '--out' : null, out,
      fountain ? '--fountain' : null, fountain,
      optionsJson ? '--options-json' : null, optionsJson].filter((a) => a !== null),

  /** Open Amazon's Personal Document Settings page, where the Kindle's email
   *  address and the approved-sender list live (the Send page's email row,
   *  send.js's emailSetupHint). The ENGINE opens it by this key, as it opens
   *  every other route's app or page, so the window names no URL. No book
   *  rides along: the page is about the reader's Amazon account. */
  emailSetup: () => ['route', 'kindle-email-setup', '--json'],
};

/** How much of an unparseable answer goes in the message a person reads.
 *  A truncated answer is still the whole of whatever did arrive, and the
 *  fault body is a 52-character column: the first few lines say what went
 *  wrong, and the rest only buries the two buttons under it. */
const RAW_SHOWN = 300;

// How many COUNTED engine calls are running right now (countsTowardBusy
// leaves out the read-only ones), plus every holdEngine() hold: a surface's
// owed save still in its settle, or a native dialog that is still open. An
// update's restart waits for the engine to have been quiet a while
// (update.js, installAndRestart), because every counted call is somebody's
// work: a conversion, a copy to a Kindle, a settings file half written.
// Counted HERE because this is the one door every call goes through.
let inFlight = 0;
let idleWaiters = [];

// Idle means no counted engine call has been RUNNING for this long, not
// merely that the count last touched zero. A reviewer reproduced the bug a
// single macrotask left open: send.js's sendTo() runs settings (via its own
// ensureSettings()), export and send as an AWAITED CHAIN, and tune.js's
// flush() runs a save then a rebuild the same way; the next call in either
// chain starts again within microtasks of the one before it finishing, so
// the count touches zero BETWEEN two calls that belong to the same job, not
// just after the job ends. 500 ms covers that gap.
//
// It does not, on its own, cover every knob move. tune.js debounces a change
// behind its own 300 ms settle timer (SETTLE_MS; schedule()) before the
// change ever reaches the engine as a save, and a knob moved after a long
// quiet has no engine call running during that settle, so a restart already
// waiting on whenIdle() could fire and drop the change before tune.js ever
// asked the engine to save it. holdEngine() below closes that gap
// (2026-09-24): tune.js takes a hold when it schedules a save, and lets it
// go once the save's own engine call has started and is counted in its
// place.
export const ENGINE_QUIET_MS = 500;

// performance.now() when a counted call last finished. -Infinity so a
// whenIdle() asked before anything has ever run resolves at once, the same
// as one asked long after everything has. performance.now() rather than
// Date.now(): it is monotonic, so a clock sync, DST, or the system clock
// changing cannot make this go negative or huge and either hold a restart
// off forever or release it early.
let lastEnded = -Infinity;
let settleTimer = null;

/** Whether a call is worth counting toward "the engine is busy".
 *
 *  `devices` is not: device-watch.js polls `devices --quick` every 2 s
 *  for as long as a book is open. It is never mid-job (killing a listing
 *  loses nothing), so counting it would flash "Restarting after this
 *  finishes…" on a window that is just sitting there polling, and WITH a
 *  quiet period, could hold a restart off indefinitely for as long as a
 *  book stayed open. `eject` IS counted: it is somebody's work on a drive.
 *
 *  `kfx-status` is not either, for the same reason: kfx.js's probe() runs
 *  it when the Send tab opens and again every time the window gets focus
 *  back, read-only and never mid-job. `kfx-install` is the one that
 *  actually writes (into Calibre) and stays counted.
 *
 *  `routes` is not either, for the same reason again: it REPLACES the
 *  `devices` poll (parity piece B), so it is the thing send.js now polls
 *  every 2 s, read-only, never mid-job. `route` is the one that actually
 *  does the work a route promises: it opens an app or writes a file, so
 *  it stays counted, the way `send` and `kfx-install` already are.
 *
 *  `app-settings` without `--set` is the same shape again: the Convert page
 *  rereads the settings file every time it is shown, read-only and never
 *  mid-job. WITH `--set` it writes that file, so only the write counts, the
 *  same split as kfx-status and kfx-install above.
 *
 *  `export --check` is not counted either: it reads two file dates and asks
 *  Calibre what is installed, builds nothing, and the Send page asks it on
 *  every visit and whenever the window gets the focus back. An `export`
 *  without it builds the Kindle file and stays counted, whether a press
 *  asked for it or the page started it in the background.
 *
 *  `reveal` is not counted either, but for a different reason: it writes
 *  nothing at all, and on some Linux desktops the xdg-open call behind it
 *  can keep running until the file manager window it opened is closed,
 *  which could hold a restart off for as long as that window stayed open. */
function countsTowardBusy(args) {
  if (args[0] === argv.devices()[0] || args[0] === argv.kfxStatus()[0]
    || args[0] === argv.routes('')[0]) return false;
  if (args[0] === argv.appSettings()[0]) return args.includes('--set');
  if (args[0] === argv.kindleCheck('')[0]) return !args.includes('--check');
  if (args[0] === argv.reveal('')[0]) return false;
  return true;
}

/** Release every whenIdle() waiter if the engine has been quiet for
 *  ENGINE_QUIET_MS; otherwise arrange to try again once it has been. Called
 *  both when a counted call finishes and when whenIdle() adds a waiter, so a
 *  caller arriving after the quiet period already elapsed does not sit
 *  waiting for a moment that already passed. */
function settle() {
  if (inFlight !== 0 || idleWaiters.length === 0) return;
  const remaining = ENGINE_QUIET_MS - (performance.now() - lastEnded);
  if (settleTimer) {
    clearTimeout(settleTimer);
    settleTimer = null;
  }
  if (remaining > 0) {
    settleTimer = setTimeout(settle, remaining);
    return;
  }
  const waiters = idleWaiters;
  idleWaiters = [];
  for (const resolve of waiters) resolve();
}

/** One counted piece of work has ended, a call or a hold: the one place the
 *  count goes down, so a hold ends exactly the way a call does, and the
 *  quiet period starts again from this moment either way. */
function workEnded() {
  inFlight -= 1;
  lastEnded = performance.now();
  settle();
}

/** What every engine call is told once an update has been installed. */
export const UPDATED_MESSAGE = 'Screepub has been updated. Restart it to keep going.';

// The shells whose bundle an update has already swapped (retireEngine).
// Keyed on the shell object rather than a bare flag: a window has one
// __TAURI__ for its whole life, so in the app the two are the same thing,
// but this module is imported once per bun test process and shared by
// every test that stubs its own shell, and one test's install must not
// refuse the next test's calls.
const retiredShells = new WeakSet();

/** The shell, or undefined outside one. Never throws, unlike tauri(),
 *  which needs a `window` to exist. */
function shellNow() {
  return typeof window === 'undefined' ? undefined : window.__TAURI__;
}

/** An update has been installed: refuse every engine call from now on.
 *
 *  tauri-plugin-updater moves the new bundle into this app's own path
 *  inside downloadAndInstall, and the shell finds the engine there afresh
 *  on every spawn. So from here on, any call this window starts runs the
 *  NEW engine with THIS window's argv and JSON expectations: a renamed flag
 *  is refused as usage, a changed answer could be misread. Calls already
 *  running finish; they were spawned from the old bundle.
 *
 *  Every call is refused, not only the counted ones. Counting is about
 *  whether a restart should wait for a call, and the hazard here is the
 *  contract, which a read like `routes` or `kfx-status` carries just as a
 *  write does. The Send page shows the refusal where its routes would be,
 *  which is the right thing to tell a reader; the KFX probe draws nothing
 *  on a failure. */
export function retireEngine() {
  const shell = shellNow();
  if (shell !== null && typeof shell === 'object') retiredShells.add(shell);
}

function engineRetired() {
  const shell = shellNow();
  return shell !== null && typeof shell === 'object' && retiredShells.has(shell);
}

/** Run the engine and parse its one line of stdout, counted while it runs
 *  (countsTowardBusy names the read-only calls this skips). See
 *  runEngineOnce for what the answer means.
 *
 *  The count goes up synchronously, before the first await, so a caller
 *  that holds a restart off with holdEngine() can release its hold on the
 *  very next line after calling this, with no moment in between where
 *  neither is counted. */
export async function runEngine(args) {
  // Refused before anything is counted: a refused call is no work.
  if (engineRetired()) throw new Error(UPDATED_MESSAGE);
  const counted = countsTowardBusy(args);
  if (counted) inFlight += 1;
  try {
    return await runEngineOnce(args);
  } finally {
    if (counted) workEnded();
  }
}

/** Count as busy while no engine call is running yet: for a surface that
 *  owes the engine some work it has not asked for, because it is waiting on
 *  something first. tune.js holds a moved knob for SETTLE_MS before asking
 *  the engine to save it, and a restart waiting on whenIdle() must wait for
 *  that save exactly as it waits for a running call.
 *
 *  A hold counts exactly like an in-flight counted call: engineBusy() is
 *  true while it is held, and whenIdle() resolves only once it is released
 *  AND the quiet period has passed since. Returns the release. Releasing a
 *  second time does nothing, so a surface with more than one way out of its
 *  hold (the save starts, the schedule is cancelled, the script is closed)
 *  can never drive the count below zero and let a restart through while
 *  someone else's call is running. */
export function holdEngine() {
  inFlight += 1;
  let held = true;
  return () => {
    if (!held) return;
    held = false;
    workEnded();
  };
}

/** True while any COUNTED engine call is running, or a holdEngine() hold
 *  is held. Unlike whenIdle, this asks nothing about the quiet period: it is
 *  the instantaneous fact, not the promise a restart waits on. Its only reader is update.js's 'waiting'
 *  label (wired through update-flow.js's `busy: engineBusy`); no surface
 *  reads it to disable a button. */
export function engineBusy() {
  return inFlight > 0;
}

/** Resolves once the engine has been quiet for ENGINE_QUIET_MS: at once if
 *  it already has been (including if nothing has run yet). Always goes
 *  through settle(), the one place a waiter is ever released, so a caller
 *  can never observe "idle" a macrotask before the quiet period is actually
 *  up. */
export function whenIdle() {
  return new Promise((resolve) => {
    idleWaiters.push(resolve);
    settle();
  });
}

/** Run the engine and parse its one line of stdout.
 *  Throws an Error whose message is fit to show a person.
 *
 *  The answer arrives as a string, however big it is. That was measured
 *  rather than assumed: returning it from Rust as BYTES instead — which
 *  takes a different route through Tauri's IPC — was tried and timed in the
 *  live window at 384 KB, 1.04 MB and 3.47 MB, and was SLOWER every time
 *  (median 430 vs 330 ms, 1006 vs 718 ms, 4803 vs 3864 ms), so the decode
 *  branch it needed was taken out again. See desktop/README.md. */
async function runEngineOnce(args) {
  let stdout;
  try {
    stdout = await tauri().core.invoke('run_engine', { args });
  } catch (message) {
    // Rust rejected: it could not find or start the binary at all.
    throw new Error(String(message));
  }
  try {
    return JSON.parse(stdout);
  } catch {
    // The engine printed something that is not its contract. Show it raw
    // rather than swallowing it — this is how a dropped --json presents —
    // but only as much of it as a person can actually read.
    const shown = stdout.length > RAW_SHOWN ? `${stdout.slice(0, RAW_SHOWN)}…` : stdout;
    throw new Error(`the engine did not answer in JSON:\n${shown}`);
  }
}

// One outstanding file dialog, ever. Ctrl-O twice used to open two native
// pickers — both modal, both waiting on the same window — because the only
// guard the window had was `busy`, and `busy` is a running CONVERSION.
let dialogOpen = false;
const dialogClosed = [];

/** Called after every dialog this file opens closes, however it closed.
 *  main.js puts the keyboard back with it; see focus.js for why that is not
 *  the dialog-opening surface's business. */
export function onDialogClosed(handler) {
  dialogClosed.push(handler);
}

/** True while a native dialog is on screen. */
export function isDialogOpen() {
  return dialogOpen;
}

/** The one guard every native dialog this window opens goes through, so at
 *  most one is ever on screen: Ctrl-O and a save button must not be able to
 *  put up two at once any more than Ctrl-O twice could. Runs `open`,
 *  resolves whatever it resolves (or null, for a cancel), and always clears
 *  the guard and calls every onDialogClosed handler afterward, in a
 *  `finally`, so a rejection still cleans up and still reaches the caller
 *  (main.js needs the keyboard back either way).
 *
 *  It also holds the engine (holdEngine) for as long as the dialog is up. A
 *  dialog is the middle of someone's work with no engine call running:
 *  Save a Kindle file builds the file, then asks where, then writes it
 *  there, and Change… asks for a folder before it stores one. A restart
 *  waiting on whenIdle() must not land on top of that. Taken here, the one
 *  door every dialog passes, so no surface can forget it; taken only once
 *  the guard has let this dialog through, so an ask it refuses holds
 *  nothing and cannot release the open dialog's hold either. */
async function withOneDialog(open) {
  if (dialogOpen) return null;
  dialogOpen = true;
  const release = holdEngine();
  try {
    const result = await open();
    return result ?? null;
  } finally {
    dialogOpen = false;
    release();
    // Each handler on its own, as update-flow.js's notify() does: a throw
    // here would otherwise replace the dialog's answer with its own error,
    // losing the reader's pick, and skip every handler after it.
    for (const handler of dialogClosed) {
      try {
        handler();
      } catch (err) {
        console.error('app.js: a dialog-closed handler threw', err);
      }
    }
  }
}

/** Ask the OS for a screenplay. Null when the reader cancelled — and null,
 *  without opening anything, when a picker is already up. */
export async function pickScreenplay() {
  return withOneDialog(() => tauri().core.invoke('pick_file'));
}

/** Ask the OS for a folder to save books into. Null when the reader
 *  cancelled, and null, without opening anything, when a dialog is already
 *  up: it goes through the same one-dialog guard as pickScreenplay and
 *  saveDialog, because every one of them is modal and waits on the same
 *  window. */
export async function pickFolder({ defaultPath } = {}) {
  const picked = await withOneDialog(
    () => tauri().dialog.open({ directory: true, defaultPath }));
  // This plugin resolves a bare string for a single-folder pick on every
  // platform this window ships for, but an array (the multi-select shape)
  // or an object is normalised here rather than left for the caller to
  // guess at.
  if (Array.isArray(picked)) return picked[0] ?? null;
  if (picked !== null && typeof picked === 'object') {
    return typeof picked.path === 'string' ? picked.path : null;
  }
  return typeof picked === 'string' ? picked : null;
}

/** Ask the OS where to save a copy. Resolves to the chosen path, or null on
 *  cancel, and null, without opening anything, when a dialog is already up
 *  (shares pickScreenplay's guard above, the one door this file ever opens
 *  onto a native dialog). Owner-approved 2026-09-23: the window may show
 *  this box, but it never writes the file itself. It hands the chosen path
 *  to `argv.route`, and the engine is what writes there. */
export async function saveDialog({ defaultPath, filters }) {
  return withOneDialog(() => tauri().dialog.save({ defaultPath, filters }));
}

/** Every diagnostic line the engine writes, verbatim, as it writes it.
 *  Resolves to the function that stops listening. */
export function onEngineLine(handler) {
  return tauri().event.listen('engine-line', (event) => handler(String(event.payload)));
}

/** OS drag-and-drop. Tauri intercepts it before the webview sees it, so the
 *  HTML5 drop event never fires and the PATHS — which is all the engine can
 *  use — arrive here instead. The payload is Tauri's DragDropPayload:
 *  `{ paths: [...], position: { x, y } }` on enter and drop, and nothing at
 *  all on leave. Which of the paths to convert is not decided here; this file
 *  is the boundary, not the surface. */
export function onFileDrag({ over, drop }) {
  tauri().event.listen('tauri://drag-enter', () => over(true));
  tauri().event.listen('tauri://drag-leave', () => over(false));
  tauri().event.listen('tauri://drag-drop', (event) => {
    over(false);
    drop(event.payload?.paths ?? []);
  });
}

/** Hand a URL to the OS. The window's first and only door onto anything
 *  outside itself (ADR 2026-09-21 — doors, not commands).
 *
 *  It is HERE because this file is the only one that touches Tauri, and
 *  opening a URL is a Tauri call. The URL itself is built by feedback.js,
 *  which is pure and knows nothing about any of this.
 *
 *  What may be opened is not decided here and must not be. The capability
 *  scopes the grant to this project's issue tracker and the three Calibre
 *  and Kindle Previewer download pages the KFX checklist links to, so a URL
 *  pointing anywhere else is refused by Tauri rather than quietly followed —
 *  which is the property that makes passing a built string to it safe at
 *  all.
 *
 *  Resolves either way. A bug report that will not open is a disappointment;
 *  it is not a reason to throw into a click handler on a screen that is
 *  already showing someone a failure. */
export async function openUrl(url) {
  try {
    await tauri().opener.openUrl(String(url));
    return true;
  } catch {
    return false;
  }
}

/** Whether the updater plugin is linked into THIS build.
 *
 *  It arrives on a separate branch, so a window built before that lands has
 *  no `updater` at all. Every update affordance is drawn behind this, which
 *  means the feature appears when the plugin does and never shows a control
 *  that can only throw. */
export function updaterReady() {
  return typeof tauri()?.updater?.check === 'function';
}

/** Ask the endpoint whether there is a newer release.
 *
 *  Deliberately NOT caught here. A rejection is the answer in the failure
 *  case and carries the only description of what went wrong — a 404 manifest
 *  reads differently from no network — so it goes up to update.js, which
 *  turns it into something a person can read. Swallowing it here is how a
 *  failed check becomes an indistinguishable "you are up to date". */
export async function updateCheck() {
  return tauri().updater.check();
}

/** Download and swap the bundle, reporting bytes as they arrive.
 *
 *  `close()` releases a resource held on the Rust side, so it runs whatever
 *  happened. The plugin does not relaunch on macOS; restartApp below does,
 *  once whenIdle above says the engine is quiet enough to leave.
 *
 *  A close() that throws is logged, never thrown. After a good install the
 *  bundle is already swapped, and reporting the cleanup as a failed update
 *  would draw a "Try again" that reinstalls a bundle already on disk. After
 *  a failed download, the download's own error is the one a reader needs,
 *  and must not be replaced by the cleanup's. */
export async function updateInstall(update, onProgress) {
  try {
    await update.downloadAndInstall((event) => onProgress?.(event));
  } finally {
    try {
      await update.close?.();
    } catch (err) {
      console.error('app.js: closing the Update object failed', err);
    }
  }
}

/** Whether tauri-plugin-process is linked into THIS build, the same test
 *  updaterReady makes for the updater. A build without it falls back to
 *  asking the reader to quit and reopen. */
export function restartReady() {
  return typeof tauri()?.process?.relaunch === 'function';
}

/** Restart into whatever bundle is on disk now. After an update, that is the
 *  new version: Tauri reads Contents/Info.plist to find the binary on macOS
 *  (tauri 2.11.5, src/process.rs). Only `restart` is granted to this window;
 *  the plugin's `exit` is not. */
export async function restartApp() {
  await tauri().process.relaunch();
}

/** The progress lines, decoded. The payload is whatever the OS handed the
 *  Rust, so one event may carry several lines or a partial one; anything
 *  that is not a progress object is ignored rather than thrown, because a
 *  stray warning on stderr must not take a conversion down with it. */
export function onProgress(handler) {
  return onEngineLine((payload) => {
    for (const line of payload.split('\n')) {
      const trimmed = line.trim();
      if (trimmed === '') continue;
      try {
        const parsed = JSON.parse(trimmed);
        if (parsed && parsed.progress) handler(parsed.progress);
      } catch {
        // Not JSON, not ours.
      }
    }
  });
}
