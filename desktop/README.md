# Screepub desktop window

A Tauri window around the Screepub engine. **No logic lives in this
directory.** The window runs the engine binary with the arguments the
frontend built and draws the JSON object the engine prints. Parsing, device
knowledge, formatting rules, settings and send routes are all in `src/`
(TypeScript), where the CLI and the window share one implementation and one
test suite. See [ADR 2026-09-12](../docs/adr/2026-09-12-cross-platform-tauri.md).

It ships for macOS (one `.dmg` for Macs with Apple silicon,
`Screepub-macOS.dmg`), Windows (an NSIS installer) and Linux (`.deb` and
`.rpm`), built by `release.yml` on
every tag. On a Mac it is `Screepub.app` with the identifier
`com.darkwell.screepub`, the identity the older Swift Mac app had, so that
app's own updater installs the window in its place. Who has checked which
of these, and how far: [the verification ledger](../docs/verification-ledger.md). Dated measurements
and transcripts from building it: [desktop build notes](../docs/history/desktop-build-notes.md).

## Build and run

You need Bun and a Rust toolchain (`rustup`, `cargo`).

    bun tools/build-sidecar.ts --host    # compiles the engine for THIS machine
    cd desktop/src-tauri && cargo run

`cargo run` is the whole dev loop. The frontend is static files that
`tauri-build` embeds at compile time: no dev server, no bundler and no npm
dependency. After a change to `desktop/ui/` alone, `cargo run` can serve the
previously embedded frontend; touch a Rust file or `tauri.conf.json` if a
change does not show.

The sidecar must exist before `cargo build` even compiles: the build script
checks for it (see "How Tauri finds the engine" below).

Installable bundles are built by `bun tools/build-app-bundle.ts`, which calls
`cargo tauri build` for you (install the Tauri CLI with
`cargo install tauri-cli --version 2.11.4 --locked`, the version CI pins). A
**macOS** build, which is what a release ships, is for Apple silicon only:

    bun tools/build-sidecar.ts --target bun-darwin-arm64
    cd desktop/src-tauri
    cargo tauri build --target aarch64-apple-darwin --bundles app,dmg

The engine is named for the target rather than for the machine, so this
works the same on any Mac. An Intel Mac can still build it (not run it),
after `rustup target add aarch64-apple-darwin` once: rustup installs only
the machine's own target by default. `build-app-bundle.ts` builds every Mac bundle
for that target and refuses any other: Intel Macs are not supported after
the identifier release, the last whose image was universal (two engines
fused into one), and an Intel or universal image would be published under
the one Mac name. See
[the plan](../docs/superpowers/plans/2026-10-01-f2-handover-amendment.md).

`cargo tauri build` may rewrite `Cargo.toml`'s `features = []` lists; they
are committed in the spelling the CLI writes, so a rewrite is a no-op. If it
is not, `desktop.yml` fails on the diff.

## How Tauri finds the engine

Two names, and they differ:

- **On disk in `desktop/src-tauri/binaries/`** the engine carries the target
  triple: `screepub-engine-<triple>`, for example
  `screepub-engine-aarch64-apple-darwin`. `tauri.conf.json`'s
  `"externalBin": ["binaries/screepub-engine"]` is a prefix, and the build
  script fails, naming the exact file it wanted, when that file is missing.
  `tools/build-sidecar.ts` writes the right name.
- **At run time** the app looks beside its own executable for the name
  without the triple, `screepub-engine`, which the build copies there.

`sidecar()` does not fail on a wrong name; the spawn does, with a bare
`No such file or directory (os error 2)` that names nothing.
`src-tauri/src/sidecar.rs` wraps it, so the window says "could not start the
Screepub engine" and how to build it.

## What the Rust side does

Exactly two commands: `run_engine` (run the engine with an argument list and
return its stdout as a string) and `pick_file` (the native open dialog for a
PDF). While the engine runs, every stderr line goes to the window as an
`engine-line` event, verbatim; the window reads `--progress` lines from it.
The progress percent is already the whole job's, so the window shows it as
given.

Everything else the window does goes through a **door**, a scoped plugin
permission in `capabilities/default.json`, never a new command (see
[ADR 2026-09-21](../docs/adr/2026-09-21-doors-not-commands.md)):

| Permission | What for |
| --- | --- |
| `core:default` | events, including the drag-and-drop paths on `tauri://drag-drop` |
| `opener:allow-open-url` | exact URLs only: the issue tracker, Calibre's macOS and Windows download pages, Amazon's Kindle Previewer page |
| `updater:default` | check, download and install; its one endpoint is `latest.json` in `tauri.conf.json` |
| `process:allow-restart` | restart after an update, once the engine is idle (not `exit`) |
| `core:window:allow-start-dragging` | move the window by its top strip and the gaps in the tab bar |
| `dialog:allow-open` | the folder picker for where books are saved |
| `dialog:allow-save` | the save box for where a copy goes |

Apple Books, Send to Kindle, email, Amazon's settings page and showing a file
in the file manager are opened by the **engine** (`screepub route`,
`screepub reveal`), so they follow the library wherever it is.

The engine's single `--json` answer is written through `sayLine` in
`src/cli.ts`, which waits for the pipe to drain; answers of several megabytes
arrive whole. A `String` return is what ships: it measured faster than bytes.

There is no Cancel during a conversion. Killing a running engine needs a kill
handle the window can reach, which is a third command, and the owner decided
against it.

## The updater overlay

**`tauri.updater.conf.json`** holds exactly
`bundle.createUpdaterArtifacts: true`, passed by
`bun tools/build-app-bundle.ts --updater`, which only release.yml's macOS leg
uses. It is not in `tauri.conf.json` because tauri-cli then signs the
updater archive itself and fails the whole bundle when
`TAURI_SIGNING_PRIVATE_KEY` is unset, which it is on every push build and on
your machine. It is the only overlay: every platform builds under the name
in `tauri.conf.json`.

`serde_json` is in `Cargo.toml` only because `tauri::generate_context!` embeds
the updater's config as `serde_json` values. No `.rs` file may name it;
`tests/desktop-shell.test.ts` checks.

## The interface

`desktop/ui/` is the app: 27 files and six bundled font files, no build
step. `index.html` loads three stylesheets and one ES module, `main.js`. The
window's content security policy is `default-src 'self'; img-src 'self'
data:` plus one `connect-src` for the device-request service: the window's
own files, pictures written into the page itself, that one address, and
nothing else (`tests/desktop-shell.test.ts` pins it). The tabs are
Convert, Read and Settings: sending happens on Convert's result, which
Read and Settings return to with their Send button. The release notes open
from the version stamp at the foot of the page. New windows open at 1280
by 800.

| File | What it owns |
| --- | --- |
| `app.js` | **the only file that talks to Rust**, and the only one that knows an engine flag: every argument list is in `argv` |
| `main.js` | boot, the shared state, switching pages, the Ctrl/Cmd-O shortcut, where focus goes after a dialog |
| `frame.js` | the sheet, the punched holes, the brads, the tab bar |
| `dom.js` | `el`, `clear`, `text`; no `innerHTML` anywhere |
| `focus.js` | where the keyboard lands on each page |
| `convert.js` `read.js` `tune.js` `notes-surface.js` | one page each (`tune.js` is the Settings page) |
| `result-page.js` | the left half of Convert's result: the book's first page |
| `page-show.js` | the page that writes itself while a script converts: the typed page in the result's box, the wave, the slim reading line |
| `page-timing.js` | the show's clock: which character is typed when, which blocks have formed, when it ends, when a key sounds |
| `typewriter.js` | the key and bell sounds, made with Web Audio |
| `send-panel.js` | the right half of Convert's result: the device cards, the one brass button, Other ways, Eject, the Kindle file built ahead |
| `device.js` | the panel's rules: which cards, which button is brass, which asterisk, which file rows, what Eject and the heads-up say |
| `send-routes.js` | what sending decides without drawing: the engine's route list and Kindle file answers read, the status line's words, the Save box's defaults |
| `requests.js` | the request service: "My device isn't here" and the asterisk's It worked / It did not work, posted to the Worker in `services/device-requests/`; the window's one `fetch`, and the one origin the CSP's `connect-src` adds |
| `device-watch.js` | the two-second `devices --quick` poll, running whenever a script is open |
| `heads-up.js` | "Kindle plugged in. Copy <Title>?" at the top of whatever page is showing |
| `kfx.js` | the send panel's KFX checklist |
| `book-queue.js` | one queue per book, so no engine call reads a book while another rewrites it |
| `feedback.js` | the pre-filled bug report URL |
| `update.js` `update-flow.js` | the update question, the once-a-day check, download, install and restart |
| `update-compare.js` `update-compare.d.ts` | **generated** from `src/update/compare.ts` by `tools/build-update-compare.ts` |
| `notes.js` | **generated** from `docs/releases/<version>.md` by `tools/build-desktop-notes.ts` |
| `tokens.css` | **generated** from `brand/tokens.json` by `tools/build-desktop-tokens.ts` |
| `style.css` `surfaces.css` | the core colours (pinned by `tests/desktop-shell.test.ts`), the fonts, the frame, the pages |
| `index.html` | the page that loads the rest |

Regenerate the generated files after touching their sources; `desktop.yml`
diffs them and fails if you didn't.

**Settings.** Each script's settings live in its own sidecar file in the
library (`src/settings/sidecar.ts`). App defaults, the library folder and the
last route used live in the engine's app settings file
(`src/settings/app.ts`); the Settings page's **Use these for new scripts**
writes them through `screepub app-settings`. The window keeps only its update
answers in its own web storage.

**The reader** is an `<iframe srcdoc>` with a constructed stylesheet. The CSP
blocks every inline style, srcdoc frames inherit it, and constructed
stylesheets are not blocked, so `read.js` lifts the engine's `<style>` out
and adopts the same text. The pictures need nothing from `read.js`: the
engine writes each one into its `<img>` as a `data:` URI, because the
document arrives as text with no folder beside it, and `img-src` admits
`data:` for exactly that. Without it, the inherited policy draws every
figure as a broken image. The sheet must be built in the frame's own realm;
the frame is `sandbox="allow-same-origin"` without scripts; `read.js`
supplies the `@font-face` rules. A focused iframe paints no focus ring, so
the frame is not a Tab stop: the stage around it is, and `read.js`'s
`scrollStep` scrolls the frame from the keyboard.

**Fonts.** Courier Prime and Literata ship as the same six WOFF2 subsets the
website uses. To check one loaded, `await document.fonts.load(...)` and read
the `FontFace` status: `document.fonts.check()` can return `true` for a font
that is not there.

**Linux launcher entry.** `Categories=Office;` only: `bundle.category` is a
fixed list, and adding `Publishing;` needs a custom desktop template that
takes over `Exec=`, `Icon=` and the rest. There is no PDF file association:
the window does not read a path from its command line, so "Open with" would
open an empty window.

## Known gaps

- A file dropped while a conversion is running is ignored without a word
  (`convertPath` returns while `busy`).
- No Cancel during a conversion (above).
