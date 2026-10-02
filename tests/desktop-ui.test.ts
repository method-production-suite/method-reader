import { afterAll, afterEach, beforeAll, beforeEach, describe, test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { kfxSetup } from '../src/export/kfx-setup';
import {
  FakeDocument, fakePage, HOLD, makeEvent, settle, withGlobals,
  type EngineCall, type FakeNode, type FakePage,
} from './helpers/fake-dom';
import { bootWindow, type BootedWindow } from './helpers/boot-window';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-desktop-ui-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const UI = join(new URL('..', import.meta.url).pathname, 'desktop', 'ui');
const read = (name: string) => readFileSync(join(UI, name), 'utf8');
const cssFiles = () => readdirSync(UI).filter((f) => f.endsWith('.css'));
const jsFiles = () => readdirSync(UI).filter((f) => f.endsWith('.js'));

/** A storage that behaves like localStorage, including its habit of handing
 *  back strings for everything. Shared by every update.js describe block
 *  below rather than copied three times. */
const store = (seed: Record<string, string> = {}) => {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => { map.set(k, String(v)); },
    dump: () => Object.fromEntries(map),
  };
};

describe('the window uses the brand, not its own colours', () => {
  test('no stylesheet but the two token files carries a hex literal', () => {
    // The spec's acceptance criterion 5, as a test: "a review can grep for a
    // hard-coded hex and find none". style.css and tokens.css are the two
    // declared copies, each pinned to brand/tokens.json by its own test.
    for (const name of cssFiles()) {
      if (name === 'style.css' || name === 'tokens.css') continue;
      const offenders = [...read(name).matchAll(/#[0-9a-fA-F]{3,8}\b/g)]
        .map((m) => m[0])
        .filter((h) => !['#fff', '#ffffff', '#000', '#000000'].includes(h.toLowerCase()));
      expect(`${name}: ${offenders.join(', ')}`).toBe(`${name}: `);
    }
  });

  test('no stylesheet sets a font size or radius in raw px', () => {
    // The same rule tests/brand-components.test.ts already enforces on the
    // previews. The window is the same design system.
    for (const name of cssFiles()) {
      const offenders = [...read(name).matchAll(/(font-size|border-radius):\s*[\d.]+px/g)]
        .map((m) => m[0]);
      expect(`${name}: ${offenders.join(', ')}`).toBe(`${name}: `);
    }
  });

  test('one file declares each token, so there is no "which one wins"', () => {
    // Carried in from Task 6's review: --radius was declared by BOTH
    // style.css and the generated tokens.css. The generator's exclusion list
    // covers only the seven COLOURS style.css owns, so nothing caught it.
    // This is the general form of that check: no custom property may be
    // declared by two of the window's stylesheets.
    const declared = new Map<string, string[]>();
    for (const name of cssFiles()) {
      for (const [, token] of read(name).matchAll(/^\s*(--[\w-]+)\s*:/gm)) {
        const seen = declared.get(token) ?? [];
        if (!seen.includes(name)) seen.push(name);
        declared.set(token, seen);
      }
    }
    const doubled = [...declared]
      .filter(([, files]) => files.length > 1)
      .map(([token, files]) => `${token} in ${files.join(' + ')}`);
    expect(doubled.join('; ')).toBe('');
    // Guards the loop: a regex that matched nothing would pass vacuously.
    expect(declared.size).toBeGreaterThan(30);
  });

  test('every stylesheet the page loads is bundled and local', () => {
    const html = read('index.html');
    const hrefs = [...html.matchAll(/<link[^>]+href="([^"]+)"/g)].map((m) => m[1]);
    expect(hrefs.length).toBeGreaterThanOrEqual(3);
    for (const href of hrefs) {
      expect(href.startsWith('http')).toBe(false);
      expect(href.startsWith('/')).toBe(false);
      expect(readdirSync(UI)).toContain(href);
    }
  });

  test('the fonts are declared from the bundled files, not from a CDN', () => {
    const css = read('style.css');
    expect(css).toContain('@font-face');
    expect(css).toContain('Courier Prime');
    expect(css).toContain('Literata');
    expect(css).not.toContain('fonts.googleapis.com');
    expect(css).not.toContain('fonts.gstatic.com');
    const urls = [...css.matchAll(/url\(([^)]+)\)/g)].map(([, u]) => u.replace(/['"]/g, ''));
    // Every face the six bundled subsets provide is actually declared: a
    // stylesheet that shipped one @font-face would otherwise pass.
    expect(urls.length).toBe(readdirSync(join(UI, 'fonts')).length);
    for (const url of urls) {
      expect(url).toMatch(/^fonts\/[a-z0-9-]+\.woff2$/);
      expect(readdirSync(join(UI, 'fonts'))).toContain(url.slice('fonts/'.length));
    }
  });
});

describe('the window respects the quality floor', () => {
  test('every animated stylesheet has a reduced-motion escape', () => {
    for (const name of cssFiles()) {
      const css = read(name);
      if (!/transition:|animation:/.test(css)) continue;
      expect(
        css.includes('prefers-reduced-motion'),
        `${name} animates but never checks prefers-reduced-motion`,
      ).toBe(true);
    }
  });

  test('it declares a narrow-window and a short-window layout', () => {
    const css = cssFiles().map(read).join('\n');
    expect(css).toMatch(/@media\s*\(max-width:/);
    expect(css).toMatch(/@media\s*\(max-height:/);
  });

  test('keyboard focus is drawn, never removed', () => {
    // `outline: none` with nothing in its place is the single most common way
    // a keyboard user loses their place in a hand-rolled interface.
    for (const name of cssFiles()) {
      const css = read(name);
      expect(`${name}: ${/outline:\s*(none|0)\b/.test(css)}`).toBe(`${name}: false`);
    }
    const css = cssFiles().map(read).join('\n');
    expect(css).toContain(':focus-visible');
  });

  test('no script sets an inline style, which this window’s CSP refuses', () => {
    // Kept as a source rule: the CSP is enforced only by the real webview,
    // and with `default-src 'self'` an inline style fails SILENTLY (measured
    // in piece C). The bar's width goes through an adopted stylesheet
    // instead, which "the bar follows the engine's own lines" drives.
    for (const name of jsFiles()) {
      expect(`${name} sets .style: ${/\.style\b/.test(read(name))}`).toBe(
        `${name} sets .style: false`,
      );
      // The key may be unquoted, and here it usually IS: dom.js falls
      // through to setAttribute for any key it does not special-case, so
      // `{ style: '…' }` is a real inline style in the house spelling. The
      // lookbehind keeps `font-style:` in read.js's @font-face from counting.
      const styleKey = /(?<![\w-])['"`]?style['"`]?\s*:/;
      expect(`${name} sets a style attribute: ${styleKey.test(read(name))}`).toBe(
        `${name} sets a style attribute: false`,
      );
    }
  });

});

describe('the window uses the brand, not its own colours (scripts too)', () => {
  test('no script builds a colour of its own', () => {
    // The CSS is checked above; a script can smuggle one in just as easily by
    // assigning a literal. The two brand-verbatim gradient stops in frame.js
    // (#fff / #000, copied from brand/components/brad.html) are three-digit
    // and deliberately not matched by the six-digit form.
    // Widened after a mutation slipped through: the six-digit-hex-and-rgb()
    // form let `'#f00'` and `'hsl(0 100% 50%)'` past. A colour is a colour in
    // whatever notation it is written, so every notation is named here, and
    // the brand-verbatim #fff/#000 are the only exemption.
    const KEEP = new Set(['#fff', '#ffffff', '#000', '#000000']);
    for (const name of jsFiles()) {
      const hexes = [...read(name).matchAll(/#[0-9a-fA-F]{3,8}\b/g)]
        .map((m) => m[0])
        .filter((h) => !KEEP.has(h.toLowerCase()));
      const funcs = [...read(name).matchAll(/\b(?:rgba?|hsla?|hwb|la[bc]|lch|oklab|oklch|color)\(/g)]
        .map((m) => m[0]);
      expect(`${name}: ${[...hexes, ...funcs].join(', ')}`).toBe(`${name}: `);
    }
  });
});

describe('the engine contract lives in exactly one file', () => {
  test('only app.js talks to Rust', () => {
    for (const name of jsFiles()) {
      if (name === 'app.js') continue;
      expect(`${name} invokes: ${/\binvoke\s*\(/.test(read(name))}`).toBe(
        `${name} invokes: false`,
      );
      expect(`${name} reaches __TAURI__: ${/__TAURI__/.test(read(name))}`).toBe(
        `${name} reaches __TAURI__: false`,
      );
    }
  });

  test('only app.js reads the engine answer', () => {
    // The argv goes out through app.js and the JSON comes back through it.
    // Without this, a surface could parse the engine's stdout itself and the
    // one-place contract would be true of flags but not of answers.
    for (const name of jsFiles()) {
      if (name === 'app.js') continue;
      expect(`${name} parses JSON: ${/JSON\s*\.\s*parse/.test(read(name))}`).toBe(
        `${name} parses JSON: false`,
      );
    }
  });

  test('only app.js knows an engine flag', () => {
    for (const name of jsFiles()) {
      // notes.js is GENERATED from docs/releases/*.md and is prose, not code:
      // it names no flag today, and if a release note ever quotes one, the
      // fix is the note, not this window's boundary. Every other carve-out in
      // these suites says why; this one used to be the exception.
      if (name === 'app.js' || name === 'notes.js') continue;
      // Any quoting. Mutation-confirmed: single-quotes-only let both
      // `const M1 = "--force";` and a backtick `--options-json` through.
      const flags = [...read(name).matchAll(/['"`](--[a-z][a-z-]*)['"`]/g)].map((m) => m[1]);
      expect(`${name} names flags: ${flags.join(', ')}`).toBe(`${name} names flags: `);
    }
  });

  test('every argv builder passes --json, and every one of them is checked', async () => {
    // Run the builders rather than reading their source. The brief's version
    // of this test filtered app.js's LINES for `=>` and `[` on the same line;
    // against the builders as actually written that selected three of seven
    // (`convert`, `reconvert`, `send` and `export` all break the line between
    // the arrow and the bracket), so four builders could drop --json
    // unnoticed. Calling them cannot miss one.
    const { argv } = await import(join(UI, 'app.js'));
    const built: Record<string, string[]> = {
      version: argv.version(),
      convert: argv.convert('/s/script.pdf'),
      convertForced: argv.convert('/s/script.pdf', { force: true, optionsJson: '{"a":1}' }),
      reconvert: argv.reconvert('/s/script.fountain', '/s/script.epub', '{"a":1}'),
      devices: argv.devices(),
      devicesQuick: argv.devices({ quick: true }),
      eject: argv.eject('/Volumes/Kindle'),
      exportOut: argv.export('/s/script.epub', { forFormat: 'kfx', fountain: '/s/script.fountain', optionsJson: '{"a":1}', out: '/s/out.kfx' }),
      readerDevice: argv.readerDevice('kobo'),
      send: argv.send('/s/script.epub'),
      sendTo: argv.send('/s/script.epub', 'kindle-1'),
      settings: argv.settings('/s/script.fountain'),
      settingsSet: argv.settings('/s/script.fountain', '{"a":1}'),
      export: argv.export('/s/script.epub', { forFormat: 'kfx' }),
      exportFull: argv.export('/s/script.epub', {
        forFormat: 'azw3', fountain: '/s/script.fountain', optionsJson: '{"a":1}',
      }),
      kfxStatus: argv.kfxStatus(),
      kfxInstall: argv.kfxInstall(),
      appSettings: argv.appSettings(),
      appSettingsSet: argv.appSettings('{"libraryPath":"/abs"}'),
      reveal: argv.reveal('/s/script.epub'),
      routes: argv.routes('/s/script.epub'),
      routesQuick: argv.routes('/s/script.epub', { quick: true }),
      kindleCheck: argv.kindleCheck('/s/script.epub'),
      route: argv.route('apple-books', '/s/script.epub'),
      routeSaveEpub: argv.route('save-epub', '/s/script.epub', { out: '/s/out.epub' }),
      routeSaveKindle: argv.route('save-kindle', '/s/script.epub', {
        out: '/s/out.azw3', fountain: '/s/script.fountain', optionsJson: '{"a":1}',
      }),
      emailSetup: argv.emailSetup(),
    };
    // Every builder the interface promises is exercised above.
    expect(Object.keys(argv).sort()).toEqual(
      ['appSettings', 'convert', 'devices', 'eject', 'emailSetup', 'export', 'kfxInstall', 'kfxStatus',
        'kindleCheck', 'readerDevice', 'reconvert', 'reveal', 'route', 'routes', 'send', 'settings',
        'version'].sort(),
    );
    for (const [name, args] of Object.entries(built)) {
      expect(`${name} has --json: ${args.includes('--json')}`).toBe(`${name} has --json: true`);
      // A `null` left in by a dropped .filter() reaches the Rust as the
      // string "null" and the engine reads it as a positional argument.
      expect(`${name} args: ${args.filter((a) => typeof a !== 'string').join(',')}`).toBe(
        `${name} args: `,
      );
    }
  });

  test('the send panel’s new calls are spelled once, here', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.devices()).toEqual(['devices', '--json']);
    expect(argv.devices({ quick: true })).toEqual(['devices', '--json', '--quick']);
    expect(argv.eject('/Volumes/Kindle')).toEqual(['eject', '/Volumes/Kindle', '--json']);
    expect(argv.export('/s/b.epub', { forFormat: 'fountain', fountain: '/s/b.fountain', out: '/o/b.fountain' }))
      .toEqual(['export', '/s/b.epub', '--json', '--for', 'fountain', '--fountain', '/s/b.fountain', '--out', '/o/b.fountain']);
    // No --out unless asked: the build-ahead and the device send keep their shape.
    expect(argv.export('/s/b.epub', { forFormat: 'kindle' })).not.toContain('--out');
    expect(argv.readerDevice('kobo')).toEqual(['app-settings', '--json', '--set', '{"readerDevice":"kobo"}']);
    expect(argv.readerDevice(null)).toEqual(['app-settings', '--json', '--set', '{"readerDevice":null}']);
  });

  test('the Kindle file check is export for kindle with --check, and nothing else', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.kindleCheck('/s/script.epub'))
      .toEqual(['export', '/s/script.epub', '--json', '--for', 'kindle', '--check']);
  });

  test('the KFX builders are exactly the two verbs, with nothing else on them', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.kfxStatus()).toEqual(['kfx-status', '--json']);
    expect(argv.kfxInstall()).toEqual(['kfx-install', '--json']);
  });

  test('the gear’s two builders: a bare read, a write that carries --set, and a plain reveal', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.appSettings()).toEqual(['app-settings', '--json']);
    expect(argv.appSettings('{"libraryPath":"/abs"}')).toEqual(
      ['app-settings', '--json', '--set', '{"libraryPath":"/abs"}'],
    );
    expect(argv.reveal('/s/script.epub')).toEqual(['reveal', '/s/script.epub', '--json']);
  });

  test('the route builders, pinned whole: routes never carries an out/fountain/options triple, route always may', async () => {
    // Same reasoning as the "pinned WHOLE" convert assertion below: this argv
    // is the entire contract with the engine, and an extra or missing element
    // is exactly what a wrong edit leaves behind.
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.routes('/s/script.epub')).toEqual(['routes', '/s/script.epub', '--json']);
    expect(argv.routes('/s/script.epub', { quick: true }))
      .toEqual(['routes', '/s/script.epub', '--json', '--quick']);

    expect(argv.route('apple-books', '/s/script.epub')).toEqual(
      ['route', 'apple-books', '/s/script.epub', '--json'],
    );
    expect(argv.route('save-epub', '/s/script.epub', { out: '/s/out.epub' })).toEqual(
      ['route', 'save-epub', '/s/script.epub', '--json', '--out', '/s/out.epub'],
    );
    expect(argv.route('save-kindle', '/s/script.epub', {
      out: '/s/out.azw3', fountain: '/s/script.fountain', optionsJson: '{"a":1}',
    })).toEqual([
      'route', 'save-kindle', '/s/script.epub', '--json',
      '--out', '/s/out.azw3', '--fountain', '/s/script.fountain', '--options-json', '{"a":1}',
    ]);
  });

  test('the email setup builder is the one engine call behind "Open Amazon’s page", with no book on it', async () => {
    // The send panel's email row offers Amazon's Personal Document Settings
    // page (where the approved-sender list lives). The ENGINE opens it, by a
    // key, so the window names no URL and needs no opener grant for it; the
    // page has nothing to do with this script, so no book path rides along.
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.emailSetup()).toEqual(['route', 'kindle-email-setup', '--json']);
  });

  test('an optional flag brings its value and an absent one brings nothing', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    const after = (args: string[], flag: string) => args[args.indexOf(flag) + 1];

    const plain = argv.convert('/s/script.pdf');
    // Pinned WHOLE, not probed flag by flag: this argv is the entire contract
    // between the window and the engine, and an extra or missing element is
    // exactly what a wrong edit leaves behind.
    expect(plain).toEqual(
      ['/s/script.pdf', '--json', '--progress', '--preview-inline', '--library'],
    );
    expect(plain[0]).toBe('/s/script.pdf');
    // The reader cannot read files, so the document must ride in the answer,
    // and the progress lines must be asked for or the bar never moves.
    expect(plain).toContain('--preview-inline');
    expect(plain).toContain('--progress');
    expect(plain).not.toContain('--force');
    expect(plain).not.toContain('--options-json');

    const forced = argv.convert('/s/script.pdf', { force: true, optionsJson: '{"a":1}' });
    expect(forced).toContain('--force');
    expect(after(forced, '--options-json')).toBe('{"a":1}');

    expect(argv.send('/s/script.epub')).not.toContain('--device');
    expect(after(argv.send('/s/script.epub', 'kindle-1'), '--device')).toBe('kindle-1');

    expect(argv.settings('/s/x.fountain')).not.toContain('--set');
    expect(after(argv.settings('/s/x.fountain', '{"a":1}'), '--set')).toBe('{"a":1}');

    // A re-render writes the library EPUB back in place, so what gets sent
    // stays what was previewed. It names that path with -o, so it must NOT
    // also ask for --library: the engine refuses the pair rather than picking
    // a winner, and a re-render that did both would fail every time.
    const again = argv.reconvert('/s/x.fountain', '/s/x.epub', '{}');
    expect(after(again, '-o')).toBe('/s/x.epub');
    expect(again).not.toContain('--library');

    const exported = argv.export('/s/x.epub', {
      forFormat: 'azw3', fountain: '/s/x.fountain', optionsJson: '{}',
    });
    expect(after(exported, '--for')).toBe('azw3');
    expect(after(exported, '--fountain')).toBe('/s/x.fountain');
    expect(argv.export('/s/x.epub', { forFormat: 'kfx' })).not.toContain('--fountain');
  });

  test('no surface sets innerHTML with anything but its own constant', () => {
    // Titles, device names and engine messages are all data from outside.
    // frame.js is the one exemption: it injects its own SVG symbol constant.
    for (const name of jsFiles()) {
      if (name === 'frame.js' || name === 'notes.js' || name === 'notes-surface.js') continue;
      expect(`${name} sets innerHTML: ${/innerHTML/.test(read(name))}`).toBe(
        `${name} sets innerHTML: false`,
      );
    }
  });
});

describe('the Convert surface', () => {
  // The real convert.js, mounted on the fake page (tests/helpers/fake-dom.ts)
  // and driven the way a reader drives it: a file goes in, the engine answers,
  // buttons are clicked. Each test imports its own copy of the module, so the
  // drawing state one test leaves (a book it converted, a stylesheet it
  // adopted) cannot leak into the next.
  const FOLDER = '/home/ann/Books';
  const library = (overrides: Record<string, unknown> = {}) => ({
    ok: true,
    library: { path: FOLDER, chosen: null, platformDefault: FOLDER, fromEnv: false, ...overrides },
    home: '/home/ann',
  });
  const converted = (pdf: string) => {
    const name = pdf.replace(/^.*\//, '').replace(/\.pdf$/, '');
    return {
      ok: true, title: name, author: 'A. Writer', pages: 18, scenes: 12, characters: 6, warnings: [],
      epubPath: `/lib/${name}/${name}.epub`, fountainPath: `/lib/${name}/${name}.fountain`, previewHtml: '',
    };
  };
  const refused = (code: string, message: string) => ({ ok: false, error: { code, message } });

  let page: FakePage | null = null;
  let copies = 0;
  afterEach(async () => { await page?.close(); page = null; });

  /** Mounts a fresh convert.js. `engine` answers by verb (a PDF path is a
   *  conversion); anything it leaves out is held for the test to answer. */
  async function mountConvert(options: {
    engine?: (args: string[]) => unknown;
    platform?: string;
    script?: Record<string, unknown> | null;
    ask?: { should: () => boolean; answer: (on: boolean) => void };
    state?: Record<string, unknown>;
  } = {}) {
    page = fakePage({ respond: options.engine ?? (() => HOLD), platform: options.platform });
    copies += 1;
    const convert = await import(`${join(UI, 'convert.js')}?mounted-${copies}`);
    const pane = page.pane();
    pane.id = 'surface-convert';
    const went: string[] = [];
    let restored = 0;
    const ctx = {
      state: { script: options.script ?? null, reducedMotion: true, ...options.state } as Record<string, any>,
      goTo: (id: string) => went.push(id),
      restoreFocus: () => { restored += 1; },
      scriptChanged: () => {},
      convertAgain: () => {},
      updates: options.ask === undefined
        ? undefined
        : { shouldAsk: options.ask.should, answer: options.ask.answer },
    };
    convert.mount(pane, ctx);
    await settle();
    return { convert, pane, ctx, went, restored: () => restored, tauri: page.tauri, doc: page.doc };
  }
  const slot = (pane: FakeNode) => pane.querySelectorAll('.well-library');

  test('the idle well names the app, says what the engine needs, and spells the shortcut the platform’s way', async () => {
    for (const platform of ['MacIntel', 'Win32', 'Linux x86_64']) {
      const { convert, pane } = await mountConvert({ platform });
      expect(pane.dataset.state).toBe('idle');
      expect(pane.querySelector('h1.wordmark')!.textContent).toBe(convert.WORDMARK);
      // Before the drop, not after the wait: a scan is the one guard a
      // reader can check at a glance.
      expect(pane.querySelector('.well .well-limits')!.textContent).toBe(convert.WELL.limits);
      const choose = pane.querySelector('.well button')!;
      expect(choose.textContent).toContain(convert.shortcutLabel(platform));
      await page!.close();
      page = null;
    }
  });

  test('a refusal shows the engine’s own sentence under a heading that names the cause, and keeps the open script', async () => {
    const open = { title: 'Field Station', path: '/s/Field Station.pdf' };
    const said = 'This PDF is <locked> & "sealed" by its owner.';
    const { convert, pane, ctx } = await mountConvert({
      engine: (args) => (args[0].endsWith('.pdf') ? refused('password', said) : HOLD),
      script: open,
    });
    await convert.convertPath('/s/Locked.pdf');
    expect(pane.dataset.state).toBe('failed');
    expect(pane.querySelector('h2.fault')!.textContent).toBe(convert.HEADINGS.password);
    // Verbatim, markup-looking characters and all: it is text, not HTML.
    expect(pane.querySelector('.fault-body')!.textContent).toBe(said);
    // The code is a labelled handle, on the screen and on the pane.
    expect(pane.querySelector('.code-note .code-note-label')!.textContent).toBe('Error code');
    expect(pane.querySelector('.code-note .code-chip')!.textContent).toBe('password');
    expect(pane.dataset.errorCode).toBe('password');
    // The book that was already open is still the open book, and the
    // refusal says so.
    expect(ctx.state.script).toBe(open);
    expect(pane.querySelector('.kept-note')!.textContent).toBe(convert.stillOpenNote(open));
    // A locked file cannot be overruled, so no override is offered.
    expect(pane.buttons('Convert anyway')).toEqual([]);
    expect(pane.buttons('Back to one').length).toBe(1);
  });

  test('a refusal with no script open says nothing about one, and never prints a stray "null"', async () => {
    const { convert, pane } = await mountConvert({
      engine: (args) => (args[0].endsWith('.pdf') ? refused('scanned', 'No text layer.') : HOLD),
    });
    await convert.convertPath('/s/Scan.pdf');
    expect(pane.querySelector('.kept-note')).toBeNull();
    expect(pane.textContent).not.toContain('null');
    expect(pane.textContent).not.toContain('undefined');
  });

  test('only a not-screenplay refusal offers Convert anyway, and it converts the same file with the override', async () => {
    for (const code of ['scanned', 'password', 'unreadable', 'unsupported-type', 'internal']) {
      const { convert, pane } = await mountConvert({
        engine: (args) => (args[0].endsWith('.pdf') ? refused(code, 'No.') : HOLD),
      });
      await convert.convertPath(`/s/${code}.pdf`);
      expect(`${code}: ${pane.buttons('Convert anyway').length}`).toBe(`${code}: 0`);
      await page!.close();
      page = null;
    }
    const { convert, pane, tauri } = await mountConvert({
      engine: (args) => {
        if (!args[0].endsWith('.pdf')) return HOLD;
        return args.includes('--force')
          ? converted(args[0])
          : refused('not-screenplay', 'No scene headings found. Pass --force to convert it anyway.');
      },
    });
    await convert.convertPath('/s/Treatment.pdf');
    // The sentence telling a CLI user to type the flag comes out, because
    // the button below it already does that; the diagnosis stays.
    expect(pane.querySelector('.fault-body')!.textContent).toBe('No scene headings found.');
    await pane.button('Convert anyway').click();
    await settle();
    const conversions = tauri.calls.filter((c) => c.args[0] === '/s/Treatment.pdf');
    expect(conversions.map((c) => c.args.includes('--force'))).toEqual([false, true]);
    expect(pane.dataset.state).toBe('done');
  });

  test('the bar follows the engine’s own lines, several to a chunk, ignores noise and never walks back', async () => {
    const { convert, pane, tauri, doc } = await mountConvert();
    const done = convert.convertPath('/s/Field Station.pdf');
    await settle();
    const readOut = () => pane.querySelector('.read-out')!.textContent;
    // The bar's width goes through an adopted stylesheet, because the
    // window's CSP refuses an inline style.
    const fill = () => (doc.adoptedStyleSheets.at(-1) as { text: string }).text;
    expect(readOut()).toBe(convert.PROGRESS_START.label);
    expect(fill()).toBe('.fill { width: 0%; }');
    // One chunk from the OS: a stray warning, a whole line, and the start
    // of the next. Nothing throws, and the whole line is drawn.
    tauri.emit('engine-line', 'warning: a font fell back\n{"progress":{"stage":"parse","percent":40}}\n{"progress":');
    expect(readOut()).toBe('reading the pages (40%)');
    expect(fill()).toBe('.fill { width: 40%; }');
    // A late line from earlier in the run does not move the bar back.
    tauri.emit('engine-line', '{"progress":{"stage":"parse","percent":20}}');
    expect(readOut()).toBe('reading the pages (40%)');
    tauri.emit('engine-line', '{"progress":{"stage":"render","percent":85}}');
    expect(readOut()).toBe('building the book (85%)');
    expect(fill()).toBe('.fill { width: 85%; }');
    tauri.calls.find((c) => c.args[0] === '/s/Field Station.pdf')!.answer(converted('/s/Field Station.pdf'));
    await done;
    await settle();
    // And it stops listening once the conversion is over.
    expect(tauri.listening('engine-line')).toBe(0);
  });

  test('the result is a page and a panel: "<Title> is ready", one line of counts, the panel, three quiet links', async () => {
    const { convert, pane } = await mountConvert({ engine: (a) => (a[0].endsWith('.pdf') ? converted(a[0]) : HOLD) });
    await convert.convertPath('/s/Field Station.pdf');
    expect(pane.dataset.state).toBe('done');
    expect(pane.querySelector('.result .result-page')).not.toBeNull();
    expect(pane.querySelector('.result-title')!.textContent).toBe('Field Station is ready');
    expect(pane.querySelector('.result .book-count')!.textContent).toBe('18 pages, 12 scenes');
    expect(pane.querySelector('.result .send-panel')).not.toBeNull();
    expect(pane.querySelectorAll('.result .asides button').map((b) => b.textContent)).toEqual(['Read it', 'Settings', 'Convert another']);
    expect(pane.querySelectorAll('button').some((b) => b.textContent === 'Send to a reader')).toBe(false);
    // The "(by author)" line went; the engine's warnings stay under the counts.
    expect(pane.textContent).not.toContain('A. Writer');
  });

  test('the result’s page is the book’s own first page, in a frame nothing inside can run in', async () => {
    const { convert, pane } = await mountConvert({
      engine: (a) => (a[0].endsWith('.pdf')
        ? { ...converted(a[0]), previewHtml: '<?xml version="1.0"?><html><head><style>p{}</style></head><body><section class="scene"><p>INT. FIELD STATION</p></section></body></html>' }
        : HOLD),
    });
    await convert.convertPath('/s/Field Station.pdf');
    const frame = pane.querySelector('.result-page iframe.result-page-frame')!;
    expect(frame.getAttribute('sandbox')).toBe('allow-same-origin');
    expect(frame.getAttribute('tabindex')).toBe('-1');
    expect(pane.querySelector('.result-page')!.getAttribute('aria-hidden')).toBe('true');
    // The stylesheet comes out (the CSP will not run it), the page goes in.
    const doc = frame.getAttribute('srcdoc')!;
    expect(doc).toStartWith('<!doctype html>');
    expect(doc).toContain('INT. FIELD STATION');
    expect(doc).not.toContain('<style>');
  });

  test('showResult draws the open book’s result again after Convert another, without converting', async () => {
    const { convert, pane, tauri } = await mountConvert({ engine: (a) => (a[0].endsWith('.pdf') ? converted(a[0]) : HOLD) });
    await convert.convertPath('/s/Field Station.pdf');
    pane.button('Convert another').click();
    expect(pane.dataset.state).toBe('idle');
    convert.showResult();
    expect(pane.dataset.state).toBe('done');
    expect(pane.querySelector('.result-title')!.textContent).toBe('Field Station is ready');
    expect(tauri.calls.filter((c) => c.args[0].endsWith('.pdf')).length).toBe(1);
  });

  test('showResult on the result already up redraws nothing, so a copy in progress keeps its buttons', async () => {
    const { convert, pane } = await mountConvert({ engine: (a) => (a[0].endsWith('.pdf') ? converted(a[0]) : HOLD) });
    await convert.convertPath('/s/Field Station.pdf');
    const panel = pane.querySelector('.send-panel');
    const another = pane.button('Convert another');
    convert.showResult();
    expect(pane.querySelector('.send-panel')).toBe(panel);
    expect(pane.button('Convert another')).toBe(another);
  });

  test('showResult with no book open, or mid-conversion, leaves the page alone', async () => {
    const { convert, pane } = await mountConvert();
    convert.showResult();
    expect(pane.dataset.state).toBe('idle');
    const going = convert.convertPath('/s/Field Station.pdf');
    await settle();
    convert.showResult();
    expect(pane.dataset.state).toBe('working');
    page!.tauri.callsTo('/s/Field Station.pdf')[0].answer(refused('scanned', 'No text.'));
    await going;
  });

  test('Show in Finder asks the engine to show this book, and says only what went wrong', async () => {
    // It sits at the foot of Other ways' Save a file now, under the panel's
    // calm screen (a reader who chose a Kobo before).
    const { convert, pane, tauri } = await mountConvert({
      engine: (args) => (args[0].endsWith('.pdf') ? converted(args[0]) : HOLD),
      state: { readerDevice: 'kobo' },
    });
    await convert.convertPath('/s/Field Station.pdf');
    pane.button('Other ways').click();
    const show = pane.button(convert.revealLabel('MacIntel'));
    expect(show.closest('section.other-ways')).not.toBeNull();
    const note = () => pane.querySelector('.path-note')!.nextSibling!;

    show.click();
    const reveal = tauri.callsTo('reveal');
    expect(reveal.map((c) => c.args)).toEqual([['reveal', '/lib/Field Station/Field Station.epub', '--json']]);
    reveal[0].answer({ ok: true });
    await settle();
    expect(note().hidden).toBe(true);

    show.click();
    tauri.callsTo('reveal')[1].answer(refused('internal', 'No file manager answered.'));
    await settle();
    expect(note().hidden).toBe(false);
    expect(note().textContent).toBe('No file manager answered.');

    show.click();
    tauri.callsTo('reveal')[2].fail('the engine could not be started');
    await settle();
    expect(note().textContent).toBe('the engine could not be started');
  });

  test('a reveal answer that arrives after the screen moved on lands nowhere', async () => {
    const { convert, pane, tauri } = await mountConvert({
      engine: (args) => (args[0].endsWith('.pdf') ? converted(args[0]) : HOLD),
      state: { readerDevice: 'kobo' },
    });
    await convert.convertPath('/s/First.pdf');
    pane.button(convert.revealLabel('MacIntel')).click();
    // Another script converts while the first reveal is still out.
    await convert.convertPath('/s/Second.pdf');
    expect(pane.querySelector('.result-title')!.textContent).toBe('Second is ready');
    tauri.callsTo('reveal')[0].answer(refused('internal', 'Could not show First.'));
    await settle();
    expect(pane.visibleText).not.toContain('Could not show First.');
  });

  test('Convert another goes back to the drop well and keeps the open script', async () => {
    const { convert, pane, ctx, tauri } = await mountConvert({
      engine: (args) => (args[0].endsWith('.pdf') ? converted(args[0]) : HOLD),
    });
    await convert.convertPath('/s/Field Station.pdf');
    const open = ctx.state.script;
    expect(open).not.toBeNull();
    pane.button('Convert another').click();
    await settle();
    expect(pane.dataset.state).toBe('idle');
    // Home, not a file dialog.
    expect(tauri.dialogs).toEqual([]);
    expect(ctx.state.script).toBe(open);
  });

  test('a refusal’s Report a bug opens an issue carrying the code, with the home folder the probe learned folded away', async () => {
    const { convert, pane, tauri } = await mountConvert({
      engine: (args) => {
        if (args[0] === 'app-settings') return { ...library({ path: '/srv/people/ann/Books' }), home: '/srv/people/ann' };
        if (args[0].endsWith('.pdf')) return refused('library', 'Cannot write under /srv/people/ann');
        return HOLD;
      },
    });
    await convert.convertPath('/s/Field Station.pdf');
    await pane.button('Report a bug').click();
    expect(tauri.opened.length).toBe(1);
    const body = new URL(tauri.opened[0]).searchParams.get('body') ?? '';
    expect(body).toContain('library: Cannot write under ~');
    expect(body).not.toContain('people');
  });

  test('the update question sits after the well, only when asked for, and an answer takes it down', async () => {
    const answers: boolean[] = [];
    let should = false;
    const quiet = await mountConvert({ ask: { should: () => should, answer: (on) => answers.push(on) } });
    expect(quiet.pane.querySelector('.well-ask-question')).toBeNull();
    await page!.close();

    should = true;
    const { convert, pane, restored } = await mountConvert({
      ask: { should: () => should, answer: (on) => answers.push(on) },
      engine: (args) => (args[0] === 'app-settings' ? library() : HOLD),
    });
    const order = pane.children.map((n) => n.className);
    expect(order.slice(0, 3)).toEqual(['wordmark', 'well', 'well-ask']);
    // So Choose PDF, not the question, is the page's first focus stop.
    const { stopAfterDialog } = await import(join(UI, 'focus.js'));
    expect(stopAfterDialog('convert', (s: string) => page!.doc.querySelectorAll(s)))
      .toBe(pane.querySelector('.well button'));
    const before = restored();
    pane.button(convert.ASK.yes).click();
    expect(answers).toEqual([true]);
    expect(pane.querySelector('.well-ask-question')).toBeNull();
    // The button that had the keyboard is gone, so it is handed back.
    expect(restored()).toBe(before + 1);
  });

  test('a stale answer to the question does not overwrite one given in the release notes meanwhile', async () => {
    const answers: boolean[] = [];
    let should = true;
    const { convert, pane } = await mountConvert({ ask: { should: () => should, answer: (on) => answers.push(on) } });
    // The switch in the release notes answered while this line was up.
    should = false;
    pane.button(convert.ASK.no).click();
    expect(answers).toEqual([]);
    expect(pane.querySelector('.well-ask-question')).toBeNull();
  });

  test('the library line is one slot after the question, filled by a probe that only reads', async () => {
    const { convert, pane, tauri, restored } = await mountConvert({
      ask: { should: () => true, answer: () => {} },
      engine: (args) => (args[0] === 'app-settings' ? library() : HOLD),
    });
    // Read-only: a probe that sent a --set would move the reader's folder
    // every time the well was redrawn.
    expect(tauri.callsTo('app-settings').map((c) => c.args)).toEqual([['app-settings', '--json']]);
    const [line, secondary] = slot(pane);
    expect(pane.children.indexOf(line)).toBe(pane.children.indexOf(pane.querySelector('.well-ask')!) + 1);
    // It reuses the question's look (well-ask) and has no rule of its own:
    // the capture tool hides the slot by its well-library name.
    expect(line.className).toBe('well-ask well-library');
    expect(cssFiles().map(read).join('\n')).not.toContain('.well-library');
    expect(line.hidden).toBe(false);
    expect(line.textContent).toBe(`${convert.libraryLine({ path: FOLDER, home: '/home/ann', fromEnv: false })}Change…`);
    expect(secondary.hidden).toBe(true);
    // Coming back to the page does not ask again; it hands the keyboard back.
    const before = restored();
    convert.show();
    await settle();
    expect(tauri.callsTo('app-settings').length).toBe(1);
    expect(restored()).toBe(before + 1);
  });

  test('a probe that fails, refuses or answers malformed leaves the slot empty', async () => {
    const answers: Array<(call: EngineCall) => void> = [
      (call) => call.fail('engine missing'),
      (call) => call.answer({ ok: false, error: { code: 'internal', message: 'no' } }),
      (call) => call.answer({ ok: true, library: { path: FOLDER }, home: '/home/ann' }),
      (call) => call.answer('not json'),
    ];
    for (const settleProbe of answers) {
      const { pane, tauri } = await mountConvert();
      settleProbe(tauri.callsTo('app-settings')[0]);
      await settle();
      expect(slot(pane).map((p) => [p.hidden, p.textContent])).toEqual([[true, ''], [true, '']]);
      await page!.close();
      page = null;
    }
  });

  test('a probe that answers after the well was redrawn is dropped', async () => {
    const { convert, pane, tauri } = await mountConvert();
    convert.reset();
    await settle();
    const [first, second] = tauri.callsTo('app-settings');
    first.answer(library({ path: '/home/ann/Old Books' }));
    await settle();
    expect(pane.textContent).not.toContain('Old Books');
    second.answer(library());
    await settle();
    expect(slot(pane)[0].textContent).toContain('~/Books');
  });

  test('Change… quiets both buttons while the folder dialog is up, starts it at the library, and a cancel or the same folder sends nothing', async () => {
    const { convert, pane, tauri } = await mountConvert({
      engine: (args) => (args[0] === 'app-settings' && !args.includes('--set') ? library({ chosen: FOLDER }) : HOLD),
    });
    const both = () => slot(pane)[0].buttons().map((b) => `${b.textContent}:${b.disabled ? 'off' : 'on'}`);
    expect(both()).toEqual(['Change…:on', 'Reset:on']);
    const writes = () => tauri.callsTo('app-settings').filter((c) => c.args.includes('--set'));

    pane.button('Change…').click();
    await settle();
    expect(tauri.dialogs.map((d) => [d.kind, d.options])).toEqual([['open', { directory: true, defaultPath: FOLDER }]]);
    expect(both()).toEqual(['Change…:off', 'Reset:off']);
    tauri.dialogs.shift()!.resolve(null);
    await settle();
    expect(both()).toEqual(['Change…:on', 'Reset:on']);

    pane.button('Change…').click();
    await settle();
    tauri.dialogs.shift()!.resolve(FOLDER);
    await settle();
    expect(writes()).toEqual([]);

    pane.button('Change…').click();
    await settle();
    tauri.dialogs.shift()!.resolve('/home/ann/Scripts');
    await settle();
    expect(writes().map((c) => c.args)).toEqual([
      ['app-settings', '--json', '--set', '{"libraryPath":"/home/ann/Scripts"}'],
    ]);
    writes()[0].answer(library({ path: '/home/ann/Scripts', chosen: '/home/ann/Scripts' }));
    await settle();
    const [line, secondary] = slot(pane);
    expect(line.textContent).toContain('~/Scripts');
    expect(secondary.hidden).toBe(false);
    expect(secondary.textContent).toBe(convert.movedLine);
  });

  test('a refusal or a crash from Change replaces only the line under it', async () => {
    const { pane, tauri } = await mountConvert({
      engine: (args) => (args[0] === 'app-settings' && !args.includes('--set') ? library() : HOLD),
    });
    const change = async (finish: (call: EngineCall) => void) => {
      pane.button('Change…').click();
      await settle();
      tauri.dialogs.shift()!.resolve('/home/ann/Elsewhere');
      await settle();
      finish(tauri.callsTo('app-settings').filter((c) => c.args.includes('--set')).at(-1)!);
      await settle();
    };
    const [line, secondary] = slot(pane);
    const before = line.textContent;

    await change((call) => call.answer({ ok: false, error: { code: 'library', message: 'That folder is read-only.' } }));
    expect(line.textContent).toBe(before);
    expect(pane.button('Change…').disabled).toBe(false);
    expect([secondary.hidden, secondary.textContent]).toEqual([false, 'That folder is read-only.']);

    await change((call) => call.fail('the engine stopped'));
    expect(line.textContent).toBe(before);
    expect(secondary.textContent).toBe('the engine stopped');
  });

  test('Reset sends the default back without asking for a folder, and claims no move', async () => {
    const { pane, tauri } = await mountConvert({
      engine: (args) => (args[0] === 'app-settings' && !args.includes('--set') ? library({ chosen: '/home/ann/Picked' }) : HOLD),
    });
    pane.button('Reset').click();
    await settle();
    expect(tauri.dialogs).toEqual([]);
    const write = tauri.callsTo('app-settings').filter((c) => c.args.includes('--set'));
    expect(write.map((c) => c.args)).toEqual([['app-settings', '--json', '--set', '{"libraryPath":null}']]);
    write[0].answer(library());
    await settle();
    expect(slot(pane)[0].buttons().map((b) => b.textContent)).toEqual(['Change…']);
    expect(slot(pane)[1].hidden).toBe(true);
  });
});

describe('converting a PDF again takes its turn on the book it rewrites', () => {
  // A conversion writes the script's library EPUB afresh, the same file the
  // Settings page's save rebuilds and the send panel copies, so it
  // takes its turn on that book (book-queue.js). The window never works out
  // a library path itself: it knows a PDF's book from the engine's answer
  // the last time it converted it, and a PDF it has not converted has no
  // book anything here can be holding, so that one converts at once.
  //
  // The real convert.js on the fake page (tests/helpers/fake-dom.ts), with
  // every engine call held for the test to answer: the result it draws now
  // mounts the send panel, which needs a page that behaves like one.
  let page: FakePage | null = null;
  afterEach(async () => { await page?.close(); page = null; });
  async function mounted(tag: string) {
    page = fakePage();
    const convert = await import(`${join(UI, 'convert.js')}?${tag}`);
    const pane = page.pane();
    pane.id = 'surface-convert';
    convert.mount(pane, {
      state: { script: null as unknown }, goTo: () => {}, restoreFocus: () => {}, scriptChanged: () => {}, convertAgain: () => {},
    });
    return { convert, pane, tauri: page.tauri };
  }
  const pdfCalls = (tauri: FakePage['tauri'], pdf?: string) =>
    tauri.pending().filter((c) => c.args[1] === '--json' && c.args[0].endsWith('.pdf') && (pdf === undefined || c.args[0] === pdf));

  test('a PDF converted before waits for whatever holds its book; one never converted does not', async () => {
    const { convert, tauri } = await mounted('turns');
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const answer = async (pdf: string, epubPath: string) => {
      pdfCalls(tauri, pdf)[0].answer({ ok: true, title: 'Field Station', epubPath, fountainPath: epubPath.replace(/epub$/, 'fountain'), previewHtml: '' });
      await settle();
    };
    const BOOK = '/lib/field-station/Field Station.epub';

    // The first conversion: no book known, asked at once.
    const first = convert.convertPath('/s/Field Station.pdf');
    await settle();
    expect(pdfCalls(tauri).map((c) => c.args[0])).toEqual(['/s/Field Station.pdf']);
    await answer('/s/Field Station.pdf', BOOK);
    await first;

    // A send holds that book now.
    let free: () => void = () => {};
    const sending = inTurn(BOOK, 'send', () => new Promise<void>((resolve) => { free = resolve; }));

    // Another PDF has no book this window has touched: at once.
    const other = convert.convertPath('/s/Other.pdf');
    await settle();
    expect(pdfCalls(tauri).map((c) => c.args[0])).toEqual(['/s/Other.pdf']);
    await answer('/s/Other.pdf', '/lib/other/Other.epub');
    await other;

    // The same PDF again rewrites the book the send holds: it waits.
    const again = convert.convertPath('/s/Field Station.pdf');
    await settle();
    expect(pdfCalls(tauri)).toEqual([]);
    free();
    await sending;
    await settle();
    expect(pdfCalls(tauri).map((c) => c.args[0])).toEqual(['/s/Field Station.pdf']);
    await answer('/s/Field Station.pdf', BOOK);
    await again;
  });

  test('while it waits its turn the read-out says what for, then "starting up" once it starts', async () => {
    // A conversion that waited out a half-minute Kindle build used to sit on
    // "starting up" the whole time, which reads as a stuck engine.
    const { convert, pane, tauri } = await mounted('waiting');
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const readOut = (): string => pane.querySelector('.read-out')?.textContent ?? '(none)';
    const conversion = () => pdfCalls(tauri, '/s/Night Shift.pdf')[0];
    const BOOK = '/lib/night-shift/Night Shift.epub';
    const answer = async () => {
      conversion().answer({ ok: true, title: 'Night Shift', epubPath: BOOK, fountainPath: BOOK.replace(/epub$/, 'fountain'), previewHtml: '' });
      await settle();
    };

    // Free book: straight to "starting up".
    const first = convert.convertPath('/s/Night Shift.pdf');
    await settle();
    expect(readOut()).toBe(convert.PROGRESS_START.label);
    await answer();
    await first;

    // A Kindle build holds the book: the read-out names it until it is done.
    let free: () => void = () => {};
    const building = inTurn(BOOK, 'kindle', () => new Promise<void>((resolve) => { free = resolve; }));
    const again = convert.convertPath('/s/Night Shift.pdf');
    await settle();
    expect(conversion()).toBeUndefined();
    expect(readOut()).toBe('Waiting for the Kindle file to finish building…');
    free();
    await building;
    await settle();
    expect(conversion()).toBeDefined();
    expect(readOut()).toBe(convert.PROGRESS_START.label);
    await answer();
    await again;
  });

  test('waitingLabel names the turn a conversion waits for, the longest wait first', async () => {
    const { waitingLabel } = await import(join(UI, 'convert.js'));
    expect(waitingLabel([])).toBeNull();
    expect(waitingLabel(['check', 'kindle'])).toBe('Waiting for the Kindle file to finish building…');
    expect(waitingLabel(['send'])).toBe('Waiting for the send to finish…');
    expect(waitingLabel(['save'])).toBe('Waiting for the settings to finish saving…');
    expect(waitingLabel(['check'])).toBe('Waiting for the book to be free…');
  });
});

describe('"Convert it again" converts the script again', () => {
  // Read's blank notice, Settings' fault screen and the send panel's "no book" line
  // each offer "Convert it again". It used to call goTo('convert') and
  // nothing else, which lands on the result screen the reader just left,
  // with no way to convert anything. Now it converts the open script's own
  // source, taking its book's turn, and shows the progress on Convert.
  class Node {
    kids: Node[] = [];
    parent: Node | null = null;
    attrs = new Map<string, string>();
    handlers = new Map<string, Array<(event: unknown) => unknown>>();
    dataset: Record<string, string> = {};
    className = '';
    hidden = false;
    disabled = false;
    text = '';
    constructor(readonly tag: string) {}
    append(...nodes: (Node | string | null | undefined)[]) {
      for (const n of nodes) {
        if (n == null) continue;
        const node = typeof n === 'string' ? Object.assign(new Node('#text'), { text: n }) : n;
        node.parent = this;
        this.kids.push(node);
      }
    }
    get firstChild() { return this.kids[0] ?? null; }
    removeChild(node: Node) { this.kids.splice(this.kids.indexOf(node), 1); node.parent = null; return node; }
    get textContent(): string { return this.tag === '#text' ? this.text : this.kids.map((k) => k.textContent).join(''); }
    set textContent(value: string) { this.kids = []; this.append(value); }
    setAttribute(name: string, value: string) { this.attrs.set(name, value); }
    getAttribute(name: string) { return this.attrs.get(name) ?? null; }
    addEventListener(type: string, fn: (event: unknown) => unknown) {
      this.handlers.set(type, [...(this.handlers.get(type) ?? []), fn]);
    }
    click() { for (const fn of this.handlers.get('click') ?? []) fn({ target: this, currentTarget: this }); }
    querySelector() { return null; }
    get classList() { return { toggle: () => true, contains: () => false, add: () => {}, remove: () => {} }; }
    buttons(): Node[] {
      return this.kids.flatMap((k) => (k.tag === 'button' ? [k, ...k.buttons()] : k.buttons()));
    }
  }
  const g = globalThis as unknown as Record<string, unknown>;
  const asked: string[][] = [];
  // Every engine call is held, then refused once the test is done: app.js
  // counts a running call toward "busy" for the whole file, so a call left
  // hanging here would hold off every later test's idle wait.
  const pending: Array<(stdout: string) => void> = [];
  beforeEach(() => {
    asked.length = 0;
    g.document = {
      createElement: (tag: string) => new Node(tag),
      createElementNS: (_ns: string, tag: string) => new Node(tag),
      createTextNode: (value: string) => Object.assign(new Node('#text'), { text: value }),
      adoptedStyleSheets: [],
    };
    g.CSSStyleSheet = class { replaceSync() {} };
    g.window = {
      __TAURI__: {
        core: {
          invoke: (_cmd: string, { args }: { args: string[] }) => {
            asked.push(args);
            return new Promise<string>((resolve) => pending.push(resolve));
          },
        },
        event: { listen: async () => () => {} },
      },
      addEventListener: () => {},
      removeEventListener: () => {},
    };
  });
  afterEach(async () => {
    for (const resolve of pending.splice(0)) resolve(JSON.stringify({ ok: false }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    delete g.window; delete g.document; delete g.CSSStyleSheet;
  });
  const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

  /** The surfaces' shared context, recording where it was sent. */
  function context(script: unknown) {
    const went: string[] = [];
    let again = 0;
    return {
      went,
      again: () => again,
      ctx: {
        state: { script, reducedMotion: true },
        goTo: (id: string) => went.push(id),
        restoreFocus: () => {},
        scriptChanged: () => {},
        convertAgain: () => { again += 1; },
      },
    };
  }
  const againButton = (pane: Node) => pane.buttons().find((b) => b.textContent === 'Convert it again');

  test('convertAgain converts the open script’s own source and goes to Convert to show it', async () => {
    const convert = await import(`${join(UI, 'convert.js')}?again-source`);
    const { ctx, went } = context({ path: '/s/Field Station.pdf', title: 'Field Station' });
    convert.mount(new Node('section'), ctx);
    went.length = 0;
    asked.length = 0;
    convert.convertAgain();
    await tick();
    expect(went).toEqual(['convert']);
    expect(asked.map((args) => args[0])).toEqual(['/s/Field Station.pdf']);
    expect(asked[0]).toContain('--json');
  });

  test('a script with no source path known only goes to Convert', async () => {
    const convert = await import(`${join(UI, 'convert.js')}?again-nopath`);
    const { ctx, went } = context({ path: null, title: 'Field Station' });
    convert.mount(new Node('section'), ctx);
    went.length = 0;
    asked.length = 0;
    convert.convertAgain();
    await tick();
    expect(went).toEqual(['convert']);
    expect(asked).toEqual([]);
  });

  test('Read’s blank notice converts again', async () => {
    const reader = await import(`${join(UI, 'read.js')}?again`);
    const { ctx, went, again } = context({ path: '/s/a.pdf', title: 'A', previewHtml: '' });
    const pane = new Node('section');
    reader.mount(pane, ctx);
    againButton(pane)!.click();
    expect(again()).toBe(1);
    expect(went).toEqual([]);
  });

  test('Settings’ fault screen converts again', async () => {
    const tune = await import(`${join(UI, 'tune.js')}?again`);
    const { ctx, again } = context({ path: '/s/a.pdf', title: 'A', fountainPath: null, epubPath: null, settings: null });
    const pane = new Node('section');
    tune.mount(pane, ctx);
    await tune.show();
    againButton(pane)!.click();
    expect(again()).toBe(1);
  });

  test('the send panel’s "no book to send" line converts again', async () => {
    // On the fake page: the panel needs a document that behaves like one.
    const fake = fakePage();
    try {
      const panel = await import(`${join(UI, 'send-panel.js')}?again`);
      const { ctx, again } = context({ path: '/s/a.pdf', title: 'A', epubPath: null });
      const pane = fake.pane();
      panel.mountPanel(pane, ctx);
      pane.button('Convert it again').click();
      expect(again()).toBe(1);
    } finally {
      await fake.close();
    }
  });
});

describe('what the Convert surface decides', () => {
  // The decisions, exercised directly. The drawing over them is thin by
  // design; these are the rules a wrong implementation would get wrong.
  // desktop/ui is plain .js with no declarations, so the module is imported
  // the same dynamic way the argv tests import app.js, and its decision
  // surface is spelled out here — which doubles as the list a later surface
  // may rely on.
  type Progress = { percent: number; stage: string | null; label: string };
  type ConvertModule = {
    HEADINGS: Record<string, string>;
    OVERRIDABLE: string;
    NO_MESSAGE: string;
    NO_LIBRARY_MESSAGE: string;
    PROGRESS_START: Progress;
    shortcutLabel: (platform: unknown) => string;
    failureFor: (error: unknown) => {
      code: string; heading: string; message: string; canForce: boolean;
    };
    nextProgress: (previous: Progress, line: unknown) => Progress;
    withoutCliRemedy: (message: unknown, flag: unknown) => string;
    stillOpenNote: (script: unknown) => string | null;
    fileName: (path: unknown) => string;
    countsLine: (script: unknown) => string;
    readyLine: (title: unknown) => string;
    scriptFrom: (path: string, answer: unknown) => Record<string, unknown>;
    droppedPath: (paths: unknown) => string | null;
    revealFailureMessage: (answer: unknown) => string | null;
    revealNote: (run: (args: string[]) => Promise<unknown>, path: string) => Promise<string>;
    libraryFrom: (answer: unknown) => {
      path: string; chosen: string | null; platformDefault: string; fromEnv: boolean; home: string;
    } | null;
    libraryLine: (library: {
      path: string; home: string; fromEnv: boolean;
    }) => string;
    libraryActions: (library: { chosen: string | null; fromEnv: boolean }) => string[];
    movedLine: string;
    libraryChangeArgs: (path: string | null) => string;
    libraryAfter: (answer: unknown) =>
      { ok: true; library: Record<string, unknown> } | { ok: false; message: string };
    showsMovedLine: (
      action: string, previousPath: string, library: { path: string },
    ) => boolean;
  };
  let convert: ConvertModule;

  beforeAll(async () => {
    convert = (await import(join(UI, 'convert.js'))) as ConvertModule;
  });

  test('the shortcut is spelled the platform’s way', () => {
    expect(convert.shortcutLabel('MacIntel')).toBe('⌘O');
    expect(convert.shortcutLabel('macOS')).toBe('⌘O');
    expect(convert.shortcutLabel('Linux aarch64')).toBe('Ctrl+O');
    expect(convert.shortcutLabel('Win32')).toBe('Ctrl+O');
    // navigator.userAgentData is absent on WebKitGTK, and navigator.platform
    // is deprecated: both can be undefined in the same window.
    expect(convert.shortcutLabel(undefined)).toBe('Ctrl+O');
  });

  test('every code the engine can refuse with gets a heading that names a cause', () => {
    const codes = ['scanned', 'not-screenplay', 'password', 'unreadable',
      'unsupported-type', 'bad-options', 'usage', 'unwritable', 'internal'];
    const headings = new Set<string>();
    for (const code of codes) {
      const { heading } = convert.failureFor({ code, message: 'x' });
      expect(`${code}: ${heading}`).not.toBe(`${code}: `);
      // A heading that is the same for every code names nothing.
      headings.add(heading);
    }
    expect(headings.size).toBeGreaterThanOrEqual(codes.length - 1);
    // The blank-heading failure this is here to prevent: a code no one
    // anticipated still renders a sentence, and is never overridable.
    const unknown = convert.failureFor({ code: 'moon-phase', message: 'x' });
    expect(unknown.heading).toBe(convert.HEADINGS.internal);
    expect(unknown.canForce).toBe(false);
    expect(unknown.code).toBe('moon-phase');
  });

  test('the engine’s sentence survives verbatim, whatever is in it', () => {
    // Not trimmed into a summary, not re-cased, not suffixed. The ONE edit
    // this window makes is the CLI remedy, and only where it has drawn the
    // button that remedy describes; every other refusal is word for word.
    for (const [code, said] of [
      ['unreadable', 'cannot read the input file (EACCES)'],
      ['scanned', 'This PDF has no selectable text — it looks like a scan.'],
      ['password', 'This PDF is password-protected.'],
      ['internal', 'the engine did not answer in JSON:\n<!DOCTYPE html>'],
    ] as const) {
      expect(`${code}: ${convert.failureFor({ code, message: said }).message}`)
        .toBe(`${code}: ${said}`);
    }
    // Including one that names the flag: the window draws no override for a
    // scan, so there is nothing here talking past the reader, and taking the
    // sentence out would delete the only instruction they have.
    const scanned = 'This PDF is a scan. Pass --force to convert it anyway.';
    expect(convert.failureFor({ code: 'scanned', message: scanned }).message).toBe(scanned);
  });

  test('the refusal stops telling a window user to type the flag it drew a button for',
    () => {
      // The engine's sentence is written for a terminal and is right there.
      // In the window it lands directly above a Convert anyway button that
      // does exactly what it asks for.
      const said = 'No scene headings and no dialogue found: this does not look like a '
        + 'screenplay. Pass --force to convert it anyway.';
      const shown = convert.failureFor({ code: 'not-screenplay', message: said });
      // Asserted against the value it CHANGED FROM, so a function that did
      // nothing at all cannot pass: the input carries the remedy, the output
      // does not, and what is left is the diagnosis, whole and unedited.
      expect(said).toContain('Pass --force');
      expect(shown.message).toBe(
        'No scene headings and no dialogue found: this does not look like a screenplay.',
      );
      expect(shown.canForce).toBe(true);
      // And the button is still there: the remedy did not go away, it moved.
      expect(convert.OVERRIDABLE).toBe('not-screenplay');
    });

  test('a failure with no sentence still says something, and says it once', () => {
    for (const broken of [{ code: 'scanned' }, { code: 'scanned', message: '   ' }, {}, undefined]) {
      const shown = convert.failureFor(broken as never);
      expect(shown.message).toBe(convert.NO_MESSAGE);
      expect(shown.heading.length).toBeGreaterThan(0);
    }
    // A missing code is the engine breaking its contract, not a file the
    // reader can overrule.
    expect(convert.failureFor({} as never).code).toBe('internal');
    expect(convert.failureFor({} as never).canForce).toBe(false);
  });

  test('only not-screenplay may be overruled', () => {
    expect(convert.failureFor({ code: 'not-screenplay', message: 'x' }).canForce).toBe(true);
    for (const code of ['scanned', 'password', 'unreadable', 'unsupported-type',
      'bad-options', 'usage', 'internal']) {
      expect(`${code} canForce: ${convert.failureFor({ code, message: 'x' }).canForce}`)
        .toBe(`${code} canForce: false`);
    }
  });

  test('the bar follows the engine\u2019s own percent, which is already the whole job', () => {
    // Measured against the engine, not assumed: a 3,601-page script emits
    // parse 6..85 and then render 85, render 100, because src/convert.ts
    // has already applied PARSE_SHARE. A window that re-weighted those
    // numbers would stall at 72% for the entire tail of the parse and then
    // leap to the end — which is exactly what the first draft of this
    // surface did, and what a live run caught.
    let at = convert.PROGRESS_START;
    const seen: number[] = [];
    for (const line of [
      { stage: 'parse', percent: 6 }, { stage: 'parse', percent: 44 },
      { stage: 'parse', percent: 85 }, { stage: 'render', percent: 85 },
      { stage: 'render', percent: 100 },
    ]) {
      at = convert.nextProgress(at, line);
      seen.push(at.percent);
    }
    expect(seen).toEqual([6, 44, 85, 85, 100]);
    // Every step forward or level, never back.
    for (let i = 1; i < seen.length; i += 1) expect(seen[i]).toBeGreaterThanOrEqual(seen[i - 1]);
  });

  test('the read-out names the stage the reader is waiting on', () => {
    expect(convert.PROGRESS_START.label).toBe('starting up');
    expect(convert.nextProgress(convert.PROGRESS_START, { stage: 'parse', percent: 40 }).label)
      .toBe('reading the pages (40%)');
    // Crossing into rendering is news even though the bar does not move: the
    // engine hands over at 85 on both sides of the boundary.
    const parsed = convert.nextProgress(convert.PROGRESS_START, { stage: 'parse', percent: 85 });
    expect(parsed.label).toBe('reading the pages (85%)');
    const rendering = convert.nextProgress(parsed, { stage: 'render', percent: 85 });
    expect(rendering.label).toBe('building the book (85%)');
    expect(rendering.percent).toBe(85);
  });

  test('a line that is late, unknown or malformed leaves the bar alone', () => {
    const half = convert.nextProgress(convert.PROGRESS_START, { stage: 'render', percent: 50 });
    for (const junk of [
      { stage: 'parse', percent: 20 },      // a parse line arriving after render began
      { stage: 'render', percent: 50 },     // the percent it is already showing
      { stage: 'polish', percent: 99 },     // a stage this window does not know
      { stage: 'render', percent: 'lots' }, // not a number
      { stage: 'render' }, {}, undefined, null,
    ]) {
      expect(convert.nextProgress(half, junk as never)).toBe(half);
    }
    // ...and a percent outside the contract is clamped, not drawn past the end.
    expect(convert.nextProgress(half, { stage: 'render', percent: 900 }).percent).toBe(100);
  });

  test('the percents the engine really emits drive the bar from end to end', async () => {
    // The other half of the pin above: the sequence is read off the engine
    // itself rather than typed out here, so a change to PARSE_SHARE or to
    // the throttle shows up as a failing test rather than as a stuck bar.
    const root = new URL('..', import.meta.url).pathname;
    const proc = Bun.spawn(
      ['bun', join(root, 'src', 'cli.ts'), join(root, 'tests', 'fixtures', 'screenplay.pdf'),
        '--json', '--progress', '-o', join(SCRATCH, 'convert-progress.epub')],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    const [stderr] = await Promise.all([new Response(proc.stderr).text(), proc.exited]);
    const lines = stderr.split('\n').map((l) => l.trim()).filter((l) => l.startsWith('{'))
      .map((l) => JSON.parse(l).progress).filter((p) => p);
    expect(lines.length).toBeGreaterThan(2);

    let at = convert.PROGRESS_START;
    const seen = lines.map((line: { stage: string; percent: number }) => {
      at = convert.nextProgress(at, line);
      return at.percent;
    });
    for (let i = 1; i < seen.length; i += 1) expect(seen[i]).toBeGreaterThanOrEqual(seen[i - 1]);
    // It starts somewhere near the beginning and finishes AT the end: a bar
    // that stopped at 72% would satisfy monotonicity and still be wrong.
    expect(seen[0]).toBeLessThan(50);
    expect(at.percent).toBe(100);
    expect(at.label).toBe('building the book (100%)');
  }, 60000);

  test('the counts are one line, singular included', () => {
    expect(convert.countsLine({ pages: 19, scenes: 11 })).toBe('19 pages, 11 scenes');
    expect(convert.countsLine({ pages: 1, scenes: 1 })).toBe('1 page, 1 scene');
    expect(convert.countsLine({ pages: 1 })).toBe('1 page');
    expect(convert.countsLine({ pages: 19, scenes: null })).toBe('19 pages');
    // The engine omits the counts entirely for a .fountain input; "undefined
    // pages" is worse than no line at all.
    expect(convert.countsLine({ title: 'x' })).toBe('');
    expect(convert.countsLine(null)).toBe('');
  });

  test('the result’s heading says the book is ready', () => {
    expect(convert.readyLine('Field Station')).toBe('Field Station is ready');
    expect(convert.readyLine('  Field Station ')).toBe('Field Station is ready');
    expect(convert.readyLine('')).toBe('The book is ready');
  });

  test('the script object tasks 9–11 read is complete whatever the engine sent', () => {
    const full = convert.scriptFrom('/s/The Script.pdf', {
      ok: true, title: 'The Last Video Store', author: 'A. N. Placeholder',
      pages: 5, scenes: 5, characters: 3, warnings: ['a dual-dialogue block was flattened'],
      epubPath: '/s/The Script.epub', fountainPath: '/s/The Script.fountain',
      previewHtml: '<p>x</p>', mobiPath: '/s/The Script.mobi',
    });
    expect(full).toEqual({
      path: '/s/The Script.pdf',
      title: 'The Last Video Store',
      author: 'A. N. Placeholder',
      pages: 5, scenes: 5, characters: 3,
      warnings: ['a dual-dialogue block was flattened'],
      epubPath: '/s/The Script.epub',
      fountainPath: '/s/The Script.fountain',
      previewHtml: '<p>x</p>',
      settings: null,
    });

    // A title-less script falls back to its filename rather than to nothing:
    // every later surface prints this.
    const bare = convert.scriptFrom('/s/untitled.pdf', { ok: true, epubPath: '/s/untitled.epub' });
    expect(bare.title).toBe('untitled.pdf');
    expect(bare.author).toBeNull();
    expect(bare.warnings).toEqual([]);
    expect(bare.previewHtml).toBe('');
    expect(bare.settings).toBeNull();
    // The same keys either way, so no surface has to test for a field.
    expect(Object.keys(bare).sort()).toEqual(Object.keys(full).sort());
  });

  test('one window converts one dropped script', () => {
    expect(convert.droppedPath(['/a/one.pdf', '/a/two.pdf'])).toBe('/a/one.pdf');
    expect(convert.droppedPath(['', '  ', '/a/real.pdf'])).toBe('/a/real.pdf');
    // Tauri sends drag-leave with no payload at all; the well must not try to
    // convert nothing.
    expect(convert.droppedPath([])).toBeNull();
    expect(convert.droppedPath(undefined as never)).toBeNull();
    expect(convert.droppedPath('/a/one.pdf' as never)).toBeNull();
  });

  test('Show in Finder says nothing on success and the engine’s own sentence on a refusal', () => {
    expect(convert.revealFailureMessage({ ok: true, revealed: '/x/a.epub' })).toBeNull();
    expect(convert.revealFailureMessage({
      ok: false,
      error: { code: 'reveal-failed', message: 'could not show /x/a.epub in the file manager' },
    })).toBe('could not show /x/a.epub in the file manager');
    // A contract-breaking refusal still says something a person can act on,
    // the same fallback failureFor uses everywhere else on this screen.
    expect(convert.revealFailureMessage({ ok: false, error: {} })).toBe(convert.NO_MESSAGE);
    expect(convert.revealFailureMessage(undefined)).toBeNull();
  });

  test('revealNote asks the engine through the injected runner, and decides what to say', async () => {
    const calls: unknown[] = [];
    const refused = async (args: unknown) => {
      calls.push(args);
      return {
        ok: false,
        error: { code: 'reveal-failed', message: 'could not show /x/a.epub in the file manager' },
      };
    };
    expect(await convert.revealNote(refused, '/x/a.epub')).toBe(
      'could not show /x/a.epub in the file manager',
    );
    // The path is handed to argv.reveal, exactly as runEngine expects it.
    expect(calls).toEqual([['reveal', '/x/a.epub', '--json']]);

    // A crash (Rust could not start the engine) and a non-JSON answer both
    // reach here as a REJECTED promise, the same shape runEngine throws for
    // either one, so both are covered by the same catch and shown the same
    // way: the thrown message, verbatim.
    const crashed = async () => { throw new Error('could not start the Screepub engine'); };
    expect(await convert.revealNote(crashed, '/x/a.epub')).toBe('could not start the Screepub engine');
    const notJson = async () => { throw new Error('the engine did not answer in JSON:\n<html>'); };
    expect(await convert.revealNote(notJson, '/x/a.epub'))
      .toBe('the engine did not answer in JSON:\n<html>');

    // Success says nothing.
    const succeeded = async () => ({ ok: true, revealed: '/x/a.epub' });
    expect(await convert.revealNote(succeeded, '/x/a.epub')).toBe('');
  });

  test('the working line names the file, not its path', () => {
    expect(convert.fileName('/home/a/scripts/The Script.pdf')).toBe('The Script.pdf');
    expect(convert.fileName('C:\\Users\\a\\The Script.pdf')).toBe('The Script.pdf');
    expect(convert.fileName('The Script.pdf')).toBe('The Script.pdf');
  });

  test('the code it branches on is a code the engine really returns', async () => {
    // The one assertion here that a change to the engine could break: if
    // prose.pdf ever stopped reporting not-screenplay, this surface would go
    // on offering "Convert anyway" for a code nobody sends.
    const root = new URL('..', import.meta.url).pathname;
    const proc = Bun.spawn(
      ['bun', join(root, 'src', 'cli.ts'), join(root, 'tests', 'fixtures', 'prose.pdf'),
        '--json', '-o', join(SCRATCH, 'convert-surface.epub')],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    const [stdout] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(false);
    const shown = convert.failureFor(answer.error);
    expect(shown.canForce).toBe(true);
    expect(shown.heading).toBe(convert.HEADINGS['not-screenplay']);
    // And the trim is measured against what the ENGINE really writes rather
    // than against a sentence typed out in this file: the remedy the window
    // replaced with a button is in the engine's message and out of the
    // window's, and every other word of it survives.
    expect(answer.error.message).toContain('--force');
    expect(shown.message).not.toContain('--force');
    expect(shown.message.length).toBeGreaterThan(20);
    expect(answer.error.message.startsWith(shown.message)).toBe(true);
  }, 60000);

  test('libraryFrom accepts a clean app-settings answer and rejects anything malformed', () => {
    const good = {
      ok: true,
      library: {
        path: '/Users/ann/Documents/Screepub',
        chosen: null,
        platformDefault: '/Users/ann/Documents/Screepub',
        fromEnv: false,
      },
      home: '/Users/ann',
    };
    expect(convert.libraryFrom(good)).toEqual({
      path: '/Users/ann/Documents/Screepub',
      chosen: null,
      platformDefault: '/Users/ann/Documents/Screepub',
      fromEnv: false,
      home: '/Users/ann',
    });
    for (const bad of [
      null,
      undefined,
      'nope',
      { ok: false, library: good.library, home: good.home },
      { ok: true, library: null, home: good.home },
      { ok: true, library: { ...good.library, path: 3 }, home: good.home },
      { ok: true, library: { ...good.library, chosen: 3 }, home: good.home },
      { ok: true, library: { ...good.library, platformDefault: null }, home: good.home },
      { ok: true, library: { ...good.library, fromEnv: 'no' }, home: good.home },
      { ok: true, library: good.library, home: 9 },
      { ok: true, library: good.library }, // no home at all
    ]) {
      expect(convert.libraryFrom(bad as never)).toBeNull();
    }
  });

  test('libraryLine shortens the home folder to ~, and only when the path is really under it', () => {
    const base = { chosen: null, platformDefault: '/Users/ann/Documents/Screepub', fromEnv: false };
    // The home folder itself.
    expect(convert.libraryLine({ ...base, path: '/Users/ann', home: '/Users/ann' }))
      .toBe('Books are saved in ~.');
    // A folder under home.
    expect(convert.libraryLine({ ...base, path: '/Users/ann/Documents/Screepub', home: '/Users/ann' }))
      .toBe('Books are saved in ~/Documents/Screepub.');
    // A home the caller handed over WITH its own trailing separator must not
    // double the slash or swallow the leading one off the rest.
    expect(convert.libraryLine({ ...base, path: '/Users/ann/Documents/Screepub', home: '/Users/ann/' }))
      .toBe('Books are saved in ~/Documents/Screepub.');
    // A sibling folder that merely shares the prefix is not "under" home.
    expect(convert.libraryLine({ ...base, path: '/Users/ann2/Books', home: '/Users/ann' }))
      .toBe('Books are saved in /Users/ann2/Books.');
    // Windows, backslashes throughout.
    expect(convert.libraryLine({
      ...base, path: 'C:\\Users\\ann\\Documents\\Screepub', home: 'C:\\Users\\ann',
    })).toBe('Books are saved in ~\\Documents\\Screepub.');
    // Nowhere near home.
    expect(convert.libraryLine({ ...base, path: '/Volumes/External/Books', home: '/Users/ann' }))
      .toBe('Books are saved in /Volumes/External/Books.');
    // SCREEPUB_LIBRARY says so, and is shortened by the same rule.
    expect(convert.libraryLine({
      ...base, path: '/Users/ann/Books', home: '/Users/ann', fromEnv: true,
    })).toBe('Books are saved in ~/Books, set by SCREEPUB_LIBRARY.');
    // A root home is still just the home folder: "~", not "~/" or "~\".
    expect(convert.libraryLine({ ...base, path: '/', home: '/' }))
      .toBe('Books are saved in ~.');
    expect(convert.libraryLine({ ...base, path: 'C:\\', home: 'C:\\' }))
      .toBe('Books are saved in ~.');
    // Windows paths compare case-insensitively, because the filesystem they
    // name does.
    expect(convert.libraryLine({
      ...base, path: 'c:\\users\\ann\\Documents\\Screepub', home: 'C:\\Users\\Ann',
    })).toBe('Books are saved in ~\\Documents\\Screepub.');
    // POSIX paths do not: a case difference is a different, unrelated path.
    expect(convert.libraryLine({
      ...base, path: '/users/ann/Documents', home: '/Users/Ann',
    })).toBe('Books are saved in /users/ann/Documents.');
    // A home given WITH its own trailing separator, where the path is the
    // home folder itself WITHOUT one, is still an exact match.
    expect(convert.libraryLine({ ...base, path: '/Users/ann', home: '/Users/ann/' }))
      .toBe('Books are saved in ~.');
  });

  test('libraryActions offers Change alone, Change and Reset, or neither', () => {
    const base = { path: '/x', platformDefault: '/x', home: '/h' };
    expect(convert.libraryActions({ ...base, chosen: null, fromEnv: false })).toEqual(['change']);
    expect(convert.libraryActions({ ...base, chosen: '/x', fromEnv: false }))
      .toEqual(['change', 'reset']);
    // SCREEPUB_LIBRARY wins over whatever was chosen, so there is nothing to
    // offer changing.
    expect(convert.libraryActions({ ...base, chosen: '/x', fromEnv: true })).toEqual([]);
    expect(convert.libraryActions({ ...base, chosen: null, fromEnv: true })).toEqual([]);
  });

  test('movedLine says what moving the library does, and does not do, in one sentence', () => {
    expect(convert.movedLine).toBe(
      'New books go here. Books already converted stay where they are.',
    );
  });

  test('libraryChangeArgs is the --set value for a folder, or for going back to the default', () => {
    expect(convert.libraryChangeArgs('/Users/ann/Books')).toBe('{"libraryPath":"/Users/ann/Books"}');
    expect(convert.libraryChangeArgs(null)).toBe('{"libraryPath":null}');
  });

  test('libraryAfter reads a clean library out of a --set answer, or the engine’s refusal', () => {
    const answer = {
      ok: true,
      library: { path: '/x', chosen: '/x', platformDefault: '/d', fromEnv: false },
      home: '/h',
    };
    expect(convert.libraryAfter(answer)).toEqual({
      ok: true,
      library: { path: '/x', chosen: '/x', platformDefault: '/d', fromEnv: false, home: '/h' },
    });
    // The engine's own sentence, verbatim.
    expect(convert.libraryAfter({
      ok: false,
      error: { code: 'bad-settings', message: 'the library folder must be a full path' },
    })).toEqual({ ok: false, message: 'the library folder must be a full path' });
    // A contract-breaking refusal still says something a person can act on,
    // but NOT failureFor's "the engine refused the file without saying why":
    // nothing was refused and there is no file, so this gets its own words.
    expect(convert.libraryAfter({ ok: false, error: {} })).toEqual({
      ok: false, message: convert.NO_LIBRARY_MESSAGE,
    });
    expect(convert.libraryAfter({ ok: false })).toEqual({
      ok: false, message: convert.NO_LIBRARY_MESSAGE,
    });
    // ok: true with nothing usable inside is just as much a broken contract
    // as a refusal with no sentence, and gets the same fallback.
    expect(convert.libraryAfter({ ok: true, library: null, home: '/h' })).toEqual({
      ok: false, message: convert.NO_LIBRARY_MESSAGE,
    });
    expect(convert.NO_LIBRARY_MESSAGE).toBe('Screepub could not confirm where books are saved.');
  });

  test('showsMovedLine only after a Change that actually moved the library, never Reset or a same-folder pick', () => {
    expect(convert.showsMovedLine('change', '/old', { path: '/new' })).toBe(true);
    // Reset: going back to the default did not convert anything either.
    expect(convert.showsMovedLine('reset', '/old', { path: '/default' })).toBe(false);
    // A same-folder pick moved nothing, so this says nothing even if it
    // were ever asked (in the running surface it never is: run() returns
    // before the engine call when the pick equals the current path).
    expect(convert.showsMovedLine('change', '/old', { path: '/old' })).toBe(false);
  });
});

describe('what the Convert surface decides about a refusal', () => {
  type ConvertBits = {
    withoutCliRemedy: (message: unknown, flag: unknown) => string;
    stillOpenNote: (script: unknown) => string | null;
  };
  let convert: ConvertBits;
  beforeAll(async () => {
    convert = (await import(join(UI, 'convert.js'))) as ConvertBits;
  });

  test('only the sentence naming the flag comes out, and only if one does', () => {
    const flag = '--force';
    // The real shape: diagnosis, then remedy. The remedy goes; the diagnosis
    // is returned unedited, which is checked by equality rather than by
    // "does not contain" — a function that returned the empty string, or the
    // first word, would pass that weaker check.
    expect(convert.withoutCliRemedy(
      'No scene headings found. Pass --force to convert it anyway.', flag,
    )).toBe('No scene headings found.');
    // Three sentences, the flag in the MIDDLE: the tail is not collateral.
    expect(convert.withoutCliRemedy(
      'A is true. Pass --force to convert it anyway. B is still true.', flag,
    )).toBe('A is true. B is still true.');
    // Nothing to take out: byte for byte what came in, for a message with no
    // flag in it at all and for one whose text merely resembles a flag.
    for (const said of [
      'cannot read the input file (EACCES)',
      'This PDF is password-protected.',
      'the line is a run-on with no full stop at all',
      'A dash--dash is not a flag.',
    ]) {
      expect(`kept: ${convert.withoutCliRemedy(said, flag)}`).toBe(`kept: ${said}`);
    }
    // A message that is ONLY the remedy is left whole. A blank fault body
    // says less than a sentence a window user cannot act on, and this is the
    // failure mode of every "strip the last sentence" implementation.
    expect(convert.withoutCliRemedy('Pass --force to convert it anyway.', flag))
      .toBe('Pass --force to convert it anyway.');
    // No flag to look for is not a licence to cut.
    expect(convert.withoutCliRemedy('Pass --force to convert it anyway.', ''))
      .toBe('Pass --force to convert it anyway.');
    expect(convert.withoutCliRemedy(undefined, flag)).toBe('');
  });

  test('the refusal says the open script survived, and names it', () => {
    // The decision behind it: a file the engine would not read produced
    // nothing to replace the open book with, so it replaces nothing.
    const note = convert.stillOpenNote({ title: 'The Last Video Store' });
    expect(note).toContain('The Last Video Store');
    expect(note).toContain('still open');
    // Nothing open, nothing to reassure anyone about: a line reading
    // "null is still open" is exactly the stray `null` this plan caught once.
    for (const nothing of [null, undefined, {}, { title: '' }, { title: '   ' }]) {
      expect(`${JSON.stringify(nothing)}: ${convert.stillOpenNote(nothing)}`)
        .toBe(`${JSON.stringify(nothing)}: null`);
    }
  });
});

describe('where the keyboard stands when a dialog closes', () => {
  // The defect this is the fix for: Ctrl-O then Escape, with a result on
  // screen, could leave the page with NO focused element — Tab, Shift-Tab
  // and the tablist's arrows all dead, and no way back without a mouse.
  // Placing the focus is a decision, so it is a pure function and it is
  // tested here rather than in the handler that first needed it.
  type FakeNode = {
    id?: string; isConnected?: boolean; disabled?: boolean; hidden?: boolean;
    closest?: (selector: string) => unknown;
  };
  type FocusModule = {
    FOCUSABLE: string;
    ALWAYS: string;
    focusPlan: (surface: unknown) => string[];
    canFocus: (node: unknown) => boolean;
    firstStop: (candidates: unknown) => unknown;
    stopAfterDialog: (surface: string, queryAll: (s: string) => Iterable<unknown>) => unknown;
  };
  let focus: FocusModule;
  // The three the bar switches between. Notes left the bar on 2026-09-20: it
  // is a <dialog> opened from the rev stamp now, so the platform owns its
  // focus and there is no #tab-notes for a plan to point at. Send left it on
  // 2026-10-01: sending is the result screen's panel now.
  const SURFACES = ['convert', 'read', 'tune'];

  beforeAll(async () => {
    focus = (await import(join(UI, 'focus.js'))) as FocusModule;
  });

  test('every surface gets a plan, and it is that surface’s plan', () => {
    const plans = new Map(SURFACES.map((s) => [s, focus.focusPlan(s)]));
    for (const [surface, plan] of plans) {
      // The order IS the decision: the surface's own work first, then the
      // pane, then its tab, then the one tab that is never disabled.
      expect(`${surface} steps: ${plan.length}`).toBe(`${surface} steps: 4`);
      expect(plan[1]).toBe(`#surface-${surface}`);
      expect(plan[2]).toBe(`#tab-${surface}`);
      expect(plan[3]).toBe(focus.ALWAYS);
      // The first step must be SCOPED to the showing pane. Unscoped, its
      // first match in the document is the tablist's first tab, so every
      // dialog on every surface would dump the reader back on Convert —
      // and a test that only checked for 'button' would not notice.
      for (const part of plan[0].split(', ')) {
        expect(`${surface} step 1 part: ${part}`).toBe(
          `${surface} step 1 part: #surface-${surface} ${part.split(' ').slice(1).join(' ')}`,
        );
      }
      expect(plan[0]).toContain('button:not([disabled])');
    }
    // Five surfaces, five different plans: a plan that ignored its argument
    // would satisfy everything above for whichever surface it hard-coded.
    expect(new Set([...plans.values()].map((p) => p.join('|'))).size).toBe(SURFACES.length);
  });

  test('a disabled, detached or hidden control is not somewhere to stand', () => {
    const good: FakeNode = { id: 'good', isConnected: true, closest: () => null };
    expect(focus.canFocus(good)).toBe(true);
    // Each one asserted against that same node with ONE property changed, so
    // a canFocus() that always returned false could not pass the line above
    // and one that always returned true cannot pass these.
    expect(focus.canFocus({ ...good, disabled: true })).toBe(false);
    expect(focus.canFocus({ ...good, isConnected: false })).toBe(false);
    expect(focus.canFocus({ ...good, hidden: true })).toBe(false);
    // A control in a pane that is hidden is just as unreachable as a hidden
    // control: this is the case that matters, because four of the five panes
    // are hidden at any moment.
    expect(focus.canFocus({ ...good, closest: (sel: string) => (sel === '[hidden]' ? {} : null) }))
      .toBe(false);
    expect(focus.canFocus(null)).toBe(false);
    expect(focus.canFocus(undefined)).toBe(false);
  });

  test('the first candidate that can take the focus wins, and null means leave it', () => {
    const at = (id: string, extra: FakeNode = {}): FakeNode =>
      ({ id, isConnected: true, closest: () => null, ...extra });
    const dead = at('dead', { disabled: true });
    const first = at('first');
    const second = at('second');
    expect(focus.firstStop([dead, first, second])).toBe(first);
    // Not the last focusable one, which a fold written the wrong way round
    // would return, and not the first CANDIDATE, which no filter would.
    expect(focus.firstStop([dead, second, first])).toBe(second);
    expect(focus.firstStop([dead, at('gone', { isConnected: false })])).toBeNull();
    expect(focus.firstStop([])).toBeNull();
    expect(focus.firstStop(undefined)).toBeNull();
  });

  test('the plan and the page together land the keyboard on the showing surface', () => {
    // A fake document: selector in, nodes out, in the order a real
    // querySelectorAll would give them. No DOM, and the composition — which
    // is where the order can be lost — is still exercised end to end.
    const node = (id: string, extra: FakeNode = {}): FakeNode =>
      ({ id, isConnected: true, closest: () => null, ...extra });
    const page = (map: Record<string, FakeNode[]>) => (selector: string) => map[selector] ?? [];

    // A refusal on Convert: two buttons in the pane, the first of them is
    // where a reader carries on from.
    const convertButtons = [node('convert-anyway'), node('back-to-one')];
    const plan = focus.focusPlan('convert');
    expect(focus.stopAfterDialog('convert', page({
      [plan[0]]: convertButtons,
      [plan[1]]: [node('pane')],
      [focus.ALWAYS]: [node('tab-convert')],
    }))).toBe(convertButtons[0]);

    // A surface with no control at all — the progress bar while it works.
    // The pane carries tabindex="0", so Tab still moves from there.
    const pane = node('pane');
    expect(focus.stopAfterDialog('convert', page({
      [plan[1]]: [pane], [focus.ALWAYS]: [node('tab-convert')],
    }))).toBe(pane);

    // The pane's controls are all disabled — Tune's two indent sliders are
    // really like this — so the pane is still the answer, not the first
    // disabled slider.
    expect(focus.stopAfterDialog('convert', page({
      [plan[0]]: [node('slider', { disabled: true })],
      [plan[1]]: [pane],
    }))).toBe(pane);

    // Nothing left of the surface: the tab that exists in every state.
    const always = node('tab-convert');
    expect(focus.stopAfterDialog('read', page({ [focus.ALWAYS]: [always] }))).toBe(always);

    // A page with nothing focusable anywhere leaves the focus alone rather
    // than moving it somewhere worse.
    expect(focus.stopAfterDialog('read', page({}))).toBeNull();

    // And it really does use the surface it was given: the same page, asked
    // for a different surface, finds nothing.
    expect(focus.stopAfterDialog('tune', page({ [plan[0]]: convertButtons }))).toBeNull();
  });

  test('the reader’s Tab stop draws a ring, and nothing tries to ring the frame', () => {
    // Kept as a CSS pin, because painting a focus ring needs a browser. The
    // stop itself (the stage around the frame, never the frame) is driven in
    // "the Tab stop is the stage around the frame, never the frame itself":
    // WebKitGTK gives a focused iframe no :focus match, no outline and no
    // focus event at all (measured), so the stage is what has to ring.
    const css = read('surfaces.css');
    const rings = [...css.matchAll(/\.script-stage:focus[^{]*\{([^}]*)\}/g)].map((m) => m[1]);
    expect(rings.length).toBeGreaterThan(0);
    expect(rings.every((body) => /outline:\s*\d/.test(body))).toBe(true);
    expect(`surfaces.css rings the frame: ${/\.script-frame:focus/.test(css)}`)
      .toBe('surfaces.css rings the frame: false');
  });

  test('the arrow keys still move the script, now that the frame is not the stop', async () => {
    // What the frame used to get free from the engine. Exercised as a
    // decision rather than read off the handler.
    const reader = (await import(join(UI, 'read.js'))) as {
      scrollStep: (key: string, height: unknown) => number | string | null;
      READER_LINE: number;
      PAGE_FLOOR: number;
    };
    const step = reader.scrollStep;
    expect(step('ArrowDown', 600)).toBe(reader.READER_LINE);
    expect(step('ArrowUp', 600)).toBe(-reader.READER_LINE);
    // A page is most of the frame, not all of it: reading loses its place if
    // the line you stopped on scrolls off the top.
    const page = step('PageDown', 600) as number;
    expect(page).toBeLessThan(600);
    expect(page).toBeGreaterThan(600 / 2);
    expect(step('PageUp', 600)).toBe(-page);
    expect(step(' ', 600)).toBe(page);
    // Absolute, not a big number: Home and End are the ends of the script.
    expect(step('Home', 600)).toBe('top');
    expect(step('End', 600)).toBe('bottom');
    // A page scales with the frame — a fixed number would be a page on one
    // window size and a line on another — and never shrinks to nothing when
    // the frame has no height yet to measure.
    expect(step('PageDown', 1200)).toBeGreaterThan(page);
    for (const nothing of [0, undefined, null, 'tall', NaN, -400]) {
      expect(`${nothing}: ${step('PageDown', nothing)}`)
        .toBe(`${nothing}: ${reader.PAGE_FLOOR}`);
    }
    // Every other key belongs to the window, not to the frame: a handler
    // that answered Tab or ArrowRight would eat the surface's own keyboard.
    for (const key of ['Tab', 'ArrowLeft', 'ArrowRight', 'Enter', 'Escape', 'a', '']) {
      expect(`${key}: ${step(key, 600)}`).toBe(`${key}: null`);
    }
  });
});

describe('one file dialog at a time', () => {
  // Ctrl-O twice used to open two native pickers, both modal, both waiting
  // on the same window. `busy` guarded a running CONVERSION and nothing else.
  type AppModule = {
    pickScreenplay: () => Promise<string | null>;
    isDialogOpen: () => boolean;
    onDialogClosed: (handler: () => void) => void;
  };

  function stubPicker() {
    let calls = 0;
    let settle: ((path: string | null) => void) | null = null;
    let fail: ((err: unknown) => void) | null = null;
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: {
        core: {
          invoke: () => {
            calls += 1;
            return new Promise((resolve, reject) => { settle = resolve; fail = reject; });
          },
        },
      },
    };
    return {
      get calls() { return calls; },
      answer: (path: string | null) => settle!(path),
      reject: (err: unknown) => fail!(err),
    };
  }

  afterEach(() => { delete (globalThis as unknown as { window?: unknown }).window; });

  test('a second ask while one is open opens nothing, and the first still answers', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const picker = stubPicker();

    const first = app.pickScreenplay();
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    // The assertion that matters: the count of dialogs actually OPENED.
    // A guard that returned the first promise again, or that let the second
    // through, both look the same from the caller.
    expect(await app.pickScreenplay()).toBeNull();
    expect(await app.pickScreenplay()).toBeNull();
    expect(`pickers opened: ${picker.calls}`).toBe('pickers opened: 1');

    picker.answer('/s/script.pdf');
    expect(await first).toBe('/s/script.pdf');

    // ...and the guard clears, or the window could never open a second file.
    // Asserted against the state it changed FROM, above.
    expect(app.isDialogOpen()).toBe(false);
    const again = app.pickScreenplay();
    await Promise.resolve();
    expect(`pickers opened after: ${picker.calls}`).toBe('pickers opened after: 2');
    picker.answer(null);
    expect(await again).toBeNull();
  });

  test('a closed handler that throws neither loses the pick nor stops the handlers after it', async () => {
    // Every onDialogClosed handler runs in withOneDialog's finally. One that
    // threw used to replace the dialog's answer with its own error, so the
    // reader's pick was lost, and every handler after it was skipped. A
    // module of its own: the handlers list lives as long as app.js does, and
    // a throwing one must not reach any other test's dialog.
    const app = (await import(`${join(UI, 'app.js')}?closed-handlers`)) as AppModule;
    const picker = stubPicker();
    const heard: string[] = [];
    app.onDialogClosed(() => { heard.push('first'); throw new Error('the keyboard had nowhere to go'); });
    app.onDialogClosed(() => { heard.push('second'); });
    const logged: unknown[][] = [];
    const realError = console.error;
    console.error = (...args: unknown[]) => { logged.push(args); };
    try {
      const asked = app.pickScreenplay();
      await Promise.resolve();
      picker.answer('/s/script.pdf');
      expect(await asked).toBe('/s/script.pdf');
    } finally {
      console.error = realError;
    }
    expect(heard).toEqual(['first', 'second']);
    expect(app.isDialogOpen()).toBe(false);
    // Not swallowed either: said where a developer looks.
    expect(logged.length).toBe(1);
    expect(String(logged[0].at(-1))).toContain('the keyboard had nowhere to go');
  });

  test('a dialog that fails still closes, and still hands the keyboard back', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const picker = stubPicker();
    let closed = 0;
    app.onDialogClosed(() => { closed += 1; });

    const asked = app.pickScreenplay().then(() => 'resolved', () => 'rejected');
    await Promise.resolve();
    picker.reject(new Error('no portal'));
    expect(await asked).toBe('rejected');
    // A guard left standing by a throw would lock the window out of ever
    // opening a file again, and the keyboard would never be put back.
    expect(app.isDialogOpen()).toBe(false);
    expect(`closed: ${closed}`).toBe('closed: 1');

    const after = app.pickScreenplay();
    await Promise.resolve();
    expect(`pickers opened: ${picker.calls}`).toBe('pickers opened: 2');
    picker.answer(null);
    await after;
    expect(`closed: ${closed}`).toBe('closed: 2');
  });
});

describe('the folder picker door', () => {
  // pickFolder shares pickScreenplay's guard rather than inventing a second
  // one: they are both modal dialogs waiting on the same window, and a
  // guard that only knew about one of them would let the other jump the
  // queue.
  type AppModule = {
    pickFolder: (opts?: { defaultPath?: string }) => Promise<string | null>;
    pickScreenplay: () => Promise<string | null>;
    isDialogOpen: () => boolean;
    onDialogClosed: (handler: () => void) => void;
  };

  function stubDialog(open: (options: unknown) => Promise<unknown>) {
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: { dialog: { open } },
    };
  }

  afterEach(() => { delete (globalThis as unknown as { window?: unknown }).window; });

  test('resolves the chosen folder, and asks the plugin for a directory picker with the default path', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    let seen: unknown;
    stubDialog(async (options) => { seen = options; return '/Users/reader/Books'; });
    expect(await app.pickFolder({ defaultPath: '/old/Books' })).toBe('/Users/reader/Books');
    expect(seen).toEqual({ directory: true, defaultPath: '/old/Books' });
  });

  test('cancelling resolves null, and an array or object answer is normalised to a string', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    stubDialog(async () => null);
    expect(await app.pickFolder()).toBeNull();
    // A future plugin version's multi-select shape: the first path wins,
    // the same "one window converts one dropped script" rule droppedPath
    // applies to a drag.
    stubDialog(async () => ['/first', '/second']);
    expect(await app.pickFolder()).toBe('/first');
    stubDialog(async () => ({ path: '/from/object' }));
    expect(await app.pickFolder()).toBe('/from/object');
    stubDialog(async () => ({}));
    expect(await app.pickFolder()).toBeNull();
  });

  test('a folder picker already open refuses pickScreenplay too, the same guard both share', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    let calls = 0;
    let settle: ((v: string | null) => void) | null = null;
    stubDialog(() => {
      calls += 1;
      return new Promise((resolve) => { settle = resolve; });
    });
    const first = app.pickFolder();
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    // pickScreenplay would open a native picker of its own if the guard did
    // not know about pickFolder too; it must answer null without asking.
    expect(await app.pickScreenplay()).toBeNull();
    expect(`dialogs opened: ${calls}`).toBe('dialogs opened: 1');
    settle!('/picked');
    expect(await first).toBe('/picked');
    expect(app.isDialogOpen()).toBe(false);
  });

  test('a second pickFolder while one is open opens nothing, and the first still answers', async () => {
    // The mirror of the mixed-picker test above, with the SAME kind of
    // picker on both ends: two folder pickers must not disagree with
    // themselves any more than a folder picker and the file picker do.
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    let calls = 0;
    let settle: ((v: string | null) => void) | null = null;
    stubDialog(() => {
      calls += 1;
      return new Promise((resolve) => { settle = resolve; });
    });
    const first = app.pickFolder();
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    expect(await app.pickFolder()).toBeNull();
    expect(await app.pickFolder()).toBeNull();
    expect(`dialogs opened: ${calls}`).toBe('dialogs opened: 1');
    settle!('/picked');
    expect(await first).toBe('/picked');
    expect(app.isDialogOpen()).toBe(false);
  });

  test('pickScreenplay open, then pickFolder resolves null without ever asking the dialog plugin', async () => {
    // The reverse direction of the mixed-picker test above: a file picker
    // already open must hold a folder picker off too, and the dialog
    // plugin's open() must never be CALLED, not merely have its answer
    // ignored.
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    let fileSettle: ((v: string | null) => void) | null = null;
    let dialogCalls = 0;
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => { fileSettle = resolve; }) },
        dialog: { open: () => { dialogCalls += 1; return Promise.resolve('/never'); } },
      },
    };
    try {
      const first = app.pickScreenplay();
      await Promise.resolve();
      expect(app.isDialogOpen()).toBe(true);
      expect(await app.pickFolder()).toBeNull();
      expect(`dialog.open calls: ${dialogCalls}`).toBe('dialog.open calls: 0');
      fileSettle!('/s/script.pdf');
      expect(await first).toBe('/s/script.pdf');
      expect(app.isDialogOpen()).toBe(false);
    } finally {
      delete (globalThis as unknown as { window?: unknown }).window;
    }
  });

  test('onDialogClosed handlers fire for pickFolder too, on a resolve and on a rejection', async () => {
    // pickScreenplay's own onDialogClosed guarantee (tested in "one file
    // dialog at a time" above) needs the same proof for pickFolder: a guard
    // left standing by a throw would lock the window out of ever opening a
    // folder picker again, and the keyboard would never be put back.
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    let closed = 0;
    app.onDialogClosed(() => { closed += 1; });

    let resolveOpen: ((v: string | null) => void) | null = null;
    stubDialog(() => new Promise((resolve) => { resolveOpen = resolve; }));
    const picked = app.pickFolder();
    await Promise.resolve();
    resolveOpen!('/picked');
    expect(await picked).toBe('/picked');
    expect(`closed after resolve: ${closed}`).toBe('closed after resolve: 1');
    expect(app.isDialogOpen()).toBe(false);

    let rejectOpen: ((err: unknown) => void) | null = null;
    stubDialog(() => new Promise((_resolve, reject) => { rejectOpen = reject; }));
    const asked = app.pickFolder().then(() => 'resolved', () => 'rejected');
    await Promise.resolve();
    rejectOpen!(new Error('no portal'));
    expect(await asked).toBe('rejected');
    expect(`closed after reject: ${closed}`).toBe('closed after reject: 2');
    expect(app.isDialogOpen()).toBe(false);
  });
});

describe('the save dialog shares pickScreenplay\'s one-dialog guard', () => {
  // Parity piece B: dialog:allow-save lets the window ask for a save path
  // directly (ADR 2026-09-21). It must not be a second, independent guard,
  // or Ctrl-O and a save button could put two native dialogs up at once,
  // which is the exact defect the picker's own guard exists to prevent.
  type AppModule = {
    pickScreenplay: () => Promise<string | null>;
    saveDialog: (opts: {
      defaultPath: string;
      filters: Array<{ name: string; extensions: string[] }>;
    }) => Promise<string | null>;
    isDialogOpen: () => boolean;
    onDialogClosed: (handler: () => void) => void;
  };

  function stubDialogs() {
    let pickCalls = 0;
    let saveCalls = 0;
    let lastSaveArgs: unknown = null;
    let settlePick: ((path: string | null) => void) | null = null;
    let settleSave: ((path: string | null) => void) | null = null;
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: {
        core: {
          invoke: () => {
            pickCalls += 1;
            return new Promise((resolve) => { settlePick = resolve; });
          },
        },
        dialog: {
          save: (args: unknown) => {
            saveCalls += 1;
            lastSaveArgs = args;
            return new Promise((resolve) => { settleSave = resolve; });
          },
        },
      },
    };
    return {
      get pickCalls() { return pickCalls; },
      get saveCalls() { return saveCalls; },
      get lastSaveArgs() { return lastSaveArgs; },
      answerPick: (path: string | null) => settlePick!(path),
      answerSave: (path: string | null) => settleSave!(path),
    };
  }

  afterEach(() => { delete (globalThis as unknown as { window?: unknown }).window; });

  test('it asks the OS with the given defaultPath and filters, and resolves the chosen path', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const dialogs = stubDialogs();
    const filters = [{ name: 'EPUB', extensions: ['epub'] }];
    const asked = app.saveDialog({ defaultPath: 'script.epub', filters });
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    expect(dialogs.lastSaveArgs).toEqual({ defaultPath: 'script.epub', filters });
    dialogs.answerSave('/s/out.epub');
    expect(await asked).toBe('/s/out.epub');
    expect(app.isDialogOpen()).toBe(false);
  });

  test('cancelling resolves null, same as a cancelled picker', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const dialogs = stubDialogs();
    const asked = app.saveDialog({ defaultPath: 'script.epub', filters: [] });
    await Promise.resolve();
    dialogs.answerSave(null);
    expect(await asked).toBeNull();
  });

  test('a picker already open blocks a save dialog, and a save dialog already open blocks a picker', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const dialogs = stubDialogs();

    const picking = app.pickScreenplay();
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    expect(await app.saveDialog({ defaultPath: 'x.epub', filters: [] })).toBeNull();
    expect(`save dialogs opened: ${dialogs.saveCalls}`).toBe('save dialogs opened: 0');
    dialogs.answerPick('/s/script.pdf');
    expect(await picking).toBe('/s/script.pdf');
    expect(app.isDialogOpen()).toBe(false);

    const saving = app.saveDialog({ defaultPath: 'x.epub', filters: [] });
    await Promise.resolve();
    expect(app.isDialogOpen()).toBe(true);
    expect(await app.pickScreenplay()).toBeNull();
    expect(`pickers opened: ${dialogs.pickCalls}`).toBe('pickers opened: 1');
    dialogs.answerSave('/s/out.epub');
    expect(await saving).toBe('/s/out.epub');
    expect(app.isDialogOpen()).toBe(false);
  });

  test('onDialogClosed handlers fire after a save dialog closes too, cancel or not', async () => {
    const app = (await import(join(UI, 'app.js'))) as AppModule;
    const dialogs = stubDialogs();
    let closed = 0;
    app.onDialogClosed(() => { closed += 1; });
    const asked = app.saveDialog({ defaultPath: 'x.epub', filters: [] });
    await Promise.resolve();
    dialogs.answerSave(null);
    await asked;
    expect(`closed: ${closed}`).toBe('closed: 1');
  });
});

describe('what runEngine does with the answer it is handed', () => {
  // Scope, stated plainly, because these used to claim more than they
  // covered: `invoke` is a stub here, so NOTHING below tests a transport.
  // Whether a large answer survives the trip out of the engine is settled
  // where it can be — tests/cli.test.ts drives the real binary through a
  // real pipe — and whether it survives Tauri's IPC was settled by measuring
  // the live window (desktop/README.md).
  //
  // What is left for this file is the one decision runEngine makes on its
  // own: it parses what it is given, and it truncates the RAW text only in
  // the message for an answer that did not parse. Getting that backwards —
  // capping the success path, or not capping the failure path — is the kind
  // of mistake a stub can catch, so it is caught here.
  async function answering(answer: unknown) {
    const { runEngine } = await import(join(UI, 'app.js'));
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: { core: { invoke: async () => answer } },
    };
    try {
      return await runEngine(['--version', '--json']);
    } finally {
      delete (globalThis as unknown as { window?: unknown }).window;
    }
  }

  /** A JSON answer of roughly `bytes` bytes, shaped like the engine's. */
  function bigAnswer(bytes: number) {
    const answer = { ok: true, title: 'Generated Load Sample', previewHtml: '' };
    answer.previewHtml = 'x'.repeat(Math.max(0, bytes - JSON.stringify(answer).length));
    return JSON.stringify(answer);
  }

  test('the 300-character cap never touches an answer that parses', async () => {
    // The cap belongs to the failure message and nowhere else. Applied one
    // step too early — to the stdout, before the parse — every answer over
    // 300 characters would become a syntax error, which is to say every
    // real one. A 500 KB answer makes that unmissable.
    const json = bigAnswer(500_000);
    expect(json.length).toBeGreaterThan(400_000);
    const parsed = await answering(json);
    // Length, not just shape: a cap that cut on a brace could still parse.
    expect(JSON.stringify(parsed).length).toBe(json.length);
    expect(parsed).toEqual(JSON.parse(json));
  });

  test('the engine’s own words come back unedited', async () => {
    // The refusal a reader sees is the engine's sentence, accents and all.
    // runEngine must hand it over exactly, not normalise or re-encode it.
    const message = 'No scene headings: this does not look like a screenplay. é 日本語 ’';
    const json = JSON.stringify({ ok: false, error: { code: 'not-screenplay', message } });
    const parsed = await answering(json);
    expect((parsed as { error: { message: string } }).error.message).toBe(message);
  });

  test('an answer that is not JSON is reported in a readable length', async () => {
    // Before the cap, a truncated 400 KB answer put 400 KB of raw JSON in a
    // 52-character column, burying the two buttons under it.
    const noise = `<!DOCTYPE html>${'a'.repeat(500_000)}`;
    const err = await answering(noise).then(
      () => null,
      (e: Error) => e,
    );
    expect(err).not.toBeNull();
    expect(err!.message).toContain('the engine did not answer in JSON');
    // The sentence, a newline, 300 characters and an ellipsis — and the
    // whole thing well under a screenful, whatever the answer's size.
    expect(err!.message.length).toBeLessThan(400);
    expect(err!.message).toContain('<!DOCTYPE html>');
    expect(err!.message.endsWith('…')).toBe(true);
  });

  test('a short answer is quoted in full, not always truncated', async () => {
    // Guards the other half: a cap that fired unconditionally would hide the
    // whole of the short answers that are the common case.
    const err = await answering('engine: command not found').then(
      () => null,
      (e: Error) => e,
    );
    expect(err!.message).toBe('the engine did not answer in JSON:\nengine: command not found');
  });

  test('a rejection from Rust is passed through as its own message', async () => {
    const { runEngine } = await import(join(UI, 'app.js'));
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: {
        core: {
          invoke: async () => {
            throw 'could not start the Screepub engine (sidecar "screepub-engine")';
          },
        },
      },
    };
    try {
      const err = await runEngine(['--version']).then(
        () => null,
        (e: Error) => e,
      );
      expect(err!.message).toBe('could not start the Screepub engine (sidecar "screepub-engine")');
    } finally {
      delete (globalThis as unknown as { window?: unknown }).window;
    }
  });
});

describe('the window knows when the engine is working, and can restart', () => {
  type Pending = { resolve: (v: string) => void; reject: (e: unknown) => void };
  const win = globalThis as unknown as { window?: unknown };

  test('whenIdle waits for every engine call, including one that fails', async () => {
    const app = await import(join(UI, 'app.js'));
    const pending: Pending[] = [];
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise<string>((resolve, reject) => pending.push({ resolve, reject })) },
      },
    };
    try {
      expect(app.engineBusy()).toBe(false);
      const first = app.runEngine(['--version', '--json']);
      // NOT 'devices': that one is never counted (see below), and this test
      // is about two COUNTED calls overlapping.
      const second = app.runEngine(['send', 'x.epub', '--json']).catch(() => 'failed');
      expect(app.engineBusy()).toBe(true);
      let idle = false;
      const waiting = app.whenIdle().then(() => { idle = true; });
      pending[0].resolve('{"ok":true}');
      await first;
      await new Promise((r) => setTimeout(r, 0));
      expect(idle).toBe(false); // one call is still running
      pending[1].reject('the engine is not there');
      expect(await second).toBe('failed');
      await waiting;
      expect(idle).toBe(true);
      expect(app.engineBusy()).toBe(false);
    } finally {
      // A failing assertion above would otherwise leave a pending promise
      // unsettled and `inFlight` stuck above zero: app.js's counters are a
      // module-level singleton, shared with every OTHER test that imports
      // it, so a hang here would hang tests that run long after this one.
      // Resolving an already-settled promise a second time is a documented
      // no-op, so this is safe whether the try block finished cleanly or not.
      for (const p of pending) p.resolve('{}');
      await new Promise((r) => setTimeout(r, 0));
      delete win.window;
    }
  });

  test('an awaited chain (settings, then export, then send) keeps the engine busy start to finish', async () => {
    // The bug a reviewer reproduced: "done settings -> phase restarting ->
    // RESTART -> invoke export". A single macrotask of quiet was read as
    // "idle" between two calls of the SAME job. Real invoke() calls answer
    // on their own timer, not a manually-resolved Promise, so this drives
    // app.runEngine for real through three awaited calls that each answer
    // after setTimeout(10ms): the same shape send-panel.js's sendTo() chain has
    // (settings via ensureSettings(), then export, then send), with
    // whenIdle() asked right after the chain starts, not after it finishes.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    const log: string[] = [];
    win.window = {
      __TAURI__: {
        core: {
          invoke: (_cmd: string, payload: { args: string[] }) => new Promise<string>((resolve) => {
            setTimeout(() => { log.push(`finished ${payload.args[0]}`); resolve('{"ok":true}'); }, 10);
          }),
        },
      },
    };
    try {
      const chain = (async () => {
        await app.runEngine(['settings', 'x.fountain', '--json']);
        await app.runEngine(['export', 'x.epub', '--json', '--for', 'azw3']);
        await app.runEngine(['send', 'x.epub', '--json']);
      })();
      app.whenIdle().then(() => log.push('idle'));
      await chain;
      // The chain is over, but the quiet period has not: this is exactly the
      // window a single-macrotask idle check would have missed.
      expect(log).not.toContain('idle');
      await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
      expect(log).toEqual(['finished settings', 'finished export', 'finished send', 'idle']);
    } finally {
      delete win.window;
    }
  });

  test('a devices call is never counted: it does not make the engine busy or hold a restart', async () => {
    // device-watch.js polls `devices` every 2 s while a book is open, and each
    // poll takes about 1.5 s on its own (the reMarkable probe timeout).
    // Counting it would flash "Restarting after this finishes..." on that tab
    // on every poll and, with a quiet period, could hold a restart off for
    // as long as the tab stayed open.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const call = app.runEngine(['devices', '--json']);
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await call;
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
    } finally {
      delete win.window;
    }
  });

  test('a kfx-status call is never counted, like devices; a kfx-install call is', async () => {
    // Merged in from parity piece D: kfx.js's probe() runs kfx-status when
    // the KFX checklist opens (kfxShown) and again on window focus (onFocus) —
    // read-only, same shape as device-watch.js's devices poll, and for the same
    // reason it must not count: flashing "Restarting after this
    // finishes…" every time the reader tabs back to the window, and
    // holding a restart off for as long as Send stays open, is the exact
    // bug devices was excluded to avoid. kfx-install actually writes (into
    // Calibre), so it counts like any other mutating call.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const status = app.runEngine(app.argv.kfxStatus());
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await status;

      const install = app.runEngine(app.argv.kfxInstall());
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      await install;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('an app-settings read is never counted; a write, with --set, is', async () => {
    // The gear rereads the settings file every time it is shown, read-only
    // and never mid-job, the same shape as devices/kfx-status above. --set
    // writes that file, so only the write counts, like kfx-install above.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const read = app.runEngine(app.argv.appSettings());
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await read;

      const write = app.runEngine(app.argv.appSettings('{"libraryPath":"/abs"}'));
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      await write;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('a reveal call is never counted either, though for a different reason than devices', async () => {
    // reveal writes nothing at all: the file manager does the work, not the
    // engine. It is still excluded, because on some Linux desktops the
    // xdg-open call behind it can keep running until the file manager
    // window it opened is closed, which could hold a restart off for as
    // long as that window stayed open.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const call = app.runEngine(app.argv.reveal('/s/script.epub'));
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await call;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('a Kindle file check is never counted; the build it can lead to is', async () => {
    // The check reads two file dates and asks Calibre what is installed; the
    // send panel asks it every time the result shows and every time the window gets the
    // focus back, never mid-job. The export that builds the file is a job,
    // and a restart must wait for it, whether a press started it or the page
    // started it in the background.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle();
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const check = app.runEngine(app.argv.kindleCheck('/s/script.epub'));
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await check;

      const build = app.runEngine(app.argv.export('/s/script.epub', { forFormat: 'kindle' }));
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      await build;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('a routes call is never counted, like devices; a route call is', async () => {
    // The old Send page's poll swapped from `devices` to `routes` (parity piece B), and
    // the reason routes must not count is the same one devices was excluded
    // for: it is polled every 2 s while Send is open, never mid-job, and
    // counting it would flash "Restarting after this finishes…" on a tab
    // that is just sitting there polling. `route` actually writes: it saves
    // a file or opens an app, so it counts like send or kfx-install.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const routes = app.runEngine(app.argv.routes('/s/script.epub'));
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await routes;

      const route = app.runEngine(app.argv.route('apple-books', '/s/script.epub'));
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      await route;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('an eject counts toward busy, the background devices poll does not', async () => {
    // device-watch.js polls `devices --quick` every 2 s while a book is open,
    // so it must not count, for the reason plain `devices` never did. An
    // eject is somebody's work on a drive, so it counts like a send.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    win.window = {
      __TAURI__: {
        core: { invoke: () => new Promise((resolve) => setTimeout(() => resolve('{"ok":true}'), 10)) },
      },
    };
    try {
      const poll = app.runEngine(app.argv.devices({ quick: true }));
      expect(app.engineBusy()).toBe(false);
      expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
      await poll;

      const eject = app.runEngine(app.argv.eject('/Volumes/Kindle'));
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      await eject;
      expect(app.engineBusy()).toBe(false);
    } finally {
      delete win.window;
    }
  });

  test('whenIdle is actually fulfilled once the engine has been quiet long enough, not just eventually', async () => {
    // A test that only awaits whenIdle() and checks the flag it set passes
    // no matter how long that takes, which is not what "at once" means.
    // Bun.peek.status reads a promise's state without a microtask of its
    // own, so this proves the promise returned is ALREADY settled the
    // instant it is created, not merely one that resolves eventually.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // let any residual quiet period from an earlier test lapse
    await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
    expect(Bun.peek.status(app.whenIdle())).toBe('fulfilled');
  });

  test('a hold counts like a running call: busy at once, and whenIdle waits for its release plus the quiet period', async () => {
    // tune.js holds a moved knob for SETTLE_MS before it asks the engine to
    // save it, and during that settle no engine call is running at all. A
    // restart already waiting on whenIdle() used to fire in that gap and
    // drop the change (spec 2026-09-23-update-notice-and-window-drag-design
    // Part 3, the known gap). A hold is how a surface says "work is owed".
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
    expect(app.engineBusy()).toBe(false);

    const release = app.holdEngine();
    try {
      expect(typeof release).toBe('function');
      expect(app.engineBusy()).toBe(true);
      let idle = false;
      const waiting = app.whenIdle().then(() => { idle = true; });
      await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
      // Held longer than the quiet period, and still not idle: no engine
      // call ran, but the hold is what a restart must wait for.
      expect(idle).toBe(false);

      release();
      expect(app.engineBusy()).toBe(false);
      await new Promise((r) => setTimeout(r, 0));
      // Released, but the quiet period starts from the release, exactly as
      // it starts from the end of a counted call.
      expect(idle).toBe(false);
      await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
      await waiting;
      expect(idle).toBe(true);
    } finally {
      release();
    }
  });

  test('releasing a hold twice is releasing it once: the count never goes below zero', async () => {
    // A surface can have more than one way out of a hold (its save starts,
    // its schedule is cancelled, the script is closed). If the second
    // release subtracted again, the count would sit at -1, and the next
    // real call would read as idle while it ran.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    const release = app.holdEngine();
    release();
    release();
    expect(app.engineBusy()).toBe(false);

    let answer: (v: string) => void = () => {};
    win.window = {
      __TAURI__: { core: { invoke: () => new Promise<string>((resolve) => { answer = resolve; }) } },
    };
    try {
      const call = app.runEngine(['send', 'x.epub', '--json']);
      expect(app.engineBusy()).toBe(true);
      expect(Bun.peek.status(app.whenIdle())).toBe('pending');
      answer('{"ok":true}');
      await call;
      expect(app.engineBusy()).toBe(false);
    } finally {
      answer('{}');
      await new Promise((r) => setTimeout(r, 0));
      delete win.window;
    }
  });

  test('two holds and a call overlap and are each counted once', async () => {
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    const first = app.holdEngine();
    const second = app.holdEngine();
    first();
    first();
    expect(app.engineBusy()).toBe(true); // the second is still held
    second();
    expect(app.engineBusy()).toBe(false);
  });

  test('an open native dialog holds a restart off, whichever door opened it, until it closes', async () => {
    // Save a Kindle file builds the file, then opens the Save box with no
    // engine call running; Change… opens the folder picker the same way. A
    // restart waiting on whenIdle() used to be free to land on top of
    // either. Every dialog goes through app.js's one guard, so the hold is
    // taken there and no surface can forget it.
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
    const doors: [string, () => Promise<unknown>][] = [
      ['pickScreenplay', () => app.pickScreenplay()],
      ['pickFolder', () => app.pickFolder({ defaultPath: '/s' })],
      ['saveDialog', () => app.saveDialog({ defaultPath: '/s/x.epub', filters: [] })],
    ];
    for (const [name, open] of doors) {
      let close: (v: unknown) => void = () => {};
      const pendingDialog = () => new Promise((resolve) => { close = resolve; });
      win.window = {
        __TAURI__: { core: { invoke: pendingDialog }, dialog: { open: pendingDialog, save: pendingDialog } },
      };
      try {
        expect(`${name} before: ${app.engineBusy()}`).toBe(`${name} before: false`);
        const asked = open();
        expect(`${name} open: ${app.engineBusy()}`).toBe(`${name} open: true`);
        let idle = false;
        const waiting = app.whenIdle().then(() => { idle = true; });
        await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
        expect(`${name} still open, idle: ${idle}`).toBe(`${name} still open, idle: false`);

        close(null);
        await asked;
        expect(`${name} closed: ${app.engineBusy()}`).toBe(`${name} closed: false`);
        await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
        await waiting;
        expect(`${name} idle: ${idle}`).toBe(`${name} idle: true`);
      } finally {
        close(null);
        await new Promise((r) => setTimeout(r, 0));
        delete win.window;
      }
    }
  }, 20_000); // Seven real quiet periods, 4.4 s measured: too near bun's 5 s default to leave on it.

  test('a dialog that fails lets its hold go, and an ask refused by the guard takes none', async () => {
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle(); // start from a genuinely idle baseline
    const settle = { resolve: (_v: unknown) => {}, reject: (_e: unknown) => {} };
    win.window = {
      __TAURI__: {
        core: {
          invoke: () => new Promise((resolve, reject) => {
            settle.resolve = resolve;
            settle.reject = reject;
          }),
        },
      },
    };
    try {
      const first = app.pickScreenplay();
      expect(app.engineBusy()).toBe(true);
      // Refused by the one-dialog guard: nothing opened, nothing held, and
      // above all the FIRST dialog's hold is not released by it.
      expect(await app.pickScreenplay()).toBeNull();
      expect(app.engineBusy()).toBe(true);

      settle.reject('no portal');
      expect(await first.then(() => 'resolved', () => 'rejected')).toBe('rejected');
      expect(app.engineBusy()).toBe(false);
    } finally {
      settle.resolve(null);
      await new Promise((r) => setTimeout(r, 0));
      delete win.window;
    }
  });

  test('restart is offered only when the process plugin is in this build', async () => {
    const app = await import(join(UI, 'app.js'));
    let relaunched = 0;
    win.window = { __TAURI__: { core: {} } };
    try {
      expect(app.restartReady()).toBe(false);
      win.window = { __TAURI__: { process: { relaunch: async () => { relaunched += 1; } } } };
      expect(app.restartReady()).toBe(true);
      await app.restartApp();
      expect(relaunched).toBe(1);
    } finally {
      delete win.window;
    }
  });

  test('the window is never given the plugin\'s exit', () => {
    // process:allow-exit is not granted (capabilities/default.json). Rather
    // than banning one spelling of "call exit", check every property this
    // file actually reads off `process`, always through tauri(), the one
    // door onto Tauri this file uses for anything: the only two are
    // restartReady's guard and restartApp's call, and both name relaunch.
    const matches = read('app.js').match(/tauri\(\)\??\.process\??\.\w+/g) ?? [];
    expect(matches.length).toBeGreaterThan(0);
    for (const m of matches) expect(m).toMatch(/relaunch$/);
  });

  test('the quiet period is timed off a monotonic clock, not the wall clock', () => {
    // Date.now() jumps: a clock sync, DST, or someone changing the system
    // clock could make (Date.now() - lastEnded) go negative or huge, either
    // holding a restart off forever or releasing it early. performance.now()
    // cannot jump like that. Comments are stripped first (as the Tune
    // surface's `engine` helper does below) so an honest future comment
    // that merely mentions Date.now() cannot fail this: the claim is about
    // CODE.
    const code = read('app.js')
      .replace(/\/\*[\s\S]*?\*\//g, ' ')
      .replace(/\/\/.*$/gm, ' ');
    expect(code).not.toContain('Date.now()');
    expect(code).toContain('performance.now()');
  });
});

describe('the real flow and the real runEngine, driven together end to end', () => {
  // Every other update-flow.js test hands createUpdateFlow its own fakes.
  // This one drives the actual exported singleton (`flow`) against a
  // stubbed window.__TAURI__ and a stubbed localStorage, with a real
  // engine call held open through app.js's OWN runEngine/whenIdle, to prove
  // the two files actually agree once wired together rather than only in
  // each one's own fakes.
  //
  // Only offerFound() and start() are called on the singleton, never
  // boot(): boot() runs once ever (createUpdateFlow's own `booted` guard),
  // this module is imported once per test process and shared with every
  // other test that imports it, and a boot() here would both be silently
  // ignored by a later legitimate boot() and could plant a phase ahead of
  // whatever another test expects of the untouched singleton.
  test('relaunch waits for the held engine call to finish, plus the quiet period, not just for the call to end', async () => {
    const app = await import(join(UI, 'app.js'));
    const { flow } = await import(join(UI, 'update-flow.js'));
    await app.whenIdle(); // start from a genuinely idle baseline

    let relaunched = 0;
    let releaseHeld: () => void = () => {};
    const held = new Promise<string>((resolve) => { releaseHeld = () => resolve('{"ok":true}'); });
    const fakeUpdate = {
      version: '9.9.9',
      currentVersion: '0.0.0',
      body: 'test notes',
      downloadAndInstall: async (onEvent: (e: unknown) => void) => {
        onEvent({ event: 'Started', data: { contentLength: 1000 } });
        onEvent({ event: 'Progress', data: { chunkLength: 1000 } });
        onEvent({ event: 'Finished' });
      },
      close: async () => {},
    };
    const g = globalThis as unknown as { window?: unknown; localStorage?: unknown };
    g.window = {
      __TAURI__: {
        // Only the held call goes through this: the offer already carries a
        // live `update`, so installAndRestart never re-checks or re-reads
        // storage on this path.
        core: { invoke: () => held },
        updater: { check: async () => fakeUpdate },
        process: { relaunch: async () => { relaunched += 1; } },
      },
    };
    const stored = new Map<string, string>();
    g.localStorage = {
      getItem: (k: string) => stored.get(k) ?? null,
      setItem: (k: string, v: string) => { stored.set(k, String(v)); },
    };
    try {
      const heldCall = app.runEngine(['send', 'held.epub', '--json']);
      expect(app.engineBusy()).toBe(true);

      flow.offerFound({ outcome: 'offer', version: '9.9.9', body: 'test notes', update: fakeUpdate });
      const run = flow.start();
      await new Promise((r) => setTimeout(r, 0)); // let start() reach whenIdle()
      expect(relaunched).toBe(0);

      releaseHeld();
      await heldCall.catch(() => {});
      await new Promise((r) => setTimeout(r, 0)); // let runEngine's finally settle()
      // The held call has ENDED, but the quiet period has not: this is
      // exactly the gap a single-macrotask idle check would miss.
      expect(relaunched).toBe(0);

      await new Promise((r) => setTimeout(r, app.ENGINE_QUIET_MS + 50));
      await run;
      expect(relaunched).toBe(1);
    } finally {
      delete g.window;
      delete g.localStorage;
    }
  });
});

describe('the scene list is one Tab stop, walked with the arrow keys', () => {
  // One Tab stop per scene put ninety stops between the tabs and the script.
  // The list is now one stop with a roving tabindex, the pattern frame.js's
  // tablist uses: Up/Down/Home/End move between scenes, Enter and Space are
  // the buttons' own, and the stop is the scene the reader is in.
  type Rove = {
    railStep: (key: string, at: number, count: number) => number | null;
  };

  test('railStep moves one scene, or to an end, and stops at the ends', async () => {
    const { railStep } = (await import(join(UI, 'read.js'))) as Rove;
    expect(railStep('ArrowDown', 0, 3)).toBe(1);
    expect(railStep('ArrowDown', 2, 3)).toBe(2);
    expect(railStep('ArrowUp', 1, 3)).toBe(0);
    expect(railStep('ArrowUp', 0, 3)).toBe(0);
    expect(railStep('Home', 2, 3)).toBe(0);
    expect(railStep('End', 0, 3)).toBe(2);
    expect(railStep('Enter', 0, 3)).toBeNull();
    expect(railStep('ArrowRight', 0, 3)).toBeNull();
    expect(railStep('ArrowDown', 0, 0)).toBeNull();
  });

  // A document just big enough for the reader to draw a ready script into,
  // load it, build its rail and mark a scene.
  class Node {
    kids: Node[] = [];
    parent: Node | null = null;
    attrs = new Map<string, string>();
    handlers = new Map<string, Array<(event: unknown) => unknown>>();
    className = '';
    hidden = false;
    disabled = false;
    text = '';
    tabIndex = 0;
    [key: string]: unknown;
    constructor(readonly tag: string) {}
    append(...nodes: (Node | string | null | undefined)[]) {
      for (const n of nodes) {
        if (n == null) continue;
        const node = typeof n === 'string' ? Object.assign(new Node('#text'), { text: n }) : n;
        node.parent = this;
        this.kids.push(node);
      }
    }
    get firstChild() { return this.kids[0] ?? null; }
    removeChild(node: Node) { this.kids.splice(this.kids.indexOf(node), 1); node.parent = null; return node; }
    get textContent(): string { return this.tag === '#text' ? this.text : this.kids.map((k) => k.textContent).join(''); }
    set textContent(value: string) { this.kids = []; this.append(value); }
    setAttribute(name: string, value: string) {
      this.attrs.set(name, value);
      if (name === 'tabindex') this.tabIndex = Number(value);
    }
    getAttribute(name: string) { return this.attrs.get(name) ?? null; }
    removeAttribute(name: string) { this.attrs.delete(name); }
    addEventListener(type: string, fn: (event: unknown) => unknown) {
      this.handlers.set(type, [...(this.handlers.get(type) ?? []), fn]);
    }
    fire(type: string, event: Record<string, unknown> = {}) {
      for (const fn of this.handlers.get(type) ?? []) fn({ target: this, ...event });
    }
    focus() { (globalThis as unknown as { document: { activeElement: unknown } }).document.activeElement = this; }
    get classList() {
      const self = this;
      const parts = () => self.className.split(' ').filter(Boolean);
      return {
        toggle(cls: string, force?: boolean) {
          const want = force ?? !parts().includes(cls);
          self.className = parts().filter((c) => c !== cls).concat(want ? [cls] : []).join(' ');
          return want;
        },
        contains: (cls: string) => parts().includes(cls),
        add: (cls: string) => { if (!parts().includes(cls)) self.className = parts().concat(cls).join(' '); },
        remove: (cls: string) => { self.className = parts().filter((c) => c !== cls).join(' '); },
      };
    }
    all(tag: string): Node[] { return this.kids.flatMap((k) => (k.tag === tag ? [k, ...k.all(tag)] : k.all(tag))); }
    byClass(cls: string): Node | null {
      for (const k of this.kids) {
        if (k.className.split(' ').includes(cls)) return k;
        const found = k.byClass(cls);
        if (found) return found;
      }
      return null;
    }
  }
  const g = globalThis as unknown as Record<string, unknown>;
  const saved: Record<string, unknown> = {};
  const GLOBALS = ['document', 'DOMParser', 'matchMedia', 'getComputedStyle', 'requestAnimationFrame'];
  beforeEach(() => { for (const k of GLOBALS) saved[k] = g[k]; });
  afterEach(() => { for (const k of GLOBALS) { if (saved[k] === undefined) delete g[k]; else g[k] = saved[k]; } });

  /** Mounts the reader on a three-scene script, scrolled into the second
   *  scene, and loads the frame. Hands back the rail's buttons. */
  async function railOf() {
    g.document = {
      createElement: (tag: string) => new Node(tag),
      createTextNode: (value: string) => Object.assign(new Node('#text'), { text: value }),
      documentElement: {},
      activeElement: null,
    };
    g.DOMParser = class {
      parseFromString() { return { querySelector: () => null, documentElement: { outerHTML: '<html></html>' } }; }
    };
    g.matchMedia = () => ({ matches: false, addEventListener: () => {} });
    g.getComputedStyle = () => ({ getPropertyValue: () => '' });
    g.requestAnimationFrame = () => 0;
    const scenes = ['sc-001', 'sc-002', 'sc-003'].map((id, i) => ({
      id,
      offsetTop: i * 100,
      offsetHeight: 100,
      querySelector: () => ({ childNodes: [{ nodeType: 3, nodeValue: `INT. ROOM ${i + 1} - DAY` }] }),
      scrollIntoView: () => {},
    }));
    const doc = {
      body: { childElementCount: 3 },
      querySelectorAll: () => scenes,
      getElementById: (id: string) => scenes.find((s) => s.id === id) ?? null,
      adoptedStyleSheets: [],
    };
    const reader = await import(`${join(UI, 'read.js')}?rove`);
    const pane = new Node('section');
    reader.mount(pane, { state: { script: { title: 'Field Station', previewHtml: '<html>scenes</html>' }, reducedMotion: true } });
    const frame = pane.all('iframe')[0];
    Object.assign(frame, {
      contentDocument: doc,
      contentWindow: { scrollY: 150, CSSStyleSheet: class { replaceSync() {} }, scrollTo: () => {} },
      clientHeight: 500,
    });
    frame.fire('load');
    const rail = pane.byClass('scene-rail')!;
    return { rail, buttons: rail.all('button') };
  }

  test('the list is one Tab stop, and it is the scene the reader is in', async () => {
    const { buttons } = await railOf();
    expect(buttons.length).toBe(3);
    expect(buttons.map((b) => b.tabIndex)).toEqual([-1, 0, -1]);
    expect(buttons[1].getAttribute('aria-current')).toBe('true');
  });

  test('Up, Down, Home and End move the stop and the focus between scenes', async () => {
    const { rail, buttons } = await railOf();
    const press = (key: string) => {
      let prevented = false;
      const target = (g.document as { activeElement: Node }).activeElement ?? buttons[1];
      rail.fire('keydown', { key, target, preventDefault: () => { prevented = true; } });
      return prevented;
    };
    buttons[1].focus();
    expect(press('ArrowDown')).toBe(true);
    expect((g.document as { activeElement: unknown }).activeElement).toBe(buttons[2]);
    expect(buttons.map((b) => b.tabIndex)).toEqual([-1, -1, 0]);
    press('Home');
    expect((g.document as { activeElement: unknown }).activeElement).toBe(buttons[0]);
    expect(buttons.map((b) => b.tabIndex)).toEqual([0, -1, -1]);
    press('End');
    expect(buttons.map((b) => b.tabIndex)).toEqual([-1, -1, 0]);
    press('ArrowUp');
    expect((g.document as { activeElement: unknown }).activeElement).toBe(buttons[1]);
    // Enter and Space are the button's own: the rail leaves them alone.
    expect(press('Enter')).toBe(false);
    expect(press(' ')).toBe(false);
  });
});

describe('the Read surface', () => {
  // The real read.js, mounted on the fake page, with a frame whose document
  // the test builds and whose window records what the reader asks of it. The
  // frame's window carries its OWN CSSStyleSheet and IntersectionObserver,
  // distinct from the parent's, so a sheet or an observer built in the wrong
  // realm shows up as the wrong class.
  const ENGINE_CSS = 'section.scene { margin: 0 0 1em; }';
  const PREVIEW = `<?xml version="1.0"?><html><head><style>${ENGINE_CSS}</style></head><body>scenes</body></html>`;

  class FrameSheet { text = ''; replaceSync(t: string) { this.text = t; } }
  class ParentSheet { text = ''; replaceSync(t: string) { this.text = t; } }

  let page: FakePage | null = null;
  let copies = 0;
  afterEach(async () => { await page?.close(); page = null; });

  /** A frame document of `n` scenes, `tall` px each; a heading may carry the
   *  engine's floated page marker, the way it does when a scene opens a page. */
  function frameDoc(n: number, tall: number) {
    const doc = new FakeDocument();
    for (let i = 0; i < n; i += 1) {
      const section = doc.createElement('section');
      section.className = 'scene';
      section.id = `sc-00${i + 1}`;
      section.offsetTop = i * tall;
      section.offsetHeight = tall;
      const heading = doc.createElement('h2');
      heading.className = 'scene-heading';
      if (i === 1) {
        const marker = doc.createElement('span');
        marker.className = 'page-marker';
        marker.append('2.');
        heading.append(marker);
      }
      heading.append(`INT. ROOM ${i + 1} - DAY`);
      section.append(heading);
      doc.body.append(section);
    }
    return doc;
  }

  async function mountReader(options: { tokens?: Record<string, string> } = {}) {
    page = fakePage();
    const tokens = { '--ink': 'oklch(0.1 0 0)', '--paper': 'oklch(0.9 0 0)', ...options.tokens };
    const parsed: Array<{ html: string; type: string }> = [];
    const dark: Array<() => void> = [];
    const media: string[] = [];
    const g = withGlobals();
    g.set('CSSStyleSheet', ParentSheet);
    g.set('getComputedStyle', () => ({ getPropertyValue: (name: string) => tokens[name as keyof typeof tokens] ?? '' }));
    g.set('requestAnimationFrame', (fn: () => void) => { fn(); return 0; });
    g.set('matchMedia', (query: string) => {
      media.push(query);
      return { matches: false, addEventListener: (_: string, fn: () => void) => dark.push(fn) };
    });
    // The window's parser, recording what it was handed: a reader that
    // parsed the preview some other way would never call it.
    g.set('DOMParser', class {
      parseFromString(html: string, type: string) {
        parsed.push({ html, type });
        let css: string | null = (html.match(/<style>([\s\S]*?)<\/style>/) ?? [])[1] ?? null;
        return {
          querySelector: (s: string) => (s === 'style' && css !== null
            ? { textContent: css, remove: () => { css = null; } } : null),
          get documentElement() {
            return { outerHTML: html.replace(/<\?xml[^>]*\?>/, '').replace(/<style>[\s\S]*?<\/style>/, css === null ? '' : `<style>${css}</style>`) };
          },
        };
      }
    });
    const restoreExtra = page.close;
    page.close = async () => { g.restore(); await restoreExtra(); };

    copies += 1;
    const reader = await import(`${join(UI, 'read.js')}?mounted-${copies}`);
    const pane = page.pane();
    pane.id = 'surface-read';
    const script = { title: 'Field Station', author: 'A. Writer', previewHtml: PREVIEW };
    reader.mount(pane, { state: { script, reducedMotion: true }, convertAgain: () => {} });
    const frame = pane.querySelector('iframe')!;
    const scrolls: number[][] = [];
    const observers: Array<{ observed: FakeNode[]; fire: () => void; disconnected: boolean }> = [];
    const frameListeners: string[] = [];
    const win = {
      scrollY: 0,
      CSSStyleSheet: FrameSheet,
      IntersectionObserver: class {
        observed: FakeNode[] = [];
        disconnected = false;
        constructor(readonly fire: () => void) { observers.push(this); }
        observe(node: FakeNode) { this.observed.push(node); }
        disconnect() { this.disconnected = true; }
      },
      scrollTo(_x: number, y: number) { scrolls.push([0, y]); win.scrollY = y; },
      scrollBy(_x: number, dy: number) { win.scrollY += dy; },
      addEventListener: (type: string) => frameListeners.push(type),
    };
    /** Hands the frame a document, as the browser does when srcdoc loads. */
    const load = (doc: FakeDocument) => {
      Object.assign(frame, { contentDocument: doc, contentWindow: win });
      frame.fire('load');
    };
    frame.clientHeight = 500;
    return { reader, pane, frame, win, load, scrolls, observers, parsed, dark, media, tokens, frameListeners, page };
  }
  const marked = (pane: FakeNode) =>
    pane.querySelectorAll('.scene-link').filter((b) => b.getAttribute('aria-current') === 'true').map((b) => b.textContent);

  test('it shows the engine’s document, parsed by the window’s own parser, with the stylesheet lifted out', async () => {
    const { frame, parsed } = await mountReader();
    expect(parsed).toEqual([{ html: PREVIEW, type: 'text/html' }]);
    const srcdoc = frame.getAttribute('srcdoc')!;
    // A <style> left in srcdoc is silently refused by the CSP.
    expect(srcdoc).not.toContain('<style');
    expect(srcdoc).not.toContain('<?xml');
    expect(srcdoc.startsWith('<!doctype html><html>')).toBe(true);
    expect(srcdoc).toContain('scenes');
  });

  test('the frame adopts one sheet, built in its own realm: the faces, the engine’s rules untouched, then the theme', async () => {
    const { reader, frame, load, tokens } = await mountReader();
    const doc = frameDoc(3, 100);
    load(doc);
    expect(doc.adoptedStyleSheets.length).toBe(1);
    const sheet = doc.adoptedStyleSheets[0] as FrameSheet;
    // A parent-realm sheet is rejected when adopted into another document.
    expect(sheet).toBeInstanceOf(FrameSheet);
    expect(sheet.text).toBe(reader.sheetText(ENGINE_CSS, { ink: tokens['--ink'], paper: tokens['--paper'] }));
    expect(sheet.text).toContain('@font-face');
    expect(frame.getAttribute('sandbox')).toBe('allow-same-origin');
  });

  test('the frame’s theme follows the desktop’s when it changes', async () => {
    const { reader, load, dark, media, tokens } = await mountReader();
    const doc = frameDoc(3, 100);
    load(doc);
    expect(media).toContain('(prefers-color-scheme: dark)');
    tokens['--ink'] = 'oklch(0.95 0 0)';
    tokens['--paper'] = 'oklch(0.15 0 0)';
    for (const fn of dark) fn();
    expect((doc.adoptedStyleSheets[0] as FrameSheet).text)
      .toBe(reader.sheetText(ENGINE_CSS, { ink: 'oklch(0.95 0 0)', paper: 'oklch(0.15 0 0)' }));
  });

  test('the Tab stop is the stage around the frame, never the frame itself', async () => {
    const { pane, frame } = await mountReader();
    const { FOCUSABLE } = await import(join(UI, 'focus.js'));
    const stage = pane.querySelector('.script-stage')!;
    // This webview paints nothing for a focused iframe and fires no event
    // that would let the page paint it instead (measured), so the stop moved.
    expect(frame.matches(FOCUSABLE)).toBe(false);
    expect(stage.matches(FOCUSABLE)).toBe(true);
    expect(stage.contains(frame)).toBe(true);
    expect(stage.getAttribute('aria-label')).toBe('Field Station, the script');
  });

  test('the arrow keys on the stage scroll the frame', async () => {
    const { pane, load, win, scrolls } = await mountReader();
    load(frameDoc(3, 100));
    const stage = pane.querySelector('.script-stage')!;
    const press = (key: string) => {
      const event = makeEvent('keydown', { key });
      stage.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(press('ArrowDown')).toBe(true);
    expect(win.scrollY).toBe(60);
    expect(press('Home')).toBe(true);
    expect(scrolls.at(-1)).toEqual([0, 0]);
    // Tab belongs to the window, not to the frame.
    expect(press('Tab')).toBe(false);
  });

  test('the rail lists the engine’s own scenes, without the page numbers inside headings, and a click goes to the scene', async () => {
    const { pane, load } = await mountReader();
    const doc = frameDoc(3, 100);
    load(doc);
    const links = pane.querySelectorAll('.scene-rail .scene-link');
    expect(links.map((b) => b.textContent)).toEqual([
      'INT. ROOM 1 – DAY', 'INT. ROOM 2 – DAY', 'INT. ROOM 3 – DAY',
    ]);
    expect(pane.querySelector('.rail-count')!.textContent).toBe('3 scenes');
    links[2].click();
    // Reduced motion asked for, so the jump is not animated.
    expect(doc.getElementById('sc-003')!.scrolledIntoView).toEqual({ behavior: 'auto', block: 'start' });
  });

  test('the rail follows the reader through an observer built in the frame, never a scroll listener', async () => {
    const { pane, load, win, observers, frameListeners } = await mountReader();
    const doc = frameDoc(3, 100);
    load(doc);
    expect(observers.length).toBe(1);
    expect(observers[0].observed).toEqual(doc.querySelectorAll('section.scene'));
    // A sandboxed srcdoc frame delivers no scroll event to the parent
    // (measured), so a reader wired to one marks scene one forever.
    expect(frameListeners).not.toContain('scroll');
    expect(marked(pane)).toEqual(['INT. ROOM 1 – DAY']);
    win.scrollY = 250;
    observers[0].fire();
    expect(marked(pane)).toEqual(['INT. ROOM 3 – DAY']);
  });

  test('the rail keeps its own mark in view inside its own scroll', async () => {
    const { pane, load, win, observers } = await mountReader();
    load(frameDoc(3, 100));
    const rail = pane.querySelector('.scene-rail')!;
    rail.clientHeight = 100;
    pane.querySelectorAll('.scene-link').forEach((b, i) => { b.offsetTop = i * 300; b.offsetHeight = 30; });
    win.scrollY = 250;
    observers[0].fire();
    expect(rail.scrollTop).toBe(600 + 30 - 100);
  });

  test('coming back to the reader re-asserts the place, and really moves the frame', async () => {
    // After a display:none round trip this webview REPORTS the scroll it had
    // and PAINTS the document somewhere else, and a scrollTo() to where it
    // already claims to be does nothing (measured). So show() goes to the
    // top first, then to the place.
    const { reader, load, win, scrolls } = await mountReader();
    load(frameDoc(3, 100));
    win.scrollY = 150;
    reader.hide();
    scrolls.length = 0;
    reader.show();
    expect(scrolls).toEqual([[0, 0], [0, 150]]);
  });

  test('a re-render keeps the reader’s place across a reflow, even two re-renders in a row while hidden', async () => {
    // Tune re-renders while this pane is hidden, where the new document
    // cannot be measured and the frame reports scrollY 0. A second re-render
    // used to read that 0 against the OLD document's marks and throw the
    // reader back to the top of scene one.
    const { reader, frame, load, win, scrolls } = await mountReader();
    load(frameDoc(3, 100));
    win.scrollY = 150; // halfway into the second scene
    reader.hide();
    frame.clientHeight = 0;
    reader.render(PREVIEW.replace('scenes', 'scenes, bigger'));
    win.scrollY = 0;
    reader.render(PREVIEW.replace('scenes', 'scenes, bigger still'));
    load(frameDoc(3, 200)); // every scene now twice as tall
    frame.clientHeight = 500;
    reader.show();
    expect(scrolls.at(-1)).toEqual([0, 300]);
  });

  test('a resized window re-measures, because a reflow moves every scene', async () => {
    const { pane, load, win, page: p } = await mountReader();
    const doc = frameDoc(3, 100);
    load(doc);
    win.scrollY = 150;
    p.fireWindow('resize');
    expect(marked(pane)).toEqual(['INT. ROOM 2 – DAY']);
    // A narrower window: every scene grows, and the same scroll is now in
    // the first one. Only a fresh measure can know that.
    doc.querySelectorAll('section.scene').forEach((s, i) => { s.offsetTop = i * 300; s.offsetHeight = 300; });
    p.fireWindow('resize');
    expect(marked(pane)).toEqual(['INT. ROOM 1 – DAY']);
  });
});

describe('what the Read surface decides', () => {
  // The decisions, exercised directly, the same way the Convert surface's
  // are. The drawing over them rides on the live run.
  type Place = { id: string; into: number };
  type Mark = { id: string; top: number; height: number };
  type ReadModule = {
    FONT_CSS: string;
    PAPER_TOKENS: string[];
    OPENING: string;
    NO_SCENES: string;
    NOTICES: Record<string, { slug: string; line: string; way: string }>;
    deviceCss: (tokens: unknown) => string;
    paperFrom: (read: (name: string) => string) => Record<string, string>;
    PAGE_MARGIN: { block: string; inline: string };
    sheetText: (engineCss: unknown, tokens: unknown) => string;
    splitPreview: (html: string, parser: unknown) => { css: string; html: string };
    readerState: (script: unknown) => string;
    sceneLabel: (heading: unknown) => { place: string; time: string | null };
    PAGE_MARKER_CLASS: string;
    headingText: (node: unknown) => string;
    railEntries: (scenes: unknown) => { id: string; place: string; time: string | null }[];
    railCount: (total: number) => string;
    readerPlace: (marks: Mark[], scrollTop: number) => Place | null;
    scrollTarget: (marks: Mark[], place: Place | null) => number;
  };
  let reader: ReadModule;
  /** The engine's own preview document, from the engine, once. */
  let preview: string;

  beforeAll(async () => {
    reader = (await import(join(UI, 'read.js'))) as ReadModule;
    const root = new URL('..', import.meta.url).pathname;
    const proc = Bun.spawn(
      ['bun', join(root, 'src', 'cli.ts'), join(root, 'tests', 'fixtures', 'screenplay.pdf'),
        '--json', '--preview-inline', '--no-fountain',
        '-o', join(SCRATCH, 'read-surface.epub')],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    const [stdout] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    preview = answer.previewHtml;
    // The input every test below leans on: the real thing, with a real
    // stylesheet inside it and real scene sections.
    expect(preview).toContain('<style>');
    expect(preview).toContain('<section class="scene"');
  }, 60000);

  /** A reference HTML document, built with regexes rather than a DOM, that
   *  answers the four things splitPreview() asks of a parser. It is not a
   *  browser and proves nothing about one — what it proves is that
   *  splitPreview TAKES the stylesheet OUT and hands it back, which is the
   *  step the CSP makes load-bearing and the one an implementation that
   *  "nearly works" gets wrong. The live window is where the rest is
   *  settled; see the report for task 9. */
  function referenceParser() {
    return {
      parseFromString(source: string) {
        let style: string | null =
          (source.match(/<style>([\s\S]*?)<\/style>/) ?? [])[1] ?? null;
        const node = style === null ? null : {
          get textContent() { return style; },
          remove() { style = null; },
        };
        return {
          querySelector: (selector: string) => (selector === 'style' ? node : null),
          get documentElement() {
            return {
              get outerHTML() {
                return source
                  .replace(/<\?xml[\s\S]*?\?>\s*/, '')
                  .replace(/<!DOCTYPE[^>]*>\s*/i, '')
                  .replace(
                    /<style>[\s\S]*?<\/style>/,
                    style === null ? '' : `<style>${style}</style>`,
                  );
              },
            };
          },
        };
      },
    };
  }

  test('the stylesheet comes OUT of the document and comes back whole', () => {
    const split = reader.splitPreview(preview, referenceParser());
    // Out: a <style> left in place is silently dropped by the CSP and the
    // script renders unstyled.
    expect(split.html).not.toContain('<style');
    // Whole: the engine's rules, not a summary of them. Byte-for-byte
    // against what the engine actually wrote.
    const engineCss = preview.slice(preview.indexOf('<style>') + '<style>'.length,
      preview.indexOf('</style>'));
    expect(engineCss.length).toBeGreaterThan(1000);
    expect(split.css).toBe(engineCss);
    // The body survived the trip.
    expect(split.html).toContain('<section class="scene"');
    expect(split.html).toContain('class="dialogue-block"');
  });

  test('the document the frame gets is a standards-mode document', () => {
    const split = reader.splitPreview(preview, referenceParser());
    // The engine writes XHTML with an XML prolog in front. Left there, a
    // text/html parse puts the frame in quirks mode and the engine's
    // vertical rhythm is not what renders.
    expect(split.html.startsWith('<!doctype html>')).toBe(true);
    expect(split.html).not.toContain('<?xml');
    expect(split.html.toLowerCase().indexOf('<html')).toBe('<!doctype html>'.length);
  });

  test('an empty preview yields no document and no stylesheet', () => {
    const split = reader.splitPreview('', referenceParser());
    expect(split.css).toBe('');
    expect(split.html).toBe('<!doctype html>');
  });

  test('the engine’s stylesheet is adopted verbatim, and last but for the device', () => {
    const engineCss = preview.slice(preview.indexOf('<style>') + '<style>'.length,
      preview.indexOf('</style>'));
    const sheet = reader.sheetText(engineCss, { ink: 'black', paper: 'white' });
    // Verbatim: not reindented, not filtered, not re-prefixed.
    expect(sheet).toContain(engineCss);
    // The faces have to be declared before anything asks for them, and the
    // device layer has to come after the book's own sheet or the engine's
    // `html, body { padding: 0 }` wins and the script touches the frame edge.
    expect(sheet.indexOf('@font-face')).toBeLessThan(sheet.indexOf(engineCss));
    expect(sheet.indexOf(engineCss)).toBeLessThan(sheet.lastIndexOf('body {'));
    expect(sheet.indexOf('color: black')).toBeGreaterThan(sheet.indexOf(engineCss));
  });

  test('the reader declares every face the engine’s CSS names, from bundled files', () => {
    const engineCss = preview.slice(preview.indexOf('<style>'), preview.indexOf('</style>'));
    // Every family the engine names that is not a generic or a system
    // fallback must be declared here, or the frame renders it in something
    // else and the preview stops being the book.
    const named = new Set(
      [...engineCss.matchAll(/font-family:\s*([^;]+);/g)]
        .flatMap(([, list]) => list.split(',').map((f) => f.trim().replace(/["']/g, ''))),
    );
    expect(named.has('Courier Prime')).toBe(true);
    const bundled = readdirSync(join(UI, 'fonts'));
    // The families the window actually ships a file for: the filenames are
    // <family>-<weight>-<subset>.woff2, so "courier" (the system face the
    // engine names as a FALLBACK) is not one of them and "courier-prime" is.
    const shipped = new Set(bundled.map((file) => file.replace(/-\d+-[a-z-]+\.woff2$/, '')));
    expect(shipped.has('courier-prime')).toBe(true);
    expect(shipped.has('courier')).toBe(false);
    let checked = 0;
    for (const family of named) {
      if (!shipped.has(family.toLowerCase().replace(/\s+/g, '-'))) continue; // a fallback
      checked += 1;
      expect(`${family} is declared: ${reader.FONT_CSS.includes(`"${family}"`)}`)
        .toBe(`${family} is declared: true`);
    }
    expect(checked).toBeGreaterThan(0);
    // Every file it points at exists, and the weights the engine's CSS uses
    // are both there: a cue set in a synthesized bold is not the book.
    const urls = [...reader.FONT_CSS.matchAll(/url\(([^)]+)\)/g)]
      .map(([, url]) => url.replace(/["']/g, ''));
    expect(urls.length).toBeGreaterThanOrEqual(2);
    for (const url of urls) {
      expect(url).toMatch(/^fonts\/[a-z0-9-]+\.woff2$/);
      expect(bundled).toContain(url.slice('fonts/'.length));
    }
    for (const weight of ['400', '700']) {
      expect(`weight ${weight}: ${reader.FONT_CSS.includes(`font-weight: ${weight}`)}`)
        .toBe(`weight ${weight}: true`);
    }
    // And they are the same files the window itself uses — two declarations
    // of one typeface drifting apart is how the frame ends up in a different
    // Courier from the page around it.
    const windowFaces = [...read('style.css').matchAll(/url\((fonts\/courier-prime[^)]+)\)/g)]
      .map(([, url]) => url.replace(/["']/g, ''));
    expect([...urls].sort()).toEqual([...new Set(windowFaces)].sort());
  });

  test('the values read off the window are the values the frame is painted in', () => {
    // The weak form of this test asserted only that getPropertyValue appears
    // in the file, which an implementation that read the tokens and then
    // wrote a colour of its OWN would pass. So: drive the whole path — which
    // properties are asked for, what comes back, what reaches the sheet —
    // with values nothing could plausibly invent.
    const asked: string[] = [];
    const answers: Record<string, string> = {
      '--ink': 'oklch(0.113 0.019 83.4)',
      '--paper': 'oklch(0.971 0.013 84.1)',
    };
    const tokens = reader.paperFrom((name) => {
      asked.push(name);
      return answers[name] ?? '';
    });
    // The two the frame cannot get any other way, asked for by their real
    // names — a reader that asked for something else would paint a theme
    // nobody chose.
    expect(asked).toEqual(['--ink', '--paper']);
    expect(tokens).toEqual({ ink: answers['--ink'], paper: answers['--paper'] });

    const sheet = reader.sheetText('section.scene { margin: 0; }', tokens);
    // Exact, and the whole rule: an invented colour cannot hide beside the
    // real one, and an extra declaration cannot hide inside it.
    expect(sheet).toContain(
      `html { color: ${answers['--ink']}; background: ${answers['--paper']}; }`,
    );
    // Nothing in the emitted sheet paints with anything but what was read.
    const painted = [...sheet.matchAll(/(?:^|[;{\s])(?:color|background)\s*:\s*([^;}]+)/g)]
      .map(([, value]) => value.trim());
    expect(painted).toEqual([answers['--ink'], answers['--paper']]);
  });

  test('the page margin is the device’s, in the units the EPUB rule demands', () => {
    // The one size in this surface that is not a brand token, because the
    // brand's are rem values for the window's furniture and do not cross
    // into the frame at all. What it owes instead is the rule the engine's
    // own stylesheet is held to: horizontal in %, vertical in em, so it
    // scales with the reader's type size and stays proportional to the
    // column. Pinned so it cannot drift into px or rem unnoticed.
    expect(reader.PAGE_MARGIN.block).toMatch(/^[\d.]+em$/);
    expect(reader.PAGE_MARGIN.inline).toMatch(/^[\d.]+%$/);
    const sheet = reader.deviceCss({ ink: 'black', paper: 'white' });
    expect(sheet).toContain(
      `body { padding: ${reader.PAGE_MARGIN.block} ${reader.PAGE_MARGIN.inline}; }`,
    );
    // And the emitted padding is that constant, not a second copy of it.
    const paddings = [...sheet.matchAll(/padding:\s*([^;}]+)/g)].map(([, v]) => v.trim());
    expect(paddings).toEqual([`${reader.PAGE_MARGIN.block} ${reader.PAGE_MARGIN.inline}`]);
  });

  test('a token that did not resolve is left out, not written as blank', () => {
    // getPropertyValue() returns '' for a property that is not declared.
    // `color: ;` is a dropped declaration at best and a dropped RULE in a
    // stricter parser, which would take the page margin down with it.
    const partial = reader.deviceCss({ ink: '', paper: 'white' });
    expect(partial).not.toContain('color:');
    expect(partial).toContain('background:');
    expect(partial).toContain('padding');
    const none = reader.deviceCss({});
    expect(none).not.toContain('color:');
    expect(none).not.toContain('background:');
    // The margin is not a theme and does not depend on one.
    expect(none).toContain('padding');
    expect(reader.PAPER_TOKENS).toEqual(['ink', 'paper']);
  });

  test('a slugline is split at its LAST separator, not its first', () => {
    // "INT./EXT. DELIVERY VAN - MOVING - LATER" is a van that is moving,
    // later. Splitting at the first separator calls the time "MOVING -
    // LATER" and loses the place entirely.
    expect(reader.sceneLabel('INT./EXT. DELIVERY VAN - MOVING - LATER')).toEqual({
      place: 'INT./EXT. DELIVERY VAN - MOVING', time: 'LATER',
    });
    expect(reader.sceneLabel('INT. THE LAST VIDEO STORE - NIGHT')).toEqual({
      place: 'INT. THE LAST VIDEO STORE', time: 'NIGHT',
    });
    // An em or en dash is the same separator to a reader.
    expect(reader.sceneLabel('EXT. ROOF — DAWN').time).toBe('DAWN');
    // A hyphenated word is not a separator: it has no spaces around it.
    expect(reader.sceneLabel('INT. DRIVE-THRU')).toEqual({
      place: 'INT. DRIVE-THRU', time: null,
    });
    expect(reader.sceneLabel('INT. KITCHEN')).toEqual({ place: 'INT. KITCHEN', time: null });
    // Nothing to split, and nothing to lose: a half-empty split would print
    // a rail entry with no place in it.
    expect(reader.sceneLabel('- DAY')).toEqual({ place: '- DAY', time: null });
    expect(reader.sceneLabel('INT. HALL -')).toEqual({ place: 'INT. HALL -', time: null });
    // A scene with no heading is the run of script before the first
    // slugline, which the engine's own table of contents calls "Opening".
    expect(reader.sceneLabel('')).toEqual({ place: 'Opening', time: null });
    expect(reader.sceneLabel(undefined)).toEqual({ place: 'Opening', time: null });
    expect(reader.OPENING).toBe('Opening');
    // The word is the engine's, not this file's invention.
    const engine = readFileSync(
      join(new URL('..', import.meta.url).pathname, 'src', 'epub', 'html.ts'), 'utf8');
    expect(engine).toContain(`'${reader.OPENING}'`);
  });

  /** Just enough of a DOM node for headingText(): the two node types it
   *  reads, a class list, and children. Not a browser, and it proves nothing
   *  about one; it proves the walk skips what it should and keeps the rest. */
  const textNode = (value: string) => ({ nodeType: 3, nodeValue: value, childNodes: [] });
  const element = (className: string, ...children: unknown[]) => ({
    nodeType: 1,
    classList: { contains: (c: string) => className.split(' ').includes(c) },
    childNodes: children,
  });

  test('a page number the engine carries inside a heading never reaches the rail', () => {
    // The bug: since 0.7.0 page numbers are on by default, and when a scene
    // starts a new page the engine puts that page's marker INSIDE the scene
    // heading, as a floated span (registry 13a). Reading the heading's whole
    // text listed the scene as "2.EXT. FIELD STATION - DAY".
    const heading = element('scene-heading',
      element(reader.PAGE_MARKER_CLASS, textNode('2.')),
      textNode('EXT. FIELD STATION - DAY'));
    expect(reader.headingText(heading)).toBe('EXT. FIELD STATION - DAY');
    expect(reader.sceneLabel(reader.headingText(heading))).toEqual({
      place: 'EXT. FIELD STATION', time: 'DAY',
    });
    // A heading with no marker is read as it always was.
    expect(reader.headingText(element('scene-heading', textNode('INT. KITCHEN - NIGHT'))))
      .toBe('INT. KITCHEN - NIGHT');
    // Only the marker goes: other markup inside a heading keeps its words.
    expect(reader.headingText(element('scene-heading',
      textNode('INT. '), element('bold', textNode('THE VAULT')), textNode(' - DAY'))))
      .toBe('INT. THE VAULT - DAY');
    // A marker nested deeper, or one carrying more classes, is still a marker.
    expect(reader.headingText(element('scene-heading',
      element('wrap', element(`x ${reader.PAGE_MARKER_CLASS}`, textNode('14.'))),
      textNode('INT. FIELD STATION - NIGHT'))))
      .toBe('INT. FIELD STATION - NIGHT');
    // No heading at all is the Opening, as before.
    expect(reader.headingText(null)).toBe('');
    expect(reader.sceneLabel(reader.headingText(undefined)).place).toBe('Opening');
  });

  test('the marker’s class is the one the engine actually writes', () => {
    // Two files, one string: the engine names the span, the window skips it.
    // If the engine renamed the class, headingText() would skip nothing and
    // every page number would be back in the rail. Checked against what the
    // real engine wrote for the fixture, not against its source.
    expect(reader.PAGE_MARKER_CLASS).toBe('page-marker');
    expect(preview).toContain(`class="${reader.PAGE_MARKER_CLASS}"`);
  });

  test('the rail keeps the engine’s order and skips what it cannot link to', () => {
    const entries = reader.railEntries([
      { id: 'sc-001', heading: 'INT. ARCHIVE - NIGHT' },
      { id: '', heading: 'INT. NOWHERE - DAY' },
      { id: 'sc-002', heading: undefined },
      { id: 'sc-003', heading: 'EXT. DOCK - CONTINUOUS' },
    ]);
    // A section with no id cannot be scrolled to; a link to it would be a
    // dead one.
    expect(entries.map((e) => e.id)).toEqual(['sc-001', 'sc-002', 'sc-003']);
    expect(entries[1]).toEqual({ id: 'sc-002', place: 'Opening', time: null });
    expect(entries[2].time).toBe('CONTINUOUS');
    expect(reader.railEntries(null)).toEqual([]);
    expect(reader.railCount(1)).toBe('1 scene');
    expect(reader.railCount(12)).toBe('12 scenes');
    expect(reader.railCount(0)).toBe('0 scenes');
  });

  test('the reader’s place is a scene and a fraction, not a pixel offset', () => {
    // THE test this surface turns on. Tune re-renders on every knob, and a
    // knob that changes the type size changes every pixel in the document.
    // A remembered scrollY lands the reader somewhere else in the script.
    const before: Mark[] = [
      { id: 'sc-001', top: 0, height: 100 },
      { id: 'sc-002', top: 100, height: 200 },
      { id: 'sc-003', top: 300, height: 100 },
    ];
    // Halfway through the second scene.
    const place = reader.readerPlace(before, 200);
    expect(place).toEqual({ id: 'sc-002', into: 0.5 });

    // The same script, reflowed at a larger type size: every scene is twice
    // as tall. Halfway through scene two is now 400px down.
    const after: Mark[] = [
      { id: 'sc-001', top: 0, height: 200 },
      { id: 'sc-002', top: 200, height: 400 },
      { id: 'sc-003', top: 600, height: 200 },
    ];
    expect(reader.scrollTarget(after, place)).toBe(400);
    // What a remembered scrollY would have done: 200px, which is the TOP of
    // scene two — a scroll jump on every keystroke.
    expect(reader.scrollTarget(after, place)).not.toBe(200);
    // Unreflowed, it is exactly where it was.
    expect(reader.scrollTarget(before, place)).toBe(200);
  });

  test('every boundary of the place has an answer', () => {
    const marks: Mark[] = [
      { id: 'sc-001', top: 40, height: 100 },
      { id: 'sc-002', top: 140, height: 0 },
      { id: 'sc-003', top: 140, height: 60 },
    ];
    // Above the first section (the document can start with matter above it).
    expect(reader.readerPlace(marks, 0)).toEqual({ id: 'sc-001', into: 0 });
    // Exactly on a boundary belongs to the scene that starts there.
    expect(reader.readerPlace(marks, 140)!.id).toBe('sc-003');
    // Past the end clamps rather than running off.
    expect(reader.readerPlace(marks, 10_000)).toEqual({ id: 'sc-003', into: 1 });
    // A zero-height section cannot divide by itself.
    expect(Number.isFinite(reader.readerPlace([marks[1]], 140)!.into)).toBe(true);
    // Nothing measured yet, and a scroll position that is not a number.
    expect(reader.readerPlace([], 10)).toBe(null);
    expect(reader.readerPlace(marks, Number.NaN)).toEqual({ id: 'sc-001', into: 0 });
    // A place whose scene is gone — re-converted, and the heading with it —
    // is the top, which is the only honest answer.
    expect(reader.scrollTarget(marks, { id: 'sc-404', into: 0.5 })).toBe(0);
    expect(reader.scrollTarget(marks, null)).toBe(0);
    // A place is never a negative scroll.
    expect(reader.scrollTarget([{ id: 'a', top: 0, height: 10 }], { id: 'a', into: 0 }))
      .toBe(0);
  });

  test('there are three things the reader can be, and two of them are words', () => {
    expect(reader.readerState(null)).toBe('closed');
    expect(reader.readerState(undefined)).toBe('closed');
    // scriptFrom() normalises a missing preview to '', so a script that
    // converted without one must not land on an empty white frame.
    expect(reader.readerState({ previewHtml: '' })).toBe('blank');
    expect(reader.readerState({ previewHtml: '   \n ' })).toBe('blank');
    expect(reader.readerState({ previewHtml: undefined })).toBe('blank');
    expect(reader.readerState({ previewHtml: preview })).toBe('ready');

    // `blank` is reachable (the tab is live, the script converted, and the
    // pages did not come back), so it says what happened AND offers the way
    // on. `closed` is NOT reachable: the Read tab is off the bar whenever no
    // script is open ("Read, Settings and Send are off the bar until a
    // script converts" boots the window to show it), so there is
    // deliberately no copy for it.
    const notice = reader.NOTICES.blank;
    expect(notice.slug.length).toBeGreaterThan(0);
    expect(notice.line.length).toBeGreaterThan(40);
    expect(notice.way.length).toBeGreaterThan(0);
    expect(Object.keys(reader.NOTICES)).toEqual(['blank']);
    expect(reader.NO_SCENES.length).toBeGreaterThan(0);
  });
});

describe('the Tune surface', () => {
  const REPO = new URL('..', import.meta.url).pathname;

  /** An engine module with its prose taken out. Every claim below about
   *  "which knob does the engine read here" is a claim about CODE; a key
   *  named in a comment would otherwise answer for one that is not. */
  const engine = (...parts: string[]) =>
    readFileSync(join(REPO, ...parts), 'utf8')
      .replace(/\/\*[\s\S]*?\*\//g, ' ')
      .replace(/\/\/.*$/gm, ' ');

  let tune: Record<string, any>;
  beforeAll(async () => { tune = await import(join(UI, 'tune.js')); });

  test('the eighteen are the engine’s eighteen, in both directions', async () => {
    // A surface with seventeen has silently dropped one and a reader would
    // never know which; a nineteenth invented here would be sent to the
    // engine and discarded by resolveFormatOptions without a word.
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    expect([...tune.OPTION_KEYS].sort()).toEqual(Object.keys(DEFAULT_FORMAT_OPTIONS).sort());
    // Drawn once each: the same key in two groups would be two controls
    // fighting over one value.
    expect(new Set(tune.OPTION_KEYS).size).toBe(tune.OPTION_KEYS.length);
    // And each one is actually a control with words on it. A knob with an
    // empty label is a row of blank space next to a switch.
    for (const knob of tune.KNOBS) {
      expect(['toggle', 'range', 'choice'], `${knob.key} has no kind`).toContain(knob.kind);
      expect(knob.label.length, `${knob.key} has no label`).toBeGreaterThan(2);
      // The label names the thing; the sentence beside it does the
      // explaining. A label that has become documentation is the tell.
      expect(knob.label.length, `${knob.key}'s label is a paragraph`).toBeLessThan(42);
      if (knob.help !== undefined) {
        expect(knob.help.length, `${knob.key}'s help says nothing`).toBeGreaterThan(40);
      }
    }
  });

  test('every slider stops exactly where src/options.ts clamps', () => {
    // A slider that goes to 60 on a knob clamped at 30 lets a reader drag
    // into a value the engine will quietly discard. The bounds are READ OUT
    // of the engine rather than copied here, so this fails if either side
    // moves — a hand-written table would only fail if the surface did.
    const clamps = new Map<string, [number, number]>();
    for (const [, key, lo, hi] of engine('src', 'options.ts')
      .matchAll(/num\('(\w+)',\s*([\d.]+),\s*([\d.]+)\)/g)) {
      clamps.set(key, [Number(lo), Number(hi)]);
    }
    const ranges = tune.KNOBS.filter((knob: any) => knob.kind === 'range');
    expect(ranges.map((knob: any) => knob.key).sort()).toEqual([...clamps.keys()].sort());
    expect(ranges.length).toBe(4);
    for (const knob of ranges) {
      expect([knob.min, knob.max], `${knob.key} is not clamped where the engine clamps`)
        .toEqual(clamps.get(knob.key)!);
      expect(knob.step, `${knob.key} has no step`).toBeGreaterThan(0);
    }
  });

  test('every choice offered is one the engine will actually keep', async () => {
    // resolveFormatOptions falls back to the default for a value it does not
    // recognise. An option list with a fourth typeface in it would render,
    // save, and change nothing.
    const { resolveFormatOptions, DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const choices = tune.KNOBS.filter((knob: any) => knob.kind === 'choice');
    expect(choices.length).toBe(4);
    for (const knob of choices) {
      expect(knob.choices.length, `${knob.key} offers nothing`).toBeGreaterThan(1);
      for (const [value, label] of knob.choices) {
        expect(label.length, `${knob.key}'s ${value} has no words`).toBeGreaterThan(2);
        const resolved = resolveFormatOptions({ [knob.key]: value }) as unknown as Record<string, unknown>;
        expect(resolved[knob.key], `the engine discards ${knob.key}=${value}`).toBe(value);
      }
      // What the script already has must be selectable, or the control opens
      // showing a value it cannot return to.
      const current = (DEFAULT_FORMAT_OPTIONS as unknown as Record<string, unknown>)[knob.key];
      expect(knob.choices.map(([value]: [string, string]) => value),
        `${knob.key} cannot show its default`).toContain(current);
    }
  });

  test('which knobs can move the preview is the engine’s answer, not this file’s', () => {
    // The brief said two knobs are decided while the PDF is read. The engine
    // says four: src/fountain/serialize.ts consumes showPageMarkers and
    // dualDialogue as well, and re-rendering from the stored script with
    // either flipped was measured to produce a byte-identical document. Both
    // sat under "Content" and "Dialogue", where they would have looked live
    // and done nothing.
    //
    // So the grouping is derived from the engine and checked against it,
    // three ways and exhaustively:
    //   live      — read by the preview builder (src/epub/html.ts, css.ts)
    //   book      — read only where the BOOK is packaged (src/epub/build.ts)
    //   reconvert — read while the PDF becomes a script (fountain/serialize)
    const preview = engine('src', 'epub', 'html.ts') + engine('src', 'epub', 'css.ts');
    const packaging = engine('src', 'epub', 'build.ts');
    const serialize = engine('src', 'fountain', 'serialize.ts');

    for (const knob of tune.KNOBS) {
      expect(tune.EFFECTS, `${knob.key} claims an effect that does not exist`)
        .toContain(knob.effect);
      if (knob.effect === 'live') {
        expect(preview.includes(knob.key),
          `${knob.key} is called live but the preview never reads it`).toBe(true);
      } else {
        expect(preview.includes(knob.key),
          `${knob.key} DOES move the preview and is not called live`).toBe(false);
      }
      if (knob.effect === 'reconvert') {
        expect(serialize.includes(knob.key),
          `${knob.key} is not read while the PDF becomes a script`).toBe(true);
      } else {
        expect(serialize.includes(knob.key),
          `${knob.key} IS read while the PDF becomes a script`).toBe(false);
      }
      if (knob.effect === 'book') {
        expect(packaging.includes(knob.key), `${knob.key} changes nothing about the book`)
          .toBe(true);
      }
    }
    // The counts, so a classification that collapsed into one bucket cannot
    // pass the loop above by accident.
    const count = (effect: string) =>
      tune.KNOBS.filter((k: any) => k.effect === effect).length;
    expect([count('live'), count('book'), count('reconvert')]).toEqual([13, 1, 4]);
  });

  test('the knobs that cannot move the preview say so, in the reader’s words', () => {
    const group = tune.GROUPS.find((g: any) => g.effect === 'reconvert');
    expect(group?.knobs.map((k: any) => k.key).sort())
      .toEqual(['contdMode', 'dualDialogue', 'rejoinSplitDialogue', 'showPageMarkers']);
    // Not hidden, and not silently inert: the note is on the group, it says
    // both halves — saved, and not visible now — and it says them without
    // naming a file format at the reader.
    expect(group.note.length).toBeGreaterThan(80);
    expect(group.note).toContain('not change what you see now');
    expect(group.note.toLowerCase()).toContain('next time you convert');
    expect(group.note).not.toContain('.fountain');
    expect(group.note).not.toContain('serialize');
    // The one knob that changes the book without changing the preview owes
    // the same explanation, for the same reason.
    const title = tune.knobFor('includeTitlePage');
    expect(title.effect).toBe('book');
    expect(title.help).toContain('preview');
  });

  test('a knob that is doing nothing says why, rather than vanishing', () => {
    // The cue indents mean nothing while cues are centered. Hiding them
    // takes the explanation away with the control, and the reader is left
    // hunting for a knob that was there a moment ago.
    const indent = tune.knobFor('cueIndentPct');
    const centered = { cueAlignment: 'centered' };
    const indented = { cueAlignment: 'indented' };
    const reason = tune.idleReason(indent, centered);
    expect(reason).not.toBe(null);
    expect(reason.toLowerCase()).toContain('indented');
    expect(tune.idleReason(indent, indented)).toBe(null);
    expect(tune.idleReason(tune.knobFor('parentheticalIndentPct'), centered)).not.toBe(null);
    // Every other knob is always live: a reason that fired on a knob with no
    // condition would disable it forever.
    for (const knob of tune.KNOBS) {
      if (knob.needs !== undefined) continue;
      expect(tune.idleReason(knob, centered), `${knob.key} idles for no reason`).toBe(null);
    }
    // Nothing is removed from the surface by a setting: eighteen controls
    // are eighteen controls whatever the script is tuned to.
    expect(tune.GROUPS.flatMap((g: any) => g.knobs).length).toBe(18);
  });

  test('a control’s value becomes something the engine will keep, or nothing', () => {
    // A range arrives from the DOM as a STRING, and a string is exactly what
    // resolveFormatOptions throws away: the knob would appear to move and
    // the book would not change.
    const margin = tune.knobFor('dialogueSideMarginPct');
    expect(tune.coerceValue(margin, '17')).toBe(17);
    expect(tune.coerceValue(margin, '99')).toBe(30);
    expect(tune.coerceValue(margin, '-4')).toBe(0);
    expect(tune.coerceValue(margin, 'twelve')).toBe(null);
    const spacing = tune.knobFor('elementSpacingEm');
    expect(tune.coerceValue(spacing, '1.4')).toBe(1.4);
    expect(tune.coerceValue(spacing, '0')).toBe(0.4);
    const toggle = tune.knobFor('scenePageBreaks');
    expect(tune.coerceValue(toggle, true)).toBe(true);
    expect(tune.coerceValue(toggle, false)).toBe(false);
    // Not coerced: 'false' from an attribute is not a decision to turn it on.
    expect(tune.coerceValue(toggle, 'false')).toBe(null);
    const face = tune.knobFor('fontFamily');
    expect(tune.coerceValue(face, 'serif')).toBe('serif');
    expect(tune.coerceValue(face, 'comic')).toBe(null);
    expect(tune.coerceValue(null, 'serif')).toBe(null);
  });

  test('two knobs moved in one breath are both saved', () => {
    // The debounce cancels the pending save, so a change that only carried
    // "the knob that moved last" would SAVE only that one — and the engine,
    // which overlays a partial on what is stored, would answer with the
    // first change missing and the surface would redraw it away.
    let owed = {};
    expect(tune.isPending(owed)).toBe(false);
    owed = tune.mergePending(owed, 'dialogueSideMarginPct', 12);
    owed = tune.mergePending(owed, 'fontFamily', 'serif');
    expect(owed).toEqual({ dialogueSideMarginPct: 12, fontFamily: 'serif' });
    // The same knob twice is the later value, not two of them.
    owed = tune.mergePending(owed, 'dialogueSideMarginPct', 14);
    expect(owed).toEqual({ dialogueSideMarginPct: 14, fontFamily: 'serif' });
    expect(tune.isPending(owed)).toBe(true);
    expect(tune.isPending({})).toBe(false);
    expect(tune.isPending(undefined)).toBe(false);
  });

  test('an answer that is not a settings answer is refused, not drawn', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const whole = tune.settingsFrom({ settings: { ...DEFAULT_FORMAT_OPTIONS } });
    expect(whole).toEqual({ ...DEFAULT_FORMAT_OPTIONS });
    // One key short: seventeen sliders and one at NaN.
    const short: Record<string, unknown> = { ...DEFAULT_FORMAT_OPTIONS };
    delete short.cueIndentPct;
    expect(tune.settingsFrom({ settings: short })).toBe(null);
    // Right keys, wrong kinds — each would draw a control that cannot show
    // its own value.
    expect(tune.settingsFrom({ settings: { ...DEFAULT_FORMAT_OPTIONS, cueIndentPct: '33' } }))
      .toBe(null);
    expect(tune.settingsFrom({ settings: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'comic' } }))
      .toBe(null);
    expect(tune.settingsFrom({ settings: { ...DEFAULT_FORMAT_OPTIONS, justifyText: 'yes' } }))
      .toBe(null);
    expect(tune.settingsFrom({ settings: null })).toBe(null);
    expect(tune.settingsFrom({ settings: [] })).toBe(null);
    expect(tune.settingsFrom({})).toBe(null);
    expect(tune.settingsFrom(undefined)).toBe(null);
    // A nineteenth knob from a newer engine survives the round trip rather
    // than being quietly dropped out of the script's settings.
    const future = tune.settingsFrom({
      settings: { ...DEFAULT_FORMAT_OPTIONS, sceneNumberSide: 'left' },
    });
    expect(future.sceneNumberSide).toBe('left');
    // And it is a copy: the surface mutates its settings on every change.
    const original = { ...DEFAULT_FORMAT_OPTIONS };
    expect(tune.settingsFrom({ settings: original })).not.toBe(original);
  });

  test('a preset that would only half-apply is not offered', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const broken: Record<string, unknown> = { ...DEFAULT_FORMAT_OPTIONS };
    delete broken.fontFamily;
    const presets = tune.presetsFrom({
      presets: [
        { displayName: 'Kindle e-ink (6")', settings: { ...DEFAULT_FORMAT_OPTIONS } },
        { displayName: 'Half a preset', settings: broken },
        { displayName: '', settings: { ...DEFAULT_FORMAT_OPTIONS } },
      ],
    });
    expect(presets.map((p: any) => p.displayName)).toEqual(['Kindle e-ink (6")']);
    expect(presets[0].settings.fontFamily).toBe('courier');
    expect(tune.presetsFrom({})).toEqual([]);
    expect(tune.presetsFrom({ presets: 'kindle' })).toEqual([]);
  });

  test('a save waiting for the book says what it waits for, and nothing it is not', () => {
    // book-queue.js's labels for what holds a book. This page's own earlier
    // save, or a label it has no words for, is still "in a moment".
    expect(tune.statusFor('waiting', 'send')).toEqual({ line: 'Waiting for the send to finish…', bad: false });
    expect(tune.statusFor('waiting', 'copy').line).toBe('Waiting for the copy to finish…');
    expect(tune.statusFor('waiting', 'convert').line).toBe('Waiting for the conversion to finish…');
    // The send panel builds the Kindle file on its own when the result appears; a save
    // waiting on that says so rather than "in a moment".
    expect(tune.statusFor('waiting', 'kindle').line).toBe('Waiting for the Kindle file to finish building…');
    expect(tune.statusFor('waiting', 'kindle-mobi').line).toBe('Waiting for the Kindle file to finish building…');
    // Readers run side by side, so a save can wait on several at once
    // (book-queue.js's holders()): it names the one that takes longest.
    expect(tune.statusFor('waiting', ['copy', 'kindle']).line).toBe('Waiting for the Kindle file to finish building…');
    expect(tune.statusFor('waiting', ['check', 'send']).line).toBe('Waiting for the send to finish…');
    expect(tune.statusFor('waiting', ['check']).line).toBe(tune.STATUS.pending);
    expect(tune.statusFor('waiting', []).line).toBe(tune.STATUS.pending);
    expect(tune.statusFor('waiting', 'save').line).toBe(tune.STATUS.pending);
    expect(tune.statusFor('waiting', 'anything else').line).toBe(tune.STATUS.pending);
    for (const line of Object.values(tune.WAITING) as string[]) {
      expect(line).not.toContain('\u2014');
      expect(line).not.toContain('Saved');
    }
  });

  test('every turn this page can wait on has words, except the two that take a moment', async () => {
    // book-queue.js's table is the list of labels; a save that waits names
    // what it waits for, and "in a moment" is left for this page's own
    // earlier save and the send panel's date check, which are both quick.
    const { TURNS } = await import(join(UI, 'book-queue.js'));
    const unnamed = Object.keys(TURNS).filter((label) => tune.WAITING[label] === undefined).sort();
    expect(unnamed).toEqual(['check', 'save']);
  });

  test('the surface never says "saved" about something that was not', () => {
    expect(tune.statusFor('saved').line).toContain('Saved');
    expect(tune.statusFor('saved').bad).toBe(false);
    expect(tune.statusFor('pending').line).not.toContain('Saved');
    expect(tune.statusFor('saving').line).not.toContain('Saved.');
    expect(tune.statusFor('idle').line).toBe('');
    // A failure shows the engine's own sentence, and never the word.
    const failed = tune.statusFor('failed', 'cannot read the script: /gone.fountain');
    expect(failed.line).toBe('cannot read the script: /gone.fountain');
    expect(failed.bad).toBe(true);
    // An engine that broke its contract and failed without a sentence still
    // has to say something a person can act on.
    expect(tune.statusFor('failed', '   ').line).toBe(tune.NO_MESSAGE);
    expect(tune.statusFor('failed', undefined).line).toBe(tune.NO_MESSAGE);
    expect(tune.statusFor('failed').bad).toBe(true);
    // An unknown phase is silence, not a lie.
    expect(tune.statusFor('elsewhere').line).toBe('');
  });

  test('a read-out carries its unit, and never reads NaN', () => {
    const margin = tune.knobFor('dialogueSideMarginPct');
    expect(tune.displayValue(margin, 20)).toBe('20%');
    // Whole-numbered knobs do not grow a decimal point.
    expect(tune.displayValue(margin, '7')).toBe('7%');
    const spacing = tune.knobFor('elementSpacingEm');
    expect(tune.displayValue(spacing, 1)).toBe('1.0 em');
    expect(tune.displayValue(spacing, 1.25)).toBe('1.3 em');
    // Said in words: a dash standing in for a value is the one mark the
    // owner keeps out of every line the window shows.
    expect(tune.displayValue(spacing, undefined)).toBe('not set');
    expect(tune.displayValue(spacing, null)).toBe('not set');
    expect(tune.NOT_SET).toBe('not set');
  });

  test('two settings objects are compared by the eighteen, not by identity', () => {
    const a: Record<string, unknown> = { extra: 1 };
    for (const key of tune.OPTION_KEYS) a[key] = 1;
    a.fontFamily = 'serif';
    const b = { ...a, extra: 2 };
    expect(tune.sameSettings(a, b)).toBe(true);
    expect(tune.sameSettings(a, { ...a, fontFamily: 'sans' })).toBe(false);
    expect(tune.sameSettings(null, null)).toBe(true);
    expect(tune.sameSettings(a, null)).toBe(false);
  });


  test('the sentence beside a knob is the CURRENT one, in both directions', () => {
    // Found live, after idleReason() was tested hard and then bound wrongly:
    // the sentence was written at draw time and never corrected, so
    //   drawn centered, switched to Indented -> "Only when character cues
    //     are indented." stood beside a control that had just become live
    //   drawn indented, switched to Centered -> a greyed-out slider with no
    //     explanation at all, and nothing for aria-describedby to point at
    // Both are answers about (knob, settings), so both come from one
    // function and the test asks it directly.
    const indent = tune.knobFor('cueIndentPct');
    const centered = { cueAlignment: 'centered' };
    const indented = { cueAlignment: 'indented' };
    expect(tune.notesFor(indent, centered)).toEqual([
      'Only when character names are indented.',
    ]);
    // Live again: nothing to say, and nothing said.
    expect(tune.notesFor(indent, indented)).toEqual([]);

    // A knob with a help sentence keeps it either way, and the reason it is
    // idle comes FIRST when there is one — a reader wants to know why the
    // control is grey before they want to know what it is for.
    const speeches = tune.knobFor('keepSpeechesWhole');
    expect(tune.notesFor(speeches, centered)).toEqual([speeches.help]);
    const both = tune.notesFor(
      { ...indent, help: 'What it is for.' }, centered,
    );
    expect(both).toEqual(['Only when character names are indented.', 'What it is for.']);

    // A knob that can never explain itself draws no element to explain with;
    // one that can, draws one even while it is empty, or there is nothing to
    // rewrite when the setting it depends on moves.
    // A bare knob, constructed rather than named. This asserted against
    // scenePageBreaks until 2026-09-21, when the copy pass gave that knob a
    // help line and the test failed for a change that was purely an
    // improvement to it. The contract is about knobs with nothing to say, not
    // about which knob currently happens to be one.
    expect(tune.canExplain({ key: 'nothing-to-say', label: 'Bare', kind: 'toggle' })).toBe(false);
    expect(tune.canExplain(indent)).toBe(true);
    expect(tune.canExplain(speeches)).toBe(true);
    expect(tune.canExplain(null)).toBe(false);
    for (const knob of tune.KNOBS) {
      expect(tune.canExplain(knob), `${knob.key} can explain itself`)
        .toBe(tune.notesFor(knob, indented).length > 0 || knob.needs !== undefined);
    }
  });

  test('a save that failed is still owed, and is not claimed as done', () => {
    // flush() clears `pending` before it asks the engine — that is what makes
    // a knob moved DURING a save land in the next one. A failure therefore
    // has to put back what the attempt carried, or the knobs on screen keep
    // values the engine never stored and nothing is left to store them.
    const owed = { dialogueSideMarginPct: 12, fontFamily: 'serif' };
    expect(tune.restorePending(owed, {})).toEqual(owed);
    // Anything moved since the attempt began is the newer answer and wins.
    expect(tune.restorePending(owed, { fontFamily: 'sans' }))
      .toEqual({ dialogueSideMarginPct: 12, fontFamily: 'sans' });
    expect(tune.isPending(tune.restorePending(owed, {}))).toBe(true);
    // Every way out of a failed save goes through it (a non-settings answer,
    // a refused rebuild, a throw): driven in "a save that fails is still
    // owed" on the mounted page. And a failure never says "saved".
    expect(tune.statusFor('failed', 'the engine fell over').line).not.toContain('Saved');
  });

  test('emptyPaneMode: FAULT only when this draw is reporting a load that failed', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    // A freshly mounted pane, one whose script just changed, or one waiting
    // on a load in flight: all three are the identical call, `(null,
    // undefined)`, because nothing redraws between load() starting and it
    // answering. Reading, never fault, for as long as no answer has arrived.
    expect(tune.emptyPaneMode(null, undefined)).toBe('reading');
    // load()'s own two failure branches are the only calls that ever pass a
    // bad status alongside a null settings.
    expect(tune.emptyPaneMode(null, tune.statusFor('failed', 'the sidecar is gone'))).toBe('fault');
    expect(tune.emptyPaneMode(null, tune.statusFor('failed'))).toBe('fault');
    // Settings in hand is ready regardless of what status says.
    expect(tune.emptyPaneMode({ ...DEFAULT_FORMAT_OPTIONS }, undefined)).toBe('ready');
    expect(tune.emptyPaneMode({ ...DEFAULT_FORMAT_OPTIONS }, tune.statusFor('failed'))).toBe('ready');
  });

});

describe('the Settings page, mounted', () => {
  // tune.js mounted on the fake page and driven through its knobs, its
  // settle and the engine's answers. Each test imports its own copy, so a
  // settle or a save one test leaves behind cannot reach the next; every
  // script gets its own book, so the shared turn queue never couples two.
  let page: FakePage | null = null;
  let current: { scriptChanged(): void } | null = null;
  let copies = 0;
  afterEach(async () => {
    // A settle still counting down holds a restart off (holdEngine); a new
    // script is what lets it go, as in the window.
    current?.scriptChanged();
    current = null;
    await page?.close();
    page = null;
  });
  const SETTLE = () => new Promise((resolve) => setTimeout(resolve, 400));

  async function mountSettings(options: {
    engine?: (args: string[]) => unknown;
    settings?: Record<string, unknown>;
    script?: Record<string, unknown>;
    previewHtml?: string;
  } = {}) {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const settings = { ...DEFAULT_FORMAT_OPTIONS, ...options.settings };
    const answer = {
      ok: true, settings, defaults: { ...DEFAULT_FORMAT_OPTIONS }, appDefaults: { ...DEFAULT_FORMAT_OPTIONS },
      preset: null, presets: [], keepScriptSettings: true,
    };
    page = fakePage({
      respond: options.engine ?? ((args) => (args[0] === 'settings' && args.length === 3 ? answer : HOLD)),
    });
    copies += 1;
    const tune = await import(`${join(UI, 'tune.js')}?mounted-${copies}`);
    current = tune;
    const pane = page.pane();
    pane.id = 'surface-tune';
    const name = `book-${copies}`;
    const script = {
      title: 'Field Station',
      fountainPath: `/lib/${name}/${name}.fountain`,
      epubPath: `/lib/${name}/${name}.epub`,
      previewHtml: options.previewHtml,
      settings: null,
      ...options.script,
    };
    const ctx = { state: { script, reducedMotion: true }, goTo: () => {}, convertAgain: () => {} };
    // A saved rebuild re-renders the reader too, so the reader is mounted
    // beside it, as main.js mounts every surface.
    (await import(join(UI, 'read.js'))).mount(page.pane(), ctx);
    tune.mount(pane, ctx);
    // Not awaited here: a test that holds the read answers it itself.
    const shown = tune.show();
    await settle();
    const knob = (key: string) => pane.querySelector(`#knob-${key}`)!;
    const status = () => pane.querySelector('.tune-status');
    return { tune, pane, script, ctx, knob, status, shown, tauri: page.tauri, storage: page.storage, settings };
  }

  test('every control is tied to its label, and its explanation when it has one', async () => {
    const { tune, pane, knob } = await mountSettings({ settings: { cueAlignment: 'centered' } });
    for (const k of tune.KNOBS) {
      const input = knob(k.key);
      expect(`${k.key}: ${input === null}`).toBe(`${k.key}: false`);
      const label = pane.querySelectorAll('label').find((l) => l.getAttribute('for') === input.id);
      expect(label?.textContent).toBe(k.label);
      const described = input.getAttribute('aria-describedby');
      if (described !== null) {
        const why = pane.getElementById(described)!;
        expect(why.hidden).toBe(false);
        expect(why.textContent.length).toBeGreaterThan(0);
      }
    }
    expect(pane.querySelector('.tune-status')!.getAttribute('role')).toBe('status');
  });

  test('a knob that depends on another one says why it is idle, and stops saying so the moment it is live', async () => {
    // Found live: the sentence was written at draw time and never corrected,
    // so a slider that had just become live kept "Only when character names
    // are indented." beside it, and one that had just gone idle had none.
    const { tune, pane, knob } = await mountSettings({ settings: { cueAlignment: 'centered' } });
    const indent = knob('cueIndentPct');
    const why = () => pane.getElementById('knob-cueIndentPct-why')!;
    const idleSentence = tune.idleReason(tune.knobFor('cueIndentPct'), { cueAlignment: 'centered' });
    expect(indent.disabled).toBe(true);
    expect(indent.closest('.knob')!.classList.contains('knob-idle')).toBe(true);
    expect([why().hidden, why().textContent]).toEqual([false, idleSentence]);
    expect(indent.getAttribute('aria-describedby')).toBe('knob-cueIndentPct-why');

    knob('cueAlignment').input('indented', 'change');
    expect(indent.disabled).toBe(false);
    expect(indent.closest('.knob')!.classList.contains('knob-idle')).toBe(false);
    expect(why().hidden).toBe(true);
    expect(indent.getAttribute('aria-describedby')).toBeNull();

    knob('cueAlignment').input('centered', 'change');
    expect(indent.disabled).toBe(true);
    expect(why().textContent).toBe(idleSentence);
    // Nothing is removed from the page by a setting.
    expect(pane.querySelectorAll('.knob').length).toBe(18);
  });

  test('a moved knob is saved through the engine once it settles, and the book rebuilt with the whole of the settings', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const { tune, pane, knob, script, tauri, storage, status } = await mountSettings();
    // Three quick moves are one save, of the last value: a dragged slider
    // is one conversion, not a hundred.
    for (const value of ['10', '12', '14']) knob('dialogueSideMarginPct').input(value);
    expect(status()!.textContent).toBe(tune.statusFor('pending').line);
    expect(tauri.callsTo('settings').length).toBe(1);
    await SETTLE();
    const saves = tauri.callsTo('settings').slice(1);
    expect(saves.map((c) => c.args)).toEqual([
      ['settings', script.fountainPath, '--json', '--set', '{"dialogueSideMarginPct":14}'],
    ]);
    saves[0].answer({ ok: true, settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 14 } });
    await settle();
    // The rebuild writes the library book back in place, from the stored
    // script, with every setting the script now has.
    const rebuild = tauri.calls.find((c) => c.args[0] === script.fountainPath)!;
    expect(rebuild.args.slice(0, 5)).toEqual([script.fountainPath, '--json', '--preview-inline', '-o', script.epubPath]);
    expect(JSON.parse(rebuild.args[rebuild.args.indexOf('--options-json') + 1]))
      .toEqual({ ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 14 });
    rebuild.answer({ ok: true, previewHtml: '' });
    await settle();
    expect(status()!.textContent).toContain('Saved');
    // Its own storage is the engine's sidecar, not the window's.
    expect(storage.dump()).toEqual({});
    expect(pane.textContent).not.toContain('null');
  });

  test('a save that fails is still owed: the next save carries it again, and nothing claims "Saved"', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const { tune, knob, tauri, status } = await mountSettings();
    const saves = () => tauri.callsTo('settings').filter((c) => c.args.includes('--set'));
    const owed = (call: EngineCall) => JSON.parse(call.args[call.args.indexOf('--set') + 1]);

    // A save answered with something that is not a settings answer.
    knob('dialogueSideMarginPct').input('12');
    await SETTLE();
    saves()[0].answer({ ok: false, error: { code: 'internal', message: 'disk full' } });
    await settle();
    expect(status()!.textContent).toBe(tune.NO_MESSAGE);
    expect(status()!.classList.contains('bad')).toBe(true);

    // The next change carries the first one again.
    knob('fontFamily').input('serif', 'change');
    await SETTLE();
    expect(owed(saves()[1])).toEqual({ dialogueSideMarginPct: 12, fontFamily: 'serif' });
    // Stored, but the rebuild was refused: the book is not what the screen
    // says, so the settings are owed again and the engine's sentence shown.
    saves()[1].answer({ ok: true, settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 12, fontFamily: 'serif' } });
    await settle();
    tauri.pending().find((c) => c.args[1] === '--json' && c.args.includes('-o'))!
      .answer({ ok: false, error: { code: 'internal', message: 'The book could not be rebuilt.' } });
    await settle();
    expect(status()!.textContent).toBe('The book could not be rebuilt.');
    expect(status()!.textContent).not.toContain('Saved');

    // A thrown call is the same.
    knob('elementSpacingEm').input('1.4');
    await SETTLE();
    expect(owed(saves()[2])).toEqual({ dialogueSideMarginPct: 12, fontFamily: 'serif', elementSpacingEm: 1.4 });
    saves()[2].fail('the engine stopped');
    await settle();
    expect(status()!.textContent).toBe('the engine stopped');
  });

  test('a save in flight when another script opens paints nothing of the old script', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const { tune, pane, knob, tauri, ctx } = await mountSettings();
    knob('dialogueSideMarginPct').input('12');
    await SETTLE();
    const save = tauri.callsTo('settings').find((c) => c.args.includes('--set'))!;
    // Another script opens while the engine is still saving the first.
    ctx.state.script = {
      title: 'Second', fountainPath: '/lib/second/second.fountain', epubPath: '/lib/second/second.epub',
      previewHtml: undefined, settings: null,
    };
    tune.scriptChanged();
    save.answer({ ok: true, settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 12 } });
    await settle();
    // No rebuild of the first book, and the page shows the second script
    // waiting to be read, not the first one's knobs.
    expect(tauri.calls.filter((c) => c.args.includes('-o'))).toEqual([]);
    expect(pane.querySelector('.tune-head .caption')!.textContent).toBe(tune.READING);
    expect(pane.querySelectorAll('.knob-group-title')).toEqual([]);
  });

  test('each way a settings read can fail shows the fault screen, and a later visit tries again', async () => {
    const { tune, pane, tauri } = await mountSettings({ engine: () => HOLD });
    const fault = () => pane.querySelector('.fault-body-block .caption.bad')?.textContent ?? null;
    tauri.callsTo('settings')[0].fail('the sidecar could not be read');
    await settle();
    expect(fault()).toBe('the sidecar could not be read');
    // Shown again: asked again, because the failure may have been passing.
    const again = tune.show();
    await settle();
    expect(tauri.callsTo('settings').length).toBe(2);
    tauri.callsTo('settings')[1].answer({ ok: true, settings: { malformed: true } });
    await again;
    expect(fault()).toBe(tune.NO_MESSAGE);
    current?.scriptChanged();
    current = null;
    await page!.close();
    page = null;

    const noSource = await mountSettings({ script: { fountainPath: null } });
    await noSource.shown;
    expect(noSource.pane.querySelector('.fault-body-block .caption.bad')!.textContent).toBe(noSource.tune.FAULT.line);
    expect(noSource.tauri.calls).toEqual([]);
  });

  test('the preview is dressed by the reader’s own dresser: the engine’s sheet, adopted in the preview frame’s realm', async () => {
    // The CSP dance that gets the engine's stylesheet into a frame must
    // exist once. Settings builds no stylesheet of its own: the sheet in its
    // preview is exactly the one read.js's sheetText makes.
    const css = 'section.scene { margin: 0; }';
    const built: string[] = [];
    class ParentSheet { constructor() { built.push('parent'); } replaceSync() {} }
    class FrameSheet { text = ''; replaceSync(t: string) { this.text = t; } }
    const g = withGlobals();
    g.set('DOMParser', class {
      parseFromString(html: string) {
        let style: string | null = css;
        return {
          querySelector: (s: string) => (s === 'style' && style !== null ? { textContent: style, remove: () => { style = null; } } : null),
          get documentElement() { return { outerHTML: html.replace(/<style>[\s\S]*?<\/style>/, '') }; },
        };
      }
    });
    try {
      const { tune, pane, shown } = await mountSettings({ previewHtml: `<html><head><style>${css}</style></head><body>x</body></html>` });
      await shown;
      // A parent-realm sheet built anywhere in a whole draw would show here.
      g.set('CSSStyleSheet', ParentSheet);
      tune.scriptChanged();
      await tune.show();
      const frame = pane.querySelector('.tune-preview iframe')!;
      expect(frame.getAttribute('srcdoc')).not.toContain('<style');
      expect(frame.getAttribute('sandbox')).toBe('allow-same-origin');
      const doc = new FakeDocument();
      doc.body.append('x');
      Object.assign(frame, { contentDocument: doc, contentWindow: { CSSStyleSheet: FrameSheet } });
      frame.fire('load');
      const reader = await import(join(UI, 'read.js'));
      expect(doc.adoptedStyleSheets.length).toBe(1);
      expect(doc.adoptedStyleSheets[0]).toBeInstanceOf(FrameSheet);
      expect((doc.adoptedStyleSheets[0] as FrameSheet).text).toBe(reader.sheetText(css, { ink: '', paper: '' }));
      expect(built).toEqual([]);
    } finally {
      g.restore();
    }
  });
});

describe('the Tune surface: app defaults for new scripts', () => {
  let tune: Record<string, any>;
  let DEFAULT_FORMAT_OPTIONS: any;
  beforeAll(async () => {
    tune = await import(join(UI, 'tune.js'));
    ({ DEFAULT_FORMAT_OPTIONS } = await import('../src/options'));
  });

  // ---- the pure decisions, driven directly -------------------------------

  test('appDefaultsFrom and shippedDefaultsFrom read the settings answer through settingsFrom', () => {
    const answer = {
      settings: { ...DEFAULT_FORMAT_OPTIONS },
      defaults: { ...DEFAULT_FORMAT_OPTIONS },
      appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' },
    };
    expect(tune.appDefaultsFrom(answer)).toEqual({ ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' });
    expect(tune.shippedDefaultsFrom(answer)).toEqual({ ...DEFAULT_FORMAT_OPTIONS });
    // Same rejection settingsFrom already applies to a script's own
    // settings: a short or malformed object is not silently accepted here
    // either, because this window would otherwise draw a caption about
    // knobs it never actually checked.
    expect(tune.appDefaultsFrom({ appDefaults: { fontFamily: 'serif' } })).toBeNull();
    expect(tune.appDefaultsFrom({})).toBeNull();
    expect(tune.appDefaultsFrom(undefined)).toBeNull();
    expect(tune.shippedDefaultsFrom({ defaults: null })).toBeNull();
  });

  test('appSettingsAfter reads formatDefaults and shippedDefaults from an app-settings --set answer, or its refusal', () => {
    const answer = {
      ok: true,
      formatDefaults: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 12 },
      shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
      customized: true,
    };
    expect(tune.appSettingsAfter(answer)).toEqual({
      ok: true,
      appDefaults: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 12 },
      shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    // The engine's own sentence, verbatim: the same rule every refusal on
    // this surface follows.
    expect(tune.appSettingsAfter({
      ok: false, error: { code: 'bad-settings', message: 'formatDefaults must be an object' },
    })).toEqual({ ok: false, message: 'formatDefaults must be an object' });
    // A broken contract, ok with nothing usable inside, or a refusal with no
    // sentence at all, falls back to the same NO_MESSAGE every other
    // refusal on this surface uses when the engine says nothing.
    expect(tune.appSettingsAfter({ ok: true })).toEqual({ ok: false, message: tune.NO_MESSAGE });
    expect(tune.appSettingsAfter({ ok: false })).toEqual({ ok: false, message: tune.NO_MESSAGE });
  });

  test('defaultsCaption and canResetDefaults compare knob by knob, not by JSON string', () => {
    const shipped = { ...DEFAULT_FORMAT_OPTIONS };
    expect(tune.defaultsCaption({ ...shipped }, shipped))
      .toBe('New scripts start from Screepub’s own defaults.');
    expect(tune.defaultsCaption({ ...shipped, fontFamily: 'serif' }, shipped))
      .toBe('New scripts start from your own defaults.');
    expect(tune.canResetDefaults({ ...shipped }, shipped)).toBe(false);
    expect(tune.canResetDefaults({ ...shipped, justifyText: !shipped.justifyText }, shipped)).toBe(true);
    // An app default this window could not validate is not something a
    // button can honestly offer to reset.
    expect(tune.canResetDefaults(null, shipped)).toBe(false);
    expect(tune.canResetDefaults(shipped, null)).toBe(false);
    // Same values, different KEY ORDER: two settings objects built by
    // rebuilding one property at a time (applyAll, a fresh engine answer)
    // are not guaranteed to insert their keys in the same order. A
    // comparison that stringified and compared would call these different;
    // sameSettings compares by key, so it must not.
    const reordered: Record<string, unknown> = {};
    for (const key of [...Object.keys(shipped)].reverse()) reordered[key] = shipped[key];
    expect(JSON.stringify(reordered)).not.toBe(JSON.stringify(shipped));
    expect(tune.defaultsCaption(reordered, shipped)).toBe('New scripts start from Screepub’s own defaults.');
    expect(tune.canResetDefaults(reordered, shipped)).toBe(false);
  });

  test('defaultsWriteArgs is the --set value for either button', () => {
    const mine = { ...DEFAULT_FORMAT_OPTIONS, cueAlignment: 'indented' };
    expect(tune.defaultsWriteArgs(mine)).toBe(JSON.stringify({ formatDefaults: mine }));
    expect(tune.defaultsWriteArgs(null)).toBe('{"formatDefaults":null}');
  });

  test('the caption and both button labels have no em dash', () => {
    for (const line of [
      tune.defaultsCaption({ ...DEFAULT_FORMAT_OPTIONS }, { ...DEFAULT_FORMAT_OPTIONS }),
      tune.defaultsCaption({ ...DEFAULT_FORMAT_OPTIONS, justifyText: true }, { ...DEFAULT_FORMAT_OPTIONS, justifyText: false }),
      tune.USE_DEFAULTS_LABEL, tune.RESET_DEFAULTS_LABEL,
      tune.USE_DEFAULTS_NOTE, tune.RESET_DEFAULTS_NOTE,
    ]) {
      expect(line).not.toContain('\u2014');
    }
  });

  test('defaultsWriteOutcome: stale is dropped, success is applied, a refusal is reported, neither ever mixed up', () => {
    const shipped = { ...DEFAULT_FORMAT_OPTIONS };
    const written = { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' };
    const okAnswer = { ok: true, formatDefaults: written, shippedDefaults: shipped };
    // The page moved to another script before the answer arrived: dropped,
    // whatever the answer said, era mismatch checked BEFORE the answer is
    // even read.
    expect(tune.defaultsWriteOutcome(1, 2, okAnswer, 'confirmed')).toEqual({
      applied: false, stale: true, message: '',
    });
    // Same era: a clean answer is applied, and carries the exact
    // confirmation message the caller asked for, not a generic one.
    expect(tune.defaultsWriteOutcome(1, 1, okAnswer, 'confirmed')).toEqual({
      applied: true, stale: false, message: 'confirmed',
      appDefaults: written, shippedDefaults: shipped,
    });
    // A refusal is reported, not applied, and the message is the engine's
    // own, not the confirmation the caller hoped for.
    const refusal = { ok: false, error: { message: 'formatDefaults must be an object' } };
    expect(tune.defaultsWriteOutcome(1, 1, refusal, 'confirmed')).toEqual({
      applied: false, stale: false, message: 'formatDefaults must be an object',
    });
  });

  // ---- drawing, driven through a minimal DOM and engine stub -------------
  //
  // Mutation testing found that most of what used to live here only checked
  // the SOURCE TEXT of drawDefaultsFoot and writeDefaults: whether a call
  // was spelled a certain way, not what happened when it ran. A mutant that
  // re-enabled the buttons before the engine call, or dropped the era check
  // on the success path, or never actually wrote the confirmation into the
  // note, passed every one of those. What follows drives the real mount(),
  // show() and a real button click, against a tiny local stand-in for the
  // DOM (desktop/ui/dom.js's whole surface is three generic calls: element
  // creation, event listeners, and text) and for the engine (Tauri's
  // invoke, with the answer held back so a test can inspect the pane before
  // and after it arrives).

  /** Enough of a DOM node for desktop/ui/dom.js's el()/clear()/text() to run
   *  against: attributes, children, event listeners, nothing else. No
   *  layout, no rendering; a structural stand-in, not a browser. */
  class FakeNode {
    tagName: string;
    isText = false;
    textValue = '';
    attrs = new Map<string, string>();
    kids: FakeNode[] = [];
    parent: FakeNode | null = null;
    handlers = new Map<string, Array<(event: unknown) => unknown>>();
    className = '';
    disabled = false;
    hidden = false;
    checked = false;
    value = '';

    constructor(tagName: string) { this.tagName = tagName; }

    setAttribute(name: string, value: string) { this.attrs.set(name, value); }
    getAttribute(name: string) { return this.attrs.has(name) ? this.attrs.get(name)! : null; }
    addEventListener(type: string, fn: (event: unknown) => unknown) {
      const list = this.handlers.get(type) ?? [];
      list.push(fn);
      this.handlers.set(type, list);
    }
    /** Fires the (one) registered handler for `type`, with a synthetic event
     *  whose `target` is this node itself: real dom.js handlers read
     *  `event.target.value` (a range or select) or `event.target.checked`
     *  (a checkbox), and this node already carries both. Returns whatever
     *  the handler returns, so a click on a button whose handler is an
     *  async function hands back the promise a test can resolve the engine
     *  stub into and then await. */
    dispatch(type: string): unknown {
      const [fn] = this.handlers.get(type) ?? [];
      return fn?.({ target: this });
    }
    click(): unknown { return this.dispatch('click'); }
    append(...nodes: Array<FakeNode | null | undefined>) {
      for (const n of nodes) {
        if (n == null) continue;
        n.parent = this;
        this.kids.push(n);
      }
    }
    get firstChild() { return this.kids[0] ?? null; }
    removeChild(child: FakeNode) {
      const i = this.kids.indexOf(child);
      if (i >= 0) this.kids.splice(i, 1);
      child.parent = null;
      return child;
    }
    replaceWith(...nodes: FakeNode[]) {
      const p = this.parent;
      if (!p) return;
      const i = p.kids.indexOf(this);
      if (i < 0) return;
      p.kids.splice(i, 1, ...nodes);
      for (const n of nodes) n.parent = p;
      this.parent = null;
    }
    get textContent(): string {
      return this.isText ? this.textValue : this.kids.map((k) => k.textContent).join('');
    }
    set textContent(value: string) {
      this.kids = [];
      const t = new FakeNode('#text');
      t.isText = true;
      t.textValue = String(value ?? '');
      t.parent = this;
      this.kids.push(t);
    }
    /** The first descendant carrying `cls`, depth first, or null. */
    find(cls: string): FakeNode | null {
      for (const k of this.kids) {
        if (k.className.split(' ').includes(cls)) return k;
        const found = k.find(cls);
        if (found) return found;
      }
      return null;
    }
    /** Every descendant with this tag name. */
    findTag(tag: string): FakeNode[] {
      const out: FakeNode[] = [];
      for (const k of this.kids) {
        if (k.tagName === tag) out.push(k);
        out.push(...k.findTag(tag));
      }
      return out;
    }
    /** Only `.toggle` is ever called on the real thing here (state(),
     *  say()), so only `.toggle` is implemented; add/remove/contains ride
     *  along on the same className string for anything that grows to need
     *  them later. */
    get classList() {
      const self = this;
      return {
        toggle(cls: string, force?: boolean) {
          const parts = self.className.split(' ').filter(Boolean);
          const has = parts.includes(cls);
          const want = force === undefined ? !has : force;
          self.className = parts.filter((c) => c !== cls).concat(want ? [cls] : []).join(' ');
          return want;
        },
        contains(cls: string) { return self.className.split(' ').filter(Boolean).includes(cls); },
      };
    }
  }

  function installFakeDocument() {
    (globalThis as unknown as { document: unknown }).document = {
      createElement: (tag: string) => new FakeNode(tag),
      createTextNode: (value: string) => {
        const t = new FakeNode('#text');
        t.isText = true;
        t.textValue = String(value);
        return t;
      },
    };
  }
  function removeFakeDocument() { delete (globalThis as unknown as { document?: unknown }).document; }

  type PendingCall = { args: string[]; resolve: (json: string) => void; reject: (err: unknown) => void };

  /** Routes every `runEngine` call this window's own argv builders can
   *  produce to a pending, manually-settled promise, exactly the shape
   *  desktop-ui.test.ts already stubs `window.__TAURI__.core.invoke` with
   *  elsewhere in this file (see "what runEngine does with the answer it is
   *  handed"), held back here instead of answered immediately, so a test
   *  can inspect the pane BEFORE the engine has answered, not only after. */
  function stubEngine() {
    const calls: PendingCall[] = [];
    (globalThis as unknown as { window: unknown }).window = {
      __TAURI__: {
        core: {
          invoke: (_cmd: string, payload: { args: string[] }) =>
            new Promise<string>((resolve, reject) => { calls.push({ args: payload.args, resolve, reject }); }),
        },
      },
    };
    return {
      calls,
      resolve(i: number, answer: unknown) { calls[i].resolve(JSON.stringify(answer)); },
      reject(i: number, message: unknown) { calls[i].reject(message); },
    };
  }
  function removeStubEngine() { delete (globalThis as unknown as { window?: unknown }).window; }

  function makeCtx(script: unknown) { return { state: { script }, goTo: () => {} }; }

  /** The two foot buttons, found by their exact label rather than by class
   *  (both share `btn-quiet`). Safe to call again after a write, which
   *  updates these same nodes in place rather than replacing them; only a
   *  FULL draw() (mount(), scriptChanged(), a fresh load) builds new ones,
   *  which is why every test re-queries rather than caching the result
   *  across one of those. */
  function footButtons(pane: FakeNode) {
    const buttons = pane.findTag('button');
    return {
      use: buttons.find((b) => b.textContent === tune.USE_DEFAULTS_LABEL) ?? null,
      reset: buttons.find((b) => b.textContent === tune.RESET_DEFAULTS_LABEL) ?? null,
    };
  }

  /** Moves a range knob the way a reader dragging its slider would: sets
   *  the input's value and fires the same 'input' event drawKnob's oninput
   *  listens for, which runs change() and updates tune.js's own `settings`
   *  in place. */
  function moveKnob(pane: FakeNode, key: string, value: string) {
    const input = pane.findTag('input').find((n) => n.getAttribute('id') === `knob-${key}`);
    if (input === undefined) throw new Error(`no knob input for ${key}`);
    input.value = value;
    input.dispatch('input');
  }

  /** A clean settings answer: this script's current settings, Screepub's
   *  shipped defaults, and (by default) app defaults equal to the shipped
   *  set, so Reset starts hidden unless a test asks for something else. */
  function settingsAnswer(overrides: Record<string, unknown> = {}) {
    return {
      ok: true,
      settings: { ...DEFAULT_FORMAT_OPTIONS },
      defaults: { ...DEFAULT_FORMAT_OPTIONS },
      appDefaults: { ...DEFAULT_FORMAT_OPTIONS },
      preset: null,
      presets: [],
      ...overrides,
    };
  }

  /** Mounts a fresh script, drains the settings load with `answer`, and
   *  returns the pane once the ready state (knobs and foot) is on screen.
   *  scriptChanged() before show() guarantees a clean module state
   *  regardless of what an earlier test in this describe block left behind:
   *  tune.js is one module instance for the whole file, so `era`,
   *  `settings`, `appDefaults` and the rest persist between tests unless
   *  something resets them, the same way a real script switch would. */
  async function mountReady(engine: ReturnType<typeof stubEngine>, overrides: Record<string, unknown> = {}) {
    const script = { fountainPath: '/scripts/demo.fountain', epubPath: null, previewHtml: undefined, settings: null };
    const ctx = makeCtx(script);
    const pane = new FakeNode('div');
    tune.mount(pane, ctx);
    mounted = true;
    tune.scriptChanged();
    const shown = tune.show();
    engine.resolve(engine.calls.length - 1, settingsAnswer(overrides));
    await shown;
    return { pane, ctx, script };
  }

  let engine: ReturnType<typeof stubEngine>;
  /** Whether this test mounted the pane through mountReady(). */
  let mounted = false;
  beforeEach(() => { installFakeDocument(); engine = stubEngine(); });
  afterEach(async () => {
    // tune.js's queue (withBook) is one module's for the whole run, so a
    // save left waiting on an engine call nobody answers would hold up every
    // later turn in it, the send panel's in send-panel-ui.test.ts included.
    // So whatever a test left unanswered is refused here, and a settle still
    // counting down (whose save would land in the next test's stub) is
    // cancelled by a script change, as it is in the window.
    for (let i = 0; i < engine.calls.length; i += 1) engine.resolve(i, { ok: false });
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (mounted) tune.scriptChanged();
    mounted = false;
    removeFakeDocument();
    removeStubEngine();
  });

  test('the pane reads, then shows the knobs and the defaults foot; a failed load shows the fault screen instead', async () => {
    const script = { fountainPath: '/scripts/demo.fountain', epubPath: null, previewHtml: undefined, settings: null };
    const ctx = makeCtx(script);
    const pane = new FakeNode('div');
    tune.mount(pane, ctx);
    tune.scriptChanged();
    // Before load() has answered: a quiet caption, not the fault screen and
    // not the knobs (M31: the reading branch falling through into them).
    expect(pane.textContent).toContain(tune.READING);
    expect(pane.find('fault-body-block')).toBeNull();
    expect(pane.find('tune-defaults')).toBeNull();

    const shown = tune.show();
    engine.resolve(0, settingsAnswer({ appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } }));
    await shown;

    expect(pane.find('fault-body-block')).toBeNull();
    expect(pane.textContent).not.toContain(tune.READING);
    const foot = pane.find('tune-defaults');
    expect(foot).not.toBeNull();
    expect(foot!.textContent).toContain('New scripts start from your own defaults.');
    // Placed at the foot: after the knob groups, inside the knobs column,
    // in the foot's own box (layout pass, 2026-09-29).
    const knobs = pane.find('tune-knobs')!;
    const box = pane.find('tune-foot')!;
    expect(box.kids.includes(foot!)).toBe(true);
    expect(knobs.kids.indexOf(box)).toBeGreaterThan(knobs.kids.findIndex((k) => k.tagName === 'DETAILS'));
  });

  test('a load that fails (a throw from the engine call) shows the fault screen, not the reading caption', async () => {
    const script = { fountainPath: '/scripts/demo.fountain', epubPath: null, previewHtml: undefined, settings: null };
    const ctx = makeCtx(script);
    const pane = new FakeNode('div');
    tune.mount(pane, ctx);
    tune.scriptChanged();
    const shown = tune.show();
    engine.reject(0, 'the sidecar could not be read');
    await shown;
    expect(pane.find('fault-body-block')).not.toBeNull();
    expect(pane.textContent).toContain('the sidecar could not be read');
    expect(pane.textContent).not.toContain(tune.READING);
  });

  test('the Reset button is hidden when the app defaults equal Screepub’s own, shown when they differ', async () => {
    const equal = await mountReady(engine);
    expect(footButtons(equal.pane).reset!.hidden).toBe(true);

    engine = stubEngine();
    const different = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    expect(footButtons(different.pane).reset!.hidden).toBe(false);
  });

  test('a malformed app-defaults answer draws no foot at all, rather than a wrong caption', async () => {
    const { pane } = await mountReady(engine, { appDefaults: { fontFamily: 'serif' } });
    expect(pane.find('tune-defaults')).toBeNull();
  });

  // The update restart's known gap, closed (spec 2026-09-23-update-notice-
  // and-window-drag-design.md, Part 3). Placed before any test here that
  // leaves a save's engine call unanswered: app.js's count is one module
  // for the whole file, and this test reads it.
  test('a moved knob holds a restart off through its settle, and its save call takes over from the hold', async () => {
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle();
    expect(app.engineBusy(), 'an earlier test left a counted engine call running').toBe(false);
    const { pane } = await mountReady(engine);

    moveKnob(pane, 'dialogueSideMarginPct', '27');
    // No engine call yet, and still busy: the settle is owed work.
    expect(engine.calls.length).toBe(1);
    expect(app.engineBusy()).toBe(true);
    expect(Bun.peek.status(app.whenIdle())).toBe('pending');

    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(2);
    expect(engine.calls[1].args[0]).toBe('settings');
    expect(app.engineBusy()).toBe(true);

    engine.resolve(1, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 } }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
    // The hold went when the call started, and the call has ended: nothing
    // is counted twice, and nothing is left holding a restart off.
    expect(app.engineBusy()).toBe(false);
  });

  test('a settle cancelled by a new script lets its hold go, and sends nothing', async () => {
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle();
    expect(app.engineBusy(), 'an earlier test left a counted engine call running').toBe(false);
    const { pane } = await mountReady(engine);
    moveKnob(pane, 'dialogueSideMarginPct', '27');
    expect(app.engineBusy()).toBe(true);
    tune.scriptChanged();
    expect(app.engineBusy()).toBe(false);
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(1);
    expect(app.engineBusy()).toBe(false);
  });

  // The Settings page and the send panel both write the library EPUB: a moved knob's
  // save rebuilds it in place (the reconvert, the argv with no verb of its
  // own), and the send panel's export can rebuild it on its MOBI rung. withBook() is the one queue both take their turn in.
  // Placed before the tests below that leave a save unanswered, because the
  // queue is one module's for the whole file, as app.js's count is.
  test('the book waits its turn: a moved knob’s save goes first, at once, and a knob moved meanwhile waits', async () => {
    const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const { pane, script } = await mountReady(engine);
    (script as { epubPath: string | null }).epubPath = '/scripts/demo.epub';
    const refused = { ok: false, error: { message: 'not what this test is about' } };
    const status = () => pane.find('tune-status')?.textContent ?? '';

    moveKnob(pane, 'dialogueSideMarginPct', '27');
    expect(engine.calls.length).toBe(1);
    let ran = false;
    let finish: (value: string) => void = () => {};
    const turn = inTurn('/scripts/demo.epub', 'send', () => {
      ran = true;
      return new Promise<string>((resolve) => { finish = resolve; });
    });
    // The settle is not waited out: its save is asked for now, ahead of
    // the turn that asked, and the turn waits for the rebuild behind it.
    await tick();
    expect(engine.calls.length).toBe(2);
    expect(engine.calls[1].args[0]).toBe('settings');
    expect(ran).toBe(false);
    engine.resolve(1, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 } }));
    await tick();
    expect(engine.calls.length).toBe(3);
    expect(engine.calls[2].args.slice(0, 5))
      .toEqual(['/scripts/demo.fountain', '--json', '--preview-inline', '-o', '/scripts/demo.epub']);
    expect(ran).toBe(false);
    engine.resolve(2, refused);
    await tick();
    expect(ran).toBe(true);

    // The other way round: a knob moved while the send holds the book is
    // saved (and the book rebuilt) only once the send is over, and the
    // page says what it is waiting for rather than "in a moment".
    moveKnob(pane, 'dialogueSideMarginPct', '28');
    expect(status()).toBe(tune.STATUS.pending);
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(3);
    expect(status()).toBe('Waiting for the send to finish…');
    finish('sent');
    expect(await turn).toBe('sent');
    await tick();
    expect(engine.calls.length).toBe(4);
    expect(engine.calls[3].args[0]).toBe('settings');
    expect(engine.calls[3].args.join(' ')).toContain('"dialogueSideMarginPct":28');
    expect(status()).toBe(tune.STATUS.saving);
    engine.resolve(3, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 28 } }));
    await tick();
    expect(engine.calls[4].args.slice(3, 5)).toEqual(['-o', '/scripts/demo.epub']);
    engine.resolve(4, refused);
    await tick();
  });

  test('a save queued for one script never runs another script’s changes when its turn comes', async () => {
    // A's save waits behind a send holding A's book. The reader opens B and
    // moves a knob there. When A's send ends, A's waiting save must not pick
    // up B's change and run it on A's turn: B's own settle saves it, on B's
    // book, and B's hold against a restart stays until then. scriptChanged()
    // already dropped what A owed and let A's hold go, so skipping A's save
    // loses nothing.
    const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
    const app = await import(join(UI, 'app.js'));
    await app.whenIdle();
    expect(app.engineBusy(), 'an earlier test left a counted engine call running').toBe(false);
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const { pane, ctx, script } = await mountReady(engine);
    (script as { epubPath: string | null }).epubPath = '/scripts/demo.epub';

    let free: () => void = () => {};
    const sending = inTurn('/scripts/demo.epub', 'send', () => new Promise<void>((resolve) => { free = resolve; }));
    moveKnob(pane, 'dialogueSideMarginPct', '27');
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(1); // A's save is waiting for the send

    // Script B, on the same page.
    const other = { fountainPath: '/scripts/other.fountain', epubPath: '/scripts/other.epub', previewHtml: undefined, settings: null };
    (ctx.state as { script: unknown }).script = other;
    tune.scriptChanged();
    const shown = tune.show();
    engine.resolve(1, settingsAnswer());
    await shown;
    moveKnob(pane, 'cueIndentPct', '31');
    expect(app.engineBusy()).toBe(true);

    free();
    await sending;
    await tick();
    // Nothing ran on A's turn: no settings call yet, and B's hold is what
    // still keeps a restart off.
    expect(engine.calls.length).toBe(2);
    expect(app.engineBusy()).toBe(true);

    // B's own settle saves B's change, once, on B's book.
    await new Promise((resolve) => setTimeout(resolve, 320));
    const saves = engine.calls.filter((call) => call.args[0] === 'settings' && call.args.includes('--set'));
    expect(saves.length).toBe(1);
    expect(saves[0].args[1]).toBe('/scripts/other.fountain');
    expect(saves[0].args.join(' ')).toContain('"cueIndentPct":31');
    expect(saves[0].args.join(' ')).not.toContain('dialogueSideMarginPct');
    engine.resolve(2, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, cueIndentPct: 31 } }));
    await tick();
    expect(engine.calls[3].args.slice(3, 5)).toEqual(['-o', '/scripts/other.epub']);
    engine.resolve(3, { ok: false, error: { message: 'not what this test is about' } });
    await tick();
  });

  test('a save made while the send panel builds the Kindle file waits for the build, and says so', async () => {
    // The build only READS the book, so Save the EPUB runs beside it; a save
    // here REWRITES the book, so it waits for the build to finish reading it,
    // and a KFX build is about half a minute, not "a moment".
    const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const { pane, script } = await mountReady(engine);
    (script as { epubPath: string | null }).epubPath = '/scripts/demo.epub';
    const status = () => pane.find('tune-status')?.textContent ?? '';
    let built: (value: string) => void = () => {};
    const building = inTurn('/scripts/demo.epub', 'kindle',
      () => new Promise<string>((resolve) => { built = resolve; }));
    const before = engine.calls.length;
    moveKnob(pane, 'dialogueSideMarginPct', '27');
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(before);
    expect(status()).toBe('Waiting for the Kindle file to finish building…');
    built('built');
    expect(await building).toBe('built');
    await tick();
    expect(engine.calls.length).toBe(before + 1);
    expect(engine.calls[before].args[0]).toBe('settings');
    expect(status()).toBe(tune.STATUS.saving);
    engine.resolve(before, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 } }));
    await tick();
    engine.resolve(before + 1, { ok: false, error: { message: 'not what this test is about' } });
    await tick();
  });

  test('another book’s turn does not hold this script’s save', async () => {
    // One queue per book: the old script's minute-long KFX export must not
    // hold the new script's saves, and an engine call that never answers
    // holds up its own book and nothing else.
    const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
    const { inTurn } = await import(join(UI, 'book-queue.js'));
    const { pane, script } = await mountReady(engine);
    (script as { epubPath: string | null }).epubPath = '/scripts/demo.epub';
    let finish: (value: string) => void = () => {};
    const other = inTurn('/scripts/an-older-script.epub', 'send',
      () => new Promise<string>((resolve) => { finish = resolve; }));
    moveKnob(pane, 'dialogueSideMarginPct', '27');
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(engine.calls.length).toBe(2);
    expect(engine.calls[1].args[0]).toBe('settings');
    engine.resolve(1, settingsAnswer({ settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 } }));
    await tick();
    expect(engine.calls[2].args.slice(3, 5)).toEqual(['-o', '/scripts/demo.epub']);
    engine.resolve(2, { ok: false, error: { message: 'not what this test is about' } });
    await tick();
    finish('sent');
    expect(await other).toBe('sent');
  });

  test('"Use these for new scripts" sends this script’s CURRENT settings, moved after the foot was drawn, not the shipped or app defaults', async () => {
    // Three distinct values in play, so a mutant that sends the wrong one
    // has nowhere to hide: shipped (20, DEFAULT_FORMAT_OPTIONS' own),
    // appDefaults (15, what a new script starts from today, about to be
    // overwritten), and this script's own setting at mount time (10). The
    // knob is then moved to a FOURTH value (27) after the foot is already
    // drawn, so a click has to read `settings` live rather than whatever
    // was captured when drawDefaultsFoot last ran (M10b) or send the
    // shipped set outright (M8).
    const { pane } = await mountReady(engine, {
      settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 10 },
      appDefaults: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 15 },
    });
    expect(footButtons(pane).reset!.hidden).toBe(false);
    moveKnob(pane, 'dialogueSideMarginPct', '27');

    const write = footButtons(pane).use!.click() as Promise<void>;
    // Before the engine has answered: both quiet, not just the one pressed
    // (a second click racing this one is exactly what disabling only the
    // clicked button would allow), and it asked app-settings, never the
    // settings( call this surface's own save path uses.
    expect(footButtons(pane).use!.disabled).toBe(true);
    expect(footButtons(pane).reset!.disabled).toBe(true);
    expect(engine.calls[1].args[0]).toBe('app-settings');
    expect(engine.calls[1].args).toContain(
      tune.defaultsWriteArgs({ ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 }),
    );

    engine.resolve(1, {
      ok: true, formatDefaults: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27 },
      shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    await write;

    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
    // Applied, not left showing the pre-write state: the confirmation
    // showed up, and the caption still says "your own" because 27 is not
    // the shipped 20 either, so Reset is still offered.
    expect(pane.find('tune-defaults')!.textContent).toContain(tune.USE_DEFAULTS_NOTE);
    expect(pane.find('tune-defaults')!.textContent).toContain('New scripts start from your own defaults.');
    expect(footButtons(pane).reset!.hidden).toBe(false);

    // Moving the knob armed schedule()'s own 300ms debounce for THIS
    // script's settings (unrelated to the write above); drained here,
    // inside this test's own engine stub, so the stray settings( call it
    // eventually makes lands here rather than corrupting a later test's
    // call-index assertions.
    await new Promise((resolve) => setTimeout(resolve, 320));
  });

  test('"Reset new scripts to Screepub’s defaults": a refusal is shown verbatim, and changes nothing', async () => {
    const { pane } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    const write = footButtons(pane).reset!.click() as Promise<void>;
    expect(engine.calls[1].args[0]).toBe('app-settings');
    expect(engine.calls[1].args).toContain(tune.defaultsWriteArgs(null));

    engine.resolve(1, { ok: false, error: { code: 'bad-settings', message: 'the app defaults could not be reset' } });
    await write;

    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
    const note = pane.find('tune-defaults')!.textContent;
    expect(note).toContain('the app defaults could not be reset');
    expect(note).not.toContain(tune.RESET_DEFAULTS_NOTE);
    // Nothing applied: still "your own", Reset still offered.
    expect(pane.find('tune-defaults')!.textContent).toContain('New scripts start from your own defaults.');
    expect(footButtons(pane).reset!.hidden).toBe(false);
  });

  test('a redraw for any other reason mid-write still shows the busy state, and the write still lands on it', async () => {
    // Stands in for applyAll's redraw on a preset click, or flush()'s own
    // correcting one: both rebuild the whole pane through draw() while a
    // defaults write may be in flight. Driven here through a second
    // mount() on the SAME pane, which reaches the identical draw() path
    // without scheduling flush()'s debounce timer, which would otherwise
    // outlive this test and fire a stray engine call into whatever runs
    // next.
    const { pane, ctx } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    const write = footButtons(pane).use!.click() as Promise<void>;
    expect(footButtons(pane).use!.disabled).toBe(true);

    // The redraw: a fresh foot is built from the SAME module state, so it
    // must come up already disabled rather than wrongly idle.
    tune.mount(pane, ctx);
    expect(footButtons(pane).use!.disabled).toBe(true);
    expect(footButtons(pane).reset!.disabled).toBe(true);

    engine.resolve(1, {
      ok: true, formatDefaults: { ...DEFAULT_FORMAT_OPTIONS }, shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    await write;

    // The confirmation lands on the box that redraw just built, not on the
    // detached one writeDefaults was originally called from.
    expect(pane.find('tune-defaults')!.textContent).toContain(tune.USE_DEFAULTS_NOTE);
    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
  });

  test('a write started from script A is dropped, not painted, once script B is ready and showing its own clean foot', async () => {
    // The earlier version of this test resolved the stale write while the
    // pane still showed the reading caption, so nothing could have painted
    // either way: it passed whether or not the era guard existed. Here
    // script B's OWN settings load answers FIRST, so B's foot is fully
    // built and on screen (module state pointing at B's own nodes) while
    // A's write is still unanswered, and only THEN does A's write resolve.
    //
    // A's write resolves with formatDefaults EQUAL to shipped, and B's own
    // appDefaults is deliberately DIFFERENT from shipped: if the era guard
    // is skipped only on the path that updates appDefaults/shippedDefaults/
    // defaultsNote and not on the one that repaints (module state
    // corrupted with A's answer, but nothing redraws to show it yet), B's
    // caption and Reset's visibility still read correctly right after A's
    // write resolves. A LATER redraw (the mount() at the end) is what
    // actually catches that half of the mutant, the same way applyAll's or
    // flush()'s own redraw could in the running app.
    const { pane, ctx } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    const staleWrite = footButtons(pane).use!.click() as Promise<void>;
    expect(footButtons(pane).use!.disabled).toBe(true);

    // The reader converts a different script while A's write is still in
    // flight. If scriptChanged() did not reset defaultsBusy, B's foot below
    // would come up already disabled (M40b) despite owing nothing to
    // anyone.
    const scriptB = {
      fountainPath: '/scripts/other.fountain', epubPath: null, previewHtml: undefined, settings: null,
    };
    ctx.state.script = scriptB;
    tune.scriptChanged();
    expect(pane.textContent).toContain(tune.READING);

    const shownB = tune.show();
    // B's own settings load, call index 2 (0 was A's load, 1 was A's
    // write), answers before A's write does.
    engine.resolve(2, settingsAnswer({ appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } }));
    await shownB;
    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
    expect(pane.find('defaults-note')!.hidden).toBe(true);
    expect(pane.find('defaults-note')!.textContent).toBe('');
    expect(pane.find('tune-defaults')!.textContent).toContain('New scripts start from your own defaults.');

    // A's write finally answers, with app defaults equal to shipped:
    // painted onto B's page, this would disable B's buttons again or leave
    // them disabled (M20b, M21, M21b), show A's confirmation under B's
    // knobs (M20c), and flip B's own caption/Reset to match A's answer
    // instead of B's own settings.
    engine.resolve(1, {
      ok: true, formatDefaults: { ...DEFAULT_FORMAT_OPTIONS }, shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    await staleWrite;

    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
    expect(pane.find('defaults-note')!.hidden).toBe(true);
    expect(pane.find('defaults-note')!.textContent).toBe('');
    expect(pane.find('tune-defaults')!.textContent).toContain('New scripts start from your own defaults.');
    expect(footButtons(pane).reset!.hidden).toBe(false);

    // A later redraw of B's own page (mount() again, the same stand-in for
    // "any other reason to rebuild" the redraw-survival test above uses)
    // must still show B's own state, not module state A's stale write may
    // have overwritten without repainting.
    tune.mount(pane, ctx);
    expect(pane.find('tune-defaults')!.textContent).toContain('New scripts start from your own defaults.');
    expect(footButtons(pane).reset!.hidden).toBe(false);
    expect(pane.find('defaults-note')!.textContent).toBe('');
  });

  test('a write updates the foot IN PLACE: the clicked button and the note keep their identity, so focus is not dropped', async () => {
    // refreshDefaultsFoot used to rebuild the whole foot, which replaced
    // the very button the reader just clicked with a fresh one: a browser
    // moves focus to <body> the instant its focused element is removed.
    // applyDefaultsFootState only ever mutates text/disabled/hidden on the
    // SAME nodes now, checked here by reference (toBe), not by rendered
    // content, which a rebuild-and-match-the-old-content mutant would still
    // pass.
    const { pane } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    const before = footButtons(pane);
    const noteBefore = pane.find('defaults-note');
    expect(before.use).not.toBeNull();
    expect(noteBefore).not.toBeNull();

    const write = before.use!.click() as Promise<void>;
    // Immediately after the click: still the identical node.
    expect(footButtons(pane).use).toBe(before.use);
    expect(footButtons(pane).reset).toBe(before.reset);
    expect(pane.find('defaults-note')).toBe(noteBefore);

    engine.resolve(1, {
      ok: true, formatDefaults: { ...DEFAULT_FORMAT_OPTIONS }, shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    await write;

    // And still the identical node once the write has answered and the
    // confirmation is showing.
    expect(footButtons(pane).use).toBe(before.use);
    expect(footButtons(pane).reset).toBe(before.reset);
    expect(pane.find('defaults-note')).toBe(noteBefore);
  });

  test('the confirmation note is hidden while empty, visible once there is something to say', async () => {
    const { pane } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    expect(pane.find('defaults-note')!.hidden).toBe(true);
    expect(pane.find('defaults-note')!.textContent).toBe('');

    const write = footButtons(pane).use!.click() as Promise<void>;
    engine.resolve(1, {
      ok: true, formatDefaults: { ...DEFAULT_FORMAT_OPTIONS }, shippedDefaults: { ...DEFAULT_FORMAT_OPTIONS },
    });
    await write;

    expect(pane.find('defaults-note')!.hidden).toBe(false);
    expect(pane.find('defaults-note')!.textContent).not.toBe('');
  });

  test('the two confirmation strings are pinned exactly', () => {
    expect(tune.USE_DEFAULTS_NOTE).toBe('New scripts will start from these settings.');
    expect(tune.RESET_DEFAULTS_NOTE).toBe('New scripts will start from Screepub’s own defaults.');
  });

  test('an engine call that THROWS (not a refusal) still puts its message in the note, verbatim', async () => {
    const { pane } = await mountReady(
      engine, { appDefaults: { ...DEFAULT_FORMAT_OPTIONS, fontFamily: 'serif' } },
    );
    const write = footButtons(pane).use!.click() as Promise<void>;
    engine.reject(1, 'the engine crashed mid-write');
    await write;

    expect(pane.find('defaults-note')!.textContent).toBe('the engine crashed mid-write');
    expect(pane.find('defaults-note')!.hidden).toBe(false);
    expect(footButtons(pane).use!.disabled).toBe(false);
    expect(footButtons(pane).reset!.disabled).toBe(false);
  });

  // ---- "When a PDF is converted": keep its settings, or follow the defaults
  //
  // Spec 2026-09-24-keep-script-settings-choice-design.md. An app-wide
  // choice read off the same settings answer as the defaults above and
  // written through the same app-settings --set.

  test('the choice’s words are the owner’s, pinned exactly, and carry no em dash', () => {
    expect(tune.KEEP_CHOICE.legend).toBe('When a PDF is converted');
    expect(tune.KEEP_CHOICE.options.map((o: any) => [o.keep, o.label, o.line])).toEqual([
      [true, 'Keep its settings',
        'The script keeps the settings it was made with. Changing the defaults later won’t change it.'],
      [false, 'Follow the defaults',
        'Scripts you haven’t tuned change when the defaults do. Scripts that already have their own settings keep them.'],
    ]);
    expect(tune.KEEP_SAVED_NOTE).toBe('Saved. It applies to PDFs you convert from now on.');
    for (const line of [
      tune.KEEP_CHOICE.legend, tune.KEEP_SAVED_NOTE,
      ...tune.KEEP_CHOICE.options.flatMap((o: any) => [o.label, o.line]),
    ]) {
      expect(line).not.toContain('\u2014');
    }
  });

  test('keepChoiceFrom takes only a boolean, and keepWriteArgs is the --set value for either option', () => {
    expect(tune.keepChoiceFrom({ keepScriptSettings: true })).toBe(true);
    expect(tune.keepChoiceFrom({ keepScriptSettings: false })).toBe(false);
    // An answer that breaks the contract draws no choice rather than a
    // guessed one.
    expect(tune.keepChoiceFrom({ keepScriptSettings: 'false' })).toBeNull();
    expect(tune.keepChoiceFrom({})).toBeNull();
    expect(tune.keepChoiceFrom(undefined)).toBeNull();
    expect(tune.keepWriteArgs(false)).toBe('{"keepScriptSettings":false}');
    expect(tune.keepWriteArgs(true)).toBe('{"keepScriptSettings":true}');
  });

  test('keepWriteOutcome: another script’s write and a superseded write are not painted; the latest is applied or reported', () => {
    const ok = (keep: boolean) => ({ ok: true, keepScriptSettings: keep });
    // The page moved to another script: nothing, not even the stored value,
    // belongs to what is on screen now.
    expect(tune.keepWriteOutcome(1, 2, 1, 1, ok(false))).toEqual({ stale: true, stored: null });
    // A newer choice is queued behind this one: what the engine stored is
    // still recorded, but the radios and the note are the newer write's.
    expect(tune.keepWriteOutcome(1, 1, 1, 2, ok(false))).toEqual({ stale: true, stored: false });
    expect(tune.keepWriteOutcome(1, 1, 2, 2, ok(false))).toEqual({
      stale: false, applied: true, stored: false, message: tune.KEEP_SAVED_NOTE,
    });
    expect(tune.keepWriteOutcome(1, 1, 2, 2, { ok: false, error: { message: 'cannot save the settings file' } }))
      .toEqual({ stale: false, applied: false, stored: null, message: 'cannot save the settings file' });
    // ok without the value in it is not a confirmation of anything.
    expect(tune.keepWriteOutcome(1, 1, 2, 2, { ok: true }))
      .toEqual({ stale: false, applied: false, stored: null, message: tune.NO_MESSAGE });
  });

  /** The choice's two radios, keyed by what each one keeps. */
  function keepRadios(pane: FakeNode) {
    const radios = pane.findTag('input').filter((n) => n.getAttribute('type') === 'radio');
    const byLabel = (label: string) => {
      const option = tune.KEEP_CHOICE.options.find((o: any) => o.label === label);
      return radios.find((r) => r.getAttribute('id') === option.id) ?? null;
    };
    return { all: radios, keep: byLabel('Keep its settings'), follow: byLabel('Follow the defaults') };
  }

  test('it is a real radio group: a fieldset with a legend, two radios sharing a name, each labelled and described', async () => {
    const { pane } = await mountReady(engine, { keepScriptSettings: true });
    const box = pane.find('keep-choice');
    expect(box).not.toBeNull();
    expect(box!.tagName).toBe('fieldset');
    const legend = box!.findTag('legend');
    expect(legend.length).toBe(1);
    expect(legend[0].textContent).toBe('When a PDF is converted');

    const { all, keep, follow } = keepRadios(pane);
    expect(all.length).toBe(2);
    // One name, so the platform makes them one group: one Tab stop, arrow
    // keys between the two, and only one ever checked.
    expect(new Set(all.map((r) => r.getAttribute('name'))).size).toBe(1);
    expect(all[0].getAttribute('name')).not.toBeNull();
    const labels = box!.findTag('label');
    const paragraphs = box!.findTag('p');
    for (const radio of [keep!, follow!]) {
      const id = radio.getAttribute('id')!;
      const option = tune.KEEP_CHOICE.options.find((o: any) => o.id === id);
      expect(labels.find((l) => l.getAttribute('for') === id)?.textContent).toBe(option.label);
      const described = radio.getAttribute('aria-describedby');
      expect(paragraphs.find((p) => p.getAttribute('id') === described)?.textContent).toBe(option.line);
    }
    expect(keep!.checked).toBe(true);
    expect(follow!.checked).toBe(false);
    // After the defaults foot, in the same box (layout pass, 2026-09-29).
    const foot = pane.find('tune-foot')!;
    const defaults = pane.find('tune-defaults')!;
    expect(foot.kids.indexOf(defaults)).toBeGreaterThan(-1);
    expect(foot.kids.indexOf(box!)).toBeGreaterThan(foot.kids.indexOf(defaults));
  });

  test('it opens on what the engine says is stored, and an answer without it draws no choice at all', async () => {
    const off = await mountReady(engine, { keepScriptSettings: false });
    expect(keepRadios(off.pane).follow!.checked).toBe(true);
    expect(keepRadios(off.pane).keep!.checked).toBe(false);

    engine = stubEngine();
    const missing = await mountReady(engine);
    expect(missing.pane.find('keep-choice')).toBeNull();
  });

  test('choosing "Follow the defaults" writes it through app-settings, never disables the radios, and confirms', async () => {
    const { pane } = await mountReady(engine, { keepScriptSettings: true });
    const { keep, follow } = keepRadios(pane);
    follow!.checked = true;
    follow!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));

    expect(engine.calls.length).toBe(2);
    expect(engine.calls[1].args).toEqual(['app-settings', '--json', '--set', '{"keepScriptSettings":false}']);
    // Not disabled while the write is out: an arrow key both moves focus
    // and changes the choice, and a disabled radio drops the keyboard.
    expect(keep!.disabled).toBe(false);
    expect(follow!.disabled).toBe(false);
    expect(follow!.checked).toBe(true);

    engine.resolve(1, { ok: true, keepScriptSettings: false });
    await new Promise((r) => setTimeout(r, 0));
    expect(pane.find('keep-note')!.textContent).toBe(tune.KEEP_SAVED_NOTE);
    expect(pane.find('keep-note')!.hidden).toBe(false);
    expect(pane.find('keep-note')!.getAttribute('role')).toBe('status');
    expect(follow!.checked).toBe(true);
    expect(keep!.checked).toBe(false);
    // Updated in place: the radio the reader is on is the same node.
    expect(keepRadios(pane).follow).toBe(follow);
  });

  test('a refusal puts the radios back to what the engine holds, and shows its sentence', async () => {
    const { pane } = await mountReady(engine, { keepScriptSettings: true });
    const { keep, follow } = keepRadios(pane);
    follow!.checked = true;
    follow!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));
    engine.resolve(1, { ok: false, error: { code: 'bad-settings', message: 'cannot save the settings file' } });
    await new Promise((r) => setTimeout(r, 0));
    expect(keep!.checked).toBe(true);
    expect(follow!.checked).toBe(false);
    expect(pane.find('keep-note')!.textContent).toBe('cannot save the settings file');

    // A throw (the engine did not answer at all) is reported the same way.
    follow!.checked = true;
    follow!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));
    engine.reject(2, 'the engine is not there');
    await new Promise((r) => setTimeout(r, 0));
    expect(keep!.checked).toBe(true);
    expect(pane.find('keep-note')!.textContent).toBe('the engine is not there');
  });

  test('two quick choices are sent one after the other, and only the latest is painted', async () => {
    const { pane } = await mountReady(engine, { keepScriptSettings: true });
    const { keep, follow } = keepRadios(pane);
    follow!.checked = true;
    follow!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));
    keep!.checked = true;
    follow!.checked = false;
    keep!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));
    // Queued, not raced: the second write waits for the first to answer.
    expect(engine.calls.length).toBe(2);

    engine.resolve(1, { ok: true, keepScriptSettings: false });
    await new Promise((r) => setTimeout(r, 0));
    // The first answer is superseded: the reader's latest choice stays shown.
    expect(keep!.checked).toBe(true);
    expect(pane.find('keep-note')!.textContent).toBe('');
    expect(engine.calls.length).toBe(3);
    expect(engine.calls[2].args).toContain('{"keepScriptSettings":true}');

    engine.resolve(2, { ok: true, keepScriptSettings: true });
    await new Promise((r) => setTimeout(r, 0));
    expect(keep!.checked).toBe(true);
    expect(pane.find('keep-note')!.textContent).toBe(tune.KEEP_SAVED_NOTE);
  });

  test('a choice made on one script is still sent after the page moves on, but not painted onto the next script', async () => {
    const { pane, ctx } = await mountReady(engine, { keepScriptSettings: true });
    const { follow } = keepRadios(pane);
    follow!.checked = true;
    follow!.dispatch('change');
    await new Promise((r) => setTimeout(r, 0));
    expect(engine.calls.length).toBe(2);

    ctx.state.script = {
      fountainPath: '/scripts/other.fountain', epubPath: null, previewHtml: undefined, settings: null,
    };
    tune.scriptChanged();
    const shown = tune.show();
    engine.resolve(2, settingsAnswer({ keepScriptSettings: true }));
    await shown;
    expect(keepRadios(pane).keep!.checked).toBe(true);

    engine.resolve(1, { ok: true, keepScriptSettings: false });
    await new Promise((r) => setTimeout(r, 0));
    expect(keepRadios(pane).keep!.checked).toBe(true);
    expect(pane.find('keep-note')!.textContent).toBe('');
  });

});

describe('turns on a book: book-queue.js', () => {
  // One queue per library EPUB, readers and writers. A call that only READS
  // the book (a Kindle file built beside it, a copy, a send, the check) runs
  // beside other readers; one that WRITES it (a Settings save and its
  // rebuild, a conversion of the same PDF again, the engine's MOBI rung) runs
  // alone. Arrival order is kept: a reader that arrives behind a waiting
  // writer waits for it, so a Settings change always lands before a later
  // send reads the book, and a writer is never starved by a stream of
  // readers. A module of its own per test: the queues and the registered
  // hooks live as long as the module does.
  //
  // Adapted from the one-at-a-time queue (2026-09-28): 'on one book, a turn
  // waits for the one before it' now puts a WRITER between two readers,
  // which is what still waits; `holder` became `holders`, every turn
  // running now rather than the one; the other two tests are unchanged.
  type Queue = {
    inTurn: <T>(book: string, label: string, work: () => T | Promise<T>) => Promise<T>;
    holders: (book: string) => string[];
    beforeEveryTurn: (hook: () => void) => void;
    TURNS: Record<string, string>;
  };
  let fresh = 0;
  const load = async () => (await import(`${join(UI, 'book-queue.js')}?turns-${++fresh}`)) as Queue;
  const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
  const held = () => {
    let release: (value: string) => void = () => {};
    const promise = new Promise<string>((resolve) => { release = resolve; });
    return { promise, release };
  };

  test('every label says, in one table, whether it reads the book or writes it', async () => {
    const q = await load();
    expect(q.TURNS).toEqual({
      save: 'write',
      convert: 'write',
      'kindle-mobi': 'write',
      kindle: 'read',
      send: 'read',
      copy: 'read',
      check: 'read',
    });
    // A label with no row is a mistake in the caller, not a guess here.
    expect(() => q.inTurn('/lib/a.epub', 'mystery', () => 'x')).toThrow('mystery');
  });

  test('with nothing queued, a turn starts in the caller’s own tick', async () => {
    const q = await load();
    let ran = false;
    const turn = q.inTurn('/lib/a.epub', 'send', () => { ran = true; return 'sent'; });
    expect(ran).toBe(true);
    expect(await turn).toBe('sent');
  });

  test('two readers run at once', async () => {
    // The background Kindle build reads the book for half a minute; Save the
    // EPUB pressed meanwhile starts at once rather than waiting it out.
    const q = await load();
    const build = held();
    const order: string[] = [];
    const a = q.inTurn('/lib/a.epub', 'kindle', () => { order.push('kindle'); return build.promise; });
    const b = q.inTurn('/lib/a.epub', 'copy', () => { order.push('copy'); return 'copied'; });
    expect(order).toEqual(['kindle', 'copy']);
    expect(await b).toBe('copied');
    expect(q.holders('/lib/a.epub')).toEqual(['kindle']);
    build.release('built');
    expect(await a).toBe('built');
    await tick();
    expect(q.holders('/lib/a.epub')).toEqual([]);
  });

  test('a writer waits for every reader running, and runs alone', async () => {
    const q = await load();
    const build = held();
    const copy = held();
    const order: string[] = [];
    q.inTurn('/lib/a.epub', 'kindle', () => { order.push('kindle'); return build.promise; });
    q.inTurn('/lib/a.epub', 'copy', () => { order.push('copy'); return copy.promise; });
    const save = held();
    const writing = q.inTurn('/lib/a.epub', 'save', () => { order.push('save'); return save.promise; });
    await tick();
    expect(order).toEqual(['kindle', 'copy']);
    expect(q.holders('/lib/a.epub')).toEqual(['kindle', 'copy']);
    copy.release('copied');
    await tick();
    expect(order).toEqual(['kindle', 'copy']); // the build still reads it
    build.release('built');
    await tick();
    expect(order).toEqual(['kindle', 'copy', 'save']);
    expect(q.holders('/lib/a.epub')).toEqual(['save']);
    // Nothing starts beside a writer, not even a reader.
    let checked = false;
    const check = q.inTurn('/lib/a.epub', 'check', () => { checked = true; return 'checked'; });
    await tick();
    expect(checked).toBe(false);
    save.release('saved');
    expect(await writing).toBe('saved');
    expect(await check).toBe('checked');
  });

  test('a reader arriving behind a waiting writer waits for it: the change lands before the book is read', async () => {
    const q = await load();
    const build = held();
    const order: string[] = [];
    q.inTurn('/lib/a.epub', 'kindle', () => { order.push('kindle'); return build.promise; });
    const saving = q.inTurn('/lib/a.epub', 'save', () => { order.push('save'); return 'saved'; });
    const sending = q.inTurn('/lib/a.epub', 'send', () => { order.push('send'); return 'sent'; });
    await tick();
    // Not beside the running reader: behind the writer that waits for it.
    expect(order).toEqual(['kindle']);
    build.release('built');
    expect(await saving).toBe('saved');
    expect(await sending).toBe('sent');
    expect(order).toEqual(['kindle', 'save', 'send']);
  });

  test('on one book, a writer between two readers waits for the first, and a failure does not stop the next', async () => {
    const q = await load();
    const first = held();
    const order: string[] = [];
    const a = q.inTurn('/lib/a.epub', 'send', () => { order.push('send'); return first.promise; });
    const b = q.inTurn('/lib/a.epub', 'save', () => { order.push('save'); throw new Error('refused'); });
    const c = q.inTurn('/lib/a.epub', 'copy', () => { order.push('copy'); return 'copied'; });
    await tick();
    expect(order).toEqual(['send']);
    expect(q.holders('/lib/a.epub')).toEqual(['send']);
    first.release('sent');
    expect(await a).toBe('sent');
    await expect(b).rejects.toThrow('refused');
    expect(await c).toBe('copied');
    expect(order).toEqual(['send', 'save', 'copy']);
    await tick();
    expect(q.holders('/lib/a.epub')).toEqual([]);
  });

  test('another book does not wait: a hung call holds up its own book and nothing else', async () => {
    const q = await load();
    const hung = held();
    q.inTurn('/lib/old.epub', 'save', () => hung.promise);
    let ran = false;
    const other = q.inTurn('/lib/new.epub', 'save', () => { ran = true; return 'saved'; });
    expect(ran).toBe(true);
    expect(await other).toBe('saved');
    expect(q.holders('/lib/old.epub')).toEqual(['save']);
    expect(q.holders('/lib/new.epub')).toEqual([]);
    hung.release('late');
  });

  test('what a page owes goes first: a pending settle is flushed before a reader starts', async () => {
    // tune.js's settle, still counting down after a knob moved, joins the
    // queue as a writer AHEAD of the reader that asked, so a send never
    // reads a book with a change still only on screen.
    const q = await load();
    const order: string[] = [];
    let owing = true;
    const settle = () => {
      if (!owing) return;
      owing = false;
      q.inTurn('/lib/a.epub', 'save', () => { order.push('save'); });
    };
    q.beforeEveryTurn(settle);
    q.beforeEveryTurn(settle);
    await q.inTurn('/lib/a.epub', 'send', () => { order.push('send'); });
    expect(order).toEqual(['save', 'send']);
  });
});

describe('sending, from the result’s panel', () => {
  test('the window holds none of the engine’s knowledge of what a reader looks like', () => {
    // A source rule, kept: a window that knew what a Kindle volume looks
    // like would be the duplication ADR 2026-09-12 forbids, and no mounted
    // test can see knowledge that is merely present. What the panel asks the
    // engine instead (routes, and the devices poll) is driven in
    // send-panel-ui.test.ts and device-watch.test.ts.
    for (const file of ['send-panel.js', 'send-routes.js', 'device.js', 'device-watch.js']) {
      const source = read(file);
      for (const knowledge of ['/Volumes', 'documents', 'system/version.txt', '.kobo']) {
        expect(`${file} knows ${knowledge}: ${source.includes(knowledge)}`).toBe(
          `${file} knows ${knowledge}: false`,
        );
      }
    }
  });

  test('only the flows that read the book take a turn on it', async () => {
    // A moved knob's save rebuilds the library EPUB in place, and an export
    // can rebuild it too (the MOBI rung); a send or a save copies it. Those
    // take their turn on the book; opening another program does not, or it
    // would hold the Settings page's saves for as long as that program took.
    // The turns themselves are driven on the panel in send-panel-ui.test.ts
    // ("only a save waits for the book", "a Settings save during a
    // background build waits for it", and the rest of that file).
    const sendUi = await import(join(UI, 'send-routes.js'));
    expect(['device', 'save-epub', 'save-kindle', 'open', 'setup', null].map(sendUi.readsTheBook))
      .toEqual([true, true, true, false, false, false]);
  });

  test('nothing connected is an answer, not an error', async () => {
    // It used to be a "Nothing plugged in" paragraph over the reach table.
    // Now the engine lists every reader kind that is not connected as a
    // dimmed row with its fix, so nothing connected is still a page of rows
    // (the engine's own answer for it is taken whole, never turned into the
    // failure line) and those rows offer no button that could only fail.
    const { routes, preselected } = await import('../src/export/routes');
    const sendUi = await import(join(UI, 'send-routes.js'));
    for (const platform of ['darwin', 'win32', 'linux']) {
      const list = routes({ platform, booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices: [] });
      const shown = sendUi.routesFrom({ ok: true, routes: list, chosen: preselected(list, undefined).id });
      expect(shown, `${platform}: the no-device answer is refused`).not.toBe(null);
      const readers = shown.routes.filter((r: { key: string }) => sendUi.isDeviceRoute(r));
      expect(readers.length).toBeGreaterThan(0);
      for (const r of readers) {
        expect(`${platform} ${r.key} available: ${r.available}`).toBe(`${platform} ${r.key} available: false`);
      }
      expect(sendUi.connectedDevices(shown)).toEqual([]);
    }
  });
});

describe('the route rows in Other ways are dressed by the stylesheet', () => {
  // perform() and sendTo() used to be pinned here by the shape of their
  // source. Every one of those behaviours (one flag for every route, a check
  // after every await, the Save box before the write, the settings fetched
  // first, the keyboard handed back, the repoll after a success, the list's
  // failure line) is driven on the mounted panel in send-panel-ui.test.ts.
  // What is left is CSS: Other ways still draws these classes.
  test('every class a route row adds is styled, and a dimmed row is dimmed by ink, not opacity', () => {
    // Kept as a CSS pin: whether a rule paints needs a browser. A dimmed row
    // dimmed by opacity would dim the email row's setup link with it.
    const css = read('surfaces.css');
    for (const name of ['route-unavailable', 'route-detail', 'route-hint']) {
      expect(`${name} styled: ${new RegExp(`\\.${name}\\b[^{]*\\{`).test(css)}`).toBe(`${name} styled: true`);
    }
    const dimmed = /\.route-unavailable[^{]*\{([^}]*)\}/.exec(css)![1]!;
    expect(dimmed).toContain('color: var(--ink-muted)');
    expect(dimmed).not.toContain('opacity');
  });
});

describe('what the send panel decides', () => {
  type Device = { id: string; kind: string; name: string; volume: string | null };
  type SendModule = {
    UNPROVEN: string;
    PROVEN: { kind: string; platform: string };
    KINDS: string[];
    NO_MESSAGE: string;
    platformOf: (hint: unknown) => string;
    undetectable: (platform: unknown) => string | null;
    routesFrom: (answer: unknown) => { routes: Record<string, unknown>[]; chosen: string } | null;
    sameRoutes: (a: unknown, b: unknown) => boolean;
    forFormat: (device: unknown) => string;
    whereLine: (device: unknown) => string;
    blockedReason: (script: unknown) => string | null;
    optionsJsonFor: (script: unknown) => string | null;
    sentLine: (device: Device, answer: unknown) => string;
    artifactLine: (built: unknown) => string;
    statusFor: (phase: string, opts?: { device?: unknown; detail?: string })
      => { line: string; bad: boolean };
    preparingPhase: (device: unknown, file?: unknown) => string;
    failureMessage: (answer: unknown) => string;
    outcomeFor: (built: unknown, sent: unknown, device: unknown)
      => [string, { device: unknown; detail: string }];
    needsSettings: (script: unknown) => boolean;
  };
  let send: SendModule;

  const kindle: Device = { id: '/m/Kindle', kind: 'kindle', name: 'Kindle', volume: '/m/Kindle' };
  const kobo: Device = { id: '/m/KOBOe', kind: 'kobo', name: 'Kobo', volume: '/m/KOBOe' };
  const rm: Device = { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null };

  // platformOf and undetectable as the device model hands them on: the
  // device cards and their lines read them through device.js.
  let device: Pick<SendModule, 'platformOf' | 'undetectable'>;

  beforeAll(async () => {
    send = (await import(join(UI, 'send-routes.js'))) as SendModule;
    device = (await import(join(UI, 'device.js'))) as Pick<SendModule, 'platformOf' | 'undetectable'>;
  });

  test('a platform this window cannot name is unknown, never some other one', () => {
    // navigator.platform is deprecated and navigator.userAgentData is absent
    // on WebKitGTK, so this window can genuinely not know. Under-claiming
    // hardware support is right; naming the wrong operating system is not.
    for (const unknown of [undefined, null, '', 'CrOS', {}]) {
      expect(device.platformOf(unknown)).toBe('unknown');
    }
    expect(device.platformOf('Linux aarch64')).toBe('linux');
    expect(device.platformOf('MacIntel')).toBe('mac');
    expect(device.platformOf('Win32')).toBe('windows');
  });

  test('a tolino on Windows is called undetectable, not merely absent', () => {
    // It is identified by the name of its volume and a Windows drive root
    // carries none. Left unsaid, a missing row reads as a bad cable.
    const said = device.undetectable('Win32');
    expect(said).toContain('tolino');
    expect(said).toContain('Windows');
    for (const platform of ['MacIntel', 'Linux aarch64', undefined]) {
      expect(`${platform}: ${device.undetectable(platform)}`).toBe(`${platform}: null`);
    }
  });

  test('a device that cannot be addressed is not offered, and an unknown one still is', () => {
    // This was devicesFrom(), which dropped a device with no id or no name
    // and kept a kind this window had never heard of. The rows come from
    // `routes` now, and routesFrom() holds the same line, stricter: a device
    // row whose device cannot be addressed turns the WHOLE answer into the
    // failure line (a quietly shorter list reads as a reader that is not
    // there), and an unknown kind is kept, with a button and the caveat.
    const pocketbook = {
      id: 'device:pocketbook#/m/z', key: 'device:pocketbook', title: 'PocketBook',
      detail: 'over USB', button: 'Copy to PocketBook', available: true,
      device: { id: '/m/z', kind: 'pocketbook', name: 'PocketBook', volume: '/m/z' },
    };
    const save = {
      id: 'save-epub', key: 'save-epub', title: 'Save the EPUB', detail: 'for email',
      button: 'Save the EPUB…', available: true,
    };
    const shown = send.routesFrom({ ok: true, routes: [pocketbook, save], chosen: 'save-epub' })!;
    expect(shown.routes.map((r) => r.id)).toEqual([pocketbook.id, 'save-epub']);
    for (const device of [
      { ...pocketbook.device, id: '' },                  // no id to send to
      { ...pocketbook.device, name: '   ' },             // nothing for the row
      { id: '/m/y', name: 'Mystery', volume: '/m/y' },   // no kind
    ]) {
      expect(send.routesFrom({ ok: true, routes: [{ ...pocketbook, device }, save], chosen: 'save-epub' }))
        .toBe(null);
    }
  });

  test('the list is rebuilt only when it really changed', () => {
    // This was sameDevices(); the rule is sameRoutes() now (more of it in
    // tests/send-routes-ui.test.ts). The poll ticks every two seconds;
    // rebuilding on every tick steals the focus from anyone tabbing to a
    // button. So it has to say "same" for a repeated answer, and it must not
    // say "same" by agreeing on length alone.
    const row = (id: string, title: string) => ({
      id, key: id, title, detail: 'd', button: title, available: true,
    });
    const shown = (routes: Record<string, unknown>[], chosen: string) =>
      send.routesFrom({ ok: true, routes, chosen })!;
    const a = shown([row('save-epub', 'Save'), row('save-kindle', 'Kindle file')], 'save-epub');
    expect(send.sameRoutes(a, shown([row('save-epub', 'Save'), row('save-kindle', 'Kindle file')], 'save-epub')))
      .toBe(true);
    expect(send.sameRoutes(a, shown([row('save-epub', 'Save'), row('save-kindle', 'Kindle')], 'save-epub')))
      .toBe(false);
    // Same two rows, opposite order, or the brass moved: out of date either way.
    expect(send.sameRoutes(a, shown([row('save-kindle', 'Kindle file'), row('save-epub', 'Save')], 'save-epub')))
      .toBe(false);
    expect(send.sameRoutes(a, shown([row('save-epub', 'Save'), row('save-kindle', 'Kindle file')], 'save-kindle')))
      .toBe(false);
    expect(send.sameRoutes(a, shown([row('save-epub', 'Save')], 'save-epub'))).toBe(false);
    expect(send.sameRoutes(a, 'nope')).toBe(false);
  });

  test('only a Kindle is sent up the ladder, and every other kind takes the EPUB', () => {
    expect(send.forFormat(kindle)).toBe('kindle');
    for (const kind of [...send.KINDS.filter((k) => k !== 'kindle'), 'pocketbook', '']) {
      expect(`${kind}: ${send.forFormat({ kind })}`).toBe(`${kind}: epub`);
    }
    expect(send.forFormat(null)).toBe('epub');
    // The long wait is the Kindle rung's alone; saying "building" for a rung
    // that builds nothing would be theatre.
    expect(send.preparingPhase(kindle)).toBe('building');
    expect(send.preparingPhase(kobo)).toBe('preparing');
    // A Kindle file the engine says is current is a stat too.
    expect(send.preparingPhase(kindle, { extension: 'kfx', fresh: true, builtBy: 'calibre' })).toBe('preparing');
  });

  test('a reMarkable is not described as if it had a volume', () => {
    expect(send.whereLine(kindle)).toBe('/m/Kindle');
    const said = send.whereLine(rm);
    expect(said).not.toBe('remarkable');
    expect(said).toContain('USB');
  });

  test('a script with no book on disk is refused before a device is offered', () => {
    expect(send.blockedReason({ epubPath: '/lib/s/s.epub' })).toBe(null);
    for (const script of [null, undefined, {}, { epubPath: null }, { epubPath: '  ' }]) {
      const said = send.blockedReason(script);
      expect(`${JSON.stringify(script) ?? 'undefined'}: ${said === null}`)
        .toBe(`${JSON.stringify(script) ?? 'undefined'}: false`);
    }
  });

  test('the settings that go with the export are the script’s own', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    expect(send.optionsJsonFor({ settings: null })).toBe(null);
    expect(send.optionsJsonFor({ settings: {} })).toBe(null);
    expect(send.optionsJsonFor({})).toBe(null);
    const tuned = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27, fontFamily: 'serif' };
    // Asserted against the value it was changed FROM: a function that quietly
    // handed back the defaults would round-trip to something that is NOT this.
    expect(tuned.dialogueSideMarginPct).not.toBe(DEFAULT_FORMAT_OPTIONS.dialogueSideMarginPct);
    expect(JSON.parse(send.optionsJsonFor({ settings: tuned })!)).toEqual(tuned);
  });

  test('"Sent" is claimed in the shape the engine reported, and never invents a path', () => {
    const copied = send.sentLine(kindle, { ok: true, destination: '/m/Kindle/x.azw3' });
    expect(copied.startsWith('Sent to Kindle')).toBe(true);
    expect(copied).toContain('/m/Kindle/x.azw3');
    expect(copied).toContain('Eject');
    // A reMarkable never mounts, so there is nothing to eject and no path.
    const uploaded = send.sentLine(rm, { ok: true, uploaded: true });
    expect(uploaded).toBe('Sent to reMarkable.');
    expect(uploaded).not.toContain('Eject');
    // Neither field: still sent — the engine said ok — but no invented path.
    expect(send.sentLine(kobo, { ok: true })).toBe('Sent to Kobo.');
    expect(send.sentLine(kobo, { ok: true })).not.toContain('—');
    expect(copied).toBe('Sent to Kindle: /m/Kindle/x.azw3. Eject the volume before you unplug it.');
  });

  test('the status line alarms on failure and on nothing else', () => {
    for (const phase of ['idle', 'preparing', 'copying', 'sent']) {
      const { bad } = send.statusFor(phase, { device: kindle, detail: 'Sent to Kindle.' });
      expect(`${phase} is bad: ${bad}`).toBe(`${phase} is bad: false`);
    }
    // A second attempt after a failure must clear the alarm, not inherit it.
    expect(send.statusFor('failed', { detail: 'no reader is connected' }).bad).toBe(true);
    expect(send.statusFor('copying', { device: kindle }).bad).toBe(false);
    // The engine's own sentence, verbatim.
    expect(send.statusFor('failed', { detail: 'reMarkable accepts PDF and EPUB, not .mobi.' }).line)
      .toBe('reMarkable accepts PDF and EPUB, not .mobi.');
    // A contract-breaking failure still says something a person can read.
    expect(send.statusFor('failed', { detail: '   ' }).line).toBe(send.NO_MESSAGE);
    expect(send.statusFor('failed', {}).line).toBe(send.NO_MESSAGE);
    // And a failure never says "Sent".
    expect(send.statusFor('failed', { device: kindle, detail: 'it broke' }).line)
      .not.toContain('Sent');
    // Each working phase names the reader it is working on.
    for (const phase of ['preparing', 'copying']) {
      expect(send.statusFor(phase, { device: kindle }).line).toContain('Kindle');
    }
  });

  test('a refusal is never dressed up as a transfer', () => {
    // The decision the whole surface turns on. sentLine CANNOT tell a refusal
    // from a success on its own: a refused answer carries no destination, and
    // "no destination" is also the shape a reMarkable upload takes — so it
    // would render a refusal as "Sent to Kindle." with no alarm. Both answers
    // are checked here, and both are checked in the failing direction.
    const good = { ok: true, label: 'AZW3: for USB sideload to Kindle', path: '/x.azw3' };
    const refusedExport = {
      ok: false,
      error: { code: 'export-failed', message: "Can't rebuild the Kindle file." },
    };
    const refusedSend = {
      ok: false,
      error: { code: 'send-failed', message: 'no reader is connected' },
    };

    const [sentPhase, sentDetail] = send.outcomeFor(good, { ok: true, destination: '/m/K/x.azw3' }, kindle);
    expect(sentPhase).toBe('sent');
    expect(send.statusFor(sentPhase, sentDetail)).toEqual({
      line: sentDetail.detail, bad: false,
    });
    expect(sentDetail.detail).toContain('Sent to Kindle');

    for (const [built, sent, why] of [
      [refusedExport, null, 'the export was refused'],
      [refusedExport, { ok: true, destination: '/m/K/x.azw3' }, 'the export was refused'],
      [good, refusedSend, 'the copy was refused'],
      [good, { ok: false }, 'the copy was refused with no sentence'],
      [good, null, 'no answer came back at all'],
      [null, null, 'nothing came back at all'],
    ] as [unknown, unknown, string][]) {
      const [phase, detail] = send.outcomeFor(built, sent, kindle);
      expect(`${why}: ${phase}`).toBe(`${why}: failed`);
      const status = send.statusFor(phase, detail);
      // The two claims a refusal must never make.
      expect(`${why} says Sent: ${status.line.includes('Sent')}`)
        .toBe(`${why} says Sent: false`);
      expect(`${why} is calm: ${status.bad === false}`).toBe(`${why} is calm: false`);
      expect(status.line.trim()).not.toBe('');
    }
    // The engine's own sentence survives, verbatim, in both positions.
    expect(send.outcomeFor(refusedExport, null, kindle)[1].detail)
      .toBe("Can't rebuild the Kindle file.");
    expect(send.outcomeFor(good, refusedSend, kindle)[1].detail)
      .toBe('no reader is connected');
    expect(send.failureMessage({ ok: false })).toBe(send.NO_MESSAGE);
  });

  test('a script whose settings this window has not read yet gets them fetched', () => {
    // Without the fetch, optionsJsonFor has nothing to serialise and the
    // export silently rebuilds with the engine's DEFAULTS — the recorded
    // first-conversion gap, landing on Send.
    const fresh = { epubPath: '/lib/s.epub', fountainPath: '/lib/s.fountain', settings: null };
    expect(send.needsSettings(fresh)).toBe(true);
    // And the fetch is what changes the answer: asserted against the value it
    // changes FROM, so a no-op cannot pass.
    expect(send.optionsJsonFor(fresh)).toBe(null);
    const loaded = { ...fresh, settings: { dialogueSideMarginPct: 27 } };
    expect(send.needsSettings(loaded)).toBe(false);
    expect(send.optionsJsonFor(loaded)).not.toBe(null);
    // Nothing to fetch from, or nothing to fetch for.
    expect(send.needsSettings({ ...fresh, fountainPath: null })).toBe(false);
    expect(send.needsSettings({ ...fresh, fountainPath: '  ' })).toBe(false);
    expect(send.needsSettings(null)).toBe(false);
    expect(send.needsSettings(undefined)).toBe(false);
  });

  test('what went across is named in the engine’s words, not guessed at', () => {
    expect(send.artifactLine({ label: 'AZW3: for USB sideload to Kindle', path: '/x.azw3' }))
      .toBe('AZW3: for USB sideload to Kindle');
    expect(send.artifactLine({ path: '/x.azw3' })).toBe('/x.azw3');
    expect(send.artifactLine({})).toBe('');
    expect(send.artifactLine(null)).toBe('');
  });

  test('every kind the engine can report is also in the standing list', async () => {
    const { DEVICE_DISPLAY_NAMES } = await import('../src/device/types');
    // The engine's own list, so a fifth vendor added to src/device/types.ts
    // fails here rather than turning up on the surface with no row.
    expect([...send.KINDS].sort()).toEqual(Object.keys(DEVICE_DISPLAY_NAMES).sort());
  });
});

describe('the ladder the send panel asks for, without a toolchain', () => {
  // CI has no Calibre and no Kindle Previewer, and neither does most of this
  // project's audience. So the argv the window builds is fed through the same
  // injection seam src/cli-export.ts already provides, and the three rungs are
  // driven by hand. What is under test is the WINDOW's half: that the argv it
  // builds for a Kindle reaches the Kindle rung carrying this script's own
  // settings, and that what it then prints is the rung that was actually hit.
  let dir: string;
  let epub: string;

  beforeAll(() => {
    dir = mkdtempSync(join(SCRATCH, 'send-'));
    epub = join(dir, 'script.epub');
    writeFileSync(epub, 'not really an epub, but it is a file');
  });

  afterAll(() => { rmSync(dir, { recursive: true, force: true }); });

  /** cli.ts's own argv reading, in miniature: the window's array in, the
   *  exportCommand options out. */
  const asOptions = (args: string[]) => {
    const after = (flag: string) => {
      const i = args.indexOf(flag);
      return i === -1 ? undefined : args[i + 1];
    };
    return {
      epub: args[1],
      for: after('--for'),
      fountain: after('--fountain'),
      optionsJson: after('--options-json'),
    };
  };

  test('a Kindle’s argv reaches the Kindle rung with this script’s settings', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    const sendUi = await import(join(UI, 'send-routes.js'));
    const { exportCommand } = await import('../src/cli-export');
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');

    const script = {
      epubPath: epub,
      fountainPath: join(dir, 'script.fountain'),
      settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 27, justifyText: true },
    };
    const kindle = { id: '/m/Kindle', kind: 'kindle', name: 'Kindle', volume: '/m/Kindle' };
    const args = argv.export(script.epubPath, {
      forFormat: sendUi.forFormat(kindle),
      fountain: script.fountainPath,
      optionsJson: sendUi.optionsJsonFor(script),
    });
    // The argv really is the one the window would build, not one this test
    // wrote for itself.
    expect(args[0]).toBe('export');
    expect(asOptions(args).epub).toBe(epub);
    expect(asOptions(args).for).toBe('kindle');

    let seen: Record<string, unknown> | null = null;
    const result = await exportCommand(asOptions(args), {
      calibreAvailable: () => true,
      kfxStatus: async () => ({ ready: false }) as never,
      freshKindleArtifact: async (opts) => {
        seen = opts.format as unknown as Record<string, unknown>;
        expect(opts.fountainPath).toBe(script.fountainPath);
        return join(dir, 'script.azw3');
      },
    });

    // The whole point: the engine rebuilt with 27%, not with the default.
    expect(seen).not.toBe(null);
    expect(seen!.dialogueSideMarginPct).toBe(27);
    expect(seen!.dialogueSideMarginPct).not.toBe(DEFAULT_FORMAT_OPTIONS.dialogueSideMarginPct);
    expect(seen!.justifyText).toBe(true);
    // And the sentence the surface prints names the rung that was hit.
    expect(sendUi.artifactLine(result)).toContain('AZW3');
  });

  test('each rung of the ladder produces a different sentence on the surface', async () => {
    const sendUi = await import(join(UI, 'send-routes.js'));
    const { exportCommand } = await import('../src/cli-export');

    const rung = async (calibre: boolean, kfx: boolean) => {
      const result = await exportCommand(
        { epub, for: 'kindle' },
        {
          calibreAvailable: () => calibre,
          kfxStatus: async () => ({ ready: kfx }) as never,
          freshKindleArtifact: async () => join(dir, 'script.out'),
        },
      );
      return sendUi.artifactLine(result);
    };

    const best = await rung(true, true);
    const middle = await rung(true, false);
    const worst = await rung(false, false);
    expect(best).toContain('KFX');
    expect(middle).toContain('AZW3');
    expect(worst).toContain('MOBI');
    // Three rungs, three sentences: a surface that printed one constant
    // would pass every "contains" check above on its own.
    expect(new Set([best, middle, worst]).size).toBe(3);

    // And the non-Kindle route does not walk the ladder at all: it hands over
    // the EPUB the library already holds.
    const plain = await exportCommand(
      { epub, for: 'epub' },
      { calibreAvailable: () => false, kfxStatus: async () => ({ ready: false }) as never },
    );
    expect(plain.path).toBe(epub);
    expect(sendUi.artifactLine(plain)).toContain('EPUB');
  });
});

describe('the window does not title its own screens as script furniture', () => {
  // Cut 2026-09-20; see docs/superpowers/specs/2026-09-20-desktop-ui-pass-design.md.
  //
  // This needs a guard rather than a one-off edit because the conceit was not
  // one string, it was NINETEEN across four files, and it spread the way a
  // house style does: each new surface copied the last one. A heading like
  // "Int. the tuning bench - day" tells a reader nothing about the tuning
  // bench, and nine of the nineteen were ERROR headings, where the cost is
  // real: a person whose file was refused was reading set dressing instead of
  // the reason.
  //
  // Deliberately a source scan and not a DOM check. The strings are what
  // spread, and a surface can print one without ever mounting in a test.

  /** Every single- or double-quoted literal in a source file. Good enough for
   *  this window, which has no bundler, no template literals carrying headings
   *  and no quotes inside these strings. */
  function stringLiterals(source: string): string[] {
    return [...source.matchAll(/'([^'\\\n]*)'|"([^"\\\n]*)"/g)]
      .map((m) => m[1] ?? m[2])
      .filter((s) => s !== undefined);
  }

  /** A slugline shape: INT./EXT., something, a dash, a time of day. */
  const SLUGLINE = /^(int|ext)\.\s.+\s-\s.+$/i;
  /** The transitions the window used the same way. */
  const TRANSITION = /^(fade in:|fade out\.|cut to:|dissolve to:)$/i;

  test('no surface is headed with a slugline or a transition', () => {
    const offenders: string[] = [];
    for (const name of jsFiles()) {
      for (const literal of stringLiterals(read(name))) {
        if (SLUGLINE.test(literal) || TRANSITION.test(literal)) {
          offenders.push(`${name}: "${literal}"`);
        }
      }
    }
    expect(offenders.join('\n')).toBe('');
  });

  test('the scan can actually see a slugline, so a pass means something', () => {
    // Guards the test above: a literal-extractor that silently matched
    // nothing would let every heading back in while staying green.
    const seen = stringLiterals(`const a = 'Int. the tuning bench - day';`);
    expect(seen).toContain('Int. the tuning bench - day');
    expect(SLUGLINE.test('Int. the tuning bench - day')).toBe(true);
    expect(TRANSITION.test('Fade in:')).toBe(true);
    // And it does not fire on the window's ordinary prose.
    expect(SLUGLINE.test('Needs selectable text, not a scan.')).toBe(false);
    expect(SLUGLINE.test('Send to a reader')).toBe(false);
  });
});

describe('the update check asks once, stamps first, and never guesses', () => {
  // Contract read off tauri-plugin-updater 2.12.0 by the engine session. The
  // three rules that are easy to get wrong, and are therefore the tests:
  //
  //   1. check() REJECTS on any failure — a 404 manifest, no network, an
  //      unlisted platform. A rejection is a message to show. It is NEVER
  //      "you are up to date", and conflating them would tell someone they
  //      are current when nobody could reach the server.
  //   2. The day-stamp is written BEFORE the request, not after. The README
  //      promises at most one request a day; stamping after would turn a
  //      persistent failure into a request on every single launch.
  //   3. The window never compares versions. pickUpdate does, in code
  //      transpiled from the engine, so the CLI and the window cannot
  //      disagree about what "newer" means.
  let update: any;
  beforeAll(async () => { update = await import(join(UI, 'update.js')); });

  test('it is offered only where the manifest actually covers', () => {
    // The manifest lists darwin only. On Linux and Windows check() throws
    // TargetNotFound every time, so showing the control there would offer a
    // button whose only outcome is an error.
    expect(update.updatesPossible('MacIntel')).toBe(true);
    expect(update.updatesPossible('Win32')).toBe(false);
    expect(update.updatesPossible('Linux x86_64')).toBe(false);
    expect(update.updatesPossible(undefined)).toBe(false);
  });

  test('a fresh install has not opted in and has not been asked', () => {
    const s = update.readState(store());
    expect(s.optedIn).toBe(false);
    expect(s.asked).toBe(false);
    expect(s.lastChecked).toBe(null);
  });

  test('a corrupt stamp does not wedge the check', () => {
    // localStorage is a string bucket anyone can edit. A junk timestamp must
    // read as "never checked" rather than NaN, which would make every
    // comparison false and silently disable the daily check forever.
    const s = update.readState(store({ updateLastChecked: 'yesterday-ish' }));
    expect(s.lastChecked).toBe(null);
  });

  test('a launch check does not run before the reader has opted in', async () => {
    let asked = false;
    const result = await update.runCheck({
      manual: false,
      storage: store({ updateAsked: 'true', updateOptIn: 'false' }),
      now: 1_000_000,
      check: async () => { asked = true; return null; },
    });
    expect(result.outcome).toBe('skipped');
    expect(asked).toBe(false);
  });

  test('a manual check runs even when the launch check would not', async () => {
    // Pressing the button IS the consent for that one request, so it ignores
    // both the opt-in and the once-a-day throttle.
    let asked = false;
    const result = await update.runCheck({
      manual: true,
      storage: store({ updateOptIn: 'false', updateLastChecked: String(1_000_000 - 5) }),
      now: 1_000_000,
      check: async () => { asked = true; return null; },
    });
    expect(asked).toBe(true);
    expect(result.outcome).toBe('current');
  });

  test('the day is stamped before the request, not after', async () => {
    // Captured into an array rather than a variable: assigning inside the
    // closure lets TypeScript narrow a `let` to its initialiser and then
    // reject the comparison.
    const stampedWhenAsked: Array<string | null> = [];
    const s = store({ updateOptIn: 'true' });
    await update.runCheck({
      manual: false,
      storage: s,
      now: 4_242_424,
      check: async () => {
        stampedWhenAsked.push(s.getItem('updateLastChecked'));
        throw 'the network is not there';
      },
    });
    // Written before the call, and still written after it failed.
    expect(stampedWhenAsked[0]).toBe('4242424');
    expect(s.dump().updateLastChecked).toBe('4242424');
  });

  test('a failed check is an error, never "you are up to date"', async () => {
    const result = await update.runCheck({
      manual: true,
      storage: store(),
      now: 1,
      check: async () => { throw 'Could not fetch a valid release JSON from the remote'; },
    });
    expect(result.outcome).toBe('error');
    expect(result.message).toContain('Could not fetch');
  });

  test('an Intel Mac the manifest no longer lists is told why, in plain words', async () => {
    // From the release after the identifier release, latest.json lists
    // darwin-aarch64 only (Intel Macs are not supported; plan
    // 2026-10-01-f2-handover-amendment.md). An Intel Mac running this
    // universal build asks for darwin-x86_64, and tauri-plugin-updater
    // 2.12.0 rejects with TargetsNotFound (or TargetNotFound), whose text
    // is Rust debug output. Recognised by the plugin's own report of the
    // key it looked for, which is the RUNNING binary's arch, and never by
    // navigator.platform, which says "MacIntel" on every Mac. It ships in
    // the identifier release because that is the last build an Intel Mac
    // runs; and it says "this copy", because an Apple silicon Mac running
    // this window under Rosetta asks for darwin-x86_64 too.
    for (const raw of [
      'None of the fallback platforms `["darwin-x86_64-app", "darwin-x86_64"]` were found in the response `platforms` object',
      'None of the fallback platforms `["darwin-x86_64"]` were found in the response `platforms` object',
      'the platform `darwin-x86_64` was not found in the response `platforms` object',
    ]) {
      const s = store({ updateFound: '0.8.0' });
      const result = await update.runCheck({
        manual: true, storage: s, now: 1, check: async () => { throw raw; },
      });
      // Still an error, never "up to date": this Mac is not current, it is
      // past the end of the line.
      expect(result.outcome).toBe('error');
      expect(result.message).toBe(update.INTEL_MAC_RETIRED);
      expect(result.message).toBe(
        'New versions of Screepub run only on Macs with Apple silicon, so this copy will not be '
        + 'offered updates. The version you have keeps working.',
      );
      expect(result.message).not.toContain('this Mac');
      expect(result.message).not.toMatch(/darwin|platforms|`|—/);
      // The server answered, definitively. A version remembered from before
      // (the identifier release, say, found but not installed) can never be
      // installed here, so the label must not keep offering it.
      expect(s.dump().updateFound).toBe('');
    }
    // An Error object, as well as the bare string Tauri's invoke rejects with.
    const fromError = await update.runCheck({
      manual: true, storage: store(), now: 1,
      check: async () => { throw new Error('the platform `darwin-x86_64` was not found in the response `platforms` object'); },
    });
    expect(fromError.message).toBe(update.INTEL_MAC_RETIRED);
  });

  test('every other failure keeps its own text and forgets nothing', async () => {
    // Offline is this app's normal case: a failed request must not erase a
    // remembered offer. And an Apple silicon Mac missing from the manifest
    // is a broken release, whose own text is the useful report.
    for (const raw of [
      'Could not fetch a valid release JSON from the remote',
      'None of the fallback platforms `["darwin-aarch64-app", "darwin-aarch64"]` were found in the response `platforms` object',
      'error sending request for url (https://github.com/darwin-x86_64)',
    ]) {
      const s = store({ updateFound: '0.8.0' });
      const result = await update.runCheck({
        manual: true, storage: s, now: 1, check: async () => { throw raw; },
      });
      expect(result.outcome).toBe('error');
      expect(result.message).toBe(raw);
      expect(s.dump().updateFound).toBe('0.8.0');
    }
  });

  test('null means current, and says so with the engine\'s reason', async () => {
    const result = await update.runCheck({
      manual: true, storage: store(), now: 1, check: async () => null,
    });
    expect(result.outcome).toBe('current');
  });

  test('an offer carries the version and the notes the server sent', async () => {
    const result = await update.runCheck({
      manual: true,
      storage: store(),
      now: 1,
      check: async () => ({ version: '0.7.0', currentVersion: '0.6.0', body: 'Two fixes.' }),
    });
    expect(result.outcome).toBe('offer');
    expect(result.version).toBe('0.7.0');
    expect(result.body).toBe('Two fixes.');
  });

  test('an older or equal version offered by the server is refused here', async () => {
    // The judgement is pickUpdate's, not this module's, and this proves the
    // wiring reaches it: a server that offers a downgrade gets no prompt.
    const result = await update.runCheck({
      manual: true,
      storage: store(),
      now: 1,
      check: async () => ({ version: '0.5.0', currentVersion: '0.6.0', body: '' }),
    });
    expect(result.outcome).toBe('current');
    expect(result.reason).toBeTruthy();
  });

  test('the once-a-day check is actually wired to something', () => {
    // The toggle stores updateOptIn, shouldCheck reads it, runCheck honours
    // it — and for a while NOTHING called runCheck with manual:false, so the
    // preference was a switch connected to no wire. The README promised "one
    // request a day" for behaviour that never ran. A feature that exists in
    // three modules and no caller is indistinguishable from an absent one.
    const flow = read('update-flow.js');
    expect(flow).toContain('runCheck');
    expect(flow).toMatch(/manual:\s*false/);
    expect(read('main.js')).toContain('flow.boot()');
  });

  test('the label and the release notes follow one flow', () => {
    // The ordering bug the old seam existed for (the notes sheet is built at
    // boot and the launch check answers later) is now the flow's job: both
    // subscribe, and a late answer reaches both the same way.
    const main = read('main.js');
    expect(main).toContain('flow.subscribe');
    expect(main).toContain('frame.setUpdateLabel');
    expect(main).toContain('frame.onUpdateClick');
    const notes = read('notes-surface.js');
    expect(notes).toContain('flow.subscribe');
    expect(notes).toContain('flow.start()');
    // Nobody but the flow installs anything.
    for (const name of ['main.js', 'notes-surface.js']) {
      expect(`${name} installs directly: ${read(name).includes('updateInstall')}`)
        .toBe(`${name} installs directly: false`);
    }
  });

  test('without a restart, it asks for a quit and reopen', () => {
    // Only reached by a build without tauri-plugin-process. Every build that
    // carries this line also carries the plugin, so in practice the window
    // restarts instead (installAndRestart); this is the honest fallback.
    const line = update.installedLine('0.7.0');
    expect(line).toContain('0.7.0');
    expect(line.toLowerCase()).toContain('quit');
  });
});

describe('what the update label says, and what it remembers', () => {
  let update: any;
  beforeAll(async () => { update = await import(join(UI, 'update.js')); });

  test('the Convert page asks only where updates work, and only until answered', () => {
    expect(update.shouldAsk(true, store())).toBe(true);
    expect(update.shouldAsk(false, store())).toBe(false);
    // "Said no" is an answer. Only "never asked" is asked.
    expect(update.shouldAsk(true, store({ updateAsked: 'true', updateOptIn: 'false' }))).toBe(false);
    expect(update.shouldAsk(true, store({ updateAsked: 'true', updateOptIn: 'true' }))).toBe(false);
  });

  test('an offer is remembered across launches, and forgotten once this build catches up', async () => {
    const s = store();
    const result = await update.runCheck({
      manual: true, storage: s, now: 1,
      check: async () => ({ version: '0.8.0', currentVersion: '0.7.2', body: '' }),
    });
    expect(result.outcome).toBe('offer');
    // Tomorrow's launch, same build: still behind, so the label comes back.
    expect(update.rememberedOffer(s, '0.7.2')).toBe('0.8.0');
    // After the update: this build IS the remembered version.
    expect(update.rememberedOffer(s, '0.8.0')).toBe(null);
    expect(update.rememberedOffer(store(), '0.7.2')).toBe(null);
  });

  test('a check that finds nothing newer forgets a remembered offer', async () => {
    // A release that was pulled must not leave a label pointing at it.
    const s = store({ updateFound: '0.8.0' });
    await update.runCheck({ manual: true, storage: s, now: 1, check: async () => null });
    expect(update.rememberedOffer(s, '0.7.2')).toBe(null);
  });

  test('a failed check forgets nothing', async () => {
    // No network is not news. The label stays until a check actually answers.
    const s = store({ updateFound: '0.8.0' });
    await update.runCheck({
      manual: true, storage: s, now: 1, check: async () => { throw 'offline'; },
    });
    expect(update.rememberedOffer(s, '0.7.2')).toBe('0.8.0');
  });

  test('a skipped check (the once-a-day throttle) leaves a remembered offer alone', async () => {
    // shouldCheck says no because lastChecked is already `now`: this is what
    // a launch check sees later the same day. No request was made, so there
    // is nothing to have learned, and nothing to forget.
    let asked = false;
    const now = 1_000_000;
    const s = store({ updateFound: '0.8.0', updateOptIn: 'true', updateLastChecked: String(now) });
    const result = await update.runCheck({
      manual: false, storage: s, now, check: async () => { asked = true; return null; },
    });
    expect(result.outcome).toBe('skipped');
    expect(asked).toBe(false);
    expect(update.rememberedOffer(s, '0.7.2')).toBe('0.8.0');
  });

  test('forgetFound empties the remembered version', () => {
    const s = store({ updateFound: '0.8.0' });
    update.forgetFound(s);
    expect(update.rememberedOffer(s, '0.7.2')).toBe(null);
  });

  test('a download reads as a whole percent, and never past 100', () => {
    expect(update.downloadPercent(0, 1000)).toBe(0);
    expect(update.downloadPercent(405, 1000)).toBe(40);
    expect(update.downloadPercent(1000, 1000)).toBe(100);
    // A server that under-reports its length must not produce "140%".
    expect(update.downloadPercent(1400, 1000)).toBe(100);
    // No length from the server: no number at all, rather than a made-up one.
    expect(update.downloadPercent(400, null)).toBe(null);
    expect(update.downloadPercent(400, 0)).toBe(null);
  });

  test('every moment of an update has its own words', () => {
    expect(update.updateLabel(null)).toBe(null);
    expect(update.updateLabel({ kind: 'offer', version: '0.8.0' })).toBe('Update to 0.8.0');
    expect(update.updateLabel({ kind: 'downloading', version: '0.8.0', received: 400, total: 1000 }))
      .toBe('Downloading 0.8.0… 40%');
    expect(update.updateLabel({ kind: 'downloading', version: '0.8.0', received: 400, total: null }))
      .toBe('Downloading 0.8.0…');
    expect(update.updateLabel({ kind: 'installing', version: '0.8.0' })).toBe('Installing…');
    expect(update.updateLabel({ kind: 'waiting', version: '0.8.0' })).toBe('Restarting after this finishes…');
    expect(update.updateLabel({ kind: 'restarting', version: '0.8.0' })).toBe('Restarting…');
    expect(update.updateLabel({ kind: 'failed', version: '0.8.0', message: 'x' })).toBe('Update failed. Try again');
    expect(update.updateLabel({ kind: 'installed', version: '0.8.0' })).toBe(update.installedLine('0.8.0'));
  });

  test('none of it uses an em dash', () => {
    // The owner's rule for text a person reads.
    for (const phase of [
      { kind: 'offer', version: '1' }, { kind: 'downloading', version: '1', received: 1, total: 2 },
      { kind: 'installing' }, { kind: 'waiting' }, { kind: 'restarting' },
      { kind: 'failed', message: '' }, { kind: 'installed', version: '1' },
    ]) {
      expect(update.updateLabel(phase)).not.toContain('—');
    }
  });

  test('the label looks clickable only where a click would do something', () => {
    // 'offer' starts the run; 'failed' retries it. Every other moment is
    // already under way, so clicking it (or the label just looking like
    // it could be clicked) would be a lie.
    expect(update.labelActionable(null)).toBe(false);
    expect(update.labelActionable({ kind: 'offer', version: '1' })).toBe(true);
    expect(update.labelActionable({ kind: 'downloading', version: '1' })).toBe(false);
    expect(update.labelActionable({ kind: 'installing' })).toBe(false);
    expect(update.labelActionable({ kind: 'waiting' })).toBe(false);
    expect(update.labelActionable({ kind: 'restarting' })).toBe(false);
    expect(update.labelActionable({ kind: 'failed', message: 'x' })).toBe(true);
    expect(update.labelActionable({ kind: 'installed', version: '1' })).toBe(false);
    // A retry's re-emitted offer (update-flow.js, start()): the fresh check
    // it triggers is still running, nothing is confirmed, so a click would
    // do nothing yet either.
    expect(update.labelActionable({ kind: 'offer', version: '1', retrying: true })).toBe(false);
  });
});

describe('an update downloads, installs, waits for the engine, then restarts', () => {
  let update: any;
  beforeAll(async () => { update = await import(join(UI, 'update.js')); });

  /** A plugin Update object whose download sends the events tauri-plugin-updater
   *  2.12.0 sends (src/commands.rs: tag "event", content "data", camelCase). */
  const liveOffer = (version = '0.8.0') => ({
    outcome: 'offer', version, body: '',
    update: { version, currentVersion: '0.7.2' },
  });
  const installer = (events: unknown[]) => async (_update: unknown, onProgress: (e: unknown) => void) => {
    for (const event of events) onProgress(event);
  };

  function deps(over: Record<string, unknown> = {}) {
    const phases: unknown[] = [];
    const calls = { check: 0, install: 0, restart: 0 };
    const base = {
      offer: liveOffer(),
      storage: store({ updateOptIn: 'true' }),
      now: 1,
      check: async () => { calls.check += 1; return { version: '0.8.0', currentVersion: '0.7.2', body: '' }; },
      install: async (u: unknown, p: (e: unknown) => void) => {
        calls.install += 1;
        await installer([
          { event: 'Started', data: { contentLength: 1000 } },
          { event: 'Progress', data: { chunkLength: 400 } },
          { event: 'Progress', data: { chunkLength: 600 } },
          { event: 'Finished' },
        ])(u, p);
      },
      busy: () => false,
      whenIdle: async () => {},
      restartReady: () => true,
      restart: async () => { calls.restart += 1; },
      onPhase: (phase: unknown) => { phases.push(phase); },
    };
    return { args: { ...base, ...over }, phases, calls };
  }

  test('each moment is reported in order, and it ends in a restart', async () => {
    // A real download reports progress in network chunks, thousands of them
    // for a bundle this size, but the reader only cares when the WORDS
    // beside the stamp change. Two chunks here (100, then 5) land on the
    // same whole percent (10%) to prove the second one produces no extra
    // phase: the assertion is the de-duplicated sequence of LABELS the
    // reader sees, not a raw count of 'downloading' phases.
    const { args, phases, calls } = deps({
      install: async (u: unknown, p: (e: unknown) => void) => {
        await installer([
          { event: 'Started', data: { contentLength: 1000 } },
          { event: 'Progress', data: { chunkLength: 100 } }, // 10%
          { event: 'Progress', data: { chunkLength: 5 } },   // still 10%: deduped
          { event: 'Progress', data: { chunkLength: 295 } }, // 40%
          { event: 'Progress', data: { chunkLength: 600 } }, // 100%
          { event: 'Finished' },
        ])(u, p);
      },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('restarting');
    expect(outcome.offer).toBe(args.offer); // the offer actually used, not just a version
    expect(calls.restart).toBe(1);
    expect(phases.map((p: any) => update.updateLabel(p))).toEqual([
      'Downloading 0.8.0…',
      'Downloading 0.8.0… 0%',
      'Downloading 0.8.0… 10%',
      'Downloading 0.8.0… 40%',
      'Downloading 0.8.0… 100%',
      'Installing…',
      'Restarting…',
    ]);
  });

  test('downloading names its own body, not whatever was offered before', async () => {
    // A retry's own fresh check (triggered when a failed attempt clears
    // `update`) can find a NEWER version with different release notes than
    // what was last offered. The release notes block needs the new body
    // the moment downloading starts, not only after Finished.
    const { args, phases } = deps({
      offer: {
        outcome: 'offer', version: '0.8.0', body: 'fresh notes',
        update: { version: '0.8.0', currentVersion: '0.7.2' },
      },
    });
    await update.installAndRestart(args);
    const downloading = phases.filter((p: any) => p.kind === 'downloading');
    expect(downloading.length).toBeGreaterThan(0);
    for (const phase of downloading) expect((phase as any).body).toBe('fresh notes');
  });

  test('it waits while the engine is working, and says so', async () => {
    let release: () => void = () => {};
    const idle = new Promise<void>((r) => { release = r; });
    const { args, phases, calls } = deps({ busy: () => true, whenIdle: () => idle });
    const run = update.installAndRestart(args);
    await new Promise((r) => setTimeout(r, 0));
    expect(phases.map((p: any) => p?.kind)).toContain('waiting');
    expect(calls.restart).toBe(0);
    release();
    await run;
    expect(calls.restart).toBe(1);
    expect((phases.at(-1) as any).kind).toBe('restarting');
  });

  test('a label drawn from memory fetches a fresh update first, and installs exactly what that check returned', async () => {
    // The plugin's Update object lives only as long as the session that
    // fetched it, so a remembered version has none. The click is the consent
    // for one request, even with the daily check switched off. And what
    // reaches install() is that request's own answer object, by reference,
    // not a stand-in built somewhere else.
    const sentinel = { version: '0.8.0', currentVersion: '0.7.2', body: '' };
    let checkCalls = 0;
    let installedWith: unknown = null;
    const { args } = deps({
      offer: { outcome: 'offer', version: '0.8.0', body: '', update: null },
      storage: store({ updateOptIn: 'false' }),
      check: async () => { checkCalls += 1; return sentinel; },
      install: async (u: unknown, p: (e: unknown) => void) => {
        installedWith = u;
        await installer([{ event: 'Finished' }])(u, p);
      },
    });
    const outcome = await update.installAndRestart(args);
    expect(checkCalls).toBe(1);
    expect(installedWith).toBe(sentinel);
    expect(outcome.outcome).toBe('restarting');
  });

  test('every outcome that has an offer reports the one actually used, not the one passed in', async () => {
    // The offer passed in can be a remembered stub with no live Update
    // object and an empty body. Once the fresh check inside replaces it,
    // the caller (update-flow.js) has no other way to learn the real
    // version and body a failed attempt was actually for.
    const live = { version: '0.8.1', currentVersion: '0.7.2', body: 'release notes' };
    const { args, phases } = deps({
      offer: { outcome: 'offer', version: '0.8.0', body: '', update: null },
      check: async () => live,
      install: async () => { throw new Error('interrupted'); },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('error');
    expect(outcome.offer?.version).toBe('0.8.1');
    expect(outcome.offer?.body).toBe('release notes');
    expect((phases.at(-1) as any).version).toBe('0.8.1');
  });

  test('an offline retry of a remembered label keeps the remembered offer, not the error that overwrote it', async () => {
    // Offline is this app's NORMAL case (built for a Kindle that is often
    // disconnected). A label drawn from memory has no live Update object,
    // so this function runs its own fresh check on the click's behalf --
    // and when THAT check fails, `result` must still be the offer this
    // call started with, not the error result that would otherwise
    // overwrite it before the throw.
    const { args, phases } = deps({
      offer: { outcome: 'offer', version: '0.8.0', body: 'notes', update: null },
      check: async () => { throw new Error('no network'); },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('error');
    expect(outcome.offer).toBe(args.offer);
    expect((phases.at(-1) as any).version).toBe('0.8.0');
  });

  test('if the fresh check finds nothing newer, nothing is installed', async () => {
    const s = store({ updateFound: '0.8.0' });
    const { args, phases, calls } = deps({
      offer: { outcome: 'offer', version: '0.8.0', body: '', update: null },
      storage: s,
      check: async () => null,
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('current');
    expect(calls.install).toBe(0);
    expect(phases.at(-1)).toBe(null);
    expect(update.rememberedOffer(s, '0.7.2')).toBe(null);
  });

  test('a failed download is reported with its reason, and nothing restarts', async () => {
    const { args, phases, calls } = deps({
      install: async () => { throw new Error('signature did not verify'); },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('error');
    expect(outcome.message).toContain('signature');
    expect(calls.restart).toBe(0);
    const failed = phases.at(-1) as any;
    expect(failed.kind).toBe('failed');
    expect(failed.message).toContain('signature');
    expect(update.updateLabel(failed)).toBe('Update failed. Try again');
  });

  test('a failed fresh check is a failure, not "nothing newer"', async () => {
    const { args, phases } = deps({
      offer: { outcome: 'offer', version: '0.8.0', body: '', update: null },
      check: async () => { throw 'offline'; },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('error');
    expect((phases.at(-1) as any).kind).toBe('failed');
  });

  test('without the restart plugin it installs and asks for a quit and reopen', async () => {
    const { args, phases, calls } = deps({ restartReady: () => false });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('installed');
    expect(outcome.offer).toBe(args.offer);
    expect(calls.restart).toBe(0);
    expect(update.updateLabel(phases.at(-1))).toBe(update.installedLine('0.8.0'));
  });

  test('a rejected restart is not reported as a failed update: the bundle is already swapped', async () => {
    // The realistic cause is a build missing process:allow-restart, not a
    // broken install. "Update failed. Try again" would draw a button that
    // reinstalls a bundle already on disk, in a loop, so this falls back to
    // the same honest line a build without the plugin at all shows.
    const { args, phases } = deps({
      restart: async () => { throw new Error('process:allow-restart is not granted'); },
    });
    const outcome = await update.installAndRestart(args);
    expect(outcome.outcome).toBe('installed');
    expect(outcome.offer).toBe(args.offer);
    expect(update.updateLabel(phases.at(-1))).toBe(update.installedLine('0.8.0'));
  });
});

describe('one update run, one moment, heard by the label and the notes alike', () => {
  let flowMod: any;
  beforeAll(async () => { flowMod = await import(join(UI, 'update-flow.js')); });

  function make(over: Record<string, unknown> = {}) {
    // `s` already resolves over.storage (a STORE, not a function) if given.
    // `...over` must come before the explicit `storage: () => s` below, or a
    // caller's raw store object would overwrite that wrapper directly and
    // every deps.storage() call downstream would throw "not a function".
    const s = (over.storage as ReturnType<typeof store>) ?? store({ updateOptIn: 'true', updateAsked: 'true' });
    const calls = { check: 0, install: 0, restart: 0 };
    const flow = flowMod.createUpdateFlow({
      usable: () => true,
      now: () => 1,
      check: async () => { calls.check += 1; return null; },
      install: async () => { calls.install += 1; },
      busy: () => false,
      whenIdle: async () => {},
      restartReady: () => true,
      restart: async () => { calls.restart += 1; },
      currentVersion: '0.7.2',
      ...over,
      storage: () => s,
    });
    const seen: unknown[] = [];
    flow.subscribe((phase: unknown) => seen.push(phase));
    return { flow, s, calls, seen };
  }

  test('a remembered version is drawn at launch, before today\'s check answers', async () => {
    let answer: (v: unknown) => void = () => {};
    const { flow, seen } = make({
      storage: store({ updateOptIn: 'true', updateAsked: 'true', updateFound: '0.8.0' }),
      check: () => new Promise((r) => { answer = r; }),
    });
    const booting = flow.boot();
    expect((seen.at(-1) as any)?.kind).toBe('offer');
    expect((seen.at(-1) as any)?.version).toBe('0.8.0');
    answer({ version: '0.8.0', currentVersion: '0.7.2', body: '' });
    await booting;
    expect(flow.currentOffer()?.update).toBeTruthy(); // the live object replaced the remembered one
  });

  test('a live check\'s body reaches subscribers even when the version repeats a remembered offer', async () => {
    // Boot draws the remembered stub first (empty body), then the live
    // check answers. When the version is UNCHANGED, a subscriber that
    // skips a redraw on the same version must still see the live body, or
    // it is stuck showing empty notes for an offer that has real ones.
    let answer: (v: unknown) => void = () => {};
    const { flow, seen } = make({
      storage: store({ updateOptIn: 'true', updateAsked: 'true', updateFound: '0.8.0' }),
      check: () => new Promise((r) => { answer = r; }),
    });
    const booting = flow.boot();
    expect((seen.at(-1) as any)?.body).toBe('');
    answer({ version: '0.8.0', currentVersion: '0.7.2', body: 'what is new' });
    await booting;
    expect((seen.at(-1) as any)?.kind).toBe('offer');
    expect((seen.at(-1) as any)?.version).toBe('0.8.0');
    expect((seen.at(-1) as any)?.body).toBe('what is new');
  });

  test('a build that has caught up forgets the remembered version and draws nothing', async () => {
    const s = store({ updateOptIn: 'false', updateAsked: 'true', updateFound: '0.7.2' });
    const { flow, seen } = make({ storage: s });
    await flow.boot();
    expect(seen.every((p) => p === null)).toBe(true);
    expect(s.dump().updateFound).toBe('');
  });

  test('where updates cannot happen, boot touches nothing', async () => {
    const s = store({ updateFound: '0.8.0' });
    const { flow, calls, seen } = make({ usable: () => false, storage: s });
    await flow.boot();
    expect(calls.check).toBe(0);
    expect(seen.every((p) => p === null)).toBe(true);
    expect(s.dump().updateFound).toBe('0.8.0');
  });

  test('boot only ever runs once: a second call cannot replace a live offer with a remembered stub', async () => {
    // A fixed clock would make the daily throttle the thing stopping a
    // second check, not the run-once guard this test is actually about:
    // `now` moves a day forward before the second boot, so WITHOUT the
    // guard the throttle would allow another check and calls.check would
    // become 2.
    let now = 1;
    const s = store({ updateOptIn: 'true', updateAsked: 'true', updateFound: '0.8.0' });
    const { flow, calls } = make({
      storage: s,
      now: () => now,
      check: async () => { calls.check += 1; return { version: '0.8.0', currentVersion: '0.7.2', body: 'notes' }; },
    });
    await flow.boot();
    expect(calls.check).toBe(1);
    expect(flow.currentOffer()?.update).toBeTruthy();
    now += 24 * 60 * 60 * 1000;
    await flow.boot();
    // A second boot must not redraw the remembered stub over the live
    // offer the first boot already fetched.
    expect(calls.check).toBe(1);
    expect(flow.currentOffer()?.update).toBeTruthy();
  });

  test('a check that answers "nothing newer" takes a remembered label down', async () => {
    const { flow, seen } = make({
      storage: store({ updateOptIn: 'true', updateAsked: 'true', updateFound: '0.8.0' }),
      check: async () => null,
    });
    await flow.boot();
    expect(seen.at(-1)).toBe(null);
    expect(flow.currentOffer()).toBe(null);
  });

  test('currentOffer() matches what was actually attempted: remembered 0.8.0, live check finds 0.8.1, install fails', async () => {
    // The probe from review: a label drawn from memory has no live Update
    // object, so start() triggers installAndRestart's own fresh check.
    // Before the fix, that live result never reached this flow's `offer`:
    // the phase said "failed 0.8.1" while currentOffer() still pointed at
    // the stale, empty-body 0.8.0 stub, even though storage (runCheck's own
    // doing) already said 0.8.1.
    const s = store({ updateOptIn: 'true', updateAsked: 'true' });
    const { flow, seen } = make({
      storage: s,
      check: async () => ({ version: '0.8.1', currentVersion: '0.7.2', body: 'release notes' }),
      install: async () => { throw new Error('interrupted'); },
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: null });
    await flow.start();
    expect((seen.at(-1) as any).kind).toBe('failed');
    expect((seen.at(-1) as any).version).toBe('0.8.1');
    expect(flow.currentOffer()?.version).toBe('0.8.1');
    expect(flow.currentOffer()?.body).toBe('release notes');
    expect(s.dump().updateFound).toBe('0.8.1');
  });

  test('an offline retry of a remembered label keeps currentOffer() and the failed phase at the remembered version', async () => {
    // Offline is this app's normal case. Before the fix, a failed fresh
    // check overwrote `result` with the error object itself, so the failed
    // phase named version null and currentOffer() lost its version and
    // body entirely.
    const s = store({ updateOptIn: 'true', updateAsked: 'true' });
    const { flow, seen } = make({
      storage: s,
      check: async () => { throw new Error('no network'); },
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: 'notes', update: null });
    await flow.start();
    expect((seen.at(-1) as any).kind).toBe('failed');
    expect((seen.at(-1) as any).version).toBe('0.8.0');
    expect(flow.currentOffer()?.version).toBe('0.8.0');
    expect(flow.currentOffer()?.body).toBe('notes');
  });

  test('saying yes checks at once; saying no sends nothing', async () => {
    const yes = make({ storage: store() });
    await yes.flow.answer(true);
    expect(yes.calls.check).toBe(1);
    expect(yes.s.dump().updateOptIn).toBe('true');
    expect(yes.s.dump().updateAsked).toBe('true');

    const no = make({ storage: store() });
    await no.flow.answer(false);
    expect(no.calls.check).toBe(0);
    expect(no.s.dump().updateOptIn).toBe('false');
    expect(no.s.dump().updateAsked).toBe('true');
  });

  test('checkAnswered does exactly what launchCheck does with a result: offer or "nothing newer"', async () => {
    // The bug: the notes' manual "Check for updates" used to handle an
    // offer/current result itself, bypassing the flow entirely. On
    // "nothing newer" it cleared the remembered version and said "newest
    // there is" in the notes, but the flow's OWN `offer` and `phase` were
    // untouched — so the label still said "Update to X" and the notes'
    // Install button stayed enabled with the launch check's live handle: a
    // click would install a release the server had just withdrawn.
    // checkAnswered is the one place a result becomes a phase, used by
    // both launchCheck and the manual button now.
    const s = store({ updateOptIn: 'true', updateAsked: 'true' });
    const { flow, seen } = make({ storage: s });
    flow.checkAnswered({ version: '0.8.0', currentVersion: '0.7.2', body: 'notes', outcome: 'offer' });
    expect((seen.at(-1) as any).kind).toBe('offer');
    expect((seen.at(-1) as any).version).toBe('0.8.0');
    expect(flow.currentOffer()?.version).toBe('0.8.0');
  });

  test('checkAnswered("current") clears an offer the LABEL is showing, not just the notes\' own message', async () => {
    const { flow, seen } = make();
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: 'notes', update: { version: '0.8.0' } });
    expect((seen.at(-1) as any).kind).toBe('offer');
    flow.checkAnswered({ outcome: 'current' });
    // The phase every subscriber hears (the label AND the notes) goes null
    // together: there is no way for one to still say "Update to 0.8.0"
    // while the other says "is the newest there is".
    expect(seen.at(-1)).toBe(null);
    expect(flow.currentOffer()).toBe(null);
  });

  test('checkAnswered("current") while a run is under way does not erase what is actually installing', async () => {
    // Mirrors launchCheck's own `&& !running` guard: a manual click's
    // result answering for a DIFFERENT, now-stale check must not clear the
    // offer a run already in flight (started from the label, say) is
    // actually using.
    let release: () => void = () => {};
    const idle = new Promise<void>((r) => { release = r; });
    const { flow, seen } = make({ busy: () => true, whenIdle: () => idle });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0));
    flow.checkAnswered({ outcome: 'current' });
    expect((seen.at(-1) as any).kind).toBe('waiting');
    expect(flow.currentOffer()).not.toBe(null);
    release();
    await run;
  });

  test('checkAnswered is what launchCheck itself now calls, so boot and answer(true) go through the same door', () => {
    expect(flowMod.flow.checkAnswered).toBeInstanceOf(Function);
    const src = read('update-flow.js');
    expect(src).toMatch(/async function launchCheck\(\)\s*\{[\s\S]*?checkAnswered\(result\)/);
  });

  test('in notesView terms: the withdrawn-release bug stays fixed end to end', async () => {
    // Reproduces the exact report: an offer is on screen (Install enabled,
    // holding a live Update handle), a manual check answers "current".
    // checkAnswered clears the flow's own offer and phase; notesView, fed
    // the resulting null phase with drewOffer already true, is what turns
    // that into "is the newest there is" with Install hidden and disabled
    // — the same object the label beside the stamp reads too.
    const notesMod = await import(join(UI, 'notes-surface.js'));
    const { RELEASE } = await import(join(UI, 'notes.js'));
    const { flow, seen } = make();
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: 'notes', update: { version: '0.8.0' } });
    flow.checkAnswered({ outcome: 'current' });
    expect(seen.at(-1)).toBe(null);
    const view = notesMod.notesView(null, { drewOffer: true });
    expect(view.say).toBe(`Screepub ${RELEASE.version} is the newest there is.`);
    expect(view.install).toEqual({ text: '', hidden: true, disabled: true });
  });

  test('two clicks run one update', async () => {
    const { flow, calls } = make();
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    await Promise.all([flow.start(), flow.start()]);
    expect(calls.install).toBe(1);
    expect(calls.restart).toBe(1);
  });

  test('after a failed install, trying again fetches a fresh update', async () => {
    let installs = 0;
    const { flow, calls, seen } = make({
      install: async () => { installs += 1; if (installs === 1) throw new Error('interrupted'); },
      // Counted so this test can prove a retry asked the server again,
      // matching every other `check` fake in this file.
      check: async () => { calls.check += 1; return { version: '0.8.0', currentVersion: '0.7.2', body: '' }; },
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    await flow.start();
    expect((seen.at(-1) as any).kind).toBe('failed');
    expect(calls.check).toBe(0);
    await flow.start();
    // The first Update object was closed by the failed attempt, so the retry
    // asked the server for a new one.
    expect(calls.check).toBe(1);
    expect(installs).toBe(2);
    expect((seen.at(-1) as any).kind).toBe('restarting');
  });

  test('a retry visibly leaves the failed phase before its own fresh check answers', async () => {
    // A retry's fresh check (installAndRestart's own, since the failed
    // attempt cleared `update`) can take a while. Until it answers, the
    // phase must not still read "failed": that is the PREVIOUS attempt,
    // and a re-emit mid-check (offerFound, from a background check landing
    // at the same moment) would otherwise repeat that stale failure.
    let answer: (v: unknown) => void = () => {};
    const { flow, seen } = make({
      install: async () => { throw new Error('interrupted'); }, // every attempt fails at install
      check: () => new Promise((r) => { answer = r; }),
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: 'notes', update: { version: '0.8.0' } });
    await flow.start(); // first attempt: offer.update is live, no check(), fails at install
    expect((seen.at(-1) as any).kind).toBe('failed');
    const retry = flow.start(); // second attempt: offer.update is now null, triggers check()
    await new Promise((r) => setTimeout(r, 0)); // let start() reach the pending check
    expect((seen.at(-1) as any).kind).toBe('offer');
    expect((seen.at(-1) as any).version).toBe('0.8.0');
    expect((seen.at(-1) as any).body).toBe('notes');
    // Marked, so a subscriber (notes-surface.js) can keep Install disabled
    // for this specific moment: the fresh check is running and nothing
    // yet to click has actually been confirmed.
    expect((seen.at(-1) as any).retrying).toBe(true);
    answer({ version: '0.8.0', currentVersion: '0.7.2', body: 'notes' });
    await retry;
  });

  test('a new offer does not interrupt a run already under way', async () => {
    let release: () => void = () => {};
    const idle = new Promise<void>((r) => { release = r; });
    const { flow, seen } = make({ busy: () => true, whenIdle: () => idle });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0));
    flow.offerFound({ outcome: 'offer', version: '0.8.1', body: '', update: { version: '0.8.1' } });
    expect((seen.at(-1) as any).kind).toBe('waiting');
    release();
    await run;
  });

  test('offerFound re-emits the current phase while a run is under way, so a "Looking…" caption does not stick', async () => {
    let release: () => void = () => {};
    const idle = new Promise<void>((r) => { release = r; });
    const { flow, seen } = make({ busy: () => true, whenIdle: () => idle });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0));
    const before = seen.length;
    flow.offerFound({ outcome: 'offer', version: '0.8.1', body: '', update: { version: '0.8.1' } });
    // Nothing about the visible phase changed (still 'waiting'), but a
    // subscriber driven by "did I hear anything new" rather than "did the
    // value change" must still hear it, or it sticks on its own last words
    // forever.
    expect(seen.length).toBeGreaterThan(before);
    expect((seen.at(-1) as any).kind).toBe('waiting');
    release();
    await run;
  });

  test('a newer offer that arrived mid-run keeps its own live handle: the finished run does not touch it, and shows it instead of its own stale failure', async () => {
    let failInstall: (e: unknown) => void = () => {};
    const pending = new Promise((_resolve, reject) => { failInstall = reject; });
    const { flow, calls, seen } = make({
      install: async () => { await pending; },
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0)); // let start() reach the pending install
    const newer = { outcome: 'offer', version: '0.8.1', body: 'what is new', update: { version: '0.8.1' } };
    flow.offerFound(newer);
    failInstall(new Error('interrupted'));
    await run;
    // The run that just failed was attempting 0.8.0, not this one: clearing
    // ITS `update` must not reach the 0.8.1 offer that replaced it, or the
    // next start() throws away a live handle nobody used and re-checks for
    // nothing. And the visible phase must say what a click would actually
    // install NOW (0.8.1, with its own body), not the stale failure that
    // was really about 0.8.0.
    expect(flow.currentOffer()).toBe(newer);
    expect(flow.currentOffer()?.update).toBeTruthy();
    expect((seen.at(-1) as any).kind).toBe('offer');
    expect((seen.at(-1) as any).version).toBe('0.8.1');
    expect((seen.at(-1) as any).body).toBe('what is new');
    await flow.start();
    expect(calls.check).toBe(0);
  });

  test('a same-version offer landing mid-run does not erase the failure it just caused', async () => {
    // A reviewer's last finding: a launch check can answer with the very
    // version a click is installing right now (the launch check answered
    // the same 0.8.0 while a click's install of 0.8.0 was running, and the
    // install failed). Before the fix, offerFound's newer object reached the
    // newer-offer branch below on version alone, and replaced the failed
    // phase with "Update to 0.8.0" the instant the install's catch block
    // set it, so the reader never saw why it failed.
    let failInstall: (e: unknown) => void = () => {};
    const pending = new Promise((_resolve, reject) => { failInstall = reject; });
    const { flow, seen } = make({
      install: async () => { await pending; },
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0)); // let start() reach the pending install
    const sameVersion = { outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } };
    flow.offerFound(sameVersion);
    failInstall(new Error('interrupted'));
    await run;
    expect((seen.at(-1) as any).kind).toBe('failed');
    // `offer` still moved to the newer object, so a "Try again" uses its
    // live handle rather than triggering a fresh check.
    expect(flow.currentOffer()).toBe(sameVersion);
  });

  test('a newer offer mid-run survives a fresh check that answers "nothing newer" for the run it interrupted', async () => {
    let answer: (v: unknown) => void = () => {};
    const s = store({ updateOptIn: 'true', updateAsked: 'true' });
    const { flow, seen } = make({
      storage: s,
      check: () => new Promise((r) => { answer = r; }),
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: null });
    const run = flow.start();
    await new Promise((r) => setTimeout(r, 0)); // let start() reach the pending check
    const newer = { outcome: 'offer', version: '0.8.1', body: 'what is new', update: { version: '0.8.1' } };
    flow.offerFound(newer);
    answer(null); // this run's OWN check finds nothing newer than 0.8.0
    await run;
    // The phase must not go null just because 0.8.0's own check found
    // nothing: 0.8.1 is still held, and a click now would install THAT.
    expect((seen.at(-1) as any).kind).toBe('offer');
    expect((seen.at(-1) as any).version).toBe('0.8.1');
    expect((seen.at(-1) as any).body).toBe('what is new');
    expect(flow.currentOffer()).toBe(newer);
    // runCheck's own forgetFound (0.8.0's check found nothing) must not be
    // the last word: storage has to agree with what is actually on screen.
    expect(s.dump().updateFound).toBe('0.8.1');
  });

  test('start(), a fresh check says "nothing newer", and nothing newer arrived meanwhile: the phase and the offer both clear', async () => {
    const s = store({ updateOptIn: 'true', updateAsked: 'true', updateFound: '0.8.0' });
    const { flow, seen } = make({
      storage: s,
      check: async () => null,
    });
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: null });
    await flow.start();
    expect(seen.at(-1)).toBe(null);
    expect(flow.currentOffer()).toBe(null);
    expect(s.dump().updateFound).toBe('');
  });

  test('a broken subscriber does not stop the others from hearing it, or stop the run', async () => {
    // A reviewer's concern: one subscriber's bug (a typo in frame.js, say)
    // must not silence the OTHER subscriber, and must not turn a restart
    // that would otherwise succeed into a reported failure. setPhase logs a
    // broken listener with console.error rather than swallowing it outright
    // (so there is still something to debug from), which this test captures
    // instead of letting it print noise — and checks it actually fired.
    const { flow, calls } = make();
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    let heard = 0;
    flow.subscribe(() => {
      heard += 1;
      // Not on the very first call: that one fires synchronously inside
      // subscribe() itself, before the run even starts.
      if (heard > 1) throw new Error('broken subscriber');
    });
    const seenGood: unknown[] = [];
    flow.subscribe((phase: unknown) => seenGood.push(phase));
    const logged: unknown[][] = [];
    const originalError = console.error;
    console.error = (...args: unknown[]) => { logged.push(args); };
    try {
      await flow.start();
    } finally {
      console.error = originalError;
    }
    expect(calls.restart).toBe(1);
    expect(seenGood.some((p: any) => p?.kind === 'restarting')).toBe(true);
    expect(logged.length).toBeGreaterThan(0);
    expect(String(logged[0]?.[0])).toContain('a subscriber threw');
  });

  test('subscribe does not throw when the new listener itself does, on the very first call', () => {
    // subscribe() calls the new listener at once with the current phase.
    // That call must go through the same guard as every later one, or a
    // broken listener crashes the SUBSCRIBE, not just a later notification.
    // The guard logs via console.error, captured here rather than left to
    // print noise for an expected throw.
    const { flow } = make();
    const originalError = console.error;
    console.error = () => {};
    try {
      expect(() => flow.subscribe(() => { throw new Error('boom'); })).not.toThrow();
    } finally {
      console.error = originalError;
    }
  });

  test('an exception starting the run resolves as a failed phase, not a rejection, and does not leave `running` stuck true', async () => {
    // Wired straight to click handlers with no catch of their own, so
    // start() must resolve, not reject, and say something rather than
    // leave the label wherever it was. A storage accessor that throws once
    // stands in for anything that could fail on the way in — real
    // localStorage can throw in a private window — and is independent of
    // the subscriber guard above: it never reaches onPhase at all. make()'s
    // `storage` override always wraps a STORE, not a function, so this flow
    // is built directly rather than through make().
    let storageCalls = 0;
    const goodStorage = store({ updateOptIn: 'true' });
    const calls = { check: 0, install: 0, restart: 0 };
    const seen: unknown[] = [];
    const flow = flowMod.createUpdateFlow({
      usable: () => true,
      storage: () => {
        storageCalls += 1;
        if (storageCalls === 1) throw new Error('storage unavailable');
        return goodStorage;
      },
      now: () => 1,
      check: async () => { calls.check += 1; return null; },
      install: async () => { calls.install += 1; },
      busy: () => false,
      whenIdle: async () => {},
      restartReady: () => true,
      restart: async () => { calls.restart += 1; },
      currentVersion: '0.7.2',
    });
    flow.subscribe((phase: unknown) => seen.push(phase));
    flow.offerFound({ outcome: 'offer', version: '0.8.0', body: '', update: { version: '0.8.0' } });
    await flow.start(); // must not throw
    const failed = seen.at(-1) as any;
    expect(failed?.kind).toBe('failed');
    expect(failed?.version).toBe('0.8.0');
    expect(failed?.message).toContain('storage unavailable');
    // If `running` were stuck true, this would silently do nothing and
    // `calls.restart` would stay 0.
    await flow.start();
    expect(calls.restart).toBe(1);
  });

  test('importing update-flow.js touches no window: storage, navigator and the clock are read lazily', () => {
    // The flow object itself, and RELEASE.version (notes.js is pure data),
    // ARE built and read at import. What must NOT be read at import is
    // anything that only exists inside a window: localStorage, navigator
    // and the clock (Date.now, the only one this file uses — app.js is the
    // one with performance.now), same as every Tauri call. Each is a
    // closure here, called only when a method runs — otherwise this module
    // could not be imported by bun test at all.
    expect(typeof flowMod.flow.boot).toBe('function');
    const src = read('update-flow.js');
    expect(src).toContain('storage: () => localStorage');
    expect(src).toContain('now: () => Date.now()');
    // navigator is read only inside the `usable` closure, not at import.
    expect(src).toMatch(/usable:\s*\(\)\s*=>[\s\S]*?navigator/);
    expect(src).not.toMatch(/__TAURI__/); // app.js is the only file that touches Tauri
  });
});

describe('what the Updates block in the release notes shows for one moment', () => {
  // Pulled out of the subscriber as a pure function so every moment can be
  // checked directly, without mounting a surface. `state` is what the block
  // has already drawn: has an offer ever been shown this session, and the
  // version/body an offer or a download most recently named (installing,
  // waiting, restarting, failed and installed carry no body of their own).
  let notes: any;
  let RELEASE: any;
  beforeAll(async () => {
    notes = await import(join(UI, 'notes-surface.js'));
    ({ RELEASE } = await import(join(UI, 'notes.js')));
  });

  test('null before any offer leaves say alone: this is the surface\'s own idle state', () => {
    const view = notes.notesView(null, { drewOffer: false });
    expect(view.say).toBeUndefined();
    expect(view.body).toEqual({ text: '', hidden: true });
    expect(view.install).toEqual({ text: '', hidden: true, disabled: true });
    expect(view.checkDisabled).toBe(false);
  });

  test('offer: names the version, shows the body it carries, and Install is ready', () => {
    const view = notes.notesView(
      { kind: 'offer', version: '0.8.0', body: 'notes' },
      { drewOffer: false },
    );
    expect(view.say).toBe('Screepub 0.8.0 is available.');
    expect(view.body).toEqual({ text: 'notes', hidden: false });
    expect(view.install).toEqual({ text: 'Install 0.8.0', hidden: false, disabled: false });
    expect(view.checkDisabled).toBe(false);
  });

  test('offer with no body: the paragraph stays hidden rather than show empty prose', () => {
    const view = notes.notesView({ kind: 'offer', version: '0.8.0', body: '' }, { drewOffer: false });
    expect(view.body).toEqual({ text: '', hidden: true });
  });

  test('a retry\'s offer moment (retrying: true) keeps Install disabled: nothing is confirmed yet', () => {
    const view = notes.notesView(
      { kind: 'offer', version: '0.8.0', body: 'notes', retrying: true },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('Screepub 0.8.0 is available.');
    expect(view.install).toEqual({ text: 'Install 0.8.0', hidden: false, disabled: true });
  });

  test('downloading: names its own body (a retry can find a newer version), disables Check', () => {
    // The version and body here are NEWER than what state remembers: a
    // retry's own fresh check found 0.8.1, with different notes than the
    // 0.8.0 that was last offered.
    const view = notes.notesView(
      { kind: 'downloading', version: '0.8.1', received: 400, total: 1000, body: 'new notes' },
      { drewOffer: true, version: '0.8.0', body: 'old notes' },
    );
    expect(view.say).toBe('Downloading 0.8.1… 40%');
    expect(view.body).toEqual({ text: 'new notes', hidden: false });
    expect(view.install).toEqual({ text: 'Install 0.8.1', hidden: false, disabled: true });
    expect(view.checkDisabled).toBe(true);
  });

  test('installing: keeps naming whatever version and body were last drawn', () => {
    const view = notes.notesView(
      { kind: 'installing', version: '0.8.0' },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('Installing…');
    expect(view.body).toEqual({ text: 'notes', hidden: false });
    expect(view.install).toEqual({ text: 'Install 0.8.0', hidden: false, disabled: true });
    expect(view.checkDisabled).toBe(true);
  });

  test('waiting: says why the restart is waiting, and disables Check', () => {
    const view = notes.notesView(
      { kind: 'waiting', version: '0.8.0' },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('Restarting after this finishes…');
    expect(view.checkDisabled).toBe(true);
  });

  test('restarting: disables Check (a manual "current" now would contradict the label)', () => {
    const view = notes.notesView(
      { kind: 'restarting', version: '0.8.0' },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('Restarting…');
    expect(view.checkDisabled).toBe(true);
  });

  test('failed: the full reason, Install re-enabled to retry, Check left alone', () => {
    const view = notes.notesView(
      { kind: 'failed', version: '0.8.0', message: 'no network' },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('no network');
    expect(view.body).toEqual({ text: 'notes', hidden: false });
    expect(view.install).toEqual({ text: 'Install 0.8.0', hidden: false, disabled: false });
    // NOT in the disabling list: a manual check is exactly how a reader
    // stuck on a failure finds out whether a newer fix has since shipped.
    expect(view.checkDisabled).toBe(false);
  });

  test('installed: the fallback line, Check stays disabled (the bundle is already swapped)', () => {
    const view = notes.notesView(
      { kind: 'installed', version: '0.8.0' },
      { drewOffer: true, version: '0.8.0', body: 'notes' },
    );
    expect(view.say).toBe('Update installed. Quit and reopen Screepub to use 0.8.0.');
    expect(view.checkDisabled).toBe(true);
  });

  test('null after an offer: a check found nothing newer, so say so', () => {
    const view = notes.notesView(null, { drewOffer: true });
    expect(view.say).toBe(`Screepub ${RELEASE.version} is the newest there is.`);
    expect(view.body).toEqual({ text: '', hidden: true });
    expect(view.install).toEqual({ text: '', hidden: true, disabled: true });
  });
});

describe('the release notes switch actually shows what it is set to', () => {
  test('checked is the boolean itself, not \'\' (which el() sets as false)', () => {
    // el()'s checked branch does `node.checked = value` directly: passing
    // the ternary's '' set node.checked to the STRING's truthiness in the
    // JSX-ish reading a reviewer expects, but assigning a DOM boolean
    // property coerces '' to false regardless of intent, so the switch
    // always rendered off.
    const notes = read('notes-surface.js');
    expect(notes).toContain('checked: state.optedIn,');
    expect(notes).not.toMatch(/checked:\s*state\.optedIn\s*\?\s*''\s*:\s*null/);
  });

  test('a "Turn on" given on the Convert page reaches the switch before the sheet opens', () => {
    // The sheet is built once at boot; readState(localStorage) was read
    // once then too. A later answer on the Convert page never reached this
    // checkbox until relaunch.
    const notes = read('notes-surface.js');
    expect(notes).toContain('export function show()');
    expect(notes).toMatch(/show\(\)\s*\{[\s\S]*?readState\(localStorage\)/);
    const main = read('main.js');
    expect(main).toMatch(/notes\.show\(\);\s*\n\s*notesSheet\.showModal\(\);/);
  });

  test('flipping it on checks at once, the same way the Convert question does', () => {
    // rememberAnswer alone stamped the answer but never ran a check: someone
    // who turns it on from the release notes on a release day had to wait
    // for tomorrow's throttle to lift rather than hear about it this
    // session, unlike the identical question under the drop well
    // (convert.js's askLine, main.js's ctx.updates.answer), which already
    // goes through flow.answer for exactly this reason.
    const notes = read('notes-surface.js');
    const onchange = notes.slice(notes.indexOf('id: \'update-auto\''), notes.indexOf('});\n  autoCheckbox'));
    expect(onchange).toContain('flow.answer(event.target.checked)');
    expect(onchange).not.toContain('rememberAnswer(');
    // The status line's own words are unchanged.
    expect(notes).toContain('Screepub will look once a day, and only for this file.');
    expect(notes).toContain('Screepub will not look on its own.');
  });
});

describe('"Check for updates" does not race the flow\'s own run', () => {
  test('it is disabled for every moment notesView disables it, not just while its own request is in flight', () => {
    // The manual click handler used to unconditionally set
    // `button.disabled = false` when ITS OWN request finished, which could
    // re-enable the button while an unrelated flow run (started from the
    // label, say) was still busy downloading or installing — a manual
    // check answering "current" then would contradict the label on screen
    // and clear the remembered offer out from under it.
    const notes = read('notes-surface.js');
    expect(notes).toMatch(/button\.disabled\s*=\s*checkDisabled/);
  });
});

describe('"Check for updates" hands an offer or "nothing newer" to the flow, not just to itself', () => {
  test('the manual handler calls flow.checkAnswered for offer and current, and still reports its own errors', () => {
    const notes = read('notes-surface.js');
    const handler = notes.slice(
      notes.indexOf("button.addEventListener('click'"),
      notes.indexOf('// One body and one Install button'),
    );
    expect(handler).toContain('flow.checkAnswered(result)');
    // Errors are still this handler's own: checkAnswered has no branch for
    // them (launchCheck does not report them either — see update-flow.js),
    // and a manual press is exactly where a person asked and is owed one.
    expect(handler).toMatch(/result\.outcome === 'error'.*\{\s*text\(say, result\.message\); return; \}/);
    // Neither offer nor "nothing newer" is handled directly any more: that
    // was the bug (the label and the flow's own `offer` never heard about
    // it, so a click on a withdrawn release's stale Update-to label, or its
    // enabled Install button in the notes, still installed it).
    expect(handler).not.toContain('flow.offerFound(result)');
    expect(handler).not.toMatch(/is the newest there is/);
  });

  test('a "current" answer to the very first check this session still says so', () => {
    // notesView's null-after-offer wording is gated on drewOffer (its own
    // doc comment): a null phase before any offer is the block's own IDLE
    // state, and leaves `say` alone. A manual press that has never drawn an
    // offer would otherwise leave `say` stuck on "Looking…" forever once
    // checkAnswered(result) turns a "current" answer into that same null
    // phase. The press itself is reason enough to speak, so the handler
    // marks drewOffer true before the request even goes out.
    const notes = read('notes-surface.js');
    const handler = notes.slice(
      notes.indexOf("button.addEventListener('click'"),
      notes.indexOf('// One body and one Install button'),
    );
    expect(handler).toContain('drewOffer = true;');
  });
});

describe('a refused file is no longer a dead end', () => {
  // The Swift app could report a bug from the failure screen, and the report
  // carried the refusal's code with it. The Tauri window could not report
  // anything, so a file the engine rejected ended the conversation: the one
  // moment someone most wants to tell you what happened was the moment the
  // app gave them nowhere to say it.
  //
  // Ported from app/Sources/ScreepubKit/Feedback.swift rather than reinvented.
  let feedback: any;
  beforeAll(async () => { feedback = await import(join(UI, 'feedback.js')); });

  const parse = (url: string) => new URL(url);

  test('it opens a new issue on this project, not a generic page', () => {
    const url = parse(feedback.newIssueUrl({ appVersion: '0.6.0', osVersion: 'macOS 15.0' }));
    expect(`${url.origin}${url.pathname}`)
      .toBe('https://github.com/ssandweiss/screepub/issues/new');
  });

  test('the report arrives stamped with both versions', () => {
    // So a report never begins with two rounds of "which version?".
    const body = parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'macOS 15.0',
    })).searchParams.get('body') ?? '';
    expect(body).toContain('0.6.0');
    expect(body).toContain('macOS 15.0');
  });

  test('a refusal seeds the "what happened" block; nothing else does', () => {
    const withContext = parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'x', context: 'password: this PDF is locked',
    })).searchParams.get('body') ?? '';
    expect(withContext).toContain('What happened');
    expect(withContext).toContain('password: this PDF is locked');

    const without = parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'x',
    })).searchParams.get('body') ?? '';
    expect(without).not.toContain('What happened');
  });

  test('a plus in the context survives the round trip', () => {
    // Feedback.swift's hard-won detail, carried over: a query parser reads a
    // literal "+" as a space, so a context containing one arrives corrupted
    // unless it is percent-encoded. URLSearchParams gets this right where
    // hand-built query strings do not, which is why this is built with it.
    const body = parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'x', context: 'C++ crashed on page 3+4',
    })).searchParams.get('body') ?? '';
    expect(body).toContain('C++ crashed on page 3+4');
  });

  test('the report carries no path at all, only its extension, whatever the engine said', () => {
    // The engine's sentence can quote a path, and every part of one can name
    // the person (their home folder) or the script: its file name, and the
    // library folder the book sits in, which is named after the script too.
    // The report goes to a public URL, so each whole path becomes <path>,
    // keeping only the extension, which tells a bug report what kind of file.
    const bodyOf = (context: string, home?: string) => parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'x', context, home,
    })).searchParams.get('body') ?? '';

    const library = bodyOf('could not write /Users/someone/Documents/Screepub/field-station/field-station.epub');
    expect(library).not.toContain('field-station');
    expect(library).not.toContain('someone');
    expect(library).toContain('could not write <path>.epub');

    const windows = bodyOf('cannot read C:\\Users\\someone\\Documents\\Screepub\\field-station\\field-station.epub, so stopped');
    expect(windows).not.toContain('field-station');
    expect(windows).not.toContain('someone');
    expect(windows).toContain('cannot read <path>.epub, so stopped');

    const mac = bodyOf("internal: ENOENT: no such file or directory, open '/Users/jdoe/Scripts/Night Shift v2.pdf'");
    expect(mac).not.toContain('jdoe');
    expect(mac).not.toContain('Night Shift');
    expect(mac).toContain("open '<path>.pdf'");

    const told = bodyOf('could not write /srv/people/jdoe/Library/Screepub/night/Night Shift.epub because the disk is full', '/srv/people/jdoe');
    expect(told).toContain('could not write <path>.epub because the disk is full');

    const tilde = bodyOf('cannot read ~/field-station/notes and gave up');
    expect(tilde).toContain('cannot read <path> and gave up');

    const linux = bodyOf('cannot read /home/jdoe/x.pdf.');
    expect(linux).toContain('cannot read <path>.pdf.');

    // A bare home folder, with nothing after it, is still not the user's name.
    expect(bodyOf('home is /Users/jdoe')).not.toContain('jdoe');

    // Prose with a slash in it is not a path, and a link is not mangled.
    const prose = bodyOf('scanned and/or locked; see https://example.com/help');
    expect(prose).toContain('scanned and/or locked; see https://example.com/help');
  });

  test('a long report is capped at about 2000 characters, footer kept', () => {
    const body = parse(feedback.newIssueUrl({
      appVersion: '0.6.0', osVersion: 'macOS 15.0', context: 'x'.repeat(10000),
    })).searchParams.get('body') ?? '';
    expect(body.length).toBeLessThanOrEqual(2000);
    expect(body).toContain('Screepub 0.6.0');
    expect(body).toContain('…');
  });

  test('the file manager is called what it is called, per platform', async () => {
    // The Swift app said "SHOW IN FINDER" because it only ran on a Mac. This
    // one runs on three, and "Finder" on Windows names a thing that is not
    // there. An unknown platform gets the generic phrasing rather than a
    // guess, on the same rule send-routes.js's platformOf already follows:
    // under-claiming beats naming the wrong system.
    const convert: any = await import(join(UI, 'convert.js'));
    expect(convert.revealLabel('MacIntel')).toBe('Show in Finder');
    expect(convert.revealLabel('Win32')).toBe('Show in File Explorer');
    expect(convert.revealLabel('Linux x86_64')).toBe('Show in folder');
    expect(convert.revealLabel(undefined)).toBe('Show in folder');
  });

});

describe('eighteen settings stop arriving as one wall', () => {
  // All eighteen were open at once, under five headings, which is a long
  // scroll of controls most of which nobody is looking for. Each group folds
  // now, and the first is open so the surface never opens as a list of five
  // shut boxes with nothing to read.
  let tune: any;
  beforeAll(async () => { tune = await import(join(UI, 'tune.js')); });

  test('the first group is open and the rest are folded', () => {
    expect(tune.groupStartsOpen(0)).toBe(true);
    expect(tune.groupStartsOpen(1)).toBe(false);
    expect(tune.groupStartsOpen(4)).toBe(false);
  });

  test('every group is still reachable, none is dropped', () => {
    // Folding is not hiding: all five headings remain, and all eighteen knobs
    // remain under them. A "compact" that quietly retired a setting would
    // leave it applying to every conversion with no way to find it.
    expect(tune.GROUPS.length).toBe(5);
    expect(tune.GROUPS.flatMap((g: any) => g.knobs).length).toBe(18);
  });
});

describe('the settings explain themselves in the reader\'s words', () => {
  // The copy was carried over from the SwiftUI reader rail rather than
  // written fresh, and it explained MECHANISM to a screenwriter in a
  // compressed register, using KFX, tolino, chyrons and ragged-right as
  // though they were common words. Rewritten 2026-09-21 to a single brief:
  // lead with what the reader will see in their book, one plain sentence, no
  // jargon that is not glossed on the spot.
  //
  // Screenplay vocabulary is NOT jargon here and is deliberately not banned.
  // (MORE), (CONT'D), parenthetical and cue are words this audience uses
  // daily; KFX is not.
  const ENGINE_WORDS = ['KFX', 'tolino', 'chyron', 'ragged-right', 'ragged right', 'e-ink ('];

  test('no label or explanation leans on a word only the engine knows', async () => {
    const tune: any = await import(join(UI, 'tune.js'));
    const offenders: string[] = [];
    for (const group of tune.GROUPS) {
      const texts = [group.title, group.note ?? '', ...group.knobs.flatMap(
        (k: any) => [k.label, k.help ?? ''],
      )];
      for (const text of texts) {
        for (const word of ENGINE_WORDS) {
          if (String(text).toLowerCase().includes(word.toLowerCase())) {
            offenders.push(`"${word}" in: ${text}`);
          }
        }
      }
    }
    expect(offenders.join('\n')).toBe('');
  });

  test('the scan can actually see the jargon it bans', async () => {
    // Guards the loop above: a GROUPS shape it could not walk would pass by
    // finding nothing, which is the failure mode of every sweep like this.
    const tune: any = await import(join(UI, 'tune.js'));
    const all = tune.GROUPS.flatMap((g: any) => g.knobs.map((k: any) => k.label));
    expect(all.length).toBe(18);
    expect(ENGINE_WORDS.some((w) => 'Applies on new-format Kindle (KFX)'.includes(w))).toBe(true);
  });
});

describe('"Start from" stops reading as a row of arbitrary buttons', () => {
  // It was three faults wearing one coat, and only the third is invisible:
  //   1. it offers exactly two presets, and one of them, "Kindle e-ink (6\")",
  //      is IDENTICAL to the defaults, so on a fresh script that button
  //      visibly does nothing;
  //   2. the label never said it OVERWRITES every setting below it, which the
  //      SwiftUI version stated out loud;
  //   3. the engine already answers "which preset am I on" in `answer.preset`
  //      and the window never read it, so it could not show you where you
  //      stood — which is exactly what makes a no-op button look arbitrary
  //      rather than reassuring.
  let tune: any;
  beforeAll(async () => { tune = await import(join(UI, 'tune.js')); });

  test('the window can say which preset the script is on', () => {
    expect(tune.currentPreset({ preset: 'phone' })).toBe('phone');
    expect(tune.currentPreset({ preset: 'kindleEink' })).toBe('kindleEink');
  });

  test('a script tuned away from every preset is on none of them', () => {
    // The engine answers null rather than keeping a remembered name, because
    // equality is the only honest answer: a stored name would go on claiming
    // "Kindle e-ink" after the first knob moved.
    expect(tune.currentPreset({ preset: null })).toBe(null);
    expect(tune.currentPreset({})).toBe(null);
    expect(tune.currentPreset(undefined)).toBe(null);
  });

  test('presets carry their id, so the current one can be marked', async () => {
    const { DEFAULT_FORMAT_OPTIONS } = await import('../src/options');
    const presets = tune.presetsFrom({
      presets: [{ id: 'kindleEink', displayName: 'Kindle e-ink (6")', settings: { ...DEFAULT_FORMAT_OPTIONS } }],
    });
    expect(presets[0].id).toBe('kindleEink');
  });

  test('the control warns that it overwrites', () => {
    expect(tune.PRESET_NOTE.toLowerCase()).toContain('overwrite');
  });
});

describe('the scene index', () => {
  // It sat to the RIGHT of the script, then became a drawer over the
  // binding margin (interface-pass design, decision 16). Since the layout
  // pass (2026-09-29) the margin is 100px and the index 218, so it has a
  // slot of its own beside the script instead: see
  // tests/desktop-layout.test.ts for the slot. What these keep is what the
  // index needs in any layout.

  /** The first declaration block for a selector, so a test can read one rule
   *  instead of the whole stylesheet. */
  function ruleBlock(css: string, selector: string): string {
    const at = css.indexOf(`${selector} {`);
    expect(`${selector} found`).toBe(at === -1 ? `${selector} missing` : `${selector} found`);
    return css.slice(at, css.indexOf('}', at) + 1);
  }

  test('the control says what it will do, not what is showing', async () => {
    const reader = (await import(join(UI, 'read.js'))) as {
      indexToggle?: (open: boolean) => { label: string; expanded: string };
    };
    expect(reader.indexToggle?.(true)).toEqual({ label: 'Hide scenes', expanded: 'true' });
    expect(reader.indexToggle?.(false)).toEqual({ label: 'Show scenes', expanded: 'false' });
  });

  test('the panel stays positioned, because read.js measures against it', () => {
    // A regression guard on an existing coupling rather than new behaviour:
    // read.js keeps the marked scene in view by its button's offsetTop, and
    // an unpositioned rail would hand it an offset measured from the page
    // instead. The panel has been `relative` since the layout pass gave it
    // its own slot: this is here so the NEXT change cannot quietly make it
    // static.
    const block = ruleBlock(read('surfaces.css'), '.scene-rail');
    expect(block).toMatch(/position:\s*(relative|absolute|fixed|sticky)/);
  });

  test('the list stands clear of the panel’s top and bottom edges', () => {
    // QA, 0.7.3: the count ("93 SCENES") sat right against the panel's top
    // edge, even scrolled to the top. The top gets the same room the bottom
    // already had, in the window's own spacing, so the list reads as set
    // inside the panel at both ends rather than cut off at one.
    const block = ruleBlock(read('surfaces.css'), '.scene-rail');
    const [top, , bottom] = block.match(/\bpadding:\s*([^;]+);/)?.[1]?.trim().split(/\s+/) ?? [];
    expect(top).toBe('var(--space-3)');
    expect(bottom).toBe('var(--space-3)');
  });

  test('hiding it does not rely on moving it', () => {
    // The panel keeps its column whether it is open or shut: opacity and
    // visibility are what actually hide it, and the small translate is only
    // the motion, not what clears it from view.
    const block = ruleBlock(read('surfaces.css'), '.scene-rail');
    expect(block).toMatch(/opacity:\s*0\b/);
    expect(block).toMatch(/visibility:\s*hidden/);
  });
});

describe('the frame, mounted', () => {
  // frame.js's mountFrame on the fake page: the bar, the foot and the drag
  // regions, driven by key, click and the calls main.js makes.
  let page: FakePage | null = null;
  afterEach(async () => { await page?.close(); page = null; });

  async function mountBar() {
    page = fakePage();
    const { mountFrame, SURFACES } = await import(join(UI, 'frame.js'));
    const root = page.doc.createElement('div');
    page.doc.body.append(root);
    const frame = mountFrame(root);
    const tab = (id: string) => page!.doc.getElementById(`tab-${id}`)!;
    const tablist = page.doc.querySelector('[role="tablist"]')!;
    return { frame, root, tab, tablist, SURFACES, doc: page.doc };
  }

  test('the switcher is a real tablist: arrows, Home and End move the selection and the focus, past a surface that is not there', async () => {
    const { frame, tab, tablist, SURFACES } = await mountBar();
    for (const { id } of SURFACES) {
      expect(tab(id).localName).toBe('button');
      expect(tab(id).getAttribute('role')).toBe('tab');
      expect(tab(id).getAttribute('aria-controls')).toBe(`surface-${id}`);
    }
    frame.enable('read', true);
    frame.enable('tune', false);
    frame.setSurface('convert');
    const selected = () => SURFACES.map((s: { id: string }) => s.id)
      .filter((id: string) => tab(id).getAttribute('aria-selected') === 'true');
    const press = (key: string) => {
      const event = makeEvent('keydown', { key });
      tablist.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(press('ArrowRight')).toBe(true);
    expect(selected()).toEqual(['read']);
    expect(page!.doc.activeElement).toBe(tab('read'));
    // Settings is off the bar, so it is not a stop: past it, round to Convert.
    press('ArrowRight');
    expect(selected()).toEqual(['convert']);
    press('ArrowLeft');
    expect(selected()).toEqual(['read']);
    press('Home');
    expect(selected()).toEqual(['convert']);
    press('End');
    expect(page!.doc.activeElement).toBe(tab('read'));
    // One Tab stop on the bar: the selected tab. An absent one is no stop.
    expect(SURFACES.map((s: { id: string }) => [s.id, tab(s.id).tabIndex, tab(s.id).hidden])).toEqual([
      ['convert', -1, false], ['read', 0, false], ['tune', -1, true],
    ]);
    // Any other key is the page's.
    expect(press('a')).toBe(false);
  });

  test('switching a surface off while it shows goes back to Convert', async () => {
    const { frame, tab } = await mountBar();
    const heard: string[] = [];
    frame.onSurface((id: string) => heard.push(id));
    frame.enable('read', true);
    frame.setSurface('read');
    frame.enable('read', false);
    expect(heard).toEqual(['read', 'convert']);
    expect(tab('read').hidden).toBe(true);
  });

  test('the window moves by its top strip and the gaps in the bar, never by a tab', async () => {
    const { doc } = await mountBar();
    const dragging = doc.querySelectorAll('[data-tauri-drag-region]');
    expect(dragging.map((n) => n.className)).toEqual(['tabs', 'drag-strip']);
    expect(doc.querySelectorAll('button').filter((b) => b.hasAttribute('data-tauri-drag-region'))).toEqual([]);
  });

  test('the update label sits beside the stamp: hidden until there are words, clickable only when told so', async () => {
    const { frame, doc } = await mountBar();
    const { RELEASE } = await import(join(UI, 'notes.js'));
    const foot = doc.querySelector('.rev-foot')!;
    const [label, stamp] = foot.children;
    // A stable handle: main.js checks the focused element against it.
    expect(label.id).toBe('rev-update');
    expect(stamp.textContent).toBe(`rev ${RELEASE.version}`);
    expect(label.hidden).toBe(true);
    const clicks: string[] = [];
    frame.onUpdateClick(() => clicks.push('update'));
    frame.onRev(() => clicks.push('notes'));

    frame.setUpdateLabel('Update to 0.8.0', { actionable: true });
    expect([label.hidden, label.textContent, label.getAttribute('aria-disabled')]).toEqual([false, 'Update to 0.8.0', null]);
    expect(label.classList.contains('rev-update-inert')).toBe(false);
    label.click();
    // "Downloading" is under way: it stops looking like a button.
    frame.setUpdateLabel('Downloading 0.8.0… 40%', { actionable: false });
    expect(label.getAttribute('aria-disabled')).toBe('true');
    expect(label.classList.contains('rev-update-inert')).toBe(true);
    frame.setUpdateLabel(null);
    expect(label.hidden).toBe(true);
    stamp.click();
    expect(clicks).toEqual(['update', 'notes']);
  });

  test('a dead engine gets its own line, and the stamp keeps naming the build', async () => {
    const { frame, doc } = await mountBar();
    const fault = doc.querySelector('.engine-fault')!;
    expect(fault.hidden).toBe(true);
    frame.engineFailed('the engine could not be started');
    expect([fault.hidden, fault.textContent]).toEqual([false, 'the engine could not be started']);
    expect(doc.querySelector('.rev-stamp')!.textContent).toMatch(/^rev \d/);
  });
});

describe('the foot of the page names the release', () => {
  // It used to print the ENGINE's self-reported version, labelled "engine".
  // Two things were wrong with that. The number was the engine's while the
  // app's own version is different, so it misnamed the build in the one place
  // people paste into bug reports. And on a failed engine the error REPLACED
  // it, so the app could be running with no version on screen at all.
  const frameMod = async () =>
    (await import(join(UI, 'frame.js'))) as {
      revLabel?: (version: string) => string;
      SURFACES?: Array<{ id: string; label: string }>;
    };

  test('it reads "rev" and the version it was built from', async () => {
    const { revLabel } = await frameMod();
    expect(revLabel?.('0.6.0')).toBe('rev 0.6.0');
  });

  test('the notes sheet is not on the page while it is shut', () => {
    // A closed <dialog> is hidden by the browser's own
    // `dialog:not([open]) { display: none }`, and ANY class selector outranks
    // that. Giving .sheet-over a display unqualified therefore does not style
    // the sheet, it un-hides it: the release notes sat permanently at the
    // foot of every surface, invisible in a short window and obvious the
    // moment anyone scrolled. Shipped and unnoticed for six commits.
    const css = read('style.css');
    const unqualified = css.match(/\.sheet-over\s*\{[^}]*\}/g) ?? [];
    for (const rule of unqualified) {
      expect(`unqualified .sheet-over: ${rule}`).not.toContain('display:');
    }
  });

  test('the bar holds four surfaces, and Notes is not one of them', async () => {
    // Notes is the release notes for the version you are running. It is a
    // thing you glance at, not a place you go, and it is reached from the
    // version it describes.
    const { SURFACES } = await frameMod();
    expect(SURFACES?.map((s) => s.id)).toEqual(['convert', 'read', 'tune']);
  });
});

describe('the window can be moved by its top, like any other window', () => {
  // titleBarStyle "Overlay" puts the page under the title bar, and until
  // 2026-09-23 the page marked nothing as a drag region, so the window could
  // not be moved at all. Tauri's drag script (2.11.5, drag.js): a bare
  // attribute drags only when the click lands on THAT element; anything
  // clickable without the attribute stays clickable. Which elements carry
  // the attribute is driven in "the window moves by its top strip and the
  // gaps in the bar, never by a tab"; where the strip sits is CSS.
  const css = read('style.css');

  test('the drag strip scrolls away with the page and covers no control at rest', () => {
    // A CSS pin: position needs a browser to observe.
    const rule = css.match(/\.drag-strip\s*\{[^}]*\}/)?.[0] ?? '';
    // NOT fixed: a strip pinned to the viewport stayed over content that
    // scrolled underneath it — confirmed live, a click on a tab started a
    // window drag once Read had scrolled 21.5px, and the strip sat over a
    // slider on Settings (1495px tall) once it had scrolled to 400px.
    // Absolute, so it is part of the page and scrolls away with it.
    expect(rule).toContain('position: absolute');
    expect(rule).not.toContain('position: fixed');
    expect(rule).toContain('top: 0');
    expect(rule).toContain('height: var(--space-7)');
  });
});

describe('the dead-engine line does not crowd the update label', () => {
  test('it sits a full row below the foot, not 1.4px away', () => {
    const rule = read('style.css').match(/\.engine-fault\s*\{[^}]*\}/)?.[0] ?? '';
    expect(rule).toContain('bottom: var(--space-9)');
  });

});

describe('a newer version is a label you can click, not a dot you can miss', () => {
  const frame = read('frame.js');
  const css = read('style.css');

  test('the label is ink with a brass rule, not brass text', () => {
    // Brass on the paper does not reach a readable contrast; the brass is the
    // rule under it, the same mark the open tab carries.
    const rule = css.match(/\.rev-update\s*\{[^}]*\}/)?.[0] ?? '';
    expect(rule).toContain('color: var(--ink)');
    expect(rule).toMatch(/border-bottom:\s*1\.5px solid var\(--brass\)/);
    expect(css).toMatch(/\.rev-update:focus-visible/);
  });

  test('the label stops looking clickable once a click would do nothing', () => {
    // "Update to 0.8.0" is a real button; "Downloading 0.8.0… 40%" only
    // looks like one unless the frame is told otherwise. The frame's half
    // (aria-disabled and the inert class) is driven in "the update label
    // sits beside the stamp"; the pointer is CSS, and main.js's use of
    // update.js's rule is left to the update work.
    const rule = css.match(/\.rev-update-inert\s*\{[^}]*\}/)?.[0] ?? '';
    expect(rule).toContain('cursor: default');
    const main = read('main.js');
    expect(main).toContain('labelActionable(phase)');
  });

  test('the keyboard does not fall off the edge when the label hides under it', () => {
    // A fresh check that says "nothing newer" hides the label (words go
    // null). If the reader had just Tabbed to it, or clicked it and it
    // resolved before they moved on, the keyboard was standing on an
    // element that is now `hidden`, which drops focus to the body with no
    // way back in one Tab press. The label needs a stable handle so
    // main.js can tell whether it was the one holding focus.
    expect(frame).toMatch(/class:\s*'rev-update',[\s\S]*?id:\s*'rev-update'/);
    const main = read('main.js');
    const subscriber = main.slice(main.indexOf('flow.subscribe'), main.indexOf('frame.onUpdateClick'));
    expect(subscriber).toMatch(/document\.activeElement\?\.id === 'rev-update'/);
    expect(subscriber).toMatch(/document\.activeElement\?\.id === 'rev-update'\)\s*restoreFocus\(\)/);
  });
});

describe('the Convert page asks once whether to look for new versions', () => {
  test('the question is short, plain and has no em dash', async () => {
    const { ASK } = await import(join(UI, 'convert.js'));
    expect(ASK).toEqual({
      question: 'Check for new versions once a day?',
      yes: 'Turn on',
      no: 'No thanks',
    });
    for (const words of Object.values(ASK)) expect(String(words)).not.toContain('—');
  });

  test('it is styled quietly, in the window\'s own tokens', () => {
    const rule = read('surfaces.css').match(/\.well-ask\s*\{[^}]*\}/)?.[0] ?? '';
    expect(rule).toContain('color: var(--ink-muted)');
  });
});

describe('the release notes redraw one Install button and one body, not a new one per offer', () => {
  // The plan's first draft of this block built a fresh <p> and a fresh
  // button on every offer() call, so a version that repeats (the remembered
  // stub, then the live check confirming it) appended a SECOND body and a
  // second Install button rather than redrawing the first. Both are built
  // once, up front, and hidden until there is something to say.
  test('exactly one Install button, and the body is redrawn with text()', () => {
    const notes = read('notes-surface.js');
    expect(notes.match(/btn-brad/g)?.length).toBe(1);
    expect(notes).toContain("el('button', { type: 'button', class: 'btn btn-brad btn-small', hidden: true }");
    expect(notes).toContain("el('p', { class: 'prose update-body', hidden: true }");
    // Redrawn from notesView's own return value (view.body.text), not
    // appended: the pure function decides the words, the subscriber only
    // paints them.
    expect(notes).toMatch(/text\(body,\s*view\.body\.text\)/);
    // Not appended per offer: say.after(...) built a new paragraph each time.
    expect(notes).not.toContain('say.after(');
  });
});

describe('a surface with nothing behind it is absent, not dimmed', () => {
  // Read, Tune and Send were DISABLED before a conversion, which drew three
  // greyed words in the bar advertising doors that do not open. Worse for a
  // screen reader, which reads a disabled control out and then refuses it.
  // The rule lives in a pure function so it can be tested without a DOM, the
  // same split convert.js, read.js and tune.js already use.
  const presence = async () =>
    (await import(join(UI, 'frame.js'))) as {
      tabPresence?: (available: boolean, selected: boolean) => { hidden: boolean; tabIndex: number };
    };

  test('an unreachable surface is off the bar and off the keyboard', async () => {
    const { tabPresence } = await presence();
    expect(tabPresence?.(false, false)).toEqual({ hidden: true, tabIndex: -1 });
  });

  test('an unreachable surface stays off the keyboard even if it was the one showing', async () => {
    // enable() moves away from a surface it is switching off, but the two
    // facts arrive separately, and a tab that kept tabIndex 0 while hidden is
    // a focus stop pointing at nothing.
    const { tabPresence } = await presence();
    expect(tabPresence?.(false, true)).toEqual({ hidden: true, tabIndex: -1 });
  });

  test('the open surface is the bar\'s single tab stop', async () => {
    const { tabPresence } = await presence();
    expect(tabPresence?.(true, true)).toEqual({ hidden: false, tabIndex: 0 });
  });

  test('a reachable surface that is not open is visible but not a tab stop', async () => {
    // Roving tabindex: one stop for Tab, then the arrows move inside. Five
    // stops would put every surface five presses further away than the last.
    const { tabPresence } = await presence();
    expect(tabPresence?.(true, false)).toEqual({ hidden: false, tabIndex: -1 });
  });
});

describe('the drop well states one guard, not two', () => {
  // The engine guards four failures and two were checkable at a glance, so
  // the well named both: a scan, and a password-locked file. The maintainer
  // cut the password half on 2026-09-20 — see the interface-pass design,
  // decision 10. That is a real trade and it is recorded there: a locked PDF
  // is now discovered AFTER the wait instead of before it. It is survivable
  // only because the refusal still names the cause, which the HEADINGS table
  // and its own tests already guarantee.
  const ROOT = new URL('..', import.meta.url).pathname;
  const wellCopy = async () =>
    (await import(join(UI, 'convert.js'))) as { WELL?: { limits?: string }; WORDMARK?: string };

  test('it still warns about a scan before the drop', async () => {
    const { WELL } = await wellCopy();
    expect(WELL?.limits ?? '').toContain('not a scan');
  });

  test('it no longer warns about a password', async () => {
    const { WELL } = await wellCopy();
    expect((WELL?.limits ?? '').toLowerCase()).not.toContain('password');
  });

  test('the brand component says the same thing the window says', async () => {
    // These two carried identical copy and NOTHING tied them together, so the
    // window could be changed and the design system left saying the old line.
    // The component is what the next surface gets built from, so that drift
    // does not stay theoretical: it re-adds the sentence by hand later.
    //
    // Compared as the rendered COPY rather than by scanning the file, because
    // the component's caption is documentation and has to stay free to
    // explain what the line used to say and why that half went. A file-wide
    // ban on the word forbids the component from recording its own history.
    const { WELL } = await wellCopy();
    const brand = readFileSync(join(ROOT, 'brand', 'components', 'drop-well.html'), 'utf8');
    const limits = [...brand.matchAll(/<span class="well-limits">([^<]*)<\/span>/g)]
      .map((m) => m[1] ?? '');
    const expected = WELL?.limits ?? '';
    expect(expected).not.toBe(''); // the window must actually have copy to compare
    expect(limits.length).toBeGreaterThan(0); // the scan must actually find them
    for (const line of limits) expect(line).toBe(expected);
  });

  test('the window names itself where the paragraph used to be', async () => {
    // With the title bar gone and the prose cut, this is the only place the
    // app says what it is.
    const { WORDMARK } = await wellCopy();
    expect(WORDMARK).toBe('Screepub');
  });
});

describe('the KFX checklist: decisions', () => {
  type Setup = {
    ready: boolean; possible: boolean; summary: string;
    steps: { id: string; name: string; installed: boolean; fix: unknown }[];
  };
  type KfxModule = {
    HEADING: string; INSTALLED: string; INSTALLING: string; NO_REASON: string;
    checklistFrom: (value: unknown) => Setup | null;
    setupFrom: (answer: unknown) => Setup | null;
    kindleRelevant: (devices: unknown) => boolean;
    showSetup: (setup: Setup | null, devices: unknown, justInstalled: boolean, installing?: boolean) => boolean;
    controlFor: (step: Setup['steps'][number], busy: boolean) =>
      | { type: 'status'; text: string }
      | { type: 'link'; label: string; url: string }
      | { type: 'install'; label: string; disabled: boolean };
    installedLine: (version: unknown, removed: unknown, ready: unknown) => string;
    failedLine: (answer: unknown) => string;
    linkFailedLine: (url: string) => string;
    afterInstall: (answer: unknown, previous: Setup | null) =>
      { setup: Setup | null; line: string; bad: boolean; justInstalled: boolean };
  };
  let kfx: KfxModule;
  beforeAll(async () => { kfx = (await import(join(UI, 'kfx.js'))) as KfxModule; });

  const notReady: Setup = {
    ready: false,
    possible: true,
    summary: 'Kindles get AZW3 for now. KFX looks better, and needs the three free tools below.',
    steps: [
      { id: 'calibre', name: 'Calibre', installed: true, fix: null },
      {
        id: 'previewer', name: 'Kindle Previewer', installed: false,
        fix: { kind: 'link', label: 'Get Kindle Previewer', url: 'https://kdp.amazon.com/en_US/help/topic/G202131170' },
      },
      { id: 'plugin', name: 'KFX plugin', installed: false, fix: { kind: 'install', label: 'Install' } },
    ],
  };
  const ready: Setup = {
    ready: true, possible: true, summary: 'Kindles get KFX, the best quality Screepub can make.',
    steps: notReady.steps.map((s) => ({ ...s, installed: true, fix: null })),
  };
  const kindle = { id: '/m/Kindle', kind: 'kindle', name: 'Kindle', volume: '/m/Kindle' };
  const kobo = { id: '/m/KOBOe', kind: 'kobo', name: 'Kobo', volume: '/m/KOBOe' };

  test('the validator accepts every checklist a real engine can build, for every platform', () => {
    // The worst possible gap in the validator below: reject exactly the
    // calibre-missing branch, and the suite stays green because none of the
    // hand-written fixtures above happen to cover it. This walks every
    // status the probe can report, on every platform kfx-setup.ts branches
    // on (8 combinations x 3 platforms = 24, 16 of them on a platform where
    // KFX is possible), and proves checklistFrom() and setupFrom() round-trip
    // every one of the engine's own real answers rather than just the
    // fixtures this file typed out by hand.
    for (const platform of ['darwin', 'win32', 'linux']) {
      for (const calibre of [true, false]) {
        for (const previewer of [true, false]) {
          for (const pluginInstalled of [true, false]) {
            const status = {
              calibre, previewer, pluginInstalled,
              ready: calibre && previewer && pluginInstalled,
            };
            const setup = kfxSetup(status, platform);
            expect(kfx.checklistFrom(setup)).toEqual(setup);
            expect(kfx.setupFrom({ ok: true, ...setup })).toEqual(setup);
          }
        }
      }
    }
  });

  test('the engine’s answer is taken whole, or not at all', () => {
    expect(kfx.setupFrom({ ok: true, ...notReady })).toEqual(notReady);
    expect(kfx.setupFrom({ ok: false, error: { code: 'x', message: 'y' } })).toBe(null);
    expect(kfx.setupFrom(null)).toBe(null);
    // A broken contract draws nothing: this block is advice, and a reader
    // cannot act on "the probe answered strangely".
    expect(kfx.checklistFrom({ ...notReady, steps: notReady.steps.slice(0, 2) })).toBe(null);
    expect(kfx.checklistFrom({ ...notReady, summary: 7 })).toBe(null);
    expect(kfx.checklistFrom({ ...notReady, possible: 'yes' })).toBe(null);
    const badLink = structuredClone(notReady);
    (badLink.steps[1] as { fix: unknown }).fix = { kind: 'link', label: 'Get it' };
    expect(kfx.checklistFrom(badLink)).toBe(null);
    const unknownKind = structuredClone(notReady);
    (unknownKind.steps[2] as { fix: unknown }).fix = { kind: 'teleport', label: 'Go' };
    expect(kfx.checklistFrom(unknownKind)).toBe(null);
    const fixWhileInstalled = structuredClone(notReady);
    (fixWhileInstalled.steps[0] as { fix: unknown }).fix = { kind: 'install', label: 'Install' };
    expect(kfx.checklistFrom(fixWhileInstalled)).toBe(null);
    // Steps in the wrong order: the contract is calibre, then previewer,
    // then plugin, in that sequence, not merely those three ids as a set.
    const swapped = structuredClone(notReady);
    swapped.steps = [swapped.steps[1], swapped.steps[0], swapped.steps[2]];
    expect(kfx.checklistFrom(swapped)).toBe(null);
    // Four steps: one too many.
    const extraStep = structuredClone(notReady);
    extraStep.steps.push({ id: 'plugin', name: 'KFX plugin', installed: true, fix: null });
    expect(kfx.checklistFrom(extraStep)).toBe(null);
    // ok: false, or no ok at all: setupFrom refuses before it ever looks at
    // the shape, however well-formed the rest of the object is.
    expect(kfx.setupFrom({ ok: false, ...notReady })).toBe(null);
    expect(kfx.setupFrom(notReady)).toBe(null);
    expect(kfx.checklistFrom({ ...notReady, ready: 'no' })).toBe(null);
    const blankName = structuredClone(notReady);
    (blankName.steps[0] as { name: string }).name = '   ';
    expect(kfx.checklistFrom(blankName)).toBe(null);
    const blankInstallLabel = structuredClone(notReady);
    (blankInstallLabel.steps[2] as { fix: unknown }).fix = { kind: 'install', label: '  ' };
    expect(kfx.checklistFrom(blankInstallLabel)).toBe(null);
    const blankAfterWhy = structuredClone(notReady);
    (blankAfterWhy.steps[2] as { fix: unknown }).fix = { kind: 'after', why: '' };
    expect(kfx.checklistFrom(blankAfterWhy)).toBe(null);
  });

  test('a fix is rebuilt clean: no extra key the engine happened to attach survives', () => {
    const withExtraLink = structuredClone(notReady);
    (withExtraLink.steps[1] as { fix: Record<string, unknown> }).fix = {
      kind: 'link', label: 'Get Kindle Previewer',
      url: 'https://kdp.amazon.com/en_US/help/topic/G202131170', extra: 'x',
    };
    expect(kfx.checklistFrom(withExtraLink)?.steps[1].fix).toEqual({
      kind: 'link', label: 'Get Kindle Previewer',
      url: 'https://kdp.amazon.com/en_US/help/topic/G202131170',
    });
    const withExtraInstall = structuredClone(notReady);
    (withExtraInstall.steps[2] as { fix: Record<string, unknown> }).fix = {
      kind: 'install', label: 'Install', extra: 'x',
    };
    expect(kfx.checklistFrom(withExtraInstall)?.steps[2].fix).toEqual({ kind: 'install', label: 'Install' });
    const withExtraAfter = structuredClone(notReady);
    (withExtraAfter.steps[2] as { fix: Record<string, unknown> }).fix = {
      kind: 'after', why: 'Install Calibre first', extra: 'x',
    };
    expect(kfx.checklistFrom(withExtraAfter)?.steps[2].fix).toEqual({ kind: 'after', why: 'Install Calibre first' });
  });

  test('Kindle advice is for Kindles: shown with a Kindle, or nothing, connected', () => {
    expect(kfx.kindleRelevant([])).toBe(true);
    expect(kfx.kindleRelevant([kindle])).toBe(true);
    expect(kfx.kindleRelevant([kobo, kindle])).toBe(true);
    expect(kfx.kindleRelevant([kobo])).toBe(false);
    // Before the first device poll answers, the list is unknown, and the
    // block waits rather than flashing up and vanishing a moment later.
    expect(kfx.kindleRelevant(null)).toBe(false);
  });

  test('shown only when it helps', () => {
    expect(kfx.showSetup(notReady, [], false)).toBe(true);
    expect(kfx.showSetup(ready, [], false)).toBe(false);
    // Right after an install the success line needs somewhere to stand.
    expect(kfx.showSetup(ready, [], true)).toBe(true);
    expect(kfx.showSetup(null, [], true)).toBe(false);
    expect(kfx.showSetup({ ...notReady, possible: false }, [], false)).toBe(false);
    expect(kfx.showSetup(notReady, [kobo], false)).toBe(false);
    expect(kfx.showSetup(notReady, [kobo], false, false)).toBe(false);
    expect(kfx.showSetup(notReady, null, false)).toBe(false);
  });

  test('an install running or just finished keeps the block up, whatever is connected', () => {
    // The reader pressed Install, then plugged in a Kobo (or unplugged the
    // Kindle). The Installing line and then the result are the answer to
    // what they pressed, so they stay where they were said.
    expect(kfx.showSetup(notReady, [kobo], false, true)).toBe(true);
    expect(kfx.showSetup(ready, [kobo], true)).toBe(true);
    expect(kfx.showSetup(ready, [kobo], true, false)).toBe(true);
    expect(kfx.showSetup(notReady, null, false, true)).toBe(true);
    // Still nothing to stand on without a checklist, or where KFX is not
    // possible at all.
    expect(kfx.showSetup(null, [kobo], true, true)).toBe(false);
    expect(kfx.showSetup({ ...notReady, possible: false }, [kobo], true, true)).toBe(false);
  });

  test('each step’s control says what to do, and install waits for a send', () => {
    expect(kfx.controlFor(notReady.steps[0], false)).toEqual({ type: 'status', text: kfx.INSTALLED });
    expect(kfx.controlFor(notReady.steps[1], true)).toEqual({
      type: 'link', label: 'Get Kindle Previewer', url: 'https://kdp.amazon.com/en_US/help/topic/G202131170',
    });
    expect(kfx.controlFor(notReady.steps[2], false)).toEqual({ type: 'install', label: 'Install', disabled: false });
    expect(kfx.controlFor(notReady.steps[2], true)).toEqual({ type: 'install', label: 'Install', disabled: true });
    expect(kfx.controlFor(
      { id: 'plugin', name: 'KFX plugin', installed: false, fix: { kind: 'after', why: 'Install Calibre first' } },
      false,
    )).toEqual({ type: 'status', text: 'Install Calibre first' });
  });

  test('success names the version, whether Kindles now get KFX, and any forks removed', () => {
    expect(kfx.installedLine('2.20.1', [], true)).toBe('Installed the KFX plugin 2.20.1. Kindles now get KFX.');
    expect(kfx.installedLine('2.20.1', [], false)).toBe('Installed the KFX plugin 2.20.1.');
    expect(kfx.installedLine('2.20.1', ['KFX Output (fork)'], true))
      .toBe('Installed the KFX plugin 2.20.1. Kindles now get KFX. Removed an older copy: KFX Output (fork).');
    expect(kfx.installedLine('2.20.1', ['KFX Output (fork)', 'Old KFX'], true))
      .toBe('Installed the KFX plugin 2.20.1. Kindles now get KFX. Removed older copies: KFX Output (fork), Old KFX.');
  });

  test('a failure shows the engine’s own sentence, or a stand-in', () => {
    expect(kfx.failedLine({ ok: false, error: { code: 'kfx-install-failed', message: 'could not install the KFX plugin: offline' } }))
      .toBe('could not install the KFX plugin: offline');
    expect(kfx.failedLine({ ok: false })).toBe(kfx.NO_REASON);
    expect(kfx.failedLine(null)).toBe(kfx.NO_REASON);
  });

  test('a link that will not open still tells the reader where it goes', () => {
    expect(kfx.linkFailedLine('https://calibre-ebook.com/download_osx'))
      .toBe('Could not open the page. It is at https://calibre-ebook.com/download_osx');
  });

  test('afterInstall: success redraws from the fresh checklist and keeps the block up', () => {
    const out = kfx.afterInstall({ ok: true, version: '2.20.1', removed: [], setup: ready }, notReady);
    expect(out).toEqual({
      setup: ready, line: 'Installed the KFX plugin 2.20.1. Kindles now get KFX.', bad: false, justInstalled: true,
    });
  });

  test('afterInstall: a success with a broken checklist keeps the old one rather than blanking', () => {
    const out = kfx.afterInstall({ ok: true, version: '2.20.1', removed: [], setup: { nonsense: true } }, notReady);
    expect(out.setup).toEqual(notReady);
    expect(out.bad).toBe(false);
  });

  test('afterInstall: a malformed checklist is never credited with readiness the validator refused', () => {
    // The bug this guards: reading `answer.setup.ready` straight off the
    // engine's raw object would still say "Kindles now get KFX." even
    // though checklistFrom() just rejected that same object and the block
    // fell back to a previous checklist that is NOT ready.
    const out = kfx.afterInstall(
      { ok: true, version: '2.20.1', removed: [], setup: { ready: true, nonsense: true } },
      notReady,
    );
    expect(out.setup).toEqual(notReady);
    expect(out.line).toBe('Installed the KFX plugin 2.20.1.');
  });

  test('afterInstall: success needs a version of its own, or it counts as a failure', () => {
    const out = kfx.afterInstall({ ok: true, removed: [], setup: ready }, notReady);
    expect(out.bad).toBe(true);
  });

  test('afterInstall: a refusal keeps the checklist and shows the reason in alarm', () => {
    const out = kfx.afterInstall({ ok: false, error: { code: 'kfx-install-failed', message: 'nope' } }, notReady);
    expect(out).toEqual({ setup: notReady, line: 'nope', bad: true, justInstalled: false });
  });
});

describe('the KFX checklist: wiring', () => {
  // The rest of the checklist's wiring (the installer only from its button,
  // one probe at a time, the focus listener going with the panel, a send and
  // an install never overlapping) is driven in send-panel-ui.test.ts's Other
  // ways; the races and the keyboard in the describe below.
  test('a redraw that throws cannot leave a send stuck on', () => {
    // Kept as a source pin until there is a seam: making kfxRedraw() throw
    // from outside needs a hook into kfx.js's draw that the panel does not
    // offer, and a throw staged through the document would throw again from
    // the finally that is under test. Inside the try, so that finally, which
    // clears `sending`, covers it.
    const send = read('send-panel.js');
    const from = send.indexOf('async function sendTo(');
    const sendTo = send.slice(from, send.indexOf('\n}', from));
    const redraw = sendTo.indexOf('kfxRedraw()');
    expect(redraw).toBeGreaterThan(sendTo.indexOf('try {'));
    expect(redraw).toBeLessThan(sendTo.indexOf('await '));
  });
});

describe('the KFX checklist: what a reader sees across redraws', () => {
  // A stub document just big enough for dom.js and for what kfx.js asks of
  // a node. Its one piece of real behaviour is the one these tests are
  // about: activeElement falls back to the body when the focused node is
  // detached, disabled or inside something hidden, as a browser's focus
  // fixup does. The engine is a queue of unanswered calls the test answers
  // in whatever order it wants, which is how the races are staged.
  class StubNode {
    childNodes: StubNode[] = [];
    parentNode: StubNode | null = null;
    attrs = new Map<string, string>();
    listeners = new Map<string, ((event: unknown) => void)[]>();
    hidden = false;
    disabled = false;
    className = '';
    data = '';
    constructor(readonly tagName: string, readonly doc: StubDocument) {}
    get firstChild() { return this.childNodes[0] ?? null; }
    append(...nodes: (StubNode | string)[]) {
      for (const each of nodes) {
        const node = typeof each === 'string' ? this.doc.createTextNode(each) : each;
        node.parentNode?.removeChild(node);
        node.parentNode = this;
        this.childNodes.push(node);
      }
    }
    removeChild(node: StubNode) {
      this.childNodes.splice(this.childNodes.indexOf(node), 1);
      node.parentNode = null;
      return node;
    }
    get textContent(): string {
      return this.tagName === '#text' ? this.data : this.childNodes.map((c) => c.textContent).join('');
    }
    set textContent(value: string) {
      if (this.tagName === '#text') { this.data = value; return; }
      for (const child of this.childNodes) child.parentNode = null;
      this.childNodes = [];
      if (value !== '') this.append(value);
    }
    setAttribute(name: string, value: string) { this.attrs.set(name, value); }
    getAttribute(name: string) { return this.attrs.get(name) ?? null; }
    get dataset(): Record<string, string> {
      return Object.fromEntries([...this.attrs].filter(([k]) => k.startsWith('data-'))
        .map(([k, v]) => [k.slice(5).replace(/-(\w)/g, (_, c: string) => c.toUpperCase()), v]));
    }
    get classList() {
      const names = () => this.className.split(/\s+/).filter(Boolean);
      return {
        contains: (name: string) => names().includes(name),
        toggle: (name: string, on: boolean) => {
          const rest = names().filter((n) => n !== name);
          this.className = (on ? [...rest, name] : rest).join(' ');
          return on;
        },
      };
    }
    addEventListener(type: string, fn: (event: unknown) => void) {
      this.listeners.set(type, [...(this.listeners.get(type) ?? []), fn]);
    }
    click() {
      if (this.disabled) return;
      for (const fn of this.listeners.get('click') ?? []) fn({ type: 'click' });
    }
    get isConnected(): boolean {
      let node: StubNode = this;
      while (node.parentNode !== null) node = node.parentNode;
      return node === this.doc.body;
    }
    contains(other: StubNode | null): boolean {
      for (let node = other; node !== null; node = node.parentNode) if (node === this) return true;
      return false;
    }
    closest(selector: string): StubNode | null {
      if (selector !== '[hidden]') throw new Error(`the stub has no closest(${selector})`);
      for (let node: StubNode | null = this; node !== null; node = node.parentNode) {
        if (node.hidden) return node;
      }
      return null;
    }
    /** A bare tag name only, which is all anything here asks for. */
    querySelectorAll(tag: string): StubNode[] {
      const found: StubNode[] = [];
      const walk = (node: StubNode) => {
        for (const child of node.childNodes) {
          if (child.tagName === tag.toUpperCase()) found.push(child);
          walk(child);
        }
      };
      walk(this);
      return found;
    }
    focus() {
      if (this.isConnected && !this.disabled && this.closest('[hidden]') === null) this.doc.focused = this;
    }
  }
  class StubDocument {
    body: StubNode;
    focused: StubNode | null = null;
    constructor() { this.body = new StubNode('BODY', this); }
    createElement(tag: string) { return new StubNode(tag.toUpperCase(), this); }
    createTextNode(value: string) {
      const node = new StubNode('#text', this);
      node.data = value;
      return node;
    }
    get activeElement() {
      const node = this.focused;
      if (node === null || !node.isConnected || node.disabled || node.closest('[hidden]') !== null) {
        return this.body;
      }
      return node;
    }
  }
  type KfxDrawing = {
    INSTALLING: string;
    mountKfx: (node: StubNode, hooks: unknown) => void;
    kfxShown: () => void;
    kfxHidden: () => void;
    kfxRedraw: () => void;
    kfxInstalling: () => boolean;
  };
  const g = globalThis as unknown as { window?: unknown; document?: unknown };
  afterEach(() => {
    delete g.window;
    delete g.document;
  });

  const machine = (calibre: boolean, previewer: boolean, plugin: boolean) => ({
    calibre, previewer, pluginInstalled: plugin, ready: calibre && previewer && plugin,
  });
  const status = (calibre: boolean, previewer: boolean, plugin: boolean) =>
    ({ ok: true, ...kfxSetup(machine(calibre, previewer, plugin), 'darwin') });
  const installed = (calibre: boolean, previewer: boolean) =>
    ({ ok: true, version: '2.20.1', removed: [], setup: kfxSetup(machine(calibre, previewer, true), 'darwin') });
  const INSTALLED_READY = 'Installed the KFX plugin 2.20.1. Kindles now get KFX.';

  let fresh = 0;
  async function world() {
    const doc = new StubDocument();
    const focusListeners = new Set<() => void>();
    const pending: { verb: string; resolve: (stdout: string) => void }[] = [];
    g.document = doc;
    g.window = {
      addEventListener: (type: string, fn: () => void) => { if (type === 'focus') focusListeners.add(fn); },
      removeEventListener: (type: string, fn: () => void) => { if (type === 'focus') focusListeners.delete(fn); },
      __TAURI__: {
        core: {
          invoke: (_cmd: string, { args }: { args: string[] }) =>
            new Promise<string>((resolve) => pending.push({ verb: args[0], resolve })),
        },
        opener: { openUrl: async () => undefined },
      },
    };
    // A module of its own per test: kfx.js keeps the machine's state at
    // module level, on purpose, and one test's must not leak into the next.
    fresh += 1;
    const kfx = (await import(`${join(UI, 'kfx.js')}?redraws-${fresh}`)) as KfxDrawing;
    const host = doc.createElement('section');
    host.hidden = true;
    doc.body.append(host);
    const w = {
      doc,
      kfx,
      host,
      restored: 0,
      mount() {
        kfx.mountKfx(host, {
          isSending: () => false,
          devices: () => [],
          onBusy: () => undefined,
          restoreFocus: () => { w.restored += 1; },
        });
      },
      async answer(verb: string, value: unknown) {
        const at = pending.findIndex((p) => p.verb === verb);
        if (at < 0) throw new Error(`nothing asked the engine for ${verb}`);
        pending.splice(at, 1)[0].resolve(JSON.stringify(value));
        await new Promise((resolve) => setTimeout(resolve, 0));
      },
      regainFocus() { for (const fn of [...focusListeners]) fn(); },
      buttons: () => host.querySelectorAll('button'),
      button(label: string) {
        const found = w.buttons().find((b) => b.textContent === label);
        if (found === undefined) throw new Error(`no ${label} button: ${w.buttons().map((b) => b.textContent)}`);
        return found;
      },
      labels: () => w.buttons().map((b) => b.textContent),
      statusNode() {
        const all: StubNode[] = [];
        const walk = (node: StubNode) => { for (const c of node.childNodes) { all.push(c); walk(c); } };
        walk(host);
        return all.find((node) => node.getAttribute('role') === 'status') ?? null;
      },
    };
    w.mount();
    w.kfx.kfxShown();
    return w;
  }

  test('one status line for the life of the block, so a screen reader hears it change', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    const line = w.statusNode();
    expect(line).not.toBe(null);
    w.button('Install').click();
    expect(w.statusNode()).toBe(line);
    expect(line!.textContent).toBe(w.kfx.INSTALLING);
    await w.answer('kfx-install', installed(true, true));
    expect(w.statusNode()).toBe(line);
    expect(line!.textContent).toBe(INSTALLED_READY);
  });

  test('a routine re-probe on coming back changes nothing on screen', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, false, false));
    const get = w.button('Get Kindle Previewer');
    get.focus();
    w.regainFocus();
    await w.answer('kfx-status', status(true, false, false));
    expect(get.isConnected).toBe(true);
    expect(w.doc.activeElement as unknown).toBe(get);
  });

  test('when the rows do change, the keyboard goes back to the same step', async () => {
    // The reader went to get Kindle Previewer; Calibre turned up meanwhile.
    const w = await world();
    await w.answer('kfx-status', status(false, false, false));
    w.button('Get Kindle Previewer').focus();
    w.regainFocus();
    await w.answer('kfx-status', status(true, false, false));
    const now = w.doc.activeElement;
    expect(now.textContent).toBe('Get Kindle Previewer');
    expect(now.isConnected).toBe(true);
    expect(w.restored).toBe(0);
  });

  test('when that control is gone, the page’s own plan takes the keyboard', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    const install = w.button('Install');
    install.focus();
    install.click();
    // "Installing…" is disabled, so it cannot hold the focus.
    expect(w.labels()).toEqual(['Installing…']);
    expect(w.restored).toBe(1);
  });

  test('a busy hook that throws as the install ends does not lose what the install did', async () => {
    // install()'s finally hands the list back through onBusy(false). A throw
    // from it used to leave install() there, skipping the lines after the
    // finally: the new checklist never stored, the status line never
    // written, the Install button still up for a plugin already installed.
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    const busy: boolean[] = [];
    w.kfx.mountKfx(w.host, {
      isSending: () => false,
      devices: () => [],
      onBusy: (on: boolean) => {
        busy.push(on);
        if (!on) throw new Error('the list was already gone');
      },
      restoreFocus: () => { w.restored += 1; },
    });
    const logged: unknown[][] = [];
    const realError = console.error;
    console.error = (...args: unknown[]) => { logged.push(args); };
    try {
      w.button('Install').click();
      await w.answer('kfx-install', installed(true, true));
    } finally {
      console.error = realError;
    }
    expect(busy).toEqual([true, false]);
    expect(w.statusNode()!.textContent).toBe(INSTALLED_READY);
    expect(w.labels()).toEqual([]);
    expect(w.kfx.kfxInstalling()).toBe(false);
    expect(logged.length).toBe(1);
    expect(String(logged[0].at(-1))).toContain('the list was already gone');
  });

  test('a probe still out when an install finishes cannot undo it', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    w.regainFocus(); // a probe goes out...
    w.button('Install').click(); // ...and the install starts behind it
    await w.answer('kfx-install', installed(true, true));
    await w.answer('kfx-status', status(true, true, false)); // the stale one lands last
    expect(w.labels()).toEqual([]);
    expect(w.statusNode()!.textContent).toBe(INSTALLED_READY);
  });

  test('leaving and coming back mid-install keeps the line that says so', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    w.button('Install').click();
    w.kfx.kfxHidden();
    w.kfx.kfxShown();
    w.kfx.kfxRedraw();
    expect(w.statusNode()!.textContent).toBe(w.kfx.INSTALLING);
  });

  test('a success line stays for an unchanged machine and goes when it changes', async () => {
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    w.button('Install').click();
    await w.answer('kfx-install', installed(true, true));
    w.regainFocus();
    await w.answer('kfx-status', status(true, true, true));
    expect(w.statusNode()!.textContent).toBe(INSTALLED_READY);
    // Kindle Previewer was removed since: "Kindles now get KFX." is no
    // longer true, and must not stand over a Get button.
    w.regainFocus();
    await w.answer('kfx-status', status(true, false, true));
    expect(w.labels()).toEqual(['Get Kindle Previewer']);
    expect(w.statusNode()!.textContent).toBe('');
  });

  test('a busy hook that throws cannot leave the install flag on', async () => {
    // While kfxInstalling() is true, send-panel.js refuses every send; stuck on,
    // it would refuse them until the window restarted.
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    w.kfx.mountKfx(w.host, {
      isSending: () => false,
      devices: () => [],
      onBusy: (on: boolean) => { if (on) throw new Error('the page broke'); },
      restoreFocus: () => undefined,
    });
    w.button('Install').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(w.kfx.kfxInstalling()).toBe(false);
    expect(w.statusNode()!.textContent).toBe('the page broke');
    expect(w.labels()).toEqual(['Install']);
  });

  test('a Kobo plugged in mid-install does not take the install’s lines away', async () => {
    const w = await world();
    let devices: unknown[] = [];
    w.kfx.mountKfx(w.host, {
      isSending: () => false,
      devices: () => devices,
      onBusy: () => undefined,
      restoreFocus: () => undefined,
    });
    await w.answer('kfx-status', status(true, true, false));
    w.button('Install').click();
    devices = [{ id: '/m/KOBOe', kind: 'kobo', name: 'Kobo', volume: '/m/KOBOe' }];
    w.kfx.kfxRedraw(); // send-panel.js's flows redraw the block the same way
    expect(w.host.hidden).toBe(false);
    expect(w.statusNode()!.textContent).toBe(w.kfx.INSTALLING);
    await w.answer('kfx-install', installed(true, true));
    expect(w.host.hidden).toBe(false);
    expect(w.statusNode()!.textContent).toBe(INSTALLED_READY);
  });

  test('a block no longer in the page asks the engine nothing', async () => {
    // send-panel.js's blocked state leaves the last block's node
    // detached; a show or a focus must not spend a kfx-status run on it.
    const w = await world();
    await w.answer('kfx-status', status(true, true, false));
    w.doc.body.removeChild(w.host);
    w.regainFocus();
    w.kfx.kfxShown();
    await expect(w.answer('kfx-status', status(true, true, false))).rejects.toThrow('nothing asked');
  });

  test('a block mounted while the page is showing asks, if nothing has answered yet', async () => {
    // A script can arrive while Send already shows its no-script state. The
    // show's probe was skipped (there was nowhere to draw), so the mount that
    // follows has to ask, or the block stays hidden until the next focus.
    const w = await world();
    await w.answer('kfx-status', { ok: false, error: { code: 'internal', message: 'x' } });
    w.doc.body.removeChild(w.host);
    w.kfx.kfxShown();
    const next = w.doc.createElement('section');
    w.doc.body.append(next);
    const hooks = { isSending: () => false, devices: () => [], onBusy: () => undefined };
    w.kfx.mountKfx(next, hooks);
    await w.answer('kfx-status', status(true, true, false));
    expect(next.hidden).toBe(false);
    expect(next.querySelectorAll('button').map((b) => b.textContent)).toEqual(['Install']);
    // Once the machine is known, a remount for a new script asks nothing:
    // the checklist is about the computer, not the script.
    const again = w.doc.createElement('section');
    w.doc.body.append(again);
    w.kfx.mountKfx(again, hooks);
    await expect(w.answer('kfx-status', status(true, true, false))).rejects.toThrow('nothing asked');
  });

  test('a mount on a page that is not showing asks nothing', async () => {
    const w = await world();
    await w.answer('kfx-status', { ok: false, error: { code: 'internal', message: 'x' } });
    w.kfx.kfxHidden();
    const next = w.doc.createElement('section');
    w.doc.body.append(next);
    w.kfx.mountKfx(next, { isSending: () => false, devices: () => [], onBusy: () => undefined });
    await expect(w.answer('kfx-status', status(true, true, false))).rejects.toThrow('nothing asked');
  });
});

describe('no screen prints a stray "null"', () => {
  // el() drops a null child; Node.append() prints it as the word "null", and
  // so does the fake page's append (tests/helpers/fake-dom.ts). The old Send
  // page's empty state shipped one under its list, and drawFailure printed one on
  // every refusal with no script open. This used to be a scan of every
  // .append( call's arguments; it is now the screens themselves, drawn in
  // the sparse states where an optional line is absent.
  let page: FakePage | null = null;
  afterEach(async () => { await page?.close(); page = null; });

  test('the sparse screens of every surface carry no "null" or "undefined"', async () => {
    page = fakePage({
      respond: (args) => (args[0].endsWith('.pdf') ? { ok: false, error: { code: 'scanned', message: 'No text.' } } : HOLD),
    });
    const ctx = (script: unknown) => ({
      state: { script, devices: [], reducedMotion: true },
      goTo: () => {}, restoreFocus: () => {}, scriptChanged: () => {}, convertAgain: () => {},
    });
    const screens: Record<string, FakeNode> = {};

    // Convert: a refusal with no script open, so no "still open" line.
    const convert = await import(`${join(UI, 'convert.js')}?null-scan`);
    screens.convert = page.pane();
    convert.mount(screens.convert, ctx(null));
    await convert.convertPath('/s/Scan.pdf');

    // Read: a script whose pages did not come back, and no author.
    const reader = await import(`${join(UI, 'read.js')}?null-scan`);
    screens.read = page.pane();
    reader.mount(screens.read, ctx({ title: 'Field Station', previewHtml: '' }));

    // Settings: a script with no stored source, so the fault screen.
    const tune = await import(`${join(UI, 'tune.js')}?null-scan`);
    screens.tune = page.pane();
    tune.mount(screens.tune, ctx({ title: 'Field Station', fountainPath: null, epubPath: null, settings: null }));
    await tune.show();

    // The result's send panel: a script with no book on disk.
    const panel = await import(`${join(UI, 'send-panel.js')}?null-scan`);
    screens.panel = page.pane();
    panel.mountPanel(screens.panel, ctx({ title: 'Field Station', epubPath: null, fountainPath: null, settings: null }));
    expect(screens.panel.querySelector('.fault-body')!.textContent).not.toBe('');
    expect(screens.panel.buttons('Convert it again').length).toBe(1);

    for (const [name, pane] of Object.entries(screens)) {
      expect(`${name}: ${pane.textContent.length > 0}`).toBe(`${name}: true`);
      expect(`${name}: ${/\bnull\b|\bundefined\b/.test(pane.textContent)}`).toBe(`${name}: false`);
    }
  });
});

describe('the Kindle heads-up, mounted on its own', () => {
  // heads-up.js on the fake page with a stand-in watcher, for what the
  // booted window cannot reach: the window never closes a script, so the
  // reset main.js makes when none is open is driven here.
  let page: FakePage | null = null;
  afterEach(async () => { await page?.close(); page = null; });
  const kindle = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };

  async function mounted() {
    page = fakePage();
    const { mountHeadsUp } = await import(join(UI, 'heads-up.js'));
    const sheet = page.doc.createElement('div');
    const tabs = page.doc.createElement('nav');
    sheet.append(tabs, page.doc.createElement('section'));
    page.doc.body.append(sheet);
    const subs: Array<(list: unknown) => void> = [];
    const copied: unknown[] = [];
    const later: Array<() => void> = [];
    const state = { script: { title: 'Field Station' } as unknown };
    const heads = mountHeadsUp(sheet, {
      watcher: { subscribe: (fn: (list: unknown) => void) => { subs.push(fn); return () => {}; } },
      state,
      onResult: () => false,
      busy: () => false,
      copyTo: (d: unknown) => copied.push(d),
      restoreFocus: () => {},
      timers: { setTimeout: (fn: () => void) => { later.push(fn); return later.length; }, clearTimeout: () => {} },
    });
    const box = sheet.querySelector('.heads-up')!;
    return { heads, box, sheet, tabs, state, copied, tell: (list: unknown) => { for (const fn of subs) fn(list); } };
  }

  test('it sits right under the tabs, hidden, until a Kindle is news', async () => {
    const m = await mounted();
    expect(m.sheet.childNodes.indexOf(m.box)).toBe(1);
    expect(m.box.hidden).toBe(true);
    m.tell([]);
    m.tell([kindle]);
    expect(m.box.hidden).toBe(false);
    m.box.button('Copy it').click();
    expect(m.copied).toEqual([kindle]);
    expect(m.box.hidden).toBe(true);
  });

  test('a list it cannot read changes nothing', async () => {
    const m = await mounted();
    m.tell([]);
    m.tell(null);
    m.tell([kindle]);
    expect(m.box.hidden).toBe(false);
  });

  test('reset, when no script is open, makes the next list a seed again', async () => {
    const m = await mounted();
    m.tell([]);
    m.heads.reset();
    expect(m.box.hidden).toBe(true);
    m.tell([kindle]);
    expect(m.box.hidden).toBe(true);
  });
});

describe('the window, booted whole', () => {
  // main.js imported the way index.html loads it, on the fake page: the real
  // frame, the three surfaces mounted in it, the window's own shortcut and the
  // drop. What used to be read off main.js's text is driven here instead.
  // Each boot runs its own copy of the scripts (tests/helpers/boot-window.ts),
  // because main.js's handlers on app.js cannot be taken off again.
  let w: BootedWindow | null = null;
  afterEach(async () => { await w?.close(); w = null; });

  const blankBook = (pdf: string) => {
    const name = pdf.replace(/^.*\//, '').replace(/\.pdf$/, '');
    return {
      ok: true, title: name, pages: 18, scenes: 12, characters: 6, warnings: [],
      epubPath: `/lib/${name}/${name}.epub`, fountainPath: `/lib/${name}/${name}.fountain`,
      // No pages: Read shows its "Convert it again" notice rather than a frame.
      previewHtml: '',
    };
  };
  /** The engine: --version and the library probe answered, a PDF converted,
   *  the send panel's Kindle file check refused (it takes a turn on the
   *  book, and a conversion again waits for it), anything else (Settings'
   *  read, the devices poll) held. */
  const engine = (args: string[]) => {
    if (args[0] === '--version') return { ok: true, version: '0.0.0' };
    if (args[0] === 'app-settings') return { ok: false, error: { code: 'internal', message: 'no' } };
    if (args[0] === 'export' && args.includes('--check')) return { ok: false, error: { code: 'internal', message: 'no' } };
    if (args[0].endsWith('.pdf')) return blankBook(args[0]);
    return HOLD;
  };
  const conversions = (win: BootedWindow) => win.tauri.calls.filter((c) => c.args[0].endsWith('.pdf'));
  /** Picks `pdf` through the shortcut and waits for its result screen. */
  async function open(win: BootedWindow, pdf: string) {
    win.press('o', { metaKey: true });
    await settle();
    win.tauri.dialogs.shift()!.resolve(pdf);
    await settle();
    expect(win.surface('convert').dataset.state).toBe('done');
  }

  test('the three surfaces are mounted as the bar’s panels, and the release notes as a sheet off the stamp', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    const { RELEASE } = await import(join(w.ui, 'notes.js'));
    // Sending is the result's panel now: no Send tab, no Send pane.
    expect(w.doc.getElementById('tab-send')).toBeNull();
    expect(w.doc.getElementById('surface-send')).toBeNull();
    expect(w.doc.querySelectorAll('[role="tab"]').map((t) => t.id)).toEqual(['tab-convert', 'tab-read', 'tab-tune']);
    for (const id of ['convert', 'read', 'tune']) {
      const pane = w.surface(id);
      expect(pane.getAttribute('role')).toBe('tabpanel');
      expect(pane.getAttribute('aria-labelledby')).toBe(`tab-${id}`);
      expect(w.tab(id).getAttribute('aria-controls')).toBe(`surface-${id}`);
    }
    expect(w.showing()).toBe('convert');
    expect(w.surface('convert').querySelector('.well')).not.toBeNull();
    // The stamp names the release the notes describe, from the same module.
    const stamp = w.doc.querySelector('.rev-stamp')!;
    expect(stamp.textContent).toBe(`rev ${RELEASE.version}`);
    const sheet = w.doc.querySelector('dialog.sheet-over')!;
    expect(sheet.open).toBe(false);
    stamp.click();
    expect(sheet.open).toBe(true);
    expect(sheet.querySelector('h2')!.textContent).toBe(`Screepub ${RELEASE.version}`);
    sheet.button('Close').click();
    expect(sheet.open).toBe(false);
  });

  test('Read and Settings are off the bar until a script converts', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    const onBar = () => ['convert', 'read', 'tune'].filter((id) => !w!.tab(id).hidden);
    expect(onBar()).toEqual(['convert']);
    await open(w, '/s/Field Station.pdf');
    expect(onBar()).toEqual(['convert', 'read', 'tune']);
  });

  test('a finished conversion hands the keyboard to the panel’s first card, not to Read it under it', async () => {
    // Found live, 2026-10-01: the cards wait on the engine's answer about the
    // stored reader, and the result had already parked the keyboard on the
    // first control it had, Read it.
    w = await bootWindow(SCRATCH, { respond: engine });
    await open(w, '/s/Field Station.pdf');
    await settle();
    expect(w.doc.activeElement).toBe(w.surface('convert').button('Kindle'));
  });

  test('Read’s Send goes back to the result, drawn again if the well replaced it, with the keyboard on the panel', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    await open(w, '/s/Field Station.pdf');
    w.surface('convert').button('Convert another').click();
    expect(w.surface('convert').dataset.state).toBe('idle');
    w.tab('read').click();
    const send = w.surface('read').button('Send');
    // Read shows its notice here (no script drawn): a box beside Convert it
    // again. In the script's head it is quiet; see the read-head test.
    expect(send.className).toContain('btn-outline');
    send.click();
    await settle();
    expect(w.showing()).toBe('convert');
    expect(w.surface('convert').dataset.state).toBe('done');
    // The boot engine refuses app-settings: no reader stored, the cards.
    expect(w.doc.activeElement).toBe(w.surface('convert').button('Kindle'));
    expect(conversions(w).length).toBe(1);
  });

  test('Settings has the same Send, and on the result already up it only hands the keyboard over', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    await open(w, '/s/Field Station.pdf');
    const panel = w.surface('convert').querySelector('.send-panel');
    w.tab('tune').click();
    w.surface('tune').button('Send').click();
    await settle();
    expect(w.showing()).toBe('convert');
    // Nothing redrawn: the result was still up behind the tab.
    expect(w.surface('convert').querySelector('.send-panel')).toBe(panel);
    expect(w.doc.activeElement).toBe(w.surface('convert').button('Kindle'));
    expect(conversions(w).length).toBe(1);
  });

  test('the devices poll runs once a book is open, and not before', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    expect(w.tauri.callsTo('devices')).toEqual([]);
    expect(w.intervals()).toBe(0);
    await open(w, '/s/Field Station.pdf');
    expect(w.tauri.callsTo('devices').map((c) => c.args)).toEqual([['devices', '--json', '--quick']]);
    expect(w.intervals()).toBe(1);
    w.tauri.callsTo('devices')[0].answer({ ok: true, devices: [] });
    await settle();
    w.tickInterval();
    expect(w.tauri.callsTo('devices').length).toBe(2);
    // A second book does not start a second poll.
    await open(w, '/s/Second.pdf');
    expect(w.intervals()).toBe(1);
  });

  test('Ctrl or Cmd O asks for a file from any surface, and a cancel leaves the reader where they were', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, platform: 'Win32' });
    expect(w.press('o').defaultPrevented).toBe(false);
    expect(w.tauri.dialogs).toEqual([]);
    await open(w, '/s/Field Station.pdf');

    w.tab('read').click();
    expect(w.showing()).toBe('read');
    const asked = w.press('O', { ctrlKey: true });
    expect(asked.defaultPrevented).toBe(true);
    await settle();
    expect(w.tauri.dialogs.map((d) => d.kind)).toEqual(['pick_file']);
    // Asking for a file is not converting one: the reader stays put.
    expect(w.showing()).toBe('read');
    w.doc.activeElement = null;
    w.tauri.dialogs.shift()!.resolve(null);
    await settle();
    expect(w.showing()).toBe('read');
    // And the keyboard is put back on the surface that is showing.
    expect(w.surface('read').button('Convert it again')).toBe(w.doc.activeElement!);
    expect(conversions(w).length).toBe(1);
  });

  test('cancelling a file dialog over a result hands the keyboard to the result, not to nobody', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    // Over the idle well: Choose PDF.
    w.press('o', { metaKey: true });
    await settle();
    w.doc.activeElement = null;
    w.tauri.dialogs.shift()!.resolve(null);
    await settle();
    expect(w.doc.activeElement as unknown).toBe(w.surface('convert').querySelector('.well button'));

    await open(w, '/s/Field Station.pdf');
    w.press('o', { metaKey: true });
    await settle();
    w.doc.activeElement = null;
    w.tauri.dialogs.shift()!.resolve(null);
    await settle();
    // The panel's first card: the boot engine refuses app-settings, so no
    // reader is stored and the cards are asked.
    expect(w.doc.activeElement as unknown).toBe(w.surface('convert').button('Kindle'));
  });

  const KINDLE_ON = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  /** Answer the newest devices poll with `devices`, then let it land. */
  async function poll(win: BootedWindow, devices: unknown[]) {
    win.tauri.callsTo('devices').at(-1)!.answer({ ok: true, devices });
    await settle();
  }
  /** The next tick of the devices poll, answered. */
  async function tickWith(win: BootedWindow, devices: unknown[]) {
    win.tickInterval();
    await poll(win, devices);
  }
  const headsUp = (win: BootedWindow) => win.doc.querySelector('.heads-up')!;

  test('a Kindle plugged in while a book is open: one line at the top of the page, Copy or Not now', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    await poll(w, []); // the first list only seeds
    expect(headsUp(w).hidden).toBe(true);
    await tickWith(w, [KINDLE_ON]);
    const line = headsUp(w);
    expect(line.hidden).toBe(false);
    expect(line.querySelector('.heads-up-line')!.textContent).toBe('Kindle plugged in. Copy Field Station?');
    expect(line.getAttribute('role')).toBe('status');
    expect(line.buttons().map((b) => b.textContent)).toEqual(['Copy it', 'Not now']);
    // At the top of the sheet, under the tabs, whatever page is showing.
    const sheet = w.doc.querySelector('.sheet')!;
    expect(sheet.childNodes.indexOf(line)).toBe(sheet.childNodes.indexOf(w.doc.querySelector('.tabs')!) + 1);
    // Never sends without a press.
    expect(w.tauri.callsTo('send')).toEqual([]);
    expect(w.tauri.callsTo('export').filter((c) => !c.args.includes('--check'))).toEqual([]);
  });

  test('Copy goes to the result and copies to that Kindle', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('tune').click();
    await poll(w, []);
    await tickWith(w, [KINDLE_ON]);
    headsUp(w).button('Copy it').click();
    await settle();
    expect(headsUp(w).hidden).toBe(true);
    expect(w.showing()).toBe('convert');
    expect(w.surface('convert').dataset.state).toBe('done');
    // This script's settings are read first (the Settings page had not
    // answered yet), then the Kindle file is made, then copied.
    for (const call of w.tauri.callsTo('settings')) call.answer({ ok: false, error: { code: 'internal', message: 'no' } });
    await settle();
    const exported = w.tauri.callsTo('export').filter((c) => !c.args.includes('--check'));
    expect(exported.map((c) => c.args.slice(0, 5))).toEqual([['export', '/lib/Field Station/Field Station.epub', '--json', '--for', 'kindle']]);
    exported[0].answer({ ok: true, path: '/lib/Field Station/Field Station.azw3', extension: 'azw3', label: 'AZW3' });
    await settle();
    expect(w.tauri.callsTo('send').map((c) => c.args))
      .toEqual([['send', '/lib/Field Station/Field Station.azw3', '--json', '--device', '/Volumes/Kindle']]);
  });

  test('Copy from the line with no reader chosen yet ends on Eject Kindle, and the panel remembers a Kindle', async () => {
    // The panel was on the cards (the boot engine stores no reader), which
    // draw no Eject: spec decision 12 says a USB copy ends with one.
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    expect(w.surface('convert').querySelector('.device-cards')).not.toBeNull();
    w.tab('read').click();
    await poll(w, []);
    await tickWith(w, [KINDLE_ON]);
    headsUp(w).button('Copy it').click();
    await settle();
    for (const call of w.tauri.callsTo('settings')) call.answer({ ok: false, error: { code: 'internal', message: 'no' } });
    await settle();
    w.tauri.callsTo('export').filter((c) => !c.args.includes('--check'))[0]
      .answer({ ok: true, path: '/lib/Field Station/Field Station.azw3', extension: 'azw3', label: 'AZW3' });
    await settle();
    w.tauri.callsTo('send')[0].answer({ ok: true, destination: '/Volumes/Kindle/documents/Field Station.azw3' });
    await settle();
    const convert = w.surface('convert');
    expect(convert.querySelector('.device-cards')).toBeNull();
    expect(convert.querySelector('.calm .btn-brad')!.textContent).toBe('Eject Kindle');
    expect(convert.querySelector('.send-status')!.textContent).toBe('Field Station is on your Kindle. Eject it before you unplug.');
    // Stored as a card press stores it, so the cards are not asked again.
    expect(w.tauri.callsTo('app-settings').map((c) => c.args).at(-1))
      .toEqual(['app-settings', '--json', '--set', '{"readerDevice":"kindle"}']);
  });

  test('the Eject reminder outlives converting another book, until the Kindle is unplugged (spec 13)', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    await poll(w, []);
    await tickWith(w, [KINDLE_ON]);
    headsUp(w).button('Copy it').click();
    await settle();
    for (const call of w.tauri.callsTo('settings')) call.answer({ ok: false, error: { code: 'internal', message: 'no' } });
    await settle();
    w.tauri.callsTo('export').filter((c) => !c.args.includes('--check'))[0]
      .answer({ ok: true, path: '/lib/Field Station/Field Station.azw3', extension: 'azw3', label: 'AZW3' });
    await settle();
    w.tauri.callsTo('send')[0].answer({ ok: true, destination: '/Volumes/Kindle/documents/Field Station.azw3' });
    await settle();
    const ON_IT = 'Field Station is on your Kindle. Eject it before you unplug.';
    expect(w.surface('convert').querySelector('.calm .btn-brad')!.textContent).toBe('Eject Kindle');

    await open(w, '/s/Second.pdf');
    const convert = w.surface('convert');
    expect(convert.querySelector('h2')!.textContent).toContain('Second');
    expect(convert.querySelector('.calm .btn-brad')!.textContent).toBe('Eject Kindle');
    expect(convert.querySelector('.send-status')!.textContent).toBe(ON_IT);

    // Unplugged without ejecting: the button goes.
    await tickWith(w, []);
    expect(convert.buttons('Eject Kindle')).toEqual([]);
  });

  test('Not now, or eight seconds, takes it away; the same Kindle does not bring it back until it is unplugged', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    await poll(w, []);
    await tickWith(w, [KINDLE_ON]);
    const notNow = headsUp(w).button('Not now');
    notNow.focus();
    notNow.click();
    expect(headsUp(w).hidden).toBe(true);
    // The keyboard is not left on a button that has gone.
    expect(w.doc.activeElement).not.toBe(notNow);
    expect(w.surface('read').contains(w.doc.activeElement)).toBe(true);
    // Still plugged in: not news.
    await tickWith(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(true);
    // Unplugged and plugged in again: news again, and it goes by itself.
    await tickWith(w, []);
    await tickWith(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(false);
    w.runTimeouts();
    expect(headsUp(w).hidden).toBe(true);
    expect(w.tauri.callsTo('send')).toEqual([]);
  });

  test('not on the result screen, where the card and the brass already say it', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    await poll(w, []);
    await tickWith(w, [KINDLE_ON]);
    expect(w.showing()).toBe('convert');
    expect(headsUp(w).hidden).toBe(true);
    // And that plug-in is spent: moving to Read does not bring it up late.
    w.tab('read').click();
    await tickWith(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(true);
  });

  test('a Kindle already plugged in when the book opened is not news', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    await poll(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(true);
    await tickWith(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(true);
  });

  test('not while a conversion runs', async () => {
    w = await bootWindow(SCRATCH, {
      respond: (args) => (args[0] === '/s/Second.pdf' ? HOLD : engine(args)), timers: true,
    });
    await open(w, '/s/Field Station.pdf');
    await poll(w, []);
    w.tauri.emit('tauri://drag-drop', { paths: ['/s/Second.pdf'], position: { x: 1, y: 1 } });
    await settle();
    expect(w.surface('convert').dataset.state).toBe('working');
    await tickWith(w, [KINDLE_ON]);
    expect(headsUp(w).hidden).toBe(true);
  });

  test('a file dragged over the window marks the well, and a drop converts the first real path, from any surface', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    const well = w.surface('convert').querySelector('.well')!;
    w.tauri.emit('tauri://drag-enter', { paths: ['/s/A.pdf'], position: { x: 1, y: 1 } });
    expect(well.classList.contains('well-targeted')).toBe(true);
    w.tauri.emit('tauri://drag-leave', null);
    expect(well.classList.contains('well-targeted')).toBe(false);

    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    expect(w.showing()).toBe('read');
    w.tauri.emit('tauri://drag-drop', { paths: ['', '   ', '/s/Second.pdf', '/s/Third.pdf'], position: { x: 1, y: 1 } });
    await settle();
    // The conversion moves the reader to Convert; the drop alone did not ask.
    expect(w.showing()).toBe('convert');
    expect(conversions(w).map((c) => c.args[0])).toEqual(['/s/Field Station.pdf', '/s/Second.pdf']);
  });

  test('"Convert it again" on Read converts the open script’s own source and shows it on Convert', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    w.surface('read').button('Convert it again').click();
    await settle();
    expect(w.showing()).toBe('convert');
    expect(conversions(w).map((c) => c.args[0])).toEqual(['/s/Field Station.pdf', '/s/Field Station.pdf']);
  });
});

describe('every dressed frame follows the desktop’s theme', () => {
  // Found live, 2026-10-01: switching the desktop to dark redrew Read's page
  // but left the result's first page and the Settings preview on light
  // paper until they were drawn again. The listener lived in Read alone.
  const g = globalThis as unknown as Record<string, unknown>;
  const NAMES = ['document', 'matchMedia', 'getComputedStyle'];
  const saved: Record<string, unknown> = {};
  beforeEach(() => { for (const k of NAMES) saved[k] = g[k]; });
  afterEach(() => { for (const k of NAMES) { if (saved[k] === undefined) delete g[k]; else g[k] = saved[k]; } });

  test('a theme change re-dresses the result page and the preview too, and leaves a detached frame alone', async () => {
    let paper = '#F4EFE4';
    const changes: Array<() => void> = [];
    g.document = { documentElement: {} };
    g.getComputedStyle = () => ({ getPropertyValue: (name: string) => (name === '--paper' ? paper : '#222') });
    g.matchMedia = () => ({ matches: false, addEventListener: (_: string, fn: () => void) => changes.push(fn) });
    const frameOf = (connected: boolean) => {
      const doc = { adoptedStyleSheets: [] as Array<{ text: string }> };
      return {
        isConnected: connected,
        contentDocument: doc,
        contentWindow: { CSSStyleSheet: class { text = ''; replaceSync(t: string) { this.text = t; } } },
        sheet: () => doc.adoptedStyleSheets[0]?.text ?? '',
      };
    };
    const { dressFrame } = await import(`${join(UI, 'read.js')}?theme-follow`);
    const result = frameOf(true);
    const preview = frameOf(true);
    const gone = frameOf(true);
    for (const f of [result, preview, gone]) expect(dressFrame(f, 'p { margin: 0 }')).toBe(true);
    gone.isConnected = false;
    expect(changes.length).toBe(1);

    paper = '#1C1A17';
    changes[0]();
    expect(result.sheet()).toContain('background: #1C1A17');
    expect(preview.sheet()).toContain('background: #1C1A17');
    expect(result.sheet()).toContain('p { margin: 0 }');
    expect(gone.sheet()).toContain('background: #F4EFE4');
  });
});
