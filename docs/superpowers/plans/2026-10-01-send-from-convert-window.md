# Send from Convert, part 2: the window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move sending onto Convert's result screen, starting from the reader's device: a page and a panel, device cards asked once, a calm screen with one brass button, "Other ways" with every route and every file type, Eject after a USB copy that survives leaving the page, the Send tab gone (Read and Settings get a Send button back to the result), and a Kindle heads-up whenever a book is open.

**Architecture:** Re-home, do not rewrite. send.js's pure decisions move unchanged to a new `send-routes.js`; its flows (the Kindle build-ahead, the Kindle line, sendTo, perform, keepFocus) move to a new `send-panel.js`, which draws inside Convert's result instead of on its own surface. The new decisions (which card, which brass, which asterisk, which save rows, what Eject says, when the heads-up shows) are pure functions in a new `device.js`. The 2 s devices poll moves out of any surface into `device-watch.js`, started by main.js whenever a script is open. Everything the result must remember across pages (the send line, the Eject button, the reader's device) lives in `ctx.state`, not in a surface's DOM. send.js and the 'send' surface are deleted at the end.

**Tech stack:** Static ES modules in `desktop/ui/` (no bundler), Bun + `bun:test`, the fake DOM in `tests/helpers/fake-dom.ts` and `tests/helpers/boot-window.ts` (behaviour tests that mount real modules; NO new source-regex tests), headless Chrome measurements in `tests/desktop-layout-measured.test.ts`, the capture gate in `tools/capture/gate.ts`.

**Spec:** `docs/superpowers/specs/2026-10-01-send-from-convert-design.md`, decisions 1 to 10, 12 (window side), 13 and 16. Part 1 (engine) is on this branch: `app-settings` keys `readerDevice`/`animatePage`/`typewriterSounds`, `export --for epub|kindle|kfx|azw3|mobi|fountain` with `--out`/`--check`, `eject <volume>` (codes `eject-failed`, `unknown-device`; JSON `{ ok: true, ejected }`). Parts 3 (animation) and 4 (device requests) are NOT here; this plan leaves two seams for them (Task 12's `result-page.js`, Task 7's `missingPanel()`).

**Working rules** (CLAUDE.md and the owner):
- TDD: the failing test first, then the code. Run `bun test` from the worktree root, never a subfolder.
- No test or manual check touches the real app settings, the real Calibre folder, or a real drive. Eject runs only through an injected runner (engine tests) or a stand-in (the window's fake Tauri, the live harness, which intercepts `eject`). The capture/measured runs point `SCREEPUB_VOLUME_ROOTS` at an empty scratch folder and `SCREEPUB_REMARKABLE_ENDPOINT` at a closed local port.
- No em dash in any text a person reads, or in new comments. Curly apostrophes as the existing files use them.
- Never a real screenplay name; the demo is `Field Station` from `tests/fixtures/`.
- Behaviour tests that mount the real modules on the fake DOM. Do not add tests that read a module's source text. (Three existing source pins are re-pointed, not added; see Task 13.)
- Each git command on its own line, never chained. Commit per task. Never push or merge.

---

## File map

| File | Change |
| --- | --- |
| `src/cli.ts`, `src/cli-devices.ts` | `devices --quick` (skip the reMarkable probe), for the background poll |
| `desktop/ui/app.js` | argv: `devices({quick})`, `eject(volume)`, `export(... { out })`, `readerDevice(kind)`; comments that named the Send page |
| `desktop/ui/send-routes.js` | NEW: send.js's pure route/file decisions, moved unchanged (less the ones the device model replaces) |
| `desktop/ui/device.js` | NEW: the device-card model and every new decision (cards, asterisks, brass, other ways, save rows, eject words, heads-up) |
| `desktop/ui/device-watch.js` | NEW: the 2 s `devices --quick` poll, app-wide, started while a script is open |
| `desktop/ui/send-panel.js` | NEW: the send panel inside the result (cards, calm, other ways, saves, eject, Kindle build-ahead), drawing + flows re-homed from send.js |
| `desktop/ui/result-page.js` | NEW: the result's left-hand page (static preview now; part 3 replaces its inside) |
| `desktop/ui/heads-up.js` | NEW: "Kindle plugged in. Copy <Title>?" line at the top of the sheet |
| `desktop/ui/convert.js` | result screen becomes page + panel; `showResult()`, `copyTo()`, `hide()`; `countsLine`, `readyLine` |
| `desktop/ui/read.js`, `desktop/ui/tune.js` | a Send button back to the result |
| `desktop/ui/frame.js`, `desktop/ui/main.js` | Send surface removed; watcher, heads-up, `ctx.devices`, `ctx.sendBook`, `ctx.copyTo` |
| `desktop/ui/kfx.js`, `desktop/ui/book-queue.js` | comments only (the checklist and the turns now belong to the panel) |
| `desktop/ui/send.js` | DELETED (Task 13) |
| `desktop/ui/surfaces.css`, `brand/tokens.json`, `desktop/ui/tokens.css` (generated) | result grid, cards, calm, other ways, asterisk, heads-up; one new colour token `present` |
| `tools/capture/gate.ts`, `tools/capture/run.ts` | allow the read-only calls the result now makes; device seams pinned |
| `tests/send-device.test.ts` | NEW: device.js |
| `tests/device-watch.test.ts` | NEW: device-watch.js |
| `tests/send-panel-ui.test.ts` | NEW: send-panel.js mounted (most of send-routes-ui's mounted block re-homed here) |
| `tests/send-routes-ui.test.ts` | pure half re-pointed at send-routes.js; mounted half moved to send-panel-ui and deleted here |
| `tests/desktop-ui.test.ts` | argv list, Convert result, tabs (3), focus plans, booted window, Send blocks re-pointed or deleted |
| `tests/capture.test.ts`, `tests/desktop-layout-measured.test.ts`, `tests/cli-device-commands.test.ts`, `tests/cli-verb-flags.test.ts` | see tasks |
| `README.md`, `desktop/README.md`, `docs/mac-qa.md`, `docs/developers.md` | the Send tab is gone; `devices --quick` |

---

## Decisions this plan makes (read before Task 1)

1. **Split.** `send-routes.js` (pure, moved), `device.js` (pure, new), `send-panel.js` (drawing + flows), `device-watch.js` (poll), `heads-up.js` (line), `result-page.js` (seam). send.js is deleted. Deleted outright with it, because the device model replaces them: `READERS`, `readerStatus`, `PROVEN_LABEL`, `UNPROVEN_SHORT`, `provenNote`, `caveatFor`, `REACH_HEADING`, the reach table, `LEDE`, `LOOKING` (re-declared in device.js), `NO_SCRIPT`, `SEND_LABEL`/`SENT_LABEL`/`sendLabel`, `buttonClassFor` (brass is the device model's now), `wantsKindleFile` (replaced by `kindleSession`). `routeLines` loses its `caveat` (the asterisk replaces it). Everything else moves verbatim.
2. **Device model.** `offerFor({ device, platform, shown, kindle })` returns `{ device, unproven, brass, next, lines, waiting }`. Kindle plugged in: brass Copy to Kindle (file type on the button), `next` = Send to Kindle, then email. Not plugged in: brass Send to Kindle (app or web, the engine's row), `next` = email, lines = the drag line and "or plug it in". Apple on a Mac with Books: Add to Apple Books; elsewhere Save the EPUB with one line. Kobo/tolino/reMarkable: their device row when connected, else Save the EPUB with a "plug it in" line (tolino on Windows: the undetectable line). The calm screen shows brass + Other ways + Different device; `next` leads Other ways' "Send it" group (that is where the spec's "then web, then email" order lands).
3. **Asterisk.** `unproven(kind, platform)`: false for `apple` (Apple Books on a Mac is in the verification ledger; elsewhere it is a plain save) and for a Kindle on a Mac; true for everything else. A Kindle off a Mac gets its own wording (the spec's literal sentence would be false: a real Kindle has been tried, on a Mac). See summary for the alternative.
4. **The poll.** `device-watch.js` polls `devices --quick` every 2 s from the moment a script is open until none is (main.js's `scriptChanged`), never counted toward busy (app.js already excludes `devices`). `--quick` is a one-line engine addition (Task 1) so a background poll never makes an HTTP probe every 2 s. A docked reMarkable is learnt from the panel's full `routes` answer instead.
5. **Routes.** No routes poll any more. The panel asks `routes --quick` then `routes` when its calm screen or Other ways first draws, and again whenever the watcher's device list changes.
6. **What survives leaving the page.** `ctx.state.readerDevice` (undefined until read, then null or a kind), `ctx.state.sendResult` (`{ book, line, bad, eject }`), `ctx.state.devices` (the watcher's list). Kept on the window's state object, not in module variables, so the result draws them again on any return and each test gets its own.
7. **Read and Settings** get a `Send` button (`btn btn-outline`) that calls `ctx.sendBook()`: draw the result again if the well is up, go to Convert, focus the brass button.
8. **Save a file** uses `export --for <type> --out <path>` for all five types (Save box first: the type names the file). The engine's `save-epub`/`save-kindle` routes are left out of "Send it" (they would duplicate the Save group). KFX greyed with "Set it up" when the Kindle check says the ladder does not reach KFX; it mounts kfx.js's checklist in place. AZW3 greyed when there is no Calibre (same link). MOBI and Fountain greyed with no `.fountain`.
9. **Show in Finder and the book's path** move from the result's asides into the foot of Other ways' "Save a file" group; the asides become exactly the spec's three: Read it, Settings, Convert another.
10. **Heads-up** suppressed while Convert's result is the page on screen (its card and brass already say it) and while a conversion runs.
11. **Capture gate.** The result now reads `app-settings`, polls `devices --quick`, and (once a card is chosen) asks `routes` and `export --check`. The gate learns exactly those shapes, plus `app-settings --set '{"readerDevice":"<kind>"}'` (a write into the capture's scratch config only). The capture and measured runs pin the device seams at empty scratch places.

---

### Task 1: `devices --quick` (engine)

**Files:** `src/cli.ts`, `tests/cli-device-commands.test.ts`, `tests/cli-verb-flags.test.ts`, `docs/developers.md`

The window will poll `devices` every 2 s whenever a book is open; without `--quick` each poll also makes an HTTP request to the reMarkable address and waits out its 1.5 s timeout.

- [ ] **Step 1: Failing tests.** In `tests/cli-device-commands.test.ts`, inside `describe('screepub devices')`:

```ts
  test('--quick lists mounted readers without asking for a reMarkable', async () => {
    let asked = 0;
    const answering = Bun.serve({ port: 0, fetch: () => { asked += 1; return new Response('[]'); } });
    const { root } = kindleRoot();
    const { stdout, exitCode } = await runCli(['devices', '--quick', '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${answering.port}`,
    });
    answering.stop(true);
    expect(exitCode).toBe(0);
    expect(soleJson(stdout).devices.map((d: { kind: string }) => d.kind)).toEqual(['kindle']);
    expect(asked).toBe(0);
  });
```
(`kindleRoot` is declared below the describe; move it above it, or define this test after it.) In `tests/cli-verb-flags.test.ts` change `devices: { own: [], positionals: [] }` to `devices: { own: ['--quick'], positionals: [] }`.
- [ ] **Step 2:** `bun test tests/cli-device-commands.test.ts tests/cli-verb-flags.test.ts`: both fail.
- [ ] **Step 3: Implement.** `VERB_FLAGS.devices = ['--quick']`; in the `devices` branch: `const { devices } = await devicesCommand({ ...deviceSeams(), skipRemarkable: values.quick });` (read `values` the way the `routes` branch does). DEVICES_USAGE gains `screepub devices [--quick] [--json]` and an options line `--quick                answer at once, without looking for a reMarkable`. The top-level usage line (around line 98) likewise.
- [ ] **Step 4:** `docs/developers.md`: the `devices` line becomes `bun src/cli.ts devices [--quick] [--json]`, and one sentence: "`devices --quick` skips the reMarkable probe, as `routes --quick` does."
- [ ] **Step 5:** `bun test` (whole suite) green. Commit: `devices --quick: list mounted readers without the reMarkable probe`.

---

### Task 2: argv builders

**Files:** `desktop/ui/app.js`, `tests/desktop-ui.test.ts` (the argv block around line 220 to 350)

- [ ] **Step 1: Failing tests.** In the block that builds every argv (`built = { ... }`), add:

```ts
      devicesQuick: argv.devices({ quick: true }),
      eject: argv.eject('/Volumes/Kindle'),
      exportOut: argv.export('/s/script.epub', { forFormat: 'kfx', fountain: '/s/script.fountain', optionsJson: '{"a":1}', out: '/s/out.kfx' }),
      readerDevice: argv.readerDevice('kobo'),
```
and extend the sorted name list with `'eject'` and `'readerDevice'`. New assertions:

```ts
  test('the send panel’s new calls are spelled once, here', () => {
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

  test('an eject counts toward busy, the background devices poll does not', async () => {
    // Driven through runEngine with a held fake shell, the way the existing
    // countsTowardBusy tests in this file do (see "devices is not counted").
  });
```
Write the second test in the exact shape of the existing `devices`/`routes` busy tests nearby (they hold a call and read `engineBusy()`): `argv.devices({ quick: true })` held leaves `engineBusy()` false; `argv.eject(...)` held makes it true.
- [ ] **Step 2:** run the file; fail.
- [ ] **Step 3: Implement** in `argv`:

```js
  /** What is plugged in. `quick` leaves out the reMarkable probe: the window
   *  polls this every two seconds while a book is open (device-watch.js), and
   *  a docked reMarkable is learnt from the send panel's full `routes`. */
  devices: ({ quick = false } = {}) =>
    ['devices', '--json', quick ? '--quick' : null].filter((a) => a !== null),

  /** Safely remove the reader at `volume`, a path `devices` listed. The
   *  engine refuses any other disk (unknown-device). */
  eject: (volume) => ['eject', volume, '--json'],

  /** The reader the person said is theirs, app-wide (the send panel's
   *  cards). Null forgets it. */
  readerDevice: (kind) => ['app-settings', '--json', '--set', JSON.stringify({ readerDevice: kind })],
```
and `export` gains `out = null` appended last: `out ? '--out' : null, out`. Keep `argv.devices()[0]` working in `countsTowardBusy` (it does). Update the `countsTowardBusy` doc comment: the poll is device-watch.js's, while a book is open, not the Send tab's; `eject` is counted (it is somebody's work).
- [ ] **Step 4:** whole suite green (the capture gate test builds `argv.devices()` and still refuses it until Task 6: fine). Commit: `app.js: argv for eject, devices --quick, export --out, readerDevice`.

---

### Task 3: send-routes.js, the pure half moved

**Files:** Create `desktop/ui/send-routes.js`; modify `desktop/ui/send.js`, `tests/send-routes-ui.test.ts` (pure half), `tests/desktop-ui.test.ts` ("what the Send surface decides", "the Send surface", "the ladder the Send surface asks for").

Pure move, no behaviour change, so the existing tests are the tests.

- [ ] **Step 1:** Re-point the imports first (they fail: the file is not there). In `tests/send-routes-ui.test.ts` lines 1 to 1030: `send = await import(join(UI, 'send-routes.js'))`, rename the type `SendModule` to `RoutesModule`, drop `LEDE`, `buttonClassFor`, `wantsKindleFile` from it. Delete the tests of the three removed exports in this half: "buttonClassFor: brass for the chosen route..." (408 to 443), "the lede is true on a first run too" and "the page no longer promises that nothing leaves this computer" (1015 to 1030), and the `wantsKindleFile` test inside "building the Kindle file before anyone presses anything" (865). Keep the em-dash test, pointed at send-routes.js's exported strings. In `tests/desktop-ui.test.ts`: "what the Send surface decides" imports `send-routes.js` and drops the tests of `READERS`, `readerStatus`, `provenNote`, `caveatFor`, `sendLabel` (their decisions go away with the reach table; Task 4 tests the replacement); `undetectable` and `platformOf` tests stay but import `device.js` in Task 4 (leave them on send.js until then). "the ladder the Send surface asks for" and "the Send surface"'s `readsTheBook`/nothing-connected tests import `send-routes.js`.
- [ ] **Step 2:** `bun test tests/send-routes-ui.test.ts`: fails (no module).
- [ ] **Step 3:** Create `desktop/ui/send-routes.js` with the file comment below and MOVE, verbatim with their doc comments, from send.js: `UNPROVEN`, `PROVEN`, `KINDS`, `isText` (exported as a helper), `platformOf`, `undetectable`, `forFormat`, `whereLine`, `blockedReason`, `optionsJsonFor`, `sentLine`, `SENT_LABEL` (sentLine needs it; keep it unexported), `artifactLine`, `NO_MESSAGE`, `statusFor`, `failureMessage`, `outcomeFor`, `needsSettings`, `preparingPhase`, `kindleTurn`, `mayBuild`, `buildsAhead`, `attemptKey`, `buildLine`, `UNAVAILABLE`, `routeDeviceFrom`, `routeFrom`, `routesFrom`, `isDeviceRoute`, `handsOverKindleFile`, `kindleCheckFrom`, `SAVE_FALLBACK`, `bareExtension`, `saveNameFor`, `saveFiltersFor`, `NO_NOTE`, `routeNoteFrom`, `emailSetupHint`, `EMAIL_SETUP`, `SETUP_BUTTON`, `SETUP_TITLE`, `performerFor`, `readsTheBook`, `connectedDevices`, `sameRoutes`, `routeLines` (with `caveat` removed: returns `{ title, detail, where }`), `buttonLabel`, `NO_EXTENSION`, `kindleFileFrom`, `NO_ROUTES`, `routesFailure`. Add one new helper the saves need:

```js
/** The file name at the end of a path, either separator. */
export function baseName(path) {
  const parts = String(path ?? '').split(/[\\/]/).filter((p) => p !== '');
  return parts.length === 0 ? '' : parts[parts.length - 1];
}
```
File comment:
```js
// What sending decides, with no drawing in it: the engine's route list and
// Kindle-file answers checked and read, the status line's words, the save
// box's defaults. Moved out of send.js unchanged when the Send page became
// the result screen's panel (spec 2026-10-01); send-panel.js draws with it,
// device.js builds the device model on top of it. If anything here starts to
// look like it knows what a Kindle is, it belongs in src/device/.
```
In send.js, replace the moved block with `import { ... } from './send-routes.js';` of what its drawing still uses, and keep `READERS`, `readerStatus`, `provenNote`, `caveatFor`, `LEDE`, `buttonClassFor`, `wantsKindleFile` etc. in send.js for now (its page still draws them until Task 13). `routeRow` there reads `caveatFor(route.device, navigator.platform)` directly since `routeLines` no longer returns it.
- [ ] **Step 4:** whole suite green (send.js's mounted tests still pass: nothing it draws changed). Commit: `send-routes.js: the Send page's pure decisions, moved out of send.js`.

---

### Task 4: device.js, the device model

**Files:** Create `desktop/ui/device.js`, `tests/send-device.test.ts`; move the `platformOf`/`undetectable` tests in `tests/desktop-ui.test.ts` to import `device.js` re-exports.

- [ ] **Step 1: Failing tests** in `tests/send-device.test.ts`. House style: real module, real engine catalog (`routes()` from `src/export/routes.ts`) for the round trips.

```ts
import { beforeAll, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import type { ConnectedDevice } from '../src/device/types';
import { preselected, routes } from '../src/export/routes';
import { READER_DEVICES } from '../src/settings/app';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
let d: any;
let r: any;
beforeAll(async () => {
  d = await import(join(UI, 'device.js'));
  r = await import(join(UI, 'send-routes.js'));
});

const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const tolino: ConnectedDevice = { kind: 'tolino', name: 'tolino', volume: '/Volumes/tolino' };
const rm: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };
const PLATFORMS = { darwin: 'MacIntel', win32: 'Win32', linux: 'Linux x86_64' } as const;

/** The engine's list as the window has it (routesFrom), for these facts. */
const shownFor = (platform: keyof typeof PLATFORMS, devices: ConnectedDevice[], extra = {}) => {
  const list = routes({ platform, booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices, ...extra });
  return r.routesFrom({ ok: true, routes: list, chosen: preselected(list, undefined).id });
};
const kfx = { extension: 'kfx', fresh: false, builtBy: 'calibre', bookDate: 1 };

describe('the cards: what do you read on', () => {
  test('five readers and the way out, in the spec’s order', () => {
    const cards = d.cardsFor('MacIntel', []);
    expect(cards.map((c: any) => c.name)).toEqual(
      ['Kindle', 'iPhone, iPad or Mac', 'Kobo', 'tolino', 'reMarkable', 'My device isn’t here']);
    expect(cards[1].sub).toBe('Apple Books');
  });
  test('every card but the last is a kind the engine will store', () => {
    expect(d.cardsFor('MacIntel', []).slice(0, -1).map((c: any) => c.kind)).toEqual([...READER_DEVICES]);
  });
  test('a reader the computer can see is marked here; an Apple card never is', () => {
    const here = d.cardsFor('MacIntel', [{ id: '/Volumes/KOBOeReader', ...kobo }])
      .filter((c: any) => c.here).map((c: any) => c.kind);
    expect(here).toEqual(['kobo']);
    expect(d.cardsFor('MacIntel', null).some((c: any) => c.here)).toBe(false);
  });
});

describe('the asterisk', () => {
  test('a Kindle on a Mac and Apple Books carry none; everything else does', () => {
    expect(d.unproven('kindle', 'MacIntel')).toBe(false);
    expect(d.unproven('apple', 'Win32')).toBe(false);
    for (const p of ['Win32', 'Linux x86_64', undefined]) expect(d.unproven('kindle', p)).toBe(true);
    for (const k of ['kobo', 'tolino', 'remarkable']) expect(d.unproven(k, 'MacIntel')).toBe(true);
  });
  test('it says the spec’s sentence, by name, and is true for a Kindle off a Mac', () => {
    expect(d.asteriskNote('kobo', 'MacIntel'))
      .toBe('Built and tested in code, never tried on a real Kobo yet. If it works for you, tell us.');
    expect(d.asteriskNote('kindle', 'Win32'))
      .toBe('Built and tested in code, never tried with a real Kindle on Windows yet. If it works for you, tell us.');
    expect(d.asteriskNote('kindle', 'MacIntel')).toBe(null);
  });
});

describe('offerFor: the one brass button', () => {
  test('a Kindle plugged in: Copy leads, naming the file; then Send to Kindle, then email', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', [kindle]), kindle: kfx });
    expect(o.brass.label).toBe('Copy to Kindle (.kfx)');
    expect(o.next.map((a: any) => a.route.key)).toEqual(['send-to-kindle', 'email-to-kindle']);
    expect(o.lines).toEqual([]);
    expect(o.unproven).toBe(false);
  });
  test('no Kindle plugged in: Send to Kindle leads, then email, then "plug it in"', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', []), kindle: kfx });
    expect(o.brass.route.key).toBe('send-to-kindle');
    expect(o.next.map((a: any) => a.route.key)).toEqual(['email-to-kindle']);
    expect(o.lines).toEqual([d.WEB_LINE, d.PLUG_KINDLE]);
  });
  test('Amazon’s app, when the engine says it is installed, is the same lead under its own name', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: shownFor('darwin', [], { sendToKindleApp: true }) });
    expect(o.brass.label).toBe('Send to Kindle app');
  });
  test('Apple: Add to Apple Books on a Mac, Save the EPUB elsewhere with its line', () => {
    expect(d.offerFor({ device: 'apple', platform: 'MacIntel', shown: shownFor('darwin', []) }).brass.route.key)
      .toBe('apple-books');
    const win = d.offerFor({ device: 'apple', platform: 'Win32', shown: shownFor('win32', []) });
    expect(win.brass.route.key).toBe('save-epub');
    expect(win.lines).toEqual([d.APPLE_ELSEWHERE]);
  });
  test('Kobo, tolino, reMarkable: their own row when connected, the EPUB and a "plug it in" line when not', () => {
    expect(d.offerFor({ device: 'kobo', platform: 'MacIntel', shown: shownFor('darwin', [kobo]) }).brass.label)
      .toBe('Copy to KOBOeReader');
    const away = d.offerFor({ device: 'kobo', platform: 'MacIntel', shown: shownFor('darwin', []) });
    expect(away.brass.route.key).toBe('save-epub');
    expect(away.lines).toEqual(['Plug your Kobo in over USB and this becomes Copy to Kobo.']);
    expect(d.offerFor({ device: 'remarkable', platform: 'MacIntel', shown: shownFor('darwin', [rm]) }).brass.route.key)
      .toBe('remarkable');
    expect(d.offerFor({ device: 'tolino', platform: 'Win32', shown: shownFor('win32', []) }).lines)
      .toEqual([r.undetectable('Win32')]);
    expect(away.unproven).toBe(true);
  });
  test('before the list arrives it waits rather than guessing', () => {
    const o = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown: null });
    expect(o.waiting).toBe(true);
    expect(o.brass).toBe(null);
  });
  test('on every list the engine can give, for every device: one brass, and it can fire', () => {
    for (const platform of ['darwin', 'win32', 'linux'] as const) {
      for (const set of [[], [kindle], [kobo, tolino], [kindle, rm]]) {
        for (const device of READER_DEVICES) {
          const o = d.offerFor({ device, platform: PLATFORMS[platform], shown: shownFor(platform, set) });
          expect(`${platform} ${device}: ${o.brass?.route.available}`).toBe(`${platform} ${device}: true`);
          expect(o.brass.label.trim()).not.toBe('');
          for (const a of o.next) expect(a.route.available).toBe(true);
        }
      }
    }
  });
});

describe('Other ways', () => {
  test('Send it: the device’s own follow-ups first, then every other route in the engine’s order, no saves, no brass', () => {
    const shown = shownFor('darwin', [kindle]);
    const offer = d.offerFor({ device: 'kindle', platform: 'MacIntel', shown });
    const rows = d.sendItRows(shown, offer);
    expect(rows.slice(0, 2).map((x: any) => x.key)).toEqual(['send-to-kindle', 'email-to-kindle']);
    expect(rows.some((x: any) => x.key === 'save-epub' || x.key === 'save-kindle')).toBe(false);
    expect(rows.some((x: any) => x.id === offer.brass.route.id)).toBe(false);
    // Every non-save route is somewhere: brass or a row.
    const all = shown.routes.filter((x: any) => !x.key.startsWith('save-')).map((x: any) => x.id);
    expect(new Set([offer.brass.route.id, ...rows.map((x: any) => x.id)])).toEqual(new Set(all));
  });
  test('Save a file: five types, each with who it is for', () => {
    const rows = d.saveRows({ fountainPath: '/l/a.fountain' }, kfx);
    expect(rows.map((x: any) => x.type)).toEqual(['epub', 'kfx', 'azw3', 'mobi', 'fountain']);
    expect(rows.every((x: any) => x.available && x.who.length > 0)).toBe(true);
  });
  test('KFX greys out with "set it up" when the ladder stops short of it; AZW3 when there is no Calibre', () => {
    const azw3 = d.saveRows({ fountainPath: '/l/a.fountain' }, { ...kfx, extension: 'azw3' });
    expect(azw3.find((x: any) => x.type === 'kfx')).toMatchObject({ available: false, setup: true });
    const none = d.saveRows({ fountainPath: '/l/a.fountain' }, { ...kfx, extension: 'mobi', builtBy: 'screepub' });
    expect(none.find((x: any) => x.type === 'azw3')).toMatchObject({ available: false, setup: true });
    // Not known yet: the engine refuses in its own words if it must.
    expect(d.saveRows({ fountainPath: '/l/a.fountain' }, null).every((x: any) => x.available)).toBe(true);
  });
  test('MOBI and Fountain need the .fountain', () => {
    const rows = d.saveRows({ fountainPath: null }, kfx);
    expect(rows.filter((x: any) => !x.available).map((x: any) => x.type)).toEqual(['mobi', 'fountain']);
  });
  test('the Kindle-file types take the Kindle line; the rest only copy', () => {
    expect(['epub', 'kfx', 'azw3', 'mobi', 'fountain'].map(d.saveTurn))
      .toEqual(['copy', 'kindle', 'kindle', 'kindle', 'copy']);
  });
});

describe('after a copy: Eject', () => {
  const dev = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  test('a copy to a volume ends with the reminder and the button', () => {
    expect(d.copiedState('Field Station', dev, { ok: true, destination: '/Volumes/Kindle/documents/x' })).toEqual({
      line: 'Field Station is on your Kindle. Eject it before you unplug.', bad: false,
      eject: { id: dev.id, name: 'Kindle', volume: '/Volumes/Kindle' },
    });
    expect(d.ejectLabel({ name: 'Kindle' })).toBe('Eject Kindle');
  });
  test('a reMarkable upload has nothing to eject', () => {
    const tab = { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null };
    expect(d.copiedState('Field Station', tab, { ok: true, uploaded: true }).eject).toBe(null);
  });
  test('ejected, gone already, or refused: the refusal keeps the button', () => {
    const e = { id: dev.id, name: 'Kindle', volume: dev.volume };
    expect(d.ejectOutcome({ ok: true, ejected: dev.volume }, e))
      .toEqual({ line: 'Your Kindle is ejected. You can unplug it now.', bad: false, eject: null });
    expect(d.ejectOutcome({ ok: false, error: { code: 'unknown-device', message: 'x' } }, e))
      .toEqual({ line: 'Your Kindle is not plugged in any more.', bad: false, eject: null });
    expect(d.ejectOutcome({ ok: false, error: { code: 'eject-failed', message: 'could not eject /Volumes/Kindle: busy' } }, e))
      .toEqual({ line: 'could not eject /Volumes/Kindle: busy', bad: true, eject: e });
  });
  test('the button goes once the poll no longer lists the volume; an unknown list keeps it', () => {
    const e = { id: dev.id, name: 'Kindle', volume: dev.volume };
    expect(d.ejectStillThere(e, [dev])).toBe(true);
    expect(d.ejectStillThere(e, [])).toBe(false);
    expect(d.ejectStillThere(e, null)).toBe(true);
  });
});

describe('the heads-up', () => {
  const k1 = { id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
  const opts = { open: true, onResult: false };
  test('the first list only seeds: a Kindle already there when the book opened is not news', () => {
    expect(d.headsUpFor(null, [k1], opts)).toEqual({ seen: [k1.id], show: null });
  });
  test('a Kindle that arrives is shown once; unplugged and back, once more', () => {
    expect(d.headsUpFor([], [k1], opts).show).toEqual(k1);
    expect(d.headsUpFor([k1.id], [k1], opts).show).toBe(null);
    const out = d.headsUpFor([k1.id], [], opts);
    expect(d.headsUpFor(out.seen, [k1], opts).show).toEqual(k1);
  });
  test('never with no book open, on the result screen, or for another kind', () => {
    expect(d.headsUpFor([], [k1], { open: false, onResult: false }).show).toBe(null);
    expect(d.headsUpFor([], [k1], { open: true, onResult: true }).show).toBe(null);
    expect(d.headsUpFor([], [{ ...k1, kind: 'kobo' }], opts).show).toBe(null);
  });
  test('its words, and how long it stays', () => {
    expect(d.headsUpLine('Field Station')).toBe('Kindle plugged in. Copy Field Station?');
    expect(d.HEADS_UP_MS).toBe(8000);
  });
});

describe('the words this adds', () => {
  test('carry no em dash, and none is blank', () => {
    const words = [d.QUESTION, d.WEB_LINE, d.PLUG_KINDLE, d.APPLE_ELSEWHERE, d.LOOKING, d.MISSING.name,
      d.MISSING_LINE, d.OTHER_WAYS, d.DIFFERENT, d.SEND_IT, d.SAVE_A_FILE, d.SEND_BOOK,
      ...d.SAVE_TYPES.flatMap((t: any) => [t.label, t.who]),
      ...['kindle', 'kobo', 'tolino', 'remarkable'].map((k) => d.asteriskNote(k, 'Win32')),
      ...['kobo', 'tolino', 'remarkable'].map((k) => d.plugLine(k, 'MacIntel'))];
    for (const w of words) {
      expect(typeof w).toBe('string');
      expect(w.trim()).not.toBe('');
      expect(w).not.toContain('—');
    }
  });
  test('readerDeviceFrom: a stored kind, or nothing', () => {
    expect(d.readerDeviceFrom({ ok: true, readerDevice: 'tolino' })).toBe('tolino');
    expect(d.readerDeviceFrom({ ok: true, readerDevice: null })).toBe(null);
    expect(d.readerDeviceFrom({ ok: true, readerDevice: 'nook' })).toBe(null);
    expect(d.readerDeviceFrom({ ok: false })).toBe(null);
  });
});
```
- [ ] **Step 2:** `bun test tests/send-device.test.ts`: fails.
- [ ] **Step 3: Implement `desktop/ui/device.js`.**

```js
// The reader's device, and what each one offers (spec 2026-10-01,
// decisions 3 to 10, 12 and 16). Pure: every rule the send panel draws is
// here, tested by tests/send-device.test.ts. The routes themselves are the
// engine's (send-routes.js checks them); this file only says which of them a
// person with this device should see first.
import {
  PROVEN, platformOf, undetectable, buttonLabel, isDeviceRoute, failureMessage, sentLine, isText,
} from './send-routes.js';

export { platformOf, undetectable };

export const QUESTION = 'What do you read on?';
export const LOOKING = 'Looking for every way to send it…';
export const OTHER_WAYS = 'Other ways';
export const DIFFERENT = 'Different device';
export const SEND_IT = 'Send it';
export const SAVE_A_FILE = 'Save a file';
/** The Read and Settings pages' button back to the result. */
export const SEND_BOOK = 'Send';

/** The cards, in the spec's order. `kind` is what the engine stores as
 *  `readerDevice` (src/settings/app.ts's READER_DEVICES). */
export const DEVICES = [
  { kind: 'kindle', name: 'Kindle', sub: null },
  { kind: 'apple', name: 'iPhone, iPad or Mac', sub: 'Apple Books' },
  { kind: 'kobo', name: 'Kobo', sub: null },
  { kind: 'tolino', name: 'tolino', sub: null },
  { kind: 'remarkable', name: 'reMarkable', sub: null },
];
export const KINDS = DEVICES.map((d) => d.kind);
export const MISSING = { kind: 'missing', name: 'My device isn’t here', sub: null };
/** The placeholder part 4 replaces with the request field. */
export const MISSING_LINE = 'Screepub does not know it yet. Tell us which one and we will look at adding it. '
  + 'Until then, the EPUB works on most readers.';

export const nameOf = (kind) => DEVICES.find((d) => d.kind === kind)?.name ?? 'reader';

/** `app-settings --json`'s readerDevice, or null for none, unknown or a
 *  refusal (the cards are asked again; nothing is written). */
export function readerDeviceFrom(answer) {
  return answer?.ok === true && KINDS.includes(answer.readerDevice) ? answer.readerDevice : null;
}

/** The first connected reader of `kind` in the watcher's list, or null. */
export function pluggedIn(kind, devices) {
  if (kind === 'apple' || !Array.isArray(devices)) return null;
  return devices.find((d) => d?.kind === kind) ?? null;
}

/** Built and code-tested, never run on that hardware here. The ledger's one
 *  proven pair is a Kindle on a Mac; Apple Books is a Mac app, and off a Mac
 *  the Apple card is only a save. */
export function unproven(kind, platform) {
  if (kind === 'apple') return false;
  return !(kind === PROVEN.kind && platformOf(platform) === PROVEN.platform);
}

const OS = { windows: 'Windows', linux: 'Linux' };

/** What pressing or hovering the asterisk says, or null when there is none. */
export function asteriskNote(kind, platform) {
  if (!unproven(kind, platform)) return null;
  const tail = 'yet. If it works for you, tell us.';
  if (kind === PROVEN.kind) {
    const where = OS[platformOf(platform)] ?? 'this kind of computer';
    return `Built and tested in code, never tried with a real Kindle on ${where} ${tail}`;
  }
  return `Built and tested in code, never tried on a real ${nameOf(kind)} ${tail}`;
}

export function cardsFor(platform, devices) {
  return [
    ...DEVICES.map((d) => ({ ...d, here: pluggedIn(d.kind, devices) !== null, unproven: unproven(d.kind, platform) })),
    { ...MISSING, here: false, unproven: false },
  ];
}

export const WEB_LINE = 'Amazon’s Send to Kindle opens beside the book’s folder. Drag the book into it: '
  + 'Screepub cannot upload it for you.';
export const PLUG_KINDLE = 'Or plug the Kindle in over USB and copy it across.';
export const APPLE_ELSEWHERE = 'Then put it in iCloud Drive, or email it to yourself, and open it in Books '
  + 'on your iPhone or iPad.';

export function plugLine(kind, platform) {
  if (kind === 'tolino') {
    const cannot = undetectable(platform);
    if (cannot !== null) return cannot;
  }
  if (kind === 'remarkable') {
    return 'Dock it over USB, with its USB web interface switched on, and this becomes Upload to reMarkable.';
  }
  const name = nameOf(kind);
  return `Plug your ${name} in over USB and this becomes Copy to ${name}.`;
}

const available = (shown, test) => shown.routes.find((r) => r.available && test(r)) ?? null;
const keyed = (key) => (r) => r.key === key;
const readerOf = (kind) => (r) => isDeviceRoute(r) && r.device?.kind === kind;

/** The calm screen for `device`: one brass action, the follow-ups that lead
 *  Other ways, the lines under the brass, and whether it carries the
 *  asterisk. `shown` is routesFrom()'s answer (null while it is on its way),
 *  `kindle` kindleCheckFrom()'s, which names the Kindle file on its button. */
export function offerFor({ device, platform, shown, kindle = null }) {
  if (!KINDS.includes(device)) return null;
  const base = { device, unproven: unproven(device, platform), brass: null, next: [], lines: [], waiting: false };
  if (!Array.isArray(shown?.routes)) return { ...base, waiting: true, lines: [LOOKING] };
  const act = (route) => (route === null ? null : { route, label: buttonLabel(route, kindle) });
  const save = act(available(shown, keyed('save-epub')));
  if (device === 'kindle') {
    const usb = available(shown, readerOf('kindle'));
    const web = act(available(shown, keyed('send-to-kindle')));
    const mail = act(available(shown, keyed('email-to-kindle')));
    if (usb !== null) return { ...base, brass: act(usb), next: [web, mail].filter(Boolean) };
    return { ...base, brass: web ?? save, next: [mail].filter(Boolean), lines: web ? [WEB_LINE, PLUG_KINDLE] : [PLUG_KINDLE] };
  }
  if (device === 'apple') {
    const books = platformOf(platform) === 'mac' ? available(shown, keyed('apple-books')) : null;
    if (books !== null) return { ...base, brass: act(books) };
    return { ...base, brass: save, lines: [APPLE_ELSEWHERE] };
  }
  const usb = available(shown, readerOf(device));
  if (usb !== null) return { ...base, brass: act(usb) };
  return { ...base, brass: save, lines: [plugLine(device, platform)] };
}

/** The reader kind a route row is about, connected or dimmed, or null. */
export function kindOfRoute(route) {
  if (isText(route?.device?.kind)) return route.device.kind;
  const key = route?.key ?? '';
  if (key.startsWith('device:')) return key.slice('device:'.length);
  return key === 'remarkable' ? 'remarkable' : null;
}

/** Other ways' "Send it" group: the offer's follow-ups first, then every
 *  other route the engine lists, in its order, dimmed ones included. Never
 *  the brass one, and never the two saves: "Save a file" holds those. */
export function sendItRows(shown, offer) {
  if (!Array.isArray(shown?.routes)) return [];
  const rows = shown.routes.filter((r) => r.key !== 'save-epub' && r.key !== 'save-kindle');
  const lead = (offer?.next ?? []).map((a) => a.route.id);
  const brass = offer?.brass?.route.id ?? null;
  return [
    ...lead.map((id) => rows.find((r) => r.id === id)).filter(Boolean),
    ...rows.filter((r) => r.id !== brass && !lead.includes(r.id)),
  ];
}

export const SAVE_TYPES = [
  { type: 'epub', label: 'EPUB', who: 'Most readers, Apple Books, and email.' },
  { type: 'kfx', label: 'KFX', who: 'Modern Kindles over USB, with the best layout.' },
  { type: 'azw3', label: 'AZW3', who: 'Older Kindles over USB.' },
  { type: 'mobi', label: 'MOBI', who: 'The oldest Kindles.' },
  { type: 'fountain', label: 'Fountain', who: 'The script as plain text, for screenwriting apps.' },
];
export const NEEDS_TOOLS = 'This computer cannot make one yet.';
export const SET_IT_UP = 'Set it up';
export const NO_FOUNTAIN = 'This book was made without a .fountain, so there is none to work from.';

/** The Save a file rows. `kindle` is the check's answer: the ladder's top
 *  rung says whether KFX is reachable, `builtBy` whether Calibre is there.
 *  Not known yet leaves every row live: the engine refuses in its own words. */
export function saveRows(script, kindle) {
  const fountain = isText(script?.fountainPath);
  return SAVE_TYPES.map((t) => {
    if ((t.type === 'mobi' || t.type === 'fountain') && !fountain) {
      return { ...t, available: false, fix: NO_FOUNTAIN, setup: false };
    }
    if (t.type === 'kfx' && kindle && kindle.extension !== 'kfx') return { ...t, available: false, fix: NEEDS_TOOLS, setup: true };
    if (t.type === 'azw3' && kindle?.builtBy === 'screepub') return { ...t, available: false, fix: NEEDS_TOOLS, setup: true };
    return { ...t, available: true, fix: null, setup: false };
  });
}

/** book-queue.js's turn for a save of `type`, and whether it also waits in
 *  the Kindle line (send-panel.js's inKindleLine). `export --for mobi` builds
 *  the MOBI without touching the book, so it only reads it. */
export function saveTurn(type) {
  return type === 'kfx' || type === 'azw3' || type === 'mobi' ? 'kindle' : 'copy';
}

/** What the panel says after a copy the engine called done. */
export function copiedState(title, device, sent) {
  const book = isText(title) ? title.trim() : 'The book';
  if (!isText(device?.volume)) return { line: sentLine(device, sent), bad: false, eject: null };
  return {
    line: `${book} is on your ${device.name}. Eject it before you unplug.`,
    bad: false,
    eject: { id: device.id, name: device.name, volume: device.volume },
  };
}

export const ejectLabel = (eject) => `Eject ${eject.name}`;

export function ejectOutcome(answer, eject) {
  if (answer?.ok === true) return { line: `Your ${eject.name} is ejected. You can unplug it now.`, bad: false, eject: null };
  if (answer?.error?.code === 'unknown-device') {
    return { line: `Your ${eject.name} is not plugged in any more.`, bad: false, eject: null };
  }
  return { line: failureMessage(answer), bad: true, eject };
}

export function ejectStillThere(eject, devices) {
  if (!Array.isArray(devices)) return true;
  return devices.some((d) => d?.volume === eject.volume);
}

export const HEADS_UP_MS = 8000;
export const headsUpLine = (title) => `Kindle plugged in. Copy ${title}?`;
export const HEADS_UP_COPY = 'Copy it';
export const HEADS_UP_LATER = 'Not now';

/** Which Kindle, if any, the heads-up offers. `seen` is the Kindle ids of
 *  the last list (null before the first, which only seeds); an id that
 *  leaves the list is forgotten, so plugging it in again is news again. */
export function headsUpFor(seen, devices, { open, onResult }) {
  if (!Array.isArray(devices)) return { seen, show: null };
  const kindles = devices.filter((d) => d?.kind === 'kindle');
  const ids = kindles.map((d) => d.id);
  if (seen === null) return { seen: ids, show: null };
  const fresh = kindles.find((d) => !seen.includes(d.id)) ?? null;
  return { seen: ids, show: open && !onResult ? fresh : null };
}

/** Whether this is a Kindle session (the build-ahead's `forKindle`): the
 *  reader said Kindle, or one is plugged in now. */
export function kindleSession(device, devices) {
  return device === 'kindle' || pluggedIn('kindle', devices) !== null;
}
```
(Add `kindleSession` tests beside the others: kindle chosen, kobo chosen with a Kindle plugged, kobo chosen alone.) Move the existing `platformOf`/`undetectable` tests in desktop-ui to import `device.js` (they re-export).
- [ ] **Step 4:** whole suite green. Commit: `device.js: what each reader is offered, as pure decisions`.

---

### Task 5: device-watch.js, the poll that runs while a book is open

**Files:** Create `desktop/ui/device-watch.js`, `tests/device-watch.test.ts`.

- [ ] **Step 1: Failing tests** (`tests/device-watch.test.ts`). The runner and the timers are injected, so no fake DOM is needed:

```ts
import { beforeAll, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
const UI = join(import.meta.dir, '..', 'desktop', 'ui');
let w: any;
beforeAll(async () => { w = await import(join(UI, 'device-watch.js')); });

function rig() {
  const calls: { args: string[]; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  let tick: (() => void) | null = null;
  const watcher = w.createWatcher({
    run: (args: string[]) => new Promise((resolve, reject) => calls.push({ args, resolve, reject })),
    timers: { setInterval: (fn: () => void) => { tick = fn; return 1; }, clearInterval: () => { tick = null; } },
  });
  const flush = () => new Promise((r) => setTimeout(r, 0));
  return { watcher, calls, tick: () => tick?.(), ticking: () => tick !== null, flush };
}
const kindle = { id: '/V/Kindle', kind: 'kindle', name: 'Kindle', volume: '/V/Kindle' };

describe('the devices poll', () => {
  test('start asks at once with --quick, then every tick; stop ends it and forgets the list', async () => {
    const { watcher, calls, tick, ticking, flush } = rig();
    watcher.start();
    expect(calls.map((c) => c.args)).toEqual([['devices', '--json', '--quick']]);
    calls[0].resolve({ ok: true, devices: [kindle] });
    await flush();
    expect(watcher.current()).toEqual([kindle]);
    tick();
    expect(calls.length).toBe(2);
    watcher.stop();
    expect(ticking()).toBe(false);
    expect(watcher.current()).toBe(null);
  });
  test('a tick while an ask is out asks nothing', () => {
    const { watcher, calls, tick } = rig();
    watcher.start();
    tick();
    tick();
    expect(calls.length).toBe(1);
  });
  test('subscribers hear a change, and only a change', async () => {
    const { watcher, calls, tick, flush } = rig();
    const heard: unknown[] = [];
    watcher.subscribe((list: unknown) => heard.push(list));
    watcher.start();
    calls[0].resolve({ ok: true, devices: [] });
    await flush();
    tick(); calls[1].resolve({ ok: true, devices: [] }); await flush();
    tick(); calls[2].resolve({ ok: true, devices: [kindle] }); await flush();
    expect(heard).toEqual([[], [kindle]]);
  });
  test('a refusal, a throw or a malformed answer keeps the last list', async () => {
    const { watcher, calls, tick, flush } = rig();
    watcher.start();
    calls[0].resolve({ ok: true, devices: [kindle] }); await flush();
    tick(); calls[1].reject(new Error('the engine is gone')); await flush();
    tick(); calls[2].resolve({ ok: true, devices: [{ kind: 'kindle' }] }); await flush();
    tick(); calls[3].resolve({ ok: false, error: { code: 'internal', message: 'x' } }); await flush();
    expect(watcher.current()).toEqual([kindle]);
  });
  test('an answer that lands after stop is dropped; start twice is one interval', async () => {
    const { watcher, calls, flush } = rig();
    watcher.start(); watcher.start();
    expect(calls.length).toBe(1);
    watcher.stop();
    calls[0].resolve({ ok: true, devices: [kindle] }); await flush();
    expect(watcher.current()).toBe(null);
  });
});
```
- [ ] **Step 2:** fails. **Step 3: Implement:**

```js
// What is plugged in, asked every two seconds whenever a book is open (spec
// decision 16: the Kindle heads-up needs it on every page, and the result's
// cards need it to mark a reader the computer can see). main.js starts it
// when a script opens and stops it when none is. `devices --quick` skips the
// reMarkable probe; the send panel's full `routes` answer finds a docked one.
// Uncounted toward busy (app.js's countsTowardBusy): a listing is never
// anybody's work.
import { argv } from './app.js';

export const POLL_MS = 2000;
const isText = (v) => typeof v === 'string' && v.trim() !== '';

/** `devices --json`'s list, rebuilt clean, or null when it is not one. */
export function devicesFrom(answer) {
  if (answer?.ok !== true || !Array.isArray(answer.devices)) return null;
  const list = answer.devices.map((d) => (d && isText(d.id) && isText(d.kind) && typeof d.name === 'string'
    && (typeof d.volume === 'string' || d.volume === null)
    ? { id: d.id, kind: d.kind, name: d.name, volume: d.volume } : null));
  return list.includes(null) ? null : list;
}

export function createWatcher({ run, every = POLL_MS, timers = globalThis }) {
  let timer = null;
  let out = 0;
  let era = 0;
  let list = null;
  const subs = new Set();
  async function ask() {
    if (out > 0) return;
    out += 1;
    const mine = era;
    try {
      const next = devicesFrom(await run(argv.devices({ quick: true })));
      if (mine !== era || next === null) return;
      if (list !== null && JSON.stringify(list) === JSON.stringify(next)) return;
      list = next;
      for (const fn of [...subs]) fn(list);
    } catch {
      // A listing that failed changes nothing; the next tick asks again.
    } finally {
      out -= 1;
    }
  }
  return {
    start() {
      if (timer !== null) return;
      ask();
      timer = timers.setInterval(ask, every);
    },
    stop() {
      timers.clearInterval(timer);
      timer = null;
      era += 1;
      list = null;
    },
    current: () => list,
    subscribe(fn) { subs.add(fn); return () => subs.delete(fn); },
  };
}
```
(Note: `out` is not reset by `stop()`, so a stale ask still in flight blocks one tick after a restart; acceptable, and the test "answer after stop is dropped" covers the list.)
- [ ] **Step 4:** whole suite green. Commit: `device-watch.js: the devices poll, for whenever a book is open`.

---

### Task 6: the capture gate and the pinned device seams

**Files:** `tools/capture/gate.ts`, `tools/capture/run.ts`, `tests/capture.test.ts`, `tests/desktop-layout-measured.test.ts` (env only).

The result screen (Task 12) will read `app-settings`, poll `devices --quick` (Task 13) and, once a card is chosen, ask `routes` and `export --check`. A refused call fails the capture and the measured test, so the gate learns these first.

- [ ] **Step 1: Failing tests** in `tests/capture.test.ts`. Change the "refuses anything that reaches devices" test: `argv.devices()` moves to the allowed list; `argv.send(...)`, `argv.export(inLib(...), { forFormat: 'kindle' })` (a build), `argv.eject('/Volumes/Kindle')` and `argv.route('send-to-kindle', E)` stay refused. Add:

```ts
  test('it allows the result screen’s reads: devices, the routes and the Kindle check for the demo book', () => {
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
      argv.export(E, { forFormat: 'epub', out: inLib('copy.epub') }),
    ]) {
      expect(gateEngineCall(args, ctx).allow).toBe(false);
    }
  });
```
- [ ] **Step 2:** fails. **Step 3: Implement** in `gateEngineCall`, after the `appSettings()` rule:

```ts
  // The result screen's device poll and its one-off full listing. The
  // capture points SCREEPUB_VOLUME_ROOTS at an empty scratch folder and the
  // reMarkable at a closed local port (run.ts), so these list nothing real.
  if (same(args, argv.devices()) || same(args, argv.devices({ quick: true }))) return ALLOW;

  // A card chosen on the result screen: written into the capture's own
  // scratch config folder, and only the reader's device, by name.
  for (const kind of ['kindle', 'apple', 'kobo', 'tolino', 'remarkable']) {
    if (same(args, argv.readerDevice(kind))) return ALLOW;
  }

  // Read-only questions about the demo book: its routes and its Kindle file.
  if (first === 'routes' && inside(args[1], ctx.library)
    && (same(args, argv.routes(args[1])) || same(args, argv.routes(args[1], { quick: true })))) return ALLOW;
  if (first === 'export' && inside(args[1], ctx.library) && same(args, argv.kindleCheck(args[1]))) return ALLOW;
```
Update the file's top comment (it now also allows the result screen's reads). In `tools/capture/run.ts` (line 174) and in `tests/desktop-layout-measured.test.ts`'s `runEngine` env, add:

```ts
        // No real reader, ever: an empty mount parent, and a reMarkable
        // address nothing listens on.
        SCREEPUB_VOLUME_ROOTS: join(library, '.no-volumes'),
        SCREEPUB_REMARKABLE_ENDPOINT: 'http://127.0.0.1:9',
```
and `mkdirSync(join(library, '.no-volumes'), { recursive: true })` before the server starts (the scan of a missing root may differ from an empty one; check `src/device/volumes.ts` and make it exist either way).
- [ ] **Step 4:** whole suite green (the measured test still passes: nothing calls these yet). Commit: `capture gate: the result screen's device and route reads, with the device seams pinned`.

---

### Task 7: send-panel.js A: the cards, the choice, the placeholder

**Files:** Create `desktop/ui/send-panel.js`, `tests/send-panel-ui.test.ts`.

The panel is mounted into a node by Convert's result (Task 12); it is tested standalone here.

- [ ] **Step 1: The test rig and the first tests** (`tests/send-panel-ui.test.ts`):

```ts
import { afterEach, describe, expect, test } from 'bun:test';
import { join } from 'node:path';
import type { ConnectedDevice } from '../src/device/types';
import { preselected, routes } from '../src/export/routes';
import { fakePage, HOLD, settle, withGlobals, type FakePage } from './helpers/fake-dom';

const UI = join(import.meta.dir, '..', 'desktop', 'ui');
const EPUB = '/lib/field-station/Field Station.epub';
const FOUNTAIN = '/lib/field-station/Field Station.fountain';
type Dev = { id: string; kind: string; name: string; volume: string | null };
const dev = (d: ConnectedDevice): Dev => ({ id: d.volume ?? 'remarkable', kind: d.kind, name: d.name, volume: d.volume });
const KINDLE: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' };
const KOBO: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/Volumes/KOBOeReader' };
const listed = (devices: ConnectedDevice[], extra = {}) => {
  const list = routes({ platform: 'darwin', booksApp: true, sendToKindleApp: false, appleMailDefault: true, devices, ...extra });
  return { ok: true, routes: list, chosen: preselected(list, undefined).id };
};

let page: FakePage | null = null;
let hide: (() => void) | null = null;
let copies = 0;
afterEach(async () => { hide?.(); hide = null; await page?.close(); page = null; });

async function mountPanel(o: {
  readerDevice?: string | null; devices?: ConnectedDevice[] | null; check?: unknown;
  script?: Record<string, unknown>; engine?: (args: string[]) => unknown; platform?: string;
  state?: Record<string, unknown>;
} = {}) {
  let plugged = o.devices === undefined ? [] : o.devices;
  page = fakePage({
    platform: o.platform ?? 'MacIntel',
    respond: (args) => {
      const own = o.engine?.(args);
      if (own !== undefined) return own;
      if (args[0] === 'app-settings' && !args.includes('--set')) return { ok: true, readerDevice: o.readerDevice ?? null };
      if (args[0] === 'routes') return listed(plugged ?? []);
      if (args[0] === 'export' && args.includes('--check')) return o.check ?? { ok: false };
      if (args[0] === 'kfx-status') return { ok: false };
      return HOLD;
    },
  });
  copies += 1;
  const panel = await import(`${join(UI, 'send-panel.js')}?panel-${copies}`);
  const node = page.pane();
  node.id = 'surface-convert';
  const subs = new Set<(l: unknown) => void>();
  let restored = 0;
  const ctx: any = {
    state: {
      script: { title: 'Field Station', path: '/s/Field Station.pdf', epubPath: EPUB, fountainPath: FOUNTAIN,
        settings: { dialogueSideMarginPct: 27 }, ...o.script },
      devices: [], readerDevice: undefined, sendResult: null, ...o.state,
    },
    restoreFocus: () => { restored += 1; },
    goTo: () => {}, convertAgain: () => {},
    devices: {
      current: () => (plugged === null ? null : plugged.map(dev)),
      subscribe: (fn: (l: unknown) => void) => { subs.add(fn); return () => subs.delete(fn); },
    },
  };
  panel.mountPanel(node, ctx);
  panel.panelShown();
  hide = () => panel.panelHidden();
  await settle();
  return {
    panel, node, ctx, tauri: page.tauri, page, restored: () => restored,
    async plug(next: ConnectedDevice[]) { plugged = next; for (const fn of subs) fn(next.map(dev)); await settle(); },
    cards: () => node.querySelectorAll('.device-card').map((c) => c.querySelector('.device-card-name')!.textContent),
    brass: () => node.querySelector('.calm .btn-brad'),
    status: () => node.querySelector('.send-status')!,
  };
}

describe('the cards', () => {
  test('first time: the question, six cards, and a plugged-in reader’s card marked', async () => {
    const p = await mountPanel({ devices: [KOBO] });
    expect(p.node.querySelector('.panel-question')!.textContent).toBe('What do you read on?');
    expect(p.cards()).toEqual(['Kindle', 'iPhone, iPad or Mac', 'Kobo', 'tolino', 'reMarkable', 'My device isn’t here']);
    const here = p.node.querySelectorAll('.device-card-here').map((c) => c.querySelector('.device-card-name')!.textContent);
    expect(here).toEqual(['Kobo']);
    // It asked the engine once, read-only, which reader this is.
    expect(p.tauri.callsTo('app-settings').map((c) => c.args)).toEqual([['app-settings', '--json']]);
  });

  test('choosing a card stores it app-wide and shows the calm screen, and the cards are not asked again', async () => {
    const p = await mountPanel({ engine: (a) => (a.includes('--set') ? { ok: true, readerDevice: 'kobo' } : undefined) });
    p.node.button('Kobo').click();
    await settle();
    expect(p.tauri.callsTo('app-settings').at(-1)!.args).toEqual(['app-settings', '--json', '--set', '{"readerDevice":"kobo"}']);
    expect(p.ctx.state.readerDevice).toBe('kobo');
    expect(p.node.querySelector('.calm')).not.toBeNull();
    expect(p.node.querySelector('.device-cards')).toBeNull();
    // A later result reads it from the window's state, not the engine.
    const again = await mountPanel({ state: { readerDevice: 'kobo' } });
    expect(again.tauri.callsTo('app-settings')).toEqual([]);
    expect(again.node.querySelector('.calm')).not.toBeNull();
  });

  test('a store the engine refuses still answers this session, and says why', async () => {
    const p = await mountPanel({ engine: (a) => (a.includes('--set') ? { ok: false, error: { code: 'bad-settings', message: 'the settings file is read-only' } } : undefined) });
    p.node.button('Kindle').click();
    await settle();
    expect(p.node.querySelector('.calm')).not.toBeNull();
    expect(p.status().textContent).toBe('the settings file is read-only');
  });

  test('Different device brings the cards back, and stores nothing until one is chosen', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    p.node.button('Different device').click();
    expect(p.cards().length).toBe(6);
    expect(p.tauri.callsTo('app-settings')).toEqual([]);
  });

  test('My device isn’t here: the placeholder, which stores nothing', async () => {
    const p = await mountPanel();
    p.node.button('My device isn’t here').click();
    const box = p.node.querySelector('.device-missing')!;
    expect(box.querySelector('p')!.textContent).toBe('Screepub does not know it yet. Tell us which one and we will look at adding it. Until then, the EPUB works on most readers.');
    box.button('Tell us on GitHub').click();
    await settle();
    expect(p.page.tauri.opened[0]).toStartWith('https://github.com/method-production-suite/method-reader/issues/new?');
    box.button('Back').click();
    expect(p.cards().length).toBe(6);
    expect(p.tauri.calls.some((c) => c.args.includes('--set'))).toBe(false);
  });

  test('a card plugged in while the cards are up is marked without a redraw stealing the keyboard', async () => {
    const p = await mountPanel();
    const kindleCard = p.node.button('Kindle');
    kindleCard.focus();
    await p.plug([KINDLE]);
    expect(p.node.querySelectorAll('.device-card-here').length).toBe(1);
    expect(p.page.doc.activeElement).toBe(p.node.button('Kindle'));
  });
});

describe('the asterisk', () => {
  test('a Kobo card and the Kobo calm screen carry it; pressing shows the note; tell us opens an issue', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    const star = p.node.querySelector('.calm .asterisk')!;
    const note = p.node.querySelector('.calm .asterisk-note')!;
    expect(note.hidden).toBe(true);
    star.click();
    expect(note.hidden).toBe(false);
    expect(star.getAttribute('aria-expanded')).toBe('true');
    expect(note.textContent).toContain('never tried on a real Kobo yet');
    note.button('tell us').click();
    await settle();
    expect(p.page.tauri.opened.length).toBe(1);
  });
  test('a Kindle on a Mac carries none', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    expect(p.node.querySelector('.calm .asterisk')).toBeNull();
  });
});
```
(Check fake-dom's `FakeNode.button(label)` helper and `focus()`/`activeElement` before relying on them; both exist: `sheet.button('Close')` and `w.doc.activeElement` are used in desktop-ui.test.ts.)
- [ ] **Step 2:** fails. **Step 3: Implement** the panel's frame and these states. Shape of `desktop/ui/send-panel.js` after this task:

```js
// The send panel: the right-hand half of Convert's result (spec 2026-10-01,
// decisions 2 to 10, 12, 13). It starts from the reader's device. First the
// cards ("What do you read on?"), asked once and stored app-wide; then the
// calm screen, one brass button for the way that fits right now; Other ways
// for everything else. The flows (the Kindle file built ahead, a copy to a
// reader, a route the engine performs, a save) are send.js's, re-homed; the
// rules are device.js's and send-routes.js's. Nothing here decides anything.
//
// What has to outlive a redraw lives on the window's state, not here:
// ctx.state.readerDevice, ctx.state.sendResult (the line and the Eject
// button), ctx.state.devices. What has to outlive a book (Kindle builds and
// their line) stays module-level, as it was in send.js.
import { runEngine, argv, saveDialog, openUrl } from './app.js';
import { el, clear, text } from './dom.js';
import { canFocus } from './focus.js';
import { newIssueUrl, osLabel } from './feedback.js';
import { RELEASE } from './notes.js';
import * as D from './device.js';
import * as R from './send-routes.js';

let ctx = null;
let host = null;
let body = null;      // the part redrawn between cards / calm / missing
let statusLine = null;
let unsubscribe = null;
let showing = false;
/** 'cards' | 'calm' | 'missing' */
let mode = 'cards';

const platform = () => navigator.userAgentData?.platform ?? navigator.platform;
const devicesNow = () => ctx.devices?.current?.() ?? null;

export function mountPanel(node, context) {
  unsubscribe?.();
  host = node;
  ctx = context;
  statusLine = el('p', { class: 'caption send-status', role: 'status' }, '');
  body = el('div', { class: 'panel-body' });
  clear(host);
  host.append(body, statusLine);
  unsubscribe = ctx.devices?.subscribe?.(onDevices) ?? null;
  // ... (Task 11 draws ctx.state.sendResult here)
  start();
}

async function start() {
  const blocked = R.blockedReason(ctx.state.script);
  if (blocked !== null) return drawBlocked(blocked);
  if (ctx.state.readerDevice === undefined) {
    let answer = null;
    try { answer = await runEngine(argv.appSettings()); } catch { /* as none */ }
    if (host === null || !host.isConnected) return;
    ctx.state.readerDevice = D.readerDeviceFrom(answer);
  }
  mode = ctx.state.readerDevice === null ? 'cards' : 'calm';
  draw();
}

export function panelShown() { showing = true; /* Task 9: the Kindle check and build ahead */ }
export function panelHidden() { showing = false; /* Task 9/10: aheadWanted off, kfxHidden */ }

function draw() {
  const focused = host.contains(document.activeElement) ? document.activeElement : null;
  const key = focused?.dataset?.key ?? null;
  clear(body);
  if (mode === 'cards') body.append(drawCards());
  else if (mode === 'missing') body.append(missingPanel());
  else body.append(drawCalm());
  if (focused !== null) giveBackFocus(key);
}
```
Cards: an `h3.panel-question` with `D.QUESTION`, then `div.device-cards` of `button.device-card` (data-key = kind; `device-card-here` added when `here`; inside it `span.device-card-name`, `span.device-card-sub` when sub, `span.device-dot` with `aria-hidden` when here, and the asterisk wrapper for unproven cards placed AFTER the button, not inside it, since a button inside a button is invalid). The card's accessible name gets ", plugged in" via a visually hidden span when `here`. Pressing a kind card:

```js
async function choose(kind) {
  ctx.state.readerDevice = kind;
  mode = 'calm';
  draw();
  say({ line: '', bad: false });
  try {
    const answer = await runEngine(argv.readerDevice(kind));
    if (answer?.ok !== true) say({ line: R.failureMessage(answer), bad: true });
  } catch (err) {
    say({ line: err.message, bad: true });
  }
}
```
`missingPanel()` (THE SEAM PART 4 REPLACES with the name field, Send and Try again, decisions 14 and 15):

```js
/** "My device isn't here", for now: a sentence, a door to a GitHub issue
 *  (the one URL the window may already open), the EPUB as the fallback, and
 *  Back. Part 4 of the spec replaces the inside of this function with the
 *  request field and its Send. It stores nothing. */
function missingPanel() {
  return el('div', { class: 'device-missing' },
    el('p', { class: 'prose' }, D.MISSING_LINE),
    el('div', { class: 'calm-ways' },
      el('button', { type: 'button', class: 'btn btn-outline', 'data-key': 'missing-tell',
        onclick: () => openUrl(newIssueUrl({ appVersion: RELEASE.version, osVersion: osLabel(platform()),
          context: 'A reader Screepub does not list yet: ' })) }, 'Tell us on GitHub'),
      el('button', { type: 'button', class: 'btn-quiet', 'data-key': 'save-epub', onclick: () => saveFile('epub') }, 'Save the EPUB…'),
      el('button', { type: 'button', class: 'btn-quiet', 'data-key': 'missing-back', onclick: () => { mode = 'cards'; draw(); } }, 'Back')));
}
```
(`saveFile` arrives in Task 10; until then wire the button to a stub that Task 10 replaces, or land Task 10's `saveFile('epub')` minimal path now.) The asterisk helper:

```js
function asterisk(kind) {
  const said = D.asteriskNote(kind, platform());
  if (said === null) return null;
  const note = el('p', { class: 'caption asterisk-note', hidden: true }, said.replace(/ tell us\.$/, ' '),
    el('button', { type: 'button', class: 'btn-quiet', onclick: tellUs(kind) }, 'tell us'), '.');
  let pinned = false;
  const star = el('button', { type: 'button', class: 'asterisk', 'aria-expanded': 'false',
    'aria-label': `About ${D.nameOf(kind)}: never tried on real hardware`,
    onclick: () => { pinned = !pinned; note.hidden = !pinned; star.setAttribute('aria-expanded', String(pinned)); },
    onmouseenter: () => { note.hidden = false; }, onmouseleave: () => { note.hidden = !pinned; } }, '*');
  return el('span', { class: 'asterisk-wrap' }, star, note);
}
```
(`tellUs(kind)` opens `newIssueUrl` with context `Sending to a ${name} on ${osLabel(...)}: it worked / it did not work (delete one).`; part 4 swaps it for the request service.) The "the words" test in Task 4 guards the note's sentence; here the note's text is that sentence with the last two words as a button.
- [ ] **Step 4:** `bun test tests/send-panel-ui.test.ts` green, whole suite green. Commit: `send-panel.js: the reader's device, asked once`.

---

### Task 8: send-panel.js B: the calm screen and its flows

**Files:** `desktop/ui/send-panel.js`, `tests/send-panel-ui.test.ts`; `tests/desktop-ui.test.ts` ("the Send page's device send, mounted": its tests move here).

- [ ] **Step 1: Failing tests:**

```ts
describe('the calm screen', () => {
  test('a Kobo not plugged in: Save the EPUB is brass, with its line; Other ways and Different device beside it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    await settle();
    expect(p.brass()!.textContent).toBe('Save the EPUB…');
    expect(p.node.querySelector('.calm-line')!.textContent).toBe('Plug your Kobo in over USB and this becomes Copy to Kobo.');
    expect(p.node.querySelectorAll('.calm .btn-brad').length).toBe(1);
    expect(p.node.button('Other ways').className).toContain('btn-outline');
    expect(p.node.button('Different device').className).toContain('btn-quiet');
  });
  test('it asks routes quick then full, and again when what is plugged in changes, never on a timer', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kobo' } });
    expect(p.tauri.callsTo('routes').map((c) => c.args)).toEqual([[ 'routes', EPUB, '--json', '--quick' ], [ 'routes', EPUB, '--json' ]]);
    await p.plug([KOBO]);
    expect(p.tauri.callsTo('routes').length).toBe(4);
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');
    expect(p.node.querySelector('.device-dot')).not.toBeNull();
  });
  test('Copy to a Kobo: this script’s settings, export, then send, and the line says so', async () => {
    // Re-homed from desktop-ui's "a device send reads this script's settings
    // first, exports with them, then copies what was built".
  });
  test('a refused export copies nothing and says the engine’s sentence', async () => { /* re-homed */ });
  test('while a copy runs every button is dead and a plug-in redraws nothing; a new script hears nothing of it', async () => { /* re-homed */ });
  test('Send to Kindle (not plugged in): the engine opens it, no turn on the book, and the drag line is under it', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, engine: (a) => (a[0] === 'route' ? { ok: true, note: 'Opened Amazon.' } : undefined) });
    expect(p.node.querySelectorAll('.calm-line').map((n) => n.textContent)).toEqual([
      'Amazon’s Send to Kindle opens beside the book’s folder. Drag the book into it: Screepub cannot upload it for you.',
      'Or plug the Kindle in over USB and copy it across.']);
    p.brass()!.click();
    await settle();
    expect(p.tauri.callsTo('route').map((c) => c.args)).toEqual([['route', 'send-to-kindle', EPUB, '--json']]);
    expect(p.status().textContent).toBe('Opened Amazon.');
  });
  test('the keyboard goes back to the brass button after a route, found again after a redraw', async () => { /* re-homed from send-routes-ui "the keyboard goes back to the route's button after a refusal, and after a success" */ });
});
```
Fill the re-homed bodies from the originals (desktop-ui.test.ts 5721 to 5780; send-routes-ui.test.ts 2057 to 2120, 2131 to 2200, 2278 to 2330), changing only how the page is reached (`mountPanel({ state: { readerDevice }, devices })` and `p.brass()` or `p.node.button(label)` instead of the route list's rows). Delete from `desktop-ui.test.ts`'s "the Send page's device send, mounted": the reach-fold test (5841 to 5849, the table is gone); the KFX tests in it (5784 to 5838) move in Task 10.
- [ ] **Step 2:** fails. **Step 3: Implement:** move into send-panel.js, verbatim with comments, send.js's `sending`, `era` (bumped by a new `mountPanel` for a different book), `ensureSettings`, `onBook`, `sendTo`, `perform` (minus its `save-epub`/`save-kindle` branches: saves are `saveFile`, Task 10), `keepFocus`, `giveBackFocus`, `buttons()`, `say`, `asks`/`newest`/`fullOut` and `refresh()` (now asked by `drawCalm` on first draw and by `onDevices`, not by an interval), `repoll()`. `onDevices(list)`:

```js
function onDevices(list) {
  ctx.state.devices = list;
  if (sending) return;                // the copy finishes first; the next change redraws
  if (mode === 'cards') return markCards(list);   // toggles classes in place, no rebuild
  refresh({ quick: true });
  refresh();
  // Task 11: drop the Eject button when its volume is gone.
}
```
`drawCalm()` builds `div.calm`: the brass `button.btn.btn-brad[data-key=<route id>]` with `offer.brass.label`, the asterisk when `offer.unproven`, `p.caption.calm-line` per line, then `div.calm-ways` with `Other ways` (`btn btn-outline`, `aria-expanded`, Task 10 fills it) and `Different device` (`btn-quiet`). `choose(route)` is send.js's: `R.performerFor(route) === 'device' ? sendTo(route.device) : route.key === 'save-epub' ? saveFile('epub') : perform(route)`, each through `keepFocus`. After `sendTo` succeeds, the line is `D.copiedState(...)` (Task 11 adds Eject; until then use its `line`). `routeRow` from send.js survives as the Other ways row in Task 10.
- [ ] **Step 4:** whole suite green; delete the moved tests from desktop-ui's mounted block. Commit: `send-panel.js: the calm screen, and send.js's flows re-homed`.

---

### Task 9: send-panel.js C: the Kindle file, named and built ahead

**Files:** `desktop/ui/send-panel.js`, `tests/send-panel-ui.test.ts`; `tests/send-routes-ui.test.ts` (its mounted Kindle tests move here).

- [ ] **Step 1:** Move, adapted to the rig, every test in send-routes-ui.test.ts from "the buttons that hand over a Kindle file say which file" (1402) through "a save that waited behind the build and changed the book" (1882). Adaptation rules: "opening the page" is `mountPanel({ state: { readerDevice: 'kindle' } })` (a Kindle session) or `readerDevice: 'kobo'` (not one); "a Kindle plugged in meanwhile" is `p.plug([KINDLE])`; "the page hidden" is `p.panel.panelHidden()`; "Save a Kindle file" presses become `saveFile('kfx')` presses in Other ways and move in Task 10, not here; the window focus return is `p.page.fireWindow('focus')`. New tests:

```ts
  test('a Kindle chosen and the file out of date: one build starts as the result appears, with its line and bar under the brass', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE],
      check: { ok: true, path: '/x.kfx', format: 'kindle', extension: 'kfx', label: 'KFX', available: ['epub', 'kindle'], stages: [], fresh: false, builtBy: 'calibre', bookDate: 1000 } });
    await settle();
    const builds = p.tauri.callsTo('export').filter((c) => !c.args.includes('--check'));
    expect(builds.map((c) => c.args)).toEqual([['export', EPUB, '--json', '--for', 'kindle', '--fountain', FOUNTAIN, '--options-json', '{"dialogueSideMarginPct":27}']]);
    expect(p.node.querySelector('.calm .kindle-build-line')!.textContent).toBe('Building the Kindle file. This takes about half a minute.');
    expect(p.brass()!.textContent).toBe('Copy to Kindle (.kfx)');
  });
  test('a Kobo reader with no Kindle in sight never starts Kindle Previewer; a Kindle plugged in does', async () => { /* kindleSession */ });
```
- [ ] **Step 2:** fails. **Step 3:** Move from send.js, verbatim: `kindleLines`, `inKindleLine`, `onKindleFile`, `builds`, `triedFor`, `aheadWanted`, `kindle`/`kindleBook`/`labelledWith`, `checkKindle`, `maybeBuildAhead` (with `forKindle: D.kindleSession(ctx.state.readerDevice, devicesNow())` and `poll !== null` replaced by `showing`), `useKindle` (redraw the calm screen when the label changes), `buildInBackground`, `track`, `builtNow`, `showBuild` (the build node lives under the brass in `.calm`). `panelShown()` does what send.js's `show()` did for these (forget freshness, `aheadWanted = true`, `checkKindle()`, add the window focus listener); `panelHidden()` what `hide()` did. `onDevices` calls `maybeBuildAhead()` after the routes answer (a Kindle just plugged in).
- [ ] **Step 4:** whole suite green; delete the moved tests from send-routes-ui. Commit: `send-panel.js: the Kindle file, named on its button and built ahead`.

---

### Task 10: send-panel.js D: Other ways (Send it, Save a file, the KFX checklist)

**Files:** `desktop/ui/send-panel.js`, `tests/send-panel-ui.test.ts`; desktop-ui.test.ts's KFX-on-the-Send-page tests (5784 to 5838) move here.

- [ ] **Step 1: Failing tests:**

```ts
describe('Other ways', () => {
  test('opens in place, in two groups: the device’s follow-ups lead Send it, every file type is in Save a file', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' }, devices: [KINDLE] });
    const toggle = p.node.button('Other ways');
    toggle.click();
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    const groups = p.node.querySelectorAll('.ways-group');
    expect(groups.map((g) => g.querySelector('.ways-heading')!.textContent)).toEqual(['Send it', 'Save a file']);
    const sendTitles = groups[0].querySelectorAll('.device-name').map((n) => n.textContent);
    expect(sendTitles.slice(0, 2)).toEqual(['Send to Kindle web', 'Send to Kindle email']);
    expect(sendTitles).not.toContain('Save the EPUB');
    expect(groups[1].querySelectorAll('.save-row .device-name').map((n) => n.textContent)).toEqual(['EPUB', 'KFX', 'AZW3', 'MOBI', 'Fountain']);
  });
  test('a dimmed route keeps its fix and has no button; a Kobo row carries the asterisk', async () => { });
  test('Save KFX: the Save box first, named for the book, then export --for kfx --out, in the Kindle line, with the build line', async () => {
    const p = await mountPanel({ state: { readerDevice: 'kindle' } });
    p.node.button('Other ways').click();
    p.node.querySelector('.save-row[data-type="kfx"] button')!.click();
    await settle();
    const box = p.tauri.dialogs.shift()!;
    expect(box.options).toEqual({ defaultPath: 'Field Station.kfx', filters: [{ name: 'KFX', extensions: ['kfx'] }] });
    box.resolve('/Users/ann/Desktop/Field Station.kfx');
    await settle();
    expect(p.tauri.callsTo('export').filter((c) => c.args.includes('--out')).map((c) => c.args)).toEqual([[
      'export', EPUB, '--json', '--for', 'kfx', '--fountain', FOUNTAIN, '--options-json', '{"dialogueSideMarginPct":27}', '--out', '/Users/ann/Desktop/Field Station.kfx']]);
    expect(p.node.querySelector('.kindle-build-line')!.textContent).toBe('Building the Kindle file. This takes about half a minute.');
  });
  test('a cancelled Save box asks the engine nothing and says nothing', async () => { });
  test('a saved file is named on the status line; a refusal is the engine’s sentence', async () => {
    // ok answer { ok: true, path: '/Users/ann/Desktop/Field Station.epub', ... } -> 'Saved Field Station.epub.'
  });
  test('KFX greyed with Set it up when the ladder stops at AZW3; Set it up shows the checklist in place', async () => {
    // check answers extension 'azw3'; kfx-status answers a not-ready checklist
    // (the shape desktop-ui's KFX tests build with kfxSetup()); pressing
    // Set it up un-hides section.kfx-setup with its heading and rows.
  });
  test('the KFX plugin cannot be installed under a running Kindle build (re-homed)', async () => { });
  test('Show in Finder and the book’s path sit at the foot of Save a file', async () => { /* convert.js passes them in Task 12; here mountPanel(..., { footer }) */ });
});
```
- [ ] **Step 2:** fails. **Step 3: Implement.** `drawOtherWays()` builds `section.other-ways` (hidden until the toggle) with two `div.ways-group`s, each with `h3.ways-heading`. Send it rows are send.js's `routeRow` (classes `device-row`, `route-unavailable`, `route-detail`, `route-hint`, `device-where`), outline buttons, `data-key = route.id`, plus `asterisk(D.kindOfRoute(route))` for device rows; no brass anywhere in the list. Save rows: `div.save-row[data-type]` with `p.device-name` label, `p.route-detail` who, and either an outline `Save…` button, or the fix line plus, when `setup`, a quiet `Set it up` button that un-hides `section.kfx-setup` and calls `mountKfx(node, hooks)` then `kfxShown()`. Hooks: `isSending: () => sending || builds.size > 0`, `devices: () => []` (the reader asked for it: always relevant), `onBusy` disables every panel button, `restoreFocus: () => ctx.restoreFocus()`. Closing Other ways, `panelHidden()` and a remount call `kfxHidden()`. `mountPanel(node, ctx, { footer = [] } = {})` appends the footer nodes under the Save group. The save flow:

```js
/** Save one file type where the Save box says. The type is the extension, so
 *  the box comes first; the engine builds (or finds) the file and writes it
 *  there (`export --for <type> --out`). The window never writes a file. */
async function saveFile(type) {
  if (sending || kfxInstalling()) return;
  sending = true;
  const mine = era;
  const stale = () => era !== mine;
  for (const b of buttons()) b.disabled = true;
  try {
    kfxRedraw();
    await ensureSettings();
    if (stale()) return;
    const script = ctx.state.script;
    const row = D.SAVE_TYPES.find((t) => t.type === type);
    const out = await saveDialog({ defaultPath: R.saveNameFor(script.epubPath, type), filters: R.saveFiltersFor(type, row.label) });
    if (stale() || out === null) return;
    const call = argv.export(script.epubPath, {
      forFormat: type, fountain: script.fountainPath, optionsJson: R.optionsJsonFor(script), out,
    });
    const book = script.epubPath;
    let answer;
    if (D.saveTurn(type) === 'kindle') {
      say(R.statusFor('idle'));
      answer = await track(book, { extension: type }, inKindleLine(book, () => onBook(book, 'kindle', call)));
    } else {
      say(R.statusFor('saving'));
      answer = await onBook(book, 'copy', call);
    }
    if (stale()) return;
    say(answer?.ok === true
      ? { line: `Saved ${R.baseName(answer.path)}.`, bad: false }
      : R.statusFor('failed', { detail: R.failureMessage(answer) }));
  } catch (err) {
    if (!stale()) say(R.statusFor('failed', { detail: err.message }));
  } finally {
    sending = false;
    for (const b of buttons()) b.disabled = false;
    kfxRedraw();
    if (!stale()) maybeBuildAhead();
  }
}
```
(`track(book, { extension: type }, run)` reuses the build line, which already says "about half a minute" for kfx.) Update book-queue.js's TURNS comments: `kindle` and `copy` rows now name send-panel.js's flows (saves of KFX/AZW3/MOBI take `kindle`; EPUB and Fountain take `copy`).
- [ ] **Step 4:** whole suite green; delete the moved KFX tests from desktop-ui's mounted block. Commit: `send-panel.js: Other ways, every route and every file type`.

---

### Task 11: send-panel.js E: Eject, and a result that survives leaving

**Files:** `desktop/ui/send-panel.js`, `tests/send-panel-ui.test.ts`.

- [ ] **Step 1: Failing tests:**

```ts
describe('after a copy over USB', () => {
  async function copied() {
    const p = await mountPanel({ state: { readerDevice: 'kobo' }, devices: [KOBO],
      engine: (a) => (a[0] === 'export' && !a.includes('--check') ? { ok: true, path: EPUB, label: 'EPUB' }
        : a[0] === 'send' ? { ok: true, destination: '/Volumes/KOBOeReader/Field Station.epub' } : undefined) });
    await settle();
    p.brass()!.click();
    await settle();
    return p;
  }
  test('Eject <name> becomes the brass button, with the reminder', async () => {
    const p = await copied();
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
    expect(p.status().textContent).toBe('Field Station is on your KOBOeReader. Eject it before you unplug.');
  });
  test('Eject asks the engine for that volume only; done, the button goes and the line says so', async () => {
    const p = await copied();
    p.brass()!.click();
    await settle();
    const call = p.tauri.callsTo('eject')[0];
    expect(call.args).toEqual(['eject', '/Volumes/KOBOeReader', '--json']);
    call.answer({ ok: true, ejected: '/Volumes/KOBOeReader' });
    await settle();
    expect(p.status().textContent).toBe('Your KOBOeReader is ejected. You can unplug it now.');
    expect(p.brass()!.textContent).toBe('Copy to KOBOeReader');   // the poll still lists it until it goes
  });
  test('a reader that will not eject says why and keeps the button', async () => {
    const p = await copied();
    p.brass()!.click();
    await settle();
    p.tauri.callsTo('eject')[0].answer({ ok: false, error: { code: 'eject-failed', message: 'could not eject /Volumes/KOBOeReader: a file is open' } });
    await settle();
    expect(p.status().textContent).toBe('could not eject /Volumes/KOBOeReader: a file is open');
    expect(p.status().classList.contains('bad')).toBe(true);
    expect(p.brass()!.textContent).toBe('Eject KOBOeReader');
  });
  test('the line and the button are drawn again on return (spec 13)', async () => {
    const p = await copied();
    p.panel.panelHidden();
    p.panel.mountPanel(p.page.pane(), p.ctx);   // the result drawn again, same window state
    await settle();
    expect(p.ctx.state.sendResult.eject.volume).toBe('/Volumes/KOBOeReader');
  });
  test('unplugged without ejecting: the button goes, the line stays', async () => {
    const p = await copied();
    await p.plug([]);
    expect(p.node.querySelector('.calm .btn-brad')!.textContent).toBe('Save the EPUB…');
    expect(p.status().textContent).toBe('Field Station is on your KOBOeReader. Eject it before you unplug.');
  });
  test('a different book does not inherit the reminder', async () => {
    const p = await copied();
    p.ctx.state.script = { ...p.ctx.state.script, epubPath: '/lib/other/Other.epub' };
    p.panel.mountPanel(p.page.pane(), p.ctx);
    await settle();
    expect(p.node.querySelector('.send-status')?.textContent ?? '').toBe('');
  });
});
```
(The re-mount test must query the new node; write it against the node it mounted.)
- [ ] **Step 2:** fails. **Step 3: Implement.** `say()` also writes `ctx.state.sendResult = { book, line, bad, eject }` (eject preserved unless the call sets it). In `sendTo`, on `phase === 'sent'`: `const after = D.copiedState(script.title, device, sent); ctx.state.sendResult = { book, ...after }; say(after); draw();`. `drawCalm()` puts `button.btn.btn-brad[data-key=eject]` labelled `D.ejectLabel(eject)` in the brass place when `sendResult?.book === book && sendResult.eject !== null`. `ejectNow()` runs under the same `sending` flag: `say({ line: `Ejecting ${eject.name}…`, bad: false })`, `runEngine(argv.eject(eject.volume))` (counted; no book turn: it touches no book), then `D.ejectOutcome`, store, redraw. `mountPanel` draws `sendResult` when its `book` is this script's epubPath, and clears it otherwise. `onDevices` drops `eject` (keeps the line) when `!D.ejectStillThere(eject, list)`.
- [ ] **Step 4:** whole suite green. Commit: `send-panel.js: Eject after a USB copy, kept when the page is left`.

---

### Task 12: Convert's result: a page and a panel

**Files:** `desktop/ui/convert.js`, Create `desktop/ui/result-page.js`, `tests/desktop-ui.test.ts` ("the Convert surface", "what the Convert surface decides", the booted window's "cancelling a file dialog over a result" test).

- [ ] **Step 1: Failing tests** (in "the Convert surface", using `mountConvert`):

```ts
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
  });
  test('showResult draws the open book’s result again after Convert another, without converting', async () => { });
  test('showResult on the result already up redraws nothing, so a copy in progress keeps its buttons', async () => { });
```
In "what the Convert surface decides": replace `countLine`'s tests with `countsLine` (`{pages:19, scenes:11}` -> `'19 pages, 11 scenes'`; one page -> `'1 page'`; scenes missing -> `'19 pages'`; nothing -> `''`) and add `readyLine('Field Station') === 'Field Station is ready'`. The Show in Finder test (543) presses it inside Other ways now: `pane.button('Other ways').click()` first. The booted-window test at 9092 expects the first card instead: `w.surface('convert').button('Kindle')` (the boot engine answers `app-settings` with a refusal, so readerDevice is none and the cards show).
- [ ] **Step 2:** fails. **Step 3: Implement.** `result-page.js`:

```js
// The left half of the result: the book's first page, in its own
// typesetting (spec decision 2). For now a static preview: the engine's
// preview document in a frame dressed exactly as the reader's (read.js's
// dressFrame), scrolled to the first scene and not scrollable. Part 3 of the
// spec (the page writes itself) replaces what is INSIDE .result-page with the
// typed page and its wave; the box, its size and its place stay.
import { el } from './dom.js';
import { dressFrame, splitPreview } from './read.js';

export function resultPage(script) {
  const box = el('div', { class: 'result-page', 'aria-hidden': 'true' });
  const html = typeof script?.previewHtml === 'string' ? script.previewHtml : '';
  if (html === '') return box;
  const { css, html: doc } = splitPreview(html, new DOMParser());
  const frame = el('iframe', { class: 'result-page-frame', title: 'First page', sandbox: 'allow-same-origin', tabindex: '-1', scrolling: 'no' });
  frame.addEventListener('load', () => {
    dressFrame(frame, css);
    frame.contentDocument?.querySelector('section.scene')?.scrollIntoView({ block: 'start' });
  });
  frame.srcdoc = doc;
  box.append(frame);
  return box;
}
```
(Read `read.js`'s `render()` to copy exactly how it feeds the frame: `splitPreview`'s return shape and whether it uses `srcdoc` or a blob; match it rather than this sketch.) In convert.js: `countsLine(answer)` and `readyLine(title)` replace `countLine`; `drawResult(path, answer)` sets `ctx.state.script = scriptFrom(...)`, clears `ctx.state.sendResult` when its book is not this one, then `paintResult()`, `ctx.scriptChanged()`, `ctx.restoreFocus()`. `paintResult()`:

```js
function paintResult() {
  const script = ctx.state.script;
  clear(pane);
  pane.dataset.state = 'done';
  pane.dataset.book = script.epubPath ?? '';
  delete pane.dataset.errorCode;
  const note = el('p', { class: 'caption', hidden: true }, '');
  const reveal = el('button', { type: 'button', class: 'btn-quiet', onclick: async () => {
    const said = await revealNote(runEngine, script.epubPath);
    text(note, said);
    note.hidden = said === '';
  } }, revealLabel(navigator.userAgentData?.platform ?? navigator.platform));
  const panelNode = el('div', { class: 'send-panel' });
  pane.append(el('div', { class: 'result' },
    resultPage(script),
    el('div', { class: 'result-side' },
      el('h2', { class: 'result-title' }, readyLine(script.title)),
      el('p', { class: 'book-count' }, countsLine(script)),
      ...script.warnings.map((w) => el('p', { class: 'caption' }, `(${w})`)),
      panelNode,
      el('div', { class: 'asides' },
        el('button', { type: 'button', class: 'btn-quiet', onclick: () => ctx.goTo('read') }, 'Read it'),
        el('button', { type: 'button', class: 'btn-quiet', onclick: () => ctx.goTo('tune') }, 'Settings'),
        el('button', { type: 'button', class: 'btn-quiet', onclick: reset }, 'Convert another')))));
  mountPanel(panelNode, ctx, { footer: [reveal, el('p', { class: 'path-note' }, script.epubPath), note] });
  if (shown) panelShown();
}

/** The open book's result, drawn again if the well (or a refusal) replaced
 *  it: the Read and Settings pages' Send button, and the heads-up. */
export function showResult() {
  const script = ctx.state.script;
  if (busy || script === null) return;
  if (pane.dataset.state === 'done' && pane.dataset.book === (script.epubPath ?? '')) return;
  paintResult();
}

/** The heads-up's Copy: the result, then a copy to that reader. */
export function copyTo(device) {
  showResult();
  panelCopyTo(device);
}
```
(`panelCopyTo` is send-panel.js's exported `copyTo(device)`: `sendTo(device)` with a device the watcher listed; it needs no route row.) `show()` adds `if (pane.dataset.state === 'done') panelShown();` and a new `hide()` calls `panelHidden()`. `reset()` and `drawFailure` call `panelHidden()` before clearing. `countsLine(script)` reads `pages`/`scenes` off the script (scriptFrom keeps them). Update convert.js's comments that named the Send page. CSS for `.result` lands in Task 15; until then the measured test still passes because `#surface-convert` keeps its 820 column.
- [ ] **Step 4:** whole suite green (the Send tab still exists and still works). Commit: `Convert's result: the first page beside the send panel`.

---

### Task 13: the Send tab goes; Read and Settings send from the result

**Files:** `desktop/ui/frame.js`, `desktop/ui/main.js`, `desktop/ui/read.js`, `desktop/ui/tune.js`, DELETE `desktop/ui/send.js`; `desktop/ui/kfx.js`, `desktop/ui/app.js`, `desktop/ui/book-queue.js` (comments); `tests/desktop-ui.test.ts`, DELETE the mounted half of `tests/send-routes-ui.test.ts` (whatever Tasks 8 to 11 did not already move).

- [ ] **Step 1: Failing tests** (booted window, `bootWindow` in desktop-ui's "the window, booted whole"):

```ts
  test('the three surfaces are the bar’s panels, and the release notes a sheet off the stamp', async () => {
    // was "the four surfaces": ['convert', 'read', 'tune'], and no #surface-send or #tab-send at all
    expect(w.doc.getElementById('tab-send')).toBeNull();
    expect(w.doc.getElementById('surface-send')).toBeNull();
  });
  test('Read and Settings are off the bar until a script converts', async () => { /* ['convert','read','tune'] */ });
  test('Read’s Send goes back to the result, drawn again if the well replaced it, with the keyboard on the panel', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    await open(w, '/s/Field Station.pdf');
    w.surface('convert').button('Convert another').click();
    w.tab('read').click();
    w.surface('read').button('Send').click();
    await settle();
    expect(w.showing()).toBe('convert');
    expect(w.surface('convert').dataset.state).toBe('done');
    expect(w.doc.activeElement).toBe(w.surface('convert').button('Kindle'));
    expect(conversions(w).length).toBe(1);
  });
  test('Settings has the same Send', async () => { });
  test('the devices poll runs once a book is open, and not before', async () => {
    w = await bootWindow(SCRATCH, { respond: engine });
    expect(w.tauri.callsTo('devices')).toEqual([]);
    await open(w, '/s/Field Station.pdf');
    expect(w.tauri.callsTo('devices').map((c) => c.args)).toEqual([['devices', '--json', '--quick']]);
  });
```
(The boot engine holds `devices`; answer it where a test needs it. The 2 s interval: bootWindow cannot set timers before main.js imports, so add an optional `timers` to `bootWindow`'s options that installs `setInterval`/`clearInterval` stand-ins through `withGlobals` before the import, and use it where a tick is needed.) Change: frame `SURFACES` test to `['convert', 'read', 'tune']`; the tablist keyboard test (7950 to 7990) to three tabs (enable read, disable tune: ArrowRight from convert -> read -> convert; End -> read; the tabIndex table `[['convert',-1,false],['read',0,false],['tune',-1,true]]`); focus's `SURFACES` list (1681) to three; the "no null or undefined on a blank screen" test (8960) drops its Send pane and mounts the send panel instead for a script with no book (`blockedReason` line plus Convert it again); "Send's 'no book to send' screen converts again" (1100) becomes the same check on the panel. The three existing source pins on send.js: the "holds none of the engine's knowledge" pin is re-pointed at `send-panel.js`, `send-routes.js` and `device.js` (an existing rule, kept); the "a redraw that throws cannot leave a send stuck on" pin (8515) is re-pointed at send-panel.js's `sendTo`; the `readsTheBook` and nothing-connected tests already import send-routes.js. Delete "the Send page's rows are dressed by the stylesheet"'s reference to nothing (keep its CSS checks; the classes are still drawn in Other ways).
- [ ] **Step 2:** fails. **Step 3: Implement.** frame.js: remove the Send entry from `SURFACES` and fix its comment. main.js:

```js
import * as convert from './convert.js';
import * as read from './read.js';
import * as tune from './tune.js';
import { createWatcher } from './device-watch.js';
import { mountHeadsUp } from './heads-up.js';   // Task 14; land the import there

const watcher = createWatcher({ run: runEngine });
watcher.subscribe((list) => { state.devices = list; });

const surfaces = { convert, read, tune };
const NEEDS_SCRIPT = ['read', 'tune'];
// state gains readerDevice: undefined, sendResult: null

const context = {
  // ...as before, plus:
  devices: { current: () => watcher.current(), subscribe: (fn) => watcher.subscribe(fn) },
  /** Read's and Settings' Send: the open book's result, on Convert, with the
   *  keyboard on its first control. */
  sendBook() {
    convert.showResult();
    if (showing === 'convert') restoreFocus();
    else frame.setSurface('convert');
  },
  copyTo(device) {
    frame.setSurface('convert');
    convert.copyTo(device);
  },
  scriptChanged() {
    const open = state.script !== null;
    for (const id of NEEDS_SCRIPT) frame.enable(id, open);
    if (open) watcher.start(); else watcher.stop();
    for (const surface of Object.values(surfaces)) surface.scriptChanged?.();
  },
};
```
read.js: in `read-head`, after the index toggle, `el('button', { type: 'button', class: 'btn btn-outline read-send', onclick: () => ctx.sendBook() }, SEND_BOOK)` (import `SEND_BOOK` from device.js). tune.js: wrap the slug in `div.tune-head` with the same button (`class: 'btn btn-outline tune-send'`). Delete `desktop/ui/send.js`. Comments: kfx.js ("the Send page" -> "the send panel's Other ways", "send.js" -> "send-panel.js"), app.js's countsTowardBusy and retireEngine comments, book-queue.js's header, convert.js's `convertAgain` doc (no "Send's no-book screen"; it is the panel's). Then `grep -rn "send\.js\|Send page\|Send tab" desktop/ui tests` and fix every hit that is not history.
- [ ] **Step 4:** whole suite green. Commit: `the Send tab goes: Read and Settings send from Convert's result`.

---

### Task 14: the Kindle heads-up

**Files:** Create `desktop/ui/heads-up.js`; `desktop/ui/main.js`; `tests/desktop-ui.test.ts` (booted window).

- [ ] **Step 1: Failing tests** (booted, with the timers option from Task 13 so the 8 s and the poll are driven by hand):

```ts
  test('a Kindle plugged in while a book is open: one line at the top of the page, Copy or Not now', async () => {
    w = await bootWindow(SCRATCH, { respond: engine, timers: true });
    await open(w, '/s/Field Station.pdf');
    w.tab('read').click();
    w.tauri.callsTo('devices')[0].answer({ ok: true, devices: [] });       // seeds
    await settle();
    w.tickInterval();                                                        // the next poll
    w.tauri.callsTo('devices')[1].answer({ ok: true, devices: [{ id: '/Volumes/Kindle', kind: 'kindle', name: 'Kindle', volume: '/Volumes/Kindle' }] });
    await settle();
    const line = w.doc.querySelector('.heads-up')!;
    expect(line.hidden).toBe(false);
    expect(line.querySelector('.heads-up-line')!.textContent).toBe('Kindle plugged in. Copy Field Station?');
    expect(line.getAttribute('role')).toBe('status');
    // Never sends without a press.
    expect(w.tauri.callsTo('send')).toEqual([]);
  });
  test('Copy goes to the result and copies to that Kindle', async () => { /* export --for kindle, then send --device /Volumes/Kindle */ });
  test('Not now, or eight seconds, takes it away; the same Kindle does not bring it back until it is unplugged', async () => { });
  test('not on the result screen, where the card and the brass already say it', async () => { });
  test('a Kindle already plugged in when the book opened is not news', async () => { });
```
- [ ] **Step 2:** fails. **Step 3: Implement** `heads-up.js`:

```js
// "Kindle plugged in. Copy <Title>?" (spec decision 16): a short line that
// slides in at the top of whatever page is showing, once per plug-in, and
// goes by itself after eight seconds. It never sends without a press. The
// rule is device.js's headsUpFor; this only draws it.
import { el, text } from './dom.js';
import * as D from './device.js';

export function mountHeadsUp(sheet, { watcher, state, onResult, copyTo, timers = globalThis }) {
  const line = el('span', { class: 'heads-up-line' });
  let offered = null;
  let timer = null;
  let seen = null;
  const close = () => { box.hidden = true; offered = null; timers.clearTimeout(timer); timer = null; };
  const box = el('div', { class: 'heads-up', role: 'status', hidden: true },
    line,
    el('button', { type: 'button', class: 'btn btn-brad', onclick: () => { const d = offered; close(); if (d) copyTo(d); } }, D.HEADS_UP_COPY),
    el('button', { type: 'button', class: 'btn-quiet', onclick: close }, D.HEADS_UP_LATER));
  sheet.prepend(box);   // under the tablist: insert after it, see frame.sheet's children
  watcher.subscribe((devices) => {
    const next = D.headsUpFor(seen, devices, { open: state.script !== null, onResult: onResult() });
    seen = next.seen;
    if (next.show === null) return;
    offered = next.show;
    text(line, D.headsUpLine(state.script.title));
    box.hidden = false;
    timers.clearTimeout(timer);
    timer = timers.setTimeout(close, D.HEADS_UP_MS);
  });
  return { close };
}
```
Insert it right after the tablist (`frame.sheet.firstChild.after(box)` or give mountFrame an `insertTop(node)`). In main.js: `mountHeadsUp(frame.sheet, { watcher, state, onResult: () => showing === 'convert' && panes.convert.dataset.state === 'done', copyTo: (d) => context.copyTo(d) })`; when the watcher stops (no script), `seen` must reset: have `mountHeadsUp` reset `seen = null` when it hears `null` (the watcher's `stop()` publishes nothing, so main.js calls the returned `reset()` in `scriptChanged` when the script goes; add `reset` to the returned object and a test for "a new book seeds again"). Suppressed while a conversion runs: `onResult` false but `convert.isBusy()`... add `busy: () => convert.converting()` (export a tiny getter) and pass `open: state.script !== null && !busy()`.
- [ ] **Step 4:** whole suite green. Commit: `the Kindle heads-up, on whatever page is showing`.

---

### Task 15: the stylesheet, a colour token, and the measured layout

**Files:** `brand/tokens.json`, `desktop/ui/tokens.css` (generated), `desktop/ui/surfaces.css`, `tests/desktop-layout-measured.test.ts`, `tests/desktop-layout.test.ts` (CSS pins only where a rule's presence is the point and a browser is not needed).

- [ ] **Step 1: The token.** Add to `brand/tokens.json` colours: `"present": { "light": "#3E7D4A", "dark": "#6FBF7E", "from": "web", "role": "the dot on a reader the computer can see" }` (not text, so no contrast pair). Run `bun tools/build-desktop-tokens.ts`; `bun test tests/brand-tokens.test.ts` (and whatever pins the generator) green.
- [ ] **Step 2: Failing measurements.** In `tests/desktop-layout-measured.test.ts`, after the existing per-size loop, measure the result twice per size: the cards (first time) and the calm screen (after a press of the Kobo card, which writes `readerDevice` into the run's scratch config, allowed by Task 6's gate; Kobo, not Kindle, so no Kindle build starts and no unallowed `export` is asked). Restructure `beforeAll`: loop sizes for cards; click `#surface-convert .device-card[data-key="kobo"]`; wait for `.calm .btn-brad`; loop sizes for calm. Constants and tests:

```ts
const RESULT_PAGE = 400;   // the first page's column
const RESULT_PANEL = 620;  // the panel's cap

interface ResultBox { result: Box; page: Box; side: Box; brass: Box | null; cards: Box[]; overflowX: boolean }
// measured with:
//   result: box(#surface-convert .result), page: box(.result-page), side: box(.result-side),
//   brass: .calm .btn-brad or null, cards: [...document.querySelectorAll('.device-card')].map(box),
//   overflowX: document.documentElement.scrollWidth > window.innerWidth

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
} else {
  test('the result under the fold: the panel first, the page under it, both centred', () => {
    for (const r of [get(size.name).cards, get(size.name).calm]) {
      expect(r.side.bottom).toBeLessThanOrEqual(r.page.top + PX);
      near(r.side.left + r.side.width / 2, sheetCentre, 'panel centre');
      near(r.page.left + r.page.width / 2, sheetCentre, 'page centre');
    }
  });
}
test('nothing on the result runs off the side, and every card and the brass button sit inside the panel', () => {
  for (const r of [get(size.name).cards, get(size.name).calm]) {
    expect(r.overflowX).toBe(false);
    for (const c of r.cards) { expect(c.left).toBeGreaterThanOrEqual(r.side.left - PX); expect(c.right).toBeLessThanOrEqual(r.side.right + PX); }
  }
  const brass = get(size.name).calm.brass!;
  expect(brass.right).toBeLessThanOrEqual(get(size.name).calm.side.right + PX);
});
```
The existing "Convert keeps a column of at most 820" test measured the result; change it to measure the drop well's column at each size before the drop (add a `well` measurement taken while idle: navigate with `?shot=drop` is a separate page load, so instead measure `#surface-convert` after pressing Convert another, which draws the well with the book still open, then `ctx`-free return via the Read tab's Send; or simply measure `.result` only and leave the 820 well to `desktop-layout.test.ts`'s CSS pin, saying so in a comment). Pick the second; it is simpler and the pin exists.
- [ ] **Step 3: CSS** in `surfaces.css` (tokens only, no raw px font sizes or radii, reduced-motion escape for the heads-up's slide):

```css
/* The result, since 2026-10-01: the book's first page beside the send
 * panel. It takes the block, not Convert's 820 column. */
#surface-convert[data-state="done"] { max-width: none; }
.result {
  display: grid;
  grid-template-columns: 400px minmax(0, 620px);
  grid-template-areas: "page side";
  justify-content: center;
  column-gap: var(--space-7);
  margin-top: var(--space-8);
}
.result-page { grid-area: page; aspect-ratio: 3 / 4; border: 1px solid var(--hole); background: var(--panel); overflow: hidden; }
.result-page-frame { width: 100%; height: 100%; border: 0; pointer-events: none; }
.result-side { grid-area: side; display: flex; flex-direction: column; gap: var(--space-4); }
@container (max-width: 879px) {
  .result { grid-template-columns: minmax(0, 620px); grid-template-areas: "side" "page"; row-gap: var(--space-7); }
  .result-page { width: 400px; max-width: 100%; justify-self: center; }
}
.device-cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); gap: var(--space-3); }
.device-card { /* outline card: ink border, paper fill, left-aligned name and sub */ }
.device-card-here { border-color: var(--brass); box-shadow: inset 3px 0 0 var(--brass); }
.device-dot { width: 0.5rem; height: 0.5rem; border-radius: 50%; background: var(--present); }
.calm { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-3); }
.calm-ways { display: flex; flex-wrap: wrap; gap: var(--space-4); align-items: center; }
.other-ways { margin-top: var(--space-5); }
.ways-heading { /* the state-label face */ }
.save-row { /* same grid as .device-row */ }
.asterisk { /* btn-quiet face, brass, superscript-sized */ }
.asterisk-note { max-width: var(--measure); }
.heads-up { display: flex; gap: var(--space-4); align-items: center; padding: var(--space-3) var(--space-4); background: var(--panel); border-bottom: 2px solid var(--brass); animation: heads-up-in var(--motion-state) var(--ease); }
@keyframes heads-up-in { from { transform: translateY(-100%); opacity: 0; } to { transform: none; opacity: 1; } }
@media (prefers-reduced-motion: reduce) { .heads-up { animation: none; } }
```
Fill the commented rules from the existing `.device-row`, `.state-label`, `.btn-quiet` rules so the panel speaks the house's type. Remove the now-unused `.reach*`, `.reader-row`, `.reader-status`, `.reader-untested`, `.reader-note`, `.reader-route`, `.send` (old result column) and `.announce` rules, and any CSS pin in `desktop-layout.test.ts`/`desktop-ui.test.ts` that named them (search first; a pin on a deleted class is deleted with its reason in the commit message).
- [ ] **Step 4:** `bun test tests/desktop-layout-measured.test.ts` (needs Chrome; on this Mac it runs) and the whole suite green. Commit: `the result screen's layout, measured at 900, 1280 and 1800`.

---

### Task 16: the words around it

**Files:** `README.md` (lines 48, 69, 149), `desktop/README.md` (the module table: drop send.js; add send-routes.js, device.js, device-watch.js, send-panel.js, result-page.js, heads-up.js; kfx.js "the send panel's KFX checklist"), `docs/mac-qa.md` (section 6 "Try to send to a reader": "Convert a script; the result asks what you read on", and the Eject step; leave passed history entries alone).

- [ ] **Step 1:** Edit. README picture alt text: "its title, its first page, and the question of what you read on". Do NOT retake the pictures here (the release skill does); note in the commit that `assets/screens/result-*.png` are now stale.
- [ ] **Step 2:** `grep -rn "Send page\|Send tab\|Send to a reader" README.md desktop docs/mac-qa.md docs/developers.md` shows only history. `bun test` green (the README tests, if any, pin picture names, not alt text; check). Commit: `docs: sending starts on the result screen`.

---

### Task 17: look at it live

The tests prove the rules; only a browser shows whether the panel reads right and whether a plug-in feels right. Scratch harness, NOT in the repo.

**Files:** Create (scratch) `<scratchpad>/harness/serve.ts`, from the layout pass's Task 7 harness (`docs/superpowers/plans/2026-09-29-window-layout-pass.md`, lines 995 to 1070), with these changes:

- env adds `SCREEPUB_VOLUME_ROOTS: join(STATE, 'volumes')` (created empty) and `SCREEPUB_REMARKABLE_ENDPOINT: 'http://127.0.0.1:9'`, beside the scratch `SCREEPUB_CONFIG_DIR`, `SCREEPUB_LIBRARY`, `CALIBRE_CONFIG_DIRECTORY`.
- The `/engine` handler NEVER spawns `eject`, `route` or `kfx-install`. It answers them itself:
  - `eject <v>`: the JSON in `<state>/eject.json` if present, else `{"ok":true,"ejected":"<v>"}`.
  - `route <key> ...`: `{"ok":true,"note":"(harness) would open <key>."}`.
  - `kfx-install`: `{"ok":false,"error":{"code":"internal","message":"(harness) not installing"}}`.
- The bridge adds `dialog: { async save(o) { return STATE_SAVED + '/' + o.defaultPath; }, async open() { return null; } }` (STATE_SAVED a scratch folder, its absolute path inlined) and `opener: { async openUrl(u) { console.log('openUrl', u); } }`.

- [ ] **Step 1:** Start it in the background: `bun <scratchpad>/harness/serve.ts "$PWD" 4817`. Open `http://127.0.0.1:4817/` with the browser pane (`preview_start` with the url). Choose PDF; wait for the result.
- [ ] **Step 2: First time.** The question and six cards; nothing marked. `mkdir -p <state>/volumes/Kobo/.kobo` (a fake Kobo volume): within about 2 s the Kobo card's edge goes brass with a green dot. Remove it.
- [ ] **Step 3: Kindle, not plugged in.** Press Kindle. Calm screen: brass "Send to Kindle web" (or app), the drag line, the plug-in line; Other ways and Different device. Press the brass: status says the harness note. Open Other ways: Send it starts with email; Save a file has five rows with their lines; KFX greyed with Set it up if this Mac has no full KFX toolchain (press it: the checklist appears in place).
- [ ] **Step 4: Plug a Kindle in** (`mkdir -p <state>/volumes/Kindle/documents`). Within about 2 s: the brass becomes "Copy to Kindle (.<ext>)", the build line appears if Calibre is here (it runs against the scratch Calibre config). Go to Read: the heads-up slides in at the top ("Kindle plugged in. Copy Field Station?") and leaves after 8 s. Unplug (`rm -rf` the folder) and plug again: it comes back once. Press Copy it: Convert, the copy, then "Field Station is on your Kindle. Eject it before you unplug." with brass "Eject Kindle". The file is in `<state>/volumes/Kindle/documents/`.
- [ ] **Step 5: Eject.** Go to Settings and back via its Send button: the line and Eject are still there. Write `{"ok":false,"error":{"code":"eject-failed","message":"could not eject: a file is open"}}` to `<state>/eject.json`; press Eject: the message in alarm ink, button kept. Delete the file; press again: "Your Kindle is ejected. You can unplug it now." Remove the volume folder: nothing else changes.
- [ ] **Step 6: Widths and themes.** At 1280x800, 1800x1000 and 900x800, light then dark (`resize_window`), screenshot the cards and the calm screen with Other ways open. Check: page and panel side by side and centred at 1280/1800, panel above page at 900; nothing runs off the side; the asterisk note readable; Tab walks cards, brass, Other ways, Different device with a visible ring; the heads-up does not cover the tabs. Reset with preset desktop.
- [ ] **Step 7:** Run `bun tools/capture-screens.ts`: it finishes with no refused engine call. Then `git checkout -- assets/screens site/img` (pictures are retaken at release). Stop the harness. Commit any fix with its test first; nothing else changes in the repo in this task.

---

### Task 18: verify the whole

- [ ] `bun test` from the root: green. `bunx tsc --noEmit`: clean.
- [ ] `grep -rn $'—' desktop/ui/*.js` shows no new em dash (compare with `git diff main -- desktop/ui | grep $'^+.*—'`: nothing).
- [ ] `grep -rn "send\.js" --exclude-dir=node_modules --exclude-dir=.git . | grep -v "docs/superpowers\|docs/history\|docs/retired-coverage"`: nothing.
- [ ] `git status`: only intended files; nothing in `assets/screens` or `site/img`.
- [ ] Hand to review (superpowers:requesting-code-review). Do not push or merge.
