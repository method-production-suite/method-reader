// The `eject` verb: safely remove a reader before it is unplugged (spec
// decision 12, send from Convert). It ejects ONLY a volume the engine
// itself lists as a reader right now, so the window can never eject an
// arbitrary disk. Like the other verb handlers, this RETURNS a result and
// never prints: cli.ts owns stdout.
import { posix, win32 } from 'node:path';
import { CliError, errorMessage } from './cli-errors';
import { listDevices } from './device/list';
import type { ConnectedDevice } from './device/types';
import {
  EJECT_TIMEOUT_MS,
  ejectCommands,
  findDeviceCommand,
  linuxEjectCommands,
  spawnEject,
  STILL_IN_USE,
  WINDOWS_STILL_MOUNTED,
  type EjectRun,
} from './device/eject';

export interface EjectDeps {
  platform?: NodeJS.Platform;
  /** The readers the engine can see; defaults to the mount scan alone (a
   *  reMarkable never mounts, so it can never be ejected). */
  devices?: () => Promise<ConnectedDevice[]>;
  run?: EjectRun;
  timeoutMs?: number;
}

export interface EjectResult {
  ejected: string;
}

/** The first non-empty line of a tool's complaint, stderr before stdout. */
function firstLine(...texts: string[]): string {
  for (const text of texts) {
    const line = text.split(/\r?\n/).find((l) => l.trim() !== '');
    if (line !== undefined) return line.trim();
  }
  return '';
}

/** Eject `volume`. Both refusals run BEFORE anything is spawned. */
export async function ejectCommand(volume: string, deps: EjectDeps = {}): Promise<EjectResult> {
  const platform = deps.platform ?? process.platform;
  const paths = platform === 'win32' ? win32 : posix;
  const run = deps.run ?? spawnEject;
  const timeoutMs = deps.timeoutMs ?? EJECT_TIMEOUT_MS;
  const devices = deps.devices ?? (() => listDevices({ skipRemarkable: true }));

  if (!paths.isAbsolute(volume)) {
    throw new CliError('usage', `eject needs a full path to a reader's volume, not "${volume}"`);
  }

  const wanted = paths.resolve(volume);
  const reader = (await devices()).find((d) => d.volume !== null && paths.resolve(d.volume) === wanted);
  if (reader === undefined) {
    throw new CliError('unknown-device', `not a reader Screepub can see: ${volume}`);
  }

  const failed = (detail: string) =>
    new CliError('eject-failed', `could not eject ${volume}${detail ? `: ${detail}` : ''}`);

  /** One step, raced against the timeout so even a runner that never
   *  answers cannot hold the window's button forever. `anyExit` hands back
   *  stdout whatever the exit code, for a question whose silence is itself
   *  the answer. */
  const step = async (argv: string[], anyExit = false): Promise<string> => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<null>((resolve) => {
      timer = setTimeout(() => resolve(null), timeoutMs);
    });
    let answer: Awaited<ReturnType<EjectRun>> | null;
    try {
      answer = await Promise.race([run(argv, timeoutMs), timeout]);
    } catch (err) {
      throw failed(errorMessage(err));
    } finally {
      clearTimeout(timer);
    }
    if (answer === null || answer.code === null) {
      throw failed('it took too long to answer');
    }
    // The Windows script's own exit: the Eject verb ran, the drive stayed.
    if (platform === 'win32' && answer.code === WINDOWS_STILL_MOUNTED) throw failed(STILL_IN_USE);
    if (answer.code !== 0 && !anyExit) throw failed(firstLine(answer.stderr, answer.stdout));
    return answer.stdout;
  };

  // The command, and the answer, name the volume exactly as the engine
  // listed it, not as the caller spelled it (a trailing slash, say).
  const listed = reader.volume as string;
  let commands = ejectCommands(listed, platform);
  if (commands === 'not-a-drive-root') {
    // Windows ejects whole drives only; a reader listed as a folder on one
    // (or a share) is not something this verb may remove.
    throw new CliError('unknown-device', `not a whole drive Screepub can eject: ${volume}`);
  }
  if (commands === 'needs-device') {
    // findmnt exits 1 with nothing to say for a folder that is not a mount
    // point; that is the same "no device" as an empty answer.
    const device = firstLine(await step(findDeviceCommand(listed), true));
    // Only a block device under /dev is something udisksctl can unmount: a
    // bind mount answers `/dev/sdb1[/books]` and a FUSE mount a name, and
    // either would only reach the person as udisksctl's own error.
    if (!/^\/dev\/[^\s[\]]+$/.test(device)) throw failed('could not find the device behind it');
    const [unmount, powerOff] = linuxEjectCommands(device);
    await step(unmount);
    // Unmounted is safe to unplug. Power-off is a courtesy (it spins the
    // drive down, and some systems refuse it without a password), so its
    // failure does not make the reader any less ejected.
    try {
      await step(powerOff);
    } catch {
      // Already safe.
    }
    return { ejected: listed };
  }
  for (const argv of commands) await step(argv);
  return { ejected: listed };
}
