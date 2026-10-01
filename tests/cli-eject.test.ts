// The `eject` verb: safely remove a reader the engine itself can see
// (spec decision 12). See src/device/eject.ts and src/cli-eject.ts.
//
// NO TEST HERE MAY EJECT A REAL DRIVE. Every handler test passes a recording
// fake runner, and every device list is injected, so no diskutil, udisksctl
// or PowerShell is ever started. The one real-spawn test runs a throwaway
// bun subprocess, never an eject tool, and the spawned CLI tests at the
// bottom cover only refusals and --help, against a volume root that holds
// no reader.
import { afterAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ejectCommand } from '../src/cli-eject';
import { ejectCommands, spawnEject, type EjectRun } from '../src/device/eject';
import type { ConnectedDevice } from '../src/device/types';

const ROOT = new URL('..', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-eject-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const linuxKobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/run/media/sam/KOBOeReader' };
const winKindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: 'E:\\' };

function codeOf(err: unknown): string {
  return (err as { code?: string }).code ?? '';
}

async function caught(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (err) {
    return err;
  }
  throw new Error('expected a throw');
}

type Answer = { code: number | null; stdout: string; stderr: string };

/** Records every argv and never spawns anything. `answer` decides what each
 *  call reports, as if a real child had exited. */
function recordingRun(answer: (argv: string[]) => Answer | Promise<Answer> = () => ({ code: 0, stdout: '', stderr: '' })) {
  const calls: string[][] = [];
  const run: EjectRun = async (argv) => {
    calls.push(argv);
    return answer(argv);
  };
  return { run, calls };
}

describe('ejectCommand: refusals come before anything runs', () => {
  test('a relative path is usage, and nothing runs', async () => {
    const { run, calls } = recordingRun();
    const err = await caught(ejectCommand('Volumes/Kindle', { platform: 'darwin', devices: async () => [kindle], run }));
    expect(codeOf(err)).toBe('usage');
    expect(calls).toEqual([]);
  });

  test('a volume that is not a reader the engine lists is unknown-device, and nothing runs', async () => {
    const { run, calls } = recordingRun();
    const err = await caught(
      ejectCommand('/Volumes/Backup', { platform: 'darwin', devices: async () => [kindle], run }),
    );
    expect(codeOf(err)).toBe('unknown-device');
    expect((err as Error).message).toBe('not a reader Screepub can see: /Volumes/Backup');
    expect(calls).toEqual([]);
  });

  test('the comparison is on resolved paths: a trailing slash still names the reader', async () => {
    const { run, calls } = recordingRun();
    const result = await ejectCommand('/Volumes/Kindle/', { platform: 'darwin', devices: async () => [kindle], run });
    expect(calls).toEqual([['diskutil', 'eject', '/Volumes/Kindle']]);
    // The answer names the volume as the engine listed it, too.
    expect(result).toEqual({ ejected: '/Volumes/Kindle' });
  });

  test('a reMarkable (no volume) never matches anything', async () => {
    const { run, calls } = recordingRun();
    const tablet: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };
    const err = await caught(ejectCommand('/', { platform: 'darwin', devices: async () => [tablet], run }));
    expect(codeOf(err)).toBe('unknown-device');
    expect(calls).toEqual([]);
  });
});

describe('ejectCommand: the command per platform', () => {
  test('darwin runs exactly diskutil eject <volume> and answers { ejected }', async () => {
    const { run, calls } = recordingRun();
    const result = await ejectCommand('/Volumes/Kindle', { platform: 'darwin', devices: async () => [kindle], run });
    expect(calls).toEqual([['diskutil', 'eject', '/Volumes/Kindle']]);
    expect(result).toEqual({ ejected: '/Volumes/Kindle' });
  });

  test('linux asks findmnt for the device, then unmounts it, then powers it off, in that order', async () => {
    const { run, calls } = recordingRun((argv) =>
      argv[0] === 'findmnt' ? { code: 0, stdout: '/dev/sdb1\n', stderr: '' } : { code: 0, stdout: '', stderr: '' },
    );
    const result = await ejectCommand(linuxKobo.volume!, { platform: 'linux', devices: async () => [linuxKobo], run });
    expect(calls).toEqual([
      ['findmnt', '-n', '-o', 'SOURCE', '--mountpoint', '/run/media/sam/KOBOeReader'],
      ['udisksctl', 'unmount', '-b', '/dev/sdb1'],
      ['udisksctl', 'power-off', '-b', '/dev/sdb1'],
    ]);
    expect(result).toEqual({ ejected: '/run/media/sam/KOBOeReader' });
  });

  test('linux: a failed unmount stops there, and power-off never runs', async () => {
    const { run, calls } = recordingRun((argv) =>
      argv[0] === 'findmnt'
        ? { code: 0, stdout: '/dev/sdb1\n', stderr: '' }
        : { code: 1, stdout: '', stderr: 'Error unmounting /dev/sdb1: target is busy\n' },
    );
    const err = await caught(ejectCommand(linuxKobo.volume!, { platform: 'linux', devices: async () => [linuxKobo], run }));
    expect(codeOf(err)).toBe('eject-failed');
    expect((err as Error).message).toContain('target is busy');
    expect(calls.map((argv) => argv[1])).toEqual(['-n', 'unmount']);
  });

  test('linux: findmnt naming no device is eject-failed, and udisksctl never runs', async () => {
    const { run, calls } = recordingRun(() => ({ code: 0, stdout: '\n', stderr: '' }));
    const err = await caught(ejectCommand(linuxKobo.volume!, { platform: 'linux', devices: async () => [linuxKobo], run }));
    expect(codeOf(err)).toBe('eject-failed');
    expect(calls.length).toBe(1);
  });

  test('linux: a reader that is a plain folder, not a mount point, never reaches the disk it sits on', async () => {
    // A tolino is recognised by its folder name alone, so a listed volume can
    // be a folder on an ordinary disk. findmnt --mountpoint answers nothing
    // (and exits 1) for such a folder; --target would have walked up to the
    // containing filesystem and named the parent disk.
    const tolino: ConnectedDevice = { kind: 'tolino', name: 'tolino', volume: '/home/sam/tolino' };
    const { run, calls } = recordingRun((argv) =>
      argv.includes('--target') ? { code: 0, stdout: '/dev/sda2\n', stderr: '' } : { code: 1, stdout: '', stderr: '' },
    );
    const err = await caught(ejectCommand(tolino.volume!, { platform: 'linux', devices: async () => [tolino], run }));
    expect(codeOf(err)).toBe('eject-failed');
    expect((err as Error).message).toBe('could not eject /home/sam/tolino: could not find the device behind it');
    expect(calls).toEqual([['findmnt', '-n', '-o', 'SOURCE', '--mountpoint', '/home/sam/tolino']]);
  });

  test('win32: the PowerShell line names the drive E: for the volume E:\\', async () => {
    const { run, calls } = recordingRun();
    const result = await ejectCommand('E:\\', { platform: 'win32', devices: async () => [winKindle], run });
    expect(calls.length).toBe(1);
    const argv = calls[0];
    expect(argv[0]).toBe('powershell');
    expect(argv[argv.length - 1]).toBe(
      "(New-Object -comObject Shell.Application).Namespace(17).ParseName('E:').InvokeVerb('Eject')",
    );
    expect(result).toEqual({ ejected: 'E:\\' });
  });

  test('win32: a listed volume that is not a drive root is refused, and PowerShell never starts', async () => {
    // A UNC share, a path with a quote in it, and a subfolder (reachable via
    // SCREEPUB_VOLUME_ROOTS) that would otherwise map to the whole D: drive.
    for (const volume of ['\\\\server\\share\\Kindle', "E:\\it's\\Kindle", 'D:\\stuff\\Kindle']) {
      const reader: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume };
      const { run, calls } = recordingRun();
      const err = await caught(ejectCommand(volume, { platform: 'win32', devices: async () => [reader], run }));
      expect(codeOf(err)).toBe('unknown-device');
      expect(calls).toEqual([]);
    }
  });

  test('ejectCommands on win32: a drive root, with or without its backslash, and nothing else', () => {
    const line = (drive: string) =>
      `(New-Object -comObject Shell.Application).Namespace(17).ParseName('${drive}').InvokeVerb('Eject')`;
    for (const root of ['E:\\', 'E:', 'e:\\']) {
      const commands = ejectCommands(root, 'win32');
      expect(Array.isArray(commands) && commands[0][commands[0].length - 1]).toBe(line('E:'));
    }
    for (const other of ['D:\\stuff\\Kindle', '\\\\server\\share', "E:\\'", 'E:\\\\', 'Kindle']) {
      expect(ejectCommands(other, 'win32')).toBe('not-a-drive-root');
    }
  });

  test('ejectCommands is pure: linux needs the device first', () => {
    expect(ejectCommands('/Volumes/Kindle', 'darwin')).toEqual([['diskutil', 'eject', '/Volumes/Kindle']]);
    expect(ejectCommands('/run/media/sam/KOBOeReader', 'linux')).toBe('needs-device');
  });
});

describe('ejectCommand: a volume that will not eject says so', () => {
  test('a non-zero exit is eject-failed carrying the first stderr line', async () => {
    const { run } = recordingRun(() => ({
      code: 1,
      stdout: '',
      stderr: 'Volume Kindle on disk4 failed to eject\nsecond line\n',
    }));
    const err = await caught(ejectCommand('/Volumes/Kindle', { platform: 'darwin', devices: async () => [kindle], run }));
    expect(codeOf(err)).toBe('eject-failed');
    expect((err as Error).message).toContain('Volume Kindle on disk4 failed to eject');
    expect((err as Error).message).not.toContain('second line');
  });

  test('a runner that never answers is eject-failed after the timeout', async () => {
    const { run } = recordingRun(() => new Promise<Answer>(() => {}));
    const err = await caught(
      ejectCommand('/Volumes/Kindle', { platform: 'darwin', devices: async () => [kindle], run, timeoutMs: 20 }),
    );
    expect(codeOf(err)).toBe('eject-failed');
  });

  test('a tool that cannot start at all is eject-failed, not a crash', async () => {
    const run: EjectRun = async () => {
      throw new Error('spawn diskutil ENOENT');
    };
    const err = await caught(ejectCommand('/Volumes/Kindle', { platform: 'darwin', devices: async () => [kindle], run }));
    expect(codeOf(err)).toBe('eject-failed');
    expect((err as Error).message).toContain('ENOENT');
  });
});

describe('spawnEject: the real one, exercised without ejecting anything', () => {
  test('collects exit code, stdout and stderr from a throwaway bun child', async () => {
    const result = await spawnEject(
      [process.execPath, '-e', 'console.log("out"); console.error("err"); process.exit(3)'],
      10_000,
    );
    expect(result).toEqual({ code: 3, stdout: 'out\n', stderr: 'err\n' });
  });

  test('a child that outlives the timeout is killed and answers code null', async () => {
    const result = await spawnEject([process.execPath, '-e', 'setTimeout(() => {}, 60000)'], 100);
    expect(result.code).toBeNull();
  });
});

describe('screepub eject, spawned: refusals and help only', () => {
  async function runCli(args: string[]) {
    const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
      stdout: 'pipe',
      stderr: 'pipe',
      // An empty volume root: the engine lists no reader, so nothing can
      // ever reach a real eject tool from here.
      env: { ...process.env, SCREEPUB_VOLUME_ROOTS: SCRATCH },
    });
    const [stdout, exitCode] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    return { stdout, exitCode };
  }

  test('a volume the engine does not list is unknown-device on stdout as JSON', async () => {
    const { stdout, exitCode } = await runCli(['eject', '/Volumes/Backup', '--json']);
    expect(exitCode).not.toBe(0);
    expect(JSON.parse(stdout)).toEqual({
      ok: false,
      error: { code: 'unknown-device', message: 'not a reader Screepub can see: /Volumes/Backup' },
    });
  });

  test('no volume, or two, is usage', async () => {
    for (const args of [['eject', '--json'], ['eject', '/a', '/b', '--json']]) {
      const { stdout } = await runCli(args);
      expect(JSON.parse(stdout).error.code).toBe('usage');
    }
  });

  test('--help describes the verb', async () => {
    const { stdout, exitCode } = await runCli(['eject', '--help']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('screepub eject <volume>');
  });
});
