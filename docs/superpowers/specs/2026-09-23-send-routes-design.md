# Design: every way a book leaves Screepub (parity piece B)

Date: 2026-09-23 · Status: accepted. The owner answered both questions at
the end on 2026-09-23: "yes go for it" (the save dialog permission, and the
engine opening Books, Amazon and Mail).
Piece B of [the parity plan](../plans/2026-09-21-parity.md).
Evidence: [the parity audit](https://github.com/method-production-suite/method-reader/blob/7eb40e5/docs/parity-audit.md), "The route catalog".
Contract: decision 25 of [the UI pass spec](2026-09-20-desktop-ui-pass-design.md).

## The problem

The Swift app offered six ways out for a finished book. The window offers
two (a reader over USB, a docked reMarkable). Missing: Apple Books (the
only route to an iPhone), Amazon's Send to Kindle, email to Kindle, and
save a copy. Also missing is the part the audit calls the easiest to
rebuild wrongly: the ranking, and the rule that **what you chose last time
beats any guess**, and the habit of keeping an unavailable route on screen
with its fix instead of hiding it.

## What was measured (2026-09-23, this Mac)

- Books is at `/System/Applications/Books.app`. Amazon's Send to Kindle
  app is not installed, so that route opens Amazon's web uploader here.
- The default `mailto:` handler is read in 10 ms from
  `defaults read com.apple.LaunchServices/com.apple.launchservices.secure
  LSHandlers`: the entry with `LSHandlerURLScheme = mailto` names
  `com.superhuman.electron` on the owner's Mac. No entry means Apple Mail.
  So on this Mac the email route is dimmed with its fix, exactly as the
  Swift app did.
- The Swift executors (`AppleBooks.swift`, `SendToKindle.swift`,
  `SaveFlow.swift`, `ResultActions.swift`) were read in full; the catalog,
  its order, its wording and `preselected` are ported, not reinvented.
- `desktop/ui/app.js` counts engine calls toward "busy" (the updater's
  restart waits for quiet) and skips the read-only `devices` and
  `kfx-status`. A polled route list must be skipped the same way.

## Decisions

1. **The engine owns the list, the order and the memory.** Decision 25's
   contract, adopted: a route list of `{ id, title, detail, button,
   available }` plus a perform call. The window draws and never ranks.
2. **App-wide settings get a home now, shared with piece C.** One engine
   file, `settings.json`, in the platform's config folder, NOT in the
   library (piece C makes the library movable, so its own location cannot
   live inside it). Piece B stores one thing in it: the last route.
3. **Three kinds of unavailable, not one.** `connect` (plug it in, dock
   it), `platform` (not made for this system), `setup` (change a setting,
   e.g. the default mail app). Each dimmed row carries its fix as its
   detail line.
4. **Save a copy is two rows, named by purpose.** "Save the EPUB" (for
   email, Apple Books and most readers) and "Save a Kindle file" (for
   copying to a Kindle by hand). A save dialog cannot carry the Swift
   panel's format picker, and naming the purpose on the row is the lesson
   of `screepub-email-export`: the file you email is not the file you
   sideload.
5. **The window never writes a file.** It raises a save dialog and hands
   the engine a path, as the UI pass spec agreed: `screepub export ...
   --out <path>`.
6. **The engine opens Books, Amazon's app and page, and Mail** (owner,
   2026-09-23), the same way it already runs Calibre. Those routes open a
   file in the library, piece C makes the library movable, and a window
   permission is a fixed path that cannot follow it. The ADR gets a dated
   amendment saying so. The window gains exactly one permission for this
   piece: `dialog:allow-save`.

## The engine

### `src/settings/app.ts` (new): the app-wide store

```ts
export interface AppSettings { lastRoute?: string }
export function appSettingsPath(platform?, env?): string;
export function readAppSettings(path?): AppSettings;      // never throws
export function writeAppSettings(patch: Partial<AppSettings>, path?): AppSettings;
```

- Folder: macOS `~/Library/Application Support/Screepub`, Windows
  `%APPDATA%\Screepub`, elsewhere `$XDG_CONFIG_HOME/screepub` (else
  `~/.config/screepub`). `SCREEPUB_CONFIG_DIR` overrides everywhere (the
  test seam, like `SCREEPUB_LIBRARY`).
- A missing, unreadable or malformed file reads as `{}`. Unknown keys are
  kept on write, so piece C's fields survive a piece B write and the
  reverse.
- Written atomically (temp file, rename), parents created.

### `src/export/routes.ts` (new, pure): the catalog

```ts
export type RouteKey = `device:${string}` | 'remarkable' | 'apple-books'
  | 'send-to-kindle' | 'email-to-kindle' | 'save-epub' | 'save-kindle';
export interface Route {
  id: string;          // row identity: a device adds its volume
  key: RouteKey;       // what is remembered: a device by KIND, never path
  title: string; detail: string; button: string;
  available: boolean;
  unavailable?: 'connect' | 'platform' | 'setup';
  device?: DeviceSummary;          // present on connected device rows
}
export interface RouteFacts {
  platform: string;
  devices: ConnectedDevice[];      // listDevices(), reMarkable included
  booksApp: boolean; sendToKindleApp: boolean; appleMailDefault: boolean;
}
export function routes(facts: RouteFacts): Route[];
export function preselected(list: Route[], lastRoute: string | undefined): Route;
```

Order, strongest claim first (Swift's, unchanged): connected devices; a
docked reMarkable; Apple Books; Send to Kindle; email; save EPUB; save
Kindle file. Then the unavailable rows: each device kind not connected
(`connect`, "plug in over USB to send"), reMarkable not docked (`connect`,
"dock over USB to send"), and the platform and setup rows below.

Platform rules:
- Apple Books: macOS only; `platform` elsewhere ("on a Mac only"). On a
  Mac with no Books.app it is hidden, as the Swift app hid it (structural
  absence on the one platform that has it is not a fix a person can make).
- Send to Kindle: everywhere. Titled "Send to Kindle app" when Amazon's app
  is installed (macOS), else "Send to Kindle web".
- Email to Kindle: macOS with Apple Mail as the default mail app. On a Mac
  without it, `setup`: "needs Apple Mail as the default mail app, the one
  mail app the attachment survives". Elsewhere, `platform`: "on a Mac
  only; save the EPUB and attach it yourself".
- tolino on Windows: `platform` ("cannot be found on Windows: a Windows
  drive carries no volume name"), not `connect`: plugging it in would not
  help, and send.js already says so in prose.

`preselected`: the remembered route wins whenever it is listed, even
unavailable (an unplugged Kindle stays chosen; its button waits); else the
first available row.

### Probes: `src/export/route-facts.ts` (new)

`routeFacts()` gathers `RouteFacts`: `listDevices()` (existing), Books at
`/System/Applications/Books.app` or `/Applications/Books.app`, Amazon's app
at `/Applications/Send to Kindle.app` or `~/Applications/Send to
Kindle.app`, and the mailto handler from the `defaults` read above (macOS
only). When `defaults` answers that LSHandlers "does not exist", nobody has
changed a default app: no entry, so Apple Mail. Any other failure reads as
"not Apple Mail", which dims the row rather than offering a compose that
loses the file. Each probe is an injectable seam, and the mail probe's
spawn is one too.

### Verbs

- `screepub routes <file.epub> [--json]`: `{ ok, routes: Route[], chosen:
  id }`. Read-only; the Send page polls it in place of `devices`.
- `screepub route <key> <file.epub> [--out <path>] [--fountain f]
  [--options-json j] [--json]`: performs one non-device route and, on
  success, remembers its key. Answers `{ ok, key, path?, note }`, where
  `note` is the sentence the window shows ("Added to Apple Books. It syncs
  to your iPhone and iPad when Books uses iCloud."). Device routes keep
  their existing `export` then `send` flow, which already narrates its own
  phases; `send` remembers `device:<kind>` when it succeeds.
- `screepub route kindle-email-setup [--json]`: takes no file, opens
  Amazon's Personal Document Settings page (the email route's one-time
  setup), is never remembered, and is available on every platform.
- `screepub export ... --out <absolute path>`: copies the fresh artifact to
  the path (parents created, written atomically, an existing file replaced:
  the dialog already asked). The path's extension must match the
  artifact's (`.epub`, or the Kindle rung's `.kfx`/`.azw3`/`.mobi`), else a
  usage error naming the right one. The answer's `path` is the written
  path. A relative `--out` is refused.

Saves go through `route save-epub|save-kindle --out`, which calls the same
export code and then remembers the key.

## The window

The Send page becomes the list of every route, in the engine's order:

```
Send it
[ Kindle            /Volumes/Kindle            ] [COPY TO KINDLE]   (brass: chosen)
[ Apple Books       syncs to your iPhone and iPad] [ADD TO APPLE BOOKS]
[ Send to Kindle web  via Amazon ...            ] [SEND TO KINDLE WEB]
[ Save the EPUB     for email, Apple Books ...  ] [SAVE THE EPUB…]
[ Save a Kindle file  AZW3, to copy by hand      ] [SAVE A KINDLE FILE…]
  Kobo              plug in over USB to send                   (dimmed)
  Send to Kindle email  needs Apple Mail as ...                (dimmed)
<status line>
Best Kindle quality ...  (piece D, unchanged)
```

- One row per route, each with its own button (the Send page already
  works row by row; a picker plus one button would be a second idiom).
  Rows stay in the engine's order, which never moves: the chosen route is
  marked by the brass button, not by jumping to the top, because a list
  that reshuffles after every send is a list you have to reread (decided
  while building, 2026-09-23). Every other available route has an outline
  button; unavailable rows are dimmed, have no button, and show their fix.
- Device rows keep what they have today: the volume line, the unproven
  caveat, the export-then-send flow and its phases.
- Save rows: "Save the EPUB…" opens the save dialog at `<stem>.epub`, then
  calls `route save-epub --out`. "Save a Kindle file…" first runs `export
  --for kindle` (builds or reuses the library's Kindle file, which may take
  Kindle Previewer's ~20 s, said on the status line) to learn the
  extension, then opens the dialog at `<stem>.<ext>`, then `route
  save-kindle --out`. A cancelled dialog changes nothing.
- The "what Screepub can reach" table stays, folded, below the list.
- `routes` replaces the `devices` poll and joins `devices` and
  `kfx-status` in app.js's list of calls that do not count as busy.

## Out of scope

- Piece C's store fields (library path, app defaults). The file and module
  are shared; C adds its keys.
- Moving Show in Finder into the engine. It will need to when piece C moves
  the library (the window's reveal grant is a fixed path); that is C's.
- Amazon's Send to Kindle for PC. Windows gets the web route.

## The owner's questions, answered 2026-09-23 ("yes go for it")

1. **Save a copy needs the window to show a Save box.** That is one new
   window permission, `dialog:allow-save` (the ADR already names it as a
   door). OK to add?
2. **Who opens Books, Amazon and Mail.** The ADR planned window
   permissions. Recommended instead: the engine opens them, the same way it
   already runs Calibre (`open -a Books <file>` on a Mac). Reason: those
   routes open a file in the library, piece C makes the library movable,
   and a window permission is a fixed path that cannot follow it. The
   engine can. That means no further window permissions for piece B, and
   an ADR amendment saying why. OK?
