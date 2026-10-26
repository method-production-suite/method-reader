# Design: the KFX install surface (parity piece D)

Date: 2026-09-23 · Status: accepted (the owner, 2026-09-23: "add the permissions
that's fine. make it easy to use.")
Piece D of [the parity plan](../plans/2026-09-21-parity.md).
Evidence: [the parity audit](https://github.com/method-production-suite/method-reader/blob/7eb40e5/docs/parity-audit.md), row "KFX plugin install:
written but unreachable".

## The problem

A Kindle gets its best rendering from a KFX file. Making one needs three
things on the computer: Calibre, Amazon's Kindle Previewer, and jhowell's
free KFX Output plugin inside Calibre. Without all three, the export ladder
quietly falls back to AZW3 (Calibre alone) or the engine's own MOBI.

`installKfxPlugin` in `src/export/kfx.ts` already installs the plugin from
Calibre's own plugin index, and it has no callers. The Swift app had a
"Best Kindle quality (KFX)" section with three rows and an "Install plugin"
button. The Tauri window has nothing, so a user can neither see why their
Kindle gets AZW3 nor fix it.

## What was measured

- `kfxStatus()` on the owner's Mac: all three present, plugin 2.20.1,
  `ready: true`, 0.5 s wall clock. So the probe is cheap enough to run each
  time the Send page opens, and the install surface will NOT appear on the
  owner's machine.
- The Swift app's download links, `https://calibre-ebook.com/download_osx`
  and `https://kdp.amazon.com/en_US/help/topic/G202131170`, both answer 200
  today; `https://calibre-ebook.com/download_windows` does too. The Amazon
  page's title is "Kindle Previewer" and it carries both the Mac and the
  Windows download.
- `tauri-plugin-opener` 2.5.5 checks `open_url` against the scope with
  `glob::Pattern::matches` on the RAW string (`src/commands.rs:36`,
  `src/scope.rs:82`). So an exact URL in the allow list matches exactly that
  string, and a `*` would also match `/`. Exact URLs are the narrowest grant
  and are what this piece uses.

## Decisions

1. **The engine owns the checklist.** A new pure function turns a
   `KfxStatus` plus the platform into the three steps, each with its fix,
   and one summary sentence. The CLI prints it and the window draws it, so
   the two cannot tell different stories. This follows the route-list
   precedent (decision 25 of the UI pass spec): what to offer is a decision,
   and decisions live in `src/`.
2. **Two new verbs, no new flags.** `screepub kfx-status` and
   `screepub kfx-install`. Hyphenated noun-verb, like `update-decision`.
   Neither takes a positional or any other verb's flag. A flag like
   `kfx --install` would have gone into the schema every verb shares and
   made every other verb grow a rejection for it.
3. **Install only on an explicit request.** It writes to the user's Calibre
   and fetches third-party code, so nothing calls it on its own: the CLI
   verb and the window's button are the only callers.
4. **Links get scoped grants, approved by the owner on 2026-09-23.** Three exact
   URLs join the existing `opener:allow-open-url` list. No wildcard.
5. **The window shows the block only when it helps.** Details below.

## The engine

### `src/export/kfx-setup.ts` (new, pure)

```ts
export type KfxFix =
  | { kind: 'link'; label: string; url: string }   // get it from this page
  | { kind: 'install'; label: string }             // Screepub can do it
  | { kind: 'after'; why: string }                 // needs another step first
  | { kind: 'unavailable'; why: string };          // not made for this system

export interface KfxStep {
  id: 'calibre' | 'previewer' | 'plugin';
  name: string;        // 'Calibre' | 'Kindle Previewer' | 'KFX plugin'
  installed: boolean;
  fix: KfxFix | null;  // null exactly when installed
}

export interface KfxSetup {
  ready: boolean;      // status.ready, passed through
  possible: boolean;   // false where Amazon makes no Kindle Previewer
  summary: string;     // one sentence, for people
  steps: KfxStep[];    // always three, in this order
}

export function kfxSetup(status: KfxStatus, platform: string): KfxSetup;
export const KFX_LINKS: readonly string[];  // every URL kfxSetup can emit
```

Rules:

- `possible` is `platform === 'darwin' || platform === 'win32'`, the same
  two platforms `previewerPath()` has a branch for.
- Calibre missing: `{ kind: 'link', label: 'Get Calibre', url }`, where the
  URL is `https://calibre-ebook.com/download_osx` on darwin and
  `https://calibre-ebook.com/download_windows` on win32. Elsewhere,
  `https://calibre-ebook.com/download` (never drawn by the window, since
  `possible` is false there, but the CLI prints it).
- Previewer missing: on a possible platform, `{ kind: 'link', label: 'Get
  Kindle Previewer', url: 'https://kdp.amazon.com/en_US/help/topic/G202131170' }`.
  Elsewhere `{ kind: 'unavailable', why: 'Amazon does not make it for
  Linux' }` (or "for this system" when the platform is not linux).
- Plugin missing with Calibre present: `{ kind: 'install', label: 'Install' }`.
  Plugin missing without Calibre: `{ kind: 'after', why: 'Install Calibre
  first' }`. `pluginInstalled` is only meaningful with Calibre, so the step
  reads `installed: false` whenever Calibre is missing. Where `possible` is
  false, a missing plugin is `{ kind: 'unavailable', why: 'Of no use without
  Kindle Previewer' }` instead, whatever Calibre says: installing it there
  would fetch code that can never run.
- `summary`, in this order of precedence:
  - ready: `Kindles get KFX, the best quality Screepub can make.`
  - not possible: `Amazon does not make Kindle Previewer for Linux, so
    Kindles get AZW3.` (MOBI without Calibre; "for this system" off Linux)
  - otherwise: `Kindles get AZW3 for now. KFX looks better, and needs the
    three free tools below.` (MOBI without Calibre)

  The AZW3/MOBI word comes from `fileExtension('kindle', ...)` in
  `src/export/formats.ts`, so it is the ladder's own answer, not a second
  copy of it.
- No em dashes in any string (house rule for copy).

### `src/cli-kfx.ts` (new)

Handlers return values and never print, like `cli-export.ts`.

```ts
export interface KfxCommandDeps {
  status?: () => Promise<KfxStatus>;          // default kfxStatus
  install?: () => Promise<KfxInstallResult>;  // default installKfxPlugin
  platform?: string;                          // default process.platform
}
export async function kfxStatusCommand(deps?: KfxCommandDeps): Promise<KfxSetup>;
export async function kfxInstallCommand(deps?: KfxCommandDeps):
  Promise<{ version: string; removed: string[]; setup: KfxSetup }>;
```

Named `KfxCommandDeps`, not `KfxDeps`: `src/export/kfx.ts` already exports a
different `KfxDeps` (`toKfx`'s seams), and an auto-import could pick the
wrong one.

`kfxInstallCommand` runs the installer, throws
`CliError('kfx-install-failed', reason)` on `{ ok: false }`, and on success
probes the status AGAIN so the answer carries the fresh checklist. The
window then needs no second call.

Before calling the installer at all, it refuses the same way where
`kfxPossible(platform)` is false (Amazon ships no Kindle Previewer, so the
plugin would have nothing to drive), reusing `kfx-setup.ts`'s
Linux/this-system wording.

When the plugin index or the download cannot be reached, the installer
reports `Calibre's plugin site could not be reached. Check the internet
connection, then try again.` rather than Python's own exception text
(`src/export/kfx.ts` marks both fetches, and only the fetches: the zip's
URL is built from the index entry before the marked download, so an entry
with no file is reported as the error it is, not as being offline).
`kfx-install` prefixes it with `could not install the KFX plugin: `, which
is why the reason does not also open with "could not".

Every failure up to the fork-clearing step leaves the reader's Calibre as it
was. After it, one can still happen: `add_plugin` failing once an older copy
is already gone. So both of the installer's failure lines carry `removed`,
and when it is not empty `kfxInstallCommand` ends its message with ` An older
copy was removed before the failure: <names>. Try again, or reinstall it in
Calibre.` (a full stop goes in first when the reason does not end a sentence).
The message is the whole of what the terminal and the window's status line
show, so the names travel in it.

### The verbs, in `src/cli.ts` and `src/cli-devices.ts`

- `VERBS` gains `'kfx-status'` and `'kfx-install'`.
- `JsonError['code']` gains `'kfx-install-failed'`.
- Both verbs refuse every other verb's flags and any positional, BEFORE
  doing anything, with the existing `"<verb> takes no --x (--x belongs to
  y)"` wording. Refusing before acting matters most for `kfx-install`.
- JSON:
  - `kfx-status --json` → `{ "ok": true, ...KfxSetup }`
  - `kfx-install --json` → `{ "ok": true, "version", "removed", "setup" }`,
    or `{ "ok": false, "error": { "code": "kfx-install-failed", "message" } }`
- Human output for `kfx-status`: the summary, then one line per step
  (`installed`, or the fix: the URL for a link, `run screepub kfx-install`
  for install, the `why` otherwise). For `kfx-install`: `installed the KFX
  plugin <version>`, one `removed an older copy: <name>` line per removed
  fork, then the full checklist, the same lines `kfx-status` prints. The
  summary alone would end on "...needs the three free tools below" with
  nothing below it on a machine still missing Kindle Previewer. Before the
  download, `installing the KFX plugin from Calibre's plugin index...` goes
  to stderr, and only where `kfxPossible` is true: elsewhere the refusal
  comes first and that line would stand over it.
- `USAGE` lists both commands; each verb has its own `--help`.

## The window

### `desktop/ui/app.js`

Two argv builders, and nothing else in that file:

```js
kfxStatus: () => ['kfx-status', '--json'],
kfxInstall: () => ['kfx-install', '--json'],
```

### `desktop/ui/kfx.js` (new)

Same two halves as `send.js`: pure exported decisions above the line,
tested directly, and drawing below it.

Decisions:

- `setupFrom(answer)`: the engine's answer as a `KfxSetup`, or `null` when
  `ok !== true` or the shape is wrong. A malformed answer draws nothing:
  this block is advice, and a broken probe is not something a reader can
  act on.
- `kindleRelevant(devices)`: true when nothing is connected or any
  connected device is a Kindle. With only a Kobo plugged in, Kindle advice
  is noise.
- `showSetup(setup, devices, justInstalled, installing = false)`: while an
  install runs or has just finished (`installing || justInstalled`), true
  whenever `setup` is not null and `setup.possible`, whatever is connected:
  the Installing line and then the result answer what the reader pressed,
  and a Kobo plugged in meanwhile does not take them away. Otherwise true
  when `setup` is not null, `setup.possible`, `kindleRelevant(devices)` and
  `!setup.ready`. So a ready machine sees nothing, except the success line
  right after an install. `draw()` passes the running install as
  `installing`.
- `installedLine(version, removed, ready)`: `Installed the KFX plugin
  2.20.1.`, plus `Kindles now get KFX.` when `ready` (the CHECKED checklist's
  own field, never the engine's raw, unvalidated `answer.setup.ready`), plus
  `Removed an older copy: <name>.` for exactly one fork removed or
  `Removed older copies: <names>.` for more than one (Swift-era users will
  see this, and it deserves saying because we removed something they
  installed).
- `failedLine(answer)`: the engine's own message, or a stand-in when it
  sent none.
- `linkFailedLine(url)`: `Could not open the page. It is at <url>`.
- Copy constants: `HEADING = 'Best Kindle quality'`, `INSTALLING =
  'Downloading and installing the KFX plugin. This takes a few seconds.'`

Drawing, at the bottom of the Send page, below the status lines:

```
Best Kindle quality
Kindles get AZW3 for now. KFX looks better, and needs the three free tools below.
  Calibre            Installed
  Kindle Previewer   [Get Kindle Previewer]
  KFX plugin         [Install]
<status line>
```

- A link fix is a button that calls `openUrl(url)`. If that resolves false,
  the status line shows `linkFailedLine(url)`, so the reader can still get
  there by hand.
- An install fix is a button. Pressed: the Install button is disabled and
  reads `Installing…`, the Get links stay live (opening a page is harmless
  at any time), the status line reads `INSTALLING`,
  `runEngine(argv.kfxInstall())` runs, then the rows redraw from
  `answer.setup` and the status line shows `installedLine` or `failedLine`.
- `after` and `unavailable` fixes are plain text.
- Existing classes where they fit: a step is a `.device-row` (name left,
  control right, the same shape as a connected reader), with `.device-name`,
  `.reader-status` for "Installed", the brass primary `.btn .btn-brad` for
  Install, `.btn .btn-outline` for a Get link, and a `.send-status` line.
  Any new rule goes in `surfaces.css`, which is where the Send page's rules
  already live and is not a file Screeepub 1 is editing. `style.css` is not
  touched.

Lifecycle:

- Probed with `kfx-status` when the Send page is shown, and again when the
  window regains focus while it is shown (the reader went off to install
  Calibre and came back). One probe in flight at a time. A probe whose
  checklist matches the one on screen changes nothing, so a routine return
  to the window moves neither the focus nor the status line; one that
  differs clears the status line and the just-installed flag, then
  redraws. A probe that was already out when an install began is dropped
  (an install counter, the same shape as `send.js`'s `era`). A block
  mounted while the page is showing, with no checklist yet (a script that
  arrived while Send showed its no-script state), probes once; a remount
  for a new script on a known machine asks nothing.
- The heading, summary and `role="status"` line are built once per host,
  in `mountKfx`. A redraw refills the summary and rebuilds only the rows,
  so the live region exists before its words change.
- A redraw keeps the keyboard. Focus that was in the block goes back to
  the same step's control (found by `data-step`) when it can take focus,
  and otherwise to `ctx.restoreFocus()`, `focus.js`'s plan for the page.
- Leaving the page and coming back mid-install keeps the `INSTALLING` line.
- Not drawn in the no-script or blocked states; only on the page that lists
  readers. Not probed there either: a probe returns at once when the
  block's node is missing or no longer in the page. `send.js`'s `show()`
  draws the page before it calls `kfxShown()`, so on the page with readers
  the node is in place when the first probe runs.
- A send and an install never overlap. The install button is disabled while
  a send runs, and `sendTo` returns early while an install runs. An install
  disables the connected readers' Send buttons, and a Send button drawn
  during one starts disabled: the device poll pauses only for a send, so a
  reader plugged in mid-install still gets a row. A plugin swapped out
  under a running KFX conversion is not a case worth finding out about.
- An install that cannot start cleanly cannot leave the block stuck on.
  The busy hook and the first draw run inside the same `try` whose
  `finally` clears the installing flag, so a throw from either shows as a
  failed install instead of refusing every send until a restart.
- The updater's restart should wait on an in-flight counter inside
  `runEngine`, which would hold the restart off while an install runs. That
  counter comes with the other session's update branch (Screeepub 1), once
  it lands; this branch's `runEngine` has none, and nothing here adds one.

### `desktop/ui/send.js`

A handful of lines: mount the block in `draw()`, tell it when `show()` and
`hide()` run and when the device list changes, and the two busy checks.

## The permission

`desktop/src-tauri/capabilities/default.json`, the existing
`opener:allow-open-url` entry gains three exact URLs:

```json
{ "url": "https://calibre-ebook.com/download_osx" },
{ "url": "https://calibre-ebook.com/download_windows" },
{ "url": "https://kdp.amazon.com/en_US/help/topic/G202131170" }
```

and the description gains one clause naming the KFX download pages.
`https://calibre-ebook.com/download` is NOT granted: `kfxSetup` emits it
only where `possible` is false, and the window never draws it there.

`tests/desktop-shell.test.ts`'s "issue tracker and nothing else" test
becomes "the issue tracker and the two KFX download pages, and nothing
else". Every allow entry is either under the repository prefix or one of
exactly three strings, and every URL `kfxSetup` can emit on darwin or win32
is matched by an entry. That second half is what stops the engine's links
and the grant from drifting apart.

The ADR's grant list gets a dated amendment line recording the new doors.

## Tests

- `tests/export-kfx-setup.test.ts`: the full matrix of calibre × previewer ×
  plugin × platform (darwin, win32, linux) against the rules above, and no
  em dash in any string `kfxSetup` can produce.
- `tests/cli-kfx.test.ts`: handlers with fake status and installer (success,
  failure, removed forks, removed-then-failed, fresh status after install).
  Spawned CLI only for `--help`, `kfx-status --json` (read-only), and the
  usage refusals for `kfx-status` alone. `kfx-install`'s refusals are pinned
  in the SOURCE instead: one branch serves both verbs, its foreign list
  names each other verb's flag, and every refusal comes before either
  handler call. **No spawned test may be able to reach the real
  installer**: it would change the Calibre on whatever machine runs the
  suite, and a spawned refusal test is one deleted refusal away from doing
  exactly that.
- `tests/desktop-ui.test.ts`: the argv builders (the list test pins the
  set), and every `kfx.js` decision.
- `tests/desktop-shell.test.ts`: the opener test above.

## Out of scope

- A settings page for this. The gear is piece C; if it lands, this block
  can move or be mirrored there.
- Detecting a running Calibre. The Swift app installed with Calibre open
  and so does this. What an open Calibre window does with a plugin added
  underneath it has not been checked in this piece.
- Linux. There is no Kindle Previewer there, so there is no KFX to set up.
