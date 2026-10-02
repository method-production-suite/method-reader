// The device-request service, from the window's side (spec 2026-10-01,
// decisions 14 and 15; the Worker is services/device-requests/). Two small
// forms, sent only when the reader presses Send in "My device isn't here" or
// answers "It worked" / "It did not work" beside an asterisk: a device's
// name, or a route and whether it worked, with the app's version and the
// platform. Nothing else, and nothing queued: a send that fails says so and
// the reader tries again or does not. This is the window's one fetch, and
// the CSP's connect-src names this one address (tauri.conf.json).
import { platformOf } from './send-routes.js';

/** PLACEHOLDER until the Worker is deployed: the owner's workers.dev
 *  subdomain, the part `bunx wrangler deploy` prints between the Worker's
 *  name and `.workers.dev`. Change the CSP's connect-src in
 *  desktop/src-tauri/tauri.conf.json to match (a test holds them together). */
export const SUBDOMAIN = 'YOUR-SUBDOMAIN';
export const REQUEST_URL = `https://screepub-device-requests.${SUBDOMAIN}.workers.dev/request`;

export const MAX_DEVICE = 80;
/** A send that hears nothing back in this long has failed. */
export const TIMEOUT_MS = 15_000;

export const LABEL = 'Which device?';
export const SEND = 'Send';
export const TRY_AGAIN = 'Try again';
export const WHAT_IS_SENT = 'Sends the device’s name, Screepub’s version and your platform. Nothing else.';
export const THANKS = 'Thanks. We count every request.';
export const WORKED = 'It worked';
export const DID_NOT_WORK = 'It did not work';

const OFFLINE = 'Could not send. Check that this computer is online, then try again.';
const TOO_MANY = 'Could not send: too many requests from this computer this hour. Try again later.';
const GARBLED = 'Could not send. The request service did not answer properly. Try again later.';

/** "My device isn't here": the name as typed (trimmed). */
export function deviceForm(name, { version, platform }) {
  return { kind: 'device', device: String(name ?? '').trim(), version, platform: platformOf(platform) };
}

/** The Worker's name for a route nobody here has tried on real hardware, or
 *  null when there is none to report on (a Kindle on a Mac is proven). */
export function reportRoute(kind, platform) {
  if (kind === 'kobo' || kind === 'tolino' || kind === 'remarkable') return kind;
  if (kind !== 'kindle') return null;
  const os = platformOf(platform);
  return os === 'windows' || os === 'linux' ? `kindle-${os}` : null;
}

export function reportForm(kind, worked, { version, platform }) {
  return { kind: 'report', route: reportRoute(kind, platform), worked, version, platform: platformOf(platform) };
}

/** Sends one form. Answers `{ ok, line }`: the thanks, or why it could not
 *  send, in words the window shows as they are. Never throws. */
export async function postRequest(form, fetchImpl) {
  if (typeof fetchImpl !== 'function') return { ok: false, line: OFFLINE };
  const abort = typeof AbortController === 'function' ? new AbortController() : null;
  const timer = abort === null ? null : setTimeout(() => abort.abort(), TIMEOUT_MS);
  let response;
  try {
    response = await fetchImpl(REQUEST_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(form),
      signal: abort?.signal,
    });
  } catch {
    return { ok: false, line: OFFLINE };
  } finally {
    if (timer !== null) clearTimeout(timer);
  }
  if (response.status === 429) return { ok: false, line: TOO_MANY };
  let body = null;
  try {
    body = await response.json();
  } catch {
    body = null;
  }
  if (response.ok && body?.ok === true) return { ok: true, line: THANKS };
  const said = typeof body?.error === 'string' && body.error.trim() !== '' ? body.error.trim() : null;
  return { ok: false, line: said === null ? GARBLED : `Could not send. ${said}` };
}
