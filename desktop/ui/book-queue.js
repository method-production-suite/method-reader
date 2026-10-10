// Turns on a book: one queue per library EPUB, readers and writers, so no
// engine call reads the book while another is rewriting it.
//
// Three pages touch a script's library EPUB. The Settings page's save
// rebuilds it in place (tune.js's reconvert), and converting the same PDF
// again writes it afresh (convert.js): those WRITE it. The send panel builds
// the Kindle file from it, copies it, sends it and checks its date: those
// only READ it, except the engine's own MOBI rung, which rewrites the EPUB in
// place before it writes the .mobi beside it. Unordered, a send could start
// before a moved knob's rebuild landed and ship the book without it, or a
// rebuild could replace the file under a send reading it.
//
// So every such engine call takes its turn here, keyed by the EPUB's path:
//  - Readers run side by side. Save the EPUB pressed while the Kindle file
//    builds in the background (half a minute of Kindle Previewer) starts at
//    once rather than waiting that out.
//  - A writer runs alone: it waits for every reader running to finish, and
//    nothing starts beside it.
//  - Arrival order is kept. A reader that arrives while a writer waits
//    queues behind that writer rather than slipping past it, so a Settings
//    change always lands before a later send reads the book, and a writer is
//    never starved by readers that keep arriving.
//
// One queue PER BOOK, not one for the window: a new script's saves must
// not wait out the old script's KFX build, and an engine call that never
// answers holds up its own book and nothing else.
//
// Two Kindle file exports of one book must not run at once either (both
// would drive Kindle Previewer at the same scratch file), but that is a rule
// about the Kindle FILE, not the book, and send-panel.js keeps it on its own.
//
// Pure: no Tauri, no DOM, nothing but promises, so tests drive it directly.

const READ = 'read';
const WRITE = 'write';

/** Every turn a page asks for, and what it does to the book: the one place
 *  a call site's label says whether it reads the library EPUB or writes it.
 *  A label with no row here is refused, never guessed at. holders() lists
 *  the labels running on a book; the Settings page names the one it waits
 *  for from them. */
export const TURNS = {
  // tune.js: a moved knob's settings stored, then the EPUB rebuilt in place.
  save: WRITE,
  // convert.js: the same PDF converted again writes the EPUB afresh.
  convert: WRITE,
  // send-panel.js: a Kindle file, built beside the EPUB without touching it:
  // the ladder's KFX (Calibre) or Screepub's own AZW3, the build the result
  // starts as it appears and Copy to a Kindle; and every Save of a KFX, an
  // AZW3 or a MOBI (`export --for <type>`). No Kindle file rewrites the
  // EPUB any more (the ladder's MOBI rung once did, on a computer without
  // Calibre, and took the book alone as a writer), so every one is a reader.
  kindle: READ,
  // send-panel.js: a reader that takes the EPUB as it is (the export that finds
  // it, and the copy to a Kobo, a tolino or a reMarkable), and the copy of
  // any file to any reader, a Kindle's included.
  send: READ,
  // send-panel.js: a Save of the EPUB, the Fountain or the FDX (`export
  // --for epub`, `--for fountain`, `--for fdx`), which copies the file, or
  // for the FDX writes it from the .fountain, where the Save box said.
  copy: READ,
  // send-panel.js: `export --check`, the Kindle file's date against the book's.
  check: READ,
};

/** book (an EPUB path) -> { running: the turns running now, oldest first,
 *  waiting: the turns queued behind them, in arrival order }. A book with
 *  nothing running or queued has no entry, so this never grows past the
 *  books that are busy right now. */
const queues = new Map();

/** What a page still owes the books, run before any turn is asked for. */
const owed = new Set();

/** Register work a page owes before any other turn on a book may start:
 *  tune.js's settle, still counting down after a knob moved, is cut short
 *  here so its save (a writer) joins its book's queue AHEAD of the turn
 *  being asked for, and what is read is what the reader last set. The same
 *  function registered twice is kept once. A hook must not throw. */
export function beforeEveryTurn(hook) {
  owed.add(hook);
}

/** Start every turn at the head of the queue that may start now: readers
 *  while no writer runs, a writer only when nothing does. Stops at the first
 *  that may not, so nothing behind it overtakes it. */
function pump(book, queue) {
  while (queue.waiting.length > 0) {
    const next = queue.waiting[0];
    const blocked = next.kind === WRITE
      ? queue.running.length > 0
      : queue.running.some((turn) => turn.kind === WRITE);
    if (blocked) break;
    queue.waiting.shift();
    next.start();
  }
  if (queue.running.length === 0 && queue.waiting.length === 0 && queues.get(book) === queue) {
    queues.delete(book);
  }
}

/** Run `work` on its turn on `book`, and hand back `work`'s own promise: its
 *  answer, or its failure, reaches the caller as it was, and a failure never
 *  stops the queue. `label` is a row of TURNS, which says whether `work`
 *  reads the book or writes it, and names what the turn is so a page whose
 *  turn is waiting can say what for (holders()). A turn that may start at
 *  once starts in the caller's own tick, as it would with no queue at all.
 *
 *  `work` must not ask for a turn on the same book from inside itself: a
 *  writer would wait for itself. */
export function inTurn(book, label, work) {
  const kind = TURNS[label];
  if (kind === undefined) throw new TypeError(`book-queue.js has no turn called "${label}"`);
  for (const hook of owed) hook();
  let queue = queues.get(book);
  if (queue === undefined) {
    queue = { running: [], waiting: [] };
    queues.set(book, queue);
  }
  return new Promise((resolve, reject) => {
    const turn = { label, kind };
    const start = () => {
      queue.running.push(turn);
      let result;
      try {
        result = Promise.resolve(work());
      } catch (err) {
        result = Promise.reject(err);
      }
      result.then(resolve, reject);
      const done = () => {
        queue.running.splice(queue.running.indexOf(turn), 1);
        pump(book, queue);
      };
      result.then(done, done);
    };
    queue.waiting.push({ ...turn, start });
    pump(book, queue);
  });
}

/** The labels of the turns running on `book` right now, oldest first; empty
 *  when nothing is. A writer asked for now waits for every one of them. */
export function holders(book) {
  return queues.get(book)?.running.map((turn) => turn.label) ?? [];
}
