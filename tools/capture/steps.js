// Drives the window into one shot's state, the way a person would, then
// reports window.__captureState = 'ready' or 'failed'. The capture tool
// waits on that before it takes the picture, so a failed state is never
// photographed.

import { fail, fire } from '/tools/capture/bridge.js';

const cfg = window.__CAPTURE__;
const q = (selector) => document.querySelector(selector);

async function until(test, what, ms = 30000) {
  const start = performance.now();
  while (!test()) {
    if (window.__captureState === 'failed') throw new Error(window.__captureError);
    if (performance.now() - start > ms) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

const frames = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));

// Anything still moving (the tabs arrive for about 400 ms after a book is
// done) would be caught at whatever frame the clock allowed, and the picture
// would differ run to run. So every animation is waited out. An endless one
// never finishes and is left out rather than hung on; the window has none.
async function settled(doc) {
  const finite = doc.getAnimations()
    .filter((a) => a.effect?.getComputedTiming().endTime !== Infinity);
  await Promise.all(finite.map((a) => a.finished.catch(() => {})));
}

// The version stamp changes every release. Left in, every picture would
// change every release and the repository would grow by megabytes each
// time. Hidden, a picture changes only when the window's look does. A
// stamp that has been renamed would stay visible and do exactly that, so
// its absence fails the capture instead.
function hideStamp() {
  if (!q('.rev-stamp')) {
    throw new Error('the window has no .rev-stamp any more, so the version stamp would ' +
      'be in every picture; update hideStamp in tools/capture/steps.js');
  }
  const style = document.createElement('style');
  style.textContent = '.rev-stamp { visibility: hidden !important; }';
  document.head.append(style);
}

// The Convert page's library line says where books are saved, and in a
// capture that is the tool's own scratch library, set by SCREEPUB_LIBRARY:
// "Books are saved in /Users/Shared/Documents/Screepub, set by
// SCREEPUB_LIBRARY." No reader ever sees that. The whole slot goes (the
// line, its Change and Reset buttons, and the line under it), with
// display: none rather than visibility, so the drop area sits where it
// would with no slot at all. Hidden by a style that is in place before the
// slot is drawn, so no frame ever shows it; the slot's absence, once the
// drop well is up, fails the capture, for the same reason the stamp's does.
function hideLibrarySlot() {
  const style = document.createElement('style');
  style.textContent = '.well-library { display: none !important; }';
  document.head.append(style);
}

async function wellIdle() {
  await until(() => q('#surface-convert[data-state="idle"]'), 'the drop well');
  if (!q('#surface-convert .well-library')) {
    throw new Error('the drop well has no .well-library any more, so the capture\'s own ' +
      'library folder would be in the pictures; update hideLibrarySlot in tools/capture/steps.js');
  }
}

async function dropped() {
  await wellIdle();
  // Exactly the event Tauri fires when a file lands on the window.
  fire('tauri://drag-drop', { paths: [cfg.demoPdf], position: { x: 400, y: 300 } });
  await until(() => {
    const pane = q('#surface-convert');
    // A refusal is final: say what the window said, now, rather than wait
    // out the timeout for a book that is not coming.
    if (pane?.dataset.state === 'failed') {
      const said = (selector) => pane.querySelector(selector)?.textContent?.trim() ?? '';
      throw new Error(`the window refused the demo script: ${said('.fault')}: ` +
        `${said('.fault-body')} (${pane.dataset.errorCode ?? 'no code'})`);
    }
    return pane?.dataset.state === 'done';
  }, 'the finished book');
}

const SHOTS = {
  async drop() {
    await wellIdle();
  },
  async result() {
    await dropped();
    // The first-time question, never the calm screen: the capture's scratch
    // settings hold no reader's device, so the panel shows the device cards,
    // which need no `routes` answer. The calm screen would be drawn from what
    // this machine has (Apple Books, Amazon's app, the Mail default, the
    // Calibre and KFX tools) and differ from one computer to the next.
    await until(() => {
      if (q('#surface-convert .send-panel .calm')) {
        throw new Error('the result shows the calm screen, so the capture\'s settings name a reader; ' +
          'the picture would depend on this machine\'s apps');
      }
      return q('#surface-convert .send-panel .device-cards');
    }, 'the device cards');
    // The book's own first page, typeset, beside them.
    const frame = q('#surface-convert .result-page-frame');
    if (frame) {
      await until(() => frame.contentDocument?.querySelector('section.scene'), 'the first page');
      await frame.contentDocument.fonts.ready;
      await settled(frame.contentDocument);
    }
  },
  async read() {
    await dropped();
    q('#tab-read').click();
    await until(() => document.querySelectorAll('#surface-read .scene-rail button').length > 0,
      'the scene index');
    const frame = q('#surface-read .script-frame');
    await until(() => frame?.contentDocument?.querySelector('section.scene'), 'the script');
    // The script is its own document, with its own faces and its own clock.
    await frame.contentDocument.fonts.ready;
    await settled(frame.contentDocument);
  },
};

try {
  const run = SHOTS[cfg.shot];
  if (!run) throw new Error(`no capture steps for shot "${cfg.shot}"`);
  hideStamp();
  hideLibrarySlot();
  await run();
  await document.fonts.ready;
  await settled(document);
  await frames();
  await new Promise((r) => setTimeout(r, 250));
  if (window.__captureState !== 'failed') window.__captureState = 'ready';
} catch (error) {
  fail(error?.message ?? error);
}
