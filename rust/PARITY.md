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
| tests/cli-devices.test.ts | 32 | sp-cli | todo |  |
| tests/cli-eject.test.ts | 24 | sp-cli | todo |  |
| tests/cli-export-fdx.test.ts | 20 | sp-cli | todo |  |
| tests/cli-export.test.ts | 79 | sp-cli | todo |  |
| tests/cli-kfx.test.ts | 24 | sp-cli | todo |  |
| tests/cli-reveal.test.ts | 21 | sp-cli | todo |  |
| tests/cli-routes.test.ts | 82 | sp-cli | todo |  |
| tests/cli-settings.test.ts | 26 | sp-cli | todo |  |
| tests/cli-update.test.ts | 17 | sp-cli | todo |  |
| tests/cli-verb-flags.test.ts | 7 | sp-cli | todo |  |
| tests/cli.test.ts | 78 | sp-cli | todo |  |
| tests/color-contrast.ts | 0 | xtask | todo | test helper: WCAG contrast |
| tests/conversion-settings.test.ts | 18 | sp-engine | todo |  |
| tests/convert.test.ts | 58 | sp-engine | todo |  |
| tests/cover.test.ts | 10 | sp-fountain | todo |  |
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
| tests/device-classify.test.ts | 13 | sp-engine | todo |  |
| tests/device-kindle.test.ts | 8 | sp-engine | todo |  |
| tests/device-list.test.ts | 6 | sp-engine | todo |  |
| tests/device-remarkable.test.ts | 8 | sp-engine | todo |  |
| tests/device-requests-ui.test.ts | 11 | sp-app | todo |  |
| tests/device-requests-worker.test.ts | 24 | (services/) | todo | tests the Cloudflare Worker in services/device-requests/, which is a web service, not app code: decision for the owner whether it stays JavaScript |
| tests/device-transfer.test.ts | 9 | sp-engine | todo |  |
| tests/device-volumes.test.ts | 9 | sp-engine | todo |  |
| tests/device-watch.test.ts | 9 | sp-app | todo |  |
| tests/epub-images.test.ts | 23 | sp-epub | todo |  |
| tests/epub.test.ts | 117 | sp-epub | todo |  |
| tests/export-artifact.test.ts | 22 | sp-engine | todo |  |
| tests/export-calibre.test.ts | 8 | sp-engine | todo | isolate CALIBRE_CONFIG_DIRECTORY |
| tests/export-formats.test.ts | 6 | sp-engine | todo |  |
| tests/export-freshness.test.ts | 5 | sp-engine | todo |  |
| tests/export-kfx-setup.test.ts | 13 | sp-engine | todo |  |
| tests/export-kfx.test.ts | 54 | sp-engine | todo | isolate CALIBRE_CONFIG_DIRECTORY |
| tests/extract.test.ts | 103 | sp-parser | todo | reads pdf.js items: run against oracle pages.json until sp-pdf lands |
| tests/fdx-integration.test.ts | 6 | sp-fdx | todo |  |
| tests/fdx.test.ts | 38 | sp-fdx | todo |  |
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
| tests/notes.test.ts | 49 | sp-fountain | todo |  |
| tests/options.test.ts | 45 | sp-model + sp-epub + sp-fountain | partial | resolveFormatOptions and keepSpeechesWhole blocks ported (sp-model/tests/options.rs); the CSS blocks go to sp-epub, the serializer blocks (rejoin, contdMode, showPageMarkers, dualDialogue) to sp-fountain, convertFountain to sp-engine |
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
| tests/replace-file.test.ts | 11 | sp-engine | todo |  |
| tests/require-release-notes.test.ts | 22 | xtask | todo | the git hook |
| tests/review-screens.test.ts | 12 | xtask | todo |  |
| tests/route-facts.test.ts | 32 | sp-engine | todo |  |
| tests/routes.test.ts | 82 | sp-engine | todo |  |
| tests/send-device.test.ts | 34 | sp-engine | todo |  |
| tests/send-panel-ui.test.ts | 125 | sp-app | todo |  |
| tests/send-routes-ui.test.ts | 71 | sp-app | todo |  |
| tests/serialize.test.ts | 64 | sp-fountain | todo |  |
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

### Test helpers: tests/helpers/

| TypeScript | Rust owner | status | notes |
|---|---|---|---|
| tests/helpers/boot-window.ts | sp-app | todo | boots desktop/ui in a fake DOM |
| tests/helpers/calibre-read.ts | sp-azw3 | todo | reads a book back through Calibre (isolated) |
| tests/helpers/fake-dom.ts | sp-app | todo |  |
| tests/helpers/fixture-epubs.ts | sp-epub | todo |  |
| tests/helpers/kf8-reader.ts | sp-azw3 | todo | structural KF8 reader |
| tests/helpers/layout-chrome.ts | sp-app | todo |  |
| tests/helpers/mini-xml.ts | sp-epub | todo |  |
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
| .fountain | script.fountain | sp-fountain | todo |
| tokens | tokens.json | sp-fountain (tokenizer) | todo; the model reads and writes it back unchanged |
| EPUB | book.epub | sp-epub | todo |
| preview | book.preview.html | sp-epub | todo |
| MOBI | book.mobi | sp-mobi | todo |
| AZW3 | book.azw3 | sp-azw3 | todo |
| FDX | book.fdx | sp-fdx | todo |
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
- **services/device-requests (decided 2026-10-11).** Ported to Rust
  (workers-rs, compiled to wasm) in a late wave, with its tests; the owner
  asked for no code but Rust. It stays deployed as is until the Rust worker
  answers the same two forms identically.
