// The KFX checklist: what the CLI's `kfx-status` prints and the window's Send
// page draws. One function, so the two cannot tell different stories.
import { describe, test, expect } from 'bun:test';
import { kfxSetup, kfxPossible, KFX_LINKS, type KfxSetup } from '../src/export/kfx-setup';
import type { KfxStatus } from '../src/export/kfx';

const status = (calibre: boolean, previewer: boolean, pluginInstalled: boolean): KfxStatus => ({
  calibre,
  previewer,
  pluginInstalled,
  ready: calibre && previewer && pluginInstalled,
});

const step = (setup: KfxSetup, id: string) => setup.steps.find((s) => s.id === id)!;

/** Every combination of the three booleans. */
const ALL: KfxStatus[] = [];
for (const c of [false, true]) for (const p of [false, true]) for (const g of [false, true]) {
  ALL.push(status(c, p, g));
}

describe('kfxSetup', () => {
  test('always three steps, in order, with the names a person knows', () => {
    for (const platform of ['darwin', 'win32', 'linux']) {
      for (const s of ALL) {
        const setup = kfxSetup(s, platform);
        expect(setup.steps.map((x) => x.id)).toEqual(['calibre', 'previewer', 'plugin']);
        expect(setup.steps.map((x) => x.name)).toEqual(['Calibre', 'Kindle Previewer', 'KFX plugin']);
      }
    }
  });

  test('a step has a fix exactly when it is not installed', () => {
    for (const platform of ['darwin', 'win32', 'linux']) {
      for (const s of ALL) {
        for (const st of kfxSetup(s, platform).steps) {
          expect(`${platform} ${st.id} installed=${st.installed} fix=${st.fix !== null}`)
            .toBe(`${platform} ${st.id} installed=${st.installed} fix=${!st.installed}`);
        }
      }
    }
  });

  test('ready passes through and a ready machine says KFX', () => {
    const setup = kfxSetup(status(true, true, true), 'darwin');
    expect(setup.ready).toBe(true);
    expect(setup.summary).toBe('Kindles get KFX, the best quality Screepub can make.');
    expect(setup.steps.every((s) => s.installed)).toBe(true);
  });

  test('ready is exactly status.ready, for every status and platform', () => {
    // A mutant that hard-codes `ready: true` passes the single-case test
    // above (status.ready IS true there). This walks the whole matrix so a
    // false case is checked too.
    for (const platform of ['darwin', 'win32', 'linux']) {
      for (const s of ALL) {
        expect(`${platform} ${JSON.stringify(s)} ready=${kfxSetup(s, platform).ready}`)
          .toBe(`${platform} ${JSON.stringify(s)} ready=${s.ready}`);
      }
    }
  });

  test('missing Calibre links to the download page for this platform', () => {
    expect(step(kfxSetup(status(false, true, false), 'darwin'), 'calibre').fix).toEqual({
      kind: 'link', label: 'Get Calibre', url: 'https://calibre-ebook.com/download_osx',
    });
    expect(step(kfxSetup(status(false, true, false), 'win32'), 'calibre').fix).toEqual({
      kind: 'link', label: 'Get Calibre', url: 'https://calibre-ebook.com/download_windows',
    });
    expect(step(kfxSetup(status(false, false, false), 'linux'), 'calibre').fix).toEqual({
      kind: 'link', label: 'Get Calibre', url: 'https://calibre-ebook.com/download',
    });
  });

  test('missing Previewer links to Amazon where Amazon makes it, and says why not elsewhere', () => {
    for (const platform of ['darwin', 'win32']) {
      expect(step(kfxSetup(status(true, false, true), platform), 'previewer').fix).toEqual({
        kind: 'link',
        label: 'Get Kindle Previewer',
        url: 'https://kdp.amazon.com/en_US/help/topic/G202131170',
      });
    }
    expect(step(kfxSetup(status(true, false, true), 'linux'), 'previewer').fix).toEqual({
      kind: 'unavailable', why: 'Amazon does not make it for Linux',
    });
    expect(step(kfxSetup(status(true, false, true), 'freebsd'), 'previewer').fix).toEqual({
      kind: 'unavailable', why: 'Amazon does not make it for this system',
    });
  });

  test('the plugin installs when Calibre is there, and waits for Calibre when it is not', () => {
    expect(step(kfxSetup(status(true, true, false), 'darwin'), 'plugin').fix).toEqual({
      kind: 'install', label: 'Install',
    });
    expect(step(kfxSetup(status(false, true, false), 'darwin'), 'plugin').fix).toEqual({
      kind: 'after', why: 'Install Calibre first',
    });
  });

  test('the plugin is unavailable where KFX cannot happen, whatever Calibre says', () => {
    // Off darwin/win32 the plugin can never run (no Kindle Previewer to
    // drive), so offering to install it, or telling the user to install
    // Calibre first so they CAN install it, both point at a dead end.
    expect(step(kfxSetup(status(true, false, false), 'linux'), 'plugin').fix).toEqual({
      kind: 'unavailable', why: 'Of no use without Kindle Previewer',
    });
    expect(step(kfxSetup(status(false, false, false), 'linux'), 'plugin').fix).toEqual({
      kind: 'unavailable', why: 'Of no use without Kindle Previewer',
    });
    // darwin is unaffected: install/after still hold there.
    expect(step(kfxSetup(status(true, true, false), 'darwin'), 'plugin').fix).toEqual({
      kind: 'install', label: 'Install',
    });
    expect(step(kfxSetup(status(false, true, false), 'darwin'), 'plugin').fix).toEqual({
      kind: 'after', why: 'Install Calibre first',
    });
  });

  test('pluginInstalled means nothing without Calibre, so the step reads not installed', () => {
    // KfxStatus documents pluginInstalled as "only meaningful when calibre is
    // true". A caller that set it anyway must not get a green plugin row under
    // a missing Calibre.
    const s = { calibre: false, previewer: true, pluginInstalled: true, ready: false };
    expect(step(kfxSetup(s, 'darwin'), 'plugin').installed).toBe(false);
  });

  test('the summary names the rung a Kindle gets today, from the ladder: our AZW3, Calibre or not', () => {
    const today = 'Kindles get AZW3 (beta) for now, which Screepub makes itself. '
      + 'KFX looks better, and needs the three free tools below.';
    expect(kfxSetup(status(true, false, false), 'darwin').summary).toBe(today);
    expect(kfxSetup(status(false, false, false), 'win32').summary).toBe(today);
    const linux = 'Amazon does not make Kindle Previewer for Linux, so Kindles get AZW3 (beta), which Screepub makes itself.';
    expect(kfxSetup(status(true, false, false), 'linux').summary).toBe(linux);
    expect(kfxSetup(status(false, false, false), 'linux').summary).toBe(linux);
  });

  test('possible is exactly the two platforms Amazon makes Previewer for', () => {
    expect(kfxPossible('darwin')).toBe(true);
    expect(kfxPossible('win32')).toBe(true);
    for (const p of ['linux', 'freebsd', 'openbsd', 'sunos', 'aix', '']) {
      expect(`${p}: ${kfxPossible(p)}`).toBe(`${p}: false`);
    }
    expect(kfxSetup(status(true, true, true), 'darwin').possible).toBe(true);
    expect(kfxSetup(status(true, false, true), 'linux').possible).toBe(false);
  });

  test('every link drawn where KFX is possible is one of KFX_LINKS, and each is used', () => {
    // KFX_LINKS is what tests/desktop-shell.test.ts holds the opener grant
    // to. A link the engine can emit on darwin or win32 that is missing from
    // it would be a button the window's permission refuses.
    const emitted = new Set<string>();
    for (const platform of ['darwin', 'win32']) {
      for (const s of ALL) {
        for (const st of kfxSetup(s, platform).steps) {
          if (st.fix?.kind === 'link') emitted.add(st.fix.url);
        }
      }
    }
    for (const url of emitted) expect(`${url} listed: ${KFX_LINKS.includes(url)}`).toBe(`${url} listed: true`);
    for (const url of KFX_LINKS) expect(`${url} emitted: ${emitted.has(url)}`).toBe(`${url} emitted: true`);
    expect([...KFX_LINKS].sort()).toEqual([
      'https://calibre-ebook.com/download_osx',
      'https://calibre-ebook.com/download_windows',
      'https://kdp.amazon.com/en_US/help/topic/G202131170',
    ]);
  });

  test('no string a person reads carries an em dash', () => {
    for (const platform of ['darwin', 'win32', 'linux', 'freebsd']) {
      for (const s of ALL) {
        const text = JSON.stringify(kfxSetup(s, platform));
        expect(`${platform}: ${text.includes('—')}`).toBe(`${platform}: false`);
      }
    }
  });
});
