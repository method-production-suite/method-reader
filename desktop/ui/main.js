// Boots the window: the frame, the three surfaces, the release notes, the
// keyboard, the devices poll while a book is open, and the one line that
// proves the engine is there.
import { runEngine, argv, onFileDrag, onDialogClosed } from './app.js';
import { shouldAsk, updateLabel, labelActionable } from './update.js';
import { flow } from './update-flow.js';
import { mountFrame } from './frame.js';
import { stopAfterDialog } from './focus.js';
import { el } from './dom.js';
import * as convert from './convert.js';
import * as read from './read.js';
import * as tune from './tune.js';
import * as notes from './notes-surface.js';
import { createWatcher } from './device-watch.js';

/** Everything the surfaces share. One object, passed in, never read as a
 *  global from another module. */
export const state = {
  script: null,
  /** What is plugged in, as the devices poll last listed it. */
  devices: [],
  /** The reader's device (app settings `readerDevice`): undefined until the
   *  send panel has read it, then null for none chosen, or a kind. */
  readerDevice: undefined,
  /** What the last send said, and its Eject button, kept with the book it
   *  was about so the result draws them again on any return. */
  sendResult: null,
  reducedMotion: matchMedia('(prefers-reduced-motion: reduce)').matches,
};

// What is plugged in, asked every two seconds while a book is open (spec
// decision 16). scriptChanged below starts and stops it; the send panel and
// the Kindle heads-up listen through context.devices.
const watcher = createWatcher({ run: runEngine });
watcher.subscribe((list) => { state.devices = list; });

const root = document.getElementById('app');
const frame = mountFrame(root);

// Notes is not in the bar. It is what changed in the version you are running,
// so it is reached from the stamp that names that version, and it arrives as
// a sheet over the page rather than a place you navigate to and have to come
// back from.
//
// A real <dialog> rather than a hand-rolled panel: it brings Esc, the focus
// trap, the inert background and the restored focus with it, all of which
// this window would otherwise have to write and keep right.
const notesSheet = el('dialog', { class: 'sheet-over', 'aria-label': 'Release notes' });
const notesBody = el('div', { class: 'sheet-body' });
notes.mount(notesBody);
notesSheet.append(
  el('button', {
    type: 'button',
    class: 'btn-quiet sheet-close',
    onclick: () => notesSheet.close(),
  }, 'Close'),
  notesBody,
);
root.append(notesSheet);
// The sheet is built once, above, at boot; the switch inside it was read
// from storage then too. show() resyncs it, so an answer given later under
// the drop well on the Convert page is not stuck showing off until relaunch.
frame.onRev(() => {
  notes.show();
  notesSheet.showModal();
});

const surfaces = { convert, read, tune };

/** The surfaces that have nothing to show until a script is open. */
const NEEDS_SCRIPT = ['read', 'tune'];

// Which surface is on screen. Kept here rather than asked of the frame,
// because it is what decides where the keyboard goes back to.
let showing = 'convert';

/** Put the keyboard somewhere it can carry on from, for whatever surface is
 *  actually on screen. The rule is focus.js's; this is the whole of the
 *  binding — query the plan's selectors in order, take the first node that
 *  can really take the focus, focus it.
 *
 *  Called after every native dialog (app.js says when, for every dialog it
 *  ever opens, not just this one path) and by any surface that has just
 *  redrawn the element the keyboard was standing on. */
function restoreFocus() {
  const stop = stopAfterDialog(showing, (selector) => document.querySelectorAll(selector));
  stop?.focus();
  return stop;
}

onDialogClosed(restoreFocus);

const context = {
  state,
  goTo: (id) => frame.setSurface(id),
  restoreFocus,
  /** The one-time update question under the drop well. The rule is
   *  update.js's; the answer goes to the flow. */
  updates: {
    shouldAsk: () => shouldAsk(flow.usable(), localStorage),
    answer: (on) => flow.answer(on),
  },
  /** "Convert it again" on any surface: the open script's source, converted
   *  again on Convert. convert.js owns the conversion; this only carries it. */
  convertAgain: () => convert.convertAgain(),
  /** What is plugged in: the watcher's last list, and its changes. */
  devices: {
    current: () => watcher.current(),
    subscribe: (fn) => watcher.subscribe(fn),
  },
  /** Read's and Settings' Send: the open book's result, on Convert, with the
   *  keyboard on its first control. */
  sendBook() {
    convert.showResult();
    if (showing === 'convert') restoreFocus();
    else frame.setSurface('convert');
  },
  /** Called whenever the script on screen changes, so the other surfaces
   *  stop showing a book that is no longer open, and the devices poll runs
   *  exactly while one is. */
  scriptChanged() {
    const open = state.script !== null;
    for (const id of NEEDS_SCRIPT) frame.enable(id, open);
    if (open) watcher.start();
    else watcher.stop();
    for (const surface of Object.values(surfaces)) surface.scriptChanged?.();
  },
};

const panes = {};
for (const [id, surface] of Object.entries(surfaces)) {
  const pane = el('section', {
    class: 'surface',
    id: `surface-${id}`,
    role: 'tabpanel',
    'aria-labelledby': `tab-${id}`,
    tabindex: '0',
    hidden: true,
  });
  frame.sheet.append(pane);
  panes[id] = pane;
  surface.mount(pane, context);
}

frame.onSurface((id) => {
  showing = id;
  for (const [name, pane] of Object.entries(panes)) {
    const on = name === id;
    pane.hidden = !on;
    if (on) surfaces[name].show?.();
    else surfaces[name].hide?.();
  }
});

// A file dragged onto the window lands on Convert, whatever was on screen.
// Tauri takes the drop before the webview sees it, so this is the only way
// the paths reach the page at all.
onFileDrag({
  over: (on) => convert.dragOver(on),
  drop: (paths) => convert.dropPaths(paths),
});

// Ctrl/Cmd-O opens a script from anywhere. Rust registers no menu, so the
// window owns its own shortcuts; convert.js prints the platform's spelling.
//
// It does NOT move to Convert first. Converting a file does (convert.js owns
// that, for the shortcut and for a drop alike), but ASKING for one should
// not: someone who hits the shortcut on Tune and then changes their mind
// used to be left on a surface they had not asked for, with the work they
// were doing off screen. Cancelling now leaves them exactly where they were
// — which is also what makes the focus rule above general rather than a
// dressed-up special case for Convert.
addEventListener('keydown', (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'o') {
    event.preventDefault();
    convert.choose();
  }
});

// No script yet, so Read and Settings start out of reach. Going through
// scriptChanged rather than two enable() calls keeps one rule for which
// surfaces a script unlocks.
context.scriptChanged();
frame.setSurface('convert');

// The stamp already names the build, so a working engine needs no
// announcement. A broken one does: this is still the only check that the
// sidecar can be started at all, and without it the first sign of trouble
// would be a conversion that never begins.
runEngine(argv.version()).then(
  () => {},
  (err) => frame.engineFailed(err.message),
);

// Updates. update-flow.js owns the one run and the moment it is in; the frame
// only shows the words and reports a click, and notes-surface.js listens to
// the same flow, so the label and the release notes never disagree.
//
// flow.boot() is the once-a-day check the README promises: it redraws what an
// earlier check found, then asks the server only if the reader said yes to
// the question on the Convert page, and at most once a day (update.js).
flow.subscribe((phase) => {
  const words = updateLabel(phase);
  frame.setUpdateLabel(words, { actionable: labelActionable(phase) });
  // The label can hide out from under the keyboard: a fresh check saying
  // "nothing newer" while the reader was standing on it (Tabbed to it, or
  // just clicked it) would otherwise drop focus to the body with no way
  // back in one Tab press.
  if (words === null && document.activeElement?.id === 'rev-update') restoreFocus();
});
frame.onUpdateClick(() => flow.start());
flow.boot();
