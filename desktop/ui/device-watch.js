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
  let era = 0;
  // The era whose ask is out, or null. Tied to the era, so an ask from
  // before a stop() never blocks the first ask of the next start().
  let outFor = null;
  let list = null;
  const subs = new Set();
  function tell(next) {
    for (const fn of [...subs]) {
      // Each listener on its own: one that throws must not keep the change
      // from the rest.
      try {
        fn(next);
      } catch (err) {
        console.error(err);
      }
    }
  }
  async function ask() {
    const mine = era;
    if (outFor === mine) return;
    outFor = mine;
    let next = null;
    try {
      next = devicesFrom(await run(argv.devices({ quick: true })));
    } catch {
      // A listing that failed changes nothing; the next tick asks again.
    } finally {
      if (outFor === mine) outFor = null;
    }
    if (mine !== era || next === null) return;
    if (list !== null && JSON.stringify(list) === JSON.stringify(next)) return;
    list = next;
    tell(list);
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
