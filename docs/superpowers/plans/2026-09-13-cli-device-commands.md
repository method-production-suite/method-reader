# CLI Device Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the engine a command-line surface for the device logic piece A ported — `screepub devices` and `screepub send` — without disturbing the existing `screepub <file>` contract by one byte. This is the interface the Tauri shell will consume in piece C, designed as a CLI first so it is testable and scriptable on its own.

**Architecture:** Three layers, each testable without the one above it. `src/device/list.ts` composes piece A's `mountedDevices()` (synchronous, injectable roots) with its `probeRemarkable()` (async, injectable endpoint) into one `listDevices()`; the two run concurrently so a machine with no reMarkable pays one probe timeout rather than a timeout plus a scan. `src/cli-devices.ts` holds verb dispatch and the two command handlers, which **return plain result objects and never print** — every behavioral test asserts on values, not on stdout. `src/cli.ts` gains a dispatch line at the top of `main()` and the printing; `src/cli-errors.ts` gains the five new codes and one `CliError` class so handlers can name a contract code without importing the printer.

**Tech Stack:** TypeScript, Bun (`bun:test`), Node built-ins (`node:fs`, `node:path`, `node:util`'s `parseArgs`). No new dependencies.

**Spec:** [`docs/superpowers/specs/2026-09-13-cli-device-commands-design.md`](../specs/2026-09-13-cli-device-commands-design.md)
**Program ADR:** [`docs/adr/2026-09-12-cross-platform-tauri.md`](../../adr/2026-09-12-cross-platform-tauri.md)
**Follows:** [piece A — device logic port](2026-09-12-device-logic-port.md)

## Global Constraints

- **No file under `app/` may be modified.** Verify with `git status --short app/` before every commit; it must print nothing.
- **The `--json` contract is absolute: every exit in that mode is ONE parseable JSON object on stdout.** `cli.ts` pre-scans raw argv for `--json` precisely because `parseArgs` can throw before it would know the mode. Verb dispatch runs *before* `parseArgs` and must not break that pre-scan, and the verb parser's own throws must route through the same `fail()`. Every new error code gets a test that parses stdout and asserts it is exactly one line.
- **`screepub <file>` behaves identically to before.** `tests/cli.test.ts` is the golden master for that and is not edited by this piece — only appended to in Task 10, never altered. If a change to it seems necessary, stop and raise it.
- **Handlers never print.** `devicesCommand` and `sendCommand` return result objects; `cli.ts` is the only module that calls `console.log`. A handler that printed would be untestable without spawning.
- **No test reads a real mount or touches a real network.** Volume enumeration is injected through piece A's `enumerateVolumes(roots)` seam; reMarkable is a local `Bun.serve` stub. This holds for the spawned end-to-end tests too, which inject through `SCREEPUB_VOLUME_ROOTS` and `SCREEPUB_REMARKABLE_ENDPOINT` (introduced in Task 8).
- **The filename-shadowing rule is tested in BOTH directions.** A file literally named `devices` converts; the bare word lists devices. One direction alone proves nothing about the rule.
- **No conversion changes of any kind.** The parser, the renderers, `convert.ts` and `options.ts` are untouched. Nothing in this piece is a formatting behavior, so `docs/formatting-options-log.md` is not updated.
- **Existing suite stays green:** all 734 existing tests pass (3 skipped) and `bunx tsc --noEmit` is clean after every task.

## File Structure

**Created:**

| File | Responsibility |
|---|---|
| `src/device/list.ts` | `listDevices()` — mounted vendors and a concurrent reMarkable probe, merged into one list. |
| `src/cli-devices.ts` | `resolveCommand` verb dispatch, `selectDevice`, and the `devicesCommand` / `sendCommand` handlers. All return values; none print. |
| `tests/device-list.test.ts` | `listDevices` against injected roots and a `Bun.serve` stub. |
| `tests/cli-devices.test.ts` | Dispatch, selection and handler tests — in-process, no spawning. |
| `tests/cli-device-commands.test.ts` | End-to-end spawned-CLI tests: the `--json` contract for every new code, shadowing in both directions. |

**Modified:**

| File | Change |
|---|---|
| `src/cli-errors.ts` | Five new codes on `JsonError['code']`; a `CliError` class carrying one. |
| `src/device/remarkable.ts` | Extract the accepted-extension rule into an exported `remarkableAccepts()` that `uploadToRemarkable` then uses. Behavior unchanged. |
| `src/cli.ts` | Verb dispatch at the top of `main()`, a verb arg parser, printing for both commands, the two test-seam env vars, and USAGE text. |
| `README.md` | The two commands in the CLI section. |
| `tests/cli.test.ts` | **Appended to only** (Task 10): one test that a file whose stem is a verb still converts. |

**Out of scope, per the spec:** conversion changes, the Mac app switching to shell out (it keeps its Swift copy until piece F), Tauri, anything under `app/`.

---

### Task 1: The device error codes and `CliError`

Handlers must be able to say "this is an `ambiguous-device`" without importing the printer or knowing about `process.exit`. One small exception class carrying a contract code does that, and keeps the code union in the one file that already owns it.

**Files:**
- Modify: `src/cli-errors.ts`
- Test: `tests/cli-devices.test.ts` (create)

**Interfaces:**
- Consumes: `JsonError` (existing).
- Produces:
  - `JsonError['code']` gains `'no-devices' | 'ambiguous-device' | 'unknown-device' | 'send-failed' | 'unsupported-file'`
  - `class CliError extends Error { readonly code: JsonError['code']; constructor(code: JsonError['code'], message: string); toJson(): JsonError }`

- [ ] **Step 1: Write the failing test**

Create `tests/cli-devices.test.ts`:

```ts
import { test, expect } from 'bun:test';
import { CliError } from '../src/cli-errors';

test('CliError carries a contract code and renders the exact JSON error shape', () => {
  const err = new CliError('ambiguous-device', 'several devices are connected: a, b');
  expect(err).toBeInstanceOf(Error);
  expect(err.name).toBe('CliError');
  expect(err.code).toBe('ambiguous-device');
  expect(err.message).toBe('several devices are connected: a, b');
  // The JSON body is exactly two keys — the app decodes {code, message} and
  // nothing else. An implementation that spread the Error (picking up `name`,
  // or nothing at all, since Error fields are non-enumerable) fails here.
  expect(err.toJson()).toEqual({ code: 'ambiguous-device', message: 'several devices are connected: a, b' });
  expect(Object.keys(err.toJson()).sort()).toEqual(['code', 'message']);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-devices.test.ts`
Expected: FAIL — `CliError` is not exported from `../src/cli-errors`.

- [ ] **Step 3: Write minimal implementation**

In `src/cli-errors.ts`, extend the union and append the class:

```ts
export interface JsonError {
  code:
    | 'scanned'
    | 'not-screenplay'
    | 'unreadable'
    | 'password'
    | 'unsupported-type'
    | 'usage'
    | 'bad-options'
    | 'internal'
    // Device commands (piece B). Same contract, same stdout rule.
    | 'no-devices'
    | 'ambiguous-device'
    | 'unknown-device'
    | 'send-failed'
    | 'unsupported-file';
  message: string;
}

/** A failure that already knows its contract code. Thrown by the device
 * command handlers, which must not import the printer: they return values or
 * throw this, and cli.ts is the only place that decides how it reaches the
 * user. */
export class CliError extends Error {
  constructor(
    readonly code: JsonError['code'],
    message: string,
  ) {
    super(message);
    this.name = 'CliError';
  }

  toJson(): JsonError {
    return { code: this.code, message: this.message };
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 735 tests, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/cli-errors.ts tests/cli-devices.test.ts
git commit -m "$(cat <<'EOF'
The CLI error contract learns the device codes

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 2: `remarkableAccepts` — one copy of the extension rule

`send` must report `unsupported-file` for an extension the tablet rejects, and it must decide that *typed*, not by matching a substring of an upload error (`cli-errors.ts` already carries that rule for the conversion codes: a path like `~/password-notes/x.pdf` must not classify as a password failure). The honest fix is to give the rule a name and have both callers use the one copy, rather than let `cli-devices.ts` keep a second list of extensions that can drift from `remarkable.ts`'s.

**Files:**
- Modify: `src/device/remarkable.ts`
- Test: `tests/device-remarkable.test.ts` (append)

**Interfaces:**
- Produces: `remarkableAccepts(file: string): boolean`
- Consumes: nothing new. `uploadToRemarkable`'s behavior is unchanged — it now calls the predicate instead of inlining it.

- [ ] **Step 1: Write the failing test**

Append to `tests/device-remarkable.test.ts` (and add `remarkableAccepts` to the existing import from `../src/device/remarkable`):

```ts
test('remarkableAccepts is the one copy of the PDF/EPUB rule', () => {
  expect(remarkableAccepts('/tmp/Script.pdf')).toBe(true);
  expect(remarkableAccepts('/tmp/Script.epub')).toBe(true);
  // Case and a dotted stem must not fool it: the extension is the LAST dot.
  expect(remarkableAccepts('/tmp/Script.EPUB')).toBe(true);
  expect(remarkableAccepts('/tmp/Draft.epub.azw3')).toBe(false);
  expect(remarkableAccepts('/tmp/Script.azw3')).toBe(false);
  expect(remarkableAccepts('/tmp/Script.mobi')).toBe(false);
  // No extension at all is not an accepted extension.
  expect(remarkableAccepts('/tmp/Script')).toBe(false);
});
```

Catches: an implementation using `file.includes('.epub')`, which would pass every accept case and wrongly accept `Draft.epub.azw3`; and one that lowercases nothing, which would reject `.EPUB`.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/device-remarkable.test.ts`
Expected: FAIL — `remarkableAccepts` is not exported.

- [ ] **Step 3: Write minimal implementation**

In `src/device/remarkable.ts`, add the predicate above `uploadToRemarkable` and rewrite that function's first three lines to use it:

```ts
/** The tablet's USB web interface accepts these two formats and no others.
 * Exported because `send` has to report `unsupported-file` BEFORE it calls
 * upload — deciding that by matching the upload error's message would be the
 * substring detection cli-errors.ts bans. One copy, two callers. */
export function remarkableAccepts(file: string): boolean {
  const ext = extname(file).replace(/^\./, '').toLowerCase();
  return ext === 'pdf' || ext === 'epub';
}
```

and inside `uploadToRemarkable`, replace the inline check:

```ts
  if (!remarkableAccepts(file)) {
    const ext = extname(file).replace(/^\./, '').toLowerCase();
    throw new RemarkableUploadError(`reMarkable accepts PDF and EPUB, not .${ext}.`);
  }
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS. Piece A's existing reMarkable assertions — including `rejects.toThrow('azw3')` — still pass, proving the extraction changed no behavior.

- [ ] **Step 5: Commit**

```bash
git add src/device/remarkable.ts tests/device-remarkable.test.ts
git commit -m "$(cat <<'EOF'
Name the reMarkable extension rule so send can ask it directly

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 3: `src/device/list.ts` — one list of everything connected

Mounted vendors come from piece A's `mountedDevices()`; reMarkable never mounts and appears only when its USB web interface answers. The probe's timeout is a real cost on every machine without one, so the two halves run **concurrently** — the command's wall clock is the probe's timeout, not the sum.

Both seams are injectable: `roots` for the mount scan (exactly as piece A's `enumerateVolumes` takes) and `remarkableEndpoint` for the probe. `scan` and `probe` themselves are injectable too, so the concurrency claim is directly observable in a test rather than asserted in prose.

**Files:**
- Create: `src/device/list.ts`
- Test: `tests/device-list.test.ts` (create)

**Interfaces:**
- Consumes: `mountedDevices(roots?: string[]): ConnectedDevice[]` and `probeRemarkable(endpoint?: string, timeoutMs?: number): Promise<boolean>` (piece A); `ConnectedDevice`, `DEVICE_DISPLAY_NAMES` from `src/device/types.ts`.
- Produces:
  ```ts
  export interface ListDevicesOptions {
    roots?: string[];
    remarkableEndpoint?: string;
    probeTimeoutMs?: number;
    scan?: (roots?: string[]) => ConnectedDevice[] | Promise<ConnectedDevice[]>;
    probe?: (endpoint: string, timeoutMs: number) => Promise<boolean>;
  }
  export async function listDevices(options?: ListDevicesOptions): Promise<ConnectedDevice[]>;
  ```

- [ ] **Step 1: Write the failing test**

Create `tests/device-list.test.ts`:

```ts
import { test, expect, afterAll } from 'bun:test';
import { mkdirSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { listDevices } from '../src/device/list';
import type { ConnectedDevice } from '../src/device/types';

/** A mount parent holding one Kobo, built in a temp dir. No real mount is
 * ever read: the root is injected, exactly as piece A's enumerateVolumes
 * allows. */
function mountRootWithKobo(): string {
  const root = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
  mkdirSync(join(root, 'KOBOeReader', '.kobo'), { recursive: true });
  return root;
}

function stub(status: number) {
  const server = Bun.serve({ port: 0, fetch: () => new Response('[]', { status }) });
  return { url: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true) };
}

const answering = stub(200);
const silent = stub(404);
afterAll(() => {
  answering.stop();
  silent.stop();
});

test('mounted vendors are listed when the tablet does not answer', async () => {
  const devices = await listDevices({
    roots: [mountRootWithKobo()],
    remarkableEndpoint: silent.url,
  });
  expect(devices.map((d) => d.kind)).toEqual(['kobo']);
  expect(devices[0].volume).toMatch(/KOBOeReader$/);
});

test('a reMarkable that answers is appended after the mounted devices', async () => {
  const devices = await listDevices({
    roots: [mountRootWithKobo()],
    remarkableEndpoint: answering.url,
  });
  // Order is asserted with a mounted device PRESENT, so an implementation
  // that prepended the tablet fails rather than passing on an empty list.
  expect(devices.map((d) => d.kind)).toEqual(['kobo', 'remarkable']);
  const rm = devices[1];
  expect(rm.volume).toBeNull();
  expect(rm.name).toBe('reMarkable');
});

test('nothing connected is an empty list, not an error', async () => {
  const empty = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
  expect(await listDevices({ roots: [empty], remarkableEndpoint: silent.url })).toEqual([]);
});

test('the mount scan and the probe run concurrently, not one after the other', async () => {
  const slow = <T>(value: T, ms: number) =>
    new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));
  const started = Date.now();
  const devices = await listDevices({
    scan: () => slow<ConnectedDevice[]>([{ kind: 'kindle', name: 'Kindle', volume: '/v/Kindle' }], 200),
    probe: () => slow(true, 200),
  });
  const elapsed = Date.now() - started;
  expect(devices.map((d) => d.kind)).toEqual(['kindle', 'remarkable']);
  // Sequential (`await scan(); await probe()`) takes ~400ms and fails here;
  // Promise.all takes ~200ms. This is the spec's "wall-clock is the probe's
  // timeout rather than the sum" made into an assertion.
  expect(elapsed).toBeLessThan(350);
});

test('a probe that throws is a tablet that is not there, not a crash', async () => {
  const devices = await listDevices({
    scan: () => [],
    probe: () => Promise.reject(new Error('ECONNREFUSED')),
  });
  expect(devices).toEqual([]);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/device-list.test.ts`
Expected: FAIL — cannot resolve `../src/device/list`.

- [ ] **Step 3: Write minimal implementation**

```ts
// src/device/list.ts
// The one list of everything currently reachable: volume-mounted vendors,
// which piece A detects by on-disk signature, plus a reMarkable, which never
// mounts and is only there if its USB web interface answers.
import { mountedDevices } from './volumes';
import { probeRemarkable, REMARKABLE_ENDPOINT } from './remarkable';
import { DEVICE_DISPLAY_NAMES, type ConnectedDevice } from './types';

export interface ListDevicesOptions {
  /** Mount parents to scan; defaults to this platform's (piece A). */
  roots?: string[];
  /** reMarkable base URL; defaults to the fixed USB address. */
  remarkableEndpoint?: string;
  probeTimeoutMs?: number;
  /** Seams, injected only by tests. Production uses the two real functions. */
  scan?: (roots?: string[]) => ConnectedDevice[] | Promise<ConnectedDevice[]>;
  probe?: (endpoint: string, timeoutMs: number) => Promise<boolean>;
}

/** Every recognised reader that is reachable right now.
 *
 * The probe's timeout is paid on every machine that has no reMarkable, so the
 * mount scan is STARTED FIRST and both are awaited together: the command's
 * wall clock is the probe's timeout, never timeout + scan. The scan itself is
 * synchronous in production; the seam's return type allows a promise so the
 * concurrency is testable. */
export async function listDevices(options: ListDevicesOptions = {}): Promise<ConnectedDevice[]> {
  const scan = options.scan ?? mountedDevices;
  const probe = options.probe ?? probeRemarkable;
  const endpoint = options.remarkableEndpoint ?? REMARKABLE_ENDPOINT;
  const timeoutMs = options.probeTimeoutMs ?? 1500;

  const scanning = Promise.resolve(scan(options.roots));
  const probing = probe(endpoint, timeoutMs).catch(() => false);

  const [mounted, remarkablePresent] = await Promise.all([scanning, probing]);
  const devices = [...mounted];
  if (remarkablePresent) {
    devices.push({ kind: 'remarkable', name: DEVICE_DISPLAY_NAMES.remarkable, volume: null });
  }
  return devices;
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 740 tests, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/device/list.ts tests/device-list.test.ts
git commit -m "$(cat <<'EOF'
One list of what is plugged in, with the probe run alongside the scan

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 4: `resolveCommand` — verb dispatch that a filename can win

The CLI's contract is `screepub <input> [options]`, and every existing invocation plus the Mac app depends on it. The rule: **the first argument is a subcommand only when it matches a known verb AND no file of that name exists.** A file named exactly `devices` wins over the verb — the user can always disambiguate the verb-losing case with `./devices`, whereas a stolen filename would be silently unconvertible.

Dispatch lives in `cli-devices.ts`, not `cli.ts`, because `cli.ts` runs `main()` on import and so cannot be imported by a test — the same reason `cli-errors.ts` exists.

**Files:**
- Modify: `src/cli-devices.ts` (create in this task)
- Test: `tests/cli-devices.test.ts` (append)

**Interfaces:**
- Produces:
  ```ts
  export const VERBS = ['devices', 'send'] as const;
  export type Verb = (typeof VERBS)[number];
  export type Command = { kind: 'verb'; verb: Verb; args: string[] } | { kind: 'convert' };
  export function resolveCommand(argv: string[], exists?: (path: string) => boolean): Command;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-devices.test.ts` (add the import at the top of the file):

```ts
import { resolveCommand, VERBS } from '../src/cli-devices';

const noFiles = () => false;
const onlyDevicesFile = (path: string) => path === 'devices';

test('the bare verb dispatches when no file of that name exists', () => {
  expect(resolveCommand(['devices'], noFiles)).toEqual({ kind: 'verb', verb: 'devices', args: [] });
  expect(resolveCommand(['devices', '--json'], noFiles)).toEqual({
    kind: 'verb', verb: 'devices', args: ['--json'],
  });
  expect(resolveCommand(['send', 'Script.epub', '--device', 'x'], noFiles)).toEqual({
    kind: 'verb', verb: 'send', args: ['Script.epub', '--device', 'x'],
  });
});

test('a file literally named `devices` beats the verb', () => {
  // The other direction of the same rule. Without the exists() check this
  // passes as a verb and the file becomes silently unconvertible; without the
  // verb branch at all, the test above fails. Neither half is provable alone.
  expect(resolveCommand(['devices'], onlyDevicesFile)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['devices', '--json'], onlyDevicesFile)).toEqual({ kind: 'convert' });
});

test('an explicit path is never a verb, even with no such file', () => {
  // Catches an implementation that strips ./ or compares basenames: `./devices`
  // is how the user disambiguates when a verb would otherwise win.
  expect(resolveCommand(['./devices'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['devices.pdf'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['/tmp/send'], noFiles)).toEqual({ kind: 'convert' });
});

test('only the first argument can be a verb', () => {
  // Catches an implementation that scans argv for any known verb: a script
  // named devices.pdf sent with --title devices must still convert.
  expect(resolveCommand(['--json', 'devices'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['Script.pdf', '--title', 'send'], noFiles)).toEqual({ kind: 'convert' });
});

test('unknown words and an empty argv are the default path', () => {
  expect(resolveCommand(['frobnicate'], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand([], noFiles)).toEqual({ kind: 'convert' });
  expect(resolveCommand(['--help'], noFiles)).toEqual({ kind: 'convert' });
});

test('VERBS is the single list of known verbs', () => {
  expect([...VERBS]).toEqual(['devices', 'send']);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-devices.test.ts`
Expected: FAIL — cannot resolve `../src/cli-devices`.

- [ ] **Step 3: Write minimal implementation**

```ts
// src/cli-devices.ts
// Verb dispatch and the device command handlers. Handlers RETURN result
// objects and never print: cli.ts owns stdout, so these are testable in
// process. Dispatch lives here rather than in cli.ts because cli.ts runs
// main() on import and cannot be imported by a test.
import { existsSync } from 'node:fs';

export const VERBS = ['devices', 'send'] as const;
export type Verb = (typeof VERBS)[number];

export type Command = { kind: 'verb'; verb: Verb; args: string[] } | { kind: 'convert' };

/** Decide whether argv opens with a subcommand.
 *
 * The rule, and the whole compatibility surface of this piece: the FIRST
 * argument is a verb only when it matches a known verb exactly AND no file of
 * that name exists. `screepub devices` lists; `screepub ./devices` and
 * `screepub devices.pdf` convert; and a real file named `devices` wins,
 * because a user can always write `./devices` to get the file, while a stolen
 * filename would be unconvertible with no way out.
 *
 * Only argv[0] is considered. A verb after a flag would be indistinguishable
 * from a flag's value (`--title send`). */
export function resolveCommand(
  argv: string[],
  exists: (path: string) => boolean = existsSync,
): Command {
  const first = argv[0];
  if (first === undefined) return { kind: 'convert' };
  if (!(VERBS as readonly string[]).includes(first)) return { kind: 'convert' };
  if (exists(first)) return { kind: 'convert' };
  return { kind: 'verb', verb: first as Verb, args: argv.slice(1) };
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 746 tests, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/cli-devices.ts tests/cli-devices.test.ts
git commit -m "$(cat <<'EOF'
Verb dispatch, with a real filename beating the verb

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 5: `devicesCommand`

The listing handler. It is a thin shape-change over `listDevices` — each device gains the `id` that `send --device` accepts — but the shape is the Tauri contract, so it is pinned here rather than assembled inline in the printer.

**Files:**
- Modify: `src/cli-devices.ts`
- Test: `tests/cli-devices.test.ts` (append)

**Interfaces:**
- Consumes: `listDevices(options?: ListDevicesOptions)`, `deviceId(device: ConnectedDevice): string`, `DeviceKind`.
- Produces:
  ```ts
  export interface DeviceSummary { id: string; kind: DeviceKind; name: string; volume: string | null }
  export interface DevicesResult { devices: DeviceSummary[] }
  export async function devicesCommand(options?: ListDevicesOptions): Promise<DevicesResult>;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-devices.test.ts`:

```ts
import { devicesCommand } from '../src/cli-devices';
import type { ConnectedDevice } from '../src/device/types';

const kobo: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: '/run/media/you/KOBOeReader' };
const kindle: ConnectedDevice = { kind: 'kindle', name: 'Kindle', volume: '/run/media/you/Kindle' };

test('devicesCommand reports each device with the id send accepts', async () => {
  const { devices } = await devicesCommand({ scan: () => [kobo], probe: async () => true });
  expect(devices).toEqual([
    { id: '/run/media/you/KOBOeReader', kind: 'kobo', name: 'KOBOeReader', volume: '/run/media/you/KOBOeReader' },
    { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null },
  ]);
});

test('devicesCommand on an empty list is an empty array, not a throw', async () => {
  const { devices } = await devicesCommand({ scan: () => [], probe: async () => false });
  expect(devices).toEqual([]);
});

test('every id devicesCommand reports round-trips through send selection', async () => {
  // The two commands must agree on what an id is. If `id` were ever the
  // device NAME (both are "Kindle" for a Kindle, so a name/volume mix-up is
  // invisible in the single-device case), selectDevice would not find the
  // Kobo here and this fails.
  const { devices } = await devicesCommand({ scan: () => [kobo, kindle], probe: async () => false });
  for (const summary of devices) {
    expect(selectDevice([kobo, kindle], summary.id).name).toBe(summary.name);
  }
});
```

Note: the third test imports `selectDevice`, which Task 6 creates. Write it now and expect it to fail for that reason too; it passes at the end of Task 6. If the executing worker prefers one-task-at-a-time green, move that single test to Task 6 Step 1 — do not weaken it.

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-devices.test.ts`
Expected: FAIL — `devicesCommand` is not exported.

- [ ] **Step 3: Write minimal implementation**

Append to `src/cli-devices.ts`:

```ts
import { listDevices, type ListDevicesOptions } from './device/list';
import { deviceId, type ConnectedDevice, type DeviceKind } from './device/types';

/** One device as the CLI and the Tauri shell see it. `id` is what
 * `send --device` accepts: the volume path, or the kind for reMarkable. */
export interface DeviceSummary {
  id: string;
  kind: DeviceKind;
  name: string;
  volume: string | null;
}

export interface DevicesResult {
  devices: DeviceSummary[];
}

function summarize(device: ConnectedDevice): DeviceSummary {
  return { id: deviceId(device), kind: device.kind, name: device.name, volume: device.volume };
}

/** Everything plugged in. An empty list is an answer, not an error. */
export async function devicesCommand(options: ListDevicesOptions = {}): Promise<DevicesResult> {
  return { devices: (await listDevices(options)).map(summarize) };
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test tests/cli-devices.test.ts`
Expected: the two `devicesCommand` tests PASS; the round-trip test still fails on `selectDevice` until Task 6.

- [ ] **Step 5: Commit**

Commit after Task 6, so the suite is green at every commit. (No commit in this task.)

---

### Task 6: `selectDevice` — which reader gets the book

Omitting `--device` is only safe when there is exactly one answer. Sending a book to the wrong reader is annoying to undo by hand, so several connected and no `--device` is a failure that lists them rather than a guess.

The precedence between the three failures is the part a wrong implementation gets wrong silently, so each is tested on an input where the orderings actually disagree.

**Files:**
- Modify: `src/cli-devices.ts`
- Test: `tests/cli-devices.test.ts` (append)

**Interfaces:**
- Consumes: `ConnectedDevice`, `deviceId`, `CliError`.
- Produces: `export function selectDevice(devices: ConnectedDevice[], requestedId?: string): ConnectedDevice;`

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-devices.test.ts`:

```ts
import { selectDevice } from '../src/cli-devices';

const remarkable: ConnectedDevice = { kind: 'remarkable', name: 'reMarkable', volume: null };

test('one device connected and no --device picks it', () => {
  expect(selectDevice([kobo])).toBe(kobo);
});

test('an explicit id picks that device even when several are connected', () => {
  // Ordering test: "ambiguous when >1" and "honour --device first" disagree
  // on exactly this input. An implementation that checks the count before the
  // id throws ambiguous-device here.
  expect(selectDevice([kobo, kindle], '/run/media/you/Kindle')).toBe(kindle);
  expect(selectDevice([kobo, kindle, remarkable], 'remarkable')).toBe(remarkable);
});

test('several connected and no --device is ambiguous-device, listing the ids', () => {
  let thrown: unknown;
  try { selectDevice([kobo, kindle]); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('ambiguous-device');
  // The message must name both, or the user has no way to pick one.
  expect((thrown as CliError).message).toContain('/run/media/you/KOBOeReader');
  expect((thrown as CliError).message).toContain('/run/media/you/Kindle');
});

test('nothing connected is no-devices even when --device was given', () => {
  // Ordering test: both no-devices and unknown-device describe this input.
  // The plan's rule is that the empty list wins, because "nothing is plugged
  // in" is the actionable fact. An implementation that checks the requested
  // id first reports unknown-device and fails here.
  let thrown: unknown;
  try { selectDevice([], '/run/media/you/Kindle'); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('no-devices');
});

test('nothing connected and no --device is also no-devices', () => {
  let thrown: unknown;
  try { selectDevice([]); } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('no-devices');
});

test('an id that matches nothing connected is unknown-device, listing what is', () => {
  let thrown: unknown;
  try { selectDevice([kobo, kindle], '/run/media/you/Nope'); } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unknown-device');
  expect((thrown as CliError).message).toContain('/run/media/you/Nope');
  expect((thrown as CliError).message).toContain('/run/media/you/KOBOeReader');
});

test('selection matches on the id, never on a prefix or the name', () => {
  // Catches `deviceId(d).includes(requested)` and `d.name === requested`.
  let thrown: unknown;
  try { selectDevice([kindle], '/run/media/you'); } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('unknown-device');
  let byName: unknown;
  try { selectDevice([kindle], 'Kindle'); } catch (err) { byName = err; }
  expect((byName as CliError).code).toBe('unknown-device');
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-devices.test.ts`
Expected: FAIL — `selectDevice` is not exported.

- [ ] **Step 3: Write minimal implementation**

Append to `src/cli-devices.ts` (and add `CliError` to its import from `./cli-errors`):

```ts
/** Which device gets the book.
 *
 * Precedence is deliberate and tested on inputs where the orders disagree:
 *   1. Nothing connected -> no-devices, EVEN IF --device was given. The
 *      actionable fact is that nothing is plugged in; "no device matches
 *      /run/media/you/Kindle" would send the user hunting for a typo.
 *   2. --device given -> exact id match, or unknown-device. It wins over the
 *      ambiguity check: the user has already answered that question.
 *   3. Exactly one connected -> that one.
 *   4. Several -> ambiguous-device, listing the ids. Never a guess: sending a
 *      book to the wrong reader is annoying to undo by hand. */
export function selectDevice(devices: ConnectedDevice[], requestedId?: string): ConnectedDevice {
  const ids = devices.map(deviceId);

  if (devices.length === 0) {
    throw new CliError('no-devices', 'no reader is connected — plug one in over USB and try again');
  }

  if (requestedId !== undefined) {
    const index = ids.indexOf(requestedId);
    if (index === -1) {
      throw new CliError(
        'unknown-device',
        `no connected reader has the id "${requestedId}" — connected: ${ids.join(', ')}`,
      );
    }
    return devices[index];
  }

  if (devices.length === 1) return devices[0];

  throw new CliError(
    'ambiguous-device',
    `several readers are connected — pick one with --device: ${ids.join(', ')}`,
  );
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 756 tests (including Task 5's round-trip test, now satisfied), `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/cli-devices.ts tests/cli-devices.test.ts
git commit -m "$(cat <<'EOF'
List devices, and decide which one a send means

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 7: `sendCommand`

Sends an existing file. It does **not** convert — the file must already exist, which keeps the command honest about what it does and leaves the conversion pipeline untouched. Volume-mounted devices go through piece A's `copyToDevice`, which puts each vendor's file where that vendor actually indexes it; reMarkable goes through `uploadToRemarkable`, which lists the root folder immediately before posting.

**Files:**
- Modify: `src/cli-devices.ts`
- Test: `tests/cli-devices.test.ts` (append)

**Interfaces:**
- Consumes: `selectDevice`, `listDevices`, `copyToDevice(file: string, device: ConnectedDevice): string`, `uploadToRemarkable(file: string, endpoint?: string): Promise<void>`, `remarkableAccepts(file: string): boolean`, `CliError`.
- Produces:
  ```ts
  export interface SendOptions extends ListDevicesOptions { file: string; deviceId?: string }
  export interface SendResult {
    device: { id: string; kind: DeviceKind; name: string };
    destination?: string;
    uploaded?: boolean;
  }
  export async function sendCommand(options: SendOptions): Promise<SendResult>;
  ```

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-devices.test.ts` (add these imports at the top: `mkdirSync`, `mkdtempSync`, `readFileSync`, `writeFileSync` from `node:fs`; `tmpdir` from `node:os`; `join` from `node:path`; `afterAll` from `bun:test`; and `sendCommand`):

```ts
function book(name = 'Script.epub'): string {
  const path = join(mkdtempSync(join(tmpdir(), 'screepub-send-')), name);
  writeFileSync(path, 'book-bytes');
  return path;
}

function kindleVolume(): ConnectedDevice {
  const volume = join(mkdtempSync(join(tmpdir(), 'screepub-vol-')), 'Kindle');
  mkdirSync(join(volume, 'documents'), { recursive: true });
  return { kind: 'kindle', name: 'Kindle', volume };
}

const uploads: string[] = [];
const upstub = Bun.serve({
  port: 0,
  async fetch(req) {
    const path = new URL(req.url).pathname;
    if (req.method === 'POST') {
      uploads.push(path);
      await req.arrayBuffer();
    }
    return new Response('[]', { status: 200 });
  },
});
const UPLOAD_URL = `http://127.0.0.1:${upstub.port}`;
afterAll(() => upstub.stop(true));

test('sending to a volume device copies it where that vendor indexes', async () => {
  const device = kindleVolume();
  const file = book();
  const result = await sendCommand({
    file,
    scan: () => [device],
    probe: async () => false,
  });
  expect(result.device).toEqual({ id: device.volume!, kind: 'kindle', name: 'Kindle' });
  expect(result.destination).toBe(join(device.volume!, 'documents', 'Script.epub'));
  // The bytes actually moved — a handler that computed the path but skipped
  // the copy passes every other assertion here.
  expect(readFileSync(result.destination!, 'utf8')).toBe('book-bytes');
  expect(result.uploaded).toBeUndefined();
});

test('sending to reMarkable uploads and reports no destination path', async () => {
  uploads.length = 0;
  const result = await sendCommand({
    file: book(),
    scan: () => [],
    probe: async () => true,
    remarkableEndpoint: UPLOAD_URL,
  });
  expect(result.device).toEqual({ id: 'remarkable', kind: 'remarkable', name: 'reMarkable' });
  expect(result.uploaded).toBe(true);
  expect(result.destination).toBeUndefined();
  expect(uploads).toEqual(['/upload']);
});

test('reMarkable rejects an extension it cannot read, before any request', async () => {
  uploads.length = 0;
  let thrown: unknown;
  try {
    await sendCommand({
      file: book('Script.azw3'),
      scan: () => [],
      probe: async () => true,
      remarkableEndpoint: UPLOAD_URL,
    });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unsupported-file');
  // "Before any request" is the point: nothing was posted. A handler that
  // called upload and mapped its message would still have hit the tablet.
  expect(uploads).toEqual([]);
});

test('a failed copy is send-failed, carrying the underlying message', async () => {
  // A volume path whose PARENT is a regular file: mkdirSync fails with
  // ENOTDIR on every platform, so this is deterministic rather than relying
  // on a read-only directory the test runner might happen to own.
  const blocker = join(mkdtempSync(join(tmpdir(), 'screepub-block-')), 'not-a-dir');
  writeFileSync(blocker, 'x');
  const device: ConnectedDevice = { kind: 'kobo', name: 'KOBOeReader', volume: join(blocker, 'Kobo') };
  let thrown: unknown;
  try {
    await sendCommand({ file: book(), deviceId: device.volume!, scan: () => [device], probe: async () => false });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('send-failed');
  expect((thrown as CliError).message.length).toBeGreaterThan(0);
});

test('a failed upload is send-failed, not a raw RemarkableUploadError', async () => {
  const refusing = Bun.serve({ port: 0, fetch: () => new Response('no', { status: 500 }) });
  let thrown: unknown;
  try {
    await sendCommand({
      file: book(),
      scan: () => [],
      probe: async () => true,
      remarkableEndpoint: `http://127.0.0.1:${refusing.port}`,
    });
  } catch (err) { thrown = err; }
  refusing.stop(true);
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('send-failed');
});

test('a file that is not there is unreadable, and no device is touched', async () => {
  let scanned = 0;
  let thrown: unknown;
  try {
    await sendCommand({
      file: join(tmpdir(), 'screepub-no-such-file.epub'),
      scan: () => { scanned += 1; return []; },
      probe: async () => false,
    });
  } catch (err) { thrown = err; }
  expect(thrown).toBeInstanceOf(CliError);
  expect((thrown as CliError).code).toBe('unreadable');
  // The file check comes FIRST: a missing file must not be reported as
  // no-devices, and must not cost the probe's timeout.
  expect(scanned).toBe(0);
});

test('a directory given as the file is unreadable, not send-failed', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'screepub-dir-'));
  let thrown: unknown;
  try {
    await sendCommand({ file: dir, scan: () => [kindleVolume()], probe: async () => false });
  } catch (err) { thrown = err; }
  expect((thrown as CliError).code).toBe('unreadable');
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-devices.test.ts`
Expected: FAIL — `sendCommand` is not exported.

- [ ] **Step 3: Write minimal implementation**

Append to `src/cli-devices.ts` (extend its imports with `statSync` from `node:fs`, `copyToDevice` from `./device/transfer`, and `remarkableAccepts`, `uploadToRemarkable` from `./device/remarkable`):

```ts
export interface SendOptions extends ListDevicesOptions {
  /** An EXISTING file. `send` never converts — see the design spec. */
  file: string;
  /** The id `devices` reports. Omitted: the single connected device. */
  deviceId?: string;
}

export interface SendResult {
  device: { id: string; kind: DeviceKind; name: string };
  /** Where the file landed. Absent for reMarkable, which has no path. */
  destination?: string;
  /** True for reMarkable, which reports no destination. */
  uploaded?: boolean;
}

/** Send an existing file to a connected reader.
 *
 * The file is checked FIRST, before the device list is built: a typo in the
 * filename must not be reported as "no devices", and must not pay the
 * reMarkable probe's timeout to find that out. */
export async function sendCommand(options: SendOptions): Promise<SendResult> {
  let isFile = false;
  try {
    isFile = statSync(options.file).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    throw new CliError('unreadable', `cannot read the file to send: ${options.file}`);
  }

  const devices = await listDevices(options);
  const device = selectDevice(devices, options.deviceId);
  const identity = { id: deviceId(device), kind: device.kind, name: device.name };

  if (device.kind === 'remarkable') {
    // Asked before the upload, not inferred from its error: cli-errors.ts's
    // rule is that detection is typed, never a substring of a message.
    if (!remarkableAccepts(options.file)) {
      throw new CliError(
        'unsupported-file',
        `reMarkable accepts PDF and EPUB only — ${options.file} is neither`,
      );
    }
    try {
      await uploadToRemarkable(options.file, options.remarkableEndpoint);
    } catch (err) {
      throw new CliError('send-failed', (err as Error).message);
    }
    return { device: identity, uploaded: true };
  }

  try {
    return { device: identity, destination: copyToDevice(options.file, device) };
  } catch (err) {
    throw new CliError('send-failed', (err as Error).message);
  }
}
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit`
Expected: PASS — 763 tests, `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/cli-devices.ts tests/cli-devices.test.ts
git commit -m "$(cat <<'EOF'
send: copy to a volume, upload to a reMarkable, one error contract

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 8: Wire `devices` into `cli.ts`

Dispatch runs at the top of `main()`, inside the existing top-level try/catch, so the `internal` catch-all still covers it. `jsonMode` keeps its raw-argv pre-scan: the verb parser can throw on an unknown flag before it would have told us the mode, exactly as the default parser can.

Two environment variables become the test seams for the spawned end-to-end tests, so **no spawned test reads a real mount or reaches a real network** either. They are read only by the device commands and change nothing on the conversion path.

**Files:**
- Modify: `src/cli.ts`
- Test: `tests/cli-device-commands.test.ts` (create)

**Interfaces:**
- Consumes: `resolveCommand`, `devicesCommand`, `type Verb` from `./cli-devices`; `CliError` from `./cli-errors`; `type ListDevicesOptions` from `./device/list`.
- Produces (internal to `cli.ts`): `deviceSeams(): ListDevicesOptions`, `parseVerbArgs(args: string[])`, `runVerb(verb: Verb, args: string[]): Promise<void>`.

- [ ] **Step 1: Write the failing test**

Create `tests/cli-device-commands.test.ts`:

```ts
import { afterAll, describe, test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-devcli-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** A mount parent with one Kobo in it. Injected through SCREEPUB_VOLUME_ROOTS
 * so the spawned CLI never enumerates this machine's real mounts. */
function mountRootWithKobo(): string {
  const root = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
  mkdirSync(join(root, 'KOBOeReader', '.kobo'), { recursive: true });
  return root;
}

const silent = Bun.serve({ port: 0, fetch: () => new Response('no', { status: 404 }) });
const SILENT_URL = `http://127.0.0.1:${silent.port}`;
afterAll(() => silent.stop(true));

async function runCli(args: string[], env: Record<string, string> = {}, cwd = ROOT) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
    cwd,
    env: { ...process.env, SCREEPUB_REMARKABLE_ENDPOINT: SILENT_URL, ...env },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}

/** The --json contract, asserted the same way everywhere: stdout is EXACTLY
 * one parseable object. `JSON.parse` alone would accept a leading log line in
 * some shapes, so the line count is asserted too. */
function soleJson(stdout: string): any {
  const lines = stdout.trim().split('\n');
  expect(lines).toHaveLength(1);
  return JSON.parse(lines[0]);
}

describe('screepub devices', () => {
  test('--json reports the injected device and exits 0', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mountRootWithKobo(),
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.devices).toHaveLength(1);
    expect(result.devices[0].kind).toBe('kobo');
    expect(result.devices[0].name).toBe('KOBOeReader');
    expect(result.devices[0].id).toBe(result.devices[0].volume);
    expect(result.devices[0].volume).toMatch(/KOBOeReader$/);
  });

  test('nothing connected is ok:true with an empty list and exit 0', async () => {
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    });
    // An empty list is an answer, not an error: an implementation that
    // reported no-devices here (as `send` correctly does) fails on BOTH.
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, devices: [] });
  });

  test('human output is one line per device on stdout', async () => {
    const { stdout, exitCode } = await runCli(['devices'], {
      SCREEPUB_VOLUME_ROOTS: mountRootWithKobo(),
    });
    expect(exitCode).toBe(0);
    expect(stdout.trim().split('\n')).toHaveLength(1);
    expect(stdout).toContain('KOBOeReader');
    expect(stdout).toContain('kobo');
  });

  test('human output says so when nothing is connected', async () => {
    const { stdout, exitCode } = await runCli(['devices'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    });
    expect(exitCode).toBe(0);
    expect(stdout.trim()).toBe('no devices connected');
  });

  test('an unknown flag on devices --json is still one JSON object', async () => {
    // The pre-scan case: parseArgs throws before it could report the mode.
    const { stdout, exitCode } = await runCli(['devices', '--json', '--no-such-flag']);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('usage');
  });

  test('devices takes no positional argument', async () => {
    const { stdout, exitCode } = await runCli(['devices', 'extra', '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
  });

  test('a real reMarkable stub appears in the listing', async () => {
    const answering = Bun.serve({ port: 0, fetch: () => new Response('[]', { status: 200 }) });
    const { stdout } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${answering.port}`,
    });
    answering.stop(true);
    expect(soleJson(stdout).devices).toEqual([
      { id: 'remarkable', kind: 'remarkable', name: 'reMarkable', volume: null },
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-device-commands.test.ts`
Expected: FAIL — `devices` is treated as an input filename, so the CLI reports `unsupported-type`.

- [ ] **Step 3: Write minimal implementation**

In `src/cli.ts`, extend the imports:

```ts
import { delimiter } from 'node:path';
import { resolveCommand, devicesCommand, sendCommand, type Verb } from './cli-devices';
import type { ListDevicesOptions } from './device/list';
import { mapConversionError, CliError, type JsonError } from './cli-errors';
```

Extend `USAGE`, after the `Usage:` block:

```
Commands:
  screepub devices [--json]                 list connected e-readers
  screepub send <file> [--device <id>] [--json]
                                            send an existing file to one

A verb is only a verb when no file of that name exists: a script saved as
"devices" still converts, and "./devices" always means the file.
```

Add, above `main()`:

```ts
/** Test seams, and a debugging hook for the Tauri shell: the mount roots to
 * scan and the reMarkable base URL. Read ONLY by the device commands — the
 * conversion path does not consult them. Unset means "the real thing". */
function deviceSeams(): ListDevicesOptions {
  const roots = process.env.SCREEPUB_VOLUME_ROOTS;
  const endpoint = process.env.SCREEPUB_REMARKABLE_ENDPOINT;
  return {
    roots: roots ? roots.split(delimiter).filter(Boolean) : undefined,
    remarkableEndpoint: endpoint || undefined,
  };
}

function parseVerbArgs(args: string[]) {
  return parseArgs({
    args,
    allowPositionals: true,
    options: {
      device: { type: 'string' },
      json: { type: 'boolean', default: false },
      help: { type: 'boolean', short: 'h', default: false },
    },
  });
}

async function runVerb(verb: Verb, args: string[]): Promise<void> {
  let parsed: ReturnType<typeof parseVerbArgs>;
  try {
    parsed = parseVerbArgs(args);
  } catch (err) {
    fail({ code: 'usage', message: (err as Error).message });
  }
  const { values, positionals } = parsed;
  jsonMode = values.json;

  if (values.help) {
    console.log(USAGE);
    process.exit(0);
  }

  try {
    if (verb === 'devices') {
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `devices takes no arguments (got "${positionals[0]}")` });
      }
      const { devices } = await devicesCommand(deviceSeams());
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, devices }));
        return;
      }
      if (devices.length === 0) {
        console.log('no devices connected');
        return;
      }
      for (const d of devices) console.log(`${d.name} (${d.kind}) — ${d.id}`);
      return;
    }
    // verb === 'send' is wired in Task 9.
    fail({ code: 'usage', message: `unimplemented command "${verb}"` });
  } catch (err) {
    if (err instanceof CliError) fail(err.toJson());
    throw err;
  }
}
```

and as the first statement of `main()`:

```ts
async function main() {
  // Dispatch BEFORE parseArgs: a verb's flags are not the conversion flags.
  // jsonMode's raw-argv pre-scan above already holds for this path, so a
  // throw inside the verb parser still exits as one JSON object.
  const command = resolveCommand(process.argv.slice(2));
  if (command.kind === 'verb') {
    await runVerb(command.verb, command.args);
    return;
  }

  let parsed: ReturnType<typeof parseCliArgs>;
  // ...unchanged from here
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git status --short app/`
Expected: PASS — 770 tests, `tsc` clean, and `git status --short app/` prints nothing.

- [ ] **Step 5: Commit**

```bash
git add src/cli.ts tests/cli-device-commands.test.ts
git commit -m "$(cat <<'EOF'
screepub devices

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 9: Wire `send` into `cli.ts`

The second verb, and with it the four remaining error codes on the wire.

**Files:**
- Modify: `src/cli.ts`
- Test: `tests/cli-device-commands.test.ts` (append)

**Interfaces:**
- Consumes: `sendCommand(options: SendOptions): Promise<SendResult>` (Task 7).
- Produces: no new exports. `runVerb`'s `send` branch, and the JSON body `{ ok: true, device, destination? | uploaded? }`.

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-device-commands.test.ts`:

```ts
import { readFileSync, writeFileSync } from 'node:fs';

function kindleRoot(): { root: string; volume: string } {
  const root = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
  const volume = join(root, 'Kindle');
  mkdirSync(join(volume, 'documents'), { recursive: true });
  return { root, volume };
}

function book(name = 'Script.epub'): string {
  const path = join(mkdtempSync(join(tmpdir(), 'screepub-book-')), name);
  writeFileSync(path, 'book-bytes');
  return path;
}

describe('screepub send', () => {
  test('--json reports the device and the destination, and the bytes moved', async () => {
    const { root, volume } = kindleRoot();
    const file = book();
    const { stdout, exitCode } = await runCli(['send', file, '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.ok).toBe(true);
    expect(result.device).toEqual({ id: volume, kind: 'kindle', name: 'Kindle' });
    expect(result.destination).toBe(join(volume, 'documents', 'Script.epub'));
    expect(readFileSync(result.destination, 'utf8')).toBe('book-bytes');
  });

  test('a reMarkable send reports uploaded:true and NO destination key', async () => {
    const posted: string[] = [];
    const tablet = Bun.serve({
      port: 0,
      async fetch(req) {
        if (req.method === 'POST') { posted.push(new URL(req.url).pathname); await req.arrayBuffer(); }
        return new Response('[]', { status: 200 });
      },
    });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${tablet.port}`,
    });
    tablet.stop(true);
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.uploaded).toBe(true);
    // The key must be ABSENT, not null: the spec says the field is omitted.
    // `expect(result.destination).toBeUndefined()` would also pass on null.
    expect('destination' in result).toBe(false);
    expect(posted).toEqual(['/upload']);
  });

  test('--device picks one of several, and the others are untouched', async () => {
    const root = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
    const kindle = join(root, 'Kindle');
    const kobo = join(root, 'KOBOeReader');
    mkdirSync(join(kindle, 'documents'), { recursive: true });
    mkdirSync(join(kobo, '.kobo'), { recursive: true });
    const { stdout, exitCode } = await runCli(['send', book(), '--device', kobo, '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(0);
    const result = soleJson(stdout);
    expect(result.device.kind).toBe('kobo');
    expect(result.destination).toBe(join(kobo, 'Script.epub'));
    // Two devices were connected: an implementation that ignored --device and
    // took the first would have written into the Kindle instead.
    expect(readFileSync(join(kobo, 'Script.epub'), 'utf8')).toBe('book-bytes');
  });

  test('several connected and no --device is ambiguous-device, naming both', async () => {
    const root = mkdtempSync(join(tmpdir(), 'screepub-mounts-'));
    const kindle = join(root, 'Kindle');
    const kobo = join(root, 'KOBOeReader');
    mkdirSync(join(kindle, 'documents'), { recursive: true });
    mkdirSync(join(kobo, '.kobo'), { recursive: true });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.error.code).toBe('ambiguous-device');
    expect(result.error.message).toContain(kindle);
    expect(result.error.message).toContain(kobo);
  });

  test('nothing connected is no-devices', async () => {
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    });
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('no-devices');
  });

  test('an id nothing matches is unknown-device', async () => {
    const { root } = kindleRoot();
    const { stdout, exitCode } = await runCli(['send', book(), '--device', '/nope', '--json'], {
      SCREEPUB_VOLUME_ROOTS: root,
    });
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unknown-device');
  });

  test('an extension reMarkable cannot read is unsupported-file', async () => {
    const tablet = Bun.serve({ port: 0, fetch: () => new Response('[]', { status: 200 }) });
    const { stdout, exitCode } = await runCli(['send', book('Script.azw3'), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${tablet.port}`,
    });
    tablet.stop(true);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unsupported-file');
  });

  test('a tablet that refuses the upload is send-failed', async () => {
    const refusing = Bun.serve({
      port: 0,
      fetch: (req) => new Response('[]', { status: req.method === 'POST' ? 500 : 200 }),
    });
    const { stdout, exitCode } = await runCli(['send', book(), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
      SCREEPUB_REMARKABLE_ENDPOINT: `http://127.0.0.1:${refusing.port}`,
    });
    refusing.stop(true);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('send-failed');
  });

  test('no file argument is a usage error', async () => {
    const { stdout, exitCode } = await runCli(['send', '--json']);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('usage');
  });

  test('a file that is not there is unreadable', async () => {
    const { stdout, exitCode } = await runCli(['send', join(SCRATCH, 'ghost.epub'), '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    });
    expect(exitCode).toBe(1);
    // Not no-devices: the file is checked before the device list is built.
    expect(soleJson(stdout).error.code).toBe('unreadable');
  });

  test('human output names the device and the destination', async () => {
    const { root, volume } = kindleRoot();
    const { stdout, exitCode } = await runCli(['send', book()], { SCREEPUB_VOLUME_ROOTS: root });
    expect(exitCode).toBe(0);
    expect(stdout).toContain('Kindle');
    expect(stdout).toContain(join(volume, 'documents', 'Script.epub'));
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-device-commands.test.ts`
Expected: FAIL — `send` reports `usage: unimplemented command "send"`.

- [ ] **Step 3: Write minimal implementation**

In `src/cli.ts`, replace the `// verb === 'send' is wired in Task 9.` placeholder and its `fail(...)` line inside `runVerb` with:

```ts
    if (positionals.length !== 1) {
      fail({ code: 'usage', message: 'expected exactly one file to send (see --help)' });
    }
    const sent = await sendCommand({
      file: positionals[0],
      deviceId: values.device,
      ...deviceSeams(),
    });
    if (jsonMode) {
      // `destination` is OMITTED for reMarkable, which has no path; `uploaded`
      // takes its place. Two shapes, one object, per the design spec.
      console.log(
        JSON.stringify({
          ok: true,
          device: sent.device,
          ...(sent.destination !== undefined ? { destination: sent.destination } : { uploaded: true }),
        }),
      );
      return;
    }
    console.log(
      sent.destination !== undefined
        ? `sent ${basename(positionals[0])} to ${sent.device.name} — ${sent.destination}`
        : `sent ${basename(positionals[0])} to ${sent.device.name}`,
    );
```

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git status --short app/`
Expected: PASS — 781 tests, `tsc` clean, nothing under `app/`.

- [ ] **Step 5: Commit**

```bash
git add src/cli.ts tests/cli-device-commands.test.ts
git commit -m "$(cat <<'EOF'
screepub send, with the five device error codes on the wire

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

### Task 10: Pin the default path, both directions of shadowing, and document it

`resolveCommand` is unit-tested, but the compatibility claim is about the *binary*: `screepub <file>` must behave identically to before, and the shadowing rule must hold end to end with a real file on a real disk, not an injected `exists`.

**Files:**
- Test: `tests/cli-device-commands.test.ts` (append), `tests/cli.test.ts` (append only — never edit an existing test there)
- Modify: `README.md`

**Interfaces:** none new. This task adds tests and documentation only.

- [ ] **Step 1: Write the failing test**

Append to `tests/cli-device-commands.test.ts`:

```ts
const FIXTURES = new URL('./fixtures/', import.meta.url).pathname;

describe('verb dispatch does not capture files', () => {
  test('a file literally named `devices` converts instead of listing', async () => {
    // Direction one of the shadowing rule, end to end, with a real file in a
    // real cwd. `devices` has no extension, so the conversion path rejects it
    // as unsupported-type — which is exactly the proof that the FILE won: the
    // verb would have printed {"ok":true,"devices":[...]} and exited 0.
    const dir = mkdtempSync(join(tmpdir(), 'screepub-shadow-'));
    writeFileSync(join(dir, 'devices'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    }, dir);
    expect(exitCode).toBe(1);
    const result = soleJson(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('unsupported-type');
  });

  test('the bare word lists devices in the same cwd with no such file', async () => {
    // Direction two. Same command, same cwd shape, only the file removed —
    // so the two tests differ in exactly the thing the rule is about.
    const dir = mkdtempSync(join(tmpdir(), 'screepub-shadow-'));
    const { stdout, exitCode } = await runCli(['devices', '--json'], {
      SCREEPUB_VOLUME_ROOTS: mkdtempSync(join(tmpdir(), 'screepub-empty-')),
    }, dir);
    expect(exitCode).toBe(0);
    expect(soleJson(stdout)).toEqual({ ok: true, devices: [] });
  });

  test('./devices always means the file', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-shadow-'));
    writeFileSync(join(dir, 'devices'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['./devices', '--json'], {}, dir);
    expect(exitCode).toBe(1);
    expect(soleJson(stdout).error.code).toBe('unsupported-type');
  });

  test('a file named `send` converts rather than being a command', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'screepub-shadow-'));
    writeFileSync(join(dir, 'send'), 'not a pdf');
    const { stdout, exitCode } = await runCli(['send', '--json'], {}, dir);
    expect(exitCode).toBe(1);
    // Without the shadowing rule this is a usage error from `send` with no
    // file argument; with it, the file is the input and its type is wrong.
    expect(soleJson(stdout).error.code).toBe('unsupported-type');
  });
});
```

and append to `tests/cli.test.ts` (adding nothing else to that file):

```ts
describe('the default conversion path is unchanged by verb dispatch', () => {
  test('a PDF whose stem is a verb converts exactly as any other would', async () => {
    // Names that brush against dispatch: "send.pdf" starts with a verb, and
    // "devices.pdf" is the shadowing rule's near miss. Both must take the
    // ordinary path and produce the ordinary success payload.
    for (const name of ['send.pdf', 'devices.pdf']) {
      const input = `${SCRATCH}/${name}`;
      writeFileSync(input, new Uint8Array(await Bun.file(`${FIXTURES}screenplay.pdf`).arrayBuffer()));
      const out = `${SCRATCH}/${name}.epub`;
      const { stdout, exitCode } = await runCli([input, '-o', out, '--no-fountain', '--json']);
      expect(exitCode).toBe(0);
      const result = JSON.parse(stdout);
      expect(result.ok).toBe(true);
      expect(result.epubPath).toBe(out);
      expect(result.pages).toBeGreaterThan(0);
    }
  }, 120000);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/cli-device-commands.test.ts tests/cli.test.ts`
Expected: the four shadowing tests run against the code from Tasks 8–9 and **should already pass** — they are regression pins, not drivers. To prove they can fail, temporarily delete the `if (exists(first)) return { kind: 'convert' };` line in `src/cli-devices.ts`, re-run, and confirm the "file literally named `devices`" and "file named `send`" tests fail; then restore the line. The `tests/cli.test.ts` addition likewise passes immediately and fails if `resolveCommand` is changed to match on a path's stem.

- [ ] **Step 3: Write minimal implementation**

No source change. Update the CLI section of `README.md`: after the existing options table and before the `.fountain` paragraph, insert

````markdown
#### Device commands

```bash
bun src/cli.ts devices [--json]                          # list connected e-readers
bun src/cli.ts send <file> [--device <id>] [--json]      # send an existing file to one
```

`devices` lists every reader it can reach: USB-mounted Kindle, Kobo and
tolino volumes, plus a reMarkable if its USB web interface is answering.
`send` copies an existing file where that vendor actually indexes it — it
does **not** convert, so run a conversion first. With one reader connected
`--device` is optional; with several it is required, and `devices` prints the
ids it accepts.

A verb is only a verb when no file of that name exists, so a script saved as
`devices` still converts and `./devices` always means the file.
````

- [ ] **Step 4: Run the suite**

Run: `bun test && bunx tsc --noEmit && git status --short app/`
Expected: PASS — 786 tests, `tsc` clean, nothing under `app/`.

- [ ] **Step 5: Commit**

```bash
git add tests/cli-device-commands.test.ts tests/cli.test.ts README.md
git commit -m "$(cat <<'EOF'
Pin the default path and both directions of the shadowing rule

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01EwcqvziTDJLjcRbJF8ppbe
EOF
)"
```

---

## Final Verification

The spec's acceptance criteria, as checkable steps.

- [ ] **1. The whole existing suite still passes; `bunx tsc --noEmit` clean**

Run: `bun test && bunx tsc --noEmit`
Expected: all pass (734 pre-existing + ~52 new), 3 skipped as before, `tsc` silent.

- [ ] **2. `screepub <file>` behaves identically to before**

Run: `bun test tests/cli.test.ts tests/convert.test.ts`
Expected: pass. `tests/cli.test.ts` is the default path's golden master and was only appended to; confirm with `git diff main -- tests/cli.test.ts` that every hunk is an addition, no line removed or changed.

- [ ] **3. No file under `app/` is modified**

Run: `git diff --stat main -- app/`
Expected: no output.

- [ ] **4. Every new error path emits exactly one JSON object on stdout under `--json`**

Run: `bun test tests/cli-device-commands.test.ts`
Expected: pass. Then confirm the ledger by hand — there is a spawned test asserting `soleJson` (one line, parseable, `ok:false`) for each of `no-devices`, `ambiguous-device`, `unknown-device`, `send-failed`, `unsupported-file`, `unreadable` (missing file) and `usage` (unknown flag, missing file argument, extra positional).

- [ ] **5a. `devices` and `send` work against injected/stubbed devices**

Run: `bun test tests/device-list.test.ts tests/cli-devices.test.ts tests/cli-device-commands.test.ts`
Expected: pass. Then confirm no test reads a real mount or a real network:

```bash
grep -rn "SCREEPUB_VOLUME_ROOTS\|10\.11\.99\.1\|/Volumes\|/run/media" tests/device-list.test.ts tests/cli-devices.test.ts tests/cli-device-commands.test.ts
```
Expected: every hit is either a `SCREEPUB_VOLUME_ROOTS` injection or a hard-coded fake path in an in-process fixture object (`/run/media/you/Kindle` in `tests/cli-devices.test.ts`, which is never touched on disk). No `10.11.99.1` anywhere, and every `Bun.serve` stub is stopped in an `afterAll` or immediately after use.

- [ ] **5b. `devices` runs for real on this machine**

Run: `bun src/cli.ts devices --json`
Expected: one JSON object, `ok:true`, and a `devices` array — empty is a pass, since nothing is plugged in here. Note the wall clock: it should be roughly the probe's 1.5 s timeout, not appreciably more. Then:

Run: `bun src/cli.ts devices`
Expected: `no devices connected` on stdout, exit 0.

- [ ] **6. The handlers really are printer-free**

Run: `grep -n "console\." src/cli-devices.ts src/device/list.ts`
Expected: no output. If a handler prints, it is untestable without spawning and the plan's architecture has been violated.

- [ ] **7. Nothing on the conversion path reads the new env vars**

Run: `grep -rn "SCREEPUB_VOLUME_ROOTS\|SCREEPUB_REMARKABLE_ENDPOINT" src/`
Expected: hits only in `src/cli.ts`'s `deviceSeams()`. They are device-command seams; a conversion must behave identically whether or not they are set.
