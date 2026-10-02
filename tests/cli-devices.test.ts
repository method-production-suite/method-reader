import { test, expect, afterAll, describe } from 'bun:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CliError, errorMessage } from '../src/cli-errors';
import { resolveCommand, VERBS } from '../src/cli-devices';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-devices-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

// No spawning helper lives here on purpose. Every test in this file drives the
// handlers IN PROCESS with injected seams; the spawned-CLI tests live in
// cli-device-commands.test.ts, whose runCli pins SCREEPUB_VOLUME_ROOTS and
// SCREEPUB_REMARKABLE_ENDPOINT. A second, unsandboxed spawner here was safe
// only as long as every test using it exited before any device work — the next
// one appended would have enumerated this machine's real mounts and fired a
// real request at the reMarkable USB address.

test('CliError carries a contract code and renders the exact JSON error shape', () => {
  const err = new CliError('ambiguous-device', 'several devices are connected: a, b');
  expect(err).toBeInstanceOf(Error);
  expect(err.name).toBe('CliError');
  expect(err.code).toBe('ambiguous-device');
  expect(err.message).toBe('several devices are connected: a, b');
  // The JSON body is exactly two keys — the app decodes {code, message} and
  // nothing else. An implementation that spread the Error (picking up `name`,
  // or nothing at all, since Error fields are non-enumerable) fails here.
  expect(err.toJson()).toEqual({ code: 'ambiguous-device', message: 'several devices are connected: a, b' });
  expect(Object.keys(err.toJson()).sort()).toEqual(['code', 'message']);
});

const noFiles = () => false;
const onlyDevicesFile = (path: string) => path === 'devices';

test('the bare verb dispatches when no file of that name exists', () => {
  expect(resolveCommand(['devices'], noFiles)).toEqual({ kind: 'verb', verb: 'devices', args: [] });
  expect(resolveCommand(['devices', '--json'], noFiles)).toEqual({
    kind: 'verb', verb: 'devices', args: ['--json'],
  });
  expect(resolveCommand(['send', 'Script.epub', '--device', 'x'], noFiles)).toEqual({
    kind: 'verb', verb: 'send', args: ['Script.epub', '--device', 'x'],
  });
});

test('a file literally named `devices` beats the verb', () => {
  // The other direction of the same rule. Without the exists() check this
  // passes as a verb and the file becomes silently unconvertible; without the
  // verb branch at all, the test above fails. Neither half is provable alone.
  expect(resolveCommand(['devices'], onlyDevicesFile)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['devices', '--json'], onlyDevicesFile)).toEqual({ kind: 'convert' });
});

test('an explicit path is never a verb, even with no such file', () => {
  // Catches an implementation that strips ./ or compares basenames: `./devices`
  // is how the user disambiguates when a verb would otherwise win.
  expect(resolveCommand(['./devices'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['devices.pdf'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['/tmp/send'], noFiles)).toEqual({ kind: 'convert' });
});

test('only the first argument can be a verb', () => {
  // Catches an implementation that scans argv for any known verb: a script
  // named devices.pdf sent with --title devices must still convert.
  expect(resolveCommand(['--json', 'devices'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['Script.pdf', '--title', 'send'], noFiles)).toEqual({ kind: 'convert' });
});

test('unknown words and an empty argv are the default path', () => {
  expect(resolveCommand(['frobnicate'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand([], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['--help'], noFiles)).toEqual({ kind: 'convert' });
});

test('VERBS is the single list of known verbs', () => {
  // A pin, so that adding a verb is deliberate. Every name here is a name
  // a user can no longer give a file without writing ./ in front of it.
  // The two update verbs joined on 2026-09-21; both are hyphenated on
  // purpose, since a file called `update-decision` is far less likely to
  // exist than one called `update`. The two KFX verbs joined on 2026-09-23
  // for the same reason. app-settings joined on 2026-09-23 too (parity
  // piece C: where books land, and what new scripts start from), and
  // reveal joined the same day (parity piece C: show a file in the
  // system's file manager).
  // `routes` joined on 2026-09-23 too (parity piece B):
  // the send panel's list of every way a book can leave. `route` joined the
  // same day: it performs one of them (Apple Books, Amazon, Mail, a saved
  // copy) and remembers it. `eject` joined on 2026-10-01 (send from
  // Convert, spec decision 12): it safely removes a reader the engine lists.
  expect([...VERBS]).toEqual([
    'devices',
    'send',
    'settings',
    'export',
    'update-decision',
    'update-should-check',
    'kfx-status',
    'kfx-install',
    'app-settings',
    'reveal',
    'routes',
    'route',
    'eject',
  ]);
});

import { devicesCommand } from '../src/cli-devices';
import type { ConnectedDevice } from '../src/device/types';

const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/run/media/sam/KOBOeReader' };

test('devicesCommand reports each device with the id send accepts', async () => {
  const { devices } = await devicesCommand({ scan: () => [kobo], probe: async () => true });
  expect(devices).toEqual([
    { id: '/run/media/sam/KOBOeReader', kind: 'kobo', name: 'KOBOeReader', volume: '/run/media/sam/KOBOeReader' },
    { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null },
  ]);
});

test('devicesCommand on an empty list is an empty array, not a throw', async () => {
  const { devices } = await devicesCommand({ scan: () => [], probe: async () => false });
  expect(devices).toEqual([]);
});

import { selectDevice, sendCommand } from '../src/cli-devices';

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/run/media/sam/Kindle' };
const remarkable: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };

test('one device connected and no --device picks it', () => {
  expect(selectDevice([kobo])).toBe(kobo);
});

test('an explicit id picks that device even when several are connected', () => {
  // Ordering test: "ambiguous when >1" and "honour --device first" disagree
  // on exactly this input. An implementation that checks the count before the
  // id throws ambiguous-device here.
  expect(selectDevice([kobo, kindle], '/run/media/sam/Kindle')).toBe(kindle);
  expect(selectDevice([kobo, kindle, remarkable], 'remarkable')).toBe(remarkable);
});

test('several connected and no --device is ambiguous-device, listing the ids', () => {
  let thrown: unknown;
  try { selectDevice([kobo, kindle]); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('ambiguous-device');
  // The message must name both, or the user has no way to pick one.
  expect((thrown as CliError).message).toContain('/run/media/sam/KOBOeReader');
  expect((thrown as CliError).message).toContain('/run/media/sam/Kindle');
});

test('nothing connected is no-devices even when --device was given', () => {
  // Ordering test: both no-devices and unknown-device describe this input.
  // The plan's rule is that the empty list wins, because "nothing is plugged
  // in" is the actionable fact. An implementation that checks the requested
  // id first reports unknown-device and fails here.
  let thrown: unknown;
  try { selectDevice([], '/run/media/sam/Kindle'); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('no-devices');
});

test('nothing connected and no --device is also no-devices', () => {
  let thrown: unknown;
  try { selectDevice([]); } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('no-devices');
});

test('an id that matches nothing connected is unknown-device, listing what is', () => {
  let thrown: unknown;
  try { selectDevice([kobo, kindle], '/run/media/sam/Nope'); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unknown-device');
  expect((thrown as CliError).message).toContain('/run/media/sam/Nope');
  expect((thrown as CliError).message).toContain('/run/media/sam/KOBOeReader');
});

test('selection matches on the id, never on a prefix or the name', () => {
  // Catches `deviceId(d).includes(requested)` and `d.name === requested`.
  let thrown: unknown;
  try { selectDevice([kindle], '/run/media/sam'); } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('unknown-device');
  let byName: unknown;
  try { selectDevice([kindle], 'Kindle'); } catch (err) { byName = err; }
  expect((byName as CliError).code).toBe('unknown-device');
});

function book(name = 'Script.epub'): string {
  const path = join(mkdtempSync(join(SCRATCH, 'send-')), name);
  writeFileSync(path, 'book-bytes');
  return path;
}

/** Where a send in this file remembers its route: a fresh folder in
 *  SCRATCH, never the real app settings file. EVERY sendCommand call below
 *  passes one, failures included, so no change to when `send` remembers can
 *  ever reach the machine running the suite. */
function settingsIn(): string {
  return join(mkdtempSync(join(SCRATCH, 'config-')), 'settings.json');
}

function remembered(path: string): unknown {
  return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : undefined;
}

function kindleVolume(): ConnectedDevice {
  const volume = join(mkdtempSync(join(SCRATCH, 'vol-')), 'Kindle');
  mkdirSync(join(volume, 'documents'), { recursive: true });
  return { kind: 'kindle', name: 'Kindle', volume };
}

const uploads: string[] = [];
const upstub = Bun.serve({
  port: 0,
  async fetch(req) {
    const path = new URL(req.url).pathname;
    if (req.method === 'POST') {
      uploads.push(path);
      await req.arrayBuffer();
    }
    return new Response('[]', { status: 200 });
  },
});
const UPLOAD_URL = `http://127.0.0.1:${upstub.port}`;
afterAll(() => upstub.stop(true));

test('sending to a volume device copies it where that vendor indexes', async () => {
  const device = kindleVolume();
  const file = book();
  const result = await sendCommand({
    file,
    scan: () => [device],
    probe: async () => false,
    settingsPath: settingsIn(),
  });
  expect(result.device).toEqual({ id: device.volume!, kind: 'kindle', name: 'Kindle' });
  expect(result.destination).toBe(join(device.volume!, 'documents', 'Script.epub'));
  // The bytes actually moved — a handler that computed the path but skipped
  // the copy passes every other assertion here.
  expect(readFileSync(result.destination!, 'utf8')).toBe('book-bytes');
  expect(result.uploaded).toBeUndefined();
});

test('sending to reMarkable uploads and reports no destination path', async () => {
  uploads.length = 0;
  const result = await sendCommand({
    file: book(),
    scan: () => [],
    probe: async () => true,
    remarkableEndpoint: UPLOAD_URL,
    settingsPath: settingsIn(),
  });
  expect(result.device).toEqual({ id: 'remarkable', kind: 'remarkable', name: 'reMarkable' });
  expect(result.uploaded).toBe(true);
  expect(result.destination).toBeUndefined();
  expect(uploads).toEqual(['/upload']);
});

test('reMarkable rejects an extension it cannot read, before any request', async () => {
  uploads.length = 0;
  let thrown: unknown;
  try {
    await sendCommand({
      file: book('Script.azw3'),
      scan: () => [],
      probe: async () => true,
      remarkableEndpoint: UPLOAD_URL,
      settingsPath: settingsIn(),
    });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unsupported-file');
  // "Before any request" is the point: nothing was posted. A handler that
  // called upload and mapped its message would still have hit the tablet.
  expect(uploads).toEqual([]);
});

test('a failed copy is send-failed, carrying the underlying message', async () => {
  // A volume path whose PARENT is a regular file: mkdirSync fails with
  // ENOTDIR on every platform, so this is deterministic rather than relying
  // on a read-only directory the test runner might happen to own.
  const blocker = join(mkdtempSync(join(SCRATCH, 'block-')), 'not-a-dir');
  writeFileSync(blocker, 'x');
  const device: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: join(blocker, 'Kobo') };
  let thrown: unknown;
  try {
    await sendCommand({
      file: book(), deviceId: device.volume!, scan: () => [device], probe: async () => false, settingsPath: settingsIn(),
    });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('send-failed');
  // The UNDERLYING message, not a substitute: an implementation that caught
  // the copy's error and threw its own prose ("could not send the file") is
  // exactly the mutation this test exists to catch, and any non-empty string
  // passes a length check. mkdirSync's failure is ENOTDIR here.
  expect((thrown as CliError).message).toContain('ENOTDIR');
});

test('a failed upload is send-failed, not a raw RemarkableUploadError', async () => {
  const refusing = Bun.serve({ port: 0, fetch: () => new Response('no', { status: 500 }) });
  let thrown: unknown;
  try {
    await sendCommand({
      file: book(),
      scan: () => [],
      probe: async () => true,
      remarkableEndpoint: `http://127.0.0.1:${refusing.port}`,
      settingsPath: settingsIn(),
    });
  } catch (err) { thrown = err; }
  refusing.stop(true);
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('send-failed');
});

test('a file that is not there is unreadable, and no device is touched', async () => {
  let scanned = 0;
  let thrown: unknown;
  try {
    await sendCommand({
      file: join(SCRATCH, 'no-such-file.epub'),
      scan: () => { scanned += 1; return []; },
      probe: async () => false,
      settingsPath: settingsIn(),
    });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unreadable');
  // The file check comes FIRST: a missing file must not be reported as
  // no-devices, and must not cost the probe's timeout.
  expect(scanned).toBe(0);
});

test('a directory given as the file is unreadable, not send-failed', async () => {
  const dir = mkdtempSync(join(SCRATCH, 'dir-'));
  let thrown: unknown;
  try {
    await sendCommand({ file: dir, scan: () => [kindleVolume()], probe: async () => false, settingsPath: settingsIn() });
  } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('unreadable');
});

test('a non-Error throw still produces a message key the decoder can read', () => {
  // `(err as Error).message` on a non-Error is undefined, and JSON.stringify
  // DROPS an undefined value: the wire object became {"code":"send-failed"}
  // with no message at all, while JsonError.message is required and the Tauri
  // decoder rejects it. Asserted through the real chain — CliError, toJson,
  // stringify, parse — because the drop only happens at stringify.
  const naive = { code: 'send-failed', message: (('boom' as unknown) as Error).message };
  expect('message' in JSON.parse(JSON.stringify(naive))).toBe(false);

  for (const thrown of ['boom', 42, { code: 'ENOTDIR' }, null]) {
    const wire = JSON.parse(JSON.stringify(new CliError('send-failed', errorMessage(thrown)).toJson()));
    expect(typeof wire.message).toBe('string');
    expect(wire.message.length).toBeGreaterThan(0);
  }
  // A real Error still reports its own message, not "Error: ...".
  expect(errorMessage(new Error('ENOTDIR: not a directory'))).toBe('ENOTDIR: not a directory');
});

describe('send remembers the route that worked', () => {
  test('a volume send remembers the device KIND, never its volume, and keeps other keys', async () => {
    const device = kindleVolume();
    const settingsPath = settingsIn();
    writeFileSync(settingsPath, JSON.stringify({ lastRoute: 'save-epub', libraryPath: '/kept' }));
    await sendCommand({ file: book(), scan: () => [device], probe: async () => false, settingsPath });
    // A path would go stale the moment the reader was unplugged; the kind is
    // what the send panel can find again. Piece C's keys share the file.
    expect(remembered(settingsPath)).toEqual({ lastRoute: 'device:kindle', libraryPath: '/kept' });
  });

  test('a Kobo send remembers device:kobo', async () => {
    const volume = join(mkdtempSync(join(SCRATCH, 'vol-')), 'KOBOeReader');
    mkdirSync(volume, { recursive: true });
    const settingsPath = settingsIn();
    await sendCommand({
      file: book(), scan: () => [{ kind: 'kobo', name: 'KOBOeReader', volume }], probe: async () => false, settingsPath,
    });
    expect(remembered(settingsPath)).toEqual({ lastRoute: 'device:kobo' });
  });

  test('a reMarkable upload remembers remarkable', async () => {
    const settingsPath = settingsIn();
    await sendCommand({
      file: book(), scan: () => [], probe: async () => true, remarkableEndpoint: UPLOAD_URL, settingsPath,
    });
    expect(remembered(settingsPath)).toEqual({ lastRoute: 'remarkable' });
  });

  test('a failed copy remembers nothing', async () => {
    const blocker = join(mkdtempSync(join(SCRATCH, 'block-')), 'not-a-dir');
    writeFileSync(blocker, 'x');
    const device: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: join(blocker, 'Kobo') };
    const settingsPath = settingsIn();
    let thrown: unknown;
    try {
      await sendCommand({ file: book(), scan: () => [device], probe: async () => false, settingsPath });
    } catch (err) { thrown = err; }
    expect((thrown as CliError).code).toBe('send-failed');
    expect(existsSync(settingsPath)).toBe(false);
  });

  test('a failed upload remembers nothing', async () => {
    const refusing = Bun.serve({ port: 0, fetch: () => new Response('no', { status: 500 }) });
    const settingsPath = settingsIn();
    let thrown: unknown;
    try {
      await sendCommand({
        file: book(), scan: () => [], probe: async () => true,
        remarkableEndpoint: `http://127.0.0.1:${refusing.port}`, settingsPath,
      });
    } catch (err) { thrown = err; }
    refusing.stop(true);
    expect((thrown as CliError).code).toBe('send-failed');
    expect(existsSync(settingsPath)).toBe(false);
  });

  test('a file the reMarkable cannot read remembers nothing', async () => {
    const settingsPath = settingsIn();
    let thrown: unknown;
    try {
      await sendCommand({
        file: book('Script.azw3'), scan: () => [], probe: async () => true, remarkableEndpoint: UPLOAD_URL, settingsPath,
      });
    } catch (err) { thrown = err; }
    expect((thrown as CliError).code).toBe('unsupported-file');
    expect(existsSync(settingsPath)).toBe(false);
  });

  test('a read-only settings folder does not fail the send: the book is on the reader', async () => {
    const folder = mkdtempSync(join(SCRATCH, 'readonly-'));
    chmodSync(folder, 0o555);
    const device = kindleVolume();
    try {
      const result = await sendCommand({
        file: book(), scan: () => [device], probe: async () => false, settingsPath: join(folder, 'settings.json'),
      });
      expect(readFileSync(result.destination!, 'utf8')).toBe('book-bytes');
      expect(existsSync(join(folder, 'settings.json'))).toBe(false);
    } finally {
      chmodSync(folder, 0o755);
    }
  });

  test('a settings path that cannot exist (its parent is a file) does not fail the send', async () => {
    // ENOTDIR on every platform and for every user, root included, where a
    // read-only folder would not stop root.
    const blocker = join(mkdtempSync(join(SCRATCH, 'block-')), 'a-file');
    writeFileSync(blocker, 'x');
    uploads.length = 0;
    const result = await sendCommand({
      file: book(), scan: () => [], probe: async () => true, remarkableEndpoint: UPLOAD_URL,
      settingsPath: join(blocker, 'config', 'settings.json'),
    });
    expect(result.uploaded).toBe(true);
    expect(uploads).toEqual(['/upload']);
  });
});
