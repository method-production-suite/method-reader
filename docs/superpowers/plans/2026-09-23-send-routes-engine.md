# Send Routes, Engine Half: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The engine answers every way a book can leave Screepub (ranked, remembered, with the fix for each unavailable one) and performs the non-device routes, including save-a-copy to a path the window chose.

**Architecture:** An app-wide settings file (`src/settings/app.ts`) remembers the last route. A pure catalog (`src/export/routes.ts`) ranks routes from probed facts (`src/export/route-facts.ts`). Verbs: `routes` lists, `route` performs a non-device route and remembers it, `export --out` writes to a chosen path, and `send` remembers a device kind on success.

**Tech Stack:** Bun + TypeScript, bun:test.

**Spec:** [2026-09-23-send-routes-design.md](../specs/2026-09-23-send-routes-design.md). The Swift originals are `app/Sources/ScreepubKit/ResultActions.swift`, `AppleBooks.swift`, `SendToKindle.swift`, and the checks at `app/Sources/KitCheck/main.swift:611-773` (45 `send-menu` checks) and `:975-999` (`mail-and-books`). Port their assertions; do not invent new wording where Swift already has it.

---

## Ground rules for every task

- Worktree: `<repo>/.claude/worktrees/epic-neumann-254843`, branch `parity-b-routes`. Absolute paths; never `cd` to the main checkout.
- TDD: failing test first, see it fail for the stated reason, implement, see it pass. `bunx tsc --noEmit` clean before each commit.
- **No em dash (`—`) in any string a person reads** that you add (engine messages, CLI output, route titles/details/buttons, docs). Colons, periods, commas.
- **No test may touch the real machine's state:** no test writes the real app-settings file (always set `SCREEPUB_CONFIG_DIR` or pass a path into SCRATCH), opens an app, opens a URL, or reads the real mail handler (inject the probe). No test may launch Books, a browser or Mail. Spawned-CLI tests pass `SCREEPUB_CONFIG_DIR`, `SCREEPUB_LIBRARY` and `SCREEPUB_VOLUME_ROOTS` pointing into SCRATCH, and use `SCREEPUB_REMARKABLE_ENDPOINT=http://127.0.0.1:9` so the reMarkable probe fails fast.
- Temp folders follow `tests/temp-hygiene.test.ts`: one top-level `const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-<file>-'));` and a top-level `afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));`; every other temp dir inside SCRATCH.
- Handlers return values and never print; `src/cli.ts` owns stdout (see `src/cli-export.ts`, `src/cli-kfx.ts`).
- Every verb refuses the other verbs' flags and stray positionals BEFORE doing anything, with the existing wording `"<verb> takes no --x (--x belongs to y)"`. When you add a flag to the shared verb schema (`parseVerbArgs` in `src/cli.ts`), every other verb must refuse it. Read how `--for`, `--fountain` and the update flags are refused today and follow that pattern exactly.
- Commit trailer: `Co-Authored-By: <your model name> <noreply@anthropic.com>`.
- Mutation scripts go under the session scratchpad, named `b<task>-mutants.ts`, and restore in `finally`.
- Do not touch `desktop/`, `app/`, `format-defaults.json`, the parser or the renderers. This plan is engine only.

---

## File map

| File | Status | Responsibility |
| --- | --- | --- |
| `src/settings/app.ts` | new | app-wide settings file: path, tolerant read, merging atomic write |
| `src/export/routes.ts` | new | pure catalog: route list, order, unavailable kinds, `preselected` |
| `src/export/route-facts.ts` | new | probes: devices, Books.app, Send to Kindle.app, Apple Mail default |
| `src/export/route-perform.ts` | new | performs Apple Books, Send to Kindle, email (injectable opener) |
| `src/cli-routes.ts` | new | `routesCommand`, `routeCommand` handlers |
| `src/cli-export.ts` | modify | `--out` |
| `src/cli-devices.ts` | modify | `VERBS`; `sendCommand` remembers the device kind |
| `src/cli-errors.ts` | modify | new codes |
| `src/cli.ts` | modify | usage text, flags, verb branches |
| `tests/app-settings.test.ts` | new | |
| `tests/routes.test.ts` | new | the ported `send-menu` checks |
| `tests/route-facts.test.ts` | new | |
| `tests/cli-routes.test.ts` | new | handlers and spawned CLI |
| `tests/cli-export.test.ts` | modify | `--out` |

---

### Task 1: The app-wide settings file (`src/settings/app.ts`)

**Behaviour:**

```ts
export interface AppSettings { lastRoute?: string; [other: string]: unknown }
/** Folder + 'settings.json'. SCREEPUB_CONFIG_DIR wins everywhere. */
export function appSettingsPath(platform?: NodeJS.Platform, env?: Record<string, string | undefined>): string;
/** Never throws: missing, unreadable, not JSON, or not an object → {}. */
export function readAppSettings(path?: string): AppSettings;
/** Reads, shallow-merges `patch` over what is there (unknown keys KEPT),
 *  writes atomically (temp file in the same folder, then rename), creates
 *  the folder. Returns what was written. A key set to undefined is removed. */
export function writeAppSettings(patch: Partial<AppSettings>, path?: string): AppSettings;
```

Folder per platform: darwin `<home>/Library/Application Support/Screepub`; win32 `%APPDATA%\Screepub` (fallback `<home>\AppData\Roaming\Screepub`); elsewhere `$XDG_CONFIG_HOME/screepub` if absolute, else `<home>/.config/screepub`. Use `posix`/`win32` path flavours by PLATFORM, not host, as `src/library.ts`'s `libraryRoot` does (read it; mirror its `env`/`platform` parameters and its home fallback `env.HOME || env.USERPROFILE || homedir()`).

**Tests (`tests/app-settings.test.ts`), each first failing:**
- `appSettingsPath('darwin', { HOME: '/Users/a' })` → `/Users/a/Library/Application Support/Screepub/settings.json`.
- win32 with `APPDATA: 'C:\\Users\\a\\AppData\\Roaming'` → `C:\Users\a\AppData\Roaming\Screepub\settings.json`; win32 without APPDATA falls back under USERPROFILE.
- linux with absolute `XDG_CONFIG_HOME` → `<it>/screepub/settings.json`; relative XDG_CONFIG_HOME is ignored → `<home>/.config/screepub/settings.json`.
- `SCREEPUB_CONFIG_DIR` wins on every platform (whitespace-only is ignored).
- read: missing file → `{}`; `"not json"` → `{}`; `[1,2]` → `{}`; `null` → `{}`; a valid object → itself.
- write: creates missing parents; merges (`{ libraryPath: 'x' }` on disk, write `{ lastRoute: 'apple-books' }` → both keys present); `lastRoute: undefined` removes the key; no temp file left behind in the folder after a write.
- The module never imports anything from `desktop/`.

- [ ] Steps: failing tests → implement → pass → tsc → commit "App-wide settings: one engine file outside the library, shared by pieces B and C".

---

### Task 2: The catalog (`src/export/routes.ts`), porting the 45 `send-menu` checks

**Behaviour:** as in the spec's `routes.ts` section. Types:

```ts
import type { ConnectedDevice, DeviceKind } from '../device/types';
export type RouteKey = string; // 'device:<kind>' | 'remarkable' | 'apple-books' | 'send-to-kindle' | 'email-to-kindle' | 'save-epub' | 'save-kindle'
export type Unavailable = 'connect' | 'platform' | 'setup';
export interface RouteDevice { id: string; kind: DeviceKind; name: string; volume: string | null }
export interface Route {
  id: string; key: RouteKey; title: string; detail: string; button: string;
  available: boolean; unavailable?: Unavailable; device?: RouteDevice;
}
export interface RouteFacts {
  platform: string; devices: ConnectedDevice[];
  booksApp: boolean; sendToKindleApp: boolean; appleMailDefault: boolean;
}
export function routes(facts: RouteFacts): Route[];
export function preselected(list: Route[], lastRoute: string | undefined): Route;
export const DEVICE_KINDS: readonly DeviceKind[]; // every volume-mounted kind, from src/device/types, NOT a hand list if types has one
```

Read `src/device/types.ts` (`ConnectedDevice`, `DeviceKind`, `deviceId`) and `src/cli-devices.ts` (`DeviceSummary`) first. A reMarkable arrives in `devices` with kind `remarkable` and `volume: null`; it becomes the `remarkable` route, never a `device:` route. `device.id` is `deviceId(device)`, the id `send --device` accepts.

Wording (port Swift's, adjusted only where the spec says):

| key | title | detail (available) | button |
| --- | --- | --- | --- |
| `device:<kind>` | the device's name | `over USB, offline, nothing leaves this computer` | `Copy to <name>` |
| `remarkable` | `reMarkable` | `the EPUB, over its USB connection` | `Upload to reMarkable` |
| `apple-books` | `Apple Books` | `syncs to your iPhone and iPad` | `Add to Apple Books` |
| `send-to-kindle` | `Send to Kindle app` / `Send to Kindle web` | `via Amazon, the best-looking Kindle result` | same as title |
| `email-to-kindle` | `Send to Kindle email` | `a Mail message with the book attached` | `Send to Kindle email` |
| `save-epub` | `Save the EPUB` | `for email, Apple Books and most e-readers` | `Save the EPUB…` |
| `save-kindle` | `Save a Kindle file` | `for copying to a Kindle by hand` | `Save a Kindle file…` |

Unavailable rows (after every available row, in this order: device kinds, reMarkable, Apple Books, email):
- each volume kind not connected: title = kind's display name (read `src/device/types.ts` for how names are spelled: `Kindle`, `Kobo`, `tolino`), `connect`, detail `plug in over USB to send`, button `Copy to <name>`. EXCEPT tolino on win32: `platform`, detail `cannot be found on Windows: a Windows drive carries no volume name`.
- reMarkable not docked: `connect`, `dock over USB to send`.
- Apple Books off darwin: `platform`, `on a Mac only`. On darwin without Books.app: not listed at all.
- email on darwin without Apple Mail as default: `setup`, `needs Apple Mail as the default mail app, the one mail app the attachment survives`. Off darwin: `platform`, `on a Mac only; save the EPUB and attach it yourself`.
- A connected device kind never also gets a placeholder.

`id`: a connected device row is `device:<kind>#<volume>` (volume falls back to its id); every other row's id is its key.

`preselected`: remembered key → first row with that key (available or not); else first available row; else first row.

**Tests (`tests/routes.test.ts`):** port EVERY check at `KitCheck/main.swift:611-773` that concerns the catalog and `preselected` (skip `legacyStoredAddress`, which is not ported: the window cannot read Swift's UserDefaults and a Mail compose opened with `open -a Mail` cannot be pre-addressed; note that in the test file's header). Translate `booksAvailable` → `booksApp`, `canEmailToKindle` → `appleMailDefault` on darwin, `sendToKindleApp` as is, `saveCopy` → both save rows, `remarkableDocked` → a reMarkable in `devices`. Use `platform: 'darwin'` unless the check is about platforms. The reMarkable `inputIsPDF` checks become one check that the detail names the EPUB (the window sends the EPUB). Then ADD:
- the three unavailable kinds on the right rows: darwin without Apple Mail → email `setup`; linux and win32 → Apple Books `platform` and email `platform`; unplugged Kindle → `connect`; tolino on win32 → `platform`, on darwin → `connect`.
- on linux and win32 the first route is `send-to-kindle` when nothing is connected.
- both save rows are always present and available, on every platform, whatever is connected (the floor).
- ids unique across a big matrix (every platform × with/without each app × 0/1/2 Kindles × reMarkable docked or not).
- no em dash in any title, detail or button across that matrix.
- every unavailable row has a non-empty detail and an `unavailable` kind; every available row has none.

- [ ] Steps: failing tests → implement → pass → tsc → commit "Route catalog: Swift's order, its remembered choice, and three kinds of unavailable".

---

### Task 3: The probes (`src/export/route-facts.ts`)

```ts
export interface RouteProbes {
  devices?: () => Promise<ConnectedDevice[]>;      // default: listDevices(deviceSeams)
  exists?: (path: string) => boolean;               // default: existsSync
  mailtoHandler?: () => Promise<string | null>;     // default: macOS defaults read, else null
  platform?: string;                                 // default: process.platform
}
export async function routeFacts(probes?: RouteProbes, deviceOptions?: ListDevicesOptions): Promise<RouteFacts>;
export function parseMailtoHandler(defaultsOutput: string): string | null;  // pure
```

- `booksApp`: darwin and (`/System/Applications/Books.app` or `/Applications/Books.app` exists). Else false.
- `sendToKindleApp`: darwin and `/Applications/Send to Kindle.app` exists.
- `appleMailDefault`: darwin and the handler is null (no mailto entry means the system default, Apple Mail) or `com.apple.mail` (case-insensitive). Any probe failure reads as false.
- The real `mailtoHandler` runs `defaults read com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers` with `Bun.spawn`, reads stdout, returns `parseMailtoHandler(stdout)`; a non-zero exit or a spawn error returns a sentinel meaning "unknown" that makes `appleMailDefault` false (do NOT treat an error as "Apple Mail"; that would offer a compose that loses the file). Distinguish "no mailto entry" (null → Apple Mail) from "could not read" (→ false): e.g. return `undefined` for unknown, or throw and catch.
- `parseMailtoHandler`: the output is an old-style plist array of dicts. Find the dict containing `LSHandlerURLScheme = mailto;` and return its `LSHandlerRoleAll` value, unquoted. Measured on the owner's Mac (2026-09-23) the relevant dict reads:
  ```
      {
          LSHandlerModificationDate = ...;
          LSHandlerPreferredVersions =         {
              LSHandlerRoleAll = "-";
          };
          LSHandlerRoleAll = "com.superhuman.electron";
          LSHandlerURLScheme = mailto;
      },
  ```
  Note the NESTED `LSHandlerRoleAll = "-"` inside `LSHandlerPreferredVersions`: the parser must take the dict's own top-level `LSHandlerRoleAll`, not the nested one. Values may be quoted or bare. Fixture strings go in the test file, including that nested case, a bare value, no mailto dict at all (→ null), and garbage (→ null).
- devices: default `listDevices()` from `src/device/list.ts` with the same seam options the CLI passes (`SCREEPUB_VOLUME_ROOTS`, `SCREEPUB_REMARKABLE_ENDPOINT`; see `deviceSeams()` in `src/cli.ts`).

**Tests (`tests/route-facts.test.ts`):** the parser fixtures above; `routeFacts` with injected probes on darwin/win32/linux (Books present/absent at either path, the app present/absent, handler null / `com.apple.mail` / `COM.APPLE.MAIL` / superhuman / unknown); off darwin, the file and mail probes are never called (count calls).

- [ ] Steps: failing tests → implement → pass → tsc → commit "Route facts: Books, Amazon's app and the default mail app, each probe injectable".

---

### Task 4: `screepub export ... --out <path>`

Read `src/cli-export.ts` and `tests/cli-export.test.ts` first.

**Behaviour:** `ExportOptions` gains `out?: string`. After the artifact is chosen (the existing code), when `out` is given:
- `out` must be absolute, else `CliError('usage', 'the path given with --out must be absolute')` raised BEFORE any toolchain probe or conversion (with the other argument checks).
- The extension of `out` (case-insensitive) must equal the artifact's (`result.extension`), else `CliError('usage', "that is a <EXT> file: choose a name ending in .<ext>")`. For the kindle rung this check runs after the ladder has produced the file (only then is the extension known).
- Copy the artifact to `out` atomically: parents created (`mkdirSync recursive`), write to a temp name in the destination folder, rename over. An existing file is replaced.
- The answer's `path` becomes `out`. Every other field unchanged.
- Refusing to write onto the SOURCE itself: if `resolve(out) === resolve(artifactPath)`, answer without copying (it is already there).
- Copy failures → `CliError('export-failed', <message>)`.
- In `src/cli.ts`, add `out: { type: 'string' }` to `parseVerbArgs`, pass `values.out` to `exportCommand`, list `--out <path>` in `EXPORT_USAGE`, and make EVERY other verb refuse `--out` ("belongs to export and route"). Task 6 adds `route`, which also accepts it.

**Tests** (extend `tests/cli-export.test.ts` in its own style, handler-level with its existing deps seams): epub to a new nested absolute path (parents created, bytes equal, answer path = out); relative out refused before the kindle ladder runs (inject a ladder that records calls: zero calls); wrong extension for epub refused, message names `.epub`; kindle rung with an injected ladder producing `x.azw3`: `out` ending `.azw3` copies, `.kfx` refused naming `.azw3`; overwrite replaces an existing file; no temp file left beside the destination; spawned CLI: `devices --out /x` and `send f --out /x` refused as usage, `export --help` mentions `--out`.

- [ ] Steps: failing tests → implement → pass → tsc → commit "export --out: the file goes where the window's save dialog said, and nowhere else".

---

### Task 5: `screepub routes <file.epub> [--json]`

**Handler** (`src/cli-routes.ts`):

```ts
export interface RoutesDeps { facts?: () => Promise<RouteFacts>; settingsPath?: string }
export async function routesCommand(epub: string, deps?: RoutesDeps): Promise<{ routes: Route[]; chosen: string }>;
```

- The epub must be a readable file (same check and error as `export`: `unreadable`), checked first.
- `chosen` is `preselected(list, readAppSettings(path).lastRoute).id`.
- Read-only: never writes the settings file.

**CLI:** `VERBS` gains `routes` (update the pin in `tests/cli-devices.test.ts` with a dated comment); usage line `screepub routes <file.epub> [--json]  every way this book can leave, best first`; its own `--help`; refuses every flag but `--json`/`--help` and exactly one positional. JSON `{ ok: true, routes, chosen }`. Human output: one line per route, `*` beside the chosen one, `(dimmed: <detail>)` for unavailable ones.

**Tests** (`tests/cli-routes.test.ts`): handler with injected facts and a SCRATCH settings path: chosen follows a remembered key, falls back when none; never writes (file absent before and after). Spawned: `routes <scratch epub> --json` with the env seams from the ground rules returns ok and a list whose save rows are available; refusals for `--for`, `--device`, `--out`, a second positional; `--help`.

- [ ] Steps: failing tests → implement → pass → tsc → commit "screepub routes: the ranked list, with the remembered choice".

---

### Task 6: `screepub route <key> <file.epub> ...`, and `send` remembers

**Performer** (`src/export/route-perform.ts`):

```ts
export type Opener = (argv: string[]) => Promise<{ code: number; stderr: string }>;
export async function addToAppleBooks(epub: string, open?: Opener): Promise<string>;   // returns the note
export async function sendViaAmazon(epub: string, facts: { platform: string; sendToKindleApp: boolean }, open?: Opener): Promise<string>;
export async function emailToKindle(epub: string, open?: Opener): Promise<string>;
```

- Apple Books: `['open', '-a', 'Books', epub]`. Note: `Added to Apple Books. It syncs to your iPhone and iPad when Books uses iCloud.`
- Send to Kindle: darwin with the app → `['open', '-a', 'Send to Kindle', epub]`, note `Opened Amazon's Send to Kindle app with the book.` Otherwise reveal the file, then open `https://www.amazon.com/sendtokindle`: darwin `['open', '-R', epub]` + `['open', url]`; win32 `['explorer', '/select,' + epub]` + `['rundll32', 'url.dll,FileProtocolHandler', url]`; else `['xdg-open', dirname(epub)]` + `['xdg-open', url]`. Note `Opened Amazon's Send to Kindle page, and the book's folder so you can drag it in.`
- Email: `['open', '-a', 'Mail', epub]`. Note `Opened a Mail message with the book attached. Address it to your Kindle's email address.`
- A non-zero exit → throw an Error whose message names what could not be opened (`could not open Apple Books: <stderr, trimmed>`); explorer on Windows exits 1 even on success, so for that one argv, ignore its exit code.
- The default `Opener` spawns argv[0] with the rest (Bun.spawn), waits, returns code and stderr.
- **Decided by the owner (2026-09-23):** the engine performs these opens, as above. The window gains no permission for them.

**Handler** (`src/cli-routes.ts`):

```ts
export interface RouteDeps extends RoutesDeps { open?: Opener; exportDeps?: ExportDeps }
export async function routeCommand(opts: { key: string; epub: string; out?: string; fountain?: string; optionsJson?: string }, deps?: RouteDeps):
  Promise<{ key: string; path?: string; note: string }>;
```

- Order: validate the key is a known NON-device key (device keys and `remarkable` → `CliError('usage', 'send to a reader with screepub send; route performs the others')`), the epub is readable, `--out` present exactly for save keys (missing → usage naming `--out`; given for a non-save key → usage), all BEFORE any probe or open.
- The route must be AVAILABLE in `routes(await facts())`; an unavailable one → `CliError('route-unavailable', <its detail, with a leading capital>)`.
- save-epub → `exportCommand({ epub, for: 'epub', out })`; save-kindle → `exportCommand({ epub, for: 'kindle', fountain, optionsJson, out })`. Note: `Saved to <out>.` and `path: out`.
- apple-books / send-to-kindle / email-to-kindle → the performer.
- On success ONLY, `writeAppSettings({ lastRoute: key })`.
- New error codes in `src/cli-errors.ts`: `'route-unavailable'`, `'route-failed'` (performer throws → `route-failed` with its message).

**`send` remembers:** in `src/cli-devices.ts` `sendCommand`, after a successful transfer, `writeAppSettings({ lastRoute: 'device:<kind>' })` for a volume device or `'remarkable'`. A settings-write failure must NOT fail the send (catch and ignore: the book is on the device). Give `sendCommand` an injectable settings path the tests use; the spawned tests set `SCREEPUB_CONFIG_DIR`.

**CLI:** `VERBS` gains `route`; usage `screepub route <key> <file.epub> [--out <path>] [--json]  send it to Apple Books, Amazon, Mail, or save a copy`; its own `--help` listing the keys; accepts `--out`, `--fountain`, `--options-json`, `--json`; refuses the rest; exactly two positionals. JSON `{ ok: true, key, path?, note }`.

**Tests** (`tests/cli-routes.test.ts`, handler level, fake `Opener` that records argv and returns code 0, facts injected, SCRATCH settings path):
- apple-books: argv exactly `['open','-a','Books',epub]`; remembered; note text.
- send-to-kindle: app installed → one argv; not installed on darwin → reveal then url, in that order; win32 and linux argv; explorer's exit 1 is not a failure but `open` exit 1 is.
- email: argv; unavailable when Apple Mail is not default → `route-unavailable` with the setup detail, nothing opened, nothing remembered.
- save-epub with SCRATCH out → file written, path = out, remembered `save-epub`; missing `--out` → usage before anything; `--out` on apple-books → usage.
- device key and `remarkable` → usage pointing at `send`.
- a performer failure → `route-failed`, NOT remembered.
- ordering: every usage refusal happens with zero opener calls and zero facts calls (count them).
- `sendCommand` remembers `device:kindle` on success (use the existing device test seams in `tests/cli-devices.test.ts` / `tests/cli-device-commands.test.ts`), does not remember on failure, and a read-only settings folder does not fail the send.
- Spawned: `route apple-books <epub> --json` on a platform/facts that make it unavailable is impossible to force from outside, so spawned tests cover only refusals and `--help` (never a real open).

- [ ] Steps: failing tests → implement → pass → tsc → commit "screepub route: Apple Books, Amazon, Mail and save a copy, remembered when they work".

---

### Task 7: Docs and the whole suite

- `README.md` `#### Device commands`: add `routes` and `route` lines to the code block and one short paragraph (what they do; save needs an absolute `--out`; Books and Mail are Mac only; the choice is remembered in the app settings file, with its location per platform).
- `docs/parity-audit.md`: in "The route catalog" table, mark Apple Books, Send to Kindle, email and save as "engine done 2026-09-23 (piece B), window pending" (do not claim the window).
- `bun test` (full) and `bunx tsc --noEmit` green; the em-dash check over the branch diff shows nothing added by this plan.
- [ ] Commit "Docs: the route verbs".
