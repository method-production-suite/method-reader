# Send from Convert, part 1: the engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the engine everything the new result screen needs: three remembered app settings, saving any file type on request, ejecting a reader, and handing the window the first page of a script while it converts.

**Architecture:** Four independent engine changes, each behind the CLI the window already calls (`bun src/cli.ts <verb> ... --json`). Each verb keeps the existing house pattern: a handler in `src/cli-<verb>.ts` that RETURNS a result and never prints, its flags declared once in `src/cli.ts`'s `VERB_FLAGS`, errors as `CliError` with a contract code from `src/cli-errors.ts`, and every OS-touching step behind an injectable seam so tests never touch a real disk, drive or Calibre.

**Tech Stack:** Bun + TypeScript, `bun:test`.

**Spec:** `docs/superpowers/specs/2026-10-01-send-from-convert-design.md`, decisions 3, 11, 12, 17 and 21 (approved 2026-10-01).

**Working rules** (from `CLAUDE.md` and the owner):
- TDD: the failing test first. Run `bun test` from the worktree root.
- No test reads or writes the real app settings or the real Calibre folder (the preloads guard both). NEVER run Calibre, `calibre-customize`, `calibre-debug`, or an engine function that spawns them outside `bun test` from the root; a manual check sets `CALIBRE_CONFIG_DIRECTORY` to a scratch folder first.
- No test ejects a real drive. `eject` runs its OS command only through the injected runner.
- No em dash in any text a person reads, or in new comments. Never a real screenplay name; fixtures are the invented ones in `tests/fixtures/`.
- Each git command on its own (no `&&` chains with git). Commit per task. Never push or merge.
- Every new verb or flag goes into `VERB_FLAGS` so `tests/cli-verb-flags.test.ts`'s matrix covers it.

---

## File map

| File | Change |
| --- | --- |
| `src/settings/app.ts` | `AppSettings` gains `readerDevice`, `animatePage`, `typewriterSounds` |
| `src/cli-app-settings.ts` | the three keys become writable and are reported, with defaults |
| `src/export/formats.ts` | `ExportFormat` gains `kfx`, `azw3`, `mobi`, `fountain` |
| `src/cli-export.ts` | `--for` accepts every format; each explicit rung builds only itself |
| `src/cli-eject.ts` | NEW: the `eject` verb's handler |
| `src/device/eject.ts` | NEW: the per-OS eject command and its runner seam |
| `src/cli-errors.ts` | new code `eject-failed` |
| `src/parser/extract.ts`, `src/convert.ts`, `src/cli.ts` | the first-page progress line |
| `tests/app-settings*.test.ts`, `tests/cli-app-settings.test.ts`, `tests/cli-export.test.ts`, NEW `tests/cli-eject.test.ts`, NEW `tests/first-page.test.ts` | tests |
| `docs/developers.md` | the CLI reference: new values and verb |

---

### Task 1: Three remembered app settings

**Files:** `src/settings/app.ts`, `src/cli-app-settings.ts`, `tests/cli-app-settings.test.ts`

The window remembers the reader's device (spec decision 3) and two animation switches (decision 21) in the app settings file, written through `screepub app-settings --set`, the way `keepScriptSettings` already is.

- `readerDevice`: one of `kindle`, `apple`, `kobo`, `tolino`, `remarkable`, or absent. `null` in a `--set` removes it. Anything else is refused with `bad-settings` before anything is written.
- `animatePage`: boolean, default `true`.
- `typewriterSounds`: boolean, default `false`.
- Reported by `screepub app-settings --json` as `readerDevice` (string or `null`), `animatePage` and `typewriterSounds` (always booleans, defaults applied), beside the existing keys.

- [ ] **Step 1: Write the failing tests** in `tests/cli-app-settings.test.ts`, following that file's existing helpers for a scratch settings path (read the top of the file first). Cover:
  - a fresh file reports `readerDevice: null`, `animatePage: true`, `typewriterSounds: false`;
  - `--set '{"readerDevice":"kobo"}'` stores it and the next read reports `"kobo"`; `--set '{"readerDevice":null}'` removes it;
  - `--set '{"readerDevice":"nook"}'` fails with code `bad-settings` and leaves the file byte-identical;
  - `--set '{"animatePage":false,"typewriterSounds":true}'` stores both; a non-boolean for either is `bad-settings`;
  - a `--set` of one key keeps every other key in the file (`lastRoute`, `libraryPath`, `keepScriptSettings` already present).
- [ ] **Step 2: Run** `bun test tests/cli-app-settings.test.ts` and see the new tests fail.
- [ ] **Step 3: Implement.** In `src/settings/app.ts` add to `AppSettings`:

```ts
  readerDevice?: ReaderDevice;
  animatePage?: boolean;
  typewriterSounds?: boolean;
```
and export
```ts
export const READER_DEVICES = ['kindle', 'apple', 'kobo', 'tolino', 'remarkable'] as const;
export type ReaderDevice = (typeof READER_DEVICES)[number];
```
In `src/cli-app-settings.ts`, extend `WRITABLE_KEYS` with the three keys, validate each in the same place `keepScriptSettings` is validated (`keepsScriptSettings` is the model: a pure function from the validated patch that returns the value, `undefined` to leave it, or throws `bad-settings`), and add the three fields to the result type and the result object with the defaults above.
- [ ] **Step 4: Run** the file again; all pass. Run `bun test tests/cli-verb-flags.test.ts tests/app-settings.test.ts` too.
- [ ] **Step 5: Commit** `"app-settings: remember the reader's device and the two animation switches"`.

---

### Task 2: Save any file type on request

**Files:** `src/export/formats.ts`, `src/cli-export.ts`, `src/cli.ts` (help text only), `tests/cli-export.test.ts`

`screepub export <book.epub> --for epub|kindle|kfx|azw3|mobi|fountain` (spec decision 11).

- `epub`, `kindle`: unchanged. `kindle` is still "the best rung this computer reaches".
- `kfx`: builds only KFX. With the KFX toolchain not ready, it refuses with `export-failed`: `this computer cannot make a KFX file yet: <what kfxStatus says is missing>`. It never falls back to AZW3.
- `azw3`: builds only AZW3 through Calibre (`toAzw3`), even when KFX is ready. Without Calibre: `export-failed`, `this computer cannot make an AZW3 file without Calibre`.
- `mobi`: the engine's own MOBI, rebuilt from `--fountain` (required for this value: `usage` without it), written beside the EPUB as `<name>.mobi`, never through Calibre.
- `fountain`: the script's `.fountain`, which must be given with `--fountain`; the result's `path` is that file (with `--out`, a copy).
- `--out` and `--check` work for every value, with the same rules as today (`--out` absolute, its extension must match: `.kfx`, `.azw3`, `.mobi`, `.fountain`, `.epub`). `--check` for `kfx`/`azw3`/`mobi` reports `fresh`, `builtBy` and `bookDate` the way `kindle` does today, for that one rung.
- An unknown value: `usage`, `--for takes epub, kindle, kfx, azw3, mobi or fountain, not "<value>"`.

- [ ] **Step 1: Write the failing tests** in `tests/cli-export.test.ts`, using its existing `beforeAll` EPUB and per-test folder helpers (read them first), and injecting `deps.kfxStatus` / `deps.calibreAvailable` so no test needs real Calibre:
  - `--for fountain --fountain <f>` returns that path and `extension: 'fountain'`; with `--out /abs/x.fountain` the copy exists and matches byte for byte; without `--fountain` it is `usage`.
  - `--for mobi --fountain <f>` writes `<name>.mobi` beside the EPUB (a real MOBI: starts with the PalmDB header the existing mobi tests check) and never calls the injected Calibre seams.
  - `--for kfx` with `kfxStatus` not ready is `export-failed` and its message names what is missing; it writes nothing.
  - `--for azw3` with `calibreAvailable: () => false` is `export-failed`.
  - `--for azw3 --check` with a fresh `.azw3` sibling (touch it newer than the EPUB) reports `fresh: true`, `builtBy: 'calibre'`.
  - `--for nook` is `usage` with the sentence above.
  - `--out` with the wrong extension for each new value is `usage` (reuse `checkOutExtension`).
  Real KFX and AZW3 builds stay covered only by the existing real-toolchain test, which skips without the tools.
- [ ] **Step 2: Run** and see them fail.
- [ ] **Step 3: Implement.** `ExportFormat` becomes `'epub' | 'kindle' | 'kfx' | 'azw3' | 'mobi' | 'fountain'`. In `exportCommand`, replace the two-value check with a set of the six. Keep the `epub` branch. Add a `fountain` branch before any toolchain probe (it needs none). For `kfx`, `azw3` and `mobi`, add one explicit-rung function beside `freshKindleArtifact` in `src/export/artifact.ts`:

```ts
/** One named rung, never the ladder: the caller asked for exactly this
 *  file. Same staleness rule as kindleArtifactPlan (a sibling no older
 *  than its EPUB is reused). */
export async function freshArtifactOf(
  rung: 'kfx' | 'azw3' | 'mobi',
  opts: FreshKindleArtifactOptions,
): Promise<string>
```
built from the same pieces `freshKindleArtifact` uses (`toKfx`, `toAzw3`, the MOBI rebuild), so there is still one copy of each rung's work. `kindleArtifactPlan`'s staleness check is reused per rung (add a `rungPlan(rung, epub)` that reports `{ path, fresh, builtBy, bookDate }` for one rung, and have `kindleArtifactPlan` call it for the rung it picks). Update the `export` help text in `src/cli.ts` to list the six values.
- [ ] **Step 4: Run** `bun test tests/cli-export.test.ts tests/export-*.test.ts tests/cli-verb-flags.test.ts`; all pass.
- [ ] **Step 5: Commit** `"export --for: any file type on request, each rung only itself"`.

---

### Task 3: Eject a reader

**Files:** NEW `src/device/eject.ts`, NEW `src/cli-eject.ts`, `src/cli.ts`, `src/cli-errors.ts`, NEW `tests/cli-eject.test.ts`

`screepub eject <volume> [--json]` (spec decision 12).

- `<volume>` must be absolute (`usage` otherwise).
- It must be the volume of a reader the engine itself lists right now (`listDevices({ skipRemarkable: true })`, comparing resolved paths). Anything else: `unknown-device`, `not a reader Screepub can see: <volume>`. So the window can never eject an arbitrary disk.
- The OS command, chosen by platform:
  - macOS: `diskutil eject <volume>`
  - Linux: `udisksctl unmount -b <device>` then `udisksctl power-off -b <device>`, where `<device>` is read from `findmnt -n -o SOURCE --target <volume>`
  - Windows: PowerShell `(New-Object -comObject Shell.Application).Namespace(17).ParseName('<drive>').InvokeVerb('Eject')` with `<drive>` like `E:`
- A non-zero exit, or a timeout of 20 seconds, is `eject-failed` carrying the command's stderr first line, e.g. a volume still in use. Success returns `{ ejected: <volume> }`.

- [ ] **Step 1: Write the failing tests** in `tests/cli-eject.test.ts`, calling `ejectCommand(volume, deps)` directly with injected `devices` and `run` (no process is ever started):
  - a relative path is `usage` and `run` is never called;
  - a volume not in the injected device list is `unknown-device` and `run` is never called;
  - on `darwin` it runs exactly `['diskutil', 'eject', '/Volumes/Kindle']` and returns `{ ejected: '/Volumes/Kindle' }`;
  - on `linux` it runs `findmnt`, then `udisksctl unmount -b /dev/sdb1`, then `udisksctl power-off -b /dev/sdb1`, in that order (the injected runner answers `findmnt` with `/dev/sdb1\n`);
  - on `win32` the PowerShell line names the drive `E:` for volume `E:\`;
  - a runner that exits 1 with stderr `Volume Kindle on disk4 failed to eject\n` is `eject-failed` and the message carries that line;
  - a runner that never resolves is `eject-failed` after the injected timeout (pass `timeoutMs: 20`);
  - the verb is in `VERB_FLAGS` with no flags, so `tests/cli-verb-flags.test.ts` refuses every foreign flag for it (add `eject` to that test's verb list if it does not read `VERB_FLAGS` itself).
- [ ] **Step 2: Run** and see them fail.
- [ ] **Step 3: Implement.** `src/device/eject.ts`:

```ts
export interface EjectRun {
  (argv: string[], timeoutMs: number): Promise<{ code: number | null; stdout: string; stderr: string }>;
}
export const EJECT_TIMEOUT_MS = 20_000;
/** The per-OS argv lists for one eject; pure, so tests read them. */
export function ejectCommands(volume: string, platform: NodeJS.Platform): string[][] | 'needs-device';
```
plus a real `spawnEject: EjectRun` using `Bun.spawn` with `env: process.env`, a kill-after timeout, and stdout/stderr collected. `src/cli-eject.ts` exports `ejectCommand(volume, deps: { platform?, devices?, run?, timeoutMs? })` following `src/cli-reveal.ts`'s shape (returns, never prints). Add `'eject-failed'` to the `CliError` code union with one comment line. In `src/cli.ts`: add `eject` to the verbs, `eject: []` to `VERB_FLAGS`, an `EJECT_USAGE` help text, and a dispatch block modelled on `reveal`'s (exactly one positional; `--json` prints `{ ok: true, ...result }`; human mode prints `Ejected <volume>. It is safe to unplug.`).
- [ ] **Step 4: Run** `bun test tests/cli-eject.test.ts tests/cli-verb-flags.test.ts tests/cli.test.ts`; all pass.
- [ ] **Step 5: Commit** `"screepub eject: a reader the engine can see, and nothing else"`.

---

### Task 4: The first page, early

**Files:** `src/parser/extract.ts`, `src/convert.ts`, `src/cli.ts`, NEW `tests/first-page.test.ts`

Under `--progress`, the engine writes one extra NDJSON line on stderr as soon as it has read the first page that is not a title page (spec decision 17):

```json
{"progress":{"stage":"page","page":2,"paragraphs":["EXT. RIDGE ROAD - DAY","Wind comes down off the ridge in long flat gusts, pushing ..."]}}
```

- `paragraphs` is that page's text as plain paragraphs: lines in reading order, joined with single spaces within a paragraph, a new paragraph wherever the vertical gap between two lines is larger than 1.5 times the page's most common line gap. No formatting, no styling markers, no `[[notes]]`.
- "Not a title page": the first page, in order, that has at least 8 lines and whose lines are not mostly centred (fewer than half of its lines sit within 10% of the page's horizontal centre). If page 1 qualifies, it is used.
- Sent at most once per conversion, and never when conversion stops early (scanned, not a screenplay) before that page is read. Pages after it keep the existing `{"progress":{"stage":"parse","percent":N}}` lines.
- Nothing changes without `--progress`, and nothing changes on stdout (the `--json` answer is the app's contract).

- [ ] **Step 1: Write the failing tests** in `tests/first-page.test.ts`:
  - a pure `pageParagraphs(lines: RawLine[])` on hand-built lines: a gap of 2x the line gap starts a new paragraph; a wrapped line joins its paragraph with one space;
  - a pure `isTitleLike(lines, pageWidth)` true for a mostly-centred 6-line page and false for a left-aligned 30-line page;
  - spawning `bun src/cli.ts tests/fixtures/field-station.pdf --json --progress -o <scratch>` (follow how `tests/cli.test.ts` spawns the CLI and makes scratch output): stderr has exactly one `stage: "page"` line; its `page` is the first page after Field Station's title page; its first paragraph is the scene heading the Read page shows first; no paragraph contains `**`, `_` or `[[`; the stdout JSON is unchanged in shape.
- [ ] **Step 2: Run** and see them fail.
- [ ] **Step 3: Implement.** In `src/parser/extract.ts`, widen `PageProgress` to `(page: number, pages: number, lines?: RawLine[]) => void` and pass `pageLines` on each call. Export `pageParagraphs` and `isTitleLike` from a new small module `src/parser/first-page.ts` (pure, imported by the tests). In `src/convert.ts`, `convertPdf` takes `opts.onFirstPage?: (page: number, paragraphs: string[]) => void` and calls it once, from the per-page callback, for the first page `isTitleLike` rejects and that has at least 8 lines. In `src/cli.ts`, when `--progress` is set, pass an `onFirstPage` that writes the line above with `JSON.stringify` (the same writer the existing progress uses).
- [ ] **Step 4: Run** `bun test tests/first-page.test.ts tests/cli.test.ts tests/convert.test.ts`; all pass. Also run the corpus check the repo uses for parser-adjacent changes on the committed fixtures: `bun test tests/fixture-stability.test.ts tests/field-station.test.ts` (the parse result itself must not change).
- [ ] **Step 5: Commit** `"--progress hands the window the first real page as plain paragraphs"`.

---

### Task 5: The CLI reference

**Files:** `docs/developers.md`

- [ ] **Step 1:** In the CLI section, list `export --for epub|kindle|kfx|azw3|mobi|fountain` with one line each, add `eject <volume>`, the three new `app-settings` keys, and the `stage: "page"` progress line. Plain words, no em dashes.
- [ ] **Step 2:** Run `bun test` (whole suite) and `bunx tsc --noEmit`. Expected: 0 fail.
- [ ] **Step 3: Commit** `"developers.md: the new export values, eject, and the first-page progress line"`.
