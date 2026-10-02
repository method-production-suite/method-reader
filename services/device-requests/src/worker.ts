// The device-request service (spec 2026-10-01, decisions 14 and 15): a
// Cloudflare Worker on the free plan, with a free D1 database behind it.
//
// The window posts one of two small JSON forms to POST /request:
//   { kind: "device", device, version, platform }   "My device isn't here"
//   { kind: "report", route, worked, version, platform }   "It worked" or not
// and this counts them. It stores no IP address, no user agent and nothing
// else: a device request adds to its device's count, a report to its
// route's. The rate limit keys on a SHA-256 of the connecting address and a
// salt that changes every day, never the address itself.
//
// Dependency-free on purpose: the few D1 types it uses are declared here
// rather than pulling in @cloudflare/workers-types. Tested by
// tests/device-requests-worker.test.ts against bun:sqlite.

/** The subset of D1 this Worker uses. */
export interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  run(): Promise<unknown>;
  first<T = Record<string, unknown>>(): Promise<T | null>;
  all<T = Record<string, unknown>>(): Promise<{ results: T[] }>;
}
export interface D1Database {
  prepare(sql: string): D1PreparedStatement;
}
export interface Env {
  DB: D1Database;
}

export const PATH = '/request';
export const MAX_BODY_BYTES = 1024;
export const MAX_DEVICE = 80;
export const MAX_VERSION = 20;
export const LIMIT_PER_HOUR = 5;
export const PLATFORMS = ['mac', 'windows', 'linux'] as const;
export const ROUTES = ['kobo', 'tolino', 'remarkable', 'kindle-windows', 'kindle-linux'] as const;
/** The Tauri window's own origins: macOS and Linux, then Windows (http and
 *  https, as Tauri's useHttpsScheme decides). */
export const ALLOWED_ORIGINS = ['tauri://localhost', 'http://tauri.localhost', 'https://tauri.localhost'];
/** Not a secret: it only makes the daily hash this service's own. */
const SALT = 'screepub-device-requests';
const HOUR_MS = 3_600_000;

const SAID = {
  notFound: 'Nothing here. Send a POST to /request.',
  tooLarge: 'That request is too large.',
  notJson: 'That request is not JSON.',
  badForm: 'That request is not one this service takes.',
  badDevice: `The device name must be 1 to ${MAX_DEVICE} characters.`,
  tooMany: 'Too many requests from here this hour. Try again later.',
  broken: 'Something went wrong on our side. Try again later.',
} as const;

type DeviceRequest = { kind: 'device'; device: string; version: string; platform: string };
type Report = { kind: 'report'; route: string; worked: boolean; version: string; platform: string };

/** Lowercased, trimmed, every run of spaces one space. */
export function normaliseName(name: string): string {
  return name.trim().replace(/\s+/g, ' ').toLowerCase();
}

function corsHeaders(origin: string | null): Record<string, string> {
  if (origin === null || !ALLOWED_ORIGINS.includes(origin)) return {};
  return { 'access-control-allow-origin': origin, vary: 'Origin' };
}

function answer(status: number, body: { ok: true } | { ok: false; error: string }, origin: string | null): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', ...corsHeaders(origin) },
  });
}
const refuse = (status: number, error: string, origin: string | null) => answer(status, { ok: false, error }, origin);

const hasExactly = (o: Record<string, unknown>, keys: string[]) => {
  const own = Object.keys(o);
  return own.length === keys.length && keys.every((k) => own.includes(k));
};
const isVersion = (v: unknown) => typeof v === 'string' && /^[0-9A-Za-z.+-]{1,20}$/.test(v) && v.length <= MAX_VERSION;
const isPlatform = (v: unknown) => typeof v === 'string' && (PLATFORMS as readonly string[]).includes(v);

/** The request, checked, or the words for why not. */
export function parseForm(body: unknown): DeviceRequest | Report | string {
  if (body === null || typeof body !== 'object' || Array.isArray(body)) return SAID.badForm;
  const o = body as Record<string, unknown>;
  if (o.kind === 'device') {
    if (!hasExactly(o, ['kind', 'device', 'version', 'platform'])) return SAID.badForm;
    if (!isVersion(o.version) || !isPlatform(o.platform)) return SAID.badForm;
    if (typeof o.device !== 'string') return SAID.badDevice;
    const name = o.device.trim();
    if (name.length < 1 || name.length > MAX_DEVICE) return SAID.badDevice;
    return { kind: 'device', device: name, version: o.version as string, platform: o.platform as string };
  }
  if (o.kind === 'report') {
    if (!hasExactly(o, ['kind', 'route', 'worked', 'version', 'platform'])) return SAID.badForm;
    if (typeof o.route !== 'string' || !(ROUTES as readonly string[]).includes(o.route)) return SAID.badForm;
    if (typeof o.worked !== 'boolean' || !isVersion(o.version) || !isPlatform(o.platform)) return SAID.badForm;
    return { kind: 'report', route: o.route, worked: o.worked, version: o.version as string, platform: o.platform as string };
  }
  return SAID.badForm;
}

async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** Who is asking, as a hash that changes every day. The address is used
 *  here and nowhere else. */
export function senderKey(ip: string, now: Date): Promise<string> {
  return sha256Hex(`${SALT}|${now.toISOString().slice(0, 10)}|${ip}`);
}

/** Takes one of this hour's places for `sender`, or false when they are
 *  gone. A new hour's first request deletes every older hour. */
async function takePlace(db: D1Database, sender: string, hour: number): Promise<boolean> {
  const row = await db.prepare(
    'INSERT INTO rate_limits (sender, hour, count) VALUES (?, ?, 1) '
    + 'ON CONFLICT (sender, hour) DO UPDATE SET count = count + 1 WHERE count < ? RETURNING count',
  ).bind(sender, hour, LIMIT_PER_HOUR).first<{ count: number }>();
  if (row === null) return false;
  if (row.count === 1) await db.prepare('DELETE FROM rate_limits WHERE hour < ?').bind(hour).run();
  return true;
}

async function store(db: D1Database, form: DeviceRequest | Report, now: Date): Promise<void> {
  const at = now.toISOString();
  if (form.kind === 'device') {
    await db.prepare(
      'INSERT INTO device_requests (name_key, spelling, count, first_seen, last_seen) VALUES (?, ?, 1, ?, ?) '
      + 'ON CONFLICT (name_key) DO UPDATE SET count = count + 1, spelling = excluded.spelling, last_seen = excluded.last_seen',
    ).bind(normaliseName(form.device), form.device, at, at).run();
    return;
  }
  await db.prepare(
    'INSERT INTO route_reports (route, platform, worked, did_not_work, first_seen, last_seen) VALUES (?, ?, ?, ?, ?, ?) '
    + 'ON CONFLICT (route, platform) DO UPDATE SET worked = worked + excluded.worked, '
    + 'did_not_work = did_not_work + excluded.did_not_work, last_seen = excluded.last_seen',
  ).bind(form.route, form.platform, form.worked ? 1 : 0, form.worked ? 0 : 1, at, at).run();
}

export async function handleRequest(request: Request, env: Env, now: Date = new Date()): Promise<Response> {
  const origin = request.headers.get('origin');
  const path = new URL(request.url).pathname;
  if (path !== PATH) return refuse(404, SAID.notFound, origin);
  if (request.method === 'OPTIONS') {
    return new Response(null, {
      status: 204,
      headers: origin !== null && ALLOWED_ORIGINS.includes(origin) ? {
        ...corsHeaders(origin),
        'access-control-allow-methods': 'POST',
        'access-control-allow-headers': 'content-type',
        'access-control-max-age': '86400',
      } : {},
    });
  }
  if (request.method !== 'POST') return refuse(404, SAID.notFound, origin);

  const declared = Number(request.headers.get('content-length') ?? '0');
  if (declared > MAX_BODY_BYTES) return refuse(413, SAID.tooLarge, origin);
  const raw = await request.text();
  if (new TextEncoder().encode(raw).byteLength > MAX_BODY_BYTES) return refuse(413, SAID.tooLarge, origin);
  const type = request.headers.get('content-type') ?? '';
  if (!/^application\/json\b/i.test(type)) return refuse(400, SAID.notJson, origin);
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return refuse(400, SAID.notJson, origin);
  }
  const form = parseForm(body);
  if (typeof form === 'string') return refuse(400, form, origin);

  try {
    const sender = await senderKey(request.headers.get('cf-connecting-ip') ?? 'unknown', now);
    if (!(await takePlace(env.DB, sender, Math.floor(now.getTime() / HOUR_MS)))) {
      return refuse(429, SAID.tooMany, origin);
    }
    await store(env.DB, form, now);
    return answer(200, { ok: true }, origin);
  } catch {
    return refuse(500, SAID.broken, origin);
  }
}

export default {
  fetch: (request: Request, env: Env): Promise<Response> => handleRequest(request, env),
};
