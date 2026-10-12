# Parity

## Window feature inventory

Everything the Tauri window (`desktop/src-tauri`, `desktop/ui`, as of
0.11.1) does, one row per feature, so each can be ticked off as `sp-app`
reaches it. Written from the sources, not from memory: every row names the
file it was read from. The toolkit decision is in
`docs/superpowers/specs/2026-10-11-sp-app-design.md`.

Status words: **done** (built in `sp-app` and covered by a Rust test),
**built** (in `sp-app`, drawn and wired, no test of its own yet),
**model** (the decision is ported and tested; drawing it is unfinished or
untested), **todo**, **engine** (waits on `sp-engine`), **wave 3** (packaging,
signing, updater transport), **gap** (cannot reach 1:1 as specified; the
row says why and what stands in).

Sources are abbreviated: `ui/x.js` is `desktop/ui/x.js`, `tauri/` is
`desktop/src-tauri/`.

### W. The window itself

| ID | Feature | Source | Status |
|---|---|---|---|
| W1 | Window title "Screepub"; opens at 1280 x 800 | tauri/tauri.conf.json | built |
| W2 | Size floor 640 x 480; resizable | tauri/tauri.conf.json | built |
| W3 | No title bar of its own on macOS (titleBarStyle Overlay, hidden title): the paper runs under the traffic lights | tauri/tauri.conf.json | built |
| W4 | Drag strip: a transparent band the height of the sheet's top padding (space-7) at the top of the PAGE, scrolling away with it, moves the window; never covers a control | ui/frame.js, ui/style.css .drag-strip | built |
| W5 | The gaps in the tab bar also move the window; a tab click is still a tab | ui/frame.js (data-tauri-drag-region on the tablist) | built |
| W6 | Identifier `com.darkwell.screepub`, product name Screepub, version 0.11.1 | tauri/tauri.conf.json | wave 3 |
| W7 | macOS minimum 13.0; Apple-silicon-only Mac build | tauri/tauri.conf.json, desktop/README.md | wave 3 |
| W8 | Bundle carries LICENSE and THIRD-PARTY-NOTICES.md as resources | tauri/tauri.conf.json | wave 3 |
| W9 | Linux launcher entry Categories=Office; no PDF file association; no path read from the command line | desktop/README.md | wave 3 |
| W10 | macOS default app menu (Tauri's `enable_macos_default_menu`, on by default): About, Hide, Quit (Cmd-Q), Edit menu (Undo, Cut, Copy, Paste, Select All, so Cmd-C/V work in the device-request field), Window menu | tauri/src/main.rs (no menu set) | todo |
| W11 | Boot check: the engine is asked `--version --json`; a failure puts the engine's sentence on its own alarm-coloured line above the stamp (the stamp stays) | ui/main.js, ui/frame.js engineFailed | done |
| W12 | Engine missing or unstartable: "could not find/start the Screepub engine (sidecar ...)" with the build hint | tauri/src/sidecar.rs | gap: no sidecar in Rust (direct calls); the line is shown for an engine call that cannot start at all |
| W13 | No Cancel during a conversion; quitting mid-call lets the engine finish (atomic writes) | tauri/src/sidecar.rs, desktop/README.md | todo |
| W14 | Paper background edge to edge; page padding: 100px binding on the left, 48px on the right; the content block capped at 1180 and centred in what is left | ui/style.css .page .sheet | done |
| W15 | The rail: two brads and one punched hole, fixed to the window (the paper scrolls under them) at 30.7%, 50%, 69.3% of the height, centred 44px from the left edge, size clamp(20px, 3.4%, 36px) | ui/style.css .rail, ui/frame.js | done |
| W16 | Brad drawn as brand/components/brad.html: cast shadow ellipse, edge ring, radial brass face (specular, highlight, brass, shadow, rim stops), bounce ellipse, specular ellipse, edge stroke | ui/frame.js DEFS | built |
| W17 | Punched hole: paper lip, ground disc, radial shade | ui/frame.js DEFS | built |
| W18 | Short window (height <= 560): one brad, centred | ui/style.css | done |
| W19 | Narrow window (width <= 720): binding 11%, right margin space-4, brads at 5.5% and 20px, tab gap space-3, buttons narrower | ui/style.css, ui/surfaces.css | done |
| W20 | Light and dark from the OS (prefers-color-scheme), live as the OS changes: seven core colours in style.css and the generated tokens.css | ui/style.css, ui/tokens.css | done |
| W21 | Fonts bundled, never fetched: Courier Prime 400/700 and Literata 400, latin and latin-ext subsets (six WOFF2) | ui/style.css, ui/fonts | done |
| W22 | Type scale (10/11/12/13/14/15/17/18/30/44 px), radii (2/3/5), motion (0.1 s press, 0.2 s state, ease-out), spacing scale (0.4 to 3.5 rem), measure 66ch | ui/tokens.css | built |
| W23 | Hidden means hidden: no control drawn hidden can be seen or clicked | ui/style.css [hidden] | done |
| W24 | Every box that prints a path, a title or an engine sentence wraps anywhere | ui/surfaces.css overflow-wrap | built |
| W25 | Reduce Motion from the OS turns off every transition and animation listed in the R/P/N rows | ui/main.js state.reducedMotion, CSS media queries | done |

### N. Frame and navigation

| ID | Feature | Source | Status |
|---|---|---|---|
| N1 | Three surfaces on a tab bar on the paper: Convert, Read, Settings (id `tune`) | ui/frame.js SURFACES | done |
| N2 | Tablist named "Screepub"; each tab a tab, selected state exposed, controls its panel; each panel a tabpanel labelled by its tab and focusable (tabindex 0) | ui/frame.js, ui/main.js | done |
| N3 | Read and Settings are ABSENT (not dimmed) until a script is open | ui/frame.js tabPresence | done |
| N4 | Roving tab stop: only the selected tab is a Tab stop | ui/frame.js tabPresence | built |
| N5 | Left/Right move between tabs, wrapping, skipping absent ones; Home/End to the ends; the moved-to tab is focused and selected | ui/frame.js keydown | built |
| N6 | A tab that goes away while selected sends the window to Convert | ui/frame.js enable | built |
| N7 | Tabs arriving: fade and rise 5px over 0.2 s, staggered 70 ms by position; only on the way in; none with Reduce Motion | ui/style.css tab-arrive | built |
| N8 | Tab look: Courier bold 11px caps, letter-spaced, muted; the open one ink with a 1.5px brass rule under it; hover ink | ui/style.css .tab | built |
| N9 | Rev stamp "rev 0.11.1" at the foot of the sheet, right; the version is the RELEASE notes' version, not the engine's | ui/frame.js revLabel | done |
| N10 | The stamp is a button that opens the release notes sheet | ui/frame.js, ui/main.js | done |
| N11 | Update label beside the stamp: hidden until the update flow has words; brass rule under it; inert moments carry aria-disabled and no pointer | ui/frame.js setUpdateLabel | model |
| N12 | If the update label hides while focused, the keyboard goes to the page's first stop | ui/main.js | todo |
| N13 | Release notes as a modal sheet: Esc closes, focus trapped inside, the background inert, focus restored on close; "Close" button; dimmed backdrop; rises 16px over 0.2 s (none with Reduce Motion); width min(44rem, 88vw), max height 82vh, scrolls | ui/main.js, ui/style.css .sheet-over | done |
| N14 | Cmd-O (Mac) / Ctrl-O opens the file picker from any surface without changing surface; cancelling leaves the reader where they were | ui/main.js | done |
| N15 | A file dropped anywhere on the window converts on Convert; a file dragged over the window turns the well's border and icon brass | ui/main.js, ui/convert.js, ui/surfaces.css .well-targeted | built |
| N16 | Of several dropped paths, the first non-empty one converts | ui/convert.js droppedPath | done |
| N17 | A drop while converting is ignored (known gap, kept) | desktop/README.md | built |
| N18 | Where the keyboard lands after any native dialog closes or a redraw throws the focused control away: first control in the showing pane, else the pane, else its tab, else the Convert tab; never a hidden, disabled or detached control | ui/focus.js | done |
| N19 | scriptChanged: tabs appear or go, the devices poll starts or stops, the heads-up resets, every surface redraws for the new book | ui/main.js | built |

### E. Engine calls (`runEngine` argv; in Rust each is a direct `sp-engine` call through the `Engine` trait)

| ID | argv (ui/app.js `argv`) | Engine trait method | Counted toward busy | Status |
|---|---|---|---|---|
| E1 | `--version --json` | `version()` | yes | done |
| E2 | `<path> --json --progress --preview-inline --library [--force] [--options-json J]` | `convert(path, force, options, on_progress)` | yes | done |
| E3 | `<fountain> --json --preview-inline -o <epub> --options-json J` (rebuild in place) | `reconvert(fountain, epub, options)` | yes | done |
| E4 | `devices --json [--quick]` | `devices(quick)` | no | done |
| E5 | `eject <volume> --json` | `eject(volume)` | yes | done |
| E6 | `app-settings --json --set {"readerDevice":K}` | `set_app_settings(json)` | yes | done |
| E7 | `app-settings --json --set {"animatePage":B}` / `{"typewriterSounds":B}` | `set_app_settings(json)` | yes | done |
| E8 | `send <file> --json [--device ID]` | `send(file, device)` | yes | done |
| E9 | `settings <fountain> --json [--set J]` | `settings(fountain, set)` | yes | done |
| E10 | `export <epub> --json --for F [--fountain P] [--options-json J] [--out P]` | `export(epub, for, fountain, options, out)` | yes | done |
| E11 | `export <epub> --json --for kindle --check` | `kindle_check(epub)` | no | done |
| E12 | `kfx-status --json` | `kfx_status()` | no | done |
| E13 | `kfx-install --json` | `kfx_install()` | yes | done |
| E14 | `app-settings --json` (read) | `app_settings()` | no | done |
| E15 | `app-settings --json --set J` (library path, format defaults, keepScriptSettings) | `set_app_settings(json)` | yes | done |
| E16 | `reveal <path> --json` | `reveal(path)` | no | done |
| E17 | `routes <epub> --json [--quick]` | `routes(epub, quick)` | no | done |
| E18 | `route <key> <epub> --json [--out] [--fountain] [--options-json]` | `route(key, epub, out, fountain, options)` | yes | done |
| E19 | `route kindle-email-setup --json` | `email_setup()` | yes | done |
| E20 | Progress lines on stderr: `{"progress":{stage,percent,page,pages}}` and the page line `{"progress":{stage:"page",page,paragraphs}}`; several or partial lines per event; anything not a progress object ignored | `on_progress` callback | | done |
| E21 | An answer that is not JSON: "the engine did not answer in JSON:\n" plus the first 300 characters and an ellipsis | ui/app.js RAW_SHOWN | | gap: a direct call returns typed values, so there is no unparseable answer; the trait's `Err(String)` stands in |
| E22 | Busy count: counted calls in flight plus holds; idle = nothing counted running for 500 ms (ENGINE_QUIET_MS); holds released idempotently | ui/app.js holdEngine, whenIdle | | done |
| E23 | After an update installs, every engine call is refused with "Screepub has been updated. Restart it to keep going." | ui/app.js retireEngine | | done |
| E24 | One native dialog at a time (open, folder, save); a second ask returns nothing without opening; a dialog holds the engine (counted) while up; every close runs the focus restore | ui/app.js withOneDialog | | done |
| E25 | Open dialog filter "Screenplay": pdf, fountain, txt | tauri/src/main.rs pick_file | | done (rfd) |
| E26 | Folder picker starts at the current library; save box starts at `<book stem>.<ext>` with one filter named by the file's label | ui/app.js pickFolder, saveDialog; ui/send-routes.js | | done |
| E27 | Book queue: one queue per library EPUB; readers side by side, a writer alone, arrival order kept, a failure never stops the queue; turns save/convert write, kindle/send/copy/check read; a settle still counting down is cut short before any turn (its save goes first) | ui/book-queue.js | | done |
| E28 | Kindle line: Kindle-file exports of one book one at a time, before their book turn | ui/send-panel.js inKindleLine | | done |

### C. Convert

| ID | Feature | Source | Status |
|---|---|---|---|
| C1 | Idle: wordmark "Screepub" as the page's heading, centred, Courier bold 30px caps, 2px brass rule under it; smaller and higher in a window under 620 tall | ui/convert.js, ui/surfaces.css .wordmark | done |
| C2 | The well: dashed 1.5px border, radius 5, centred column: the document-with-arrow mark (26px, four strokes), "Drop a screenplay PDF", "or", "Choose PDF…  ⌘O" (or "Ctrl+O"), "Needs selectable text, not a scan." | ui/convert.js WELL, icon | done |
| C3 | Convert's column is at most 820 wide in every state but the result | ui/surfaces.css #surface-convert | built |
| C4 | One-time question under the well: "Check for new versions once a day?" Turn on / No thanks; only where updates are possible and the question is unanswered; a stale click after the notes switch answered does not overwrite; the line removes itself and the keyboard is restored | ui/convert.js askLine | done |
| C5 | Library line: "Books are saved in ~/…." or "…, set by SCREEPUB_LIBRARY."; the home folder folded to ~ only when the path is genuinely under it (case-insensitive on Windows paths, root homes) | ui/convert.js libraryLine, shortenHome | done |
| C6 | Change… (folder picker at the current library; a cancel or the same folder sends nothing) and Reset (only when a folder was chosen); neither when SCREEPUB_LIBRARY is set | ui/convert.js libraryActions | done |
| C7 | After Change: "New books go here. Books already converted stay where they are." only when the library moved; a refusal or thrown call shows its sentence on the secondary line and leaves the line as it was; NO_LIBRARY_MESSAGE for an answer that breaks the contract | ui/convert.js libraryAfter, showsMovedLine | model |
| C8 | Change and Reset go quiet while one runs, back afterwards even on a throw | ui/convert.js buildLibrarySlot | built |
| C9 | A probe answer that arrives after the well was redrawn is dropped (generation guard); a malformed or failed probe draws no line | ui/convert.js probeLibrary | built |
| C10 | The same probe carries the two page switches into the window state | ui/convert.js probeLibrary | built |
| C11 | Working: "Converting" heading, "The pages of <file> reflow themselves, one scene at a time.", a 220px track with a brass fill (width eased 0.2 s; none with Reduce Motion), a read-out that is a polite live status, "Please stand by:" | ui/convert.js drawProgress | built |
| C12 | Read-out words: "starting up"; while the book is held by another turn the Settings page's words for that wait ("Waiting for the Kindle file to finish building…" and the rest, longest wait first; "Waiting for the settings to finish saving…"; "Waiting for the book to be free…"); then "reading the pages (N%)", "reading the pictures (N%)", "building the book (N%)" | ui/convert.js waitingLabel, nextProgress | model |
| C13 | The bar never goes back: the percent is the engine's whole-job figure, clamped, rounded, never falls; unknown stages and non-numbers ignored; the stage never walks back; a stage change at the same percent is news | ui/convert.js nextProgress | done |
| C14 | Converting a PDF converted earlier in this window takes its turn on the book it rewrites | ui/convert.js landed, inTurn | built |
| C15 | The keyboard lands on the working pane (it has no control) | ui/convert.js drawProgress | built |
| C16 | Result layout: the book's first page (400 wide, 3:4, hairline, radius 5, paper) and the side (up to 620), 32 apart, centred in the block; under an 880 block the side first and the page under it, 400 wide and centred | ui/surfaces.css .result | done |
| C17 | The first page is the book's own (engine preview), scrolled to its first scene less 1.5 lines, not a Tab stop, hidden from the screen reader | ui/result-page.js | gap: see R18 |
| C18 | "<Title> is ready" ("The book is ready" with no title); counts "19 pages, 11 scenes" (singulars; a missing count left out); each warning in parentheses | ui/convert.js readyLine, countsLine | done |
| C19 | The send panel (D rows) in a box: hairline, radius 5, panel fill | ui/surfaces.css .send-panel | built |
| C20 | Three quiet links under it: Read it, Settings, Convert another (back to the well without closing the book) | ui/convert.js paintResult, reset | done |
| C21 | At the foot of Other ways' Save a file: Show in Finder / Show in File Explorer / Show in folder (by platform), the book's path, and the engine's refusal under them when the reveal failed (built into this result, never a later one) | ui/convert.js revealLabel, revealNote | done |
| C22 | A last send's line and its Eject come back with the result for the same book; another book's line is dropped unless a reader still waits to be ejected | ui/convert.js drawResult | built |
| C23 | Refusal: "Smash cut to:", the heading for the code (table of ten), the engine's sentence, Convert anyway (brass, only for `not-screenplay`, converts with force), Back to one (outline), Report a bug (quiet) | ui/convert.js drawFailure, HEADINGS | done |
| C24 | The `--force` sentence is taken out of the engine's message only where Convert anyway is drawn; never down to nothing | ui/convert.js withoutCliRemedy | done |
| C25 | Unknown or missing code: "internal", "The engine did not answer"; no sentence: "The engine refused the file without saying why." | ui/convert.js failureFor | done |
| C26 | "<Title> is still open. This changed nothing about it." when a book was already open; the open book survives a refusal | ui/convert.js stillOpenNote | model |
| C27 | "Error code" label and the code in a chip | ui/convert.js | done |
| C28 | A conversion that could not run at all shows the refusal screen with code internal and the thrown sentence | ui/convert.js convertPath | done |
| C29 | "Convert it again" (Read's blank notice, Settings' fault, the panel's no-book line): converts the open script's source, or just goes to Convert when there is none or one is running | ui/convert.js convertAgain | built |
| C30 | Read's and Settings' Send: back to the result on Convert, redrawn only if the well or a refusal replaced it, keyboard on its first control | ui/main.js sendBook, ui/convert.js showResult | done |
| C31 | No screen prints a stray "null" | tests/desktop-ui.test.ts | done |

### P. The page that writes itself, and its sounds

| ID | Feature | Source | Status |
|---|---|---|---|
| P1 | Starts when the engine's page line arrives, unless Reduce Motion is on, "Animate the page" is off, or the line carries no text | ui/page-show.js canShow | done |
| P2 | Drawn in the result's own box: a typed page over where the result's page will be; the side says "Converting" and the file name | ui/page-show.js beginShow | built |
| P3 | The slim line under the page, a polite live status: "Starting up", "Reading page N of M" (never back a page), "Reading the pages (N%)" from an engine that names no page, "Reading the pictures", "Building the book" | ui/page-show.js readingLine | model |
| P4 | The page's whole text is in the window for a screen reader from the first moment; the typing itself is hidden from it | ui/page-show.js .page-show-words | built |
| P5 | Typing takes 4000 ms whatever the script, spread evenly over the characters (code points); empty paragraphs skipped | ui/page-timing.js | done |
| P6 | The wave: block i takes the book's form two blocks behind the typing, never before the book exists plus 120 ms a block (faster on long pages, a sweep never over 1200 ms) | ui/page-timing.js formAt | done |
| P7 | Each formed block glows brass for 600 ms; the typed page lifts away over 250 ms at the end | ui/page-timing.js, ui/page-show.js | done |
| P8 | Typed paragraphs matched to the book's blocks by their words (letters and digits, upper case; a prefix of at least half counts; the first match anywhere, then within 4); an unmatched paragraph only glows | ui/page-show.js alignBlocks | model |
| P9 | A click anywhere or any key ends it: before the book exists the whole page shows typed, and it ends the moment the book arrives | ui/page-show.js onPress | done |
| P10 | A refusal or another file cancels it | ui/page-show.js cancel | built |
| P11 | Key sounds at most one per 55 ms while typing; a carriage-return bell when a block finishes, at most one per 700 ms | ui/page-timing.js soundsDue | done |
| P12 | Key: 35 ms of white noise through a band-pass at 1800 to 3400 Hz (random each key), Q 1.4, gain 0.3, exponential fade; bell: 2093 Hz sine, gain 0.08, 0.5 s fade; made, not recorded | ui/typewriter.js | model |
| P13 | Sounds only when "Typewriter sounds" is on; a machine with no audio is quiet; closed when the show ends | ui/convert.js startShow, ui/typewriter.js | todo |

### R. Read

| ID | Feature | Source | Status |
|---|---|---|---|
| R1 | Head: the title (Courier bold caps), "by <Author>" when known, Show scenes / Hide scenes (names the click; expanded state exposed), Send (quiet) | ui/read.js draw, indexToggle | done |
| R2 | Two columns: a 218 index slot, then the script up to 706, 32 apart; the index opens into its own slot, so the script never moves | ui/surfaces.css .reader | done |
| R3 | Under an 880 block: the index is a short strip above the script (at most 6.5rem, entries inline), absent when shut | ui/surfaces.css | built |
| R4 | The script's frame: 68% of the window's height (58% narrow, 62% short), at least 300, paper with a hairline and radius 5 | ui/surfaces.css .script-frame | done |
| R5 | The stage around the script is the Tab stop, a group named "<Title>, the script", with a focus ring | ui/read.js | done |
| R6 | Keys on the stage: Up/Down 60px, PageUp/PageDown/Space 90% of the frame (at least 120), Home/End to the ends | ui/read.js scrollStep | model |
| R7 | The scene index, a navigation region named "Scenes": "N scenes" (1 scene), then one entry per scene: the place in caps, the time of day after an en dash, quieter; the run before the first heading is "Opening"; the page number the engine puts inside a heading is left out | ui/read.js sceneLabel, railEntries, headingText | done |
| R8 | One Tab stop for the whole index; Up/Down/Home/End move it, stopping at the ends | ui/read.js railStep | model |
| R9 | Pressing an entry scrolls its scene to the top (smooth, instant with Reduce Motion) | ui/read.js buildRail | built |
| R10 | The scene being read is marked (current, highlighted) and kept in the index's view; the index's Tab stop follows it unless the keyboard is walking the index | ui/read.js markCurrent | built |
| R11 | Open by default; opening and shutting slides 0.8rem and fades over 0.2 s (none with Reduce Motion); shut, it is out of the Tab order | ui/read.js setIndexOpen, ui/surfaces.css | done |
| R12 | No headings: "No scene headings in this script." | ui/read.js NO_SCENES | model |
| R13 | A book that came back with no pages: "No pages came back", the sentence, Convert it again, Send | ui/read.js NOTICES | done |
| R14 | The place is kept as a scene and a fraction into it, across a Settings re-render, a window resize and a tab switch; a scene that is gone puts the reader at the top | ui/read.js readerPlace, scrollTarget, keep, restore | model |
| R15 | Re-rendered from every Settings save that rebuilds the book | ui/tune.js flush | built |
| R16 | The device layer over the book: the window's ink and paper, margins 1.6em and 7%, re-dressed live when the OS theme changes | ui/read.js deviceCss, followTheme | built |
| R17 | Pictures in the book shown (the engine writes them as data: URIs) | ui/read.js header | engine (needs the preview model to carry pixels) |
| R18 | The script is the ENGINE's preview (the EPUB's XHTML and css.ts, untouched) rendered by a browser engine: one renderer for the window and the book, so a formatting decision cannot disagree | ui/read.js header | gap: no HTML/CSS engine in the toolkit; sp-app draws the preview from structured blocks with the same options (see the design doc). Pixel parity with the EPUB's CSS is not reachable this way |
| R19 | "Pager": paging is the keyboard's (R6); there is no on-screen pager control in the window | ui/read.js | done |

### S. Settings (`tune`)

| ID | Feature | Source | Status |
|---|---|---|---|
| S1 | Head "This script’s settings" with Send on its line; the lede paragraph | ui/tune.js | done |
| S2 | While the answer is out: "Reading this script’s settings…" and Send (never the fault screen) | ui/tune.js emptyPaneMode | built |
| S3 | Settings that could not be read: "Settings out of reach", the sentence, the engine's sentence in alarm colour, Convert it again, Send; asked again next visit | ui/tune.js FAULT, load | done |
| S4 | Two columns: 420 of settings and the preview in the rest, the preview pinned while the page scrolls and as tall as the window less its margins; under an 880 block one column, the preview below at 44% height, unpinned, boxes a measure wide | ui/surfaces.css .tune-split | done |
| S5 | Presets: "Load a preset", one button per preset the engine lists, the one the script is on marked (brass outline), inert and current; "Overwrites every setting below."; a preset replaces all twenty; the mark follows the values, and goes when a knob moves | ui/tune.js drawPresets, applyAll, presetsFrom, currentPreset | done |
| S6 | Five groups, each a disclosure box (hairline, radius, panel fill), only the first open; a brass triangle that turns as it opens (0.2 s) | ui/tune.js drawGroup, groupStartsOpen | done |
| S7 | "Read from the PDF" carries its note: these four apply at the next conversion | ui/tune.js GROUPS | built |
| S8 | Twenty knobs: label, Beta badge on the two picture switches, then a slider with a tabular read-out ("1.0 em", "20%"; "not set" for a missing value), a checkbox, or a choice | ui/tune.js GROUPS, displayValue | built |
| S9 | Each knob's sentence under it: why it is idle (first), then what it is for; tied to the control as its description; hidden when empty | ui/tune.js notesFor, state | done |
| S10 | Character name indent and Parenthetical indent are idle (disabled, muted) unless Character names are Indented: "Only when character names are indented." and they come back live in place when it changes | ui/tune.js idleReason, refreshIdle | done |
| S11 | A moved value is clamped to the engine's range; a non-value is never sent | ui/tune.js coerceValue | done |
| S12 | Changes gather for 300 ms, then save on the book's turn: settings --set, then the rebuild; the status line, a live status: "Changed. Saving in a moment…", "Saving, and rebuilding the book to match…", "Saved. The book on disk matches what you see.", the wait's words, or the engine's refusal in alarm colour | ui/tune.js STATUS, statusFor, schedule, flush | done |
| S13 | A failed save owes its changes again; a newer change wins; another script opened meanwhile drops the answer (era) | ui/tune.js restorePending, era | model |
| S14 | The engine's stored values redraw the screen when they differ and nothing newer is owed | ui/tune.js flush | built |
| S15 | The rebuild's warnings in parentheses under the status, as a live status, replaced only by the next good rebuild | ui/tune.js warningLines | model |
| S16 | A settle holds a pending update restart until its save has started | ui/tune.js holdEngine | done |
| S17 | Foot: "New scripts start from Screepub’s own defaults." / "… your own defaults.", Use these for new scripts, Reset new scripts to Screepub’s defaults (only when they differ), a live note ("New scripts will start from these settings." and the reset's), both quiet while one runs, era guard | ui/tune.js drawDefaultsFoot, writeDefaults | built |
| S18 | "When a PDF is converted": a named radio group, Keep its settings / Follow the defaults, each with its line as its description; one Tab stop and arrows; writes queued, only the newest paints; "Saved. It applies to PDFs you convert from now on." | ui/tune.js KEEP_CHOICE, chooseKeep | built |
| S19 | "While a script converts": Animate the page, Typewriter sounds (rests while the page is still), each with its line; "This computer is set to reduce motion, so the page does not animate." under Reduce Motion; a refused write flips back with the sentence | ui/tune.js PAGE_SWITCHES, flipSwitch | built |
| S20 | The preview: labelled "Preview", hidden from the screen reader, not a Tab stop; the same document as Read, re-rendered after each save | ui/tune.js drawPreview | gap: see R18 |
| S21 | Knob effects: live (preview changes), book (title page: the book only), reconvert (the four read from the PDF) | ui/tune.js EFFECTS | done |

### D. The send panel (Convert's result, right half)

| ID | Feature | Source | Status |
|---|---|---|---|
| D1 | Nothing drawn until the reader's device is known (app settings `readerDevice`), so the cards never flash for someone who chose | ui/send-panel.js start | built |
| D2 | No book on disk: "No book to send", the reason, Convert it again | ui/send-panel.js drawBlocked, send-routes.js blockedReason | model |
| D3 | Cards: "What do you read on?", Kindle; iPhone, iPad or Mac (Apple Books); Kobo; tolino; reMarkable; My device isn’t here (Beta) | ui/device.js DEVICES, MISSING | done |
| D4 | A card for a reader plugged in now: brass edge and a green dot with "Plugged in"; its name read as "<Name>, plugged in"; marked in place when the poll changes | ui/send-panel.js markCard | done |
| D5 | A card press goes calm at once and stores the choice app-wide; a refused store says why on the status line | ui/send-panel.js chooseDevice | done |
| D6 | The asterisk on what nobody has run on that hardware (everything but a Kindle on a Mac; Apple never): "About <Name>: never tried on real hardware"; hover opens the note, leaving closes it after 400 ms; a press pins it (expanded state exposed) | ui/send-panel.js asterisk, device.js asteriskNote | done |
| D7 | The note: "Built and tested in code, never tried with a real Kindle on Windows/Linux yet…" or "… on a real <Name> yet…", then It worked / It did not work (Beta) where there is a route to report | ui/device.js, ui/requests.js reportRoute | model |
| D8 | A report: both answers dead while it is out; thanks takes them away; a failure offers Try again for the same answer | ui/send-panel.js reportAnswers | built |
| D9 | Calm screen: one brass button for the way that fits (device.js offerFor), its Beta badge for our own AZW3, "Plugged in" beside a device button, the asterisk; the lines under it (the web-drag line, the plug-in line, Apple elsewhere, tolino on Windows) | ui/send-panel.js drawCalm, device.js offerFor | done |
| D10 | Kindle: plugged in → copy (then web and email lead Other ways); otherwise Send to Kindle (web), then email, then Save the EPUB | ui/device.js offerFor | model |
| D11 | Apple: Books on a Mac; elsewhere Save the EPUB with the iCloud line | ui/device.js offerFor | model |
| D12 | While the route list is out: "Looking for every way to send it…" | ui/device.js LOOKING | model |
| D13 | Route list asked quick then full on the calm screen's first draw for a book and on every plug-in change; an older answer never replaces a newer one; nothing redraws while a send runs (owed, asked after); an unchanged list does not redraw | ui/send-panel.js refresh | built |
| D14 | A list that cannot be read: the engine's sentence in place of the brass button, never a brass picked from half a list | ui/send-panel.js fault, send-routes.js routesFrom | model |
| D15 | A reMarkable reader: the full list asked again every 10 s while the calm screen shows, and on a focus return | ui/send-panel.js lookForRemarkable | built |
| D16 | Other ways (outline, expanded state) and Different device (quiet); opening and shutting keeps the keyboard on the toggle; shutting puts the KFX checklist away | ui/send-panel.js toggleWays | done |
| D17 | Other ways, Send it: the offer's follow-ups, then every other route in the engine's order, dimmed ones with their fix; never the brass one or the two saves | ui/device.js sendItRows | done |
| D18 | Readers that only need plugging in or docking folded into one muted line ("Kobo, tolino and reMarkable: plug in over USB to send."), each name with its own asterisk; fewer than two stay rows | ui/device.js foldUnplugged | model |
| D19 | A route row: title, the engine's detail, the volume for a mounted reader, the email row's "First time? Amazon needs your sender address approved…" with Open Amazon’s page; the button (none when dimmed) | ui/send-panel.js routeRow, send-routes.js routeLines, emailSetupHint | built |
| D20 | Buttons that hand over the Kindle file name it: "Copy to Kindle (.kfx)", "Save a Kindle file (.azw3)…" | ui/send-routes.js buttonLabel | done |
| D21 | Save a file: EPUB, KFX, AZW3 (Beta), MOBI, Fountain, FDX (Beta), each with who it is for and Save…; KFX not makeable here: "This computer cannot make one yet." and Set it up; no .fountain: "This book was made without a .fountain, so there is none to work from." | ui/device.js SAVE_TYPES, saveRows | done |
| D22 | A save: the Save box first (named for the book, one filter), then the engine writes there; "Saving…", "Saved <file>.", or the refusal | ui/send-panel.js saveFile | built |
| D23 | A copy to a reader: "Getting the book ready for <Name>…" (or the build line), "Copying it to <Name>…", then "<Title> is on your <Name>. Eject it before you unplug." with Eject in the brass place, or "Sent to <Name>." for one that does not mount; what went across in the engine's words under it | ui/send-panel.js sendTo, device.js copiedState | done |
| D24 | A route the engine performs: "Opening <title>…", then the engine's note or "The engine said it worked, without saying what it did." | ui/send-panel.js perform, send-routes.js routeNoteFrom | built |
| D25 | One flow at a time: every button dead while a send, a save, a route, an eject or a KFX install runs; nothing redrawn under it; a flow begun for one book says nothing beside another (era) | ui/send-panel.js sending, era | built |
| D26 | The keyboard goes back to the button that started a flow (found by its key after a redraw), else the brass, else the page's plan; the brass is owed the keyboard on a fresh result and after a card press | ui/send-panel.js keepFocus, giveBackFocus, brassOwed | todo |
| D27 | Eject <Name> (Beta off a Mac): "Ejecting <Name>…", "Your <Name> is ejected. You can unplug it now.", "… is not plugged in any more.", or the refusal with the button kept while it is still plugged in; unplugging takes the button away and keeps the line | ui/device.js ejectOutcome, ejectStillThere | done |
| D28 | A reader still waiting to be ejected follows the reader to the next book's result | ui/send-panel.js mountPanel, sendTo | built |
| D29 | The Kindle file asked about when the result shows and on every focus return (`export --check`), naming the file on its buttons | ui/send-panel.js checkKindle | built |
| D30 | Built ahead in the background when the check says the KFX is out of date, Calibre builds it, a Kindle is in play, once per book version and file kind, never while one runs or a flow is busy | ui/send-routes.js buildsAhead, send-panel.js maybeBuildAhead | model |
| D31 | The build line and a sweeping bar while a Kindle file builds: "Building the Kindle file. This takes about half a minute." (KFX), "… This takes a moment.", "Building the Kindle file…"; the bar breathes instead of sweeping under Reduce Motion; a live status that speaks only when its words change | ui/send-routes.js buildLine, ui/surfaces.css .build-sweep | built |
| D32 | Settings fetched before an export if this window never saw them, so a rebuild uses the script's own | ui/send-routes.js needsSettings | built |
| D33 | My device isn’t here: the sentence, "Which device?" field (80 characters), Send (Beta; dead while empty or sending), "Sends the device’s name, Screepub’s version and your platform. Nothing else.", a live line for the answer, Save the EPUB…, Back; the typed name survives a redraw and Back | ui/send-panel.js missingPanel, ui/requests.js | done |
| D34 | The status line (live) and its alarm colour for a failure; kept with the book on the window state | ui/send-panel.js say | done |

### K. The KFX checklist ("Best Kindle quality")

| ID | Feature | Source | Status |
|---|---|---|---|
| K1 | Opened by Set it up under Save a file, in place; the engine's checklist taken whole or not at all (Calibre, Kindle Previewer, the KFX plugin) | ui/kfx.js checklistFrom | model |
| K2 | Shown only where KFX is possible, and only while not ready (or an install just ran or is running) | ui/kfx.js showSetup | model |
| K3 | Each step: its name and Installed, a link button (opens the page; a failure says "Could not open the page. It is at <url>"), an Install button (brass, dead while busy, "Installing…"), or the engine's why | ui/kfx.js controlFor | built |
| K4 | Install: "Downloading and installing the KFX plugin. This takes a few seconds.", then "Installed the KFX plugin <v>. Kindles now get KFX. Removed an older copy: <name>." or the refusal; every other panel button dead while it runs | ui/kfx.js afterInstall, installedLine | model |
| K5 | Asked again whenever the window gets the focus back; an unchanged answer redraws nothing | ui/kfx.js probe | built |
| K6 | The links allowed: the issue tracker, Calibre's macOS and Windows download pages, Kindle Previewer's page; nothing else opens | tauri/capabilities/default.json | done |

### H. Devices and the Kindle heads-up

| ID | Feature | Source | Status |
|---|---|---|---|
| H1 | `devices --quick` every 2 s while a book is open, stopped when none is; one ask out at a time; a failed or malformed listing changes nothing; listeners hear only a changed list | ui/device-watch.js | done |
| H2 | A reader with an empty name is named for its kind | ui/device-watch.js devicesFrom | model |
| H3 | Heads-up: "Kindle plugged in. Copy <Title>?" in a box under the tabs, a live status, Copy it (brass) and Not now; once per plug-in (the first list only seeds; a Kindle that leaves and returns is news again) | ui/heads-up.js, ui/device.js headsUpFor | done |
| H4 | Not over the result (its card says it), never while converting; closes when the page under it becomes one of those | ui/heads-up.js recheck | built |
| H5 | Goes by itself after 8 s; slides in from above (none with Reduce Motion); the keyboard is not left on it as it goes | ui/heads-up.js, ui/surfaces.css | built |
| H6 | Copy it: the result on Convert, then a copy to that Kindle, unless a conversion started meanwhile | ui/main.js copyTo | built |

### U. Updates

| ID | Feature | Source | Status |
|---|---|---|---|
| U1 | Only on a Mac, and only where the updater exists in the build | ui/update.js updatesPossible, ui/app.js updaterReady | done |
| U2 | Opt-in once ("Check for new versions once a day?"); "said no" and "never asked" kept apart; stored in the window's own storage | ui/update.js rememberAnswer, shouldAsk | done |
| U3 | At most one check a day, stamped BEFORE the request; a manual check ignores the switch and the throttle | ui/update.js runCheck | model |
| U4 | A failed check is a message, never "you are current"; an Intel Mac is told "New versions of Screepub run only on Macs with Apple silicon…" | ui/update.js isIntelMacRetired | model |
| U5 | A version found is remembered across launches until a check answers otherwise | ui/update.js rememberFound, rememberedOffer | model |
| U6 | The label: "Update to 0.8.0", "Downloading 0.8.0…" / "… 40%", "Installing…", "Restarting after this finishes…", "Restart now", "Restarting…", "Update failed. Try again", "Update installed. Quit and reopen Screepub to use 0.8.0."; clickable only on offer (not a retry's), failed and stalled | ui/update.js updateLabel, labelActionable | model |
| U7 | Download, verify (minisign), swap the bundle, then retire the engine, wait for 500 ms of quiet (capped at 120 s, then "Restart now"), restart into the new bundle; a refused restart is "installed", not a failure; a failed cleanup after a good install is not a failure | ui/update.js installAndRestart, ui/app.js updateInstall | wave 3 (its rules are ported: model/update.rs) |
| U8 | The endpoint: latest.json on the GitHub releases; the public key | tauri/tauri.conf.json | wave 3 |
| U9 | One run, one moment, heard by the label and the notes alike; a newer offer during a run waits; a same-version offer after a failure keeps the failure on screen | ui/update-flow.js | todo |
| U10 | Version comparison is the engine's (`src/update/compare.ts`: v prefix, build metadata, describe suffixes, prereleases) | ui/update-compare.js | done |

### O. Release notes sheet

| ID | Feature | Source | Status |
|---|---|---|---|
| O1 | "Screepub 0.11.1", the headline, each section with its items (a bold lead or plain prose), compiled from docs/releases/<version>.md, offline | ui/notes-surface.js, ui/notes.js | done |
| O2 | No heading with nothing under it | ui/notes-surface.js visibleSections | done |
| O3 | Updates block where updates are possible: the once-a-day switch with its words, Check for updates, a live line, the new version's notes, Install <version> | ui/notes-surface.js updateBlock, notesView | built |
| O4 | Elsewhere: "Screepub does not update itself here. New releases are at github.com/ssandweiss/screepub/releases." | ui/notes-surface.js | built |
| O5 | The switch is resynced each time the sheet opens | ui/notes-surface.js show | built |
| O6 | Check for updates: "Looking…", then the flow's words; disabled while a run is busy | ui/notes-surface.js | built |

### B. Bug report, Beta badges, the request service

| ID | Feature | Source | Status |
|---|---|---|---|
| B1 | Report a bug opens a pre-filled GitHub issue: an instruction, "What happened:" with the code and the engine's sentence, cut to fit 2000 characters, and "Screepub <version> · <platform>" | ui/feedback.js newIssueUrl | done |
| B2 | Every absolute path in it becomes `<path>` plus its extension; the home folder becomes ~ | ui/feedback.js redact | done |
| B3 | The Beta badge: "Beta", in a hairline box, beside (never inside) what it qualifies, read out with its line, described as "New: works in our tests, not yet tried widely on real devices or apps." | ui/beta.js | built |
| B4 | Where it is worn: the FDX and AZW3 save rows, our own AZW3 on Copy to Kindle, the two picture switches, Eject off a Mac, the device-request Send, the report answers, the My device isn’t here card | ui/beta.js, callers | built |
| B5 | The request service: POST JSON to https://screepub-device-requests.samuelsandweiss.workers.dev/request; a device form or a report form with version and platform; 15 s timeout; "Thanks. We count every request.", offline, too many (429), garbled, or "Could not send. <reason>" | ui/requests.js | done |
| B6 | The window's network allow-list (CSP connect-src): only that address; the opener allow-list: only the four pages in K6 | tauri/tauri.conf.json, capabilities | done |

### L. Layout, measured (tests/desktop-layout-measured.test.ts)

| ID | Feature | Status |
|---|---|---|
| L1 | 1280 x 800: block 100..1232 (1132 wide), not folded | done |
| L2 | 1800 x 1000: block capped at 1180, starting at 336 | done |
| L3 | 900 x 800: block 752, folded (Read, Settings and the result one column) | done |
| L4 | Brads 44 px from the left edge at every size over 720 | done |
| L5 | Result: 400 page, 32 gap, panel up to 620, pair centred; folded: panel first, page under | done |
| L6 | Read: 218 slot then 706 script; folded: index above, script takes the block | done |
| L7 | Settings: 420 column, preview fills the rest; folded: one column | done |
| L8 | Nothing on the result runs off the side; cards and the brass inside the panel | done |

### A. Accessibility, as a list

| ID | Feature | Source | Status |
|---|---|---|---|
| A1 | Every control is a native control with its own words as its name (no icon-only control except the asterisk, which has its own label) | all | done |
| A2 | Live regions (polite status): the conversion read-out, the show's slim line, Settings' status and warnings, the defaults note, the keep note, the switch note, the send status, the build line, the request line, the report line, the KFX status, the heads-up, the notes' update line | all | done |
| A3 | Descriptions: each knob's sentence, each radio's and switch's line, the Beta badge's note | ui/tune.js, ui/beta.js | built |
| A4 | Focus rings: 2px ink outline offset 2 to 3 px on every control, the panes and the reader's stage, keyboard only | ui/style.css, ui/surfaces.css | built |
| A5 | Decorative furniture hidden from assistive technology: the rail, the well's mark, the result's page, the typed page, the preview, the build bar | all | built |
| A6 | Nothing is announced twice: a live line is rewritten only when its words change | ui/send-panel.js showBuild, ui/kfx.js say | built |

**Inventory size: 238 rows** (W 25, N 19, E 28, C 31, P 13, R 19, S 21, D 34, K 6, H 6, U 10, O 6, B 6, L 8, A 6). As of 2026-10-11: 122 done, 69 built, 29 model, 6 todo, 6 wave 3, 1 engine, 5 gap.

## Window test files

The TypeScript tests that load `desktop/ui` (or `desktop/src-tauri`) are
the window's parity spec. Where each one's behaviors live in `sp-app`
(`m/` is `rust/crates/sp-app/src/model/`, `screens` is
`rust/crates/sp-app/tests/screens.rs`):

| TS test file (tests) | Rust tests | Status |
|---|---|---|
| desktop-ui.test.ts (425) | m/convert.rs, m/read.rs, m/tune.rs, m/kfx.rs, m/book_queue.rs, m/busy.rs, m/focus.rs, m/frame.rs, m/update.rs, m/feedback.rs, engine.rs (argv contract), screens (Convert, Read, Settings, notes, heads-up, focus after a dialog, stray null) | partial: every decision block ported; mounted-DOM blocks ported as screens tests where the behavior is drawn; source-reading tests about desktop/ui itself (CSP, no innerHTML, one file talks to Tauri) have no Rust subject |
| send-panel-ui.test.ts (125) | screens (cards, calm, Other ways, after a copy, My device isn't here), m/device.rs | partial: build-ahead timing, the reMarkable clock and keep-focus-by-key blocks not yet ported |
| send-routes-ui.test.ts (71) | m/send_routes.rs | partial: the round trip against the engine's real route catalog waits on sp-engine |
| send-device.test.ts (34) | m/device.rs | ported |
| page-show.test.ts (22) | m/page_show.rs, app/show.rs tests, screens (the page writes itself) | ported (formed blocks draw natively: gap R18) |
| page-timing.test.ts (16) | m/page_timing.rs | ported |
| device-watch.test.ts (9) | m/device_watch.rs | ported |
| device-requests-ui.test.ts (11) | m/requests.rs, screens (My device isn't here) | ported |
| desktop-update-install.test.ts (20) | m/update.rs, m/busy.rs (retire, holds, quiet) | partial: the install and restart run waits on the wave 3 transport |
| desktop-notes.test.ts (14) | m/notes.rs, screens (the stamp opens the notes) | ported (the generator becomes an xtask) |
| desktop-layout.test.ts (26) | m/layout.rs | partial: geometry ported; CSS-text pins have no Rust subject |
| desktop-layout-measured.test.ts (16) | m/layout.rs (900, 1280, 1800) | ported as geometry; a pass measuring drawn rects is still to write |
| desktop-tokens.test.ts (9) | theme.rs (values pinned to tokens.css and style.css), fonts.rs (the six faces, byte for byte) | ported |
| desktop-shell.test.ts (67) | app/host.rs (opener allow-list, the one network address), main.rs (window size, floor) | partial: Tauri pins (sidecar name, capabilities, tauri.conf) retire with Tauri; updater and bundle pins move to wave 3 |
| beta-marks.test.ts (10) | m/tune.rs, m/device.rs (which rows wear it), screens (badges drawn) | partial: CLI help, README and site checks are not the window's |
| update-compare-module.test.ts (9) | m/update_compare.rs | ported (moves to sp-engine) |
| no-em-dash.test.ts (3) | none yet | todo: a Rust source scan of user-facing strings |
| app-references, capture*, build-*, bump-version, cli-*, release-*, smoke-bundle | not window behavior | owned by the xtask, CLI and packaging waves |
