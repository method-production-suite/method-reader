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
| tests/app-settings.test.ts | 31 | sp-engine | done | sp-engine/tests/app_settings.rs (28); the bun preload checks have their Rust form in tests/guards.rs (the engine takes its environment as a value, so the guard is what tests hand it) |
| tests/artwork.test.ts | 44 | sp-parser | done | sp-parser/tests/artwork.rs (43); retired: the Bun.build bundle check that artwork.ts never reaches pdf.js (sp-parser has no PDF dependency at all) |
| tests/azw3-calibre.test.ts | 0 | sp-azw3 | done | sp-azw3/tests/readers.rs (calibres_reader_opens_our_azw3); run only through a Sandbox that hands Calibre a scratch copy of its settings, and check the real folder is unchanged afterwards |
| tests/azw3-chunk.test.ts | 8 | sp-azw3 | done | sp-azw3/tests/chunk.rs |
| tests/azw3-compare.test.ts | 0 | sp-azw3 | done | sp-azw3/tests/readers.rs (ours_beside_calibres_azw3); run only through a Sandbox that hands Calibre a scratch copy of its settings, and check the real folder is unchanged afterwards |
| tests/azw3-corpus.test.ts | 0 | sp-azw3 | done | sp-azw3/tests/readers.rs, over rust/oracle-real's EPUBs; counts only |
| tests/azw3-epub.test.ts | 3 | sp-azw3 | done | sp-azw3/tests/parts.rs, on the oracle's EPUBs |
| tests/azw3-index.test.ts | 7 | sp-azw3 | done | sp-azw3/tests/parts.rs |
| tests/azw3-kindleunpack.test.ts | 0 | sp-azw3 | done | sp-azw3/tests/readers.rs; runs where SCREEPUB_KINDLEUNPACK names KindleUnpack |
| tests/azw3-palmdoc.test.ts | 5 | sp-azw3 | done | sp-azw3/tests/parts.rs |
| tests/azw3-revision.test.ts | 8 | sp-azw3 + sp-engine | done | the stamp and its reader: sp-azw3/tests/azw3.rs; the five staleness-rule tests (rungPlan, freshKindleArtifact) in sp-engine/tests/azw3_revision.rs (6: the five, plus the stand-in stamp read back through sp_azw3::azw3_revision) |
| tests/azw3-tbs.test.ts | 4 | sp-azw3 | done | sp-azw3/tests/parts.rs |
| tests/azw3-xml.test.ts | 6 | sp-azw3 | done | sp-azw3/tests/parts.rs; the round trip reads the oracle's EPUBs instead of a convertFountain |
| tests/azw3.test.ts | 9 | sp-azw3 | done | sp-azw3/tests/azw3.rs |
| tests/beta-marks.test.ts | 10 | sp-cli + sp-app | partial | CLI help: sp-cli/tests/beta_marks.rs (6); the 3 badge tests are sp-app's; the 2 README/site checks read documents, not code, and move to the xtask with no-em-dash and site |
| tests/boilerplate.test.ts | 16 | sp-parser | done | sp-parser/tests/boilerplate.rs |
| tests/brand-components.test.ts | 6 | xtask | todo | brand/ stays HTML; the check moves |
| tests/brand-tokens.test.ts | 9 | xtask | todo |  |
| tests/build-app-bundle.test.ts | 72 | sp-release + xtask | done | one implementation, sp-release's (names.rs, mac.rs, linux.rs, nsis.rs, archive.rs; sp-release/tests/pipeline.rs and compat.rs); `cargo xtask build-app-bundle` keeps the Tauri path until cutover and drives sp-release for `--app native`: xtask/tests/build_app_bundle.rs (74) and native_bundle.rs (18) run against it. The Tauri-bundler checks (overlay, discovered artifact, magic) retire with tauri-bundler |
| tests/build-cli-e2e.test.ts | 4 | xtask | done | xtask/tests/build_cli_e2e.rs (2): opt-in (SCREEPUB_REAL_CLI_BUILD=1, the Mac half also SCREEPUB_REAL_CODESIGN=1), host target and Mac only, since each target builds on its own runner |
| tests/build-cli.test.ts | 83 | xtask | done | xtask/tests/build_cli.rs (89) |
| tests/build-sidecar.test.ts | 28 | xtask | retired | the Rust app links the engine, so there is no sidecar (Decisions); what survives (the Rust triples, now the CLI's release targets) is xtask/tests/cli_targets.rs (6); `build-sidecar` answers "retired" |
| tests/build-update-manifest.test.ts | 20 | sp-release | done | sp-release/src/manifest.rs (shape pinned byte for byte, every refusal); xtask/tests/build_update_manifest.rs (21) runs the same checks through `cargo xtask build-update-manifest`, a thin caller |
| tests/bump-version.test.ts | 15 | xtask | done | xtask/tests/bump_version.rs (20), on scratch repos shaped before and after cutover |
| tests/bundle-archive.test.ts | 27 | xtask | done | xtask/tests/bundle_archive.rs (28): the READER of .deb and .rpm, on the same frozen archive bytes; the writers are sp-release's (archive.rs, linux.rs) |
| tests/calibre-config-isolation.test.ts | 7 | sp-engine | done | sp-engine/tests/guards.rs; not ported: the bare calibre-customize spawn on the guard path (it would start Calibre against a folder that is not a scratch copy, which this port's rules forbid), the guard VALUE is checked instead |
| tests/capture-chrome.test.ts | 5 | xtask | done | xtask/tests/capture_chrome.rs (7), against a fake Chrome; the last needs a real Chrome and skips without it |
| tests/capture.test.ts | 46 | xtask | done | xtask/tests/capture.rs (47) and capture_picture.rs (6); still drives desktop/ui/app.js, through a copy of its argument builders checked against app.js (Decisions) |
| tests/check-latest.test.ts | 17 | sp-release | done | sp-release/src/check_latest.rs, the one judgement (taken from xtask's port, the faithful one); xtask/tests/check_latest.rs (18) runs every case through `cargo xtask check-latest`'s thin wrapper, against a fake GitHub on 127.0.0.1 |
| tests/cli-app-defaults.test.ts | 8 | sp-cli | done | sp-cli/tests/cli_app_defaults.rs (8), through the CLI; the app-defaults reads themselves are sp-engine/tests/settings_app_defaults.rs |
| tests/cli-app-settings.test.ts | 56 | sp-cli | done | sp-cli/tests/cli_app_settings.rs (56): the handler in process with an injected path, platform and env (43), the verb through the CLI with HOME in the scratch folder (13) |
| tests/cli-azw3.test.ts | 4 | sp-cli | done | sp-cli/tests/cli_azw3.rs (4), read back with sp-azw3's tests/common/kf8_reader.rs |
| tests/cli-device-commands.test.ts | 35 | sp-cli | done | sp-cli/tests/cli_device_commands.rs (35); the no-hint checks are case-blind (the TypeScript's lower-case "did you mean" never matched the real hint) |
| tests/cli-devices.test.ts | 32 | sp-cli | done | the handlers (devicesCommand, selectDevice, sendCommand) are in sp-device/tests/commands.rs; sp-cli/tests/cli_devices.rs (15): the JSON error shape, verb dispatch and VERBS through the spawned binary, and the settings file a send remembers into; the non-Error-throw check has no Rust subject |
| tests/cli-eject.test.ts | 24 | sp-cli | done | the handler and the real runner are in sp-device/tests/eject.rs; the spawned-CLI refusals and --help in sp-cli/tests/cli_eject.rs (3) |
| tests/cli-export-fdx.test.ts | 20 | sp-cli | done | export command and CLI blocks (19) in sp-cli/tests/cli_export_fdx.rs; the renderer half is in sp-fdx/tests/fdx.rs; the `formats` block (1) is in sp-engine/tests/export_small.rs |
| tests/cli-export.test.ts | 79 | sp-cli | done | sp-cli/tests/cli_export.rs (79): handler checks against sp-engine's export_command with the TypeScript's seams and the real Books; spawned checks through the built screepub; the real KFX build runs only where the toolchain is ready, Calibre at a scratch copy; the relative --out checks run without the TypeScript's process.chdir guard (threads cannot change folder); the temp-and-rename source check reads sp-engine's commands/export.rs |
| tests/cli-kfx.test.ts | 24 | sp-cli | done | sp-cli/tests/cli_kfx.rs (24): handlers with a stand-in probe and installer; spawned kfx-status with Calibre at a scratch copy; kfx-install never runs (only --help is spawned), its refusal order read from sp-cli/src/verbs.rs; resolveCommand checks run through the CLI |
| tests/cli-reveal.test.ts | 21 | sp-cli | done | the handler and the real opener are in sp-device/tests/reveal.rs; the spawned-CLI refusals and --help in sp-cli/tests/cli_reveal.rs (6) |
| tests/cli-routes.test.ts | 82 | sp-cli | done | sp-cli/tests/cli_routes.rs (106: the refusal table and the three setup platforms expanded); handlers in process through sp-engine with a recording opener, spawned runs with stand-in open/xdg-open/defaults and a local stand-in tablet; the never-settling folder-opener race has no Rust subject (the opener is synchronous): its order half is kept; the cli.ts source pin reads verbs.rs |
| tests/cli-settings.test.ts | 26 | sp-cli | done | sp-cli/tests/cli_settings.rs (26): the handler in process (19), the verb through the CLI (7); "no appSettingsPath given" runs through the CLI, because the Rust handler takes the path as a required field and the CLI resolves it |
| tests/cli-update.test.ts | 17 | sp-cli | done | sp-cli/tests/cli_update.rs (17); resolveCommand/VERBS checks run through the CLI; the offline check reads sp-engine/src/update/ and verbs.rs's update arms |
| tests/cli-verb-flags.test.ts | 7 | sp-cli | done | sp-cli/tests/verb_flags.rs (7; the per-verb loop is one matrix test), the table checked against verbs.rs's VERBS and VERB_SPECS; cargo xtask parity runs every verb against every foreign flag against the TypeScript |
| tests/cli.test.ts | 78 | sp-cli | done | sp-cli/tests/cli.rs (93: the five-output-flag loop runs once per flag); the error-mapping and unwritable tests run on sp_engine::errors; retired: "--debug lets pdf.js warnings through" (no pdf.js, nothing warns; PARITY Decisions). The password test checks that a path containing "password" stays `unreadable`; the typed half holds by construction (ConvertError::Password), with no encrypted fixture |
| tests/color-contrast.ts | 0 | xtask | todo | test helper: WCAG contrast |
| tests/conversion-settings.test.ts | 18 | sp-cli | done | sp-cli/tests/conversion_settings.rs (18), through the CLI; "a sidecar write that fails" uses the debug binary's SCREEPUB_TEST_FAIL_PIN (compiled out of release builds) where the TypeScript patched fs.writeFileSync with a bun --preload |
| tests/convert.test.ts | 58 | sp-engine | done | sp-engine/tests/convert.rs (58), in process on tests/fixtures through sp-pdf. The resolveImageData/encodePng spies become a count of picture fetches through `convert_facts_with` (the facts sp-pdf reads first carry no pixels, so no fetch means nothing encoded). The two real-script blocks read root fixtures/ (the main checkout's from a worktree) and skip without it, counts only |
| tests/cover.test.ts | 10 | sp-fountain | done | sp-fountain/tests/cover.rs (10); `Cover.index` stands for the TypeScript's token identity |
| tests/css-rules.test.ts | 11 | sp-epub | done | sp-epub/tests/css_rules.rs (11) |
| tests/css-rules.ts | 0 | sp-epub | done | test helper (selector-exact rule extractor): `rule_for`, `each_rule` in sp-epub/tests/common/mod.rs |
| tests/cue-text.test.ts | 4 | sp-parser | done | sp-parser/tests/cue_text.rs (the 17 table rows as 17 tests) |
| tests/desktop-bundle-step.test.ts | 7 | xtask | todo | CI workflow steps |
| tests/desktop-layout-measured.test.ts | 16 | sp-app | partial | geometry in m/layout.rs; drawn rects measured in behaviors.rs (C3, L rows); see "Window test files" |
| tests/desktop-layout.test.ts | 26 | sp-app | partial | see "Window test files" |
| tests/desktop-notes.test.ts | 14 | xtask | done | xtask/tests/desktop_notes.rs (13): the generator writes notes.js and sp-app's notes_release.rs; the window's half is sp-app's (Window test files) |
| tests/desktop-shell.test.ts | 67 | sp-app | partial | Tauri pins retire with Tauri; see "Window test files" |
| tests/desktop-tokens.test.ts | 9 | sp-app | done | xtask/tests/desktop_tokens.rs (11): the generator, plus sp-app's theme.rs held to brand/; skips the desktop/ui half once it is gone; and theme.rs, fonts.rs |
| tests/desktop-ui.test.ts | 425 | sp-app | partial | see "Window test files" |
| tests/desktop-update-install.test.ts | 20 | sp-app | partial | m/update_flow.rs; the run waits on the wave 3 transport |
| tests/device-classify.test.ts | 13 | sp-device | done | sp-device/tests/device_classify.rs |
| tests/device-kindle.test.ts | 8 | sp-device | done | sp-device/tests/device_kindle.rs |
| tests/device-list.test.ts | 6 | sp-device | done | sp-device/tests/device_list.rs (with mtp-kindle's three listing checks) |
| tests/device-remarkable.test.ts | 8 | sp-device | done | sp-device/tests/device_remarkable.rs, through the real ureq client against a local stand-in |
| tests/device-requests-ui.test.ts | 11 | sp-app | done | m/requests.rs, tests/screens.rs |
| tests/device-requests-worker.test.ts | 24 | sp-device-requests | done | sp-device-requests/tests/worker.rs (all 24) and tests/sqlite.rs (the SQL and the migration on real SQLite with the service's schema.sql) |
| tests/device-transfer.test.ts | 9 | sp-device | done | sp-device/tests/device_transfer.rs |
| tests/device-volumes.test.ts | 9 | sp-device | done | sp-device/tests/device_volumes.rs |
| tests/device-watch.test.ts | 9 | sp-app | done | m/device_watch.rs |
| tests/epub-images.test.ts | 23 | sp-epub | done | sp-epub/tests/epub_images.rs (26: the 23 `test`s and the 3 epubcheck `checked`s, which skip without epubcheck on PATH). Where the TypeScript converts images.pdf or torture.pdf it reads the oracle dump of that conversion |
| tests/epub.test.ts | 117 | sp-epub | done | sp-epub/tests/epub.rs (117, plus the XHTML half of serialize.test.ts's mini-slug seam); the classify.ts pin reads src/parser/classify.ts until sp-parser has its copy; the token half of "PRIMARY_SLUG stays in lockstep with fountain-js" is in sp-fountain/tests/slug.rs; the XHTML half and the classify.ts pin (import `sp_fountain::PRIMARY_SLUG_SOURCE`) stay here |
| tests/export-artifact.test.ts | 22 | sp-engine | done | sp-engine/tests/export_artifact.rs (22) pins the ladder against stand-in book makers (tests/common/books.rs); the checks that read a real KF8 (readKf8, sp-azw3's reader via #[path]) or MOBI (mobiPictures) back run against `Books` in sp-engine/tests/export_artifact_books.rs (7) |
| tests/export-calibre.test.ts | 8 | sp-engine | done | sp-engine/tests/export_calibre.rs (8, stand-in tools only); the two real-Calibre checks (toKepub's koboSpan, the dialogue column in an exploded AZW3) on the engine's own EPUB in sp-engine/tests/export_calibre_real.rs (2), only through common::calibre_env's scratch copy, each checking the real folder unchanged; a binary of their own so they never load the machine under export_calibre.rs's timeout check |
| tests/export-formats.test.ts | 6 | sp-engine | done | sp-engine/tests/export_small.rs |
| tests/export-freshness.test.ts | 5 | sp-engine | done | sp-engine/tests/export_small.rs |
| tests/export-kfx-setup.test.ts | 13 | sp-engine | done | sp-engine/tests/export_small.rs |
| tests/export-kfx.test.ts | 54 | sp-engine | done | sp-engine/tests/export_kfx.rs (46; the TypeScript's loops and PATH-shadow cases folded); Calibre only through common::calibre_env's scratch copy |
| tests/extract.test.ts | 83 | sp-parser | done | sp-parser/tests/extract.rs (83; the file has 83 tests, not 103); fixture PDFs read through the oracle's pages.json until sp-pdf lands. The real-script repair-count test checks nothing until the script it was written for is dumped (every current real dump repairs 0) |
| tests/fdx-integration.test.ts | 6 | sp-fdx | done | sp-fdx/tests/fdx_integration.rs; reads the oracle's tokens.json and script.fountain where the TypeScript converts the PDF on the spot, until sp-parser and sp-fountain land |
| tests/fdx.test.ts | 38 | sp-fdx | done | sp-fdx/tests/fdx.rs, every case's Fountain kept as written; its tokens are fountain-js's, recorded in sp-fdx/tests/data/ until sp-fountain's tokenizer lands (TODO(sp-fountain) there). The lone-surrogate case becomes "cannot reach the writer": a Rust string cannot hold one. sp-fdx/tests/against_typescript.rs adds byte parity with the TypeScript on every recorded case plus 36 corners |
| tests/field-station.test.ts | 2 | sp-engine | done | parser half in sp-parser/tests/field_station.rs (scenes, pages, cast, the site's scene walked over elements); the convertPdf title and the .fountain `= pg N` walk in sp-engine/tests/field_station.rs (one in-process conversion stands for the CLI run) |
| tests/first-page.test.ts | 10 | sp-parser + sp-cli | done | sp-parser/tests/first_page.rs (10); the --progress CLI test (stderr page events, exit codes, --json): sp-cli/tests/first_page.rs (1) |
| tests/fixture-stability.test.ts | 1 | xtask | done | xtask/tests/fixture.rs (stability_*, 6): `cargo xtask make-fixture` regenerates every committed fixture but torture.pdf byte for byte (Decisions) |
| tests/images-fixture.test.ts | 24 | sp-parser | done | sp-parser/tests/images_fixture.rs (22); retired: the two pdf.js objs.has() checks; moved: extractTitleMeta to sp-fountain; the pixel side reads images.pdf through sp-pdf (dev-dependency), so its three pixel checks compare the real decoded bytes |
| tests/images.test.ts | 68 | sp-parser | done | sp-parser/tests/images.rs (57); retired: the 10 resolveImageData store-race and timeout tests and the lying-buffer test (pixels arrive resolved, a slice cannot misstate its length) |
| tests/isolate-app-settings.ts | 0 | sp-engine | done | sp-engine/tests/common/mod.rs (guarded_env) and tests/guards.rs: every test hands the engine an environment carrying the guard, and no test file may use this process's own |
| tests/isolate-calibre-config.ts | 0 | sp-engine | done | sp-engine/tests/common/mod.rs (calibre_config_copy, calibre_env): a scratch COPY per test binary, caches left behind |
| tests/jpx-fixture.test.ts | 11 | sp-pdf | done | sp-pdf/tests/jpx.rs (4: reaches the book, the codestream's size, decoders available, the fixture's gradient pixel for pixel); retired (7): the decoder directory (4), the decoders failing to unpack (2), a decoder folder removed between documents (1): sp-pdf's decoders are compiled in, so there is no folder to guard |
| tests/kfx-snippet-stubs.ts | 0 | sp-engine | done | sp-engine/tests/common/kfx_stubs.rs |
| tests/library-images.test.ts | 55 | sp-engine | done | sp-engine/tests/library_images.rs (55); the PNG encoder it uses is a stored-deflate stand-in (tests/common/png.rs) until sp-parser's lands, which is the worst case the size bound must hold for |
| tests/library.test.ts | 45 | sp-engine + sp-cli | done | sp-engine/tests/library.rs (37, the half-written-file checks through fsx::testing's fault seam); the --library CLI block: sp-cli/tests/cli_library.rs (6) |
| tests/mobi-picture-probe.test.ts | 7 | xtask | done | the MOBI checks are sp-mobi/tests/probe.rs (byte-identical to the TypeScript's MOBI for the same pictures); the run-as-a-process and EPUB halves are xtask/tests/mobi_picture_probe.rs (6; epubcheck skips where it is not installed) |
| tests/mobi.test.ts | 65 | sp-mobi | done | sp-mobi/tests/mobi.rs, pictures.rs (images.pdf in the MOBI), mobi_calibre.rs (the two Calibre round trips, isolated); sources tokenized by sp-fountain (checked against fountain-js's recorded tokens); the EPUB's picture answers come from tests/data/mobi-sources.json until sp-epub lands |
| tests/no-em-dash.test.ts | 3 | xtask | partial | repo-wide copy rule; the window's half is rust/crates/sp-app/tests/no_em_dash.rs; the engine crates are left |
| tests/notes.test.ts | 49 | sp-fountain | done | sp-fountain/tests/notes.rs (49) |
| tests/options.test.ts | 45 | sp-model + sp-epub + sp-fountain + sp-engine | done | resolveFormatOptions and keepSpeechesWhole blocks ported (sp-model/tests/options.rs); the serializer blocks (rejoin 2, contdMode 5, showPageMarkers 4, dualDialogue 2) ported (sp-fountain/tests/options.rs, 13); the CSS, body and package blocks (screenplayCss 10, tokensToBody 2, buildEpub 1, cueAlignment 2) ported (sp-epub/tests/options.rs, 15); convertFountain (1) ported (sp-engine/tests/options.rs) |
| tests/page-show.test.ts | 22 | sp-app | done | m/page_show.rs, app/show.rs, tests/behaviors.rs |
| tests/page-timing.test.ts | 16 | sp-app | done | m/page_timing.rs |
| tests/parser-pipeline.test.ts | 47 | sp-parser + sp-fountain | done | sp-parser/tests/parser_pipeline.rs (45); the 2 tests and the toFountain halves of 3 in sp-fountain/tests/parser_pipeline.rs (5, sp-parser as dev-dependency) |
| tests/parser.test.ts | 137 | sp-parser | done | sp-parser/tests/parser.rs (137) |
| tests/pdfjs-wasm.test.ts | 11 | sp-pdf | done | the "JPEG2000 decodes" half is sp-pdf/tests/jpx.rs; the rest (decoder filenames, the wasmUrl string, removePdfjsWasmDir, exit hooks, the bun-run temp check) retires with pdf.js's decoder folder |
| tests/png.test.ts | 24 | sp-parser | done | sp-parser/tests/png.rs (24); IDAT is miniz_oxide level 9, compared by decoded pixels (Decisions) |
| tests/presets.test.ts | 7 | sp-model | done | sp-model/tests/presets.rs; the aliasing test has no Rust form (presets are returned by value) |
| tests/release-app-step.test.ts | 23 | xtask | todo |  |
| tests/release-artifacts.test.ts | 82 | xtask | todo |  |
| tests/release-notes-tool.test.ts | 26 | xtask | done | xtask/tests/release_notes_tool.rs (27), in throwaway repos |
| tests/release-notes.test.ts | 6 | xtask | done | xtask/tests/release_notes_copy.rs (6) |
| tests/release-version-gate.test.ts | 6 | xtask | todo |  |
| tests/remove-pdfjs-decoders.ts | 0 | sp-pdf | todo | test preload; retires with the pdf.js decoder folder |
| tests/replace-file.test.ts | 11 | sp-fsutil | done | sp-fsutil/tests/replace_file.rs. src/atomic-write.ts has no test file of its own (its checks sit in library-images and desktop-shell); sp-fsutil/tests/atomic_write.rs pins them where the module now is |
| tests/require-release-notes.test.ts | 22 | xtask | done | xtask/tests/release_hook.rs (24): `cargo xtask require-release-notes`, in a scratch repo |
| tests/review-screens.test.ts | 12 | xtask | done | xtask/tests/review_screens.rs (12), in scratch repos |
| tests/route-facts.test.ts | 32 | sp-engine | done | sp-engine/tests/route_facts.rs (29); the 3 checks of the DEFAULT device probe (listDevices with its options) wait for the sp-device branch: route_facts takes the device list from its caller |
| tests/routes.test.ts | 82 | sp-engine | done | sp-engine/tests/routes.rs (49; the TypeScript's generated per-case tests folded into loops) |
| tests/send-device.test.ts | 34 | sp-engine | todo | the sp-device branch |
| tests/send-panel-ui.test.ts | 125 | sp-app | partial | see "Window test files" |
| tests/send-routes-ui.test.ts | 71 | sp-app | partial | see "Window test files" |
| tests/serialize.test.ts | 64 | sp-fountain | done | sp-fountain/tests/serialize.rs (64); "a parsed mini-slug reaches the EPUB" keeps its token half here, its `<p class="mini-slug">` half is sp-epub's (sp-epub/tests/epub.rs, done) |
| tests/settings-app-defaults.test.ts | 18 | sp-engine | done | sp-engine/tests/settings_app_defaults.rs |
| tests/sidecar-targets.test.ts | 26 | xtask | retired | no sidecar in the Rust app (Decisions); the surviving target table is pinned by xtask/tests/cli_targets.rs |
| tests/sidecar.test.ts | 10 | sp-engine | done | sp-engine/tests/sidecar.rs (the per-script settings sidecar) |
| tests/signature-box.ts | 0 | sp-update + xtask | done | test helper: sp-update/tests/common/mod.rs (TestKey signs as tauri-cli does), xtask/tests/packaging_common/mod.rs (fake boxes and tauri-cli's published test key) |
| tests/site.test.ts | 14 | xtask | todo | site/ stays HTML; the check moves |
| tests/smoke-bundle.test.ts | 35 | xtask | done | xtask/tests/smoke_bundle.rs (38); the native window's bundles get the binary-and-version check only until sp-app has a headless mode (Decisions) |
| tests/smoke-cli.test.ts | 21 | xtask | done | xtask/tests/smoke_cli.rs (21) |
| tests/tap-scripts.test.ts | 15 | xtask | done | xtask/tests/tap_tools.rs (18): bump-tap and check-tap against a fake tap and a fake GitHub |
| tests/temp-hygiene.test.ts | 14 | xtask | todo | must cover the Rust tests' temp folders |
| tests/tool-temp-folders.test.ts | 4 | xtask | done | xtask/tests/tool_temp_folders.rs (4): smoke-cli twice, smoke-bundle, verify-signing |
| tests/torture-coverage.test.ts | 6 | xtask | done | xtask/tests/torture_coverage.rs (10), with `device-checklist` |
| tests/torture-layout.test.ts | 4 | xtask | done | xtask/tests/fixture.rs (emit_layout_*, atline_*: 4) |
| tests/torture-markup.test.ts | 10 | xtask | done | xtask/tests/fixture.rs (markup_*, wrap_*: 10) |
| tests/torture-pictures.test.ts | 14 | sp-parser + sp-engine | done | sp-parser/tests/torture_pictures.rs (7); the 5 EPUB and 2 MOBI checks in sp-engine/tests/torture_pictures.rs (7) |
| tests/torture.test.ts | 23 | sp-parser + sp-engine | done | sp-parser/tests/torture.rs (19); the 4 [[fmt]] note checks in sp-engine/tests/torture.rs (they read torture.pdf through sp-pdf and serialize with sp-fountain) |
| tests/update-compare-module.test.ts | 9 | sp-app | done | m/update_compare.rs; one Rust copy serves both |
| tests/update-compare.test.ts | 34 | sp-engine | done | sp-engine/tests/update_compare.rs |
| tests/update-signature.test.ts | 14 | sp-release | done | the readers are sp-release/src/boxes.rs and the signer sign.rs (sp-release/tests/compat.rs opens tauri-cli's own key with the pinned minisign); verify is sp-update/src/verify.rs; xtask/tests/update_signature.rs (17) runs the readers and the signer through `cargo xtask update-signature`'s module |
| tests/verify-signing.test.ts | 32 | xtask | done | xtask/tests/verify_signing.rs (34), on a fake runner |
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
| tests/cli-mtp-send.test.ts | 13 | sp-device + sp-engine + sp-cli | partial | "send to an MTP Kindle" (8) is in sp-device/tests/commands.rs; the route list (3) goes to sp-engine's routes; the two end-to-end runs are in sp-cli/tests/cli_mtp_send.rs (stand-in helper, launchctl and pkill; they found the CLI never wired the camera-import pause or its recovery, fixed in sp-cli) |
| tests/mtp-window-rules.test.ts | 7 | sp-app | done | tests/mtp_window_rules.rs (and send-panel-ui's "a Kindle over MTP"); MTP stays off in the engine |

### Test helpers: tests/helpers/

| TypeScript | Rust owner | status | notes |
|---|---|---|---|
| tests/helpers/boot-window.ts | sp-app | retired | egui_kittest drives the real window headless (tests/common/mod.rs) |
| tests/helpers/calibre-read.ts | sp-azw3 | done | sp-azw3/tests/common/calibre_read.rs, through sp-mobi/tests/common/calibre.rs's Sandbox (the Rust form of tests/isolate-calibre-config.ts for these two crates) |
| tests/helpers/fake-dom.ts | sp-app | retired | egui_kittest drives the real window headless (tests/common/mod.rs) |
| tests/helpers/fixture-epubs.ts | sp-epub | retired | the fixtures' EPUBs are rust/oracle/<fixture>/book.epub, which sp-epub/tests/oracle.rs proves the Rust renderer reproduces; the AZW3 tests read those |
| tests/helpers/kf8-reader.ts | sp-azw3 | done | sp-azw3/tests/common/kf8_reader.rs |
| tests/helpers/layout-chrome.ts | sp-app | retired | egui_kittest drives the real window headless (tests/common/mod.rs) |
| tests/helpers/mini-xml.ts | sp-epub | retired | sp-fdx reads its XML back with roxmltree (dev-dependency) instead |
| tests/helpers/mobi-records.ts | sp-mobi | done | sp-mobi/tests/common/mod.rs |
| tests/helpers/palmdoc-decode.ts | sp-azw3 | done | sp-azw3/tests/common/palmdoc_decode.rs (only the AZW3 is compressed) |
| tests/helpers/stamped-azw3.ts | sp-azw3 + sp-engine | done | sp-azw3/tests/azw3.rs's stamped_azw3; sp-engine/tests/common/books.rs's stamped_azw3, checked against sp_azw3::azw3_revision in sp-engine/tests/azw3_revision.rs |

## Stage parity: the oracle

Each crate also proves its stage against `rust/oracle/<fixture>/` (committed)
and `rust/oracle-real/` (local only). Regenerate with `cargo xtask oracle`.

| Stage | Oracle file | Owner | status |
|---|---|---|---|
| pdf.js facts | pages.json | sp-pdf | todo |
| lines and pictures | lines.json | sp-parser (extract) | todo |
| parsed elements | elements.json | sp-parser | todo; the model reads and writes it back unchanged (sp-model/tests/oracle.rs) |
| .fountain | script.fountain | sp-fountain | todo |
| tokens | tokens.json | sp-fountain (tokenizer) | done: equal on all 5 converting fixtures and every converting real script (sp-fountain/tests/oracle.rs); plus 44 hand cases recorded from fountain-js (tests/tokenizer.rs) and an ignored differential fuzz test against fountain-js under bun (tests/tokenizer_fuzz.rs) |
| EPUB | book.epub | sp-epub | todo |
| preview | book.preview.html | sp-epub | todo |
| MOBI | book.mobi | sp-mobi | todo |
| AZW3 | book.azw3 | sp-azw3 | todo |
| FDX | book.fdx | sp-fdx | todo |
| pictures | images/ | sp-parser | todo |
| --json answer | convert.json | sp-cli | done: `cargo xtask parity` runs both CLIs on every committed fixture and real script and compares the answer as values (the convert rows), with every book and the picture folder |

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
| W1 | Window title "Screepub"; opens at 1280 x 800 | tauri/tauri.conf.json | done |
| W2 | Size floor 640 x 480; resizable | tauri/tauri.conf.json | done |
| W3 | No title bar of its own on macOS (titleBarStyle Overlay, hidden title): the paper runs under the traffic lights | tauri/tauri.conf.json | done |
| W4 | Drag strip: a transparent band the height of the sheet's top padding (space-7) at the top of the PAGE, scrolling away with it, moves the window; never covers a control | ui/frame.js, ui/style.css .drag-strip | done |
| W5 | The gaps in the tab bar also move the window; a tab click is still a tab | ui/frame.js (data-tauri-drag-region on the tablist) | done |
| W6 | Identifier `com.darkwell.screepub`, product name Screepub, version 0.11.1 | tauri/tauri.conf.json | wave 3 |
| W7 | macOS minimum 13.0; Apple-silicon-only Mac build | tauri/tauri.conf.json, desktop/README.md | wave 3 |
| W8 | Bundle carries LICENSE and THIRD-PARTY-NOTICES.md as resources | tauri/tauri.conf.json | wave 3 |
| W9 | Linux launcher entry Categories=Office; no PDF file association; no path read from the command line | desktop/README.md | wave 3 |
| W10 | macOS default app menu (Tauri's `enable_macos_default_menu`, on by default): About, Hide, Quit (Cmd-Q), Edit menu (Undo, Cut, Copy, Paste, Select All, so Cmd-C/V work in the device-request field), Window menu | tauri/src/main.rs (no menu set) | done (muda: app, Edit and Window menus; Edit items passed on to egui, menu.rs test) |
| W11 | Boot check: the engine is asked `--version --json`; a failure puts the engine's sentence on its own alarm-coloured line above the stamp (the stamp stays) | ui/main.js, ui/frame.js engineFailed | done |
| W12 | Engine missing or unstartable: "could not find/start the Screepub engine (sidecar ...)" with the build hint | tauri/src/sidecar.rs | gap: no sidecar in Rust (direct calls); the line is shown for an engine call that cannot start at all |
| W13 | No Cancel during a conversion; quitting mid-call lets the engine finish (atomic writes) | tauri/src/sidecar.rs, desktop/README.md | done (no Cancel; a window closed mid-call waits for the in-process call to finish its writes: src/direct.rs in_flight, main.rs; direct.rs tests) |
| W14 | Paper background edge to edge; page padding: 100px binding on the left, 48px on the right; the content block capped at 1180 and centred in what is left | ui/style.css .page .sheet | done |
| W15 | The rail: two brads and one punched hole, fixed to the window (the paper scrolls under them) at 30.7%, 50%, 69.3% of the height, centred 44px from the left edge, size clamp(20px, 3.4%, 36px) | ui/style.css .rail, ui/frame.js | done |
| W16 | Brad drawn as brand/components/brad.html: cast shadow ellipse, edge ring, radial brass face (specular, highlight, brass, shadow, rim stops), bounce ellipse, specular ellipse, edge stroke | ui/frame.js DEFS | done |
| W17 | Punched hole: paper lip, ground disc, radial shade | ui/frame.js DEFS | done |
| W18 | Short window (height <= 560): one brad, centred | ui/style.css | done |
| W19 | Narrow window (width <= 720): binding 11%, right margin space-4, brads at 5.5% and 20px, tab gap space-3, buttons narrower | ui/style.css, ui/surfaces.css | done |
| W20 | Light and dark from the OS (prefers-color-scheme), live as the OS changes: seven core colours in style.css and the generated tokens.css | ui/style.css, ui/tokens.css | done |
| W21 | Fonts bundled, never fetched: Courier Prime 400/700 and Literata 400, latin and latin-ext subsets (six WOFF2) | ui/style.css, ui/fonts | done |
| W22 | Type scale (10/11/12/13/14/15/17/18/30/44 px), radii (2/3/5), motion (0.1 s press, 0.2 s state, ease-out), spacing scale (0.4 to 3.5 rem), measure 66ch | ui/tokens.css | done |
| W23 | Hidden means hidden: no control drawn hidden can be seen or clicked | ui/style.css [hidden] | done |
| W24 | Every box that prints a path, a title or an engine sentence wraps anywhere | ui/surfaces.css overflow-wrap | done |
| W25 | Reduce Motion from the OS turns off every transition and animation listed in the R/P/N rows | ui/main.js state.reducedMotion, CSS media queries | done |

### N. Frame and navigation

| ID | Feature | Source | Status |
|---|---|---|---|
| N1 | Three surfaces on a tab bar on the paper: Convert, Read, Settings (id `tune`) | ui/frame.js SURFACES | done |
| N2 | Tablist named "Screepub"; each tab a tab, selected state exposed, controls its panel; each panel a tabpanel labelled by its tab and focusable (tabindex 0) | ui/frame.js, ui/main.js | done |
| N3 | Read and Settings are ABSENT (not dimmed) until a script is open | ui/frame.js tabPresence | done |
| N4 | Roving tab stop: only the selected tab is a Tab stop | ui/frame.js tabPresence | done |
| N5 | Left/Right move between tabs, wrapping, skipping absent ones; Home/End to the ends; the moved-to tab is focused and selected | ui/frame.js keydown | done |
| N6 | A tab that goes away while selected sends the window to Convert | ui/frame.js enable | done |
| N7 | Tabs arriving: fade and rise 5px over 0.2 s, staggered 70 ms by position; only on the way in; none with Reduce Motion | ui/style.css tab-arrive | done |
| N8 | Tab look: Courier bold 11px caps, letter-spaced, muted; the open one ink with a 1.5px brass rule under it; hover ink | ui/style.css .tab | done |
| N9 | Rev stamp "rev 0.11.1" at the foot of the sheet, right; the version is the RELEASE notes' version, not the engine's | ui/frame.js revLabel | done |
| N10 | The stamp is a button that opens the release notes sheet | ui/frame.js, ui/main.js | done |
| N11 | Update label beside the stamp: hidden until the update flow has words; brass rule under it; inert moments carry aria-disabled and no pointer | ui/frame.js setUpdateLabel | done (tests/model_rows.rs) |
| N12 | If the update label hides while focused, the keyboard goes to the page's first stop | ui/main.js | done |
| N13 | Release notes as a modal sheet: Esc closes, focus trapped inside, the background inert, focus restored on close; "Close" button; dimmed backdrop; rises 16px over 0.2 s (none with Reduce Motion); width min(44rem, 88vw), max height 82vh, scrolls | ui/main.js, ui/style.css .sheet-over | done |
| N14 | Cmd-O (Mac) / Ctrl-O opens the file picker from any surface without changing surface; cancelling leaves the reader where they were | ui/main.js | done |
| N15 | A file dropped anywhere on the window converts on Convert; a file dragged over the window turns the well's border and icon brass | ui/main.js, ui/convert.js, ui/surfaces.css .well-targeted | done |
| N16 | Of several dropped paths, the first non-empty one converts | ui/convert.js droppedPath | done |
| N17 | A drop while converting is ignored (known gap, kept) | desktop/README.md | done |
| N18 | Where the keyboard lands after any native dialog closes or a redraw throws the focused control away: first control in the showing pane, else the pane, else its tab, else the Convert tab; never a hidden, disabled or detached control | ui/focus.js | done |
| N19 | scriptChanged: tabs appear or go, the devices poll starts or stops, the heads-up resets, every surface redraws for the new book | ui/main.js | done |

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
| E21 | An answer that is not JSON: "the engine did not answer in JSON:\n" plus the first 300 characters and an ellipsis | ui/app.js RAW_SHOWN | | done (src/direct.rs answer_from: the last stdout line, else "the engine did not answer in JSON:" and its first 300 characters) |
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
| C3 | Convert's column is at most 820 wide in every state but the result | ui/surfaces.css #surface-convert | done |
| C4 | One-time question under the well: "Check for new versions once a day?" Turn on / No thanks; only where updates are possible and the question is unanswered; a stale click after the notes switch answered does not overwrite; the line removes itself and the keyboard is restored | ui/convert.js askLine | done |
| C5 | Library line: "Books are saved in ~/…." or "…, set by SCREEPUB_LIBRARY."; the home folder folded to ~ only when the path is genuinely under it (case-insensitive on Windows paths, root homes) | ui/convert.js libraryLine, shortenHome | done |
| C6 | Change… (folder picker at the current library; a cancel or the same folder sends nothing) and Reset (only when a folder was chosen); neither when SCREEPUB_LIBRARY is set | ui/convert.js libraryActions | done |
| C7 | After Change: "New books go here. Books already converted stay where they are." only when the library moved; a refusal or thrown call shows its sentence on the secondary line and leaves the line as it was; NO_LIBRARY_MESSAGE for an answer that breaks the contract | ui/convert.js libraryAfter, showsMovedLine | done (tests/model_rows.rs) |
| C8 | Change and Reset go quiet while one runs, back afterwards even on a throw | ui/convert.js buildLibrarySlot | done |
| C9 | A probe answer that arrives after the well was redrawn is dropped (generation guard); a malformed or failed probe draws no line | ui/convert.js probeLibrary | done |
| C10 | The same probe carries the two page switches into the window state | ui/convert.js probeLibrary | done |
| C11 | Working: "Converting" heading, "The pages of <file> reflow themselves, one scene at a time.", a 220px track with a brass fill (width eased 0.2 s; none with Reduce Motion), a read-out that is a polite live status, "Please stand by:" | ui/convert.js drawProgress | done |
| C12 | Read-out words: "starting up"; while the book is held by another turn the Settings page's words for that wait ("Waiting for the Kindle file to finish building…" and the rest, longest wait first; "Waiting for the settings to finish saving…"; "Waiting for the book to be free…"); then "reading the pages (N%)", "reading the pictures (N%)", "building the book (N%)" | ui/convert.js waitingLabel, nextProgress | done (tests/model_rows.rs) |
| C13 | The bar never goes back: the percent is the engine's whole-job figure, clamped, rounded, never falls; unknown stages and non-numbers ignored; the stage never walks back; a stage change at the same percent is news | ui/convert.js nextProgress | done |
| C14 | Converting a PDF converted earlier in this window takes its turn on the book it rewrites | ui/convert.js landed, inTurn | done |
| C15 | The keyboard lands on the working pane (it has no control) | ui/convert.js drawProgress | done |
| C16 | Result layout: the book's first page (400 wide, 3:4, hairline, radius 5, paper) and the side (up to 620), 32 apart, centred in the block; under an 880 block the side first and the page under it, 400 wide and centred | ui/surfaces.css .result | done |
| C17 | The first page is the book's own (engine preview), scrolled to its first scene less 1.5 lines, not a Tab stop, hidden from the screen reader | ui/result-page.js | done (Blitz, as R18) |
| C18 | "<Title> is ready" ("The book is ready" with no title); counts "19 pages, 11 scenes" (singulars; a missing count left out); each warning in parentheses | ui/convert.js readyLine, countsLine | done |
| C19 | The send panel (D rows) in a box: hairline, radius 5, panel fill | ui/surfaces.css .send-panel | done |
| C20 | Three quiet links under it: Read it, Settings, Convert another (back to the well without closing the book) | ui/convert.js paintResult, reset | done |
| C21 | At the foot of Other ways' Save a file: Show in Finder / Show in File Explorer / Show in folder (by platform), the book's path, and the engine's refusal under them when the reveal failed (built into this result, never a later one) | ui/convert.js revealLabel, revealNote | done |
| C22 | A last send's line and its Eject come back with the result for the same book; another book's line is dropped unless a reader still waits to be ejected | ui/convert.js drawResult | done |
| C23 | Refusal: "Smash cut to:", the heading for the code (table of ten), the engine's sentence, Convert anyway (brass, only for `not-screenplay`, converts with force), Back to one (outline), Report a bug (quiet) | ui/convert.js drawFailure, HEADINGS | done |
| C24 | The `--force` sentence is taken out of the engine's message only where Convert anyway is drawn; never down to nothing | ui/convert.js withoutCliRemedy | done |
| C25 | Unknown or missing code: "internal", "The engine did not answer"; no sentence: "The engine refused the file without saying why." | ui/convert.js failureFor | done |
| C26 | "<Title> is still open. This changed nothing about it." when a book was already open; the open book survives a refusal | ui/convert.js stillOpenNote | done (tests/model_rows.rs) |
| C27 | "Error code" label and the code in a chip | ui/convert.js | done |
| C28 | A conversion that could not run at all shows the refusal screen with code internal and the thrown sentence | ui/convert.js convertPath | done |
| C29 | "Convert it again" (Read's blank notice, Settings' fault, the panel's no-book line): converts the open script's source, or just goes to Convert when there is none or one is running | ui/convert.js convertAgain | done |
| C30 | Read's and Settings' Send: back to the result on Convert, redrawn only if the well or a refusal replaced it, keyboard on its first control | ui/main.js sendBook, ui/convert.js showResult | done |
| C31 | No screen prints a stray "null" | tests/desktop-ui.test.ts | done |

### P. The page that writes itself, and its sounds

| ID | Feature | Source | Status |
|---|---|---|---|
| P1 | Starts when the engine's page line arrives, unless Reduce Motion is on, "Animate the page" is off, or the line carries no text | ui/page-show.js canShow | done |
| P2 | Drawn in the result's own box: a typed page over where the result's page will be; the side says "Converting" and the file name | ui/page-show.js beginShow | done |
| P3 | The slim line under the page, a polite live status: "Starting up", "Reading page N of M" (never back a page), "Reading the pages (N%)" from an engine that names no page, "Reading the pictures", "Building the book" | ui/page-show.js readingLine | done (tests/model_rows.rs) |
| P4 | The page's whole text is in the window for a screen reader from the first moment; the typing itself is hidden from it | ui/page-show.js .page-show-words | done |
| P5 | Typing takes 4000 ms whatever the script, spread evenly over the characters (code points); empty paragraphs skipped | ui/page-timing.js | done |
| P6 | The wave: block i takes the book's form two blocks behind the typing, never before the book exists plus 120 ms a block (faster on long pages, a sweep never over 1200 ms) | ui/page-timing.js formAt | done |
| P7 | Each formed block glows brass for 600 ms; the typed page lifts away over 250 ms at the end | ui/page-timing.js, ui/page-show.js | done |
| P8 | Typed paragraphs matched to the book's blocks by their words (letters and digits, upper case; a prefix of at least half counts; the first match anywhere, then within 4); an unmatched paragraph only glows | ui/page-show.js alignBlocks | done (tests/model_rows.rs) |
| P9 | A click anywhere or any key ends it: before the book exists the whole page shows typed, and it ends the moment the book arrives | ui/page-show.js onPress | done |
| P10 | A refusal or another file cancels it | ui/page-show.js cancel | done |
| P11 | Key sounds at most one per 55 ms while typing; a carriage-return bell when a block finishes, at most one per 700 ms | ui/page-timing.js soundsDue | done |
| P12 | Key: 35 ms of white noise through a band-pass at 1800 to 3400 Hz (random each key), Q 1.4, gain 0.3, exponential fade; bell: 2093 Hz sine, gain 0.08, 0.5 s fade; made, not recorded | ui/typewriter.js | done |
| P13 | Sounds only when "Typewriter sounds" is on; a machine with no audio is quiet; closed when the show ends (typewriter.js has close() but nothing calls it, so in practice the output stays open for the window's life, as sp-app's does) | ui/convert.js startShow, ui/typewriter.js | done |

### R. Read

| ID | Feature | Source | Status |
|---|---|---|---|
| R1 | Head: the title (Courier bold caps), "by <Author>" when known, Show scenes / Hide scenes (names the click; expanded state exposed), Send (quiet) | ui/read.js draw, indexToggle | done |
| R2 | Two columns: a 218 index slot, then the script up to 706, 32 apart; the index opens into its own slot, so the script never moves | ui/surfaces.css .reader | done |
| R3 | Under an 880 block: the index is a short strip above the script (at most 6.5rem, entries inline), absent when shut | ui/surfaces.css | done |
| R4 | The script's frame: 68% of the window's height (58% narrow, 62% short), at least 300, paper with a hairline and radius 5 | ui/surfaces.css .script-frame | done |
| R5 | The stage around the script is the Tab stop, a group named "<Title>, the script", with a focus ring | ui/read.js | done |
| R6 | Keys on the stage: Up/Down 60px, PageUp/PageDown/Space 90% of the frame (at least 120), Home/End to the ends | ui/read.js scrollStep | done (tests/model_rows.rs) |
| R7 | The scene index, a navigation region named "Scenes": "N scenes" (1 scene), then one entry per scene: the place in caps, the time of day after an en dash, quieter; the run before the first heading is "Opening"; the page number the engine puts inside a heading is left out | ui/read.js sceneLabel, railEntries, headingText | done |
| R8 | One Tab stop for the whole index; Up/Down/Home/End move it, stopping at the ends | ui/read.js railStep | done (tests/model_rows.rs) |
| R9 | Pressing an entry scrolls its scene to the top (smooth, instant with Reduce Motion) | ui/read.js buildRail | done |
| R10 | The scene being read is marked (current, highlighted) and kept in the index's view; the index's Tab stop follows it unless the keyboard is walking the index | ui/read.js markCurrent | done |
| R11 | Open by default; opening and shutting slides 0.8rem and fades over 0.2 s (none with Reduce Motion); shut, it is out of the Tab order | ui/read.js setIndexOpen, ui/surfaces.css | done |
| R12 | No headings: "No scene headings in this script." | ui/read.js NO_SCENES | done (tests/model_rows.rs) |
| R13 | A book that came back with no pages: "No pages came back", the sentence, Convert it again, Send | ui/read.js NOTICES | done |
| R14 | The place is kept as a scene and a fraction into it, across a Settings re-render, a window resize and a tab switch; a scene that is gone puts the reader at the top | ui/read.js readerPlace, scrollTarget, keep, restore | done (tests/model_rows.rs) |
| R15 | Re-rendered from every Settings save that rebuilds the book | ui/tune.js flush | done |
| R16 | The device layer over the book: the window's ink and paper, margins 1.6em and 7%, re-dressed live when the OS theme changes | ui/read.js deviceCss, followTheme | done |
| R17 | Pictures in the book shown (the engine writes them as data: URIs) | ui/read.js header | done (Blitz draws the data: URIs; book.rs pictures_are_drawn) |
| R18 | The script is the ENGINE's preview (the EPUB's XHTML and css.ts, untouched) rendered by a browser engine: one renderer for the window and the book, so a formatting decision cannot disagree | ui/read.js header | done (Blitz renders the engine preview: the EPUB XHTML and css.ts untouched, plus one window-only page-marker shim; see the design doc) |
| R19 | "Pager": paging is the keyboard's (R6); there is no on-screen pager control in the window | ui/read.js | done |

### S. Settings (`tune`)

| ID | Feature | Source | Status |
|---|---|---|---|
| S1 | Head "This script’s settings" with Send on its line; the lede paragraph | ui/tune.js | done |
| S2 | While the answer is out: "Reading this script’s settings…" and Send (never the fault screen) | ui/tune.js emptyPaneMode | done |
| S3 | Settings that could not be read: "Settings out of reach", the sentence, the engine's sentence in alarm colour, Convert it again, Send; asked again next visit | ui/tune.js FAULT, load | done |
| S4 | Two columns: 420 of settings and the preview in the rest, the preview pinned while the page scrolls and as tall as the window less its margins; under an 880 block one column, the preview below at 44% height, unpinned, boxes a measure wide | ui/surfaces.css .tune-split | done |
| S5 | Presets: "Load a preset", one button per preset the engine lists, the one the script is on marked (brass outline), inert and current; "Overwrites every setting below."; a preset replaces all twenty; the mark follows the values, and goes when a knob moves | ui/tune.js drawPresets, applyAll, presetsFrom, currentPreset | done |
| S6 | Five groups, each a disclosure box (hairline, radius, panel fill), only the first open; a brass triangle that turns as it opens (0.2 s) | ui/tune.js drawGroup, groupStartsOpen | done |
| S7 | "Read from the PDF" carries its note: these four apply at the next conversion | ui/tune.js GROUPS | done |
| S8 | Twenty knobs: label, Beta badge on the two picture switches, then a slider with a tabular read-out ("1.0 em", "20%"; "not set" for a missing value), a checkbox, or a choice | ui/tune.js GROUPS, displayValue | done |
| S9 | Each knob's sentence under it: why it is idle (first), then what it is for; tied to the control as its description; hidden when empty | ui/tune.js notesFor, state | done |
| S10 | Character name indent and Parenthetical indent are idle (disabled, muted) unless Character names are Indented: "Only when character names are indented." and they come back live in place when it changes | ui/tune.js idleReason, refreshIdle | done |
| S11 | A moved value is clamped to the engine's range; a non-value is never sent | ui/tune.js coerceValue | done |
| S12 | Changes gather for 300 ms, then save on the book's turn: settings --set, then the rebuild; the status line, a live status: "Changed. Saving in a moment…", "Saving, and rebuilding the book to match…", "Saved. The book on disk matches what you see.", the wait's words, or the engine's refusal in alarm colour | ui/tune.js STATUS, statusFor, schedule, flush | done |
| S13 | A failed save owes its changes again; a newer change wins; another script opened meanwhile drops the answer (era) | ui/tune.js restorePending, era | done (tests/model_rows.rs) |
| S14 | The engine's stored values redraw the screen when they differ and nothing newer is owed | ui/tune.js flush | done |
| S15 | The rebuild's warnings in parentheses under the status, as a live status, replaced only by the next good rebuild | ui/tune.js warningLines | done (tests/model_rows.rs) |
| S16 | A settle holds a pending update restart until its save has started | ui/tune.js holdEngine | done |
| S17 | Foot: "New scripts start from Screepub’s own defaults." / "… your own defaults.", Use these for new scripts, Reset new scripts to Screepub’s defaults (only when they differ), a live note ("New scripts will start from these settings." and the reset's), both quiet while one runs, era guard | ui/tune.js drawDefaultsFoot, writeDefaults | done |
| S18 | "When a PDF is converted": a named radio group, Keep its settings / Follow the defaults, each with its line as its description; one Tab stop and arrows; writes queued, only the newest paints; "Saved. It applies to PDFs you convert from now on." | ui/tune.js KEEP_CHOICE, chooseKeep | done |
| S19 | "While a script converts": Animate the page, Typewriter sounds (rests while the page is still), each with its line; "This computer is set to reduce motion, so the page does not animate." under Reduce Motion; a refused write flips back with the sentence | ui/tune.js PAGE_SWITCHES, flipSwitch | done |
| S20 | The preview: labelled "Preview", hidden from the screen reader, not a Tab stop; the same document as Read, re-rendered after each save | ui/tune.js drawPreview | done (Blitz, as R18) |
| S21 | Knob effects: live (preview changes), book (title page: the book only), reconvert (the four read from the PDF) | ui/tune.js EFFECTS | done |

### D. The send panel (Convert's result, right half)

| ID | Feature | Source | Status |
|---|---|---|---|
| D1 | Nothing drawn until the reader's device is known (app settings `readerDevice`), so the cards never flash for someone who chose | ui/send-panel.js start | done |
| D2 | No book on disk: "No book to send", the reason, Convert it again | ui/send-panel.js drawBlocked, send-routes.js blockedReason | done (tests/model_rows.rs) |
| D3 | Cards: "What do you read on?", Kindle; iPhone, iPad or Mac (Apple Books); Kobo; tolino; reMarkable; My device isn’t here (Beta) | ui/device.js DEVICES, MISSING | done |
| D4 | A card for a reader plugged in now: brass edge and a green dot with "Plugged in"; its name read as "<Name>, plugged in"; marked in place when the poll changes | ui/send-panel.js markCard | done |
| D5 | A card press goes calm at once and stores the choice app-wide; a refused store says why on the status line | ui/send-panel.js chooseDevice | done |
| D6 | The asterisk on what nobody has run on that hardware (everything but a Kindle on a Mac; Apple never): "About <Name>: never tried on real hardware"; hover opens the note, leaving closes it after 400 ms; a press pins it (expanded state exposed) | ui/send-panel.js asterisk, device.js asteriskNote | done |
| D7 | The note: "Built and tested in code, never tried with a real Kindle on Windows/Linux yet…" or "… on a real <Name> yet…", then It worked / It did not work (Beta) where there is a route to report | ui/device.js, ui/requests.js reportRoute | done (tests/model_rows.rs) |
| D8 | A report: both answers dead while it is out; thanks takes them away; a failure offers Try again for the same answer | ui/send-panel.js reportAnswers | done |
| D9 | Calm screen: one brass button for the way that fits (device.js offerFor), its Beta badge for our own AZW3, "Plugged in" beside a device button, the asterisk; the lines under it (the web-drag line, the plug-in line, Apple elsewhere, tolino on Windows) | ui/send-panel.js drawCalm, device.js offerFor | done |
| D10 | Kindle: plugged in → copy (then web and email lead Other ways); otherwise Send to Kindle (web), then email, then Save the EPUB | ui/device.js offerFor | done (tests/model_rows.rs) |
| D11 | Apple: Books on a Mac; elsewhere Save the EPUB with the iCloud line | ui/device.js offerFor | done (tests/model_rows.rs) |
| D12 | While the route list is out: "Looking for every way to send it…" | ui/device.js LOOKING | done (tests/model_rows.rs) |
| D13 | Route list asked quick then full on the calm screen's first draw for a book and on every plug-in change; an older answer never replaces a newer one; nothing redraws while a send runs (owed, asked after); an unchanged list does not redraw | ui/send-panel.js refresh | done |
| D14 | A list that cannot be read: the engine's sentence in place of the brass button, never a brass picked from half a list | ui/send-panel.js fault, send-routes.js routesFrom | done (tests/model_rows.rs) |
| D15 | A reMarkable reader: the full list asked again every 10 s while the calm screen shows, and on a focus return | ui/send-panel.js lookForRemarkable | done |
| D16 | Other ways (outline, expanded state) and Different device (quiet); opening and shutting keeps the keyboard on the toggle; shutting puts the KFX checklist away | ui/send-panel.js toggleWays | done |
| D17 | Other ways, Send it: the offer's follow-ups, then every other route in the engine's order, dimmed ones with their fix; never the brass one or the two saves | ui/device.js sendItRows | done |
| D18 | Readers that only need plugging in or docking folded into one muted line ("Kobo, tolino and reMarkable: plug in over USB to send."), each name with its own asterisk; fewer than two stay rows | ui/device.js foldUnplugged | done (tests/model_rows.rs) |
| D19 | A route row: title, the engine's detail, the volume for a mounted reader, the email row's "First time? Amazon needs your sender address approved…" with Open Amazon’s page; the button (none when dimmed) | ui/send-panel.js routeRow, send-routes.js routeLines, emailSetupHint | done |
| D20 | Buttons that hand over the Kindle file name it: "Copy to Kindle (.kfx)", "Save a Kindle file (.azw3)…" | ui/send-routes.js buttonLabel | done |
| D21 | Save a file: EPUB, KFX, AZW3 (Beta), MOBI, Fountain, FDX (Beta), each with who it is for and Save…; KFX not makeable here: "This computer cannot make one yet." and Set it up; no .fountain: "This book was made without a .fountain, so there is none to work from." | ui/device.js SAVE_TYPES, saveRows | done |
| D22 | A save: the Save box first (named for the book, one filter), then the engine writes there; "Saving…", "Saved <file>.", or the refusal | ui/send-panel.js saveFile | done |
| D23 | A copy to a reader: "Getting the book ready for <Name>…" (or the build line), "Copying it to <Name>…", then "<Title> is on your <Name>. Eject it before you unplug." with Eject in the brass place, or "Sent to <Name>." for one that does not mount; what went across in the engine's words under it | ui/send-panel.js sendTo, device.js copiedState | done |
| D24 | A route the engine performs: "Opening <title>…", then the engine's note or "The engine said it worked, without saying what it did." | ui/send-panel.js perform, send-routes.js routeNoteFrom | done |
| D25 | One flow at a time: every button dead while a send, a save, a route, an eject or a KFX install runs; nothing redrawn under it; a flow begun for one book says nothing beside another (era) | ui/send-panel.js sending, era | done |
| D26 | The keyboard goes back to the button that started a flow (found by its key after a redraw), else the brass, else the page's plan; the brass is owed the keyboard on a fresh result and after a card press | ui/send-panel.js keepFocus, giveBackFocus, brassOwed | done |
| D27 | Eject <Name> (Beta off a Mac): "Ejecting <Name>…", "Your <Name> is ejected. You can unplug it now.", "… is not plugged in any more.", or the refusal with the button kept while it is still plugged in; unplugging takes the button away and keeps the line | ui/device.js ejectOutcome, ejectStillThere | done |
| D28 | A reader still waiting to be ejected follows the reader to the next book's result | ui/send-panel.js mountPanel, sendTo | done |
| D29 | The Kindle file asked about when the result shows and on every focus return (`export --check`), naming the file on its buttons | ui/send-panel.js checkKindle | done |
| D30 | Built ahead in the background when the check says the KFX is out of date, Calibre builds it, a Kindle is in play, once per book version and file kind, never while one runs or a flow is busy | ui/send-routes.js buildsAhead, send-panel.js maybeBuildAhead | done (tests/model_rows.rs) |
| D31 | The build line and a sweeping bar while a Kindle file builds: "Building the Kindle file. This takes about half a minute." (KFX), "… This takes a moment.", "Building the Kindle file…"; the bar breathes instead of sweeping under Reduce Motion; a live status that speaks only when its words change | ui/send-routes.js buildLine, ui/surfaces.css .build-sweep | done |
| D32 | Settings fetched before an export if this window never saw them, so a rebuild uses the script's own | ui/send-routes.js needsSettings | done |
| D33 | My device isn’t here: the sentence, "Which device?" field (80 characters), Send (Beta; dead while empty or sending), "Sends the device’s name, Screepub’s version and your platform. Nothing else.", a live line for the answer, Save the EPUB…, Back; the typed name survives a redraw and Back | ui/send-panel.js missingPanel, ui/requests.js | done |
| D34 | The status line (live) and its alarm colour for a failure; kept with the book on the window state | ui/send-panel.js say | done |

### K. The KFX checklist ("Best Kindle quality")

| ID | Feature | Source | Status |
|---|---|---|---|
| K1 | Opened by Set it up under Save a file, in place; the engine's checklist taken whole or not at all (Calibre, Kindle Previewer, the KFX plugin) | ui/kfx.js checklistFrom | done (tests/model_rows.rs) |
| K2 | Shown only where KFX is possible, and only while not ready (or an install just ran or is running) | ui/kfx.js showSetup | done (tests/model_rows.rs) |
| K3 | Each step: its name and Installed, a link button (opens the page; a failure says "Could not open the page. It is at <url>"), an Install button (brass, dead while busy, "Installing…"), or the engine's why | ui/kfx.js controlFor | done |
| K4 | Install: "Downloading and installing the KFX plugin. This takes a few seconds.", then "Installed the KFX plugin <v>. Kindles now get KFX. Removed an older copy: <name>." or the refusal; every other panel button dead while it runs | ui/kfx.js afterInstall, installedLine | done (tests/model_rows.rs) |
| K5 | Asked again whenever the window gets the focus back; an unchanged answer redraws nothing | ui/kfx.js probe | done |
| K6 | The links allowed: the issue tracker, Calibre's macOS and Windows download pages, Kindle Previewer's page; nothing else opens | tauri/capabilities/default.json | done |

### H. Devices and the Kindle heads-up

| ID | Feature | Source | Status |
|---|---|---|---|
| H1 | `devices --quick` every 2 s while a book is open, stopped when none is; one ask out at a time; a failed or malformed listing changes nothing; listeners hear only a changed list | ui/device-watch.js | done |
| H2 | A reader with an empty name is named for its kind | ui/device-watch.js devicesFrom | done (tests/model_rows.rs) |
| H3 | Heads-up: "Kindle plugged in. Copy <Title>?" in a box under the tabs, a live status, Copy it (brass) and Not now; once per plug-in (the first list only seeds; a Kindle that leaves and returns is news again) | ui/heads-up.js, ui/device.js headsUpFor | done |
| H4 | Not over the result (its card says it), never while converting; closes when the page under it becomes one of those | ui/heads-up.js recheck | done |
| H5 | Goes by itself after 8 s; slides in from above (none with Reduce Motion); the keyboard is not left on it as it goes | ui/heads-up.js, ui/surfaces.css | done |
| H6 | Copy it: the result on Convert, then a copy to that Kindle, unless a conversion started meanwhile | ui/main.js copyTo | done |

### U. Updates

| ID | Feature | Source | Status |
|---|---|---|---|
| U1 | Only on a Mac, and only where the updater exists in the build | ui/update.js updatesPossible, ui/app.js updaterReady | done |
| U2 | Opt-in once ("Check for new versions once a day?"); "said no" and "never asked" kept apart; stored in the window's own storage | ui/update.js rememberAnswer, shouldAsk | done |
| U3 | At most one check a day, stamped BEFORE the request; a manual check ignores the switch and the throttle | ui/update.js runCheck | done (tests/model_rows.rs) |
| U4 | A failed check is a message, never "you are current"; an Intel Mac is told "New versions of Screepub run only on Macs with Apple silicon…" | ui/update.js isIntelMacRetired | done (tests/model_rows.rs) |
| U5 | A version found is remembered across launches until a check answers otherwise | ui/update.js rememberFound, rememberedOffer | done (tests/model_rows.rs) |
| U6 | The label: "Update to 0.8.0", "Downloading 0.8.0…" / "… 40%", "Installing…", "Restarting after this finishes…", "Restart now", "Restarting…", "Update failed. Try again", "Update installed. Quit and reopen Screepub to use 0.8.0."; clickable only on offer (not a retry's), failed and stalled | ui/update.js updateLabel, labelActionable | done (tests/model_rows.rs) |
| U7 | Download, verify (minisign), swap the bundle, then retire the engine, wait for 500 ms of quiet (capped at 120 s, then "Restart now"), restart into the new bundle; a refused restart is "installed", not a failure; a failed cleanup after a good install is not a failure | ui/update.js installAndRestart, ui/app.js updateInstall | done (sp-update flow over the engine gate the busy count mirrors into; tests/update_run.rs) |
| U8 | The endpoint: latest.json on the GitHub releases; the public key | tauri/tauri.conf.json | done (sp-update baked endpoint and key; ManifestUpdater) |
| U9 | One run, one moment, heard by the label and the notes alike; a newer offer during a run waits; a same-version offer after a failure keeps the failure on screen | ui/update-flow.js | done (model/update_flow.rs; tests/update_run.rs) |
| U10 | Version comparison is the engine's (`src/update/compare.ts`: v prefix, build metadata, describe suffixes, prereleases) | ui/update-compare.js | done |

### O. Release notes sheet

| ID | Feature | Source | Status |
|---|---|---|---|
| O1 | "Screepub 0.11.1", the headline, each section with its items (a bold lead or plain prose), compiled from docs/releases/<version>.md, offline | ui/notes-surface.js, ui/notes.js | done |
| O2 | No heading with nothing under it | ui/notes-surface.js visibleSections | done |
| O3 | Updates block where updates are possible: the once-a-day switch with its words, Check for updates, a live line, the new version's notes, Install <version> | ui/notes-surface.js updateBlock, notesView | done |
| O4 | Elsewhere: "Screepub does not update itself here. New releases are at github.com/ssandweiss/screepub/releases." | ui/notes-surface.js | done |
| O5 | The switch is resynced each time the sheet opens | ui/notes-surface.js show | done |
| O6 | Check for updates: "Looking…", then the flow's words; disabled while a run is busy | ui/notes-surface.js | done |

### B. Bug report, Beta badges, the request service

| ID | Feature | Source | Status |
|---|---|---|---|
| B1 | Report a bug opens a pre-filled GitHub issue: an instruction, "What happened:" with the code and the engine's sentence, cut to fit 2000 characters, and "Screepub <version> · <platform>" | ui/feedback.js newIssueUrl | done |
| B2 | Every absolute path in it becomes `<path>` plus its extension; the home folder becomes ~ | ui/feedback.js redact | done |
| B3 | The Beta badge: "Beta", in a hairline box, beside (never inside) what it qualifies, read out with its line, described as "New: works in our tests, not yet tried widely on real devices or apps." | ui/beta.js | done |
| B4 | Where it is worn: the FDX and AZW3 save rows, our own AZW3 on Copy to Kindle, the two picture switches, Eject off a Mac, the device-request Send, the report answers, the My device isn’t here card | ui/beta.js, callers | done |
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
| A3 | Descriptions: each knob's sentence, each radio's and switch's line, the Beta badge's note | ui/tune.js, ui/beta.js | done |
| A4 | Focus rings: 2px ink outline offset 2 to 3 px on every control, the panes and the reader's stage, keyboard only | ui/style.css, ui/surfaces.css | done |
| A5 | Decorative furniture hidden from assistive technology: the rail, the well's mark, the result's page, the typed page, the preview, the build bar | all | done |
| A6 | Nothing is announced twice: a live line is rewritten only when its words change | ui/send-panel.js showBuild, ui/kfx.js say | done |

**Inventory size: 238 rows** (W 25, N 19, E 28, C 31, P 13, R 19, S 21, D 34, K 6, H 6, U 10, O 6, B 6, L 8, A 6). As of 2026-10-11 (wave 3): 233 done, 4 wave 3 (W6 to W9, the bundle: the xtask's), 1 gap (W12: no sidecar to be missing).

## Window test files

The TypeScript tests that load `desktop/ui` (or `desktop/src-tauri`) are
the window's parity spec. Where each one's behaviors live in `sp-app`
(`m/` is `rust/crates/sp-app/src/model/`, `screens` is
`rust/crates/sp-app/tests/screens.rs`, `behaviors` is
`rust/crates/sp-app/tests/behaviors.rs`, one test per inventory row, named
for it):

| TS test file (tests) | Rust tests | Status |
|---|---|---|
| desktop-ui.test.ts (425) | m/convert.rs, m/read.rs, m/tune.rs, m/kfx.rs, m/book_queue.rs, m/busy.rs, m/focus.rs, m/frame.rs, m/update.rs, m/feedback.rs, engine.rs (argv contract), screens (Convert, Read, Settings, notes, heads-up, focus after a dialog, stray null) | partial: every decision block ported; mounted-DOM blocks ported as screens tests where the behavior is drawn; source-reading tests about desktop/ui itself (CSP, no innerHTML, one file talks to Tauri) have no Rust subject |
| send-panel-ui.test.ts (125) | screens (cards, calm, Other ways, after a copy, My device isn't here), behaviors (D rows, keep-focus-by-key D26), m/device.rs, m/focus.rs | partial: build-ahead timing blocks ported as model tests only |
| send-routes-ui.test.ts (71) | m/send_routes.rs | partial: the round trip against the engine's real route catalog waits on sp-engine |
| send-device.test.ts (34) | m/device.rs | ported |
| page-show.test.ts (22) | m/page_show.rs, app/show.rs tests, screens (the page writes itself), behaviors (P2, P4, P10, P13 sounds) | ported (formed blocks drawn by Blitz from the book) |
| page-timing.test.ts (16) | m/page_timing.rs | ported |
| device-watch.test.ts (9) | m/device_watch.rs | ported |
| device-requests-ui.test.ts (11) | m/requests.rs, screens (My device isn't here) | ported |
| desktop-update-install.test.ts (20) | m/update.rs, m/update_flow.rs (the one run's state machine), m/busy.rs (retire, holds, quiet), behaviors (U9, N12) | partial: the install and restart run itself waits on the wave 3 transport |
| desktop-notes.test.ts (14) | m/notes.rs, screens (the stamp opens the notes) | ported (the generator becomes an xtask) |
| desktop-layout.test.ts (26) | m/layout.rs | partial: geometry ported; CSS-text pins have no Rust subject |
| desktop-layout-measured.test.ts (16) | m/layout.rs (900, 1280, 1800) | ported as geometry; a pass measuring drawn rects is still to write |
| desktop-tokens.test.ts (9) | theme.rs (values pinned to tokens.css and style.css), fonts.rs (the six faces, byte for byte) | ported |
| desktop-shell.test.ts (67) | app/host.rs (opener allow-list, the one network address), main.rs (window size, floor) | partial: Tauri pins (sidecar name, capabilities, tauri.conf) retire with Tauri; updater and bundle pins move to wave 3 |
| beta-marks.test.ts (10) | m/tune.rs, m/device.rs (which rows wear it), screens (badges drawn) | partial: CLI help, README and site checks are not the window's |
| update-compare-module.test.ts (9) | m/update_compare.rs | ported (moves to sp-engine) |
| no-em-dash.test.ts (3) | tests/no_em_dash.rs (sp-app's sources, comments stripped, allow-list kept honest) | ported for the window; the engine crates' half belongs to the xtask scan |
| app-references, capture*, build-*, bump-version, cli-*, release-*, smoke-bundle | not window behavior | owned by the xtask, CLI and packaging waves |

## Tools

Every file in tools/ (inventoried 2026-10-11, 45 files), where it is used,
and the `cargo xtask` command it becomes (rust/crates/xtask/src/lib.rs lists
them all; `cargo xtask --help` prints them). "Used by" names the callers
that run it; the docs that only mention a tool are history and are not
listed, except the current ones (CLAUDE.md, CONTRIBUTING.md,
docs/developers.md, docs/mac-qa.md, docs/release-secrets.md). SKILL is the
/release skill (.claude/skills/release/SKILL.md, local to the owner's
machine and not versioned); its commands change to `cargo xtask ...` at
cutover.

| Tool | Kind | Used by | xtask command | status |
|---|---|---|---|---|
| tools/make-fixture.py | Python | CLAUDE.md, CONTRIBUTING.md, tests/fixture-stability, torture-layout, torture-markup (and named by many fixture tests) | `make-fixture` | done: xtask/tests/fixture.rs (25) |
| tools/torture-content.py | Python | make-fixture.py (torture.pdf's text) | data of `make-fixture` | done: src/fixture/torture_content.rs |
| tools/field-station-content.py | Python | make-fixture.py (field-station.pdf's text) | data of `make-fixture` | done: src/fixture/field_station_content.rs |
| tools/torture-manifest.json | data | tests/torture-coverage, torture-pictures; device-checklist.ts; CLAUDE.md | stays data, read by `device-checklist` | done: xtask/tests/torture_coverage.rs |
| tools/device-checklist.ts | TS | CLAUDE.md (by hand before a device pass) | `device-checklist` | done: xtask/tests/torture_coverage.rs |
| tools/mobi-picture-probe.ts | TS | CLAUDE.md (by hand); tests/mobi-picture-probe | `mobi-picture-probe` (books from sp-mobi::probe) | done: xtask/tests/mobi_picture_probe.rs (6), sp-mobi/tests/probe.rs |
| tools/calibre-config.ts | TS | bunfig.toml preload via tests/isolate-calibre-config.ts, src/export/calibre.ts, tools/capture/run.ts | module `calibre_config` (the engine's half belongs to sp-engine) | done: src/calibre_config.rs (its 4 unit tests) |
| tools/capture-screens.ts | TS | CLAUDE.md, SKILL, docs/mac-qa.md | `capture-screens` | done: xtask/tests/capture.rs, capture_chrome.rs |
| tools/review-screens.ts | TS | CLAUDE.md, SKILL | `review-screens` | done: xtask/tests/review_screens.rs |
| tools/capture/cdp.ts | TS | capture-screens, tests/helpers/layout-chrome.ts | `capture` module (CDP over tungstenite) | done: xtask/tests/capture_chrome.rs (fake Chrome) |
| tools/capture/run.ts, server.ts, page.ts, shots.ts, gate.ts, review.ts | TS | capture-screens, review-screens | `capture` module | done: src/capture/; xtask/tests/capture.rs |
| tools/capture/bridge.js, steps.js | JS (runs in the page) | capture-screens | drives desktop/ui today; the egui window is driven natively at cutover | partial: still drive desktop/ui; replaced at cutover |
| tools/capture/frame.html, hero.html | HTML | capture-screens (picture frames) | stay HTML, like site/ | done: stay HTML |
| tools/make-og.sh | shell | site/og-card.html (by hand) | `make-og` | done: xtask/tests/capture_picture.rs, capture_chrome.rs |
| tools/bump-version.ts | TS | CLAUDE.md, SKILL (`ship`) | `bump-version` | done: xtask/tests/bump_version.rs (20) |
| tools/release-notes.ts | TS | SKILL (the draft's facts) | `release-notes` | done: xtask/tests/release_notes_tool.rs (27) |
| tools/build-desktop-notes.ts | TS | desktop.yml, bump-version.ts, SKILL | `build-desktop-notes` (also writes sp-app's notes) | done: xtask/tests/desktop_notes.rs (13) |
| tools/build-desktop-tokens.ts | TS | desktop.yml | `build-desktop-tokens` | done: xtask/tests/desktop_tokens.rs (11) |
| tools/build-update-compare.ts | TS | desktop.yml | `build-update-compare` | done: xtask/tests/update_compare.rs (13) |
| tools/hooks/require-release-notes.sh | shell | the owner's PreToolUse hook (.claude/settings.local.json), SKILL | `require-release-notes` | done: xtask/tests/release_hook.rs (24) |
| tools/bump-tap.sh | shell | release.yml (`tap`), SKILL | `bump-tap` | done: xtask/tests/tap_tools.rs |
| tools/check-tap.sh | shell | release.yml, tap-freshness.yml | `check-tap` | done: xtask/tests/tap_tools.rs |
| tools/check-latest.ts | TS | release.yml, tap-freshness.yml | `check-latest` | done: thin over sp-release/src/check_latest.rs (`sp-release check-latest` in release.yml and tap-freshness.yml); xtask/tests/check_latest.rs (18) |
| tools/build-cli.ts | TS | ci.yml, release.yml, SKILL | `build-cli` | done: xtask/tests/build_cli.rs (89), build_cli_e2e.rs (2, opt-in); its archives are written by sp-release's `cli_tarball`/`cli_zip`, as release.yml's `sp-release cli-archive` |
| tools/macos/screepub-cli.entitlements | plist | build-cli.ts, SECURITY.md | stays data, read by `build-cli` | done: stays data; three bun-era keys go at cutover (Decisions) |
| tools/smoke-cli.ts | TS | ci.yml, release.yml, desktop.yml, CLAUDE.md | `smoke-cli` | done: xtask/tests/smoke_cli.rs (21) |
| tools/verify-signing.ts | TS | docs/mac-qa.md (by hand) | `verify-signing` | done: xtask/tests/verify_signing.rs (34) |
| tools/build-app-bundle.ts | TS | release.yml, CLAUDE.md | `build-app-bundle` | done: the files are sp-release's (mac, linux, nsis, archive, sign); the xtask command orders the steps (xtask/tests/build_app_bundle.rs, native_bundle.rs); release.yml calls sp-release directly |
| tools/bundle-archive.ts | TS | desktop.yml, build-app-bundle.ts | `bundle-archive` | done: xtask/tests/bundle_archive.rs (28); reading only |
| tools/smoke-bundle.ts | TS | desktop.yml, release.yml | `smoke-bundle` | partial: xtask/tests/smoke_bundle.rs (38); left: running the native window headless (Decisions) |
| tools/dmg.ts | TS | smoke-bundle.ts, verify-signing.ts | module `dmg`, shared by `smoke-bundle` and `verify-signing` | done: src/dmg.rs, the one copy smoke-bundle and verify-signing share |
| tools/update-signature.ts | TS | release.yml | `update-signature` | done: thin over sp-release (boxes.rs, sign.rs) and sp-update (verify.rs); xtask/tests/update_signature.rs (17), sp-release/tests/compat.rs |
| tools/build-update-manifest.ts | TS | release.yml | `build-update-manifest` | done: thin over sp-release/src/manifest.rs (`sp-release manifest` in release.yml); xtask/tests/build_update_manifest.rs (21) |
| tools/build-sidecar.ts | TS | CLAUDE.md, CONTRIBUTING.md, desktop.yml, release.yml | `build-sidecar` answers "retired" | retired |
| tools/sidecar-targets.ts | TS | build-sidecar.ts, desktop/src-tauri/src/sidecar.rs | none | retired |
| tools/oracle/dump.ts, dump-all.ts, freeze-clock.ts | TS | `cargo xtask oracle` and `parity` | stay TypeScript: they ARE the TypeScript engine asked to show its work (dump.ts also writes each picture's raw pixels to `pixels/`); deleted at cutover with it | stays until cutover |

Transitional TypeScript generators outside tools/. Each reads the TypeScript
engine or its dependencies (pdfjs-dist, fountain-js), so each retires at
cutover and the file it wrote stays committed as the only copy.
`rust/crates/xtask/tests/generated_files.rs` pins every such file by
sha256 (LF line endings), so a hand edit or a regeneration from a different
pdf.js shows; a deliberate regeneration before cutover updates the digest in
the same commit.

| Generator | Writes (committed) | status |
|---|---|---|
| rust/crates/sp-pdf/oracle/gen-tables.ts | rust/crates/sp-pdf/src/tables.rs (pdf.js 6.3.289's data tables as Rust) | retires at cutover; output pinned |
| rust/crates/sp-epub/tests/data/options-golden.ts | rust/crates/sp-epub/tests/data/options-golden.json | retires at cutover; output pinned |
| rust/crates/sp-mobi/tests/data/mobi-sources.ts | rust/crates/sp-mobi/tests/data/mobi-sources.json | retires at cutover; output pinned |
| rust/crates/sp-fdx/tests/data/record-tokens.ts | rust/crates/sp-fdx/tests/data/fountain-tokens.json | retires at cutover; output pinned |

## Decisions

- **EPUB deflate streams (sp-epub, 2026-10-11).** The zip writer
  (sp-epub/src/zip.rs) writes JSZip 3.10.1's container field for field, but
  deflates with zlib-rs, which descends from zlib-ng and picks different
  matches from pako (zlib 1.2.8): every entry inflates to the TypeScript's
  bytes, while a deflated entry's compressed bytes, and so its compressed
  size and the offsets after it, can differ by a few bytes. Parity compares
  inflated entries, as the harness always has. Whole-file identity would
  need a port of zlib 1.2.8's deflate; nothing a reader sees depends on it.

Differences the port keeps on purpose, and questions for the owner. Each
names the check it affects.

- **sp-engine's CLI parity (2026-10-11).** `sp-engine/tests/parity_cli.rs`
  runs `bun src/cli.ts <verb> ... --json` and the crate's handler for the
  same verb on the same files, and requires the same line byte for byte and
  the same files left behind: `app-settings` (25 cases, reads, every key,
  every refusal, a file the TypeScript wrote with keys this build does not
  know), `settings` (10), `update-decision` (18), `update-should-check`
  (11), `kfx-status`, `export` (28, among them the FDX bytes and the
  Fountain copy with Screepub's notes stripped), `export --for kindle|kfx
  --check`, `routes --quick` (5 remembered routes) and `route` (17: every
  refusal, and `save-epub`). All agree. Not compared: any `route` performer
  (they open Books, a browser or Mail) and any Kindle BUILD (the AZW3 and
  MOBI makers are not plugged in yet, and a KFX build is a real Calibre and
  Kindle Previewer run); the device rows of `routes` (device detection is
  the sp-device branch).
- **The KFX plugin install needs no HTTP in Rust (2026-10-11).** The
  TypeScript does not fetch the plugin itself: it hands Calibre's own
  Python (`calibre-debug -c`) the install snippet, which fetches Calibre's
  index with Calibre's pinned CA, checks the zip's size against the index
  and that it is a plugin, and installs it with Calibre's add_plugin. The
  port runs the same snippet, byte for byte (`export/kfx_install.py`), so
  the two downloads, the checks and the trust root are unchanged, and the
  engine stays without a network stack. A Rust (rustls) fetch would have
  replaced Calibre's certificate pinning with ours and changed what is
  trusted; nothing in this crate's half needs HTTP.
- **The environment is a value, not a global (2026-10-11).** Every
  function the TypeScript gives `env = process.env` takes an `Env` here
  (`Env::process()` for a real run), and every Calibre spawn gets exactly
  the `Env` it is handed (`env_clear`), as the TypeScript's `runCalibre`
  passes `process.env` on purpose. Rust tests are threads of one process,
  where mutating the environment is unsafe, so this is also how tests keep
  off the real settings file and the real Calibre folder.
- **Open differences in sp-engine (for the owner; none reached by any
  committed fixture or parity case).** (1) A settings file holding a lone
  UTF-16 surrogate escape (`"\ud800"`) or a number past the f64 range
  (`1e400`) is unreadable to serde_json and reads as `{}` (JavaScript reads
  it), so the next save drops it; Screepub never writes either. (2) A
  filesystem error the engine has no Node code for reports Rust's own
  wording where Node's `plainReason` falls back to Node's message. (3)
  `localeCompare(..., { numeric: true })` for two pre-release tags is an
  ICU collation; the port's is digits-as-numbers, letters without case
  first, then ICU root's punctuation order, checked against bun on the
  version pairs in parity_cli.rs, but not every Unicode letter. (4)
  `installKfxPlugin` returns a reason when calibre-debug cannot be started
  at all, where the TypeScript's promise rejects.
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
- **Hostile Fountain is refused, not an abort (review, 2026-10-12).** jsre
  recursed once per repetition of a group, so about 3,000 blank lines,
  lyric lines or centered lines in a row overflowed the window's 2 MiB
  engine thread and aborted the whole app (fountain-js answers them at
  once: V8 keeps its backtrack stack on the heap). `tokenize` now parses on
  its own 256 MiB-stack thread, and every search carries a stack budget
  (192 MiB there) and a step budget (200 million steps, about a second).
  A search over either fails and `tokenize` returns an `Err` ("This script
  is too complex to read..."). Answers differ from the TypeScript only on
  inputs no script has: a run of roughly 100,000+ blank lines (refused;
  fountain-js answers), and the patterns that backtrack exponentially in
  BOTH engines (a title-page value of 26+ characters ending in U+2028, 50,000
  `/*`, 50,000 `[[`), which fountain-js finishes in seconds to hours and
  Rust refuses in about a second. Checked by sp-fountain/tests/hostile.rs;
  no committed fixture or real script trips either budget.
- **services/device-requests (decided 2026-10-11).** Ported to Rust
  (workers-rs, compiled to wasm) in a late wave, with its tests; the owner
  asked for no code but Rust. It stays deployed as is until the Rust worker
  answers the same two forms identically.
  **Ported 2026-10-11** (rust/crates/sp-device-requests, branch
  rust/release): every answer byte for byte, all 24 checks. Two inputs
  answer differently and are refused either way (400), neither one the
  window sends: a JSON number beyond the double range and a lone UTF-16
  surrogate escape. JSON.parse accepts both and the form check refuses with
  "That request is not one this service takes."; serde_json refuses them as
  "That request is not JSON." Not deployed; the release design (part f)
  says what deploying it takes.
- **The release pipeline (2026-10-11, branch rust/release; design
  docs/superpowers/specs/2026-10-11-rust-release-design.md).** Bundles are
  laid out by sp-release to match the Tauri ones file for file (the
  window's executable keeps the name `screepub-desktop`); the Linux release
  gains an AppImage (the Bun engine made one impossible), so SHA256SUMS-app
  covers five installers. The Rust app's updater requires the signed
  version the Tauri windows only check when present, and stages its swap
  beside the bundle. Its HTTPS is rustls on the ring provider (C and
  assembly inside ring): an owner decision whether to take the unaudited
  pure-Rust provider instead. tools/bump-tap.sh and tools/check-tap.sh stay
  shell. rust/handover-proof compiles Tauri on purpose, in its own
  workspace, to run the window's own updater as the release's handover
  gate; it goes once no Tauri window is left to carry over.
- **The Kindle books' clock (2026-10-11).** `MobiInput::time` and the AZW3's
  `build_azw3_at` take the build time explicitly; `None` (and `build_azw3`)
  read `SOURCE_DATE_EPOCH`, else the system clock (sp-mobi's `clock`). The
  TypeScript read `Date.now()`. A MOBI cover index is a `usize`, so the
  TypeScript's checks against -1 and 0.5 are refused by the type instead.
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
- **sp-parser's input (2026-10-11).** The parser reads `sp_parser::facts::DocumentFacts`,
  which IS the oracle's `pages.json` shape (sp-parser/tests/oracle.rs proves every
  dump reads in and writes back unchanged). sp-pdf produces it: per page the
  viewport size, pdf.js's text items untouched, each font id's resolved name
  (`None` if unresolved), the operators listed in `facts::Op` (constructPath's
  paint by operator NAME, showText as joined glyph unicode), and per painted
  image object its width, height, ImageKind (1/2/3) and decoded pixels in
  `ImageFact::data` (the bytes pdf.js would hand over, before any conversion).
  When `data` is absent but `dataSha256` is set (an oracle dump), a picture is
  judged on its facts and carries no PNG; its furniture identity is its kind,
  size and pixel hash, which is exactly when two PNGs would be identical.
- **Pixels for the oracle (question for the owner).** pages.json hashes each
  picture's pixels instead of carrying them, so the standing parity test cannot
  compare PNGs. Proposal: tools/oracle/dump.ts also writes
  `pixels/<dataSha256>.bin` (raw pdf.js bytes; about 1 MB for the committed
  fixtures). sp-parser/tests/oracle.rs already reads that layout from
  `SP_PARSER_PIXELS=<folder>`; run that way on 2026-10-11 (pixels dumped by a
  one-off script), every committed and real picture decoded pixel for pixel
  and every picture statistic matched.
- **JSON floats.** sp-parser turns on serde_json's `float_roundtrip`: the
  default parser read 107.99999999999999 as 108, one ULP off pdf.js's double.
  Any crate comparing oracle numbers exactly needs the same (Cargo feature
  unification already gives it to the workspace).
- **JavaScript semantics.** sp-parser/src/js.rs holds what the TypeScript relied
  on silently: UTF-16 lengths for every length threshold, ECMAScript `\s` and
  `trim`, `Math.round` (half toward +infinity), and `/i` folding that never maps
  the long s or Kelvin sign onto ASCII. The two backreference regexes are code.
- **Timeouts.** resolveImageData's per-image wait and store race have no Rust
  counterpart (pixels arrive resolved); the 20 s document budget still charges
  conversion and encoding. `imageStats.decodersUnavailable` is never set.
- **Pictures are decoded past the guards, inside the budget (sp-cli,
  2026-10-11).** convert.ts decodes a PDF's pictures only after the scanned
  and not-a-screenplay refusals, and its budget counts the waiting for
  pdf.js's pixels. sp-pdf's `read` decodes every picture up front, so the
  conversion uses `sp_pdf::read_pages` (text and operators only) and fetches
  each picture's pixels through `sp_pdf::image_fact` as sp-parser reaches it
  (`PendingExtract::decode_images_with`), inside the timed region that
  charges the budget. A refused PDF decodes nothing. `sp_pdf::read` keeps
  its old answer (read_pages, then every picture).
- **PNG deflate level (sp-parser, 2026-10-11).** Pictures compare by decoded
  pixels (above), so the IDAT deflate is free to be the one that keeps the
  picture budget comfortably met; the budget itself is unchanged. Measured
  on the real scripts, release build, picture stage alone: the one
  picture-heavy document (converted with --force) took the whole 20 s at
  miniz_oxide level 9 (the budget ran out and pictures were dropped), 10.7 s
  at miniz 6, 17.0 s at zlib-rs 9, 6.9 s at zlib-rs 6 and 5.1 s at level 1
  (whose files are up to three times larger). sp-parser now deflates with
  zlib-rs (flate2) at level 6 (`PNG_DEFLATE_LEVEL`): files 2.5% larger than
  zlib-rs 9 on that document, 6% on the script with one large picture.
  The budget is wall-clock, so an unoptimized build ran out of it on that
  script (sp-parser's real-script oracle test dropped 107 pictures in a
  debug build); the workspace now builds sp-parser, sp-pdf and every
  dependency at opt-level 3 in dev and test profiles, which changes no
  answer and keeps the tests' timing the shipped program's.
- **What the parity harness compares, and how (sp-cli, 2026-10-11).** The
  `--debug` dump (`<stem>.elements.json`) is compared as JSON VALUES, not
  bytes: the TypeScript's key order is whatever order each code path built
  its element object in (`id` first on some, `type` first on others), an
  accident of object literals that nothing reads; the values match on every
  fixture and real script. Pictures compare by decoded content wherever they
  appear: the picture folder, EPUB entries, the preview's and the inline
  answer's `data:image/png` URIs, and MOBI and AZW3 records (record by
  record, a PNG record by its pixels; the PalmDB record offsets move with a
  picture's size and are not compared). Every other file is compared as
  before. Text files in a compared folder tree (library, config) have each
  side's scratch folder spelled `$WORK`.
- **No pdf.js warnings (sp-cli, 2026-10-11).** `--debug` let pdf.js's own
  warnings through to stderr; there is no pdf.js in Rust, so `--debug` writes
  only the elements dump. cli.test.ts's "--debug lets them through" retires.
- **The pages.json test path was never shipped (sp-cli, 2026-10-11).** sp-pdf
  landed before sp-cli needed it, so PDF input is always sp-pdf's.
- **Two copies of Node's path functions (open, for the owner).** sp-fsutil
  (`nodepath`) and sp-engine (`nodepath`) each port node:path; the CLI uses
  sp-engine's, sp-device uses sp-fsutil's. Both are checked against Bun;
  folding them into sp-fsutil's is a mechanical follow-up.
- **Fixture pictures (xtask make-fixture, 2026-10-11).** make-fixture's
  pictures are deflated by zlib-rs (flate2's zlib-rs backend, level 9),
  which writes the same streams zlib 1.2.12 gave Python, so every committed
  fixture but torture.pdf regenerates byte for byte
  (xtask/tests/fixture.rs, stability_*). torture.pdf was never byte-stable.
- **make-fixture's error text (2026-10-11).** Where Python died with a
  traceback, the port prints only the traceback's last line (the exception
  and its message). The exit codes are unchanged.
- **The CLI size floors (build-cli, 2026-10-11).** The bun CLI embedded a
  runtime (floor 20 MB); a Rust binary carries only the engine, so the
  floors are now 1 MB for the binary and 300 KB for the archive, set where
  a truncated write would land. Re-tune them on the first real release
  build of sp-cli (`RELEASE_FLOORS` in xtask/src/cli_build/build.rs).
- **DECIDED: the Mac CLI's bun-era entitlements go at cutover.**
  tools/macos/screepub-cli.entitlements grants allow-jit,
  allow-unsigned-executable-memory and disable-library-validation because
  bun's JIT needed them; a Rust binary needs none of the three. They are
  dropped at cutover, and the checks that pin the file
  (xtask/tests/build_cli.rs, build_cli_e2e.rs) change then.
- **DECIDED: release CI builds each target on its own native runner.**
  macos-15 (arm64), windows-latest, ubuntu-latest (x64) and
  ubuntu-24.04-arm (linux-arm64), with no cross-compiling. That is also why
  build_cli_e2e.rs builds only the host target.
- **The TLS provider: one stack, ring (2026-10-11; the owner may still
  choose otherwise for the whole port).** The xtask branches first gave
  bump-tap, check-tap and check-latest ureq over rustls with
  rustls-rustcrypto 0.0.2-alpha, a pure-Rust provider. That alpha depends
  on rustls-webpki 0.102.8, which RUSTSEC-2026-0104 covers (advisory-db,
  2026-04-22; CVE-2026-93599, GHSA-82j2-j2ch-gfr8; denial of service). What
  the advisory says: parsing a certificate revocation list with
  `BorrowedCertRevocationList::from_der` or
  `OwnedCertRevocationList::from_der` can panic on a valid but empty BIT
  STRING in the onlySomeReasons element of an IssuingDistributionPoint
  extension, before the CRL's signature is checked; applications that do
  not use CRLs are not affected. Patched: >= 0.103.13 and < 0.104.0-alpha.1,
  or >= 0.104.0-alpha.7; there is no 0.102.x fix, so 0.102.8 is in range.
  xtask configured no CRLs, so by the advisory's own terms it was not
  affected, but `cargo audit` would flag the lockfile. Once xtask's release
  commands became callers of sp-release, xtask linked sp-update's ureq and
  ring anyway, so rustls-rustcrypto was dropped: the whole port now speaks
  HTTPS through ureq's rustls on ring, rustls-webpki 0.103.15 (patched),
  and the lock holds no 0.102.x. Ring carries C and assembly; the release
  design already leaves "the unaudited pure-Rust provider instead" to the
  owner, and that choice would now cover sp-app, sp-update, sp-release and
  xtask together.
- **The capture gate's argument builders (2026-10-11).** The capture runs
  the engine the way the window does, so xtask/src/capture/window_argv.rs
  holds a copy of the window's argv builders. Until cutover
  xtask/tests/capture.rs checks the copy against desktop/ui/app.js; after
  it, against sp-app's own builders.
- **Windows upgrade over 0.11 (sp-release nsis, open).** The NSIS script
  sp-release writes installs per user into the Tauri installer's folder
  (`%LOCALAPPDATA%\Screepub`) under its uninstall key, with the same
  executable name, and deletes the old engine sidecar, so it should
  upgrade 0.11 in place. That is unverified on a real Windows system.
- **Linux package dependencies (open, half done).** egui loads X11 or
  Wayland and xkbcommon at run time. The .deb release.yml publishes gets
  its Depends: measured on the built binary (dpkg-shlibdeps, passed to
  `sp-release deb --depends`), but the .rpm declares no Requires: and a
  laptop `cargo xtask build-app-bundle --app native` writes neither. Add
  the rpm's Requires: (sp-release linux.rs) before cutover.
- **sp-app needs a headless mode for smoke-bundle (open).** The Tauri
  bundle's smoke runs its engine sidecar (`--version --json` and a
  conversion). The native window has neither, so smoke-bundle checks only
  that its binary is present, executable and declares the right version.
  sp-app needs `--version --json` and `--smoke <pdf>` for the full check.
- **Before cutover: the icons and the updater key (build-app-bundle).**
  `cargo xtask build-app-bundle --app native` looks for icons in
  rust/crates/sp-app/icons first and falls back to desktop/src-tauri/icons,
  and reads the updater public key from tauri.conf.json (release.yml hands
  sp-release its icons by path, and the app's own key is sp_update::PUBKEY). The icons move to rust/crates/sp-app/icons, and the
  public key needs a home of its own outside tauri.conf.json, before
  desktop/ goes.
- **One implementation per release job (2026-10-11, the xtask
  integration).** The xtask tool branches and rust/release each ported the
  release tools. sp-release, the library release.yml runs, is now the one
  implementation of every file a release publishes: the `.app` layout
  (mac.rs), the update archive and the CLI archives (archive.rs, with
  Contents/MacOS forced to 0755 as xtask's writer did), the minisign
  signer (sign.rs, minisign pinned =0.7.3 in the workspace), latest.json
  (manifest.rs), the .deb, the .rpm and the AppDir (linux.rs, now mapping a
  prerelease's `-` to `~` as xtask's did), the NSIS script (nsis.rs), and
  check-latest's judgement (check_latest.rs). Where xtask's port was the
  more faithful, it moved INTO sp-release: the strict minisign box readers
  (boxes.rs) and the check-latest judgement (JSON-quoted values, a
  platforms array, the last matching asset winning), now with an
  injectable fetcher. The xtask commands keep their names and flags for
  the /release skill and are thin callers: `update-signature`,
  `build-update-manifest`, `check-latest`, `build-cli`'s packing, and
  `build-app-bundle --app native` (which still orders codesign, notarytool,
  stapler, hdiutil and makensis on a laptop). What stays xtask's alone:
  the Tauri path of build-app-bundle until cutover, bundle-archive and
  smoke-bundle (readers), the one DMG helper (src/dmg.rs), verify-signing,
  and the tap tools. The native window's executable is `screepub-desktop`
  everywhere (xtask's branch had called it Screepub, screepub-window and
  Screepub.exe); xtask's tests were moved onto sp-release's names. Tests:
  xtask's stronger suites (check-latest 18, manifest 21, update-signature
  17, native bundle 18) now exercise sp-release through the wrappers, and
  the checks nothing in sp-release had are sp-release/tests/compat.rs:
  tauri-cli's published key opened by the pinned minisign, the update
  archive unpacked as tauri-plugin-updater and sp-update unpack it, and
  the real dpkg-deb and rpm reading the packages where installed. One
  base64 (0.23, which plist, rpm and ureq already bring) and one tar in
  the workspace table.
