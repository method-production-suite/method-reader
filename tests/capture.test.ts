// The capture tool without Chrome: the pure parts (the gate, the page, the
// shot list), the page server driven with plain Requests, and whole runs
// with a fake browser and a stand-in engine, down to what each one leaves
// behind. The Chrome driver is tested against a fake Chrome in
// capture-chrome.test.ts. Real Chrome runs only when someone runs
// `bun tools/capture-screens.ts`; /release does not call it yet.
import { afterAll, describe, test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Browser } from '../tools/capture/cdp';
import { gateEngineCall } from '../tools/capture/gate';
import { captureIndex } from '../tools/capture/page';
import { CaptureError, MARKER, claimLibrary, cleanupErrorsOf, runCapture, type RunOptions } from '../tools/capture/run';
import { makeHandler, type EngineResult } from '../tools/capture/server';
import { FRAME_PAD, SHOTS, framedSize, outputsFor, writeIfChanged } from '../tools/capture/shots';
// @ts-expect-error -- plain JS module, no types
import { argv } from '../desktop/ui/app.js';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-capture-test-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

// Every path below is built off the file's own location, not the working
// directory the suite happens to be launched from.
const ROOT = join(import.meta.dir, '..');

const ctx = {
  demoPdf: '/repo/tests/fixtures/field-station.pdf',
  library: '/Users/Shared/Documents/Screepub',
};
const inLib = (p: string) => `${ctx.library}/field-station/${p}`;

describe('the engine gate', () => {
  test('it allows exactly what the window asks for on the way to the pictures', () => {
    // Built with the WINDOW'S OWN argv builders, so a flag the window adds
    // tomorrow is still recognised as the same call.
    expect(gateEngineCall(argv.version(), ctx)).toEqual({ allow: true });
    expect(gateEngineCall(argv.convert(ctx.demoPdf), ctx)).toEqual({ allow: true });
    expect(gateEngineCall(argv.settings(inLib('field-station.fountain')), ctx)).toEqual({ allow: true });
    expect(
      gateEngineCall(argv.reconvert(inLib('field-station.fountain'), inLib('field-station.epub'), '{}'), ctx),
    ).toEqual({ allow: true });
    // The Convert page's library line reads the app settings every time the
    // drop area draws. A read: it writes nothing, and the capture points it
    // at a scratch settings folder anyway.
    expect(gateEngineCall(argv.appSettings(), ctx)).toEqual({ allow: true });
  });

  test('it allows the app-settings READ only: any --set, or anything else on it, is refused', () => {
    for (const args of [
      argv.appSettings('{"libraryPath":"/Users/me/Books"}'),
      argv.appSettings('{"keepScriptSettings":false}'),
      argv.appSettings('{"formatDefaults":null}'),
      [...argv.appSettings(), '--set', '{}'],
      [...argv.appSettings(), '--debug'],
      ['app-settings'],
    ]) {
      const a = gateEngineCall(args, ctx);
      expect(`${args.join(' ')}: ${a.allow}`).toBe(`${args.join(' ')}: false`);
    }
  });

  test('it refuses anything that reaches devices or writes outside the demo library, naming it', () => {
    for (const args of [
      argv.send(inLib('field-station.epub')),
      argv.export(inLib('field-station.epub'), { forFormat: 'kindle' }),
      argv.eject('/Volumes/Kindle'),
      argv.route('send-to-kindle', inLib('field-station.epub')),
    ]) {
      const a = gateEngineCall(args, ctx);
      expect(a.allow).toBe(false);
      if (!a.allow) expect(a.reason).toContain(args[0]!);
    }
  });

  test('it allows the result screen’s reads: devices, the routes and the Kindle check for the demo book', () => {
    const E = inLib('field-station.epub');
    for (const args of [
      argv.devices(), argv.devices({ quick: true }),
      argv.routes(E), argv.routes(E, { quick: true }), argv.kindleCheck(E),
      ...['kindle', 'apple', 'kobo', 'tolino', 'remarkable'].map((k) => argv.readerDevice(k)),
    ]) {
      const a = gateEngineCall(args, ctx);
      expect(`${args.join(' ')}: ${a.allow}`).toBe(`${args.join(' ')}: true`);
    }
  });

  test('but not for a book outside the library, nor any other app-settings write', () => {
    for (const args of [
      argv.routes('/Users/someone/elsewhere.epub'), argv.kindleCheck(`${ctx.library}-evil/x.epub`),
      argv.readerDevice('nook'), argv.appSettings('{"readerDevice":"kobo","libraryPath":"/x"}'),
      argv.export(inLib('field-station.epub'), { forFormat: 'epub', out: inLib('copy.epub') }),
    ]) {
      const a = gateEngineCall(args, ctx);
      expect(`${args.join(' ')}: ${a.allow}`).toBe(`${args.join(' ')}: false`);
    }
  });

  test('it refuses a conversion of any file but the demo script', () => {
    const a = gateEngineCall(argv.convert('/Users/someone/real-script.pdf'), ctx);
    expect(a.allow).toBe(false);
    if (!a.allow) expect(a.reason).toContain('real-script.pdf');
  });

  test('it refuses a re-render that would write outside the library', () => {
    const a = gateEngineCall(argv.reconvert(inLib('field-station.fountain'), '/tmp/elsewhere.epub', '{}'), ctx);
    expect(a.allow).toBe(false);
    // A path that merely STARTS with the library's name is not inside it.
    const b = gateEngineCall(argv.settings(`${ctx.library}-evil/x.fountain`), ctx);
    expect(b.allow).toBe(false);
  });

  test('an empty call is refused, not allowed by default', () => {
    expect(gateEngineCall([], ctx).allow).toBe(false);
  });

  const D = ctx.demoPdf;
  const F = inLib('field-station.fountain');
  const E = inLib('field-station.epub');

  // Each of these is CLOSE to a real call but not one the window's own argv
  // builders can produce: a smuggled flag, a repeated flag, a `--x=` form,
  // a missing constant flag, a relative path, a `..` escape, or a verb the
  // window never sends. A gate that only checks the first argument, or only
  // checks that -o lands in the library, allows every one of these.
  const REFUSALS: [string, string[]][] = [
    ['a flag smuggled onto a convert after --library',
      [...argv.convert(D), '--fountain', '/Users/me/real.fountain']],
    ['a second flag smuggled onto a convert after --library',
      [...argv.convert(D), '--preview-html', '/anywhere.html']],
    ['--output instead of -o', [...argv.convert(D), '--output', '/x.epub']],
    ['--output= form', [...argv.convert(D), '--output=/x.epub']],
    ['-o glued to its value', [...argv.convert(D), '-o/x.epub']],
    ['a repeated -o', [F, '--json', '--preview-inline', '-o', E, '-o', '/x.epub', '--options-json', '{}']],
    // The point of this one IS that it is missing --progress/--preview-inline/
    // --library, so it stays a hand-written short call rather than a full
    // argv.convert() plus an addition.
    ['a convert without --library, which would write into tests/fixtures', [D, '--json']],
    ['an unknown flag after --library', [...argv.convert(D), '--options', '/any/file']],
    ['a flag smuggled onto a reconvert after --options-json',
      [...argv.reconvert(F, E, '{}'), '--output=/x.epub']],
    ['a .. escape out of the library', argv.settings(`${ctx.library}/../../../Users/me/x.fountain`)],
    ['a relative demo path', argv.convert('tests/fixtures/field-station.pdf')],
    ['a relative settings path', argv.settings('field-station/x.fountain')],
    ['flag-first', ['--json', 'settings', F]],
    ['update-decision', ['update-decision', '--offered', '1', '--current', '0']],
    ['update-should-check', ['update-should-check', '--json']],
    ['extra args on version', ['--version', '--json', '--debug']],
    ['kfx-status: the Send page is never captured', argv.kfxStatus()],
    ['kfx-install: a capture must never install anything', argv.kfxInstall()],
  ];

  test('it refuses any call whose whole shape the window would not actually produce', () => {
    for (const [label, args] of REFUSALS) {
      const a = gateEngineCall(args, ctx);
      expect(`${label}: ${a.allow}`).toBe(`${label}: false`);
    }
  });

  test('it allows every shape the window really sends on the way to a picture', () => {
    const ALLOWS: string[][] = [
      argv.convert(D, { force: true }),
      argv.convert(D, { optionsJson: '{"a":1}' }),
      argv.convert(D, { force: true, optionsJson: '{}' }),
      argv.settings(F, '{"a":1}'),
    ];
    for (const args of ALLOWS) {
      expect(gateEngineCall(args, ctx)).toEqual({ allow: true });
    }
  });
});

describe('the window page the capture serves', () => {
  const real = readFileSync(join(ROOT, 'desktop', 'ui', 'index.html'), 'utf8');
  const cfg = { shot: 'result', demoPdf: '/Users/Shared/demo.pdf', token: 't' };

  test('it is the window’s own index.html, with the capture scripts inserted before main.js', () => {
    const out = captureIndex(real, cfg);
    // Every stylesheet the window loads, still loaded: the page is served,
    // not copied, so a stylesheet added to the window tomorrow comes along.
    for (const m of real.matchAll(/<link rel="stylesheet" href="([^"]+)"/g)) {
      expect(out).toContain(`href="${m[1]}"`);
    }
    const bridge = out.indexOf('/tools/capture/bridge.js');
    const main = out.indexOf('src="main.js"');
    const steps = out.indexOf('/tools/capture/steps.js');
    expect(bridge).toBeGreaterThan(0);
    expect(bridge).toBeLessThan(main);
    expect(steps).toBeGreaterThan(main);
    // Relative URLs resolve against the window's folder.
    expect(out).toContain('<base href="/desktop/ui/">');
    expect(out).toContain(JSON.stringify(cfg));
  });

  test('a window page it does not recognise fails loudly instead of capturing a blank', () => {
    expect(() => captureIndex('<html><body>nothing</body></html>', cfg)).toThrow(/main\.js/);
    const twice = real.replace('</body>', '<script type="module" src="main.js"></script></body>');
    expect(() => captureIndex(twice, cfg)).toThrow(/exactly once/);
  });

  test('a value cannot close the <script> early, or be read as a replace() substitution pattern', () => {
    const evil = { shot: '</script><script>alert(1)</script>', demoPdf: '/Users/a$&b/demo.pdf', token: 't' };
    const out = captureIndex(real, evil);
    expect(out).toContain('/Users/a$&b/demo.pdf');
    expect(out).not.toContain('</script><script>alert');
  });

  test('a <head> with attributes is not recognised, and fails loudly rather than skip the insert', () => {
    const html = '<html><head lang="en"></head><body>' +
      '<script type="module" src="main.js"></script></body></html>';
    expect(() => captureIndex(html, cfg)).toThrow(/no <head>/);
  });
});

describe('nothing from the capture ships in the window', () => {
  test('no file in desktop/ui knows the capture tool exists', () => {
    const ui = join(ROOT, 'desktop', 'ui');
    for (const f of readdirSync(ui)) {
      if (!/\.(js|html|css)$/.test(f)) continue;
      const text = readFileSync(join(ui, f), 'utf8');
      expect(`${f}: ${/__CAPTURE__|__captureState|tools\/capture/.test(text)}`).toBe(`${f}: false`);
    }
  });
});

describe('the shot list', () => {
  test('the four README pictures, with light and dark for the window ones', () => {
    expect(SHOTS.map((s) => s.name)).toEqual(['hero', 'drop', 'result', 'read']);
    for (const s of SHOTS) {
      expect(s.themes).toEqual(s.kind === 'window' ? ['light', 'dark'] : ['light']);
    }
  });

  test('the window pictures are the window’s own size', () => {
    // tauri.conf.json's window is 1280 by 800.
    const conf = JSON.parse(readFileSync(join(ROOT, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8'));
    const w = conf.app.windows[0];
    for (const s of SHOTS.filter((x) => x.kind === 'window')) {
      expect([s.width, s.height]).toEqual([w.width, w.height]);
    }
  });

  test('the README gets every picture; the site gets the light drop and result', () => {
    const all = SHOTS.flatMap((s) => s.themes.flatMap((t) => outputsFor(s, t)));
    expect(all.filter((p) => p.startsWith('assets/screens/')).length).toBe(7);
    expect(all.filter((p) => p.startsWith('site/img/')).sort()).toEqual([
      'site/img/drop-light.png',
      'site/img/result-light.png',
    ]);
  });

  test('a file is written only when its bytes differ', () => {
    const dir = mkdtempSync(join(SCRATCH, 'write-'));
    try {
      const path = join(dir, 'x.png');
      expect(writeIfChanged(path, new Uint8Array([1, 2, 3]))).toBe('written');
      expect(writeIfChanged(path, new Uint8Array([1, 2, 3]))).toBe('unchanged');
      expect(writeIfChanged(path, new Uint8Array([1, 2, 4]))).toBe('written');
      expect([...readFileSync(path)]).toEqual([1, 2, 4]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('the capture command and Chrome driver typecheck', () => {
  test('they import without running, so tsc covers them', async () => {
    // tsconfig.json includes only src/ and tests/; importing these here is
    // what puts them under `bunx tsc --noEmit`.
    const cmd = await import('../tools/capture-screens');
    const cdp = await import('../tools/capture/cdp');
    expect(typeof cmd.main).toBe('function');
    expect(cdp.CHROME).toBe('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome');
  });
});

describe('the capture server', () => {
  // A throwaway repository with the files a careless static server would
  // hand out: the gitignored REAL screenplays, git's own files, the package
  // manifest, and a sibling folder whose name starts with the repo's.
  function scene() {
    const dir = mkdtempSync(join(SCRATCH, 'scene-'));
    const repo = join(dir, 'Screepub');
    const put = (rel: string, body: string) => {
      mkdirSync(join(repo, rel, '..'), { recursive: true });
      writeFileSync(join(repo, rel), body);
    };
    put('desktop/ui/index.html', readFileSync(join(ROOT, 'desktop', 'ui', 'index.html'), 'utf8'));
    put('desktop/ui/main.js', '// main');
    put('site/index.html', '<!doctype html><title>site</title>');
    put('tools/capture/steps.js', '// steps');
    put('fixtures/real-script.pdf', 'A REAL SCREENPLAY');
    put('.git/HEAD', 'ref: refs/heads/main');
    put('package.json', '{}');
    mkdirSync(join(dir, 'Screepub-old'), { recursive: true });
    writeFileSync(join(dir, 'Screepub-old', 'x'), 'not served');
    const passOne = join(dir, 'pass-one');
    mkdirSync(passOne);
    writeFileSync(join(passOne, 'drop-light.png'), 'PNG');
    const calls: string[][] = [];
    let answer: EngineResult = { code: 0, stdout: '{"ok":true}\n', stderr: '' };
    const refused: string[] = [];
    const failed: string[] = [];
    const notFound: string[] = [];
    const library = '/Users/Shared/Documents/Screepub';
    const handle = makeHandler({
      repoDir: repo, passOne, token: 'secret', library,
      demoPdf: join(repo, 'tests', 'fixtures', 'field-station.pdf'),
      host: '127.0.0.1:4321',
      runEngine: async (args) => { calls.push(args); return answer; },
      refused, failed, notFound,
    });
    const get = (path: string, headers: Record<string, string> = {}) =>
      handle(new Request(`http://127.0.0.1:4321${path}`, { headers: { host: '127.0.0.1:4321', ...headers } }));
    const engine = (args: unknown, headers: Record<string, string> = { 'x-capture-token': 'secret' }) =>
      handle(new Request('http://127.0.0.1:4321/engine', {
        method: 'POST',
        headers: { host: '127.0.0.1:4321', 'content-type': 'application/json', ...headers },
        body: typeof args === 'string' ? args : JSON.stringify({ args }),
      }));
    return {
      dir, repo, library, calls, refused, failed, notFound, get, engine,
      answer: (a: EngineResult) => { answer = a; },
      done: () => rmSync(dir, { recursive: true, force: true }),
    };
  }

  test('it serves the window, the site and the capture scripts', async () => {
    const s = scene();
    try {
      for (const [path, type] of [
        ['/desktop/ui/main.js', 'text/javascript'],
        ['/site/index.html', 'text/html'],
        ['/tools/capture/steps.js', 'text/javascript'],
      ] as const) {
        const r = await s.get(path);
        expect(`${path} ${r.status}`).toBe(`${path} 200`);
        expect(r.headers.get('content-type')).toContain(type);
      }
    } finally { s.done(); }
  });

  test('it serves nothing outside the window, the site and the capture scripts', async () => {
    const s = scene();
    try {
      // Every one of these exists on disk, so a 404 is the rule, not luck.
      for (const path of [
        '/fixtures/real-script.pdf', '/.git/HEAD', '/package.json',
        '/..%2fScreepub-old%2fx', '/site/..%2f..%2ffixtures%2freal-script.pdf',
        '/site/%2e%2e/package.json', '/site/', '/site', '/desktop/ui',
      ]) {
        const r = await s.get(path);
        expect(`${path} ${r.status}`).toBe(`${path} 404`);
      }
      const bad = await s.get('/site/%E0%A4%A');
      expect(bad.status).toBe(400);
      // Every 404 is written down, so a capture that times out can say
      // which file the page asked for and did not get.
      expect(s.notFound).toContain('/.git/HEAD');
      expect(s.notFound).toContain('/..%2fScreepub-old%2fx');
    } finally { s.done(); }
  });

  test('a request for any other host is refused, so a rebound name cannot reach it', async () => {
    const s = scene();
    try {
      expect((await s.get('/site/index.html', { host: 'evil.example:4321' })).status).toBe(403);
      expect((await s.get('/site/index.html', { host: 'localhost:4321' })).status).toBe(403);
      const r = await s.engine(argv.version(), { host: 'evil.example:4321', 'x-capture-token': 'secret' });
      expect(r.status).toBe(403);
      expect(s.calls).toEqual([]);
    } finally { s.done(); }
  });

  test('/engine needs the run token, and refuses what the gate refuses, recording it', async () => {
    const s = scene();
    try {
      expect((await s.engine(argv.version(), {})).status).toBe(403);
      expect((await s.engine(argv.version(), { 'x-capture-token': 'guess' })).status).toBe(403);
      expect(s.calls).toEqual([]);
      const r = await s.engine(argv.eject('/Volumes/Kindle'));
      expect(r.status).toBe(403);
      expect(await r.text()).toContain('eject /Volumes/Kindle --json');
      expect(s.refused).toEqual([expect.stringContaining('eject /Volumes/Kindle --json')]);
      expect(s.calls).toEqual([]);
      expect((await s.engine('not json')).status).toBe(400);
      expect((await s.engine([1, 2])).status).toBe(400);
      const get = await s.get('/engine', { 'x-capture-token': 'secret' });
      expect(get.status).toBe(405);
    } finally { s.done(); }
  });

  test('an allowed call runs the engine with exactly its arguments and returns its answer', async () => {
    const s = scene();
    try {
      const r = await s.engine(argv.version());
      expect(r.status).toBe(200);
      expect(await r.text()).toBe('{"ok":true}');
      expect(s.calls).toEqual([argv.version()]);
      expect(s.failed).toEqual([]);
    } finally { s.done(); }
  });

  test('an engine that prints nothing is a 500 carrying what it said on stderr, as sidecar.rs does', async () => {
    const s = scene();
    try {
      s.answer({ code: 1, stdout: '', stderr: 'TypeError: boom\n' });
      const r = await s.engine(argv.version());
      expect(r.status).toBe(500);
      expect(await r.text()).toContain('TypeError: boom');
      expect(s.failed).toEqual([expect.stringContaining('TypeError: boom')]);
    } finally { s.done(); }
  });

  test('an engine that exits non-zero WITH an answer is passed through, as sidecar.rs does, and recorded', async () => {
    // sidecar.rs: an engine error exits 1 and prints a perfectly good error
    // object, which the window draws as its refusal. The run still fails.
    const s = scene();
    try {
      s.answer({ code: 1, stdout: '{"ok":false,"error":{"code":"scanned"}}', stderr: 'no text\n' });
      const r = await s.engine(argv.version());
      expect(r.status).toBe(200);
      expect(await r.text()).toContain('"scanned"');
      expect(s.failed).toEqual([expect.stringContaining('exited with 1')]);
    } finally { s.done(); }
  });

  test('the window page is served only for a window shot, carrying the run token', async () => {
    const s = scene();
    try {
      expect((await s.get('/capture/window.html?shot=hero')).status).toBe(400);
      expect((await s.get('/capture/window.html?shot=nope')).status).toBe(400);
      expect((await s.get('/capture/window.html')).status).toBe(400);
      const r = await s.get('/capture/window.html?shot=result');
      expect(r.status).toBe(200);
      const html = await r.text();
      expect(html).toContain('"token":"secret"');
      expect(html).toContain('/tools/capture/bridge.js');
    } finally { s.done(); }
  });

  test('pass-one pictures are served by bare name only', async () => {
    const s = scene();
    try {
      expect((await s.get('/pass-one/drop-light.png')).status).toBe(200);
      expect((await s.get('/pass-one/..%2f..%2fScreepub%2fpackage.json')).status).toBe(404);
      expect((await s.get('/pass-one/missing-light.png')).status).toBe(404);
      expect(s.notFound).toContain('/pass-one/missing-light.png');
    } finally { s.done(); }
  });
});

describe('a capture run cleans up after itself, whatever happens', () => {
  const PNG = new Uint8Array([137, 80, 78, 71]);

  function fakeBrowser(capture: (url: string) => Promise<Uint8Array>, events: string[] = [],
    close: () => Promise<void> = async () => {}): Browser {
    return {
      version: 'FakeChrome/1.0',
      capture: ({ url }) => capture(url),
      close: async () => { events.push('closed'); await close(); },
    };
  }

  // Every path is under a temp folder. bun test never touches /Users/Shared.
  function scene(opts: { documents?: boolean } = {}) {
    const dir = mkdtempSync(join(SCRATCH, 'scene-'));
    const documents = join(dir, 'Documents');
    if (opts.documents) mkdirSync(documents);
    const tmp = join(dir, 'tmp');
    mkdirSync(tmp);
    const lines: string[] = [];
    const library = join(documents, 'Screepub');
    const run = (over: Partial<RunOptions>) => runCapture({
      shots: SHOTS,
      repoDir: ROOT,
      demoPdf: join(ROOT, 'tests', 'fixtures', 'field-station.pdf'),
      library,
      engine: ['false'],
      outDir: join(dir, 'out'),
      tmp,
      log: (line) => lines.push(line),
      launch: async () => fakeBrowser(async () => PNG),
      ...over,
    });
    return {
      documents, library, tmp, lines, run,
      out: join(dir, 'out'),
      done: () => rmSync(dir, { recursive: true, force: true }),
    };
  }

  async function thrown(p: Promise<unknown>): Promise<unknown> {
    try { await p; } catch (e) { return e; }
    throw new Error('expected the run to throw');
  }

  test('Chrome failing to start leaves nothing behind, and its error is the one thrown', async () => {
    const s = scene();
    try {
      const boom = new Error('no Chrome today');
      expect(await thrown(s.run({ launch: async () => { throw boom; } }))).toBe(boom);
      expect(existsSync(s.library)).toBe(false);
      // The run created Documents, so the run removes it.
      expect(existsSync(s.documents)).toBe(false);
      expect(readdirSync(s.tmp)).toEqual([]);
    } finally { s.done(); }
  });

  test('a Documents folder that was there before the run is still there after it', async () => {
    const s = scene({ documents: true });
    try {
      const boom = new Error('no Chrome today');
      expect(await thrown(s.run({ launch: async () => { throw boom; } }))).toBe(boom);
      expect(existsSync(s.library)).toBe(false);
      expect(existsSync(s.documents)).toBe(true);
    } finally { s.done(); }
  });

  test('a capture that fails part-way writes no picture, closes Chrome, and throws the capture error', async () => {
    const s = scene();
    try {
      const events: string[] = [];
      const boom = new Error('capture failed at the read shot');
      let n = 0;
      const browser = fakeBrowser(async () => { if (++n === 6) throw boom; return PNG; }, events);
      expect(await thrown(s.run({ launch: async () => browser }))).toBe(boom);
      expect(events).toEqual(['closed']);
      expect(existsSync(s.out)).toBe(false);
      expect(existsSync(s.library)).toBe(false);
      expect(existsSync(s.documents)).toBe(false);
      expect(readdirSync(s.tmp)).toEqual([]);
    } finally { s.done(); }
  });

  test('a failure while cleaning up is reported after the original error, never instead of it', async () => {
    const s = scene();
    try {
      const boom = new Error('capture failed');
      const browser = fakeBrowser(async () => { throw boom; }, [],
        async () => { throw new Error('Chrome would not die'); });
      const err = await thrown(s.run({ launch: async () => browser }));
      expect(err).toBe(boom);
      expect(cleanupErrorsOf(err)).toEqual([expect.stringContaining('Chrome would not die')]);
      // The steps after the one that failed still ran.
      expect(existsSync(s.library)).toBe(false);
      expect(readdirSync(s.tmp)).toEqual([]);
    } finally { s.done(); }
  });

  test('a whole run writes every picture, and each window shot starts from an empty library', async () => {
    const s = scene();
    try {
      const seen: string[][] = [];
      const browser = fakeBrowser(async (url) => {
        if (url.includes('/capture/window.html')) {
          seen.push(readdirSync(s.library));
          // What a conversion leaves behind, which the next shot must not see.
          mkdirSync(join(s.library, 'field-station'), { recursive: true });
          writeFileSync(join(s.library, 'field-station', 'field-station.epub'), 'book');
        }
        return PNG;
      });
      await s.run({ launch: async () => browser });
      expect(seen.length).toBe(6);
      for (const entries of seen) expect(entries).toEqual([MARKER]);
      const all = SHOTS.flatMap((shot) => shot.themes.flatMap((t) => outputsFor(shot, t)));
      for (const out of all) expect([...readFileSync(join(s.out, out))]).toEqual([...PNG]);
      expect(s.lines[0]).toContain('FakeChrome/1.0');
      expect(s.lines.slice(1).sort()).toEqual(all.map((out) => `written  ${out}`).sort());
      expect(existsSync(s.library)).toBe(false);
      expect(existsSync(s.documents)).toBe(false);
    } finally { s.done(); }
  });

  /** A pid that belonged to a process a moment ago and to nothing now. */
  async function deadPid(): Promise<number> {
    const p = Bun.spawn(['true']);
    await p.exited;
    return p.pid;
  }

  test('a library left by an interrupted run is cleared on the next start, and what it created goes too', async () => {
    const s = scene();
    try {
      // An interrupted run whose process is gone: it created Documents and
      // the library, marked the library, and never got to clean up.
      claimLibrary(s.library, await deadPid());
      writeFileSync(join(s.library, 'half-a-book.epub'), 'x');
      const boom = new Error('no Chrome today');
      expect(await thrown(s.run({ launch: async () => { throw boom; } }))).toBe(boom);
      expect(existsSync(s.library)).toBe(false);
      expect(existsSync(s.documents)).toBe(false);
    } finally { s.done(); }
  });

  test('a library a capture that is still running is using is refused, naming its pid', async () => {
    const s = scene();
    try {
      // Marked by this very process, which is certainly alive.
      claimLibrary(s.library, process.pid);
      writeFileSync(join(s.library, 'book-in-progress.epub'), 'x');
      let launched = false;
      const err = await thrown(s.run({
        launch: async () => { launched = true; return fakeBrowser(async () => PNG); },
      }));
      expect(err).toBeInstanceOf(CaptureError);
      expect((err as Error).message).toContain(`capture pid ${process.pid} is using ${s.library}`);
      expect(launched).toBe(false);
      expect(readdirSync(s.library).sort()).toEqual([MARKER, 'book-in-progress.epub'].sort());
    } finally { s.done(); }
  });

  test('a marker that does not say whose it is is refused rather than guessed at', async () => {
    const s = scene();
    try {
      mkdirSync(s.library, { recursive: true });
      writeFileSync(join(s.library, MARKER), '{"made":"/"}\n');
      const err = await thrown(s.run({}));
      expect(err).toBeInstanceOf(CaptureError);
      expect((err as Error).message).toContain('pid');
      expect(readdirSync(s.library)).toEqual([MARKER]);
    } finally { s.done(); }
  });

  test('cleanup refuses an engine call made while it runs, instead of waiting on it', async () => {
    const s = scene();
    // A stand-in engine that runs until it is stopped, named so it can be
    // found afterwards: by a duration no other run of this suite uses, so
    // a second `bun test` at the same time (another worktree, a release's
    // preflight) is never found here, and never killed below.
    const seconds = `29.${process.pid}${String(Math.floor(Math.random() * 1e6)).padStart(6, '0')}`;
    const ENGINE = ['/bin/sh', '-c', `exec sleep ${seconds}`];
    const running = () =>
      Bun.spawnSync(['pgrep', '-f', `sleep ${seconds.replace('.', '\\.')}$`]).stdout.toString().trim();
    try {
      let post: (() => Promise<Response>) | null = null;
      let duringCleanup: { status: number; body: string } | null = null;
      const boom = new Error('capture failed');
      const browser: Browser = {
        version: 'FakeChrome/1.0',
        async capture({ url }) {
          if (!url.includes('/capture/window.html')) return PNG;
          // What the window does at boot: ask the engine for its version.
          const html = await (await fetch(url)).text();
          const token = /"token":"([^"]+)"/.exec(html)![1]!;
          post = () => fetch(new URL('/engine', url), {
            method: 'POST',
            headers: { 'content-type': 'application/json', 'x-capture-token': token },
            body: JSON.stringify({ args: argv.version() }),
          });
          void post().catch(() => {});
          while (running() === '') await Bun.sleep(10);
          throw boom;
        },
        // Chrome closes AFTER the engines are stopped, so a page still alive
        // at this point can post a new call. It must be refused, not run.
        async close() {
          const r = await post!();
          duringCleanup = { status: r.status, body: await r.text() };
        },
      };
      const shots = SHOTS.filter((x) => x.name === 'drop');
      const err = await thrown(s.run({ shots, engine: ENGINE, launch: async () => browser }));
      expect(err).toBe(boom);
      expect(duringCleanup!).toEqual({ status: 500, body: expect.stringContaining('cleaning up') });
      expect(running()).toBe('');
      expect(existsSync(s.library)).toBe(false);
    } finally {
      for (const pid of running().split('\n').filter(Boolean)) process.kill(Number(pid));
      s.done();
    }
  });

  test('the engine gets its own SCREEPUB_CONFIG_DIR, never the developer\'s real settings', async () => {
    // Piece C makes the app read its own format defaults on every
    // conversion. Without a SCREEPUB_CONFIG_DIR of the run's own, this
    // tool's pictures would pick up whatever the developer running it last
    // chose in their own app settings, and drift.
    const s = scene();
    try {
      const seen: { configDir?: string } = {};
      const ENGINE = ['/bin/sh', '-c', 'printf %s "$SCREEPUB_CONFIG_DIR"'];
      const boom = new Error('one engine call is enough for this test');
      const browser: Browser = {
        version: 'FakeChrome/1.0',
        async capture({ url }) {
          if (!url.includes('/capture/window.html')) return PNG;
          const html = await (await fetch(url)).text();
          const token = /"token":"([^"]+)"/.exec(html)![1]!;
          const r = await fetch(new URL('/engine', url), {
            method: 'POST',
            headers: { 'content-type': 'application/json', 'x-capture-token': token },
            body: JSON.stringify({ args: argv.version() }),
          });
          seen.configDir = await r.text();
          throw boom;
        },
        async close() {},
      };
      const shots = SHOTS.filter((x) => x.name === 'drop');
      const err = await thrown(s.run({ shots, engine: ENGINE, launch: async () => browser }));
      expect(err).toBe(boom);
      // Under the run's own scratch library, and a folder this run never
      // creates or writes into itself, so a read there finds nothing.
      expect(seen.configDir).toBe(join(s.library, '.capture-app-settings'));
    } finally { s.done(); }
  });

  test('a library the tool did not make is refused and left exactly as it was', async () => {
    const s = scene();
    try {
      mkdirSync(s.library, { recursive: true });
      writeFileSync(join(s.library, 'my-real-book.epub'), 'mine');
      let launched = false;
      const err = await thrown(s.run({
        launch: async () => { launched = true; return fakeBrowser(async () => PNG); },
      }));
      expect(err).toBeInstanceOf(CaptureError);
      expect((err as Error).message).toContain('already exists');
      expect(launched).toBe(false);
      expect(readdirSync(s.library)).toEqual(['my-real-book.epub']);
      expect(readdirSync(s.tmp)).toEqual([]);
    } finally { s.done(); }
  });
});

describe('the pieces that must agree with each other', () => {
  test('the frame page pads the window by exactly what the command sizes pass two for', () => {
    const html = readFileSync(join(ROOT, 'tools', 'capture', 'frame.html'), 'utf8');
    const m = html.match(/\.pad\s*\{\s*padding:\s*(\d+)px\s+(\d+)px\s+(\d+)px;\s*\}/);
    expect(m).not.toBeNull();
    const [top, sides, bottom] = m!.slice(1).map(Number);
    expect({ top, right: sides, bottom, left: sides }).toEqual({ ...FRAME_PAD });
    const drop = SHOTS.find((x) => x.name === 'drop')!;
    expect(framedSize(drop)).toEqual({ width: 1280 + 56 + 56, height: 800 + 44 + 68 });
  });

  test('the step runner has a step for every window shot, and nothing else', () => {
    const steps = readFileSync(join(ROOT, 'tools', 'capture', 'steps.js'), 'utf8');
    const block = steps.match(/const SHOTS = \{([\s\S]*?)\n\};/);
    expect(block).not.toBeNull();
    const names = [...block![1]!.matchAll(/^ {2}async (\w+)\(\)/gm)].map((m) => m[1]).sort();
    expect(names).toEqual(SHOTS.filter((x) => x.kind === 'window').map((x) => x.name).sort());
  });

  test('the window still draws the library slot the step runner hides', () => {
    // The Convert page's "Books are saved in ..." line names the capture's
    // own scratch library, set by SCREEPUB_LIBRARY, which no reader ever
    // has. steps.js hides the whole slot (the line, its buttons, and the
    // line under it) by this class, and fails the capture if it is gone.
    const convert = readFileSync(join(ROOT, 'desktop', 'ui', 'convert.js'), 'utf8');
    const slot = convert.slice(
      convert.indexOf('function buildLibrarySlot'), convert.indexOf('async function probeLibrary'));
    expect([...slot.matchAll(/class: 'well-ask well-library'/g)].length).toBe(2);
    const steps = readFileSync(join(ROOT, 'tools', 'capture', 'steps.js'), 'utf8');
    expect(steps).toContain('.well-library');
    expect(steps).toMatch(/\.well-library \{ display: none !important; \}/);
  });

  test('the window still draws the version stamp the step runner hides', () => {
    // Renamed, the stamp would stay visible and every picture would change
    // every release. steps.js also fails the capture if .rev-stamp is gone.
    const frame = readFileSync(join(ROOT, 'desktop', 'ui', 'frame.js'), 'utf8');
    expect(frame).toContain("class: 'rev-stamp'");
    const steps = readFileSync(join(ROOT, 'tools', 'capture', 'steps.js'), 'utf8');
    expect(steps).toContain('.rev-stamp');
  });
});
