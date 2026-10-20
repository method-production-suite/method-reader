# Design: the gear (parity piece C)

Date: 2026-09-23 · Status: accepted. The owner answered both questions at
the end on 2026-09-23: "yes go for it" (the folder picker permission, and
the engine revealing files). Built in parallel with piece B.
Piece C of [the parity plan](../plans/2026-09-21-parity.md).
Evidence: [the parity audit](https://github.com/method-production-suite/method-reader/blob/7eb40e5/docs/parity-audit.md), "The gear is three
things". Shares its settings file with piece B:
[2026-09-23-send-routes-design.md](2026-09-23-send-routes-design.md).

## The problem

The Swift app had three app-wide settings the window lacks:

- **Output folder**, with Choose and Reset (`ScreepubApp.swift:157`). Today
  nobody can change where books land without setting `$SCREEPUB_LIBRARY`.
- **Save as app defaults** (`ReaderRail.swift:100`): promote one script's
  tuning to the starting point for new scripts.
- **Reset to Defaults** (`ScreepubApp.swift:221`): put that starting point
  back to Screepub's own.

The engine can already layer settings (`resolveFormatOptions(values,
base)`, used by `src/settings/sidecar.ts`, `src/cli-settings.ts` and
`src/cli.ts`), but every caller passes the shipped defaults as the base.
Nothing stores a layer of the user's own underneath the per-script sidecar.

## Decisions

1. **One settings file for the app, shared with piece B.**
   `src/settings/app.ts` (built first, on piece B's branch) holds
   `lastRoute` for B and, for C, `libraryPath` and `formatDefaults`. It
   lives in the platform's config folder, never in the library, because
   this piece makes the library movable.
2. **Where books land:** `$SCREEPUB_LIBRARY` (the test seam and escape
   hatch) beats the chosen folder, which beats the platform default
   (`~/Documents/Screepub`). Reset removes the choice. A chosen folder must
   be absolute and writable (created if missing); otherwise the choice is
   refused and nothing is stored.
3. **Moving the library moves nothing.** Books already converted stay
   where they are and keep working (every path the window holds is
   absolute). New conversions go to the new folder. Said on screen.
4. **App defaults are the base under every script.** Precedence, knob by
   knob, through the one merge that already exists: explicit flags > the
   script's sidecar > the app defaults > Screepub's shipped defaults. This
   applies to the CLI too: a conversion anywhere starts from the user's
   defaults, and `--help` says so.
5. **Stored as a full, resolved options object**, so a later release that
   adds a knob fills it from the shipped default rather than from an
   absent key, and a stored value the engine no longer accepts is clamped
   by `resolveFormatOptions` on read.

## The engine

- `src/library.ts` `libraryRoot()`: after the `SCREEPUB_LIBRARY` check,
  reads `readAppSettings().libraryPath` (an injectable settings path, so
  tests never read the real file). A stored value that is not an absolute
  path is ignored (falls through to the default).
- `src/settings/app-defaults.ts` (new): `appDefaultOptions(settingsPath?)`
  = `resolveFormatOptions(formatDefaults ?? {}, DEFAULT_FORMAT_OPTIONS)`.
  Every place that passes `DEFAULT_FORMAT_OPTIONS` as a script's BASE uses
  it instead: the conversion in `src/cli.ts` (sidecar fallback and the
  no-sidecar case), `src/cli-settings.ts` (the fallback under a sidecar),
  `src/cli-export.ts` (`readFormat` with no `--options-json`). Places that
  mean "Screepub's own defaults" (the presets, `format-defaults.json`
  pinning, the `defaults` field of the `settings` answer) keep the shipped
  object. The `settings` answer gains `appDefaults`.
- `screepub app-settings [--set <json>] [--json]`, new verb:
  - `--set` takes `{ "libraryPath": "/abs" | null, "formatDefaults": {...}
    | null }`, either key or both; any other key is refused (`usage`), so
    `lastRoute` is not writable from here. `null` removes the key (Reset).
  - A `libraryPath` that is relative, or cannot be created or written,
    is refused with `bad-settings` and a sentence; nothing is stored.
  - `formatDefaults` goes through `resolveFormatOptions` before it is
    stored (clamped, complete).
  - Answer: `{ ok, file, library: { path, chosen, platformDefault, fromEnv
    }, formatDefaults, shippedDefaults, customized }`, where `path` is the
    folder conversions will actually use, `chosen` the stored choice or
    null, `fromEnv` true when `$SCREEPUB_LIBRARY` is overriding both, and
    `customized` true when app defaults differ from the shipped ones.
- **The engine reveals files** (owner, 2026-09-23): `screepub reveal
  <path>` opens the file manager with the file selected (`open -R` on a Mac, `explorer /select,`
  on Windows, `xdg-open <folder>` elsewhere), so Show in Finder keeps
  working for a moved library.

## The window

- **Convert page, under the drop area:** one quiet line, `Books are saved
  in ~/Documents/Screepub` with `Change…` and, when a folder was chosen,
  `Reset`. Change opens a folder picker (question 1), then `app-settings
  --set {"libraryPath": ...}`; the line then says the new place and that
  books already converted stay where they are. When `$SCREEPUB_LIBRARY` is
  set, the line says so and offers no Change (the variable would win).
- **Settings page (per script), at the foot:** `Use these for new scripts`
  (stores this script's current settings as the app defaults) and, when
  the app defaults differ from Screepub's own, `Reset new scripts to
  Screepub's defaults`. One caption says which defaults a new script starts
  from.
- Show in Finder (the result screen) calls the engine's `reveal`, so it
  works wherever the library is. The window's reveal permission is then
  unused and is removed, with the ADR amendment piece B writes.

## Out of scope

- Moving existing books when the folder changes.
- Per-device defaults, or more than one set of app defaults.
- The Swift app's settings (UserDefaults): not read, not migrated. A user
  of both apps sets the folder once in each.

## The owner's questions, answered 2026-09-23 ("yes go for it")

1. **Choosing the folder needs the window to show a folder picker.** One
   new window permission, `dialog:allow-open`. OK?
2. **Show in Finder for a moved library.** The window's reveal permission
   is fixed to `~/Documents/Screepub`, so it cannot follow a folder the
   user picks. Recommended: the engine reveals the file instead (the same
   answer as piece B's question 2), and the window's reveal permission is
   retired once nothing uses it. OK?

## Decided during the build (2026-09-24)

**Pin the script's settings on its first library conversion.** A PDF
converted with `--library` and no settings sidecar of its own now saves
the app defaults it converted from as that script's own, the moment it
first lands in the library (before any one-off `--options` flag for that
single run is layered on top; a flag used once must never freeze into the
script's standing choice). Without
this, "Use these for new scripts" or "Reset new scripts to Screepub's
defaults" would reach BACKWARD into a library already on a reader's
device: the book on disk was built at one set of knobs, and the next
`screepub settings` call for it would silently start answering with
another. App defaults are the starting point for NEW scripts, not a live
wire into every old one. **Limit:** scripts already converted into the
library before this change have no saved settings of their own, so they
keep following the app defaults until they are tuned once (any knob moved
on their Settings page saves them from then on).

**Reveal on Windows opens the folder, not the file.** The design above
says `explorer /select,` picks the file itself; built, it does not.
`/select,` breaks on a folder or script name with a SPACE in it alone (the
spawn layer quotes the whole argv element, and explorer reads the quotes
as part of the path instead of as `/select,` followed by one) or a COMMA
in it alone (explorer reads the first comma it finds as the end of the
`/select,` token, wherever that actually falls). Both are ordinary in a
screenplay title. `reveal` opens the containing folder on Windows instead,
the same as it already does on Linux, which sidesteps the quoting problem
rather than escaping around it.
