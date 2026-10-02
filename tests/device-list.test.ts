import { test, expect, afterAll } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { listDevices } from '../src/device/list';
import type { ConnectedDevice } from '../src/device/types';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-device-list-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** A mount parent holding one Kobo, built in a temp dir. No real mount is
 * ever read: the root is injected, exactly as piece A's enumerateVolumes
 * allows. */
function mountRootWithKobo(): string {
  const root = mkdtempSync(join(SCRATCH, 'mounts-'));
  mkdirSync(join(root, 'KOBOeReader', '.kobo'), { recursive: true });
  return root;
}

function stub(status: number) {
  const server = Bun.serve({ port: 0, fetch: () => new Response('[]', { status }) });
  return { url: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true) };
}

const answering = stub(200);
const silent = stub(404);
afterAll(() => {
  answering.stop();
  silent.stop();
});

test('mounted vendors are listed when the tablet does not answer', async () => {
  const devices = await listDevices({
    roots: [mountRootWithKobo()],
    remarkableEndpoint: silent.url,
  });
  expect(devices.map((d) => d.kind)).toEqual(['kobo']);
  expect(devices[0].volume).toMatch(/KOBOeReader$/);
});

test('a reMarkable that answers is appended after the mounted devices', async () => {
  const devices = await listDevices({
    roots: [mountRootWithKobo()],
    remarkableEndpoint: answering.url,
  });
  // Order is asserted with a mounted device PRESENT, so an implementation
  // that prepended the tablet fails rather than passing on an empty list.
  expect(devices.map((d) => d.kind)).toEqual(['kobo', 'remarkable']);
  const rm = devices[1];
  expect(rm.volume).toBeNull();
  expect(rm.name).toBe('reMarkable');
});

test('nothing connected is an empty list, not an error', async () => {
  const empty = mkdtempSync(join(SCRATCH, 'mounts-'));
  expect(await listDevices({ roots: [empty], remarkableEndpoint: silent.url })).toEqual([]);
});

test('the mount scan and the probe run concurrently, not one after the other', async () => {
  // The property is that both are STARTED before either is awaited, so each
  // stub records its own entry time and the two are compared directly. The
  // earlier version timed the whole call and asserted <350ms against ~200ms
  // nominal — same property, but a stopwatch, and the one plausible CI flake
  // on this branch: a scheduling hiccup would have read as a concurrency
  // regression. Sequential (`await scan(); await probe()`) puts the probe's
  // start 200ms after the scan's and fails here just as loudly.
  const slow = <T>(value: T, ms: number) =>
    new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));
  let scanStarted = 0;
  let probeStarted = 0;
  const devices = await listDevices({
    scan: () => {
      scanStarted = Date.now();
      return slow<ConnectedDevice[]>([{ kind: 'kindle', name: 'Kindle', volume: '/v/Kindle' }], 200);
    },
    probe: () => {
      probeStarted = Date.now();
      return slow(true, 200);
    },
  });
  expect(devices.map((d) => d.kind)).toEqual(['kindle', 'remarkable']);
  expect(scanStarted).toBeGreaterThan(0);
  expect(probeStarted).toBeGreaterThan(0);
  expect(Math.abs(probeStarted - scanStarted)).toBeLessThan(20);
});

test('a probe that throws is a tablet that is not there, not a crash', async () => {
  const devices = await listDevices({
    scan: () => [],
    probe: () => Promise.reject(new Error('ECONNREFUSED')),
  });
  expect(devices).toEqual([]);
});

test('skipRemarkable lists what the mount scan sees and never starts the probe', async () => {
  // `routes --quick`, the send panel's first answer: the probe waits out its
  // whole timeout on every machine with no tablet docked, and that wait is
  // what kept the page empty for a second and a half. The probe here never
  // answers at all, so a listing that still waited for it would hang.
  let probes = 0;
  const devices = await listDevices({
    scan: () => [{ kind: 'kindle', name: 'Kindle', volume: '/v/Kindle' }],
    probe: () => {
      probes += 1;
      return new Promise<boolean>(() => {});
    },
    skipRemarkable: true,
  });
  expect(devices.map((d) => d.kind)).toEqual(['kindle']);
  expect(probes).toBe(0);
});
