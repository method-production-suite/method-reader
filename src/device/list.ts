// The one list of everything currently reachable: volume-mounted vendors,
// which piece A detects by on-disk signature, plus a reMarkable, which never
// mounts and is only there if its USB web interface answers.
import { mountedDevices } from './volumes';
import { probeRemarkable, REMARKABLE_ENDPOINT, REMARKABLE_PROBE_TIMEOUT_MS } from './remarkable';
import { DEVICE_DISPLAY_NAMES, type ConnectedDevice } from './types';

export interface ListDevicesOptions {
  /** Mount parents to scan; defaults to this platform's (piece A). */
  roots?: string[];
  /** reMarkable base URL; defaults to the fixed USB address. */
  remarkableEndpoint?: string;
  probeTimeoutMs?: number;
  /** List only what the mount scan sees, and never start the reMarkable
   *  probe. The probe waits out its whole timeout on every machine with no
   *  tablet docked, so a caller that has to answer at once (`routes --quick`,
   *  the send panel's first answer) leaves the tablet out and asks again in
   *  full. */
  skipRemarkable?: boolean;
  /** Seams, injected only by tests. Production uses the two real functions. */
  scan?: (roots?: string[]) => ConnectedDevice[] | Promise<ConnectedDevice[]>;
  probe?: (endpoint: string, timeoutMs: number) => Promise<boolean>;
}

/** Every recognised reader that is reachable right now.
 *
 * The probe's timeout is paid on every machine that has no reMarkable, so the
 * mount scan is STARTED FIRST and both are awaited together: the command's
 * wall clock is the probe's timeout, never timeout + scan. The scan itself is
 * synchronous in production; the seam's return type allows a promise so the
 * concurrency is testable. */
export async function listDevices(options: ListDevicesOptions = {}): Promise<ConnectedDevice[]> {
  const scan = options.scan ?? mountedDevices;
  const probe = options.probe ?? probeRemarkable;
  const endpoint = options.remarkableEndpoint ?? REMARKABLE_ENDPOINT;
  const timeoutMs = options.probeTimeoutMs ?? REMARKABLE_PROBE_TIMEOUT_MS;

  const scanning = Promise.resolve(scan(options.roots));
  if (options.skipRemarkable === true) return [...(await scanning)];
  const probing = probe(endpoint, timeoutMs).catch(() => false);

  const [mounted, remarkablePresent] = await Promise.all([scanning, probing]);
  const devices = [...mounted];
  if (remarkablePresent) {
    devices.push({ kind: 'remarkable', name: DEVICE_DISPLAY_NAMES.remarkable, volume: null });
  }
  return devices;
}
