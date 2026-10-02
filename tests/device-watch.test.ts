import { beforeAll, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
const UI = join(import.meta.dir, '..', 'desktop', 'ui');
let w: any;
beforeAll(async () => { w = await import(join(UI, 'device-watch.js')); });

function rig() {
  const calls: { args: string[]; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  let tick: (() => void) | null = null;
  const watcher = w.createWatcher({
    run: (args: string[]) => new Promise((resolve, reject) => calls.push({ args, resolve, reject })),
    timers: { setInterval: (fn: () => void) => { tick = fn; return 1; }, clearInterval: () => { tick = null; } },
  });
  const flush = () => new Promise((r) => setTimeout(r, 0));
  return { watcher, calls, tick: () => tick?.(), ticking: () => tick !== null, flush };
}
const kindle = { id: '/V/Kindle', kind: 'kindle', name: 'Kindle', volume: '/V/Kindle' };

describe('the devices poll', () => {
  test('start asks at once with --quick, then every tick; stop ends it and forgets the list', async () => {
    const { watcher, calls, tick, ticking, flush } = rig();
    watcher.start();
    expect(calls.map((c) => c.args)).toEqual([['devices', '--json', '--quick']]);
    calls[0].resolve({ ok: true, devices: [kindle] });
    await flush();
    expect(watcher.current()).toEqual([kindle]);
    tick();
    expect(calls.length).toBe(2);
    watcher.stop();
    expect(ticking()).toBe(false);
    expect(watcher.current()).toBe(null);
  });
  test('a tick while an ask is out asks nothing', () => {
    const { watcher, calls, tick } = rig();
    watcher.start();
    tick();
    tick();
    expect(calls.length).toBe(1);
  });
  test('subscribers hear a change, and only a change', async () => {
    const { watcher, calls, tick, flush } = rig();
    const heard: unknown[] = [];
    watcher.subscribe((list: unknown) => heard.push(list));
    watcher.start();
    calls[0].resolve({ ok: true, devices: [] });
    await flush();
    tick(); calls[1].resolve({ ok: true, devices: [] }); await flush();
    tick(); calls[2].resolve({ ok: true, devices: [kindle] }); await flush();
    expect(heard).toEqual([[], [kindle]]);
  });
  test('a refusal, a throw or a malformed answer keeps the last list', async () => {
    const { watcher, calls, tick, flush } = rig();
    watcher.start();
    calls[0].resolve({ ok: true, devices: [kindle] }); await flush();
    tick(); calls[1].reject(new Error('the engine is gone')); await flush();
    tick(); calls[2].resolve({ ok: true, devices: [{ kind: 'kindle' }] }); await flush();
    tick(); calls[3].resolve({ ok: false, error: { code: 'internal', message: 'x' } }); await flush();
    expect(watcher.current()).toEqual([kindle]);
  });
  test('an answer that lands after stop is dropped; start twice is one interval', async () => {
    const { watcher, calls, flush } = rig();
    watcher.start(); watcher.start();
    expect(calls.length).toBe(1);
    watcher.stop();
    calls[0].resolve({ ok: true, devices: [kindle] }); await flush();
    expect(watcher.current()).toBe(null);
  });
  test('a subscriber that throws does not keep the change from the ones after it', async () => {
    const { watcher, calls, flush } = rig();
    const heard: unknown[] = [];
    const errors: unknown[] = [];
    const was = console.error;
    console.error = (...a: unknown[]) => { errors.push(a); };
    try {
      watcher.subscribe(() => { throw new Error('a broken listener'); });
      watcher.subscribe((list: unknown) => heard.push(list));
      watcher.start();
      calls[0].resolve({ ok: true, devices: [kindle] });
      await flush();
    } finally {
      console.error = was;
    }
    expect(heard).toEqual([[kindle]]);
    expect(errors.length).toBe(1);
    expect(watcher.current()).toEqual([kindle]);
  });
  test('an ask from before stop never blocks the first ask of a new start', async () => {
    const { watcher, calls, flush } = rig();
    watcher.start();
    watcher.stop();
    watcher.start();
    expect(calls.length).toBe(2);
    calls[1].resolve({ ok: true, devices: [kindle] });
    await flush();
    expect(watcher.current()).toEqual([kindle]);
    // The old ask landing late changes nothing, and does not free the new
    // start's guard early: a tick while the new ask is out asks nothing.
    calls[0].resolve({ ok: true, devices: [] });
    await flush();
    expect(watcher.current()).toEqual([kindle]);
  });
  test('the old ask settling does not let a new start ask twice at once', async () => {
    const { watcher, calls, tick, flush } = rig();
    watcher.start();
    watcher.stop();
    watcher.start();
    calls[0].resolve({ ok: true, devices: [] });
    await flush();
    tick();
    expect(calls.length).toBe(2);
  });
});
