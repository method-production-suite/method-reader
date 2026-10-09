// The `eject` verb's performer: the per-OS commands that safely remove a
// reader, and the ONE place that spawns them. src/cli-eject.ts is the
// handler: it checks the volume is a reader the engine lists BEFORE any of
// this runs, and every test hands it a fake EjectRun, so no test ever
// ejects a real drive.

/** One child process, run to completion or killed at `timeoutMs`. A killed
 *  child answers `code: null`. */
export interface EjectRun {
  (argv: string[], timeoutMs: number): Promise<{ code: number | null; stdout: string; stderr: string }>;
}

/** How long one eject step may take before it counts as failed. A volume
 *  still being written to can make diskutil wait; twenty seconds is long
 *  enough for that and short enough that the window's button comes back. */
export const EJECT_TIMEOUT_MS = 20_000;

/** The Windows drive a volume names, like `E:` for `E:\`, but ONLY when the
 *  volume is exactly a drive root (`X:\` or `X:`); null for anything else,
 *  a subfolder, a UNC share, or a path with a quote in it. So only a letter
 *  and a colon ever reach the PowerShell line, nothing in a volume path can
 *  be read as script, and a folder on a drive never ejects the whole drive. */
export function windowsDrive(volume: string): string | null {
  const match = /^([A-Za-z]):\\?$/.exec(volume);
  return match ? `${match[1].toUpperCase()}:` : null;
}

/** The per-OS argv lists for one eject; pure, so tests read them.
 *
 * darwin: `diskutil eject <volume>`.
 * win32: the shell's own Eject verb on the drive, through PowerShell, and
 * only for a drive root ('not-a-drive-root' otherwise; the caller refuses).
 * Everything else (Linux): udisksctl works on the block DEVICE, not the
 * mount point, so the caller first asks findmnt for it ('needs-device')
 * and then runs linuxEjectCommands. */
export function ejectCommands(
  volume: string,
  platform: NodeJS.Platform,
): string[][] | 'needs-device' | 'not-a-drive-root' {
  if (platform === 'darwin') return [['diskutil', 'eject', volume]];
  if (platform === 'win32') {
    const drive = windowsDrive(volume);
    if (drive === null) return 'not-a-drive-root';
    return [['powershell', '-NoProfile', '-NonInteractive', '-Command', windowsEjectScript(drive)]];
  }
  return 'needs-device';
}

/** How the Windows script says the drive was still there after its wait. */
export const WINDOWS_STILL_MOUNTED = 3;
export const STILL_IN_USE = 'something on this computer is still using the reader. Close it and try again.';

/** The Shell's Eject verb answers at once and exits 0 even when the drive
 *  is busy and stays, so the script then looks for the drive for up to five
 *  seconds (20 x 250 ms) and exits WINDOWS_STILL_MOUNTED if it is still
 *  there. `drive` is windowsDrive()'s answer, a letter and a colon and
 *  nothing else, so nothing from the volume path can be read as script. */
function windowsEjectScript(drive: string): string {
  return `(New-Object -comObject Shell.Application).Namespace(17).ParseName('${drive}').InvokeVerb('Eject'); `
    + `for ($i = 0; $i -lt 20; $i++) { if (-not (Test-Path '${drive}\\')) { exit 0 }; Start-Sleep -Milliseconds 250 }; `
    + `exit ${WINDOWS_STILL_MOUNTED}`;
}

/** Linux: the question that finds the block device behind a mount point.
 *  `--mountpoint`, never `--target`: a tolino is recognised by its folder
 *  name alone, so a listed volume can be a plain folder on an ordinary disk,
 *  and `--target` would walk up to the filesystem holding it and name that
 *  disk. `--mountpoint` answers nothing (exit 1) for a folder that is not a
 *  mount point itself, and the caller refuses. */
export function findDeviceCommand(volume: string): string[] {
  return ['findmnt', '-n', '-o', 'SOURCE', '--mountpoint', volume];
}

/** Linux: unmount, then power off, in that order. */
export function linuxEjectCommands(device: string): string[][] {
  return [
    ['udisksctl', 'unmount', '-b', device],
    ['udisksctl', 'power-off', '-b', device],
  ];
}

/** The real runner, via Bun.spawn. The environment is passed through on
 *  purpose (the house rule for every spawn), and stdin is ignored: none of
 *  these tools reads it. Past `timeoutMs` the child is killed and the
 *  answer is `code: null`. */
export const spawnEject: EjectRun = async (argv, timeoutMs) => {
  const proc = Bun.spawn(argv, { stdout: 'pipe', stderr: 'pipe', stdin: 'ignore', env: process.env });
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    proc.kill();
  }, timeoutMs);
  try {
    const [code, stdout, stderr] = await Promise.all([
      proc.exited,
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    return { code: timedOut ? null : code, stdout, stderr };
  } finally {
    clearTimeout(timer);
  }
};
