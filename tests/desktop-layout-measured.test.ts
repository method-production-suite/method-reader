// The window layout pass, measured rather than read
// (docs/superpowers/specs/2026-09-29-window-layout-pass-design.md).
//
// tests/desktop-layout.test.ts reads the stylesheets, and a stylesheet can
// say every right thing and still draw a broken page: a rule that loses on
// specificity, a container query on the wrong element, a grid track that
// overflows. This draws the real window (desktop/ui/index.html with its own
// CSS and JS) in headless Chrome at three window sizes and measures where
// things land with getBoundingClientRect.
//
// The page is served by the capture tool's own server (tools/capture/
// server.ts) with its stand-in for window.__TAURI__ (bridge.js) and its
// "result" steps (steps.js), which drop tests/fixtures/field-station.pdf on
// the window and wait for the finished book, so Read and Settings have a
// script to show. The engine behind the stand-in is the real CLI, run with a
// scratch library and a scratch settings folder, through the capture gate.
// It converts to EPUB only, so no Calibre is needed.
//
// The result screen (spec 2026-10-01, decision 2) is measured twice at each
// size: with the device cards a first run shows, and with the calm screen a
// press of the Kobo card leaves. Kobo, not Kindle: no Kindle file is built
// ahead for a Kobo, so the run asks the engine for nothing the capture gate
// does not already allow. The press writes `readerDevice` into the run's
// scratch settings folder, never a real one.
//
// Skipped, saying why, where Chrome is not at the capture tool's path (Linux
// CI); the CSS pins in desktop-layout.test.ts still run there.

import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CHROME } from '../tools/capture/cdp';
import { type EngineResult, makeHandler } from '../tools/capture/server';
import { type LayoutPage, launchLayoutPage } from './helpers/layout-chrome';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-desktop-layout-measured-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const REPO = join(import.meta.dir, '..');
const DEMO_PDF = join(REPO, 'tests', 'fixtures', 'field-station.pdf');
const HAVE_CHROME = existsSync(CHROME);

/** Layout tolerance in CSS pixels: sub-pixel rounding, nothing more. */
const PX = 1;
/** The brief allows the brads a little more, for the svg's own rounding. */
const BRAD_PX = 2;

interface Box { left: number; right: number; top: number; bottom: number; width: number; height: number }

/** What the CSS intends at each size. The block is the window less the
 *  100px binding and the 48px right margin, capped at 1180 and centred in
 *  that space; it folds Read and Settings to one column under 880. */
const SIZES = [
  // 1280: the block fills the space, 100..1232.
  { name: '1280x800', width: 1280, height: 800, sheet: { left: 100, width: 1132 }, folded: false },
  // 1800: the space is 1652, the block is capped at 1180, the 472 left over
  // splits evenly, so it starts at 100 + 236.
  { name: '1800x1000', width: 1800, height: 1000, sheet: { left: 336, width: 1180 }, folded: false },
  // 900: still over the 720 narrow-window rules, so the same margins; the
  // block is 752, under the 880 fold.
  { name: '900x800', width: 900, height: 800, sheet: { left: 100, width: 752 }, folded: true },
] as const;

const BRAD_CENTRE = 44; // --hole-center, from the window's left edge
// Convert's 820 column is now only the drop well's (the result takes the
// whole block). The well is not on screen here, so desktop-layout.test.ts's
// CSS pin on #surface-convert holds it instead.
const RESULT_PAGE = 400;   // the first page's column
const RESULT_PANEL = 620;  // the panel's cap
const INDEX_SLOT = 218;
const SCRIPT_COLUMN = 706;
const SETTINGS_COLUMN = 420;
const GAP = 32; // --space-7 at a 16px root

interface ResultBox { result: Box; page: Box; side: Box; brass: Box | null; cards: Box[]; overflowX: boolean }

interface Measured {
  frame: { vw: number; sheet: Box; brads: number[]; firstTab: Box; foot: Box };
  cards: ResultBox;
  calm: ResultBox;
  read: { rail: Box; stage: Box; frame: Box; railShown: boolean };
  tune: {
    split: Box; knobs: Box; preview: Box; previewFrame: Box;
    position: string; stickyTop: number; rem: number; innerHeight: number;
    scrolledBy: number; knobsAfter: Box; previewAfter: Box; previewFrameAfter: Box;
  };
}

// The page-side measuring scripts. Plain JS strings: they run in Chrome.
const BOX = `const box = (el) => {
  if (!el) throw new Error('missing element');
  const b = el.getBoundingClientRect();
  return { left: b.left, right: b.right, top: b.top, bottom: b.bottom, width: b.width, height: b.height };
};`;
const q = (selector: string) => `document.querySelector(${JSON.stringify(selector)})`;
const twoFrames = 'new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))';
/** The result screen's boxes, as the page draws them right now. */
const RESULT = `(() => { ${BOX}
  const brass = ${q('#surface-convert .calm .btn-brad')};
  return {
    result: box(${q('#surface-convert .result')}), page: box(${q('#surface-convert .result-page')}),
    side: box(${q('#surface-convert .result-side')}), brass: brass ? box(brass) : null,
    cards: [...document.querySelectorAll('#surface-convert .device-card')].map(box),
    overflowX: document.documentElement.scrollWidth > window.innerWidth,
  };
})()`;

/** Poll a page expression until it is truthy, or fail naming `what`. */
async function until(page: LayoutPage, expression: string, what: string, ms = 30_000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < ms) {
    if (await page.evaluate<boolean>(`Boolean(${expression})`).catch(() => false)) return;
    await Bun.sleep(50);
  }
  throw new Error(`layout: timed out waiting for ${what}`);
}

const TITLE = 'the window, measured in headless Chrome';
describe.skipIf(!HAVE_CHROME)(HAVE_CHROME ? TITLE : `${TITLE} (skipped: no Chrome at ${CHROME})`, () => {
  const measured = new Map<string, Measured>();
  const refused: string[] = [];
  const failed: string[] = [];
  const notFound: string[] = [];
  const serverErrors: string[] = [];
  const running = new Set<ReturnType<typeof Bun.spawn>>();
  let library = '';
  let server: ReturnType<typeof Bun.serve> | null = null;
  let page: LayoutPage | null = null;

  afterAll(async () => {
    // Each on its own, so one failure cannot skip the rest.
    for (const proc of running) proc.kill('SIGKILL');
    await page?.close().catch(() => {});
    server?.stop(true);
    if (library !== '') rmSync(library, { recursive: true, force: true });
  }, 30_000);

  beforeAll(async () => {
    library = realpathSync(mkdtempSync(join(SCRATCH, 'library-')));
    mkdirSync(join(library, '.no-volumes'), { recursive: true });
    const runEngine = async (args: string[]): Promise<EngineResult> => {
      const proc = Bun.spawn([process.execPath, join(REPO, 'src', 'cli.ts'), ...args], {
        cwd: REPO,
        stdin: 'ignore', stdout: 'pipe', stderr: 'pipe',
        // A scratch library, and a settings folder under it that is never
        // made, so the engine reads no settings at all (the capture tool's
        // own arrangement). process.env carries the Calibre guard on.
        env: {
          ...process.env, SCREEPUB_LIBRARY: library, SCREEPUB_CONFIG_DIR: join(library, '.layout-app-settings'),
          // No real reader, ever: an empty mount parent, and a reMarkable
          // address nothing listens on.
          SCREEPUB_VOLUME_ROOTS: join(library, '.no-volumes'),
          SCREEPUB_REMARKABLE_ENDPOINT: 'http://127.0.0.1:9',
        },
      });
      running.add(proc);
      try {
        const [stdout, stderr, code] = await Promise.all([
          new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
        ]);
        return { code: proc.signalCode ? null : code, stdout, stderr };
      } finally {
        running.delete(proc);
      }
    };

    let handle: (req: Request) => Promise<Response> = async () => new Response('', { status: 503 });
    server = Bun.serve({
      hostname: '127.0.0.1',
      port: 0,
      fetch: (req) => handle(req).catch((e: unknown) => {
        serverErrors.push(e instanceof Error ? e.message : String(e));
        return new Response('', { status: 500 });
      }),
    });
    const host = `127.0.0.1:${server.port}`;
    handle = makeHandler({
      repoDir: REPO, passOne: library, token: crypto.randomUUID(), library, demoPdf: DEMO_PDF, host,
      runEngine, refused, failed, notFound,
    });

    page = await launchLayoutPage();
    const p = page;
    const viewport = (width: number, height: number) =>
      p.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
    // Light, and no motion: the index and the tabs then sit where they end
    // up at once, with no transition to wait out.
    await p.send('Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-color-scheme', value: 'light' }, { name: 'prefers-reduced-motion', value: 'reduce' }],
    });
    await viewport(SIZES[0].width, SIZES[0].height);
    await p.send('Page.navigate', { url: `http://${host}/capture/window.html?shot=result` });
    // steps.js drops the demo PDF and says ready once the book is done.
    const start = Date.now();
    for (;;) {
      const [state, error] = await p.evaluate<[string?, string?]>('[window.__captureState, window.__captureError]')
        .catch(() => [] as [string?, string?]);
      if (state === 'ready') break;
      if (state === 'failed') throw new Error(`layout: the window failed to convert the demo script: ${error}`);
      if (serverErrors.length > 0) throw new Error(`layout: the page server failed: ${serverErrors.join('; ')}`);
      if (Date.now() - start > 90_000) {
        throw new Error(`layout: the window never finished the demo script (state ${state ?? 'none'}; ` +
          `refused ${refused.join('; ') || 'none'}; failed ${failed.join('; ') || 'none'}; ` +
          `not found ${notFound.join(', ') || 'none'})`);
      }
      await Bun.sleep(100);
    }

    const tab = async (id: string) => {
      await p.evaluate(`${q(`#tab-${id}`)}.click()`);
      await until(p, `${q(`#surface-${id}`)} && !${q(`#surface-${id}`)}.hidden`, `the ${id} surface`);
    };

    for (const size of SIZES) {
      await viewport(size.width, size.height);
      await tab('convert');
      await p.evaluate('window.scrollTo(0, 0)');
      await p.evaluate(twoFrames);
      const frame = await p.evaluate<Measured['frame']>(`(() => { ${BOX}
        const brads = [...document.querySelectorAll('.rail svg')]
          .filter((s) => getComputedStyle(s).display !== 'none')
          .map((s) => { const b = s.getBoundingClientRect(); return b.left + b.width / 2; });
        return {
          vw: window.innerWidth, sheet: box(${q('.sheet')}), brads,
          firstTab: box(${q('.tabs [role="tab"]')}), foot: box(${q('.rev-foot')}),
        };
      })()`);
      await until(p, `${q('#surface-convert .device-card')}`, 'the device cards');
      const cards = await p.evaluate<ResultBox>(RESULT);

      await tab('read');
      await until(p, `${q('#surface-read .reader.index-open .scene-rail button')}`, 'the open scene index');
      await p.evaluate(twoFrames);
      const read = await p.evaluate<Measured['read']>(`(() => { ${BOX}
        const rail = ${q('#surface-read .scene-rail')};
        return {
          rail: box(rail), stage: box(${q('#surface-read .script-stage')}),
          frame: box(${q('#surface-read .script-frame')}),
          railShown: getComputedStyle(rail).visibility === 'visible' && getComputedStyle(rail).opacity === '1',
        };
      })()`);

      await tab('tune');
      await until(p, `${q('#surface-tune .tune-split .tune-preview-frame')}`, 'the Settings preview');
      await p.evaluate('window.scrollTo(0, 0)');
      await p.evaluate(twoFrames);
      const before = await p.evaluate<Pick<Measured['tune'],
        'split' | 'knobs' | 'preview' | 'previewFrame' | 'position' | 'stickyTop' | 'rem' | 'innerHeight'>>(`(() => { ${BOX}
        const preview = ${q('#surface-tune .tune-preview')};
        const style = getComputedStyle(preview);
        return {
          split: box(${q('#surface-tune .tune-split')}), knobs: box(${q('#surface-tune .tune-knobs')}),
          preview: box(preview), previewFrame: box(${q('#surface-tune .tune-preview-frame')}),
          position: style.position, stickyTop: parseFloat(style.top) || 0,
          rem: parseFloat(getComputedStyle(document.documentElement).fontSize),
          innerHeight: window.innerHeight,
        };
      })()`);
      // Far enough that an unpinned preview would have gone above its pinned
      // top, and no further than the page can scroll.
      const want = Math.max(0, Math.ceil(before.preview.top - before.stickyTop) + 200);
      const scrolledBy = await p.evaluate<number>(`(async () => {
        window.scrollTo(0, ${want}); await ${twoFrames}; return window.scrollY;
      })()`);
      const after = await p.evaluate<Pick<Measured['tune'], 'knobsAfter' | 'previewAfter' | 'previewFrameAfter'>>(`(() => { ${BOX}
        return {
          knobsAfter: box(${q('#surface-tune .tune-knobs')}), previewAfter: box(${q('#surface-tune .tune-preview')}),
          previewFrameAfter: box(${q('#surface-tune .tune-preview-frame')}),
        };
      })()`);
      await p.evaluate('window.scrollTo(0, 0)');

      measured.set(size.name, { frame, cards, calm: cards, read, tune: { ...before, ...after, scrolledBy } });
    }

    // The calm screen: the Kobo card pressed once, then every size again.
    await tab('convert');
    await p.evaluate(`${q('#surface-convert .device-card[data-key="kobo"]')}.click()`);
    await until(p, `${q('#surface-convert .calm .btn-brad')}`, 'the calm screen’s brass button');
    for (const size of SIZES) {
      await viewport(size.width, size.height);
      await p.evaluate('window.scrollTo(0, 0)');
      await p.evaluate(twoFrames);
      const calm = await p.evaluate<ResultBox>(RESULT);
      measured.set(size.name, { ...measured.get(size.name)!, calm });
    }
  }, 180_000);

  const get = (name: string) => {
    const m = measured.get(name);
    if (!m) throw new Error(`layout: nothing was measured at ${name}`);
    return m;
  };
  const near = (actual: number, expected: number, what: string, tol = PX) => {
    expect(`${what}: ${actual.toFixed(1)} ${Math.abs(actual - expected) <= tol ? '~' : '!='} ${expected}`)
      .toBe(`${what}: ${actual.toFixed(1)} ~ ${expected}`);
  };

  for (const size of SIZES) {
    const sheetLeft = size.sheet.left;
    const sheetRight = size.sheet.left + size.sheet.width;
    const sheetCentre = sheetLeft + size.sheet.width / 2;

    describe(`at ${size.name}`, () => {
      test('the viewport is the size asked for', () => {
        expect(get(size.name).frame.vw).toBe(size.width);
      });

      test('the brads sit 44px from the window’s left edge', () => {
        const { brads } = get(size.name).frame;
        expect(brads.length).toBe(size.height < 560 ? 1 : 3);
        for (const [i, centre] of brads.entries()) near(centre, BRAD_CENTRE, `brad ${i} centre`, BRAD_PX);
      });

      test('the block runs from the binding to the right margin, capped at 1180 and centred', () => {
        const { sheet } = get(size.name).frame;
        near(sheet.left, sheetLeft, 'block left');
        near(sheet.right, sheetRight, 'block right');
        expect(sheet.width).toBeLessThanOrEqual(1180 + PX);
        // Centred in the space between the 100 binding and the 48 margin.
        near(sheet.left - 100, size.width - 48 - sheet.right, 'room left of the block vs right of it');
      });

      test('the tabs start at the block’s left edge and the foot ends at its right', () => {
        const { firstTab, foot } = get(size.name).frame;
        near(firstTab.left, sheetLeft, 'first tab left');
        near(foot.right, sheetRight, 'foot right');
      });

      if (!size.folded) {
        test('the result: a 400 page at the left of the pair, the panel 32 to its right, the pair centred in the block', () => {
          for (const r of [get(size.name).cards, get(size.name).calm]) {
            near(r.page.width, RESULT_PAGE, 'page width');
            near(r.side.left, r.page.right + GAP, 'panel left');
            expect(r.side.width).toBeLessThanOrEqual(RESULT_PANEL + PX);
            near(r.page.top, r.side.top, 'page top vs panel top');
            near(r.page.left - sheetLeft, sheetRight - r.side.right, 'room left of the pair vs right of it');
          }
        });

        test('Read: a 218 index slot at the block’s left, then the 706 script beside it', () => {
          const { rail, stage, frame, railShown } = get(size.name).read;
          expect(railShown).toBe(true);
          near(rail.left, sheetLeft, 'index left');
          near(rail.width, INDEX_SLOT, 'index width');
          near(stage.left, sheetLeft + INDEX_SLOT + GAP, 'script left');
          near(stage.width, SCRIPT_COLUMN, 'script width');
          near(frame.width, SCRIPT_COLUMN, 'script frame width');
          near(stage.top, rail.top, 'script top vs index top');
        });

        test('Settings: a 420 column, the preview filling the rest of the block', () => {
          const { knobs, preview, previewFrame } = get(size.name).tune;
          near(knobs.left, sheetLeft, 'settings left');
          near(knobs.width, SETTINGS_COLUMN, 'settings width');
          near(preview.left, sheetLeft + SETTINGS_COLUMN + GAP, 'preview left');
          near(preview.right, sheetRight, 'preview right');
          near(preview.width, size.sheet.width - SETTINGS_COLUMN - GAP, 'preview width');
          // The frame's border stays inside its column.
          expect(previewFrame.right).toBeLessThanOrEqual(preview.right + PX);
        });

        test('Settings: the preview stays pinned while the page scrolls, and its frame fits the window', () => {
          const t = get(size.name).tune;
          expect(t.scrolledBy).toBeGreaterThan(100);
          // The page did scroll under it...
          near(t.knobsAfter.top, t.knobs.top - t.scrolledBy, 'settings top after scrolling');
          // ...and the preview did not go with it.
          near(t.previewAfter.top, t.stickyTop, 'preview top after scrolling');
          expect(t.stickyTop).toBeGreaterThan(0);
          // Pinned, the whole frame is on screen and runs most of its height:
          // it stops a bottom margin (3rem) short of the window's foot, less
          // whatever the label above it does not take of its 2rem allowance.
          expect(t.previewFrameAfter.bottom).toBeLessThanOrEqual(t.innerHeight + PX);
          expect(t.previewFrameAfter.bottom).toBeGreaterThan(t.innerHeight - 6 * t.rem);
        });
      } else {
        test('the result under the fold: the panel first, the page under it, both centred', () => {
          for (const r of [get(size.name).cards, get(size.name).calm]) {
            expect(r.side.bottom).toBeLessThanOrEqual(r.page.top + PX);
            near(r.side.left + r.side.width / 2, sheetCentre, 'panel centre');
            near(r.page.left + r.page.width / 2, sheetCentre, 'page centre');
          }
        });

        test('Read: under the fold the index stacks above the script, which takes the block', () => {
          const { rail, stage, railShown } = get(size.name).read;
          expect(railShown).toBe(true);
          near(rail.left, sheetLeft, 'index left');
          near(stage.left, sheetLeft, 'script left');
          near(stage.width, size.sheet.width, 'script width');
          expect(rail.bottom).toBeLessThanOrEqual(stage.top + PX);
        });

        test('Settings: under the fold it is one column, the preview below the settings and unpinned', () => {
          const t = get(size.name).tune;
          near(t.knobs.left, sheetLeft, 'settings left');
          near(t.preview.left, sheetLeft, 'preview left');
          near(t.preview.width, size.sheet.width, 'preview width');
          expect(t.preview.top).toBeGreaterThanOrEqual(t.knobs.bottom - PX);
          expect(t.position).toBe('static');
          expect(t.scrolledBy).toBeGreaterThan(100);
          near(t.previewAfter.top, t.preview.top - t.scrolledBy, 'preview top after scrolling');
        });
      }

      test('nothing on the result runs off the side, and every card and the brass button sit inside the panel', () => {
        for (const r of [get(size.name).cards, get(size.name).calm]) {
          expect(r.overflowX).toBe(false);
          for (const c of r.cards) {
            expect(c.left).toBeGreaterThanOrEqual(r.side.left - PX);
            expect(c.right).toBeLessThanOrEqual(r.side.right + PX);
          }
        }
        // Six cards on the first run, none once one is chosen.
        expect(get(size.name).cards.cards.length).toBe(6);
        expect(get(size.name).calm.cards.length).toBe(0);
        const brass = get(size.name).calm.brass;
        expect(brass).not.toBeNull();
        expect(brass!.left).toBeGreaterThanOrEqual(get(size.name).calm.side.left - PX);
        expect(brass!.right).toBeLessThanOrEqual(get(size.name).calm.side.right + PX);
      });
    });
  }

  test('the window made no engine call the capture gate refused, and none failed', () => {
    expect(refused).toEqual([]);
    expect(failed).toEqual([]);
    expect(serverErrors).toEqual([]);
  });
});
