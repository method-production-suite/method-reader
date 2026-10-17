//! Port of tests/cli-export.test.ts: `method-reader export`, the file you would
//! actually put on a reader.
//!
//! The handler is sp-engine's export_command (src/commands/export.rs),
//! called here in process with the TypeScript's seams (the Calibre and KFX
//! probes, the Kindle ladder, the two rung builders, the app settings
//! path), and the real book makers (`Books`). sp-engine's
//! tests/export_artifact.rs ports the ladder's own work and
//! tests/parity_cli.rs runs `export` against the TypeScript; no sp-engine
//! test ported this file's handler checks, so they are all here. Every
//! spawned check runs the built `method-reader` in a Sandbox.
//!
//! CALIBRE: every handler call gets common::seams::guarded_env (Calibre at
//! the guard path) and injected probes wherever the TypeScript injects
//! them. Where the real answer is the point (the real KFX build, and the
//! spawned `--for kindle --check`, which asks Calibre for its plugins),
//! Calibre is pointed at a scratch COPY of its folder, never the real one.
//!
//! Not portable as written: the TypeScript runs the relative `--out`
//! checks after `process.chdir` into the scratch folder, as a guard in case
//! the refusal ever stopped firing. Tests here are threads of one process,
//! so they cannot change folder; the refusal is checked the same way, and a
//! write it failed to stop would land beside the test binary's own working
//! folder, where the assertions on the refusal fail first.

mod common;

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use common::Sandbox;
use common::seams::{
    calibre_copy_str, calibre_env, copy_keeping_time, current_azw3, guarded_env,
    mobi_header_version, mtime, mtime_ms, names, refusal, set_mtime, source,
};
use serde_json::{Value, json};
use sp_engine::commands::export::{ExportDeps, ExportOptions, ExportResult, export_command};
use sp_engine::errors::EngineError;
use sp_engine::export::artifact::{
    BuiltBy, FreshKindleArtifactOptions, RungBuilders, available_formats,
};
use sp_engine::export::kfx::{KfxStatus, kfx_status};
use sp_engine::export::renderers::Books;
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions};

const KFX_READY: KfxStatus = KfxStatus {
    calibre: true,
    previewer: true,
    plugin_installed: true,
    ready: true,
};
const CALIBRE_ONLY: KfxStatus = KfxStatus {
    calibre: true,
    previewer: false,
    plugin_installed: false,
    ready: false,
};
const NO_TOOLCHAIN: KfxStatus = KfxStatus {
    calibre: false,
    previewer: false,
    plugin_installed: false,
    ready: false,
};

// ---- the shared book ----

/// A real EPUB, built ONCE by the engine through the CLI from a short
/// script, and checked rather than trusted: a failed build would leave
/// every test downstream failing (or passing) for an unrelated reason.
struct Built {
    sb: Sandbox,
    answer: Value,
}

fn built() -> &'static Built {
    static BUILT: OnceLock<Built> = OnceLock::new();
    BUILT.get_or_init(|| {
        let sb = Sandbox::new();
        let source = sb.write(
            "built/Script.fountain",
            "INT. ROOM - DAY\n\nMARGO\nHello.\n",
        );
        let book = sb.p("built/Script.epub");
        let r = sb.run(&[&source.to_string_lossy(), "--json", "-o", &book]);
        assert!(
            r.code == 0 && Path::new(&book).exists(),
            "the engine did not build the test EPUB (exit {}):\n{}\n{}",
            r.code,
            r.stdout,
            r.stderr
        );
        let answer = r.json();
        Built { sb, answer }
    })
}

/// One test's own folder holding a copy of the book and its script, times
/// kept, so the EPUB is no older than its .fountain.
struct Case {
    sb: Sandbox,
    dir: String,
    epub: String,
    fountain: String,
}

impl Case {
    fn new() -> Case {
        let b = built();
        let sb = Sandbox::new();
        // Not "export": a folder of that name in the CLI's working folder
        // would shadow the verb, as any file of that name does.
        let dir = sb.mkdir("case");
        copy_keeping_time(
            &b.sb.path("built/Script.fountain"),
            &dir.join("Script.fountain"),
        );
        copy_keeping_time(&b.sb.path("built/Script.epub"), &dir.join("Script.epub"));
        let s = |p: &Path| p.to_string_lossy().into_owned();
        Case {
            epub: s(&dir.join("Script.epub")),
            fountain: s(&dir.join("Script.fountain")),
            dir: s(&dir),
            sb,
        }
    }

    fn join(&self, name: &str) -> String {
        common::join(&self.dir, name)
    }

    /// `{ epub, for }`.
    fn opts(&self, for_: &str) -> ExportOptions {
        ExportOptions {
            epub: self.epub.clone(),
            for_: Some(for_.into()),
            ..Default::default()
        }
    }

    /// `{ epub, for, fountain }`.
    fn with_script(&self, for_: &str) -> ExportOptions {
        ExportOptions {
            fountain: Some(self.fountain.clone()),
            ..self.opts(for_)
        }
    }

    /// An `.<ext>` beside the book, a minute newer than it: current. An
    /// .azw3 is current only when this writer revision made it, so its
    /// stand-in carries the stamp.
    fn fresh_sibling(&self, ext: &str) -> String {
        let path = self.join(&format!("Script.{ext}"));
        let bytes = if ext == "azw3" {
            current_azw3()
        } else {
            format!("fake-{ext}-bytes").into_bytes()
        };
        std::fs::write(&path, bytes).unwrap();
        set_mtime(&path, mtime(&self.epub) + Duration::from_secs(60));
        path
    }

    /// Put a file a minute in the past.
    fn older(&self, path: &str) {
        set_mtime(path, SystemTime::now() - Duration::from_secs(60));
    }
}

/// ExportDeps with only the environment given: every probe real, but
/// Calibre at the guard path.
fn plain<'a>() -> ExportDeps<'a> {
    ExportDeps {
        env: Some(guarded_env()),
        ..Default::default()
    }
}

/// `{ calibreAvailable: () => calibre, kfxStatus: async () => status }`.
fn probes<'a>(calibre: bool, status: KfxStatus) -> ExportDeps<'a> {
    ExportDeps {
        calibre_available: Some(Box::new(move || calibre)),
        kfx_status: Some(Box::new(move || status)),
        ..plain()
    }
}

fn run(opts: &ExportOptions, deps: &ExportDeps) -> Result<ExportResult, EngineError> {
    export_command(opts, deps, &Books)
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("read")
}

fn read_text(path: &str) -> String {
    std::fs::read_to_string(path).expect("read")
}

fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

fn strs(formats: &[sp_engine::export::formats::ExportFormat]) -> Vec<&'static str> {
    formats.iter().map(|f| f.as_str()).collect()
}

/// A ladder that records what it was handed and answers `path`.
fn ladder_into<'a>(
    seen: &'a RefCell<Option<FreshKindleArtifactOptions>>,
    stage: Option<&'static str>,
    path: String,
) -> Box<sp_engine::commands::export::KindleLadder<'a>> {
    Box::new(move |opts, on_stage| {
        *seen.borrow_mut() = Some(opts.clone());
        if let Some(s) = stage {
            on_stage(s);
        }
        Ok(path.clone())
    })
}

/// The format the ladder was handed, for a `--for kindle` run whose
/// ladder answers an .azw3 beside the book.
fn format_seen(c: &Case, opts: &ExportOptions, app_settings: Option<String>) -> FormatOptions {
    let seen = RefCell::new(None);
    let mut deps = probes(true, CALIBRE_ONLY);
    deps.app_settings_path = app_settings;
    deps.fresh_kindle_artifact = Some(ladder_into(&seen, None, c.join("Script.azw3")));
    run(opts, &deps).expect("exported");
    drop(deps);
    seen.into_inner().expect("the ladder ran").format
}

#[test]
fn the_shared_epub_is_a_real_one_the_engine_built() {
    let b = built();
    assert_eq!(b.answer["ok"], json!(true));
    assert_eq!(
        b.answer["epubPath"],
        Value::String(b.sb.p("built/Script.epub"))
    );
    // An EPUB is a zip whose first entry is the uncompressed mimetype.
    let c = Case::new();
    let bytes = read(&c.epub);
    assert_eq!(&bytes[0..2], b"PK");
    assert_eq!(&bytes[30..58], b"mimetypeapplication/epub+zip");
}

// ---- exportCommand ----

#[test]
fn epub_is_always_available_and_is_returned_as_itself() {
    let c = Case::new();
    let result = run(&c.opts("epub"), &plain()).expect("exported");
    assert_eq!(result.path, c.epub);
    assert_eq!(result.format.as_str(), "epub");
    assert_eq!(result.extension, "epub");
    assert!(
        result.label.contains("for emailing to Kindle"),
        "{}",
        result.label
    );
    assert!(strs(&result.available).contains(&"epub"));
}

#[test]
fn epub_format_does_no_ladder_work_so_it_reports_no_stages() {
    let c = Case::new();
    let result = run(&c.opts("epub"), &plain()).expect("exported");
    assert!(result.stages.is_empty());
}

#[test]
fn available_is_exactly_available_formats_the_epub_and_a_kindle_file_everywhere() {
    let c = Case::new();
    let result = run(&c.opts("epub"), &plain()).expect("exported");
    assert_eq!(result.available, available_formats());
    assert_eq!(strs(&result.available), ["epub", "kindle"]);
}

#[test]
fn kindle_rung_kfx_ready_selects_the_kfx_rung() {
    let c = Case::new();
    let fake = c.join("Script.kfx");
    std::fs::write(&fake, "fake-kfx-bytes").unwrap();
    let seen = RefCell::new(None);
    let mut deps = probes(true, KFX_READY);
    deps.fresh_kindle_artifact = Some(ladder_into(
        &seen,
        Some("converting to KFX (Kindle Previewer can take ~20s to start)…"),
        fake.clone(),
    ));
    let result = run(&c.with_script("kindle"), &deps).expect("exported");
    // The ladder must be HANDED the ready state.
    let seen = seen.borrow().clone().expect("the ladder ran");
    assert!(seen.kfx_ready);
    assert!(seen.calibre_available);
    assert_eq!(result.path, fake);
    assert_eq!(result.extension, "kfx");
    assert!(result.label.contains("best quality"), "{}", result.label);
    assert!(result.stages.join(" ").contains("KFX"));
}

#[test]
fn kindle_rung_calibre_present_without_kfx_selects_the_azw3_rung() {
    let c = Case::new();
    let fake = c.join("Script.azw3");
    std::fs::write(&fake, "fake-azw3-bytes").unwrap();
    let seen = RefCell::new(None);
    let mut deps = probes(true, CALIBRE_ONLY);
    deps.fresh_kindle_artifact = Some(ladder_into(
        &seen,
        Some("building the AZW3 for Kindle…"),
        fake.clone(),
    ));
    let result = run(&c.with_script("kindle"), &deps).expect("exported");
    let seen = seen.borrow().clone().expect("the ladder ran");
    assert!(!seen.kfx_ready);
    assert!(seen.calibre_available);
    assert_eq!(result.path, fake);
    assert_eq!(result.extension, "azw3");
    assert!(!result.label.contains("best quality"));
    assert!(result.stages.join(" ").contains("AZW3"));
}

#[test]
fn kindle_rung_neither_calibre_nor_kfx_builds_our_own_azw3_from_the_epub() {
    // The REAL ladder and the real writer, only the probes forced.
    let c = Case::new();
    let mut opts = c.with_script("kindle");
    opts.options_json = Some(r#"{"showSceneNumbers":true}"#.into());
    let result = run(&opts, &probes(false, NO_TOOLCHAIN)).expect("exported");
    assert_eq!(result.extension, "azw3");
    assert!(result.label.contains("(beta)"), "{}", result.label);
    assert_eq!(result.path, c.join("Script.azw3"));
    assert_eq!(mobi_header_version(&read(&result.path)), 8);
    assert_eq!(result.stages, ["building the AZW3 for Kindle…"]);
}

#[test]
fn kindle_rung_the_azw3_needs_no_fountain_since_it_is_made_from_the_epub() {
    let c = Case::new();
    let result = run(&c.opts("kindle"), &probes(false, NO_TOOLCHAIN)).expect("exported");
    assert_eq!(result.extension, "azw3");
    assert!(exists(&result.path));
}

#[test]
fn a_stale_azw3_beside_the_epub_is_rebuilt_not_returned_as_is() {
    let c = Case::new();
    let azw3 = c.join("Script.azw3");
    std::fs::write(&azw3, "stale-placeholder").unwrap();
    c.older(&azw3);
    let result = run(&c.with_script("kindle"), &probes(false, NO_TOOLCHAIN)).expect("exported");
    assert_eq!(result.extension, "azw3");
    assert_eq!(result.path, azw3);
    assert_ne!(read(&result.path), b"stale-placeholder");
    assert!(result.stages.join(" ").contains("AZW3"));
}

#[test]
fn a_fresh_azw3_beside_the_epub_is_reused_with_no_invented_stages() {
    let c = Case::new();
    let azw3 = c.join("Script.azw3");
    std::fs::write(&azw3, current_azw3()).unwrap();
    let now = SystemTime::now();
    set_mtime(&c.epub, now - Duration::from_secs(10));
    set_mtime(&azw3, now);
    let result = run(&c.with_script("kindle"), &probes(false, NO_TOOLCHAIN)).expect("exported");
    assert_eq!(result.extension, "azw3");
    assert_eq!(result.path, azw3);
    assert_eq!(read(&result.path), current_azw3());
    assert!(result.stages.is_empty());
}

/// The REAL toolchain's answer on this machine, asked once, with Calibre
/// at the scratch copy. It decides whether the real-conversion test runs.
fn real_kfx_ready() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| kfx_status(&calibre_env()).ready)
}

#[test]
fn kindle_rung_on_the_real_ready_kfx_toolchain_a_real_kfx_file() {
    // The real, non-injected path: real probes, the real ladder, a real
    // Calibre conversion. It runs only where the KFX toolchain reports
    // ready (with Calibre at its scratch copy), and there it demands a
    // real .kfx; elsewhere the injected tests cover every rung.
    if !real_kfx_ready() {
        eprintln!("skipped: the KFX toolchain is not ready here");
        return;
    }
    let c = Case::new();
    let deps = ExportDeps {
        env: Some(calibre_env()),
        ..Default::default()
    };
    let result = run(&c.with_script("kindle"), &deps).expect("a real KFX");
    assert_eq!(result.extension, "kfx");
    assert_eq!(result.path, c.join("Script.kfx"));
    assert!(std::fs::metadata(&result.path).expect("there").len() > 0);
}

#[test]
fn a_missing_epub_is_unreadable_not_a_ladder_failure() {
    let c = Case::new();
    let mut opts = c.opts("kindle");
    opts.epub = c.join("nope.epub");
    assert_eq!(refusal(run(&opts, &plain())).0, "unreadable");
}

#[test]
fn a_missing_epub_names_the_path_not_a_generic_ladder_complaint() {
    let c = Case::new();
    let missing = c.join("nope.epub");
    let mut opts = c.opts("kindle");
    opts.epub = missing.clone();
    let (_, message) = refusal(run(&opts, &plain()));
    assert!(message.contains(&missing), "{message}");
}

#[test]
fn an_unknown_for_is_a_usage_error_naming_the_formats_it_accepts() {
    let c = Case::new();
    let (_, message) = refusal(run(&c.opts("pdf"), &plain()));
    assert!(message.contains("epub"), "{message}");
    assert!(message.contains("kindle"), "{message}");
}

#[test]
fn malformed_options_json_is_bad_options_not_export_failed() {
    let c = Case::new();
    let ladder_called = Cell::new(false);
    let fake = c.join("Script.azw3");
    let mut deps = probes(true, CALIBRE_ONLY);
    deps.fresh_kindle_artifact = Some(Box::new(|_, _| {
        ladder_called.set(true);
        Ok(fake.clone())
    }));
    let mut opts = c.with_script("kindle");
    opts.options_json = Some("not json".into());
    let (code, message) = refusal(run(&opts, &deps));
    assert_eq!(code, "bad-options");
    assert!(message.contains("--options-json"), "{message}");
    assert!(!ladder_called.get());
}

#[test]
fn options_json_that_parses_but_is_not_a_json_object_is_also_bad_options() {
    let c = Case::new();
    let mut opts = c.with_script("kindle");
    opts.options_json = Some("[1,2,3]".into());
    assert_eq!(refusal(run(&opts, &plain())).0, "bad-options");
}

#[test]
fn a_missing_epub_is_unreadable_even_when_options_json_is_also_malformed() {
    let c = Case::new();
    let mut opts = c.opts("kindle");
    opts.epub = c.join("nope.epub");
    opts.options_json = Some("not json".into());
    assert_eq!(refusal(run(&opts, &plain())).0, "unreadable");
}

// ---- app-wide defaults underneath --options-json ----

fn app_settings_with(c: &Case, name: &str, format_defaults: Value) -> String {
    let path = c.join(name);
    std::fs::write(
        &path,
        serde_json::to_string(&json!({ "formatDefaults": format_defaults })).unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn no_options_json_no_app_settings_the_ladder_gets_the_shipped_defaults() {
    let c = Case::new();
    let format = format_seen(
        &c,
        &c.with_script("kindle"),
        Some(c.join("no-such-settings.json")),
    );
    assert_eq!(format, DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn no_options_json_app_defaults_set_the_ladder_gets_them_not_the_shipped_defaults() {
    let c = Case::new();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let format = format_seen(&c, &c.with_script("kindle"), Some(settings));
    assert_eq!(format.dialogue_side_margin_pct, 5.0);
}

#[test]
fn a_partial_options_json_overlays_on_the_app_defaults_not_the_shipped_ones() {
    let c = Case::new();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let mut opts = c.with_script("kindle");
    opts.options_json = Some(r#"{"justifyText":true}"#.into());
    let format = format_seen(&c, &opts, Some(settings));
    assert!(format.justify_text);
    assert_eq!(format.dialogue_side_margin_pct, 5.0);
}

#[test]
fn a_full_options_json_wins_outright_same_as_before_this_change() {
    let c = Case::new();
    let full = FormatOptions {
        dialogue_side_margin_pct: 1.0,
        ..DEFAULT_FORMAT_OPTIONS
    };
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let mut opts = c.with_script("kindle");
    opts.options_json = Some(serde_json::to_string(&full).unwrap());
    assert_eq!(format_seen(&c, &opts, Some(settings)), full);
}

#[test]
fn with_no_app_settings_path_given_it_defaults_to_the_production_path_test_guarded() {
    // No path: the production one, under the guarded settings folder.
    let c = Case::new();
    let format = format_seen(&c, &c.with_script("kindle"), None);
    assert_eq!(format, DEFAULT_FORMAT_OPTIONS);
}

// ---- the script's own sidecar, when --fountain names one ----

#[test]
fn no_options_json_the_ladder_gets_the_sidecars_own_value_not_the_app_defaults() {
    let c = Case::new();
    std::fs::write(
        c.join("Script.method-reader.json"),
        r#"{"dialogueSideMarginPct":13}"#,
    )
    .unwrap();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let format = format_seen(&c, &c.with_script("kindle"), Some(settings));
    assert_eq!(format.dialogue_side_margin_pct, 13.0);
}

#[test]
fn a_partial_options_json_moves_the_named_knob_and_leaves_the_sidecars_other_values_standing() {
    let c = Case::new();
    std::fs::write(
        c.join("Script.method-reader.json"),
        r#"{"dialogueSideMarginPct":13,"justifyText":true}"#,
    )
    .unwrap();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let mut opts = c.with_script("kindle");
    opts.options_json = Some(r#"{"dialogueSideMarginPct":24}"#.into());
    let format = format_seen(&c, &opts, Some(settings));
    assert_eq!(format.dialogue_side_margin_pct, 24.0);
    assert!(format.justify_text);
}

#[test]
fn no_sidecar_on_disk_falls_back_to_the_app_defaults_not_the_shipped_ones() {
    let c = Case::new();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let format = format_seen(&c, &c.with_script("kindle"), Some(settings));
    assert_eq!(format.dialogue_side_margin_pct, 5.0);
}

#[test]
fn no_fountain_at_all_still_the_app_defaults_unaffected_by_this_fix() {
    let c = Case::new();
    let settings = app_settings_with(&c, "app.json", json!({ "dialogueSideMarginPct": 5 }));
    let format = format_seen(&c, &c.opts("kindle"), Some(settings));
    assert_eq!(format.dialogue_side_margin_pct, 5.0);
}

// ---- --check: which Kindle file, and whether it is built, without building it ----

/// Probes, and a ladder that fails the test if it runs.
fn never<'a>(calibre: bool, status: KfxStatus) -> ExportDeps<'a> {
    ExportDeps {
        fresh_kindle_artifact: Some(Box::new(|_, _| {
            panic!("the ladder was not expected to run")
        })),
        ..probes(calibre, status)
    }
}

fn checking(mut opts: ExportOptions) -> ExportOptions {
    opts.check = true;
    opts
}

#[test]
fn kfx_ready_and_no_kfx_yet_kfx_not_fresh_built_by_calibre_and_nothing_is_built() {
    let c = Case::new();
    let result = run(&checking(c.opts("kindle")), &never(true, KFX_READY)).expect("checked");
    assert_eq!(result.path, c.join("Script.kfx"));
    assert_eq!(result.format.as_str(), "kindle");
    assert_eq!(result.extension, "kfx");
    assert_eq!(result.fresh, Some(false));
    assert_eq!(result.built_by, Some(BuiltBy::Calibre));
    assert!(result.stages.is_empty());
    assert_eq!(result.book_date, Some(Some(mtime_ms(&c.epub))));
    assert!(result.label.contains("best quality"));
    assert!(!exists(&c.join("Script.kfx")));
}

#[test]
fn an_azw3_answer_says_it_is_beta_so_the_window_can_mark_the_file_a_kfx_answer_does_not() {
    let c = Case::new();
    let azw3 = run(&checking(c.opts("kindle")), &never(false, NO_TOOLCHAIN)).expect("checked");
    assert!(azw3.beta);
    assert_eq!(azw3.to_json()["beta"], json!(true));
    let kfx = run(&checking(c.opts("kindle")), &never(true, KFX_READY)).expect("checked");
    assert!(kfx.to_json().get("beta").is_none());
    let epub = run(&checking(c.opts("epub")), &plain()).expect("checked");
    assert!(epub.to_json().get("beta").is_none());
}

#[test]
fn calibre_alone_with_an_azw3_newer_than_the_book_azw3_fresh_and_method_reader_builds_it() {
    let c = Case::new();
    c.older(&c.epub);
    std::fs::write(c.join("Script.azw3"), current_azw3()).unwrap();
    let result = run(&checking(c.opts("kindle")), &never(true, CALIBRE_ONLY)).expect("checked");
    assert_eq!(result.extension, "azw3");
    assert_eq!(result.fresh, Some(true));
    assert_eq!(result.built_by, Some(BuiltBy::Screepub));
}

#[test]
fn no_toolchain_our_own_azw3_which_method_reader_builds_itself_and_nothing_is_built() {
    let c = Case::new();
    let result = run(
        &checking(c.with_script("kindle")),
        &never(false, NO_TOOLCHAIN),
    )
    .expect("checked");
    assert_eq!(result.extension, "azw3");
    assert_eq!(result.fresh, Some(false));
    assert_eq!(result.built_by, Some(BuiltBy::Screepub));
    assert!(!exists(&c.join("Script.azw3")));
}

#[test]
fn an_azw3_older_than_the_book_is_not_fresh() {
    let c = Case::new();
    let azw3 = c.join("Script.azw3");
    std::fs::write(&azw3, "azw3").unwrap();
    c.older(&azw3);
    let result = run(&checking(c.opts("kindle")), &never(true, CALIBRE_ONLY)).expect("checked");
    assert_eq!(result.fresh, Some(false));
}

#[test]
fn the_epub_is_the_book_itself_always_there_and_current_and_nothing_builds_it() {
    let c = Case::new();
    let deps = ExportDeps {
        calibre_available: Some(Box::new(|| false)),
        ..plain()
    };
    let result = run(&checking(c.opts("epub")), &deps).expect("checked");
    assert_eq!(result.path, c.epub);
    assert_eq!(result.extension, "epub");
    assert_eq!(result.fresh, Some(true));
    assert_eq!(result.built_by, None);
}

#[test]
fn check_writes_nothing_so_out_beside_it_is_refused_before_anything_is_probed() {
    let c = Case::new();
    let probed = Cell::new(0);
    let deps = ExportDeps {
        calibre_available: Some(Box::new(|| {
            probed.set(probed.get() + 1);
            true
        })),
        kfx_status: Some(Box::new(|| {
            probed.set(probed.get() + 1);
            KFX_READY
        })),
        fresh_kindle_artifact: Some(Box::new(|_, _| {
            panic!("the ladder was not expected to run")
        })),
        ..plain()
    };
    // Absolute or not: --check takes no --out at all.
    for out in [c.join("Copy.kfx"), "Copy.kfx".to_owned()] {
        let mut opts = checking(c.opts("kindle"));
        opts.out = Some(out.clone());
        let (code, message) = refusal(run(&opts, &deps));
        assert_eq!(format!("{out}: {code}"), format!("{out}: usage"));
        assert!(message.contains("--check"), "{message}");
    }
    assert_eq!(probed.get(), 0);
}

#[test]
fn through_the_cli_one_object_the_extension_this_machine_makes_and_no_file_beside_the_book() {
    // The real probes, so Calibre is asked for its plugins: at the scratch
    // copy of its folder, never the real one.
    let mut c = Case::new();
    c.sb.set("CALIBRE_CONFIG_DIRECTORY", calibre_copy_str());
    let before = names(&c.dir);
    let r =
        c.sb.run(&["export", &c.epub, "--for", "kindle", "--check", "--json"]);
    let answer = r.json();
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let extension = answer["extension"].as_str().unwrap_or_default();
    assert!(["kfx", "azw3", "mobi"].contains(&extension), "{answer}");
    assert!(answer["fresh"].is_boolean());
    // Only KFX still comes from Calibre: the AZW3 is Method Reader's own.
    let by = if extension == "kfx" {
        "calibre"
    } else {
        "screepub"
    };
    assert_eq!(answer["builtBy"], json!(by));
    assert_eq!(answer["bookDate"].as_f64(), Some(mtime_ms(&c.epub)));
    assert_eq!(names(&c.dir), before);
}

#[test]
fn export_help_names_check() {
    let r = Sandbox::new().run(&["export", "--help"]);
    assert!(r.stdout.contains("--check"));
}

// ---- method-reader export (through the CLI) ----

#[test]
fn json_prints_one_object_with_the_path_and_the_available_formats() {
    let c = Case::new();
    let r = c.sb.run(&["export", &c.epub, "--for", "epub", "--json"]);
    let answer = r.json();
    assert_eq!(answer["ok"], json!(true), "{answer}");
    assert_eq!(answer["path"], Value::String(c.epub.clone()));
    assert!(
        answer["available"]
            .as_array()
            .unwrap()
            .contains(&json!("epub"))
    );
}

#[test]
fn a_failure_is_one_json_object_with_export_failed_or_unreadable() {
    let c = Case::new();
    let r =
        c.sb.run(&["export", &c.join("nope.epub"), "--for", "kindle", "--json"]);
    assert_eq!(r.code, 1);
    let answer = r.json();
    assert_eq!(answer["ok"], json!(false));
    let code = answer["error"]["code"].as_str().unwrap_or_default();
    assert!(["unreadable", "export-failed"].contains(&code), "{answer}");
    let mut keys: Vec<&String> = answer["error"].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["code", "message"]);
    assert!(!answer["error"]["message"].as_str().unwrap().is_empty());
}

#[test]
fn a_file_literally_named_export_shadows_the_verb_and_is_not_converted() {
    // The bare verb word as argv[0], the working folder holding the file.
    let c = Case::new();
    std::fs::write(c.join("export"), "not a screenplay").unwrap();
    let r = c.sb.run_in(Path::new(&c.dir), &["export", "--json"]);
    let answer = r.json();
    assert_eq!(answer["ok"], json!(false));
    assert_ne!(answer["error"]["code"], json!("usage"), "{answer}");
}

// ---- --options-json validity does not depend on --for ----

#[test]
fn malformed_json_is_bad_options_on_the_epub_rung_too() {
    let c = Case::new();
    let mut opts = c.opts("epub");
    opts.options_json = Some("{oops".into());
    assert_eq!(refusal(run(&opts, &plain())).0, "bad-options");
}

#[test]
fn malformed_json_is_bad_options_on_the_kindle_rung() {
    let c = Case::new();
    let mut opts = c.opts("kindle");
    opts.options_json = Some("{oops".into());
    assert_eq!(
        refusal(run(&opts, &probes(false, NO_TOOLCHAIN))).0,
        "bad-options"
    );
}

// ---- exportCommand --out ----

fn with_out(mut opts: ExportOptions, out: &str) -> ExportOptions {
    opts.out = Some(out.into());
    opts
}

#[test]
fn epub_is_copied_to_a_new_nested_absolute_path_parents_created_bytes_equal_answer_path_is_out() {
    let c = Case::new();
    let out = c.join("nested/deeper/Saved.epub");
    let result = run(&with_out(c.opts("epub"), &out), &plain()).expect("copied");
    assert_eq!(result.path, out);
    assert_eq!(read(&out), read(&c.epub));
}

#[test]
fn a_relative_out_is_refused_on_the_epub_rung_naming_absolute() {
    let c = Case::new();
    let (code, message) = refusal(run(&with_out(c.opts("epub"), "relative.epub"), &plain()));
    assert_eq!(code, "usage");
    assert!(message.contains("absolute"), "{message}");
}

#[test]
fn a_relative_out_is_refused_before_the_kindle_ladder_runs_zero_ladder_calls() {
    let c = Case::new();
    let ladder_calls = Cell::new(0);
    let calibre_calls = Cell::new(0);
    let fake = c.join("Script.mobi");
    let deps = ExportDeps {
        calibre_available: Some(Box::new(|| {
            calibre_calls.set(calibre_calls.get() + 1);
            false
        })),
        kfx_status: Some(Box::new(|| NO_TOOLCHAIN)),
        fresh_kindle_artifact: Some(Box::new(|_, _| {
            ladder_calls.set(ladder_calls.get() + 1);
            Ok(fake.clone())
        })),
        ..plain()
    };
    let (code, message) = refusal(run(
        &with_out(c.with_script("kindle"), "relative/path.mobi"),
        &deps,
    ));
    assert_eq!(code, "usage");
    assert!(message.contains("absolute"), "{message}");
    assert_eq!(calibre_calls.get(), 0);
    assert_eq!(ladder_calls.get(), 0);
}

#[test]
fn the_wrong_extension_for_epub_is_refused_before_any_copy_naming_epub() {
    let c = Case::new();
    let out = c.join("Saved.mobi");
    let (code, message) = refusal(run(&with_out(c.opts("epub"), &out), &plain()));
    assert_eq!(code, "usage");
    assert_eq!(
        message,
        "that is an EPUB file: choose a name ending in .epub"
    );
    assert!(!exists(&out));
}

/// Calibre without KFX, and a ladder that answers `fake`, counting calls.
fn azw3_ladder<'a>(fake: &'a str, calls: &'a Cell<u32>) -> ExportDeps<'a> {
    ExportDeps {
        fresh_kindle_artifact: Some(Box::new(move |_, _| {
            calls.set(calls.get() + 1);
            Ok(fake.to_owned())
        })),
        ..probes(true, CALIBRE_ONLY)
    }
}

#[test]
fn kindle_rung_out_ending_in_the_produced_extension_azw3_copies() {
    let c = Case::new();
    let fake = c.join("Script.azw3");
    std::fs::write(&fake, "fake-azw3-bytes").unwrap();
    let out = c.join("ForKindle.azw3");
    let calls = Cell::new(0);
    let result = run(
        &with_out(c.with_script("kindle"), &out),
        &azw3_ladder(&fake, &calls),
    )
    .expect("copied");
    assert_eq!(result.path, out);
    assert_eq!(read_text(&out), "fake-azw3-bytes");
}

#[test]
fn kindle_rung_out_ending_kfx_is_refused_naming_azw3_after_the_ladder_ran() {
    let c = Case::new();
    let fake = c.join("Script.azw3");
    std::fs::write(&fake, "fake-azw3-bytes").unwrap();
    let out = c.join("ForKindle.kfx");
    let calls = Cell::new(0);
    let (code, message) = refusal(run(
        &with_out(c.with_script("kindle"), &out),
        &azw3_ladder(&fake, &calls),
    ));
    assert_eq!(code, "usage");
    assert_eq!(
        message,
        "that is an AZW3 file: choose a name ending in .azw3"
    );
    // The extension cannot be known until the ladder has run once.
    assert_eq!(calls.get(), 1);
    assert!(!exists(&out));
}

#[test]
fn an_existing_file_at_out_is_replaced() {
    let c = Case::new();
    let out = c.join("Existing.epub");
    std::fs::write(&out, "stale-bytes-that-must-go").unwrap();
    let result = run(&with_out(c.opts("epub"), &out), &plain()).expect("copied");
    assert_eq!(result.path, out);
    assert_eq!(read(&out), read(&c.epub));
}

#[test]
fn no_temp_file_is_left_beside_the_destination_on_success() {
    let c = Case::new();
    let dest = c.join("clean-success");
    std::fs::create_dir(&dest).unwrap();
    let out = common::join(&dest, "Saved.epub");
    run(&with_out(c.opts("epub"), &out), &plain()).expect("copied");
    assert_eq!(names(&dest), ["Saved.epub"]);
}

#[test]
fn no_temp_file_is_left_beside_the_destination_on_failure_and_it_answers_export_failed() {
    let c = Case::new();
    let dest = c.join("clean-failure");
    std::fs::create_dir(&dest).unwrap();
    let out = common::join(&dest, "Saved.epub");
    std::fs::create_dir(&out).unwrap();
    let (code, _) = refusal(run(&with_out(c.opts("epub"), &out), &plain()));
    assert_eq!(code, "export-failed");
    assert_eq!(names(&dest), ["Saved.epub"]);
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_written_says_where_the_copy_was_going_never_the_temp_file() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::process::Command::new("id")
        .arg("-u")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0");
    if root {
        eprintln!("skipped: chmod does not stop root");
        return;
    }
    let c = Case::new();
    let dest = c.join("read-only");
    std::fs::create_dir(&dest).unwrap();
    let out = common::join(&dest, "Saved.epub");
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o555)).unwrap();
    let answer = run(&with_out(c.opts("epub"), &out), &plain());
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, message) = refusal(answer);
    assert_eq!(code, "export-failed");
    let head = format!("could not save the copy to {out}: ");
    assert!(message.starts_with(&head), "{message}");
    assert!(message.len() > head.len(), "{message}");
    assert!(!message.contains(".tmp"), "{message}");
    assert!(!message.contains(".Saved.epub."), "{message}");
    assert!(names(&dest).is_empty());
}

#[test]
fn a_copy_that_fails_at_the_rename_names_the_chosen_path_too_never_the_temp_file() {
    let c = Case::new();
    let dest = c.join("rename-failure");
    std::fs::create_dir(&dest).unwrap();
    let out = common::join(&dest, "Saved.epub");
    std::fs::create_dir(&out).unwrap();
    let (code, message) = refusal(run(&with_out(c.opts("epub"), &out), &plain()));
    assert_eq!(code, "export-failed");
    assert!(
        message.starts_with(&format!("could not save the copy to {out}: ")),
        "{message}"
    );
    assert!(!message.contains(".tmp"), "{message}");
}

#[test]
fn out_equal_to_the_epub_artifacts_own_path_answers_without_copying() {
    let c = Case::new();
    let past = SystemTime::now() - Duration::from_secs(60);
    set_mtime(&c.epub, past);
    let result = run(&with_out(c.opts("epub"), &c.epub), &plain()).expect("answered");
    assert_eq!(result.path, c.epub);
    assert_eq!(mtime(&c.epub), past);
}

#[test]
fn kindle_rung_out_equal_to_the_produced_path_answers_without_copying() {
    let c = Case::new();
    let fake = c.join("Script.azw3");
    std::fs::write(&fake, "fake-azw3-bytes").unwrap();
    let past = SystemTime::now() - Duration::from_secs(60);
    set_mtime(&fake, past);
    let calls = Cell::new(0);
    let result = run(
        &with_out(c.with_script("kindle"), &fake),
        &azw3_ladder(&fake, &calls),
    )
    .expect("answered");
    assert_eq!(result.path, fake);
    assert_eq!(mtime(&fake), past);
}

#[test]
fn the_copy_goes_through_a_temp_name_and_a_rename_not_a_direct_write() {
    // Pinned at the source, as the TypeScript does: sp-engine's
    // copy_artifact_atomic writes a temp name in the destination folder,
    // then renames it over.
    let src = source("rust/crates/sp-engine/src/commands/export.rs");
    let from = src
        .find("fn copy_artifact_atomic(")
        .expect("copy_artifact_atomic moved");
    let body = &src[from..from + src[from..].find("\n}\n").expect("its end")];
    assert!(body.contains("fsx::temp_path_for(destination)"), "{body}");
    assert!(body.contains("fsx::copy(source, &tmp)"), "{body}");
    assert!(body.contains("fsx::rename(&tmp, destination)"), "{body}");
}

// ---- method-reader export --out (through the CLI) ----

#[test]
fn epub_to_a_new_absolute_path_through_the_spawned_cli() {
    let c = Case::new();
    let out = c.join("cli-nested/Saved.epub");
    let r =
        c.sb.run(&["export", &c.epub, "--for", "epub", "--out", &out, "--json"]);
    let answer = r.json();
    assert_eq!(answer["ok"], json!(true));
    assert_eq!(answer["path"], Value::String(out.clone()));
    assert!(exists(&out));
}

#[test]
fn export_help_mentions_out() {
    let answer = Sandbox::new().run(&["export", "--help", "--json"]).json();
    assert_eq!(answer["ok"], json!(true));
    assert!(answer["usage"].as_str().unwrap().contains("--out"));
}

// ---- export --for one named file type ----
//
// Each explicit value builds only itself, never the ladder. Every test
// injects the toolchain answers AND the two rung builders, so none can
// reach a real Calibre: the seams that must not be used panic if asked.

/// Rung builders: each answers its path when given one, and panics when it
/// was not expected to run.
#[derive(Default)]
struct Builders {
    kfx: Option<String>,
    azw3: Option<String>,
    built: RefCell<Vec<String>>,
}

impl RungBuilders for Builders {
    fn kfx(&self, epub: &str, on_stage: &mut dyn FnMut(&str)) -> Result<String, EngineError> {
        let path = self.kfx.clone().expect("the KFX builder must not run here");
        self.built.borrow_mut().push(epub.to_owned());
        on_stage("converting to KFX");
        Ok(path)
    }

    fn azw3(&self, epub: &str) -> Result<String, EngineError> {
        let path = self
            .azw3
            .clone()
            .expect("the AZW3 builder must not run here");
        self.built.borrow_mut().push(epub.to_owned());
        Ok(path)
    }
}

/// `{ calibreAvailable, kfxStatus, freshKindleArtifact: ladderNeverRun,
/// rungBuilders }`; `status: None` is `kfxNeverAsked`.
fn safe<'a>(calibre: bool, status: Option<KfxStatus>, builders: &'a Builders) -> ExportDeps<'a> {
    ExportDeps {
        calibre_available: Some(Box::new(move || calibre)),
        kfx_status: Some(Box::new(move || {
            status.expect("kfxStatus must not be asked for this value")
        })),
        fresh_kindle_artifact: Some(Box::new(|_, _| {
            panic!("the Kindle ladder must not run for this value")
        })),
        rung_builders: Some(builders),
        ..plain()
    }
}

#[test]
fn fountain_returns_the_script_itself_and_needs_no_toolchain() {
    let c = Case::new();
    let b = Builders::default();
    let result = run(&c.with_script("fountain"), &safe(false, None, &b)).expect("exported");
    assert_eq!(result.path, c.fountain);
    assert_eq!(result.format.as_str(), "fountain");
    assert_eq!(result.extension, "fountain");
    assert!(result.stages.is_empty());
}

#[test]
fn fountain_with_out_writes_a_byte_for_byte_copy_there() {
    let c = Case::new();
    let b = Builders::default();
    let out = c.join("saved/Copy.fountain");
    let result = run(
        &with_out(c.with_script("fountain"), &out),
        &safe(false, None, &b),
    )
    .expect("copied");
    assert_eq!(result.path, out);
    assert_eq!(read(&out), read(&c.fountain));
}

/// A script carrying Method Reader's own notes (registry #18, #19): the cache
/// needs them, a screenwriting app does not.
const NOTED: &str = "INT. HUT - NIGHT\n\n[[fmt: sans +1]]\nCHYRON: LIVE.\n\n@MARA\n[[fmt: mono]] Read it back.\n\n[[img: p002-01.png w=71]]\n\nShe looks up. [[a writer's own note]]\n";

#[test]
fn fountain_with_out_leaves_method_readers_own_notes_behind_and_the_script_beside_the_book_keeps_them()
 {
    let c = Case::new();
    std::fs::write(&c.fountain, NOTED).unwrap();
    let b = Builders::default();
    let out = c.join("saved/Copy.fountain");
    let result = run(
        &with_out(c.with_script("fountain"), &out),
        &safe(false, None, &b),
    )
    .expect("copied");
    assert_eq!(result.path, out);
    assert_eq!(
        read_text(&out),
        "INT. HUT - NIGHT\n\nCHYRON: LIVE.\n\n@MARA\nRead it back.\n\nShe looks up. [[a writer's own note]]\n"
    );
    // The cache boundary is untouched.
    assert_eq!(read_text(&c.fountain), NOTED);
    assert_eq!(names(c.join("saved")), ["Copy.fountain"]);
}

#[test]
fn fountain_out_onto_the_books_own_script_is_refused_when_the_script_carries_notes_and_the_script_is_left_alone()
 {
    let c = Case::new();
    std::fs::write(&c.fountain, NOTED).unwrap();
    let b = Builders::default();
    let (code, message) = refusal(run(
        &with_out(c.with_script("fountain"), &c.fountain),
        &safe(false, None, &b),
    ));
    assert_eq!(code, "export-failed");
    assert!(message.contains(&c.fountain), "{message}");
    assert_eq!(read_text(&c.fountain), NOTED);
}

#[test]
fn fountain_out_onto_the_books_own_script_with_no_notes_in_it_is_already_there() {
    let c = Case::new();
    let before = read(&c.fountain);
    let b = Builders::default();
    let result = run(
        &with_out(c.with_script("fountain"), &c.fountain),
        &safe(false, None, &b),
    )
    .expect("already there");
    assert_eq!(result.path, c.fountain);
    assert_eq!(read(&c.fountain), before);
}

#[test]
fn fountain_without_fountain_is_usage() {
    let c = Case::new();
    let b = Builders::default();
    assert_eq!(
        refusal(run(&c.opts("fountain"), &safe(false, None, &b))).0,
        "usage"
    );
}

#[test]
fn fountain_check_says_the_script_is_current_and_builds_nothing() {
    let c = Case::new();
    let b = Builders::default();
    let result =
        run(&checking(c.with_script("fountain")), &safe(false, None, &b)).expect("checked");
    assert_eq!(result.path, c.fountain);
    assert_eq!(result.extension, "fountain");
    assert_eq!(result.fresh, Some(true));
}

#[test]
fn mobi_rebuilds_the_engine_mobi_beside_the_epub_never_through_calibre() {
    let c = Case::new();
    let b = Builders::default();
    let result = run(&c.with_script("mobi"), &safe(true, None, &b)).expect("built");
    let mobi = c.join("Script.mobi");
    assert_eq!(result.path, mobi);
    assert_eq!(result.format.as_str(), "mobi");
    assert_eq!(result.extension, "mobi");
    // A real MOBI: the PalmDB header's type and creator.
    assert_eq!(&read(&mobi)[60..68], b"BOOKMOBI");
    assert!(!exists(&c.join("Script.azw3")));
    assert!(!exists(&c.join("Script.kfx")));
}

#[test]
fn mobi_without_fountain_is_usage() {
    let c = Case::new();
    let b = Builders::default();
    assert_eq!(
        refusal(run(&c.opts("mobi"), &safe(false, None, &b))).0,
        "usage"
    );
}

#[test]
fn mobi_check_reports_the_mobi_rung_even_on_a_machine_that_has_kfx_ready() {
    let c = Case::new();
    let b = Builders::default();
    let result = run(
        &checking(c.with_script("mobi")),
        &safe(true, Some(KFX_READY), &b),
    )
    .expect("checked");
    assert_eq!(result.path, c.join("Script.mobi"));
    assert_eq!(result.extension, "mobi");
    assert_eq!(result.fresh, Some(false));
    assert_eq!(result.built_by, Some(BuiltBy::Screepub));
}

#[test]
fn fountain_refuses_a_fountain_file_that_is_not_a_fountain_as_usage_and_writes_nothing() {
    let c = Case::new();
    let not_script = c.join("Script.txt");
    std::fs::write(&not_script, "Title: Test\n").unwrap();
    let out = c.join("saved/Copy.fountain");
    let mut opts = with_out(c.opts("fountain"), &out);
    opts.fountain = Some(not_script);
    let b = Builders::default();
    let (code, message) = refusal(run(&opts, &safe(false, None, &b)));
    assert_eq!(code, "usage");
    assert!(message.contains(".fountain"), "{message}");
    assert!(!exists(&out));
}

#[test]
fn mobi_with_a_fountain_that_cannot_be_read_is_unreadable_before_check_and_before_any_probe() {
    let c = Case::new();
    let gone = c.join("Gone.fountain");
    let b = Builders::default();
    for check in [false, true] {
        let before = names(&c.dir);
        let mut deps = safe(false, None, &b);
        deps.calibre_available = Some(Box::new(|| {
            panic!("Calibre must not be probed for a script that cannot be read")
        }));
        let opts = ExportOptions {
            fountain: Some(gone.clone()),
            check,
            ..c.opts("mobi")
        };
        let (code, message) = refusal(run(&opts, &deps));
        assert_eq!(
            format!("check={check}: {code}"),
            format!("check={check}: unreadable")
        );
        assert_eq!(message, format!("cannot read the script to export: {gone}"));
        assert_eq!(names(&c.dir), before);
    }
}

#[test]
fn kfx_with_the_toolchain_not_ready_is_export_failed_names_what_is_missing_and_writes_nothing() {
    let c = Case::new();
    let before = names(&c.dir);
    let b = Builders::default();
    let (code, message) = refusal(run(
        &c.with_script("kfx"),
        &safe(true, Some(CALIBRE_ONLY), &b),
    ));
    assert_eq!(code, "export-failed");
    assert!(
        message.contains("this computer cannot make a KFX file yet"),
        "{message}"
    );
    assert!(message.contains("Kindle Previewer"), "{message}");
    assert!(message.contains("the KFX plugin"), "{message}");
    assert_eq!(names(&c.dir), before);
}

#[test]
fn kfx_never_falls_back_to_azw3_even_with_calibre_present() {
    let c = Case::new();
    let b = Builders::default();
    let (code, message) = refusal(run(
        &c.with_script("kfx"),
        &safe(true, Some(NO_TOOLCHAIN), &b),
    ));
    assert_eq!(code, "export-failed");
    assert!(message.contains("Calibre"), "{message}");
    assert!(!exists(&c.join("Script.azw3")));
}

#[test]
fn kfx_with_the_toolchain_ready_builds_through_the_kfx_builder_only() {
    let c = Case::new();
    let fake = c.join("Script.kfx");
    let b = Builders {
        kfx: Some(fake.clone()),
        ..Default::default()
    };
    let result = run(&c.with_script("kfx"), &safe(true, Some(KFX_READY), &b)).expect("built");
    assert_eq!(*b.built.borrow(), std::slice::from_ref(&c.epub));
    assert_eq!(result.path, fake);
    assert_eq!(result.format.as_str(), "kfx");
    assert_eq!(result.extension, "kfx");
    assert_eq!(result.stages, ["converting to KFX"]);
}

#[test]
fn azw3_builds_through_the_azw3_builder_even_when_kfx_is_ready() {
    let c = Case::new();
    let fake = c.join("Script.azw3");
    let b = Builders {
        azw3: Some(fake.clone()),
        ..Default::default()
    };
    let result = run(&c.with_script("azw3"), &safe(true, Some(KFX_READY), &b)).expect("built");
    assert_eq!(*b.built.borrow(), std::slice::from_ref(&c.epub));
    assert_eq!(result.path, fake);
    assert_eq!(result.format.as_str(), "azw3");
    assert_eq!(result.extension, "azw3");
}

#[test]
fn azw3_without_calibre_is_built_all_the_same_the_engine_writes_it() {
    let c = Case::new();
    let b = Builders {
        azw3: Some(c.join("Script.azw3")),
        ..Default::default()
    };
    let result = run(&c.opts("azw3"), &safe(false, None, &b)).expect("built");
    assert_eq!(*b.built.borrow(), std::slice::from_ref(&c.epub));
    assert_eq!(result.format.as_str(), "azw3");
    assert_eq!(result.extension, "azw3");
}

#[test]
fn azw3_check_with_a_fresh_sibling_reports_it_fresh_and_built_by_screepub_even_with_kfx_ready() {
    let c = Case::new();
    let azw3 = c.fresh_sibling("azw3");
    let b = Builders::default();
    let result = run(&checking(c.opts("azw3")), &safe(true, Some(KFX_READY), &b)).expect("checked");
    assert_eq!(result.path, azw3);
    assert_eq!(result.format.as_str(), "azw3");
    assert_eq!(result.extension, "azw3");
    assert_eq!(result.fresh, Some(true));
    assert_eq!(result.built_by, Some(BuiltBy::Screepub));
    assert_eq!(result.book_date, Some(Some(mtime_ms(&c.epub))));
}

#[test]
fn azw3_reuses_a_fresh_sibling_without_building() {
    let c = Case::new();
    let azw3 = c.fresh_sibling("azw3");
    let b = Builders::default();
    let result = run(&c.opts("azw3"), &safe(true, Some(KFX_READY), &b)).expect("reused");
    assert_eq!(result.path, azw3);
    assert_eq!(read(&azw3), current_azw3());
}

#[test]
fn kfx_check_with_a_fresh_sibling_reports_it_never_the_azw3_beside_it() {
    let c = Case::new();
    c.fresh_sibling("azw3");
    let kfx = c.fresh_sibling("kfx");
    let b = Builders::default();
    let result = run(&checking(c.opts("kfx")), &safe(true, Some(KFX_READY), &b)).expect("checked");
    assert_eq!(result.path, kfx);
    assert_eq!(result.extension, "kfx");
    assert_eq!(result.fresh, Some(true));
    assert_eq!(result.built_by, Some(BuiltBy::Calibre));
}

#[test]
fn an_unknown_value_is_usage_and_the_sentence_lists_all_six() {
    let c = Case::new();
    let b = Builders::default();
    let mut deps = safe(false, None, &b);
    deps.calibre_available = None;
    let (code, message) = refusal(run(&c.opts("nook"), &deps));
    assert_eq!(code, "usage");
    assert_eq!(
        message,
        "--for takes epub, kindle, kfx, azw3, mobi, fountain or fdx, not \"nook\""
    );
}

#[test]
fn out_with_the_wrong_extension_is_usage_for_every_new_value_before_any_work() {
    let c = Case::new();
    let b = Builders::default();
    let out = c.join("Wrong.epub");
    for value in ["kfx", "azw3", "mobi", "fountain"] {
        let (code, message) = refusal(run(
            &with_out(c.with_script(value), &out),
            &safe(true, Some(KFX_READY), &b),
        ));
        let article = if value == "azw3" { "an" } else { "a" };
        assert_eq!(
            format!("{value}: {code} {message}"),
            format!(
                "{value}: usage that is {article} {} file: choose a name ending in .{value}",
                value.to_uppercase()
            )
        );
    }
    assert!(!exists(&out));
    assert!(!exists(&c.join("Script.mobi")));
}

#[test]
fn export_help_lists_the_six_values() {
    let answer = Sandbox::new().run(&["export", "--help", "--json"]).json();
    assert!(
        answer["usage"]
            .as_str()
            .unwrap()
            .contains("epub|kindle|kfx|azw3|mobi|fountain")
    );
}
