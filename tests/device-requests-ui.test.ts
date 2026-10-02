// The window's side of the device-request service (desktop/ui/requests.js):
// the two forms it sends, what it says back, and the one address the CSP
// lets it reach. Every fetch here is a stand-in; nothing reaches a network.
// What a person sees and presses is in send-panel-ui.test.ts.
import { beforeAll, describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
let Q: any;
beforeAll(async () => { Q = await import(join(UI, 'requests.js')); });

const CONF = join(import.meta.dir, '..', 'desktop', 'src-tauri', 'tauri.conf.json');

type Call = { url: string; init: RequestInit };
function fakeFetch(reply: (call: Call) => Response | Promise<Response>) {
  const calls: Call[] = [];
  const fetch = async (url: string, init: RequestInit) => {
    const call = { url, init };
    calls.push(call);
    return reply(call);
  };
  return { fetch, calls };
}
const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('the forms', () => {
  test('a device request carries the name, the version and the platform, and nothing else', () => {
    expect(Q.deviceForm('  PocketBook Era ', { version: '0.7.4', platform: 'MacIntel' }))
      .toEqual({ kind: 'device', device: 'PocketBook Era', version: '0.7.4', platform: 'mac' });
    expect(Q.deviceForm('Boox', { version: '0.7.4', platform: 'Win32' }).platform).toBe('windows');
  });

  test('a report names the route the Worker knows, or none for a Kindle on a Mac', () => {
    expect(Q.reportRoute('kobo', 'MacIntel')).toBe('kobo');
    expect(Q.reportRoute('tolino', 'Linux x86_64')).toBe('tolino');
    expect(Q.reportRoute('remarkable', 'Win32')).toBe('remarkable');
    expect(Q.reportRoute('kindle', 'Win32')).toBe('kindle-windows');
    expect(Q.reportRoute('kindle', 'Linux x86_64')).toBe('kindle-linux');
    expect(Q.reportRoute('kindle', 'MacIntel')).toBe(null);
    expect(Q.reportRoute('kindle', '')).toBe(null);
    expect(Q.reportRoute('apple', 'MacIntel')).toBe(null);
    expect(Q.reportForm('kobo', false, { version: '0.7.4', platform: 'Linux x86_64' }))
      .toEqual({ kind: 'report', route: 'kobo', worked: false, version: '0.7.4', platform: 'linux' });
  });

  test('the field takes 80 characters, as the Worker does', () => {
    expect(Q.MAX_DEVICE).toBe(80);
  });
});

describe('sending', () => {
  test('posts JSON to the one address, and a yes is thanks', async () => {
    const f = fakeFetch(() => json(200, { ok: true }));
    const said = await Q.postRequest({ kind: 'device', device: 'Boox', version: '0.7.4', platform: 'mac' }, f.fetch);
    expect(said).toEqual({ ok: true, line: Q.THANKS });
    expect(Q.THANKS).toBe('Thanks. We count every request.');
    expect(f.calls.length).toBe(1);
    expect(f.calls[0].url).toBe(Q.REQUEST_URL);
    expect(f.calls[0].init.method).toBe('POST');
    expect((f.calls[0].init.headers as Record<string, string>)['content-type']).toBe('application/json');
    expect(JSON.parse(String(f.calls[0].init.body))).toEqual({ kind: 'device', device: 'Boox', version: '0.7.4', platform: 'mac' });
  });

  test('offline: could not send, in plain words', async () => {
    const f = fakeFetch(() => { throw new TypeError('Load failed'); });
    const said = await Q.postRequest({}, f.fetch);
    expect(said.ok).toBe(false);
    expect(said.line).toBe('Could not send. Check that this computer is online, then try again.');
  });

  test('too many: could not send, and says when to try', async () => {
    const said = await Q.postRequest({}, fakeFetch(() => json(429, { ok: false, error: 'Too many requests from here this hour. Try again later.' })).fetch);
    expect(said).toEqual({ ok: false, line: 'Could not send: too many requests from this computer this hour. Try again later.' });
  });

  test('any other refusal says the service’s own words after could not send', async () => {
    const said = await Q.postRequest({}, fakeFetch(() => json(400, { ok: false, error: 'The device name must be 1 to 80 characters.' })).fetch);
    expect(said).toEqual({ ok: false, line: 'Could not send. The device name must be 1 to 80 characters.' });
    const garbled = await Q.postRequest({}, fakeFetch(() => new Response('<html>502</html>', { status: 502 })).fetch);
    expect(garbled).toEqual({ ok: false, line: 'Could not send. The request service did not answer properly. Try again later.' });
    // A 200 that is not a yes is not a yes.
    const odd = await Q.postRequest({}, fakeFetch(() => json(200, { ok: false })).fetch);
    expect(odd.ok).toBe(false);
  });

  test('no fetch at all is offline, not a crash', async () => {
    expect((await Q.postRequest({}, undefined as never)).ok).toBe(false);
  });

  test('nothing it says carries an em dash', () => {
    for (const line of [Q.THANKS, Q.WHAT_IS_SENT, Q.LABEL, Q.SEND, Q.TRY_AGAIN, Q.WORKED, Q.DID_NOT_WORK]) {
      expect(line).not.toContain('—');
    }
  });
});

describe('the address', () => {
  test('is the Worker’s workers.dev address, its subdomain a marked placeholder until deploy', () => {
    expect(Q.REQUEST_URL).toBe(`https://screepub-device-requests.${Q.SUBDOMAIN}.workers.dev/request`);
    expect(Q.SUBDOMAIN).toMatch(/^[a-z0-9-]+$/i);
  });

  test('the CSP lets the window reach exactly that origin, and nothing else beyond itself', () => {
    const csp: string = JSON.parse(readFileSync(CONF, 'utf8')).app.security.csp;
    const directives = Object.fromEntries(csp.split(';').map((d) => d.trim()).filter(Boolean)
      .map((d) => { const [name, ...values] = d.split(/\s+/); return [name, values]; }));
    expect(directives['default-src']).toEqual(["'self'"]);
    // Spelled as the module spells it (a URL would lowercase the placeholder).
    expect(directives['connect-src']).toEqual(["'self'", Q.REQUEST_URL.replace(/\/request$/, '')]);
  });
});
