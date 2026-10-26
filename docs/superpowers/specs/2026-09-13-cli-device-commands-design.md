# Design: CLI device commands (cross-platform piece B)

Date: 2026-09-13 · Status: accepted (autonomous — see Decisions log)
Program: [ADR 2026-09-12 — cross-platform rewrite](../../adr/2026-09-12-cross-platform-tauri.md)
Follows: [piece A — device logic port](2026-09-12-device-logic-port-design.md)
Target version: 0.6.0

## Goal

Give the engine a command-line surface for the device logic piece A ported.
Two commands: list what is plugged in, and send a book to it. This is the
contract the Tauri shell will consume — designing it as a CLI first means it
is testable, scriptable and useful on its own, before any Rust exists.

Piece A deliberately left its modules consumed by nothing. This piece wires
them up.

## Scope

**In:** a `devices` command and a `send` command, both with `--json`; the
device-facing error codes they need; and the first real consumer of
`src/device/` and `src/export/`.

**Out:** conversion changes of any kind (the existing default command is
untouched), the Mac app switching to shell out (it keeps its Swift copy until
piece F), Tauri, and anything under `app/`.

## Command shape

The CLI's current contract is `screepub <input> [options]` — one positional,
the file to convert. Adding verbs has to preserve that exactly, because every
existing invocation and the Mac app depend on it.

**Rule: the first positional is a subcommand only when it matches a known verb
AND no file of that name exists.** `screepub devices` lists devices;
`screepub ./devices` and `screepub devices.pdf` convert a file. A file named
exactly `devices` in the working directory wins over the verb — a path the
user can always disambiguate with `./devices`, where a stolen filename would
be silently unconvertible.

```
screepub devices [--json]
screepub send <file> [--device <id>] [--json]
```

### `devices`

Lists every recognised reader. Volume-mounted vendors come from
`mountedDevices()`; reMarkable never mounts, so it is probed over its USB web
interface and appears only when that answers.

Human output is one line per device. `--json`:

```json
{"ok":true,"devices":[{"id":"/run/media/you/KOBOeReader","kind":"kobo","name":"KOBOeReader","volume":"/run/media/you/KOBOeReader"},
                      {"id":"remarkable","kind":"remarkable","name":"reMarkable","volume":null}]}
```

`id` is `deviceId()` from piece A — the volume path, or the kind for
reMarkable. It is what `send --device` accepts.

An empty list is `{"ok":true,"devices":[]}` and exit 0. Nothing is plugged in;
that is an answer, not an error.

### `send`

Sends an existing file to a device. It does **not** convert — the file must
exist already, which keeps this command honest about what it does and leaves
the conversion pipeline untouched.

- `--device <id>` selects by the id `devices` reports. Omitted: if exactly one
  device is connected, use it; if several are, fail and list them rather than
  guess. Sending a book to the wrong reader is annoying to undo by hand.
- Volume-mounted devices go through `copyToDevice`, which puts each vendor's
  file where that vendor actually indexes it.
- reMarkable goes through `uploadToRemarkable`, which accepts only PDF and
  EPUB and lists the root folder immediately before posting.

`--json` on success:

```json
{"ok":true,"device":{"id":"...","kind":"kindle","name":"Kindle"},"destination":"/run/media/you/Kindle/documents/Script.epub"}
```

For reMarkable there is no destination path, so the field is omitted and
`"uploaded":true` takes its place.

## Error contract

`--json`'s existing rule holds without exception: **every exit in that mode is
one parseable JSON object on stdout.** New codes, added to `cli-errors.ts`
beside the conversion ones:

| Code | When |
|---|---|
| `no-devices` | `send` found nothing connected |
| `ambiguous-device` | several connected and no `--device`; the message lists the ids |
| `unknown-device` | the `--device` id matches nothing currently connected |
| `send-failed` | the copy or upload failed; carries the underlying message |
| `unsupported-file` | reMarkable rejected the extension |

`usage` covers a missing file argument, as it already does.

## Architecture

```
src/device/list.ts     listDevices() — mounted vendors + a reMarkable probe
src/cli-devices.ts     the two command handlers, returning result objects
src/cli.ts             verb dispatch; unchanged for the default path
src/cli-errors.ts      the new codes
```

`listDevices` is async because the reMarkable probe is a network call; the
mounted half stays synchronous underneath. Handlers return plain result
objects and never print, so tests assert on values rather than on stdout.

**The probe's timeout is a real cost.** A reMarkable that is not plugged in
costs the timeout on every `devices` call. The probe uses the short default
from piece A, and `devices` runs the probe concurrently with the mount scan
rather than after it, so the command's wall-clock is the probe's timeout
rather than the sum.

## Testing

- Device listing is tested through injected roots, exactly as piece A's
  `enumerateVolumes` is — no test reads real mounts.
- The reMarkable half is tested against a local `Bun.serve` stub, reusing the
  pattern piece A established; no test touches a real network.
- Verb dispatch is tested directly, including the filename-shadowing rule in
  both directions (a file named `devices` converts; the bare word lists).
- The `--json` contract is tested for every new error code: one object,
  stdout, parseable.
- CLI tests follow the existing `tests/cli.test.ts` conventions.

## Acceptance criteria

1. The whole existing suite still passes; `bunx tsc --noEmit` clean.
2. `screepub <file>` behaves identically to before, byte for byte.
3. No file under `app/` is modified.
4. Every new error path emits exactly one JSON object on stdout under `--json`.
5. `devices` and `send` work against injected/stubbed devices in tests, and
   `devices` runs for real on this machine (reporting an empty list is a pass).

## Risks

- **Verb dispatch is a compatibility surface.** The filename-shadowing rule is
  the mitigation; both directions are tested.
- **No hardware.** `send` cannot be exercised against a real reader here. The
  copy destinations are already pinned by piece A's tests; this piece adds no
  new device knowledge, only a way to invoke it.
- **The reMarkable probe adds latency to `devices`** on every machine without
  one. Concurrency bounds it to one timeout.
