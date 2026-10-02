// Boots the whole window the way index.html does: imports desktop/ui/main.js
// against the fake DOM and a stand-in Tauri, so a test can press keys, drop
// files and click tabs on the real frame with the real surfaces mounted in it.
//
// Each boot imports its own COPY of desktop/ui's scripts, from a folder
// under the caller's scratch folder. main.js hangs handlers on app.js
// (onDialogClosed) and on the update flow that nothing can take off again;
// on the shared modules they would outlive the page and fire into every
// later test file's dialogs. On a copy, nothing else ever reaches them.
//
// The window's one interval (device-watch.js's devices poll, from the moment
// a book is open) never runs on a real clock here: setInterval and
// clearInterval are stand-ins for the whole boot, so no poll outlives the
// test, and tickInterval() fires the next one by hand. With `timers: true`
// a timeout of a second or more (the heads-up's eight seconds) is held too,
// for runTimeouts() to fire; shorter ones (a settle, a debounce) run as
// usual, because the fake page's own settle() is one of them.
import { copyFileSync, mkdtempSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fakePage, settle, withGlobals, type FakeNode, type FakePage } from './fake-dom';

const UI = join(import.meta.dir, '..', '..', 'desktop', 'ui');
/** A timeout this long or longer is held when `timers` is on. */
const LONG_MS = 1000;

export interface BootedWindow extends FakePage {
  /** The pane a surface is mounted in. */
  surface(id: string): FakeNode;
  tab(id: string): FakeNode;
  /** The id of the surface whose pane is showing. */
  showing(): string | null;
  /** A key pressed with nothing in particular focused: the window hears it. */
  press(key: string, mods?: { metaKey?: boolean; ctrlKey?: boolean }): ReturnType<FakePage['fireWindow']>;
  /** The folder this boot's copy of the scripts was imported from. */
  ui: string;
  /** Fires every interval still set, once. */
  tickInterval(): void;
  /** How many intervals are set now. */
  intervals(): number;
  /** Fires every long timeout still held (`timers: true` only). */
  runTimeouts(): void;
}

/** `scratch` is the calling test file's own scratch folder; the copy is
 *  made inside it, so it goes when that folder does. */
export async function bootWindow(
  scratch: string,
  options: Parameters<typeof fakePage>[0] & { timers?: boolean } = {},
): Promise<BootedWindow> {
  const ui = mkdtempSync(join(scratch, 'ui-'));
  for (const name of readdirSync(UI)) {
    if (name.endsWith('.js')) copyFileSync(join(UI, name), join(ui, name));
  }
  const clock = withGlobals();
  const intervals = new Map<number, () => void>();
  const held = new Map<number, () => void>();
  let next = 1;
  clock.set('setInterval', (fn: () => void) => { const id = next++; intervals.set(id, fn); return id; });
  clock.set('clearInterval', (id: number) => { intervals.delete(id); });
  if (options.timers) {
    const realSet = globalThis.setTimeout;
    const realClear = globalThis.clearTimeout;
    clock.set('setTimeout', (fn: () => void, ms = 0, ...rest: unknown[]) => {
      if (ms < LONG_MS) return realSet(fn, ms, ...rest);
      const id = next++;
      held.set(id, fn);
      return id;
    });
    clock.set('clearTimeout', (id: unknown) => {
      if (typeof id === 'number' && held.has(id)) held.delete(id);
      else realClear(id as Parameters<typeof clearTimeout>[0]);
    });
  }
  const page = fakePage(options);
  const root = page.doc.createElement('div');
  root.id = 'app';
  page.doc.body.append(root);
  await import(join(ui, 'main.js'));
  await settle();
  return {
    ...page,
    ui,
    surface: (id) => page.doc.getElementById(`surface-${id}`)!,
    tab: (id) => page.doc.getElementById(`tab-${id}`)!,
    showing() {
      const on = page.doc.querySelectorAll('section.surface').find((p) => !p.hidden);
      return on === undefined ? null : on.id.replace(/^surface-/, '');
    },
    press(key, mods = {}) {
      return page.fireWindow('keydown', { key, metaKey: false, ctrlKey: false, ...mods });
    },
    tickInterval() { for (const fn of [...intervals.values()]) fn(); },
    intervals: () => intervals.size,
    runTimeouts() {
      const due = [...held.values()];
      held.clear();
      for (const fn of due) fn();
    },
    async close() {
      await page.close();
      intervals.clear();
      held.clear();
      clock.restore();
    },
  };
}
