// What is plugged in, asked every two seconds whenever a book is open (spec
// decision 16: the Kindle heads-up needs it on every page, and the result's
// cards need it to mark a reader the computer can see). main.js starts it
// when a script opens and stops it when none is. `devices --quick` skips the
// reMarkable probe; the send panel's full `routes` answer finds a docked one.
// Uncounted toward busy (app.js's countsTowardBusy): a listing is never
// anybody's work.
import { argv } from './app.js';

export const POLL_MS = 2000;
const isText = (v) => typeof v === 'string' && v.trim() !== '';

/** `devices --json`'s list, rebuilt clean, or null when it is not one. */
export function devicesFrom(answer) {
  if (answer?.ok !== true || !Array.isArray(answer.devices)) return null;
  const list = answer.devices.map((d) => (d && isText(d.id) && isText(d.kind) && typeof d.name === 'string'
    && (typeof d.volume === 'string' || d.volume === null)
    ? { id: d.id, kind: d.kind, name: d.name, volume: d.volume } : null));
  return list.includes(null) ? null : list;
}

export function createWatcher({ run, every = POLL_MS, timers = globalThis }) {
  let timer = null;
  let out = 0;
  let era = 0;
  let list = null;
  const subs = new Set();
  async function ask() {
    if (out > 0) return;
    out += 1;
    const mine = era;
    try {
      const next = devicesFrom(await run(argv.devices({ quick: true })));
      if (mine !== era || next === null) return;
      if (list !== null && JSON.stringify(list) === JSON.stringify(next)) return;
      list = next;
      for (const fn of [...subs]) fn(list);
    } catch {
      // A listing that failed changes nothing; the next tick asks again.
    } finally {
      out -= 1;
    }
  }
  return {
    start() {
      if (timer !== null) return;
      ask();
      timer = timers.setInterval(ask, every);
    },
    stop() {
      timers.clearInterval(timer);
      timer = null;
      era += 1;
      list = null;
    },
    current: () => list,
    subscribe(fn) { subs.add(fn); return () => subs.delete(fn); },
  };
}
