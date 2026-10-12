# Parity ledger

The Rust port is done when every row here is ported and green, and
`cargo xtask parity` is clean (docs/superpowers/plans/2026-10-11-pure-rust-port.md).
A row's owner is the crate whose tests carry its checks after the TypeScript
is deleted. A difference the port keeps on purpose is not fixed by editing a
check: it is written up under "Decisions" below for the owner to see.

Status: `todo`, `partial` (say what is left), `done` (name the Rust test
file), or `retired` (say why the check has nothing left to guard).

## Behavior tests: tests/*.ts

140 files and 3753 `test(...)` calls, counted 2026-10-11. (The
brief's "142" was `ls tests`, which also counts the fixtures/ and helpers/
folders.) Preloads and helpers are listed because their guarantees have to
move too.

| TypeScript | tests | Rust owner | status | notes |
|---|---:|---|---|---|
| tests/app-references.test.ts | 14 | xtask | todo | the retired Swift app stays retired |
| tests/app-settings.test.ts | 31 | sp-engine | todo |  |
| tests/artwork.test.ts | 44 | sp-parser | todo |  |
| tests/azw3-calibre.test.ts | 0 | sp-azw3 | todo | reads the AZW3 back through Calibre: isolate CALIBRE_CONFIG_DIRECTORY |
| tests/azw3-chunk.test.ts | 8 | sp-azw3 | todo |  |
| tests/azw3-compare.test.ts | 0 | sp-azw3 | todo | compares with Calibre's AZW3: isolate CALIBRE_CONFIG_DIRECTORY |
| tests/azw3-corpus.test.ts | 0 | sp-azw3 | todo |  |
| tests/azw3-epub.test.ts | 3 | sp-azw3 | todo |  |
| tests/azw3-index.test.ts | 7 | sp-azw3 | todo |  |
| tests/azw3-kindleunpack.test.ts | 0 | sp-azw3 | todo |  |
| tests/azw3-palmdoc.test.ts | 5 | sp-azw3 | todo |  |
| tests/azw3-revision.test.ts | 8 | sp-azw3 | todo |  |
| tests/azw3-tbs.test.ts | 4 | sp-azw3 | todo |  |
| tests/azw3-xml.test.ts | 6 | sp-azw3 | todo |  |
| tests/azw3.test.ts | 9 | sp-azw3 | todo |  |
| tests/beta-marks.test.ts | 10 | sp-cli | todo | the CLI help, README and site say "beta" the same way |
| tests/boilerplate.test.ts | 16 | sp-parser | todo |  |
| tests/brand-components.test.ts | 6 | xtask | todo | brand/ stays HTML; the check moves |
| tests/brand-tokens.test.ts | 9 | xtask | todo |  |
| tests/build-app-bundle.test.ts | 72 | xtask | todo |  |
| tests/build-cli-e2e.test.ts | 4 | xtask | todo |  |
| tests/build-cli.test.ts | 83 | xtask | todo |  |
| tests/build-sidecar.test.ts | 28 | xtask | todo | the Rust app has no sidecar: most of this retires; decision for the owner |
| tests/build-update-manifest.test.ts | 20 | xtask | todo |  |
| tests/bump-version.test.ts | 15 | xtask | todo |  |
| tests/bundle-archive.test.ts | 27 | xtask | todo |  |
| tests/calibre-config-isolation.test.ts | 7 | sp-engine | todo |  |
| tests/capture-chrome.test.ts | 5 | xtask | todo |  |
| tests/capture.test.ts | 46 | xtask | todo | README and site pictures; drives desktop/ui/app.js today |
| tests/check-latest.test.ts | 17 | xtask | todo |  |
| tests/cli-app-defaults.test.ts | 8 | sp-cli | todo |  |
| tests/cli-app-settings.test.ts | 56 | sp-cli | todo |  |
| tests/cli-azw3.test.ts | 4 | sp-cli | todo |  |
| tests/cli-device-commands.test.ts | 35 | sp-cli | todo |  |
| tests/cli-devices.test.ts | 32 | sp-cli | partial | the handlers (devicesCommand, selectDevice, sendCommand) are in sp-device/tests/commands.rs; left for sp-cli: verb dispatch (resolveCommand, VERBS) and the settings file a send remembers into (sp-device takes `remember` as a callback) |
| tests/cli-eject.test.ts | 24 | sp-cli | partial | the handler and the real runner are in sp-device/tests/eject.rs; left for sp-cli: the spawned-CLI refusals and --help |
| tests/cli-export-fdx.test.ts | 20 | sp-cli | partial | the renderer half (its script's FDX) is in sp-fdx/tests/fdx.rs; the `formats` block goes with export-formats to sp-engine; the export command and CLI blocks are left |
| tests/cli-export.test.ts | 79 | sp-cli | todo |  |
| tests/cli-kfx.test.ts | 24 | sp-cli | todo |  |
| tests/cli-reveal.test.ts | 21 | sp-cli | partial | the handler and the real opener are in sp-device/tests/reveal.rs; left for sp-cli: the spawned-CLI refusals and --help |
| tests/cli-routes.test.ts | 82 | sp-cli | todo |  |
| tests/cli-settings.test.ts | 26 | sp-cli | todo |  |
| tests/cli-update.test.ts | 17 | sp-cli | todo |  |
| tests/cli-verb-flags.test.ts | 7 | sp-cli | todo |  |
| tests/cli.test.ts | 78 | sp-cli | todo |  |
| tests/color-contrast.ts | 0 | xtask | todo | test helper: WCAG contrast |
| tests/conversion-settings.test.ts | 18 | sp-engine | todo |  |
| tests/convert.test.ts | 58 | sp-engine | todo |  |
| tests/cover.test.ts | 10 | sp-fountain | done | sp-fountain/tests/cover.rs (10); `Cover.index` stands for the TypeScript's token identity |
| tests/css-rules.test.ts | 11 | sp-epub | todo |  |
| tests/css-rules.ts | 0 | sp-epub | todo | test helper (selector-exact rule extractor) |
| tests/cue-text.test.ts | 4 | sp-parser | todo |  |
| tests/desktop-bundle-step.test.ts | 7 | xtask | todo | CI workflow steps |
| tests/desktop-layout-measured.test.ts | 16 | sp-app | todo | measured in headless Chrome today; a native toolkit measures its own layout |
| tests/desktop-layout.test.ts | 26 | sp-app | todo |  |
| tests/desktop-notes.test.ts | 14 | xtask | todo |  |
| tests/desktop-shell.test.ts | 67 | sp-app | todo |  |
| tests/desktop-tokens.test.ts | 9 | sp-app | todo | brand tokens into the window theme |
| tests/desktop-ui.test.ts | 425 | sp-app | todo |  |
| tests/desktop-update-install.test.ts | 20 | sp-app | todo | updater handover |
| tests/device-classify.test.ts | 13 | sp-device | done | sp-device/tests/device_classify.rs |
| tests/device-kindle.test.ts | 8 | sp-device | done | sp-device/tests/device_kindle.rs |
| tests/device-list.test.ts | 6 | sp-device | done | sp-device/tests/device_list.rs (with mtp-kindle's three listing checks) |
| tests/device-remarkable.test.ts | 8 | sp-device | done | sp-device/tests/device_remarkable.rs, through the real ureq client against a local stand-in |
| tests/device-requests-ui.test.ts | 11 | sp-app | todo |  |
| tests/device-requests-worker.test.ts | 24 | (services/) | todo | tests the Cloudflare Worker in services/device-requests/, which is a web service, not app code: decision for the owner whether it stays JavaScript |
| tests/device-transfer.test.ts | 9 | sp-device | done | sp-device/tests/device_transfer.rs |
| tests/device-volumes.test.ts | 9 | sp-device | done | sp-device/tests/device_volumes.rs |
| tests/device-watch.test.ts | 9 | sp-app | todo |  |
| tests/epub-images.test.ts | 23 | sp-epub | todo |  |
| tests/epub.test.ts | 117 | sp-epub | todo | the token half of "PRIMARY_SLUG stays in lockstep with fountain-js" is in sp-fountain/tests/slug.rs; the XHTML half and the classify.ts pin (import `sp_fountain::PRIMARY_SLUG_SOURCE`) stay here |
| tests/export-artifact.test.ts | 22 | sp-engine | todo |  |
| tests/export-calibre.test.ts | 8 | sp-engine | todo | isolate CALIBRE_CONFIG_DIRECTORY |
| tests/export-formats.test.ts | 6 | sp-engine | todo |  |
| tests/export-freshness.test.ts | 5 | sp-engine | todo |  |
| tests/export-kfx-setup.test.ts | 13 | sp-engine | todo |  |
| tests/export-kfx.test.ts | 54 | sp-engine | todo | isolate CALIBRE_CONFIG_DIRECTORY |
| tests/extract.test.ts | 103 | sp-parser | todo | reads pdf.js items: run against oracle pages.json until sp-pdf lands |
| tests/fdx-integration.test.ts | 6 | sp-fdx | done | sp-fdx/tests/fdx_integration.rs; reads the oracle's tokens.json and script.fountain where the TypeScript converts the PDF on the spot, until sp-parser and sp-fountain land |
| tests/fdx.test.ts | 38 | sp-fdx | done | sp-fdx/tests/fdx.rs, every case's Fountain kept as written; its tokens are fountain-js's, recorded in sp-fdx/tests/data/ until sp-fountain's tokenizer lands (TODO(sp-fountain) there). The lone-surrogate case becomes "cannot reach the writer": a Rust string cannot hold one. sp-fdx/tests/against_typescript.rs adds byte parity with the TypeScript on every recorded case plus 36 corners |
| tests/field-station.test.ts | 2 | sp-engine | todo |  |
| tests/first-page.test.ts | 10 | sp-parser | todo |  |
| tests/fixture-stability.test.ts | 1 | xtask | todo | fixtures stay Python-generated (tools/make-fixture.py) |
| tests/images-fixture.test.ts | 24 | sp-parser | todo |  |
| tests/images.test.ts | 68 | sp-parser | todo |  |
| tests/isolate-app-settings.ts | 0 | sp-engine | todo | test preload: no test reaches the real settings file |
| tests/isolate-calibre-config.ts | 0 | sp-engine | todo | test preload: Calibre only ever sees a scratch copy |
| tests/jpx-fixture.test.ts | 11 | sp-pdf | todo | JPEG2000 picture decodes pixel for pixel |
| tests/kfx-snippet-stubs.ts | 0 | sp-engine | todo | test helper: runs the KFX plugin install snippet under python3 |
| tests/library-images.test.ts | 55 | sp-engine | todo |  |
| tests/library.test.ts | 45 | sp-engine | todo |  |
| tests/mobi-picture-probe.test.ts | 7 | xtask | todo |  |
| tests/mobi.test.ts | 65 | sp-mobi | todo |  |
| tests/no-em-dash.test.ts | 3 | xtask | todo | repo-wide copy rule; must cover the Rust sources |
| tests/notes.test.ts | 49 | sp-fountain | done | sp-fountain/tests/notes.rs (49) |
| tests/options.test.ts | 45 | sp-model + sp-epub + sp-fountain | partial | resolveFormatOptions and keepSpeechesWhole blocks ported (sp-model/tests/options.rs); the serializer blocks (rejoin 2, contdMode 5, showPageMarkers 4, dualDialogue 2) ported (sp-fountain/tests/options.rs, 13); the CSS blocks go to sp-epub, convertFountain to sp-engine |
| tests/page-show.test.ts | 22 | sp-app | todo |  |
| tests/page-timing.test.ts | 16 | sp-app | todo |  |
| tests/parser-pipeline.test.ts | 47 | sp-parser | todo |  |
| tests/parser.test.ts | 137 | sp-parser | todo |  |
| tests/pdfjs-wasm.test.ts | 11 | sp-pdf | todo | pdf.js unpacks its JPEG2000/JBIG2 decoders to a temp folder; the Rust decoders are compiled in, so most of this retires: keep the "JPEG2000 decodes" half |
| tests/png.test.ts | 24 | sp-parser | todo | src/parser/png.ts; IDAT is zlib level 9, see the note below |
| tests/presets.test.ts | 7 | sp-model | done | sp-model/tests/presets.rs; the aliasing test has no Rust form (presets are returned by value) |
| tests/release-app-step.test.ts | 23 | xtask | todo |  |
| tests/release-artifacts.test.ts | 82 | xtask | todo |  |
| tests/release-notes-tool.test.ts | 26 | xtask | todo |  |
| tests/release-notes.test.ts | 6 | xtask | todo |  |
| tests/release-version-gate.test.ts | 6 | xtask | todo |  |
| tests/remove-pdfjs-decoders.ts | 0 | sp-pdf | todo | test preload; retires with the pdf.js decoder folder |
| tests/replace-file.test.ts | 11 | sp-fsutil | done | sp-fsutil/tests/replace_file.rs. src/atomic-write.ts has no test file of its own (its checks sit in library-images and desktop-shell); sp-fsutil/tests/atomic_write.rs pins them where the module now is |
| tests/require-release-notes.test.ts | 22 | xtask | todo | the git hook |
| tests/review-screens.test.ts | 12 | xtask | todo |  |
| tests/route-facts.test.ts | 32 | sp-engine | todo |  |
| tests/routes.test.ts | 82 | sp-engine | todo |  |
| tests/send-device.test.ts | 34 | sp-engine | todo |  |
| tests/send-panel-ui.test.ts | 125 | sp-app | todo |  |
| tests/send-routes-ui.test.ts | 71 | sp-app | todo |  |
| tests/serialize.test.ts | 64 | sp-fountain | done | sp-fountain/tests/serialize.rs (64); "a parsed mini-slug reaches the EPUB" keeps its token half here, its `<p class="mini-slug">` half is sp-epub's |
| tests/settings-app-defaults.test.ts | 18 | sp-engine | todo |  |
| tests/sidecar-targets.test.ts | 26 | xtask | todo | no sidecar in the Rust app: retires with build-sidecar; decision for the owner |
| tests/sidecar.test.ts | 10 | sp-engine | todo | the per-script settings sidecar (<script>.screepub.json), not the engine sidecar binary |
| tests/signature-box.ts | 0 | sp-app | todo | test helper: minisign boxes for the updater |
| tests/site.test.ts | 14 | xtask | todo | site/ stays HTML; the check moves |
| tests/smoke-bundle.test.ts | 35 | xtask | todo |  |
| tests/smoke-cli.test.ts | 21 | xtask | todo |  |
| tests/tap-scripts.test.ts | 15 | xtask | todo |  |
| tests/temp-hygiene.test.ts | 14 | xtask | todo | must cover the Rust tests' temp folders |
| tests/tool-temp-folders.test.ts | 4 | xtask | todo |  |
| tests/torture-coverage.test.ts | 6 | xtask | todo | tools/torture-manifest.json |
| tests/torture-layout.test.ts | 4 | xtask | todo | make-fixture.py --emit-layout |
| tests/torture-markup.test.ts | 10 | xtask | todo |  |
| tests/torture-pictures.test.ts | 14 | sp-parser | todo |  |
| tests/torture.test.ts | 23 | sp-parser | todo |  |
| tests/update-compare-module.test.ts | 9 | sp-app | todo | the window's copy of update/compare; one Rust copy serves both |
| tests/update-compare.test.ts | 34 | sp-engine | todo |  |
| tests/update-signature.test.ts | 14 | xtask | todo |  |
| tests/verify-signing.test.ts | 32 | xtask | todo |  |
| tests/weekly-toolchain.test.ts | 5 | xtask | todo |  |
| tests/workflow-shell.test.ts | 3 | xtask | todo |  |
| tests/workflows.test.ts | 3 | xtask | todo |  |

### Branch mtp-kindle (BETA, not shipped)

Merged into the port by sp-device's helper on 2026-10-11 and kept off for
users until hardware day (see Decisions). These files exist only on branch
`mtp-kindle`, so they are not in the count above.

| TypeScript or Rust | tests | Rust owner | status | notes |
|---|---:|---|---|---|
| native/mtp-helper (cargo tests) | 47 | sp-mtp | done | moved to rust/crates/sp-mtp unchanged but for the crate name; 40 unit tests and tests/virtual_device.rs (7, mtp-rs's in-memory device) |
| tests/device-mtp.test.ts | 23 | sp-device | done | sp-device/tests/device_mtp.rs; the listing block is in device_list.rs. The `.env.test` guard block retires: MTP is off unless SCREEPUB_MTP_HELPER names a helper, and every runner is injected |
| tests/camera-import.test.ts | 19 | sp-device | done | sp-device/tests/camera_import.rs; "the marker sits under the /dev/null config guard" moves to sp-engine, which owns the settings path the marker sits beside |
| tests/cli-mtp-send.test.ts | 13 | sp-device + sp-engine + sp-cli | partial | "send to an MTP Kindle" (8) is in sp-device/tests/commands.rs; the route list (3) goes to sp-engine's routes, the two end-to-end runs to sp-cli |
| tests/mtp-window-rules.test.ts | 7 | sp-app | todo | the window's rules for an MTP Kindle (Beta badge, no Eject, the copy note) |

### Test helpers: tests/helpers/

| TypeScript | Rust owner | status | notes |
|---|---|---|---|
| tests/helpers/boot-window.ts | sp-app | todo | boots desktop/ui in a fake DOM |
| tests/helpers/calibre-read.ts | sp-azw3 | todo | reads a book back through Calibre (isolated) |
| tests/helpers/fake-dom.ts | sp-app | todo |  |
| tests/helpers/fixture-epubs.ts | sp-epub | todo |  |
| tests/helpers/kf8-reader.ts | sp-azw3 | todo | structural KF8 reader |
| tests/helpers/layout-chrome.ts | sp-app | todo |  |
| tests/helpers/mini-xml.ts | sp-epub | todo | sp-fdx reads its XML back with roxmltree (dev-dependency) instead |
| tests/helpers/mobi-records.ts | sp-mobi | todo |  |
| tests/helpers/palmdoc-decode.ts | sp-mobi | todo |  |
| tests/helpers/stamped-azw3.ts | sp-azw3 | todo |  |

## Stage parity: the oracle

Each crate also proves its stage against `rust/oracle/<fixture>/` (committed)
and `rust/oracle-real/` (local only). Regenerate with `cargo xtask oracle`.

| Stage | Oracle file | Owner | status |
|---|---|---|---|
| pdf.js facts | pages.json | sp-pdf | todo |
| lines and pictures | lines.json | sp-parser (extract) | todo |
| parsed elements | elements.json | sp-parser | todo; the model reads and writes it back unchanged (sp-model/tests/oracle.rs) |
| .fountain | script.fountain | sp-fountain | done: byte for byte on all 5 converting fixtures and every converting real script (sp-fountain/tests/oracle.rs) |
| tokens | tokens.json | sp-fountain (tokenizer) | done: equal on all 5 converting fixtures and every converting real script (sp-fountain/tests/oracle.rs); plus 44 hand cases recorded from fountain-js (tests/tokenizer.rs) and an ignored differential fuzz test against fountain-js under bun (tests/tokenizer_fuzz.rs) |
| EPUB | book.epub | sp-epub | todo |
| preview | book.preview.html | sp-epub | todo |
| MOBI | book.mobi | sp-mobi | todo |
| AZW3 | book.azw3 | sp-azw3 | todo |
| FDX | book.fdx | sp-fdx | done: sp-fdx/tests/oracle.rs, byte for byte, 5 of 5 committed and every real script dumped (5 of 5 on 2026-10-11) |
| pictures | images/ | sp-parser | todo |
| --json answer | convert.json | sp-cli | todo |

## Window feature inventory

Filled in by the UI helper before any screen is built: every screen, flow,
keyboard path and screen-reader behavior in desktop/ui, one row each, each
mapped to the sp-app screen that reproduces it.

| Feature (desktop/ui) | sp-app screen | keyboard | screen reader | status |
|---|---|---|---|---|
| _to be inventoried_ | | | | |

## Decisions

Differences the port keeps on purpose, and questions for the owner. Each
names the check it affects.

- **PNG bytes (decided 2026-10-11).** Pictures compare by DECODED PIXELS
  (dimensions, colour type and every pixel), not by compressed bytes. The
  Rust side uses its own deflater; reproducing node:zlib's exact stream buys
  nothing a reader can see. Everything else in the EPUB still compares entry
  by entry, and every other file byte for byte.
- **The engine sidecar (decided 2026-10-11).** Retired: the Rust app links
  sp-engine directly. The CLI build checks those files also hold move to
  the release-pipeline crate (the `screepub` CLI is still built and
  smoke-tested per platform).
- **fountain-js state between parses (decided 2026-10-11): a TypeScript bug
  the port fixes.** fountain-js 1.2.4 keeps two module statics it never
  resets: `Lexer.lastLineWasDualDialogue` and the `lastIndex` of its global
  `blank_lines` rule. So in one process the same text can tokenize
  differently the second time: a script whose FIRST speech is a `NAME ^`
  right column makes the next parse wrap its LAST speech in a dual exchange
  (as a left column with no partner, which the renderers then drop or
  mis-set). The Rust tokenizer (`sp_fountain::tokenizer::tokenize`) starts
  every parse fresh, answering as a clean process's first parse does; within
  one parse the state still moves exactly as fountain-js moves it. No
  committed fixture or real script triggers it (the oracle, which dumps every
  fixture in one bun process, matches token for token). Checked by
  `a_script_opening_on_a_right_column_tokenizes_the_same_twice` and
  `a_parse_never_depends_on_the_one_before` (sp-fountain/tests/tokenizer.rs);
  the fuzz and case tests reset fountain-js to a first parse before each
  input so they compare the same thing.
- **`[[fmt: ...]]` words that name a JavaScript object property (open
  question for the owner).** notes.ts looks a word up with
  `FAMILY_CLASS[word]`, a plain-object index, so a hand-written
  `[[fmt: toString]]` (or `constructor`, `valueOf`, ...) finds the
  inherited function and `fmtClasses` appends its source text (checked under
  bun: `" function toString() {\n    [native code]\n}"`) to the class
  attribute. The
  Rust `fmt_classes` knows only the seven real words and ignores the rest,
  which is what the comment above FAMILY_CLASS says the code means to do.
  Screepub never writes such a note and no fixture or real script has one.
  Affects sp-epub's class attributes only; suggested fix on the TypeScript
  side is `Object.hasOwn`.
- **fountain-js's one throw is an `Err`.** A dialogue block whose first
  line is a lyric (`@BOB\n~la la`) makes fountain-js read
  `previousToken.type` of nothing and throw. `tokenize` returns
  `Err(FountainError)` for exactly that input, with the message bun prints
  (issue link included), so a caller can fail the way the TypeScript fails.
- **services/device-requests (decided 2026-10-11).** Ported to Rust
  (workers-rs, compiled to wasm) in a late wave, with its tests; the owner
  asked for no code but Rust. It stays deployed as is until the Rust worker
  answers the same two forms identically.
- **fountain-js carries state between parses (found 2026-10-11, open, for
  sp-fountain and the owner).** `Lexer.lastLineWasDualDialogue` is static
  and never reset. When a script's FIRST speech is the right half of a dual
  exchange (`NAME ^`), the NEXT parse in the same process opens a dual
  exchange around its LAST speech, and that speech is lost: the TypeScript
  `fountainToFdx` of such a script differs between its first and second
  call, the second missing the final speech. One conversion parses the same
  .fountain several times (src/convert.ts twice, fdx.ts, notes.ts), so books
  of such a script can disagree with each other. No committed fixture or
  real script triggers it. A pure Rust tokenizer will not reproduce it, so a
  script that does would show as a parity difference that is the
  TypeScript's bug. sp-fdx's recorded test tokens are made from a clean
  state for this reason (sp-fdx/tests/data/record-tokens.ts).

- **Device children are killed with SIGKILL (sp-device, 2026-10-11).** Bun's
  `proc.kill()` sends SIGTERM to an eject, MTP helper or launchctl step that
  outlived its timeout; std's `Child::kill` sends SIGKILL, and sending
  SIGTERM needs an unsafe libc call the workspace forbids. Either stops the
  child; the answer (`code: null`, "it took too long to answer") is the same.
  Checked by sp-device/tests/eject.rs.
- **The reMarkable client has no TLS (sp-device, 2026-10-11).** The tablet's
  USB web interface is plain HTTP on 10.11.99.1, so ureq is built with no TLS
  feature at all: pure Rust, no ring, no C. A crate that needs HTTPS (the
  update check) adds its own pure-Rust TLS. Bun's two network failures keep
  their words: "Unable to connect. Is the computer able to access the url?"
  and "The operation timed out." (device_remarkable.rs). Any other transport
  failure also reads as the first; Bun has a few rarer sentences.
- **MTP stays off (sp-device, 2026-10-11).** `sp_device::mtp::MTP_SHIPPED` is
  false. Until hardware day the engine looks for MTP Kindles only when
  SCREEPUB_MTP_HELPER names a helper, which is how hardware day turns it on
  and why no user meets it. That replaces mtp-kindle's `.env.test` guard
  paths. A dev checkout's helper is now `rust/target/release/screepub-mtp`.
  The helper's release profile (strip, lto, one codegen unit) was a
  per-crate profile and is not carried into the workspace: the packaging
  wave decides the release profile for every binary.
- **Where the camera-import marker lives (sp-device, 2026-10-11).** sp-device
  takes the config folder (`camera_import_marker(config_dir)`,
  `CameraImportDeps::host(config_dir)`); sp-engine supplies the folder its
  app-settings path names, so SCREEPUB_CONFIG_DIR still moves it.
- **A copy from a folder (sp-fsutil, 2026-10-11).** Bun's copyFileSync on a
  folder says `ENOTSUP` on a Mac; std's copy refuses it before the system
  call and sp-fsutil reports `EINVAL: invalid argument`. Both fail the send
  and leave the reader's old copy; `send` refuses a folder as `unreadable`
  before it gets that far, so no person reads either.
