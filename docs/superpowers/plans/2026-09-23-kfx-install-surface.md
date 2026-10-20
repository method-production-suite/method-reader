# KFX Install Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A user with Calibre and no KFX plugin can install it from the CLI (`screepub kfx-install`) or from a "Best Kindle quality" block on the window's Send page, which also links to Calibre and Kindle Previewer when they are missing.

**Architecture:** The engine turns `kfxStatus()` into a three-step checklist (`src/export/kfx-setup.ts`, pure). Two new verbs carry it out (`src/cli-kfx.ts` handlers, wired in `src/cli.ts`). The window gets two argv builders in `app.js`, a new `desktop/ui/kfx.js` (pure decisions above the line, drawing below), and a few lines in `send.js`. Three exact URLs join the opener grant.

**Tech Stack:** Bun + TypeScript (engine, `bun:test`), plain ES modules in `desktop/ui/`, Tauri 2 capability JSON.

**Spec:** [2026-09-23-kfx-install-surface-design.md](../specs/2026-09-23-kfx-install-surface-design.md)

---

## Ground rules for every task

- Worktree: `<repo>/.claude/worktrees/epic-neumann-254843`, branch `parity-d-kfx-install`. Use absolute paths. Never `cd` to the main checkout.
- TDD: write the failing test, run it, see it fail for the stated reason, then implement.
- **No em dashes (`—`) in any string a person reads** that this plan adds: engine messages, CLI output, window copy, docs you write. Use colons, periods or commas. Existing copy keeps its style.
- **Nothing in the test suite may reach the real installer on the machine running it.** It fetches from the network and writes into the user's Calibre. Handler tests inject a fake installer. Spawned-CLI tests of `kfx-install` run with `CALIBRE_CONFIG_DIRECTORY` pointed into the test file's scratch folder, so even a regression lands in a throwaway Calibre config, not the real one (verified 2026-09-23: with that variable set, `kfxStatus()` reads the scratch config and the real one is untouched).
- Temp folders in tests follow `tests/temp-hygiene.test.ts`: one top-level `const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-<file>-'));`, one top-level `afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));`, every other temp dir inside SCRATCH.
- Commit messages end with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- If you write a mutation script to prove a test can fail, name it for your task (`task3-mutants.ts`), write it under the session scratchpad, restore the source in a `finally`, and confirm `git status --short` shows only your intended files before committing.
- Files another session (Screeepub 1) is editing: `desktop/ui/frame.js`, `main.js`, `update.js`, `notes-surface.js`, `convert.js`, `style.css`, `desktop/src-tauri/Cargo.toml`, `src/main.rs`. **Do not touch them.** In `desktop/src-tauri/capabilities/default.json` touch only the `opener:allow-open-url` allow list and one clause of the description. In `tests/desktop-shell.test.ts` touch only the opener URL test. In `desktop/ui/app.js` touch only the `argv` object.

---

## File map

| File | Status | Responsibility |
| --- | --- | --- |
| `src/export/kfx-setup.ts` | new | `KfxStatus` + platform → checklist (steps, fixes, summary); `KFX_LINKS` |
| `src/cli-kfx.ts` | new | `kfxStatusCommand`, `kfxInstallCommand`, `setupLines`, `installLines` |
| `src/cli-errors.ts` | modify | add `'kfx-install-failed'` to the code union |
| `src/cli-devices.ts` | modify | `VERBS` gains two names |
| `src/cli.ts` | modify | usage text, `verbUsage`, the verb branch |
| `desktop/src-tauri/capabilities/default.json` | modify | three exact URLs, one description clause |
| `desktop/ui/app.js` | modify | `argv.kfxStatus`, `argv.kfxInstall` |
| `desktop/ui/kfx.js` | new | Send page's KFX block: decisions + drawing |
| `desktop/ui/send.js` | modify | mount the block, lifecycle hooks, busy checks |
| `tests/export-kfx-setup.test.ts` | new | checklist matrix |
| `tests/cli-kfx.test.ts` | new | handlers + spawned CLI |
| `tests/cli-devices.test.ts` | modify | `VERBS` pin |
| `tests/desktop-shell.test.ts` | modify | opener test |
| `tests/desktop-ui.test.ts` | modify | argv list pin, `kfx.js` decisions and shape |
| `tests/capture.test.ts` | modify | the capture gate refuses both kfx verbs |
| docs | modify | ADR amendment, desktop README note, parity audit + plan, README |

---

### Task 1: The checklist (`src/export/kfx-setup.ts`)

**Files:**
- Create: `src/export/kfx-setup.ts`
- Test: `tests/export-kfx-setup.test.ts`

- [ ] **Step 1: Write the failing test**

Create `tests/export-kfx-setup.test.ts`:

```ts
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

  test('pluginInstalled means nothing without Calibre, so the step reads not installed', () => {
    // KfxStatus documents pluginInstalled as "only meaningful when calibre is
    // true". A caller that set it anyway must not get a green plugin row under
    // a missing Calibre.
    const s = { calibre: false, previewer: true, pluginInstalled: true, ready: false };
    expect(step(kfxSetup(s, 'darwin'), 'plugin').installed).toBe(false);
  });

  test('the summary names the rung a Kindle gets today, from the ladder', () => {
    expect(kfxSetup(status(true, false, false), 'darwin').summary).toBe(
      'Kindles get AZW3 for now. KFX looks better, and needs the three free tools below.',
    );
    expect(kfxSetup(status(false, false, false), 'win32').summary).toBe(
      'Kindles get MOBI for now. KFX looks better, and needs the three free tools below.',
    );
    expect(kfxSetup(status(true, false, false), 'linux').summary).toBe(
      'Amazon does not make Kindle Previewer for Linux, so Kindles get AZW3.',
    );
    expect(kfxSetup(status(false, false, false), 'linux').summary).toBe(
      'Amazon does not make Kindle Previewer for Linux, so Kindles get MOBI.',
    );
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
```

- [ ] **Step 2: Run it and see it fail**

Run: `bun test tests/export-kfx-setup.test.ts`
Expected: FAIL, `Cannot find module '../src/export/kfx-setup'`.

- [ ] **Step 3: Write the module**

Create `src/export/kfx-setup.ts`:

```ts
// What to tell a person about the KFX rung, and what they can do about it.
//
// kfx.ts PROBES (is Calibre there, is Previewer there, is the plugin in
// Calibre) and formats.ts owns the LADDER (KFX, else AZW3, else MOBI). This
// file turns the probe into a three-step checklist with one fix per missing
// step and one sentence saying what a Kindle gets today. The CLI's
// `kfx-status` prints it and the window's Send page draws it, so the two
// tell the same story from one place. Pure: no probe, no spawn, no network.
import { fileExtension } from './formats';
import type { KfxStatus } from './kfx';

/** How to fix one missing step. */
export type KfxFix =
  /** Only the user can install it; this page is where. */
  | { kind: 'link'; label: string; url: string }
  /** Screepub can install it: `screepub kfx-install`. */
  | { kind: 'install'; label: string }
  /** Needs another step done first. */
  | { kind: 'after'; why: string }
  /** Cannot be had on this platform at all. */
  | { kind: 'unavailable'; why: string };

export interface KfxStep {
  id: 'calibre' | 'previewer' | 'plugin';
  name: string;
  installed: boolean;
  /** Null exactly when installed. */
  fix: KfxFix | null;
}

export interface KfxSetup {
  /** KfxStatus.ready, passed through. */
  ready: boolean;
  /** False where Amazon makes no Kindle Previewer, so KFX cannot happen. */
  possible: boolean;
  /** One sentence: what a Kindle gets today. */
  summary: string;
  /** Always three, in this order: calibre, previewer, plugin. */
  steps: KfxStep[];
}

const CALIBRE_PAGES: Record<string, string> = {
  darwin: 'https://calibre-ebook.com/download_osx',
  win32: 'https://calibre-ebook.com/download_windows',
};
/** Calibre's chooser page, for a platform with no page of its own. Only
 *  emitted where `possible` is false, which the window never draws, so it is
 *  deliberately NOT in KFX_LINKS and not in the window's opener grant. */
const CALIBRE_ANY = 'https://calibre-ebook.com/download';
/** Amazon's Kindle Previewer page. One page carries both the Mac and the
 *  Windows download (checked 2026-09-23). */
const PREVIEWER_PAGE = 'https://kdp.amazon.com/en_US/help/topic/G202131170';

/** Every URL a link fix can carry where KFX is possible. The window's opener
 *  grant allows exactly these, and tests/desktop-shell.test.ts holds the two
 *  together, so the engine cannot draw a link the window may not open. */
export const KFX_LINKS: readonly string[] = [...Object.values(CALIBRE_PAGES), PREVIEWER_PAGE];

/** Amazon makes Kindle Previewer for macOS and Windows and nothing else;
 *  kfx.ts's previewerPath() has a branch for exactly these two. */
export function kfxPossible(platform: string): boolean {
  return platform === 'darwin' || platform === 'win32';
}

function systemName(platform: string): string {
  return platform === 'linux' ? 'Linux' : 'this system';
}

function calibreFix(status: KfxStatus, platform: string): KfxFix | null {
  if (status.calibre) return null;
  return { kind: 'link', label: 'Get Calibre', url: CALIBRE_PAGES[platform] ?? CALIBRE_ANY };
}

function previewerFix(status: KfxStatus, platform: string): KfxFix | null {
  if (status.previewer) return null;
  if (!kfxPossible(platform)) {
    return { kind: 'unavailable', why: `Amazon does not make it for ${systemName(platform)}` };
  }
  return { kind: 'link', label: 'Get Kindle Previewer', url: PREVIEWER_PAGE };
}

function pluginFix(status: KfxStatus, installed: boolean): KfxFix | null {
  if (installed) return null;
  if (!status.calibre) return { kind: 'after', why: 'Install Calibre first' };
  return { kind: 'install', label: 'Install' };
}

export function kfxSetup(status: KfxStatus, platform: string): KfxSetup {
  const possible = kfxPossible(platform);
  // KfxStatus documents pluginInstalled as meaningful only with Calibre:
  // the plugin lives inside it.
  const pluginInstalled = status.calibre && status.pluginInstalled;
  const steps: KfxStep[] = [
    { id: 'calibre', name: 'Calibre', installed: status.calibre, fix: calibreFix(status, platform) },
    {
      id: 'previewer',
      name: 'Kindle Previewer',
      installed: status.previewer,
      fix: previewerFix(status, platform),
    },
    { id: 'plugin', name: 'KFX plugin', installed: pluginInstalled, fix: pluginFix(status, pluginInstalled) },
  ];
  // The ladder's own answer for "not KFX", so this sentence cannot disagree
  // with what the export actually builds.
  const today = fileExtension('kindle', { calibreAvailable: status.calibre, kfxReady: false }).toUpperCase();
  let summary: string;
  if (status.ready) {
    summary = 'Kindles get KFX, the best quality Screepub can make.';
  } else if (!possible) {
    summary = `Amazon does not make Kindle Previewer for ${systemName(platform)}, so Kindles get ${today}.`;
  } else {
    summary = `Kindles get ${today} for now. KFX looks better, and needs the three free tools below.`;
  }
  return { ready: status.ready, possible, summary, steps };
}
```

- [ ] **Step 4: Run it and see it pass**

Run: `bun test tests/export-kfx-setup.test.ts`
Expected: PASS, all tests.

- [ ] **Step 5: Typecheck and commit**

```bash
bunx tsc --noEmit
git add src/export/kfx-setup.ts tests/export-kfx-setup.test.ts
git commit -m "KFX checklist: the three steps, their fixes, and what a Kindle gets today

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The handlers (`src/cli-kfx.ts`)

**Files:**
- Create: `src/cli-kfx.ts`
- Modify: `src/cli-errors.ts` (the `JsonError['code']` union)
- Test: `tests/cli-kfx.test.ts` (handler half; Task 3 adds the spawned half)

- [ ] **Step 1: Write the failing test**

Create `tests/cli-kfx.test.ts`:

```ts
// `screepub kfx-status` and `screepub kfx-install`.
//
// The handlers are tested in process with a fake probe and a fake installer.
// NOTHING in this file may reach the real installer: it downloads a plugin
// and writes it into the Calibre of whatever machine runs the suite.
import { describe, test, expect } from 'bun:test';
import {
  kfxStatusCommand,
  kfxInstallCommand,
  setupLines,
  installLines,
} from '../src/cli-kfx';
import { CliError } from '../src/cli-errors';
import type { KfxInstallResult, KfxStatus } from '../src/export/kfx';

const missingPlugin: KfxStatus = { calibre: true, previewer: true, pluginInstalled: false, ready: false };
const allThere: KfxStatus = { calibre: true, previewer: true, pluginInstalled: true, ready: true };

describe('kfxStatusCommand', () => {
  test('answers the checklist for the probed status and the given platform', async () => {
    const setup = await kfxStatusCommand({ status: async () => missingPlugin, platform: 'darwin' });
    expect(setup.ready).toBe(false);
    expect(setup.possible).toBe(true);
    expect(setup.steps.find((s) => s.id === 'plugin')!.fix).toEqual({ kind: 'install', label: 'Install' });
  });
});

describe('kfxInstallCommand', () => {
  test('success reports the version, the removed forks, and a FRESH checklist', async () => {
    // The status is probed again after the install, so the answer the window
    // redraws from is the one Calibre now reports, not the one from before.
    const probes: KfxStatus[] = [allThere];
    let probed = 0;
    const answer = await kfxInstallCommand({
      install: async (): Promise<KfxInstallResult> => ({ ok: true, version: '2.20.1', removed: ['KFX Output (fork)'] }),
      status: async () => { probed += 1; return probes.shift()!; },
      platform: 'darwin',
    });
    expect(probed).toBe(1);
    expect(answer.version).toBe('2.20.1');
    expect(answer.removed).toEqual(['KFX Output (fork)']);
    expect(answer.setup.ready).toBe(true);
  });

  test('a missing removed list reads as empty, never undefined', async () => {
    const answer = await kfxInstallCommand({
      install: async () => ({ ok: true, version: '2.20.1' }),
      status: async () => allThere,
      platform: 'darwin',
    });
    expect(answer.removed).toEqual([]);
  });

  test('a failure is a kfx-install-failed CliError carrying Calibre’s reason', async () => {
    let probed = 0;
    const run = kfxInstallCommand({
      install: async () => ({ ok: false, reason: 'downloaded 10 bytes, index says 20' }),
      status: async () => { probed += 1; return missingPlugin; },
      platform: 'darwin',
    });
    await expect(run).rejects.toBeInstanceOf(CliError);
    try {
      await kfxInstallCommand({
        install: async () => ({ ok: false, reason: 'downloaded 10 bytes, index says 20' }),
        status: async () => missingPlugin,
        platform: 'darwin',
      });
    } catch (err) {
      const e = err as CliError;
      expect(e.code).toBe('kfx-install-failed');
      expect(e.message).toBe('could not install the KFX plugin: downloaded 10 bytes, index says 20');
    }
    // No point probing after a failure: nothing changed that the caller
    // does not already have.
    expect(probed).toBe(0);
  });

  test('ok without a version is still a failure: nothing can say what was installed', async () => {
    await expect(kfxInstallCommand({
      install: async () => ({ ok: true }),
      status: async () => allThere,
      platform: 'darwin',
    })).rejects.toBeInstanceOf(CliError);
  });

  test('a failure with no reason still carries a sentence', async () => {
    try {
      await kfxInstallCommand({ install: async () => ({ ok: false }), status: async () => missingPlugin, platform: 'darwin' });
      throw new Error('expected a throw');
    } catch (err) {
      expect((err as CliError).message).toBe(
        'could not install the KFX plugin: Calibre did not say why',
      );
    }
  });
});

describe('human output', () => {
  test('status: the summary, then one aligned line per step with its fix', async () => {
    const setup = await kfxStatusCommand({
      status: async () => ({ calibre: true, previewer: false, pluginInstalled: false, ready: false }),
      platform: 'darwin',
    });
    expect(setupLines(setup)).toEqual([
      'Kindles get AZW3 for now. KFX looks better, and needs the three free tools below.',
      '  Calibre           installed',
      '  Kindle Previewer  not installed: https://kdp.amazon.com/en_US/help/topic/G202131170',
      '  KFX plugin        not installed: run screepub kfx-install',
    ]);
  });

  test('status: an after or unavailable fix prints its reason', async () => {
    const setup = await kfxStatusCommand({
      status: async () => ({ calibre: false, previewer: false, pluginInstalled: false, ready: false }),
      platform: 'linux',
    });
    const lines = setupLines(setup);
    expect(lines[2]).toBe('  Kindle Previewer  not available: Amazon does not make it for Linux');
    expect(lines[3]).toBe('  KFX plugin        not available: Of no use without Kindle Previewer');
    const mac = await kfxStatusCommand({
      status: async () => ({ calibre: false, previewer: false, pluginInstalled: false, ready: false }),
      platform: 'darwin',
    });
    expect(setupLines(mac)[3]).toBe('  KFX plugin        not installed: Install Calibre first');
  });

  test('install: the version, each removed fork, then the summary', () => {
    expect(installLines({
      version: '2.20.1',
      removed: ['KFX Output (fork)'],
      setup: { ready: true, possible: true, summary: 'S.', steps: [] },
    })).toEqual([
      'installed the KFX plugin 2.20.1',
      'removed an older copy: KFX Output (fork)',
      'S.',
    ]);
  });

  test('no line carries an em dash', async () => {
    const setup = await kfxStatusCommand({ status: async () => missingPlugin, platform: 'darwin' });
    for (const line of [...setupLines(setup), ...installLines({ version: '1', removed: ['x'], setup })]) {
      expect(line.includes('—')).toBe(false);
    }
  });
});
```

- [ ] **Step 2: Run it and see it fail**

Run: `bun test tests/cli-kfx.test.ts`
Expected: FAIL, `Cannot find module '../src/cli-kfx'`.

- [ ] **Step 3: Add the error code**

In `src/cli-errors.ts`, extend the union after `'export-failed'`:

```ts
    | 'bad-settings'
    | 'export-failed'
    // The KFX plugin install (parity piece D). Same contract, same stdout rule.
    | 'kfx-install-failed';
```

- [ ] **Step 4: Write the handlers**

Create `src/cli-kfx.ts`:

```ts
// The `kfx-status` and `kfx-install` verbs. Handlers RETURN values and never
// print: cli.ts owns stdout, the same split as cli-export.ts. The checklist is
// src/export/kfx-setup.ts's and the installer is src/export/kfx.ts's; this
// file chooses nothing, it only carries their answers out.
import { platform as hostPlatform } from 'node:process';
import { CliError } from './cli-errors';
import { installKfxPlugin, kfxStatus, type KfxInstallResult, type KfxStatus } from './export/kfx';
import { kfxSetup, type KfxSetup, type KfxStep } from './export/kfx-setup';

/** Injectable seams, defaulted to the real thing, so a test can drive every
 *  outcome without a Calibre, and without ever installing into a real one. */
export interface KfxDeps {
  status?: () => Promise<KfxStatus>;
  install?: () => Promise<KfxInstallResult>;
  platform?: string;
}

export interface KfxInstallAnswer {
  version: string;
  /** Conflicting KFX forks cleared to make room (see installKfxPlugin). */
  removed: string[];
  /** The checklist as Calibre reports it AFTER the install. */
  setup: KfxSetup;
}

export async function kfxStatusCommand(deps: KfxDeps = {}): Promise<KfxSetup> {
  const status = await (deps.status ?? kfxStatus)();
  return kfxSetup(status, deps.platform ?? hostPlatform);
}

/** Install or update the plugin, then probe again so the answer carries the
 *  fresh checklist and the window needs no second call. NEVER call this on
 *  the program's own initiative: it fetches third-party code and writes into
 *  the user's Calibre, so only an explicit request reaches it. */
export async function kfxInstallCommand(deps: KfxDeps = {}): Promise<KfxInstallAnswer> {
  const result = await (deps.install ?? (() => installKfxPlugin()))();
  if (!result.ok || !result.version) {
    throw new CliError(
      'kfx-install-failed',
      `could not install the KFX plugin: ${result.reason ?? 'Calibre did not say why'}`,
    );
  }
  const setup = await kfxStatusCommand(deps);
  return { version: result.version, removed: result.removed ?? [], setup };
}

function stepState(step: KfxStep): string {
  const fix = step.fix;
  if (fix === null) return 'installed';
  if (fix.kind === 'link') return `not installed: ${fix.url}`;
  if (fix.kind === 'install') return 'not installed: run screepub kfx-install';
  if (fix.kind === 'unavailable') return `not available: ${fix.why}`;
  return `not installed: ${fix.why}`;
}

/** `kfx-status` for a person: the summary, then one aligned line per step. */
export function setupLines(setup: KfxSetup): string[] {
  const width = Math.max(0, ...setup.steps.map((s) => s.name.length));
  return [setup.summary, ...setup.steps.map((s) => `  ${s.name.padEnd(width)}  ${stepState(s)}`)];
}

/** `kfx-install` for a person. */
export function installLines(answer: KfxInstallAnswer): string[] {
  return [
    `installed the KFX plugin ${answer.version}`,
    ...answer.removed.map((name) => `removed an older copy: ${name}`),
    answer.setup.summary,
  ];
}
```

- [ ] **Step 5: Run it and see it pass**

Run: `bun test tests/cli-kfx.test.ts`
Expected: PASS.

- [ ] **Step 6: Typecheck and commit**

```bash
bunx tsc --noEmit
git add src/cli-kfx.ts src/cli-errors.ts tests/cli-kfx.test.ts
git commit -m "kfx-status and kfx-install handlers, with a fresh checklist after an install

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The verbs in the CLI

**Files:**
- Modify: `src/cli-devices.ts` (`VERBS`)
- Modify: `src/cli.ts` (import, `USAGE`, two usage constants, `verbUsage`, the verb branch in `runVerb`)
- Modify: `tests/cli-devices.test.ts:74-88` (the `VERBS` pin)
- Test: `tests/cli-kfx.test.ts` (append the spawned half)

- [ ] **Step 1: Update the VERBS pin (failing)**

In `tests/cli-devices.test.ts`, the test `'VERBS is the single list of known verbs'`: extend the comment and the list.

```ts
  // The two update verbs joined on 2026-09-21; both are hyphenated on
  // purpose, since a file called `update-decision` is far less likely to
  // exist than one called `update`. The two KFX verbs joined on 2026-09-23
  // for the same reason.
  expect([...VERBS]).toEqual([
    'devices',
    'send',
    'settings',
    'export',
    'update-decision',
    'update-should-check',
    'kfx-status',
    'kfx-install',
  ]);
```

- [ ] **Step 2: Append the spawned tests to `tests/cli-kfx.test.ts` (failing)**

Add these imports at the top of the file (merge with the existing import lines):

```ts
import { afterAll } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveCommand, VERBS } from '../src/cli-devices';
```

Then, at top level below the imports:

```ts
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-kfx-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const ROOT = new URL('..', import.meta.url).pathname;

/** Spawn the real CLI. CALIBRE_CONFIG_DIRECTORY points Calibre at a
 *  throwaway config inside SCRATCH, so if a refusal below ever regressed and
 *  let `kfx-install` through, it would install into that folder and not into
 *  the Calibre of the machine running the suite. Verified 2026-09-23: with
 *  this set, kfxStatus() reads the scratch config and the real one is not
 *  touched. */
async function runCli(args: string[]) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
    env: { ...process.env, CALIBRE_CONFIG_DIRECTORY: join(SCRATCH, 'calibre-config') },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}
```

And the tests:

```ts
describe('the verbs', () => {
  test('both are verbs, and a file of either name still converts', () => {
    expect(VERBS as readonly string[]).toContain('kfx-status');
    expect(VERBS as readonly string[]).toContain('kfx-install');
    expect(resolveCommand(['kfx-install'], () => false)).toEqual({ kind: 'verb', verb: 'kfx-install', args: [] });
    expect(resolveCommand(['kfx-install'], () => true)).toEqual({ kind: 'convert' });
  });

  test('kfx-status --json is one object with the checklist', async () => {
    // Read-only: it probes and prints. Whatever this machine has, the shape
    // is the contract.
    const { stdout, exitCode } = await runCli(['kfx-status', '--json']);
    expect(exitCode).toBe(0);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    expect(typeof answer.ready).toBe('boolean');
    expect(typeof answer.possible).toBe('boolean');
    expect(typeof answer.summary).toBe('string');
    expect(answer.steps.map((s: { id: string }) => s.id)).toEqual(['calibre', 'previewer', 'plugin']);
  });

  test('kfx-status prints the summary for a person', async () => {
    const { stdout, exitCode } = await runCli(['kfx-status']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('Kindles get');
    expect(stdout).toContain('KFX plugin');
  });

  test('each verb has its own --help, naming itself and not the conversion flags', async () => {
    for (const verb of ['kfx-status', 'kfx-install']) {
      const { stdout, exitCode } = await runCli([verb, '--json', '--help']);
      expect(exitCode).toBe(0);
      const { usage } = JSON.parse(stdout);
      expect(usage).toContain(`screepub ${verb}`);
      expect(usage).not.toContain('--mobi');
    }
  });

  test('the main usage lists both', async () => {
    const { stdout } = await runCli(['--help']);
    expect(stdout).toContain('screepub kfx-status');
    expect(stdout).toContain('screepub kfx-install');
  });

  // Refused BEFORE anything runs. For kfx-install that order is the whole
  // point: a mistyped command must not reach the network or anyone's Calibre.
  const FOREIGN: [string[], string][] = [
    [['--device', 'x'], '--device'],
    [['--set', '{}'], '--set'],
    [['--for', 'kindle'], '--for'],
    [['--fountain', '/x.fountain'], '--fountain'],
    [['--options-json', '{}'], '--options-json'],
    [['--offered', '1.0'], '--offered'],
    [['--opted-in'], '--opted-in'],
  ];

  for (const verb of ['kfx-status', 'kfx-install']) {
    test(`${verb} refuses every other verb's flags as usage errors`, async () => {
      for (const [flags, name] of FOREIGN) {
        const { stdout, exitCode } = await runCli([verb, ...flags, '--json']);
        const answer = JSON.parse(stdout);
        expect(`${verb} ${name}: ${exitCode} ${answer.ok} ${answer.error?.code}`)
          .toBe(`${verb} ${name}: 1 false usage`);
        expect(answer.error.message).toContain(name);
      }
    });

    test(`${verb} takes no arguments`, async () => {
      const { stdout, exitCode } = await runCli([verb, 'extra', '--json']);
      expect(exitCode).toBe(1);
      const answer = JSON.parse(stdout);
      expect(answer.error.code).toBe('usage');
      expect(answer.error.message).toBe(`${verb} takes no arguments (got "extra")`);
    });
  }
});

describe('kfx-install refuses before it installs', () => {
  test('in the source, every refusal comes before the installer is called', async () => {
    // The spawned refusals above prove each refusal FIRES. This proves the
    // ORDER without ever letting the installer run: in cli.ts's kfx branch,
    // the last refusal is written above the first call to kfxInstallCommand.
    const source = await Bun.file(`${ROOT}src/cli.ts`).text();
    const branch = source.slice(source.indexOf("verb === 'kfx-status' || verb === 'kfx-install'"));
    const installed = branch.indexOf('kfxInstallCommand(');
    const lastRefusal = branch.lastIndexOf("fail({ code: 'usage'", installed);
    expect(installed).toBeGreaterThan(-1);
    expect(lastRefusal).toBeGreaterThan(-1);
    expect(lastRefusal).toBeLessThan(installed);
  });
});
```

Run: `bun test tests/cli-kfx.test.ts tests/cli-devices.test.ts`
Expected: FAIL (VERBS pin, spawned tests: `kfx-status` is not a verb yet so it tries to convert a file called `kfx-status`).

- [ ] **Step 3: Add the verbs**

In `src/cli-devices.ts`:

```ts
export const VERBS = [
  'devices',
  'send',
  'settings',
  'export',
  'update-decision',
  'update-should-check',
  'kfx-status',
  'kfx-install',
] as const;
```

- [ ] **Step 4: Wire them into `src/cli.ts`**

Import, beside `import { exportCommand } from './cli-export';`:

```ts
import { kfxInstallCommand, kfxStatusCommand, installLines, setupLines } from './cli-kfx';
```

In `USAGE`, add after the `screepub export ...` entry (two lines each, matching the column):

```
  screepub kfx-status [--json]              can this computer make KFX for a Kindle?
  screepub kfx-install [--json]             install the KFX plugin into Calibre (online)
```

Add two usage constants after `EXPORT_USAGE`:

```ts
const KFX_STATUS_USAGE = `screepub kfx-status: can this computer make KFX for a Kindle?

Usage:
  screepub kfx-status [--json]

A Kindle gets its best rendering from a KFX file, and making one needs three
free tools: Calibre, Amazon's Kindle Previewer, and the KFX Output plugin
inside Calibre. This says which are installed, where to get the missing ones,
and what a Kindle gets until then. Reads only; works offline.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const KFX_INSTALL_USAGE = `screepub kfx-install: install the KFX plugin into Calibre

Usage:
  screepub kfx-install [--json]

Downloads the current KFX Output plugin from Calibre's own plugin index and
installs it with Calibre's own installer, replacing any older copy. Needs
Calibre, and the internet. Kindle Previewer is not installed by this: it is
Amazon's, and \`screepub kfx-status\` says where to get it.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;
```

In `verbUsage`, before the final `return SEND_USAGE;`:

```ts
  if (verb === 'kfx-status') return KFX_STATUS_USAGE;
  if (verb === 'kfx-install') return KFX_INSTALL_USAGE;
```

In `runVerb`, add this branch directly BEFORE `if (verb === 'settings') {`. (The block above it already refuses the update verbs' flags for every verb that is not an update verb, so `--offered`, `--current`, `--last-checked` and `--opted-in` are covered.)

```ts
    if (verb === 'kfx-status' || verb === 'kfx-install') {
      // Every refusal comes BEFORE anything runs. For kfx-install that order
      // is the point: a mistyped command must not reach the network or the
      // user's Calibre. tests/cli-kfx.test.ts pins the order in this source.
      const foreign: [unknown, string, string][] = [
        [values.device, '--device', 'send'],
        [values.set, '--set', 'settings'],
        [values.for, '--for', 'export'],
        [values.fountain, '--fountain', 'export'],
        [values['options-json'], '--options-json', 'export'],
      ];
      for (const [value, flag, owner] of foreign) {
        if (value !== undefined) {
          fail({ code: 'usage', message: `${verb} takes no ${flag} (${flag} belongs to ${owner})` });
        }
      }
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `${verb} takes no arguments (got "${positionals[0]}")` });
      }

      if (verb === 'kfx-status') {
        const setup = await kfxStatusCommand();
        if (jsonMode) {
          console.log(JSON.stringify({ ok: true, ...setup }));
          return;
        }
        for (const line of setupLines(setup)) console.log(line);
        return;
      }

      // A person at a terminal waits several seconds for a download; say so
      // on stderr, where it cannot disturb the one JSON line on stdout.
      if (!jsonMode) console.error("installing the KFX plugin from Calibre's plugin index...");
      const installed = await kfxInstallCommand();
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...installed }));
        return;
      }
      for (const line of installLines(installed)) console.log(line);
      return;
    }
```

(`kfxInstallCommand` throws `CliError('kfx-install-failed', ...)` on failure; the existing `catch (err) { if (err instanceof CliError) fail(err.toJson()); ... }` at the end of `runVerb` turns it into the one JSON error object.)

- [ ] **Step 5: Run and see it pass**

Run: `bun test tests/cli-kfx.test.ts tests/cli-devices.test.ts tests/cli-update.test.ts tests/cli-device-commands.test.ts`
Expected: PASS.

- [ ] **Step 6: Check the real thing by hand, safely**

```bash
bun src/cli.ts kfx-status
bun src/cli.ts kfx-status --json
bun src/cli.ts kfx-install --device x --json   # must refuse, exit 1
```

Do NOT run `kfx-install` without `CALIBRE_CONFIG_DIRECTORY` pointed at a scratch folder. The controller does the one real end-to-end install at the end of the plan.

- [ ] **Step 7: Typecheck and commit**

```bash
bunx tsc --noEmit
git add src/cli.ts src/cli-devices.ts tests/cli-kfx.test.ts tests/cli-devices.test.ts
git commit -m "screepub kfx-status and kfx-install: the KFX installer is reachable from the CLI

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The permission

**Files:**
- Modify: `desktop/src-tauri/capabilities/default.json`
- Modify: `tests/desktop-shell.test.ts` (ONLY the test `'the opener may reach this project’s issue tracker and nothing else'`)
- Modify: `docs/adr/2026-09-21-doors-not-commands.md`
- Modify: `desktop/README.md` (the capability note near line 134)

- [ ] **Step 1: Replace the opener test (failing)**

In `tests/desktop-shell.test.ts`, replace the whole test `'the opener may reach this project’s issue tracker and nothing else'` with:

```ts
  test('the opener may reach the issue tracker and the KFX download pages, and nothing else', async () => {
    const { KFX_LINKS } = await import('../src/export/kfx-setup');
    const opener = capability().permissions.find(
      (p: unknown) => typeof p === 'object' && p !== null
        && (p as { identifier: string }).identifier === 'opener:allow-open-url',
    );
    expect(opener).toBeDefined();
    // The repository is a glob, so it must not widen past the repository:
    // "https://*" or a bare "https://github.com/*" would let any page on the
    // host be opened from whatever text the window happened to be holding.
    // The KFX pages are EXACT strings. tauri-plugin-opener 2.5.5 matches the
    // raw URL with glob::Pattern (src/scope.rs), where `*` also matches `/`,
    // so an exact string is the only grant that means one page.
    const exact = new Set<string>(KFX_LINKS);
    for (const entry of opener.allow) {
      const ok = entry.url.startsWith('https://github.com/method-production-suite/method-reader/') || exact.has(entry.url);
      expect(`${entry.url} allowed: ${ok}`).toBe(`${entry.url} allowed: true`);
    }
    // And the other way round: every link the engine can put on the Send
    // page is granted, character for character. Without this half, a new
    // link in src/export/kfx-setup.ts would draw a button Tauri refuses.
    const granted = new Set(opener.allow.map((e: { url: string }) => e.url));
    for (const url of KFX_LINKS) {
      expect(`${url} granted: ${granted.has(url)}`).toBe(`${url} granted: true`);
    }
  });
```

Run: `bun test tests/desktop-shell.test.ts`
Expected: FAIL, `... granted: false` for the three KFX URLs.

- [ ] **Step 2: Grant them**

In `desktop/src-tauri/capabilities/default.json`, the `opener:allow-open-url` entry becomes:

```json
    {
      "identifier": "opener:allow-open-url",
      "allow": [
        { "url": "https://github.com/method-production-suite/method-reader/*" },
        { "url": "https://calibre-ebook.com/download_osx" },
        { "url": "https://calibre-ebook.com/download_windows" },
        { "url": "https://kdp.amazon.com/en_US/help/topic/G202131170" }
      ]
    },
```

And in `"description"`, change `the window may ask the OS to open the project's issue tracker,` to `the window may ask the OS to open the project's issue tracker and the three Calibre and Kindle Previewer download pages the KFX checklist links to,`. Change nothing else in the file.

- [ ] **Step 3: Run and see it pass**

Run: `bun test tests/desktop-shell.test.ts`
Expected: PASS (including `'every plugin permission is scoped, never a bare grant'`, which the edit does not affect).

- [ ] **Step 4: Record the new doors**

In `docs/adr/2026-09-21-doors-not-commands.md`, directly after the paragraph that ends `Write the scopes with the permissions, not afterwards.`, add:

```markdown
**Amended 2026-09-23 (parity piece D).** `opener:allow-open-url` gained
three exact URLs: Calibre's macOS and Windows download pages and Amazon's
Kindle Previewer page, so the Send page's KFX checklist can link to what it
says is missing. Approved by the owner the same day. Same test as every
grant here: a door, not an opinion. What to link is decided by
`src/export/kfx-setup.ts`, and `tests/desktop-shell.test.ts` holds that
file's links and this grant to the same three strings.
```

In `desktop/README.md`, at the end of the quoted note that begins `> Written when the capability was exactly \`core:default\`.` (after the paragraph ending `Still two commands.`), add:

```markdown
>
> On 2026-09-23 `allow-open-url` gained three exact URLs, Calibre's macOS
> and Windows download pages and Amazon's Kindle Previewer page, for the
> KFX checklist on the Send page (parity piece D). Exact strings and no
> wildcard: the opener matches the raw URL as a glob, so a `*` would also
> match `/`. Still two commands.
```

- [ ] **Step 5: Commit**

```bash
git add desktop/src-tauri/capabilities/default.json tests/desktop-shell.test.ts docs/adr/2026-09-21-doors-not-commands.md desktop/README.md
git commit -m "Grant the three KFX download pages to the opener, exactly, held to the engine's links

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The window's decisions (`app.js` argv + `kfx.js` above the line)

**Files:**
- Modify: `desktop/ui/app.js` (the `argv` object only)
- Create: `desktop/ui/kfx.js` (decisions half; Task 6 adds drawing)
- Modify: `tests/desktop-ui.test.ts` (the argv list test near line 203; a new `describe` for kfx.js decisions)
- Modify: `tests/capture.test.ts` (two REFUSALS entries)

- [ ] **Step 1: Pin the new argv builders (failing)**

In `tests/desktop-ui.test.ts`, test `'every argv builder passes --json, and every one of them is checked'`: add to `built`:

```ts
      kfxStatus: argv.kfxStatus(),
      kfxInstall: argv.kfxInstall(),
```

and change the key list to:

```ts
    expect(Object.keys(argv).sort()).toEqual(
      ['convert', 'devices', 'export', 'kfxInstall', 'kfxStatus', 'reconvert', 'send', 'settings', 'version'].sort(),
    );
```

Add a test right after it:

```ts
  test('the KFX builders are exactly the two verbs, with nothing else on them', async () => {
    const { argv } = await import(join(UI, 'app.js'));
    expect(argv.kfxStatus()).toEqual(['kfx-status', '--json']);
    expect(argv.kfxInstall()).toEqual(['kfx-install', '--json']);
  });
```

In `tests/capture.test.ts`, add to the `REFUSALS` list (next to the `update-should-check` entry). Import `argv` is already in scope there (check the top of the file; if the file builds calls from `argv`, use it; otherwise write the arrays literally):

```ts
    ['kfx-status: the Send page is never captured', ['kfx-status', '--json']],
    ['kfx-install: a capture must never install anything', ['kfx-install', '--json']],
```

- [ ] **Step 2: Write the kfx.js decision tests (failing)**

Append to `tests/desktop-ui.test.ts`:

```ts
describe('the Send page’s KFX block: decisions', () => {
  type Setup = {
    ready: boolean; possible: boolean; summary: string;
    steps: { id: string; name: string; installed: boolean; fix: unknown }[];
  };
  type KfxModule = {
    HEADING: string; INSTALLED: string; INSTALLING: string; NO_REASON: string;
    checklistFrom: (value: unknown) => Setup | null;
    setupFrom: (answer: unknown) => Setup | null;
    kindleRelevant: (devices: unknown) => boolean;
    showSetup: (setup: Setup | null, devices: unknown, justInstalled: boolean) => boolean;
    controlFor: (step: Setup['steps'][number], busy: boolean) =>
      | { type: 'status'; text: string }
      | { type: 'link'; label: string; url: string }
      | { type: 'install'; label: string; disabled: boolean };
    installedLine: (answer: unknown) => string;
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
    expect(kfx.showSetup(notReady, null, false)).toBe(false);
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

  test('success names the version, whether Kindles now get KFX, and any fork removed', () => {
    expect(kfx.installedLine({ ok: true, version: '2.20.1', removed: [], setup: ready }))
      .toBe('Installed the KFX plugin 2.20.1. Kindles now get KFX.');
    expect(kfx.installedLine({ ok: true, version: '2.20.1', removed: [], setup: notReady }))
      .toBe('Installed the KFX plugin 2.20.1.');
    expect(kfx.installedLine({ ok: true, version: '2.20.1', removed: ['KFX Output (fork)', 'Old KFX'], setup: ready }))
      .toBe('Installed the KFX plugin 2.20.1. Kindles now get KFX. Removed an older copy: KFX Output (fork), Old KFX.');
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

  test('afterInstall: a refusal keeps the checklist and shows the reason in alarm', () => {
    const out = kfx.afterInstall({ ok: false, error: { code: 'kfx-install-failed', message: 'nope' } }, notReady);
    expect(out).toEqual({ setup: notReady, line: 'nope', bad: true, justInstalled: false });
  });

  test('no copy in kfx.js carries an em dash', () => {
    expect(read('kfx.js').includes('—')).toBe(false);
  });

  test('kfx.js holds no engine flag and no Tauri call of its own', () => {
    const source = read('kfx.js');
    expect(source).not.toContain("'--json'");
    expect(source).not.toContain('__TAURI__');
    expect(source).not.toContain("'kfx-install'");
    expect(source).not.toContain("'kfx-status'");
  });
});
```

Run: `bun test tests/desktop-ui.test.ts tests/capture.test.ts`
Expected: FAIL (`argv.kfxStatus is not a function`; `kfx.js` missing). The capture REFUSALS entries should already pass (the gate refuses unknown calls); that is fine, they pin it.

- [ ] **Step 3: Add the argv builders**

In `desktop/ui/app.js`, inside `export const argv = { ... }`, after `export: ...`:

```js
  /** Can this computer make KFX for a Kindle? Reads only. */
  kfxStatus: () => ['kfx-status', '--json'],

  /** Install the KFX plugin into Calibre. Fetches it from Calibre's plugin
   *  index and writes into the user's Calibre, so it is only ever built in
   *  answer to a press of the button that says so (kfx.js). */
  kfxInstall: () => ['kfx-install', '--json'],
```

- [ ] **Step 4: Write `desktop/ui/kfx.js`, decisions half**

Create `desktop/ui/kfx.js`:

```js
// The Send page's "Best Kindle quality" block: can this computer make KFX,
// and if not, the one thing to press for each missing piece.
//
// The checklist is the ENGINE's (src/export/kfx-setup.ts, via
// `screepub kfx-status`): which of Calibre, Kindle Previewer and the KFX
// plugin are present, what each missing one needs, and what a Kindle gets
// today. This file draws that answer and never re-decides it. It adds only
// what is about the window: when the block is worth showing, what a button
// says while it works, and what to say after.
//
// Two halves, like send.js: pure exported decisions above the line, tested
// directly by tests/desktop-ui.test.ts, and drawing below it.
import { runEngine, argv, openUrl } from './app.js';
import { el, clear, text } from './dom.js';

// ---------------------------------------------------------------- decisions

export const HEADING = 'Best Kindle quality';
export const INSTALLED = 'Installed';
export const INSTALLING = 'Installing the KFX plugin from Calibre’s plugin index. This takes a few seconds.';
export const NO_REASON = 'The KFX plugin was not installed, and nothing said why.';

const KINDS = new Set(['link', 'install', 'after', 'unavailable']);
const IDS = ['calibre', 'previewer', 'plugin'];

const isText = (value) => typeof value === 'string' && value.trim() !== '';

function fixFrom(fix) {
  if (fix === null) return null;
  if (typeof fix !== 'object' || Array.isArray(fix) || !KINDS.has(fix.kind)) return undefined;
  if (fix.kind === 'link') return isText(fix.label) && isText(fix.url) ? fix : undefined;
  if (fix.kind === 'install') return isText(fix.label) ? fix : undefined;
  return isText(fix.why) ? fix : undefined;
}

function stepFrom(step, id) {
  if (typeof step !== 'object' || step === null || step.id !== id) return null;
  if (!isText(step.name) || typeof step.installed !== 'boolean') return null;
  const fix = fixFrom(step.fix);
  if (fix === undefined) return null;
  // The engine's rule: a fix exactly when the step is not installed.
  if ((fix === null) !== step.installed) return null;
  return { id, name: step.name, installed: step.installed, fix };
}

/** The checklist, whole, or null. A broken contract draws NOTHING: this
 *  block is advice, and "the probe answered strangely" is not something a
 *  reader can act on. */
export function checklistFrom(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  if (typeof value.ready !== 'boolean' || typeof value.possible !== 'boolean') return null;
  if (!isText(value.summary) || !Array.isArray(value.steps) || value.steps.length !== IDS.length) return null;
  const steps = IDS.map((id, i) => stepFrom(value.steps[i], id));
  if (steps.some((step) => step === null)) return null;
  return { ready: value.ready, possible: value.possible, summary: value.summary, steps };
}

/** `kfx-status --json`'s answer as a checklist, or null. */
export function setupFrom(answer) {
  return answer?.ok === true ? checklistFrom(answer) : null;
}

/** Kindle advice is for Kindles. Shown with nothing connected (the reader
 *  is deciding) or with a Kindle connected; hidden when only other readers
 *  are. An unknown list (the first poll has not answered) is not relevant
 *  YET, so the block does not flash up and vanish a moment later. */
export function kindleRelevant(devices) {
  if (!Array.isArray(devices)) return false;
  return devices.length === 0 || devices.some((device) => device?.kind === 'kindle');
}

/** Whether the block is drawn at all. A ready machine sees nothing, except
 *  right after an install, when the success line needs somewhere to stand. */
export function showSetup(setup, devices, justInstalled) {
  if (setup === null || setup === undefined || !setup.possible) return false;
  if (!kindleRelevant(devices)) return false;
  return !setup.ready || justInstalled === true;
}

/** What stands to the right of one step. `busy` is true while a send or an
 *  install is running: the plugin must not be swapped under a running KFX
 *  conversion. Opening a web page is harmless at any time. */
export function controlFor(step, busy) {
  const fix = step.fix;
  if (fix === null) return { type: 'status', text: INSTALLED };
  if (fix.kind === 'link') return { type: 'link', label: fix.label, url: fix.url };
  if (fix.kind === 'install') return { type: 'install', label: fix.label, disabled: busy === true };
  return { type: 'status', text: fix.why };
}

/** What the status line says after a successful install. Removed forks are
 *  named: Screepub took out something the reader installed (usually the
 *  Swift app's copy), and that deserves saying. */
export function installedLine(answer) {
  const parts = [`Installed the KFX plugin ${answer?.version}.`];
  if (answer?.setup?.ready === true) parts.push('Kindles now get KFX.');
  const removed = Array.isArray(answer?.removed) ? answer.removed.filter(isText) : [];
  if (removed.length > 0) parts.push(`Removed an older copy: ${removed.join(', ')}.`);
  return parts.join(' ');
}

/** The engine's own sentence for a refusal, or a stand-in. */
export function failedLine(answer) {
  const said = answer?.error?.message;
  return isText(said) ? said.trim() : NO_REASON;
}

/** When the OS will not open a link, the reader can still get there by hand. */
export function linkFailedLine(url) {
  return `Could not open the page. It is at ${url}`;
}

/** Everything an install's answer changes, in one place: the checklist to
 *  draw, the line to say, whether it is an alarm, and whether the block
 *  stays up on a now-ready machine. A success whose checklist is broken keeps
 *  the old one rather than blanking the block under the success line. */
export function afterInstall(answer, previous) {
  if (answer?.ok === true && isText(answer.version)) {
    return {
      setup: checklistFrom(answer.setup) ?? previous,
      line: installedLine(answer),
      bad: false,
      justInstalled: true,
    };
  }
  return { setup: previous, line: failedLine(answer), bad: true, justInstalled: false };
}
```

- [ ] **Step 5: Run and see it pass**

Run: `bun test tests/desktop-ui.test.ts tests/capture.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add desktop/ui/app.js desktop/ui/kfx.js tests/desktop-ui.test.ts tests/capture.test.ts
git commit -m "Window: the KFX argv builders and the Send page block's decisions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The window's drawing, and the Send page wiring

**Files:**
- Modify: `desktop/ui/kfx.js` (append the drawing half)
- Modify: `desktop/ui/send.js` (import, `draw()`, `show()`, `hide()`, `refresh()`, `sendTo()`)
- Modify: `tests/desktop-ui.test.ts` (shape tests)
- Maybe modify: `desktop/ui/surfaces.css` (only if a rule is genuinely needed; NOT `style.css`)

- [ ] **Step 1: Write the shape tests (failing)**

Append to `tests/desktop-ui.test.ts`:

```ts
describe('the Send page’s KFX block: wiring', () => {
  const kfx = read('kfx.js');
  const send = read('send.js');

  test('the installer is reached only from the button, never on the page’s own initiative', () => {
    // It downloads third-party code and writes into the reader's Calibre.
    expect(kfx.match(/argv\.kfxInstall\(\)/g)?.length).toBe(1);
    const install = kfx.slice(kfx.indexOf('async function install('));
    expect(install.slice(0, install.indexOf('\n}'))).toContain('argv.kfxInstall()');
    // `install` is handed to a click and never called directly. The
    // lookbehind skips its own definition, `async function install()`.
    expect(kfx).toContain('onclick: install');
    expect(kfx.match(/(?<!function )\binstall\(\)/g)).toBe(null);
  });

  test('one probe at a time, and the focus listener goes when the page does', () => {
    expect(kfx).toContain('argv.kfxStatus()');
    const probe = kfx.slice(kfx.indexOf('async function probe('));
    expect(probe.slice(0, 200)).toMatch(/if \(probing/);
    const hidden = /export function kfxHidden\(\) \{([\s\S]*?)\n\}/.exec(kfx);
    expect(hidden, 'kfx.js exports no kfxHidden()').not.toBe(null);
    expect(hidden![1]).toContain("removeEventListener('focus'");
  });

  test('send.js mounts the block and tells it when the page comes and goes', () => {
    expect(send).toContain("from './kfx.js'");
    expect(send).toMatch(/mountKfx\(/);
    const show = /export function show\(\) \{([\s\S]*?)\n\}/.exec(send);
    expect(show![1]).toContain('kfxShown()');
    const hide = /export function hide\(\) \{([\s\S]*?)\n\}/.exec(send);
    expect(hide![1]).toContain('kfxHidden()');
  });

  test('a send and an install never overlap', () => {
    // A plugin swapped out under a running KFX conversion is not a case
    // worth finding out about.
    const sendTo = send.slice(send.indexOf('async function sendTo('));
    expect(sendTo.slice(0, 200)).toMatch(/if \(sending \|\| kfxInstalling\(\)\) return;/);
    const install = kfx.slice(kfx.indexOf('async function install('));
    expect(install.slice(0, 300)).toMatch(/hooks\?\.isSending\?\.\(\)/);
  });
});
```

Run: `bun test tests/desktop-ui.test.ts`
Expected: FAIL on the new describe block.

- [ ] **Step 2: Append the drawing half to `desktop/ui/kfx.js`**

```js
// ------------------------------------------------------------------ drawing

/** The node the block lives in, and what send.js lends it. */
let host = null;
/** { isSending(), devices(), onBusy(busy) } */
let hooks = null;
/** The last checklist the engine gave. About the MACHINE, not the script,
 *  so it survives send.js redrawing the page for a new script. */
let setup = null;
let justInstalled = false;
let installingNow = false;
let probing = false;
let statusNode = null;
let status = { line: '', bad: false };

/** Draw into `node` from now on. Called from send.js's draw(), which
 *  rebuilds the page whenever the script changes. */
export function mountKfx(node, options) {
  host = node;
  hooks = options;
  draw();
}

/** The Send page came into view: ask the engine, and ask again whenever the
 *  window gets the focus back (the reader went to install Calibre and came
 *  back). */
export function kfxShown() {
  justInstalled = false;
  status = { line: '', bad: false };
  window.addEventListener('focus', onFocus);
  probe();
}

export function kfxHidden() {
  window.removeEventListener('focus', onFocus);
}

/** send.js's device list changed; the block may now matter, or not. */
export function kfxDevicesChanged() {
  draw();
}

/** A send started or finished; the Install button follows. */
export function kfxRedraw() {
  draw();
}

/** True while an install runs, so send.js can refuse to start a send. */
export function kfxInstalling() {
  return installingNow;
}

function onFocus() {
  probe();
}

async function probe() {
  if (probing || installingNow) return;
  probing = true;
  try {
    setup = setupFrom(await runEngine(argv.kfxStatus()));
  } catch {
    // Advice, not a feature: a probe that failed draws nothing.
    setup = null;
  } finally {
    probing = false;
  }
  draw();
}

function busy() {
  return installingNow || hooks?.isSending?.() === true;
}

function draw() {
  if (host === null || !host.isConnected) return;
  clear(host);
  statusNode = null;
  if (!showSetup(setup, hooks?.devices?.() ?? null, justInstalled)) {
    host.hidden = true;
    return;
  }
  host.hidden = false;
  statusNode = el('p', { class: 'caption send-status', role: 'status' }, '');
  host.append(
    el('p', { class: 'state-label' }, HEADING),
    el('p', { class: 'prose' }, setup.summary),
    el('div', { class: 'devices' }, ...setup.steps.map(stepRow)),
    statusNode,
  );
  say(status);
}

function stepRow(step) {
  return el('div', { class: 'device-row' },
    el('div', { class: 'device-what' }, el('p', { class: 'device-name' }, step.name)),
    control(controlFor(step, busy())),
  );
}

function control(c) {
  if (c.type === 'link') {
    return el('button', {
      type: 'button', class: 'btn btn-outline', onclick: () => follow(c.url),
    }, c.label);
  }
  if (c.type === 'install') {
    return el('button', {
      type: 'button', class: 'btn btn-brad', disabled: c.disabled, onclick: install,
    }, installingNow ? 'Installing…' : c.label);
  }
  return el('p', { class: 'reader-status' }, c.text);
}

async function follow(url) {
  if (await openUrl(url)) return;
  status = { line: linkFailedLine(url), bad: true };
  say(status);
}

async function install() {
  if (installingNow || hooks?.isSending?.() === true) return;
  installingNow = true;
  hooks?.onBusy?.(true);
  status = { line: INSTALLING, bad: false };
  draw();
  let outcome;
  try {
    outcome = afterInstall(await runEngine(argv.kfxInstall()), setup);
  } catch (err) {
    // runEngine throws only when the engine could not run or broke its
    // contract; its message is already written for a person.
    outcome = afterInstall({ ok: false, error: { message: err?.message } }, setup);
  } finally {
    installingNow = false;
    hooks?.onBusy?.(false);
  }
  setup = outcome.setup;
  justInstalled = outcome.justInstalled;
  status = { line: outcome.line, bad: outcome.bad };
  draw();
}

function say(next) {
  if (statusNode === null) return;
  text(statusNode, next.line);
  statusNode.classList.toggle('bad', next.bad);
}
```

- [ ] **Step 3: Wire it into `desktop/ui/send.js`**

Import, after the existing imports:

```js
import { mountKfx, kfxShown, kfxHidden, kfxDevicesChanged, kfxRedraw, kfxInstalling } from './kfx.js';
```

In `draw()`, where the reader-list page is built, create the node and append it LAST, after `artifactNote`, then mount (mount after append, because the block only draws into a node that is in the document):

```js
  const kfxNode = el('section', { class: 'kfx-setup', 'aria-label': 'Best Kindle quality' });
  kfxNode.hidden = true;

  pane.append(
    el('h2', { class: 'slug' }, 'Send to a reader'),
    el('p', { class: 'prose' }, LEDE),
    list,
    statusLine,
    artifactNote,
    kfxNode,
  );
  mountKfx(kfxNode, {
    isSending: () => sending,
    devices: () => drawn,
    onBusy: (on) => { for (const button of buttons()) button.disabled = on; },
  });
```

In `show()`, after `refresh();`:

```js
  kfxShown();
```

In `hide()`, after `poll = null;`:

```js
  kfxHidden();
```

In `refresh()`, right after `drawn = devices;`:

```js
  kfxDevicesChanged();
```

In `sendTo()`, change the first line and tell the block when a send starts and ends:

```js
async function sendTo(device) {
  if (sending || kfxInstalling()) return;
  sending = true;
  kfxRedraw();
```

and in its `finally`:

```js
  } finally {
    sending = false;
    for (const button of buttons()) button.disabled = false;
    kfxRedraw();
  }
```

Do NOT add any `await` to `sendTo`: `tests/desktop-ui.test.ts` counts its await boundaries (3) and its staleness checks (3).

- [ ] **Step 4: Run the UI suites**

Run: `bun test tests/desktop-ui.test.ts tests/desktop-shell.test.ts`
Expected: PASS. If a pre-existing send.js shape test fails because of the new lines, read what it guards and fix the wiring, not the test.

- [ ] **Step 5: Spacing, only if needed**

`.devices` gives the step list its top margin already. If the block sits flush against the status lines above it, add ONE rule to `desktop/ui/surfaces.css` beside the Send rules (after `.send-artifact`):

```css
/* The KFX checklist, under the send status: its own section, set apart. */
.kfx-setup { margin-top: var(--space-6); }
```

No hex, no raw px font sizes (both are tested). Do not touch `style.css`.

- [ ] **Step 6: Commit**

```bash
git add desktop/ui/kfx.js desktop/ui/send.js tests/desktop-ui.test.ts desktop/ui/surfaces.css
git commit -m "Send page: the Best Kindle quality checklist, with Get and Install buttons

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Docs, and the whole suite

**Files:**
- Modify: `docs/parity-audit.md`
- Modify: `docs/superpowers/plans/2026-09-21-parity.md`
- Modify: `README.md` (the "Device commands" section)

- [ ] **Step 1: The audit row**

In `docs/parity-audit.md`, the table row `| **KFX plugin install** | button | 0 | **written but unreachable** |` becomes:

```markdown
| **KFX plugin install** | button | 0 | **done 2026-09-23** (piece D: `kfx-status`, `kfx-install`, Send page) |
```

and add after the paragraph that begins `**The KFX plugin installer is written and unreachable.**`:

```markdown
**Closed 2026-09-23 (piece D).** `screepub kfx-status` and
`screepub kfx-install` reach it from the CLI, and the Send page draws the
same three-step checklist with Get and Install buttons. See
[the spec](superpowers/specs/2026-09-23-kfx-install-surface-design.md).
```

- [ ] **Step 2: The parity plan**

In `docs/superpowers/plans/2026-09-21-parity.md`, under `## Piece D: the KFX install surface`, directly below the heading:

```markdown
**Done 2026-09-23** on branch `parity-d-kfx-install`:
[spec](../specs/2026-09-23-kfx-install-surface-design.md),
[plan](2026-09-23-kfx-install-surface.md). The window also links to the
Calibre and Kindle Previewer download pages, which added three exact URLs
to the opener grant (approved by the owner the same day).
```

- [ ] **Step 3: The README**

In `README.md`, in `#### Device commands`, extend the code block:

```bash
bun src/cli.ts devices [--json]                          # list connected e-readers
bun src/cli.ts send <file> [--device <id>] [--json]      # send an existing file to one
bun src/cli.ts kfx-status [--json]                       # can this computer make KFX for a Kindle?
bun src/cli.ts kfx-install [--json]                      # install the KFX plugin into Calibre (online)
```

and add this paragraph after the one that ends `and \`devices\` prints the ids it accepts.`:

```markdown
A Kindle gets its best rendering from KFX, which needs three free tools:
Calibre, Amazon's Kindle Previewer, and the KFX Output plugin inside
Calibre. `kfx-status` says which are installed and where to get the rest;
`kfx-install` installs the plugin from Calibre's own plugin index. Until all
three are there, a Kindle gets AZW3 (with Calibre) or the engine's MOBI.
The desktop app shows the same checklist on its Send page.
```

- [ ] **Step 4: The whole suite and the typecheck**

```bash
bun test
bunx tsc --noEmit
```

Expected: both green. Report the pass/fail counts exactly as printed.

- [ ] **Step 5: No em dashes in anything this branch added**

```bash
git diff main -- . ':(exclude)*.lock' | grep '^+' | grep -n '—' || echo "no em dashes added"
```

Expected: `no em dashes added`. (Pre-existing lines that merely moved do not count; if one shows up, check whether this branch wrote it.)

- [ ] **Step 6: Commit**

```bash
git add docs/parity-audit.md docs/superpowers/plans/2026-09-21-parity.md README.md
git commit -m "Docs: piece D is done; the KFX verbs in the README and the audit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## After the tasks (controller, not a subagent)

1. **Whole-branch review** by a fresh reviewer, told to attack: the refusal order in `kfx-install`, the engine-link/grant agreement (mutate a URL in `kfx-setup.ts` and confirm `desktop-shell.test.ts` fails), `checklistFrom` against malformed answers, and the send/install overlap.
2. **One real end-to-end install, into a throwaway Calibre config:**
   ```bash
   export CALIBRE_CONFIG_DIRECTORY=<scratchpad>/calibre-e2e
   bun src/cli.ts kfx-status          # plugin: not installed
   bun src/cli.ts kfx-install         # real download from Calibre's index
   bun src/cli.ts kfx-status          # plugin: installed; Kindles get KFX
   ```
   The owner's real Calibre is untouched by construction.
3. **The window, live**, against the real engine with the same throwaway config, so the block actually appears on a machine where everything is installed: check the three rows, the Install press, the success line, and a Get link.
