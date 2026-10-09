// "Kindle plugged in. Copy <Title>?" (spec 2026-10-01, decision 16): a short
// line that slides in at the top of whatever page is showing, once per
// plug-in, and goes by itself after eight seconds. It never sends without a
// press. The rule is device.js's headsUpFor; this only draws it.
//
// Not on Convert's result, where the Kindle's card and brass button already
// say the same, and not while a conversion runs: main.js says when those
// are (onResult, busy).
import { el, text } from './dom.js';
import * as D from './device.js';

export function mountHeadsUp(sheet, {
  watcher, state, onResult, busy = () => false, copyTo, restoreFocus = () => {}, timers = globalThis,
}) {
  const line = el('span', { class: 'heads-up-line' });
  /** The Kindle on offer while the line is up. */
  let offered = null;
  let timer = null;
  /** The Kindle ids of the last list, or null before the first (a seed). */
  let seen = null;

  function close() {
    // The keyboard is not left on a button that is about to go.
    const held = box.contains(document.activeElement);
    box.hidden = true;
    offered = null;
    if (timer !== null) timers.clearTimeout(timer);
    timer = null;
    if (held) restoreFocus();
  }

  const box = el('div', { class: 'heads-up', role: 'status', hidden: true },
    line,
    el('button', {
      type: 'button', class: 'btn btn-brad',
      onclick: () => {
        const device = offered;
        close();
        // A conversion started while the line was up (a PDF dropped on
        // Convert does not change the page, so nothing rechecked): Convert
        // would refuse the copy without a word, so none is asked for.
        if (device !== null && !busy()) copyTo(device);
      },
    }, D.HEADS_UP_COPY),
    el('button', { type: 'button', class: 'btn-quiet', onclick: close }, D.HEADS_UP_LATER));

  // Right under the tabs: the sheet's first child is the tablist.
  const top = sheet.firstChild;
  sheet.insertBefore(box, top === null ? null : top.nextSibling);

  watcher.subscribe((devices) => {
    const next = D.headsUpFor(seen, devices, {
      open: state.script !== null && !busy(),
      onResult: onResult(),
    });
    seen = next.seen;
    if (next.show === null) return;
    offered = next.show;
    text(line, D.headsUpLine(state.script.title));
    box.hidden = false;
    if (timer !== null) timers.clearTimeout(timer);
    timer = timers.setTimeout(close, D.HEADS_UP_MS);
  });

  return {
    close,
    /** The page under it changed (another tab, a result drawn): the line
     *  never stays over the result, where the brass button says the same,
     *  nor over a conversion. */
    recheck() {
      if (!box.hidden && (onResult() || busy())) close();
    },
    /** No script open: the next list seeds again, so a Kindle left plugged
     *  in is not news to the next book. */
    reset() {
      close();
      seen = null;
    },
  };
}
