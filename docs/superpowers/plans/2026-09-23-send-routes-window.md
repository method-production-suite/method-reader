# Send Routes, Window Half: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Send page lists every route the engine answers, in its order, with the remembered one first and brass, and performs each one: devices as today, Apple Books / Amazon / Mail through `screepub route`, and both saves through a save dialog plus `screepub route save-* --out`.

**Architecture:** `app.js` gains two argv builders and a `saveDialog` wrapper (it stays the only file that touches Tauri). `send.js` swaps its `devices` poll for a `routes` poll and draws route rows; its pure decisions stay above the line and are tested directly. One new permission, `dialog:allow-save`, approved by the owner on 2026-09-23.

**Tech Stack:** plain ES modules in `desktop/ui/`, Tauri 2 capability JSON, bun:test.

**Spec:** [2026-09-23-send-routes-design.md](../specs/2026-09-23-send-routes-design.md) ("The window"). **Depends on** the engine half ([plan](2026-09-23-send-routes-engine.md)) being merged into this branch first: `routes`, `route`, `export --out`.

---

## Ground rules for every task

- Worktree `<repo>/.claude/worktrees/epic-neumann-254843`, branch `parity-b-routes`. Absolute paths.
- TDD with bun:test. `bunx tsc --noEmit` and the full `bun test` green before each commit.
- No em dash (`—`) in any string you add to the window, docs or tests' fixtures a person reads.
- `desktop/ui/app.js` is the only file that touches `window.__TAURI__` or knows an engine flag. Every other file imports from it.
- Existing window rules stay green: `tests/desktop-ui.test.ts` (no hex outside the token files, no raw px font sizes, the argv builder pin, send.js's shape tests) and `tests/desktop-shell.test.ts` (capability rules). If a send.js shape test fails because the flow legitimately changed, update that test in the same commit and say in its comment what it now guards; never delete a guard without replacing what it protected.
- Coordination: `desktop/ui/app.js` (outside `argv` and `countsTowardBusy`), `frame.js`, `main.js`, `update.js`, `notes-surface.js`, `convert.js`, `style.css` belong to other work. Piece C (branch `parity-c-gear`, in parallel) edits `convert.js`, `tune.js`, `app.js` argv, the capability file and `tests/desktop-shell.test.ts`: keep your edits in those shared files small and local.
- Commit trailer `Co-Authored-By: <your model name> <noreply@anthropic.com>`.

---

### Task W1: The door, the builders, and the ADR

**Files:** `desktop/ui/app.js`, `desktop/src-tauri/capabilities/default.json`, `tests/desktop-shell.test.ts`, `tests/desktop-ui.test.ts`, `docs/adr/2026-09-21-doors-not-commands.md`, `desktop/README.md`.

- `argv.routes(epub)` → `['routes', epub, '--json']`; `argv.route(key, epub, { out = null, fountain = null, optionsJson = null } = {})` → `['route', key, epub, '--json', ...(out ? ['--out', out] : []), ...fountain..., ...optionsJson...]` (same null-filter idiom as `argv.export`). Extend the argv pin test and the "every builder passes --json" test.
- `countsTowardBusy`: `routes` does not count (it replaces the `devices` poll; say so in the comment beside the `devices` and `kfx-status` reasons). `route` DOES count (it writes: saves a file, opens apps).
- `export async function saveDialog({ defaultPath, filters })`: `tauri().dialog.save({ defaultPath, filters })`, resolving to the chosen path or null on cancel; it goes through the same one-dialog-at-a-time guard and the `onDialogClosed` handlers `pickScreenplay` uses (read that function and share its guard, so Ctrl-O and a save dialog can never both be up).
- Capability: add the bare string `"dialog:allow-save"`. In `tests/desktop-shell.test.ts` "every plugin permission is scoped, never a bare grant": add it as a named exception with the reason (the user chooses the path in a native dialog; there is nothing to scope; the window only receives a path string and hands it to the engine). Extend the description with one clause: the window may show a save dialog so the user can choose where a copy goes; the engine writes the file.
- ADR `docs/adr/2026-09-21-doors-not-commands.md`: a dated amendment after the 2026-09-23 piece D paragraph: the owner decided (2026-09-23) that the ENGINE opens Apple Books, Amazon's Send to Kindle app and page, Mail and (with piece C) the file manager, because those open a file in the library, piece C makes the library movable, and a window permission is a fixed path that cannot follow it. The window's doors for piece B are one: `dialog:allow-save`. The test for every door stays the same. No em dashes.
- `desktop/README.md`: one quoted dated note beside the others about the new grant.

- [ ] Commit "Window: the route builders, a save dialog door, and why the engine opens the rest".

---

### Task W2: send.js decisions for routes (above the line)

Pure exported functions, tested directly in `tests/desktop-ui.test.ts` (the Send surface describe blocks show the style):

- `routesFrom(answer)`: `{ routes, chosen }` or `null`. Validates the engine's `routes --json` shape strictly (the kfx.js `checklistFrom` is the model): `ok === true`; `routes` an array; each route has string `id`, `key`, `title`, `detail`, `button`, boolean `available`, `unavailable` absent when available and one of `connect|platform|setup` when not; `device` optional with string `id`, `kind`, `name` and string-or-null `volume`; `chosen` is the id of a listed route. Returns CLEAN copies. Malformed → null (the page then shows the engine's failure line, not an empty list).
- `isDeviceRoute(route)`: `key` starts with `device:` or equals `remarkable` (both go through export then send).
- `buttonClassFor(route, chosenId)`: `'btn btn-brad'` for the chosen AVAILABLE route, `'btn btn-outline'` for other available routes, `null` for unavailable (no button).
- `saveNameFor(epubPath, extension)`: the EPUB's stem plus `.<extension>` (stem from the file name, never the folder), used as the dialog's default name.
- `saveFiltersFor(extension, label)`: `[{ name: label, extensions: [extension] }]`.
- `routeNoteFrom(answer)`: the engine's `note`, trimmed, or a stand-in (`'Done.'` is not honest; use the failure path when `ok !== true`).
- `statusFor` gains phases for routes if needed (`'opening'`, `'saving'`, `'building-kindle'`) with plain words, e.g. `Opening Apple Books…`, `Saving…`, `Building the Kindle file (Kindle Previewer can take about twenty seconds)…`. Keep `statusFor`'s existing phases and tests.
- Unavailable detail wording comes from the engine and is shown as is.

Tests: every function, malformed-answer matrix for `routesFrom`, and a round trip against the ENGINE's real output: import `routes` and `preselected` from `src/export/routes.ts`, build answers for a matrix of facts, and `routesFrom({ ok: true, routes, chosen: preselected(...).id })` must deep-equal what went in (the Task 5 matrix test in piece D is the precedent).

- [ ] Commit "Send page decisions: the engine's route list, taken whole or not at all".

---

### Task W3: The Send page draws and performs routes

**Behaviour** (read `desktop/ui/send.js` in full first, and the piece D block `desktop/ui/kfx.js` for how a sub-block is mounted):

- Poll `argv.routes(script.epubPath)` every 2 s instead of `argv.devices()`, with the same "rebuild only when something changed" rule (compare the list of `id`+`available`+`detail` and `chosen`), the same `sending` guard, the same `hide()` that stops the poll. `ctx.state.devices` keeps being set, from the device rows' `device` fields, because other code reads it (`kfx.js` relevance uses the hook send.js passes: keep that working from the routes answer).
- Draw rows in the engine's order. Available rows: title, detail (device rows keep their volume line and unproven caveat exactly as today), and a button with `buttonClassFor`. Unavailable rows: dimmed (a new `.route-unavailable` rule in `surfaces.css` using existing tokens, e.g. `color: var(--ink-muted)`), title and the engine's detail, no button.
- Clicking:
  - device or reMarkable route → the existing `sendTo(device)` flow, unchanged (export, then send, with its phases, staleness checks and the KFX-install guard). It is found by the route's `device`.
  - `apple-books`, `send-to-kindle`, `email-to-kindle` → `runEngine(argv.route(key, epub))`, status `opening`, then the engine's note or its refusal.
  - `save-epub` → `saveDialog({ defaultPath: saveNameFor(epub, 'epub'), filters: saveFiltersFor('epub', 'EPUB') })`; cancel changes nothing and says nothing; else `runEngine(argv.route('save-epub', epub, { out }))`.
  - `save-kindle` → status `building-kindle`; `runEngine(argv.export(epub, { forFormat: 'kindle', fountain, optionsJson }))` to learn `extension` (refusal → its sentence); then `saveDialog` with `saveNameFor(epub, extension)` and a filter named by the answer's `label`; then `runEngine(argv.route('save-kindle', epub, { out, fountain, optionsJson }))`.
  - Every route shares the ONE `sending` flag and the era/staleness discipline `sendTo` uses: no two routes at once, no line painted into a replaced script. Keep `sendTo`'s own shape tests (its await count and stale checks) true for `sendTo`; give the new performer its own shape tests of the same kind.
- After any success, repoll once so the remembered route moves to the top.
- The empty state keeps its "What Screepub can reach" fold (the honesty table) below the list; the "Nothing plugged in" paragraph is replaced by the dimmed device rows, which say the same thing per device.
- The KFX block (piece D) is unchanged and still mounted after the status lines.

**Tests:** shape tests for the poll swap (`argv.devices` no longer polled by send.js; `argv.routes` is), the performer's staleness checks and single `sending` guard, the save flows call `saveDialog` before `route` and `export` before `saveDialog` for the Kindle file, a cancelled dialog calls nothing further. Update existing send.js tests whose premise changed, keeping what they guard.

- [ ] Commit "Send page: every route in the engine's order, the remembered one first".

---

### Task W4: Docs

- `docs/parity-audit.md` route catalog table: all six routes done (window and engine), date and piece.
- `docs/superpowers/plans/2026-09-21-parity.md` piece B: a Done note naming what landed and the one permission.
- `README.md` "Which readers?" table and "Device commands": Apple Books, Send to Kindle, email and save are in the new app too (keep the section's voice; no em dashes).
- Full suite and tsc green; em-dash check over the branch diff.
- [ ] Commit "Docs: piece B is done".
