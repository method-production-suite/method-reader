// The device-request service (services/device-requests/): the Cloudflare
// Worker that counts "My device isn't here" requests and "It worked" /
// "It did not work" reports (spec 2026-10-01, decisions 14 and 15). Its
// fetch handler is called directly, against a stand-in for D1 built on
// bun:sqlite with the real schema.sql, so every path is the one deployed.
// Nothing here reaches Cloudflare or any network.
import { Database } from 'bun:sqlite';
import { beforeEach, describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import worker, {
  ALLOWED_ORIGINS, handleRequest, LIMIT_PER_MINUTE, MAX_BODY_BYTES, normaliseName, type D1Database, type Env,
  type RateLimit,
} from '../services/device-requests/src/worker';

const SERVICE = join(import.meta.dir, '..', 'services', 'device-requests');
const SCHEMA = readFileSync(join(SERVICE, 'schema.sql'), 'utf8');
const URL_ = 'https://screepub-device-requests.example.workers.dev/request';
const IP = '203.0.113.77';
const ORIGIN = 'tauri://localhost';

/** D1's prepare/bind/run/first/all, over an in-memory SQLite. */
function fakeD1(db: Database): D1Database {
  const statement = (sql: string, values: unknown[]) => ({
    bind: (...next: unknown[]) => statement(sql, next),
    run: async () => {
      const r = db.query(sql).run(...(values as never[]));
      return { success: true, meta: { changes: r.changes } };
    },
    first: async <T>() => (db.query(sql).get(...(values as never[])) ?? null) as T | null,
    all: async <T>() => ({ results: db.query(sql).all(...(values as never[])) as T[] }),
  });
  return { prepare: (sql: string) => statement(sql, []) };
}

/** Cloudflare's rate-limit binding: LIMIT_PER_MINUTE calls per key, then
 *  refused. Its counters live at Cloudflare, never in D1; `keys` records
 *  what it was asked about, so a test can see what the key was. */
function fakeLimiter(): RateLimit & { keys: string[] } {
  const counts = new Map<string, number>();
  const keys: string[] = [];
  return {
    keys,
    limit: async ({ key }) => {
      keys.push(key);
      const n = (counts.get(key) ?? 0) + 1;
      counts.set(key, n);
      return { success: n <= LIMIT_PER_MINUTE };
    },
  };
}

let db: Database;
let limiter: ReturnType<typeof fakeLimiter>;
let env: Env;
beforeEach(() => {
  db = new Database(':memory:');
  db.exec(SCHEMA);
  limiter = fakeLimiter();
  env = { DB: fakeD1(db), RATE_LIMITER: limiter };
});

const device = (name: string, extra: Record<string, unknown> = {}) =>
  ({ kind: 'device', device: name, version: '0.7.4', platform: 'mac', ...extra });
const report = (extra: Record<string, unknown> = {}) =>
  ({ kind: 'report', route: 'kobo', worked: true, version: '0.7.4', platform: 'windows', ...extra });

function post(body: unknown, o: { ip?: string; origin?: string | null; type?: string; raw?: string; path?: string; method?: string } = {}) {
  const headers: Record<string, string> = { 'content-type': o.type ?? 'application/json', 'cf-connecting-ip': o.ip ?? IP };
  if (o.origin !== null) headers.origin = o.origin ?? ORIGIN;
  return new Request(o.path ?? URL_, {
    method: o.method ?? 'POST', headers, body: o.raw ?? JSON.stringify(body),
  });
}

async function send(req: Request, now?: Date) {
  const res = now === undefined ? await worker.fetch(req, env) : await handleRequest(req, env, now);
  const text = await res.text();
  return { res, status: res.status, body: text === '' ? null : JSON.parse(text) };
}

const rows = (table: string) => db.query(`SELECT * FROM ${table}`).all() as Record<string, unknown>[];

describe('a device request', () => {
  test('is counted, and answers ok', async () => {
    const r = await send(post(device('PocketBook Era')));
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ ok: true });
    expect(r.res.headers.get('content-type')).toStartWith('application/json');
    const [row] = rows('device_requests');
    expect(row.name_key).toBe('pocketbook era');
    expect(row.spelling).toBe('PocketBook Era');
    expect(row.count).toBe(1);
  });

  test('the same device asked again adds to its count, keeping the latest spelling and both days', async () => {
    await send(post(device('PocketBook Era')), new Date('2026-10-02T10:00:00Z'));
    await send(post(device('  pocketbook    ERA ', { platform: 'linux' }), { ip: '198.51.100.1' }), new Date('2026-10-03T11:00:00Z'));
    const all = rows('device_requests');
    expect(all.length).toBe(1);
    expect(all[0].count).toBe(2);
    expect(all[0].spelling).toBe('pocketbook    ERA');
    // The day only: a time to the millisecond could pick one request out.
    expect(all[0].first_seen).toBe('2026-10-02');
    expect(all[0].last_seen).toBe('2026-10-03');
    // A different device is its own row.
    await send(post(device('Boox Note')), new Date('2026-10-03T11:00:00Z'));
    expect(rows('device_requests').length).toBe(2);
  });

  test('normalises by lowercasing, trimming and collapsing spaces', () => {
    expect(normaliseName('  Kobo   Libra\t2 ')).toBe('kobo libra 2');
  });

  test('takes 80 characters after trimming, and refuses 81 or none', async () => {
    expect((await send(post(device(`  ${'x'.repeat(80)}  `)))).status).toBe(200);
    for (const name of ['x'.repeat(81), '', '    ']) {
      const r = await send(post(device(name), { ip: '198.51.100.9' }));
      expect(r.status).toBe(400);
      expect(r.body.ok).toBe(false);
      expect(typeof r.body.error).toBe('string');
    }
  });
});

describe('a report', () => {
  test('counts worked and did not work per route and platform', async () => {
    await send(post(report()));
    await send(post(report({ worked: false })));
    await send(post(report({ worked: true })));
    await send(post(report({ route: 'kindle-linux', platform: 'linux', worked: false })));
    const all = rows('route_reports').sort((a, b) => String(a.route).localeCompare(String(b.route)));
    expect(all.map((r) => [r.route, r.platform, r.worked, r.did_not_work])).toEqual([
      ['kindle-linux', 'linux', 0, 1],
      ['kobo', 'windows', 2, 1],
    ]);
  });

  test('takes every route the window asks about', async () => {
    for (const route of ['kobo', 'tolino', 'remarkable', 'kindle-windows', 'kindle-linux']) {
      expect((await send(post(report({ route }), { ip: `198.51.100.${route.length}` }))).status).toBe(200);
    }
  });
});

describe('refused', () => {
  const bad: [string, unknown][] = [
    ['an unknown kind', { kind: 'vote', device: 'x', version: '0.7.4', platform: 'mac' }],
    ['an extra field', device('Boox', { email: 'a@b.c' })],
    ['a missing field', { kind: 'device', device: 'Boox', platform: 'mac' }],
    ['a number for a name', device(42 as unknown as string)],
    ['an unknown platform', device('Boox', { platform: 'ios' })],
    ['a version over 20 characters', device('Boox', { version: '1'.repeat(21) })],
    ['an empty version', device('Boox', { version: '' })],
    ['an unknown route', report({ route: 'kindle-mac' })],
    ['worked as a string', report({ worked: 'yes' })],
    ['a report carrying a device', report({ device: 'Kobo' })],
    ['an array', [device('Boox')]],
    ['null', null],
  ];
  for (const [what, body] of bad) {
    test(`${what}: 400, and nothing stored`, async () => {
      const r = await send(post(body));
      expect(r.status).toBe(400);
      expect(r.body.ok).toBe(false);
      expect(rows('device_requests')).toEqual([]);
      expect(rows('route_reports')).toEqual([]);
    });
  }

  test('a body that is not JSON: 400', async () => {
    expect((await send(post(null, { raw: '{not json' }))).status).toBe(400);
  });

  test('a body that is not marked as JSON: 400', async () => {
    expect((await send(post(device('Boox'), { type: 'text/plain' }))).status).toBe(400);
  });

  test('a body over 1 KB: 413', async () => {
    const big = JSON.stringify(device('Boox', { pad: 'x'.repeat(MAX_BODY_BYTES) }));
    const r = await send(post(null, { raw: big }));
    expect(r.status).toBe(413);
    expect(r.body.ok).toBe(false);
  });

  test('any other method or path: 404', async () => {
    expect((await send(post(device('Boox'), { method: 'PUT' }))).status).toBe(404);
    expect((await send(new Request(URL_, { method: 'GET' })))).toMatchObject({ status: 404 });
    expect((await send(post(device('Boox'), { path: 'https://x.workers.dev/' }))).status).toBe(404);
    expect((await send(post(device('Boox'), { path: 'https://x.workers.dev/request/more' }))).status).toBe(404);
    expect(rows('device_requests')).toEqual([]);
  });

  test('every error is said in plain words, without an em dash', async () => {
    const said = [
      (await send(post(device('')))).body.error,
      (await send(post(null, { raw: 'x'.repeat(2000) }))).body.error,
      (await send(post(device('Boox'), { method: 'PUT' }))).body.error,
    ];
    for (const line of said) {
      expect(line).toMatch(/^[A-Z].*\.$/);
      expect(line).not.toContain('—');
    }
  });
});

describe('the rate limit', () => {
  test(`${LIMIT_PER_MINUTE} accepted a minute per sender, then 429, while another sender still gets through`, async () => {
    for (let i = 0; i < LIMIT_PER_MINUTE; i++) expect((await send(post(device(`Boox ${i}`)))).status).toBe(200);
    const over = await send(post(device('Boox again')));
    expect(over.status).toBe(429);
    expect(over.body.ok).toBe(false);
    expect(rows('device_requests').length).toBe(LIMIT_PER_MINUTE);
    expect((await send(post(device('Boox other'), { ip: '198.51.100.2' }))).status).toBe(200);
  });

  test('a refused request does not use up a place: the limiter is asked only about a good form', async () => {
    for (let i = 0; i < 10; i++) await send(post(device('')));
    expect(limiter.keys).toEqual([]);
    expect((await send(post(device('Boox')))).status).toBe(200);
  });

  test('the limiter is Cloudflare\'s binding, keyed on the connecting address, and nothing of it reaches D1', async () => {
    await send(post(device('Boox')));
    await send(post(report()));
    expect(limiter.keys).toEqual([IP, IP]);
    const tables = (db.query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all() as { name: string }[])
      .map((t) => t.name);
    expect(tables).toEqual(['device_requests', 'route_reports']);
    const everything = JSON.stringify([rows('device_requests'), rows('route_reports')]);
    expect(everything).not.toContain(IP);
  });

  test('a limiter that fails is a 500 in plain words, and nothing is counted', async () => {
    env = { DB: fakeD1(db), RATE_LIMITER: { limit: async () => { throw new Error('limiter down'); } } };
    const r = await send(post(device('Boox')));
    expect(r.status).toBe(500);
    expect(r.body.error).not.toContain('limiter');
    expect(rows('device_requests')).toEqual([]);
  });

  test('the tables hold nothing beyond their counts', () => {
    const columns = (table: string) => (db.query(`PRAGMA table_info(${table})`).all() as { name: string }[]).map((c) => c.name);
    expect(columns('device_requests')).toEqual(['name_key', 'spelling', 'count', 'first_seen', 'last_seen']);
    expect(columns('route_reports')).toEqual(['route', 'platform', 'worked', 'did_not_work', 'first_seen', 'last_seen']);
  });

  test('wrangler.toml binds RATE_LIMITER at the limit the Worker says, per 60 seconds', () => {
    const toml = readFileSync(join(SERVICE, 'wrangler.toml'), 'utf8');
    const block = /\[\[ratelimits\]\]([\s\S]*?)(?=\n\[\[|$)/.exec(toml)?.[1] ?? '';
    expect(block).toMatch(/^name = "RATE_LIMITER"$/m);
    expect(block).toMatch(/^namespace_id = "[1-9][0-9]*"$/m);
    expect(block).toMatch(new RegExp(`^\\s*limit = ${LIMIT_PER_MINUTE}$`, 'm'));
    expect(block).toMatch(/^\s*period = 60$/m);
  });
});

describe('the migration for the live database', () => {
  // What the first deploy made (2026-10-02): the counts, with times to the
  // millisecond, and the hourly table of hashed senders.
  const OLD = `
    CREATE TABLE device_requests (name_key TEXT PRIMARY KEY, spelling TEXT NOT NULL, count INTEGER NOT NULL,
      first_seen TEXT NOT NULL, last_seen TEXT NOT NULL);
    CREATE TABLE route_reports (route TEXT NOT NULL, platform TEXT NOT NULL, worked INTEGER NOT NULL DEFAULT 0,
      did_not_work INTEGER NOT NULL DEFAULT 0, first_seen TEXT NOT NULL, last_seen TEXT NOT NULL, PRIMARY KEY (route, platform));
    CREATE TABLE rate_limits (sender TEXT NOT NULL, hour INTEGER NOT NULL, count INTEGER NOT NULL, PRIMARY KEY (sender, hour));
    CREATE INDEX rate_limits_hour ON rate_limits (hour);
    INSERT INTO device_requests VALUES ('boox', 'Boox', 3, '2026-10-02T10:00:00.000Z', '2026-10-04T23:59:59.999Z');
    INSERT INTO route_reports VALUES ('kobo', 'mac', 1, 0, '2026-10-03T08:00:00.000Z', '2026-10-03T08:00:00.000Z');
    INSERT INTO rate_limits VALUES ('${'a'.repeat(64)}', 489000, 1);`;
  const MIGRATION = readFileSync(join(SERVICE, 'migrations', '0001-no-senders-day-dates.sql'), 'utf8');

  test('drops the senders table and its index, rounds every date to its day, and keeps the counts', () => {
    const old = new Database(':memory:');
    old.exec(OLD);
    old.exec(MIGRATION);
    const names = (old.query("SELECT name FROM sqlite_master WHERE name LIKE 'rate_limits%'").all());
    expect(names).toEqual([]);
    expect(old.query('SELECT * FROM device_requests').all()).toEqual([
      { name_key: 'boox', spelling: 'Boox', count: 3, first_seen: '2026-10-02', last_seen: '2026-10-04' }]);
    expect(old.query('SELECT * FROM route_reports').all()).toEqual([
      { route: 'kobo', platform: 'mac', worked: 1, did_not_work: 0, first_seen: '2026-10-03', last_seen: '2026-10-03' }]);
    // Safe to run twice, and on a database made from today's schema.sql.
    old.exec(MIGRATION);
    const fresh = new Database(':memory:');
    fresh.exec(SCHEMA);
    fresh.exec(MIGRATION);
  });
});

describe('CORS', () => {
  test('the window’s origins may post JSON, and a preflight is answered', async () => {
    for (const origin of ALLOWED_ORIGINS) {
      const pre = await send(new Request(URL_, {
        method: 'OPTIONS',
        headers: { origin, 'access-control-request-method': 'POST', 'access-control-request-headers': 'content-type' },
      }));
      expect(pre.status).toBe(204);
      expect(pre.res.headers.get('access-control-allow-origin')).toBe(origin);
      expect(pre.res.headers.get('access-control-allow-methods')).toBe('POST');
      expect(pre.res.headers.get('access-control-allow-headers')?.toLowerCase()).toBe('content-type');
      const r = await send(post(device('Boox'), { origin, ip: `${origin}-ip` }));
      expect(r.res.headers.get('access-control-allow-origin')).toBe(origin);
      expect(r.res.headers.get('vary')).toBe('Origin');
    }
    expect(ALLOWED_ORIGINS).toEqual(['tauri://localhost', 'http://tauri.localhost', 'https://tauri.localhost']);
  });

  test('any other origin is not allowed, and an error answer carries the header too', async () => {
    const pre = await send(new Request(URL_, { method: 'OPTIONS', headers: { origin: 'https://evil.example' } }));
    expect(pre.res.headers.get('access-control-allow-origin')).toBeNull();
    const refused = await send(post(device('')));
    expect(refused.res.headers.get('access-control-allow-origin')).toBe(ORIGIN);
  });

  test('a preflight anywhere but /request is a 404', async () => {
    const r = await send(new Request('https://x.workers.dev/other', { method: 'OPTIONS', headers: { origin: ORIGIN } }));
    expect(r.status).toBe(404);
  });
});

test('a body that cannot be read is a 400 the window can read, with its CORS header', async () => {
  // The connection drops mid-body: without its own answer, the runtime's
  // bare 500 has no CORS header, and the window would say "check that this
  // computer is online" for what was the service's problem.
  const broken = new ReadableStream({ start(c) { c.error(new Error('connection reset')); } });
  const req = new Request(URL_, {
    method: 'POST', body: broken,
    headers: { 'content-type': 'application/json', origin: ORIGIN, 'cf-connecting-ip': IP },
  });
  const r = await send(req);
  expect(r.status).toBe(400);
  expect(r.body.ok).toBe(false);
  expect(r.res.headers.get('access-control-allow-origin')).toBe(ORIGIN);
});

test('a database that fails says so in plain words, as a 500', async () => {
  env = { DB: { prepare: () => { throw new Error('D1 is down'); } } as unknown as D1Database, RATE_LIMITER: limiter };
  const r = await send(post(device('Boox')));
  expect(r.status).toBe(500);
  expect(r.body.ok).toBe(false);
  expect(r.body.error).not.toContain('D1');
});
