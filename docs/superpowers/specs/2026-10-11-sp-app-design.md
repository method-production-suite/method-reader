# sp-app: the window in Rust, and the toolkit it is drawn with

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

## What cannot reach 1:1, precisely

1. **The book preview is not the EPUB's own CSS** (inventory R18, C17, S20,
   P8). The window renders the engine's XHTML and `css.ts` stylesheet in a
   browser engine, so the window and the book share one renderer. egui has
   no HTML/CSS engine. sp-app draws the preview from structured blocks
   (scene heading, action, cue, parenthetical, dialogue, transition, page
   marker, figure, dual dialogue) applying the same FormatOptions. The
   formatting decisions stay the engine's, but the RENDERING is a second
   implementation of css.ts, and it will not be pixel-identical to a
   reading device or to today's preview. The alternative that keeps one
   renderer is embedding Blitz (Dioxus's pure-Rust HTML/CSS engine:
   Stylo, Parley, Vello) into an egui texture; it is pre-1.0, would bring
   its own accessibility tree to bridge, and is not proposed here. Owner
   decision needed.
2. **The formed blocks in the page show** (P8) use that same native
   renderer, so "the book's own block" is the native rendering of it.
3. **An answer that is not JSON** (E21) cannot happen with direct calls;
   the trait's `Err(String)` is shown in its place.
4. **No sidecar** (W12): the "could not find the engine" sentence has no
   cause left; an engine that cannot start at all is a panic caught at the
   call boundary and shown as a refusal with code `internal`.
5. **Reduce Motion on Linux** is read from GNOME's setting only (mundy's
   `_gnome_only`); KDE and others read as "motion allowed".
6. **Focus rings and hover are egui's model**: keyboard focus is shown with
   the same 2px ink ring, but egui shows it on any focus, not only
   keyboard-initiated focus (`:focus-visible`); sp-app hides rings after a
   pointer press to approximate it.

Everything else in the inventory has a direct egui equivalent.
