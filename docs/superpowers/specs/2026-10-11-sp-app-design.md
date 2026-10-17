# sp-app: the window in Rust, and the toolkit it is drawn with

> **Superseded in part (2026-10-16):** the window's look is now the Method
> family's, not the Screepub paper this spec describes: the system's own UI
> sans for all chrome (Inter bundled as the fallback), sentence case, a
> normal title bar, a tab strip, Method Budget's control proportions, and no
> brads or wordmark. docs/BRAND.md ("The window") and brand/README.md
> describe it; Courier Prime and Literata set only the script. The toolkit
> decision and everything else below still hold.

Date: 2026-10-11. Plan: `docs/superpowers/plans/2026-10-11-pure-rust-port.md`
(the UI spike). Inventory: `rust/PARITY.md`, "Window feature inventory".

## Decision

**egui 0.36.2 with eframe 0.36.2**, default features (AccessKit and wgpu
on), plus `rfd` for the native dialogs, `wuff` to decode the bundled WOFF2
fonts, and `mundy` for Reduce Motion and the OS colour scheme.

## What was checked, and how

Every claim below was read off the crate sources downloaded from
static.crates.io on 2026-10-11 (the versions named), or the project's
default branch where a release was not enough, not from memory.

| | iced | egui / eframe |
|---|---|---|
| Version | 0.14.0 (crates.io); master is 0.15.0-dev | 0.36.2 |
| Screen reader | **None.** 0.14.0's Cargo.toml and `iced_winit`, `iced_widget`, `iced_core` sources contain no AccessKit at all. Master's workspace Cargo.toml (fetched live) has no `accesskit` dependency either. The accessibility work lives in forks (System76's COSMIC fork; plushie-iced), not upstream, merged nor released. | **Shipped, on by default.** eframe's `default` features include `accesskit` (`egui-winit/accesskit` → `accesskit_winit` 0.34.1, which pulls `accesskit_macos` 0.27, `accesskit_windows` 0.35, `accesskit_unix` 0.24 per target). |
| Custom painting | Canvas widget | `Painter`: shapes, meshes with per-vertex colour (radial gradients), clipping |
| Text | cosmic-text | epaint with harfrust (a Rust HarfBuzz port) shaping, skrifa outlines, GPOS kerning (epaint 0.36.2 `text_layout.rs` tests it) |
| Fonts | TTF/OTF | TTF/OTF; WOFF2 decoded by `wuff` 0.2.9 (pure Rust, MIT) at start |
| Drag and drop | winit file events | `RawInput::hovered_files` / `dropped_files` |
| Dialogs | rfd | rfd 0.17.2 |
| Light/dark | own detection | `Context::system_theme`, plus `mundy` |
| IME | yes | yes (egui-winit `Ime` events into `TextEdit`) |
| Multiple windows | yes | viewports (not needed: one window, one modal sheet) |
| Headless UI tests | `iced_test` | **`egui_kittest` 0.36.2**: drives the real UI with no window and queries it through the AccessKit tree by role and name |
| Platforms | Mac, Windows, Linux | Mac, Windows, Linux |
| License | MIT | MIT OR Apache-2.0 |
| Maintenance | active | active (monthly releases) |

iced is out on the plan's hard requirement alone. Others looked at, all from
their 2026-10-11 crates: **Xilem/Masonry 0.4.0** has AccessKit and is the
credible runner-up, but is pre-1.0 with a thin widget set; **vizia 0.4.0**
has AccessKit but draws with `skia-safe` (C++); **floem 0.2.0** and
**gpui 0.2.2** have no AccessKit in their manifests; **Slint** is excluded
by the plan; **freya** is a release candidate on Skia.

### The proof

`rust/crates/sp-app/examples/a11y_proof.rs`: one window, a brass button
"CHOOSE PDF…", a text field, a painted brad. Run it and read the tree from
OUTSIDE the process, as VoiceOver does, with
`rust/crates/sp-app/tools/ax-tree.applescript` (System Events):

```
AXGroup | group | missing value
AXButton | CHOOSE PDF… | missing value
AXStaticText | text | Pressed 0 times
AXStaticText | text | Which device?
AXTextField | text field |
AXUnknown | A brass brad, painted | missing value
```

Then `perform action "AXPress"` on that button through System Events moved
the counter to "Pressed 1 times": assistive technology can both read and
operate the window. Two facts learnt doing it: AccessKit on macOS starts
only when something asks, so the first query sees only the title bar and
the second sees the content (no fix needed; VoiceOver asks continuously);
and a field's name must be tied on with `labelled_by`, which sp-app does.

Windows (UI Automation) and Linux (AT-SPI) use the same AccessKit tree
through their own adapters; they were not run here (no machine), and are on
the verification list for wave 3.

## How sp-app is built

- **Decisions are pure and toolkit-free** (`src/model/`), ported from the
  exported functions above the line in each `desktop/ui/*.js`, with their
  tests ported beside them. This is the same split the window already has.
- **Screens are egui** (`src/screens/`), and hold no rule of their own.
- **The engine is a trait** (`src/engine.rs`), one method per `argv` builder
  in `desktop/ui/app.js` (inventory rows E1 to E19). Answers keep the CLI's
  `--json` shape as `serde_json::Value`, so the window's validators port
  unchanged and refuse what they refuse today. `FakeEngine` answers from
  fixtures for every state, so every screen is testable before `sp-engine`
  exists. Calls run on worker threads; the UI thread never blocks.
- **Tests run headless** through `egui_kittest`: each test drives a screen
  with the fake engine and asserts what it shows by the AccessKit tree
  (role and name), which tests the screen reader's view at the same time.

## The book preview: Blitz (spike, 2026-10-11)

The window's preview promise is "reflows the way it will on your reader",
so it has to be the engine's OWN XHTML and css.ts, not a second renderer.
The first pass drew structured blocks; the coordinator asked for a spike of
Blitz, Dioxus's HTML/CSS engine, and it is the answer.

**Pure Rust.** blitz-dom, blitz-html and blitz-paint (Stylo for CSS, Taffy
for layout, Parley for text, vello_cpu through anyrender to paint pixels):
`cargo tree` for macOS and Windows has no `-sys` crate, no `cc`, no Skia
(`tiny-skia-path` is a Rust port of Skia's path code, not Skia). On Linux,
Parley's font discovery (fontique) links the system's libfontconfig through
`yeslogic-fontconfig-sys`; the window registers its own six faces, so the
system-fonts feature can be turned off there (to do when wiring). Stylo is
MPL-2.0, which is compatible with the AGPL. The network crate
(`blitz-net`, reqwest with native-tls) is NOT used: a 20-line provider in
the window answers `data:` URLs (the engine's pictures) and refuses
everything else, so the preview fetches nothing.

**What was rendered.** The committed oracle previews
(`rust/oracle/{field-station,torture,images}/book.preview.html`, the
engine's own output) at 706 wide, with read.js's device layer (ink, paper,
1.6em/7% margins) and the bundled fonts registered:

| css.ts feature | Result |
|---|---|
| Percentage margins, the dialogue column, centred cues | right: cues centred, dialogue in its 20% column |
| em spacing between elements | right |
| Dual dialogue side by side (`table.dual-dialogue`) | right: two columns, each cue over its speech |
| Font shifts (`fmt-sans`, sizes), bold, underline | right |
| Italic | no synthetic oblique: the window bundles no Courier Prime italic, and Blitz does not slant the regular face (browsers do) |
| Figures (`data:` PNG) | right, with the data-only provider |
| Courier Prime from the bundled faces | right (rendered differently with and without them) |
| Light and dark | right (the device layer's colours) |
| Page markers (`span.page-marker { float: right }`) | **broken**: Blitz 0.3.0-beta.2 (crates.io) hangs forever in Parley's line breaker on any float inside inline text; Blitz main (git 45496d9) no longer hangs but draws the float at the start of the line, over the heading's first letter |
| Speed | the 18-page demo: layout 10 ms, painting the whole 12,000px page 17 ms on the CPU |

**Decision.** Use Blitz for Read, the Settings preview, the result's first
page and the page show's formed blocks, pinned to Blitz main (git rev
45496d9) until a crates.io release carries the float fixes, with ONE
window-only shim in the device layer, beside read.js's colours and margins:

    h2.scene-heading, p { position: relative; }
    span.page-marker { float: none; position: absolute; right: 0; top: 0; }

It draws the marker where a browser draws the float (the line's top, at
the right edge, small and dimmed), in the window only; the EPUB keeps
css.ts untouched. It comes out when Blitz positions inline floats. Text is
exposed to the screen reader by the window: one AccessKit node per
heading and paragraph, named by its words, at its laid-out rect (Blitz's
own accessibility tree is not bridged). Read pages the book by painting
only the visible slice of the laid-out document.

Owner-visible: a git dependency on an unreleased engine, and the shim.

## The rows that were still to build (2026-10-11)

- **The Mac menu bar** is muda 0.21 (Tauri's own menu crate): the app
  menu, Edit and Window as Tauri's default gave them. macOS hands Edit's
  shortcuts to the menu, so each Edit item is passed on to egui as the
  keystroke or clipboard event it stands for (app/menu.rs).
- **The typewriter** is cpal 0.18 playing model/typewriter.rs's samples
  (app/sound.rs). The output opens at the first sound; a computer that
  cannot open one is quiet and is not asked again. typewriter.js has a
  close() nothing calls, so both keep the output for the window's life.
- **The keyboard's return after a flow** (send-panel.js keepFocus): panel
  buttons carry ids made from their data-key, so a button rebuilt by a
  redraw is the same widget, and model/focus.rs decides where the
  keyboard goes (the same key, else the brass, else the page's plan) and
  when the brass is owed it.
- **The update flow** is model/update_flow.rs, a state machine with the
  run left as calls (`start`, `during`, `finished`, `threw`) for the wave
  3 transport to drive. The label and the notes read its one phase.
- **No em dash**: tests/no_em_dash.rs scans sp-app's sources with comments
  taken out, as the TypeScript scan did with Bun's transpiler.

## Wave 3: the real engine and the updater (2026-10-11)

- **The engine is in-process.** sp-cli gained a library `run(argv, sink)`;
  the binary is that function on the process's streams, and the window's
  `DirectEngine` (src/direct.rs) is that function on each `Request::argv`,
  on a thread of its own per call. Every request takes exactly the CLI
  verb's path, so the window and the CLI cannot disagree. stderr lines
  that are `{"progress": ...}` reach the window as they are written (the
  stage, the page reached, the first page for the page show); the last
  stdout line is the answer, and anything else is E21's "did not answer
  in JSON". A panic is one failed call.
- **Quitting mid-call (W13).** The sidecar outlived the Tauri window. Here
  the process waits, after the window has gone, for every call still
  running (at most ten minutes), so atomic writes complete.
- **Install.** The label and the notes' button run sp-update's
  `install_and_restart` on a thread. The window's busy count (calls and
  holds) is mirrored into sp-update's `EngineGate`, which the run retires
  and waits on: 500 ms of quiet, at most 120 s, then "Restart now". Once
  retired, every engine call is refused with the window's sentence. A
  restart that starts closes the window.
- **Headless.** `screepub-window --version --json` and `--smoke <pdf>
  [--out <epub>]` print what `screepub` prints for the same command, for
  xtask smoke-bundle.
- **SCREEPUB_CONFIG_DIR** also moves the window's own storage file, and a
  run with it set reads nothing of the Tauri window's WebKit storage.
- **Unaligned overlay off.** The spacing is the stylesheet's rem scale
  (6.4, 12.8 points), which falls between pixels as it does in a browser;
  egui's debug-build overlay is switched off rather than rounding the
  layout away from the CSS.

## What cannot reach 1:1, precisely

1. **The page marker** is drawn by the shim above, not by the float rule
   (until Blitz places floats). **Italic** has no synthetic slant.
2. **Linux** links libfontconfig (C) unless fontique's system fonts are
   switched off, and the typewriter's sound links ALSA's libasound (C):
   cpal 0.18 reaches Core Audio on a Mac and WASAPI on Windows through
   the OS's own APIs with no C library of ours, but its Linux backend is
   the `alsa` crate over `alsa-sys`. A Linux machine without libasound
   cannot start the window at all, where the webview's Web Audio needed
   nothing; Linux packages list it as a dependency (every desktop has it).
3. **An answer that is not JSON** (E21) cannot happen with direct calls;
   the trait's `Err(String)` is shown in its place.
4. **No sidecar** (W12): the "could not find the engine" sentence has no
   cause left; a call that cannot run at all answers `Err` through the
   trait and is shown as a refusal with code `internal`, as before.
5. **Reduce Motion on Linux** is read from GNOME's setting only (mundy's
   `_gnome_only`); KDE and others read as "motion allowed".
6. **Hover is egui's model.** Focus rings are no longer a gap
   (2026-10-12): egui keeps one focus for every cause, so sp-app keeps
   `:focus-visible`'s rule beside it. Each frame, before drawing, the
   window reads its input (`widgets::note_ring_input`, deciding with
   model/focus.rs `rings_after`): a key pressed without Command, Control
   or Alt (Tab, the arrows, Space, Enter, typing) turns rings on; a
   pointer press or a touch turns them off; a frame with neither (a focus
   the window moves itself with restore_focus, after a dialog, on a
   surface change, a redraw, or a screen reader's focus request) keeps
   the last mode. Before any input there are no rings. Every ring (the
   2px ink ring of `widgets::focus_ring`, and the reader's stage) is drawn
   only when the control has egui's focus AND the mode is on. Only the
   paint is gated: egui's focus, and AccessKit's (which follows it), are
   untouched, so the screen reader and the keys behave as before. Two
   drag-only areas (the strip over the sheet's top and the gaps between
   tabs) used `Sense::drag()`, which egui makes focusable, so the first
   Tab used to vanish into the strip; they sense drags only now. Text
   fields keep egui's own focused stroke on a click, as a browser's
   inputs match `:focus-visible` on a click too.

Everything else in the inventory has a direct egui equivalent.
