import { afterAll, describe, test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-devcli-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** A mount parent with one Kobo in it. Injected through SCREEPUB_VOLUME_ROOTS
 * so the spawned CLI never enumerates this machine's real mounts. */
function mountRootWithKobo(): string {
  const root = mkdtempSync(join(SCRATCH, 'mounts-'));
  mkdirSync(join(root, 'KOBOeReader', '.kobo'), { recursive: true });
  return root;
}

const silent = Bun.serve({ port: 0, fetch: () => new Response('no', { status: 404 }) });
const SILENT_URL = `http://127.0.0.1:${silent.port}`;
afterAll(() => silent.stop(true));

/** An empty mount parent, so a spawned CLI that reaches the device code with
 * no explicit roots scans THIS and finds nothing. Both seams are pinned by
 * default — mounts here, the tablet at SILENT_URL — so no test in this file
 * can read a real mount or reach the real reMarkable USB address, whatever it
 * asks the CLI to do. A caller that cares passes its own roots. */
const NO_MOUNTS = mkdtempSync(join(SCRATCH, 'empty-'));

/** Where a spawned `send` that works remembers its route (the app settings
 * file). Pinned by default like the two device seams, so no test here can
 * write the real settings file of the machine running the suite. */
const CONFIG = mkdtempSync(join(SCRATCH, 'config-'));

async function runCli(args: string[], env: Record<string, string> = {}, cwd = ROOT) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
    cwd,
    env: {
      ...process.env,
      SCREEPUB_VOLUME_ROOTS: NO_MOUNTS,
      SCREEPUB_REMARKABLE_ENDPOINT: SILENT_URL,
      SCREEPUB_CONFIG_DIR: CONFIG,
      ...env,
    },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}

/** The --json contract, asserted the same way everywhere: stdout is EXACTLY
 * one parseable object. `JSON.parse` alone would accept a leading log line in
 * some shapes, so the line count is asserted too. */
function soleJson(stdout: string): any {
  const lines = stdout.trim().split('\n');
  expect(lines).toHaveLength(1);
  return JSON.parse(lines[0]);
}

describe('screepub devices', () => {
  test('--json reports the injected device and exits 0', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mountRootWithKobo(),
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.devices).toHaveLength(1);
    expect(result.devices[0].kind).toBe('kobo');
    expect(result.devices[0].name).toBe('KOBOeReader');
    expect(result.devices[0].id).toBe(result.devices[0].volume);
    expect(result.devices[0].volume).toMatch(/KOBOeReader$/);
  });

  test('nothing connected is ok:true with an empty list and exit 0', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    });
    // An empty list is an answer, not an error: an implementation that
    // reported no-devices here (as `send` correctly does) fails on BOTH.
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, devices: [] });
  });

  test('human output is one line per device on stdout', async () => {
    const { stdout, exitCode } = await runCli(['devices'], {
      SCREEPUB_VOLUME_ROOTS: mountRootWithKobo(),
    });
    expect(exitCode).toBe(0);
    expect(stdout.trim().split('\n')).toHaveLength(1);
    expect(stdout).toContain('KOBOeReader');
    expect(stdout).toContain('kobo');
  });

  test('human output says so when nothing is connected', async () => {
    const { stdout, exitCode } = await runCli(['devices'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    });
    expect(exitCode).toBe(0);
    expect(stdout.trim()).toBe('no devices connected');
  });

  test('an unknown flag on devices --json is still one JSON object', async () => {
    // The pre-scan case: parseArgs throws before it could report the mode.
    const { stdout, exitCode } = await runCli(['devices', '--json', '--no-such-flag']);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('usage');
  });

  test('devices takes no positional argument', async () => {
    const { stdout, exitCode } = await runCli(['devices', 'extra', '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
  });

  test('a real reMarkable stub appears in the listing', async () => {
    const answering = Bun.serve({ port: 0, fetch: () => new Response('[]', { status: 200 }) });
    const { stdout } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${answering.port}`,
    });
    answering.stop(true);
    expect(soleJson(stdout).devices).toEqual([
      { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null },
    ]);
  });

  test('--quick lists mounted readers without asking for a reMarkable', async () => {
    let asked = 0;
    const answering = Bun.serve({ port: 0, fetch: () => { asked += 1; return new Response('[]'); } });
    const { root } = kindleRoot();
    const { stdout, exitCode } = await runCli(['devices', '--quick', '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${answering.port}`,
    });
    answering.stop(true);
    expect(exitCode).toBe(0);
    expect(soleJson(stdout).devices.map((d: { kind: string }) => d.kind)).toEqual(['kindle']);
    expect(asked).toBe(0);
  });
});

function kindleRoot(): { root: string; volume: string } {
  const root = mkdtempSync(join(SCRATCH, 'mounts-'));
  const volume = join(root, 'Kindle');
  mkdirSync(join(volume, 'documents'), { recursive: true });
  return { root, volume };
}

function book(name = 'Script.epub'): string {
  const path = join(mkdtempSync(join(SCRATCH, 'book-')), name);
  writeFileSync(path, 'book-bytes');
  return path;
}

describe('screepub send', () => {
  test('--json reports the device and the destination, and the bytes moved', async () => {
    const { root, volume } = kindleRoot();
    const file = book();
    const config = mkdtempSync(join(SCRATCH, 'config-'));
    const { stdout, exitCode } = await runCli(['send', file, '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
      SCREEPUB_CONFIG_DIR: config,
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.device).toEqual({ id: volume, kind: 'kindle', name: 'Kindle' });
    expect(result.destination).toBe(join(volume, 'documents', 'Script.epub'));
    expect(readFileSync(result.destination, 'utf8')).toBe('book-bytes');
    // It worked, so it is remembered: by kind, in the settings file under
    // SCREEPUB_CONFIG_DIR, as the Send page's choice next time.
    expect(JSON.parse(readFileSync(join(config, 'settings.json'), 'utf8'))).toEqual({
      lastRoute: 'device:kindle',
    });
  });

  test('a send that fails remembers nothing', async () => {
    const config = mkdtempSync(join(SCRATCH, 'config-'));
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
      SCREEPUB_CONFIG_DIR: config,
    });
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('no-devices');
    expect(existsSync(join(config, 'settings.json'))).toBe(false);
  });

  test('a reMarkable send reports uploaded:true and NO destination key', async () => {
    const posted: string[] = [];
    const tablet = Bun.serve({
      port: 0,
      async fetch(req) {
        if (req.method === 'POST') { posted.push(new URL(req.url).pathname); await req.arrayBuffer(); }
        return new Response('[]', { status: 200 });
      },
    });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${tablet.port}`,
    });
    tablet.stop(true);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.uploaded).toBe(true);
    // The key must be ABSENT, not null: the spec says the field is omitted.
    // `expect(result.destination).toBeUndefined()` would also pass on null.
    expect('destination' in result).toBe(false);
    expect(posted).toEqual(['/upload']);
  });

  test('--device picks one of several, and the others are untouched', async () => {
    const root = mkdtempSync(join(SCRATCH, 'mounts-'));
    const kindle = join(root, 'Kindle');
    const kobo = join(root, 'KOBOeReader');
    mkdirSync(join(kindle, 'documents'), { recursive: true });
    mkdirSync(join(kobo, '.kobo'), { recursive: true });
    const { stdout, exitCode } = await runCli(['send', book(), '--device', kobo, '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.device.kind).toBe('kobo');
    expect(result.destination).toBe(join(kobo, 'Script.epub'));
    // Two devices were connected: an implementation that ignored --device and
    // took the first would have written into the Kindle instead.
    expect(readFileSync(join(kobo, 'Script.epub'), 'utf8')).toBe('book-bytes');
  });

  test('several connected and no --device is ambiguous-device, naming both', async () => {
    const root = mkdtempSync(join(SCRATCH, 'mounts-'));
    const kindle = join(root, 'Kindle');
    const kobo = join(root, 'KOBOeReader');
    mkdirSync(join(kindle, 'documents'), { recursive: true });
    mkdirSync(join(kobo, '.kobo'), { recursive: true });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.error.code).toBe('ambiguous-device');
    expect(result.error.message).toContain(kindle);
    expect(result.error.message).toContain(kobo);
  });

  test('nothing connected is no-devices', async () => {
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    });
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('no-devices');
  });

  test('an id nothing matches is unknown-device', async () => {
    const { root } = kindleRoot();
    const { stdout, exitCode } = await runCli(['send', book(), '--device', '/nope', '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unknown-device');
  });

  test('an extension reMarkable cannot read is unsupported-file', async () => {
    const tablet = Bun.serve({ port: 0, fetch: () => new Response('[]', { status: 200 }) });
    const { stdout, exitCode } = await runCli(['send', book('Script.azw3'), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${tablet.port}`,
    });
    tablet.stop(true);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unsupported-file');
  });

  test('a tablet that refuses the upload is send-failed', async () => {
    const refusing = Bun.serve({
      port: 0,
      fetch: (req) => new Response('[]', { status: req.method === 'POST' ? 500 : 200 }),
    });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${refusing.port}`,
    });
    refusing.stop(true);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('send-failed');
  });

  test('no file argument is a usage error', async () => {
    const { stdout, exitCode } = await runCli(['send', '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
  });

  test('a file that is not there is unreadable', async () => {
    const { stdout, exitCode } = await runCli(['send', join(SCRATCH, 'ghost.epub'), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    });
    expect(exitCode).toBe(1);
    // Not no-devices: the file is checked before the device list is built.
    expect(soleJson(stdout).error.code).toBe('unreadable');
  });

  test('human output names the device and the destination', async () => {
    const { root, volume } = kindleRoot();
    const { stdout, exitCode } = await runCli(['send', book()], { SCREEPUB_VOLUME_ROOTS: root });
    expect(exitCode).toBe(0);
    expect(stdout).toContain('Kindle');
    expect(stdout).toContain(join(volume, 'documents', 'Script.epub'));
  });
});

describe('verb dispatch does not capture files', () => {
  test('a file literally named `devices` converts instead of listing', async () => {
    // Direction one of the shadowing rule, end to end, with a real file in a
    // real cwd. `devices` has no extension, so the conversion path rejects it
    // as unsupported-type — which is exactly the proof that the FILE won: the
    // verb would have printed {"ok":true,"devices":[...]} and exited 0.
    const dir = mkdtempSync(join(SCRATCH, 'shadow-'));
    writeFileSync(join(dir, 'devices'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    }, dir);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('unsupported-type');
  });

  test('the bare word lists devices in the same cwd with no such file', async () => {
    // Direction two. Same command, same cwd shape, only the file removed —
    // so the two tests differ in exactly the thing the rule is about.
    const dir = mkdtempSync(join(SCRATCH, 'shadow-'));
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(SCRATCH, 'empty-')),
    }, dir);
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, devices: [] });
  });

  test('./devices always means the file', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'shadow-'));
    writeFileSync(join(dir, 'devices'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['./devices', '--json'], {}, dir);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unsupported-type');
  });

  test('a file named `send` converts rather than being a command', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'shadow-'));
    writeFileSync(join(dir, 'send'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['send', '--json'], {}, dir);
    expect(exitCode).toBe(1);
    // Without the shadowing rule this is a usage error from `send` with no
    // file argument; with it, the file is the input and its type is wrong.
    expect(soleJson(stdout).error.code).toBe('unsupported-type');
  });
});

describe('cli --json --help and --version', () => {
  // Moved here from cli-devices.test.ts, which had a second runCli with the
  // INHERITED environment: safe only while every test using it exited before
  // any device work. This file's runCli pins both device seams, so a test
  // appended below cannot reach a real mount or the real tablet address.
  test('--json --help emits JSON with ok: true and non-empty usage', async () => {
    const { stdout, exitCode } = await runCli(['--json', '--help']);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(typeof result.usage).toBe('string');
    expect(result.usage.length).toBeGreaterThan(0);
  });

  test('--json --version emits JSON with ok: true and matching version', async () => {
    const pkg = await Bun.file(`${ROOT}package.json`).json();
    const { stdout, exitCode } = await runCli(['--json', '--version']);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.version).toBe(pkg.version);
  });

  test('plain --help without --json prints the human text, not JSON', async () => {
    const { stdout, exitCode } = await runCli(['--help']);
    expect(exitCode).toBe(0);
    expect(stdout.trim().startsWith('{')).toBe(false);
    expect(stdout).toContain('Usage:');
  });

  test('devices --json --help is the devices usage, not the conversion usage', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json', '--help']);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    // The point of the fix: a verb's help lists the verb's OWN flags. Any
    // string satisfies `usage.length > 0`, including the conversion usage that
    // advertised -o, --mobi and --options — none of which parseVerbArgs
    // accepts. --device is send's, so devices must not offer that either.
    expect(result.usage).toContain('screepub devices');
    expect(result.usage).not.toContain('--mobi');
    expect(result.usage).not.toContain('--device');
  });

  test('send --json --help is the send usage, naming --device and not --mobi', async () => {
    const { stdout, exitCode } = await runCli(['send', '--json', '--help']);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.usage).toContain('--device');
    expect(result.usage).not.toContain('--mobi');
    expect(result.usage).not.toContain('--options');
  });

  test('plain send --help prints the send usage as human text', async () => {
    const { stdout, exitCode } = await runCli(['send', '--help']);
    expect(exitCode).toBe(0);
    expect(stdout.trim().startsWith('{')).toBe(false);
    expect(stdout).toContain('--device');
    expect(stdout).not.toContain('--mobi');
  });
});

describe('flag before verb', () => {
  test('--json devices says the verb must come first', async () => {
    // resolveCommand reads argv[0] only, deliberately, so this reaches the
    // CONVERSION path and fails as unsupported-type — correct, but the bare
    // 'unsupported input type ""' named neither the cause nor the cure.
    const { stdout, exitCode } = await runCli(['--json', 'devices']);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.error.code).toBe('unsupported-type');
    expect(result.error.message).toContain('screepub devices');
    expect(result.error.message).toContain('verb must come first');
    expect(result.error.message).toBe(
      'unsupported input type "": expected .pdf, .fountain, or .txt. '
        + 'Did you mean `screepub devices`? The verb must come first',
    );
  });

  test('--json send gets the hint naming send, not devices', async () => {
    const { stdout } = await runCli(['--json', 'send']);
    expect(soleJson(stdout).error.message).toContain('screepub send');
  });

  test('an extension-less file that is not a verb gets no hint', async () => {
    // The hint is for verbs only: a real script saved as `Makefile` must not
    // be told to run a command that does not exist.
    const { stdout, exitCode } = await runCli(['--json', 'Makefile']);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.error.code).toBe('unsupported-type');
    expect(result.error.message).not.toContain('did you mean');
  });

  test('a real file named devices gets no hint either', async () => {
    // Then the user meant the file — that is the shadowing rule — and telling
    // them to run the verb instead would be wrong.
    const dir = mkdtempSync(join(SCRATCH, 'shadow-'));
    writeFileSync(join(dir, 'devices'), 'not a pdf');
    const { stdout } = await runCli(['--json', 'devices'], {}, dir);
    expect(soleJson(stdout).error.message).not.toContain('did you mean');
  });
});

describe('devices rejects flags it cannot act on', () => {
  // The refusals themselves (--device included, with its own aside) are
  // checked for every verb and every flag in tests/cli-verb-flags.test.ts.
  test('devices without --device still lists, so the guard is not blanket', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json']);
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, devices: [] });
  });
});
