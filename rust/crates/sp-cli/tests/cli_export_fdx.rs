//! Port of tests/cli-export-fdx.test.ts, its export-command and CLI
//! halves: `method-reader export <book.epub> --for fdx --fountain <f>`, the
//! editable screenplay written the way every other export write is,
//! atomically, and the conversion's own `--fdx <file>`.
//!
//! The `formats` block (fdx is its own extension and label) is in
//! sp-engine's tests/export_small.rs, and the renderer (`fountainToFdx`)
//! in sp-fdx's tests/fdx.rs. The handler is sp-engine's export_command,
//! called here with the TypeScript's seams: Calibre absent, and a KFX probe
//! and a Kindle ladder that fail the test if they are reached at all.

mod common;

use std::cell::Cell;
use std::time::{Duration, SystemTime};

use common::Sandbox;
use common::seams::{guarded_env, names, refusal, set_mtime};
use serde_json::Value;
use sp_engine::commands::export::{ExportDeps, ExportOptions, ExportResult, export_command};
use sp_engine::errors::EngineError;
use sp_engine::export::renderers::{Books, Renderers};

const FOUNTAIN: &str = "Title: Test Piece\n\nINT. ROOM - DAY\n\nMARGO\nHello *there*.\n";

/// One case's folder: a book (never read for an FDX, only required to
/// exist) and its script.
struct Case {
    sb: Sandbox,
    dir: String,
    epub: String,
    fountain: String,
    ladder_calls: Cell<u32>,
}

impl Case {
    fn new() -> Case {
        let sb = Sandbox::new();
        let dir = sb.p("case");
        let epub = sb.write("case/Script.epub", "not really an epub");
        let fountain = sb.write("case/Script.fountain", FOUNTAIN);
        Case {
            dir,
            epub: epub.to_string_lossy().into_owned(),
            fountain: fountain.to_string_lossy().into_owned(),
            sb,
            ladder_calls: Cell::new(0),
        }
    }

    fn join(&self, name: &str) -> String {
        common::join(&self.dir, name)
    }

    fn opts(&self, fountain: Option<&str>, out: Option<&str>, check: bool) -> ExportOptions {
        ExportOptions {
            epub: self.epub.clone(),
            for_: Some("fdx".into()),
            fountain: fountain.map(str::to_owned),
            options_json: None,
            out: out.map(str::to_owned),
            check,
        }
    }

    /// The TypeScript's `deps()`: Calibre absent; the KFX probe and the
    /// ladder must not run.
    fn deps(&self) -> ExportDeps<'_> {
        ExportDeps {
            calibre_available: Some(Box::new(|| false)),
            kfx_status: Some(Box::new(|| panic!("the KFX probe must not run for an FDX"))),
            fresh_kindle_artifact: Some(Box::new(|_, _| {
                self.ladder_calls.set(self.ladder_calls.get() + 1);
                Err(EngineError::plain(
                    "the Kindle ladder must not run for an FDX",
                ))
            })),
            rung_builders: None,
            app_settings_path: Some(self.join("app-settings.json")),
            env: Some(guarded_env()),
        }
    }

    fn export(&self, opts: &ExportOptions) -> Result<ExportResult, EngineError> {
        export_command(opts, &self.deps(), &Books)
    }
}

fn fdx_of(fountain: &str) -> String {
    Books.fountain_to_fdx(fountain).expect("an FDX")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).expect("read")
}

fn exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

fn tmp_names(dir: &str) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|f| f.ends_with(".tmp"))
        .collect()
}

/// The document element's name (`parseXml(..).name`).
fn root_element(xml: &str) -> String {
    let mut rest = xml;
    loop {
        let at = rest.find('<').expect("an element");
        rest = &rest[at..];
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[rest.find('>').expect("closed") + 1..];
            continue;
        }
        let end = rest[1..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .expect("a name")
            + 1;
        return rest[1..end].to_owned();
    }
}

// ---- exportCommand --for fdx ----

#[test]
fn writes_the_fdx_beside_the_book_by_default_built_from_the_fountain() {
    let c = Case::new();
    let result = c
        .export(&c.opts(Some(&c.fountain), None, false))
        .expect("written");
    let want = c.join("Script.fdx");
    assert_eq!(result.path, want);
    assert_eq!(result.format.as_str(), "fdx");
    assert_eq!(result.extension, "fdx");
    assert!(result.stages.is_empty());
    assert_eq!(read(&want), fdx_of(FOUNTAIN));
    assert_eq!(root_element(&read(&want)), "FinalDraft");
    assert_eq!(c.ladder_calls.get(), 0);
}

#[test]
fn out_writes_it_there_instead_parents_created_and_nothing_beside_the_book() {
    let c = Case::new();
    let out = c.join("nested/deeper/Saved.fdx");
    let result = c
        .export(&c.opts(Some(&c.fountain), Some(&out), false))
        .expect("written");
    assert_eq!(result.path, out);
    assert_eq!(read(&out), fdx_of(FOUNTAIN));
    assert!(!exists(&c.join("Script.fdx")));
}

#[test]
fn out_replaces_an_existing_file_and_leaves_no_temp_file_behind() {
    let c = Case::new();
    let out = c.join("Saved.fdx");
    std::fs::write(&out, "old").unwrap();
    c.export(&c.opts(Some(&c.fountain), Some(&out), false))
        .expect("written");
    assert_eq!(read(&out), fdx_of(FOUNTAIN));
    assert!(tmp_names(&c.dir).is_empty());
}

#[test]
fn without_fountain_it_is_a_usage_error_naming_fountain_and_writes_nothing() {
    let c = Case::new();
    let (code, message) = refusal(c.export(&c.opts(None, None, false)));
    assert_eq!(code, "usage");
    assert!(message.contains("--fountain"), "{message}");
    assert!(!exists(&c.join("Script.fdx")));
}

#[test]
fn a_missing_fountain_is_unreadable_and_names_the_path() {
    let c = Case::new();
    let missing = c.join("Gone.fountain");
    let (code, message) = refusal(c.export(&c.opts(Some(&missing), None, false)));
    assert_eq!(code, "unreadable");
    assert!(message.contains(&missing), "{message}");
}

#[test]
fn a_missing_fountain_is_refused_before_calibre_is_probed_or_settings_are_read() {
    let c = Case::new();
    let probed = Cell::new(0);
    let missing = c.join("Gone.fountain");
    let mut deps = c.deps();
    deps.calibre_available = Some(Box::new(|| {
        probed.set(probed.get() + 1);
        false
    }));
    let (code, message) = refusal(export_command(
        &c.opts(Some(&missing), None, false),
        &deps,
        &Books,
    ));
    assert_eq!(code, "unreadable");
    assert!(message.contains(&missing), "{message}");
    assert_eq!(probed.get(), 0);
}

#[test]
fn a_missing_book_is_unreadable_the_same_as_every_other_export() {
    let c = Case::new();
    let mut opts = c.opts(Some(&c.fountain), None, false);
    opts.epub = c.join("nope.epub");
    let (code, _) = refusal(c.export(&opts));
    assert_eq!(code, "unreadable");
}

#[test]
fn a_relative_out_is_refused_before_anything_is_written() {
    let c = Case::new();
    let (code, message) = refusal(c.export(&c.opts(Some(&c.fountain), Some("Saved.fdx"), false)));
    assert_eq!(code, "usage");
    assert!(message.contains("absolute"), "{message}");
}

#[test]
fn the_wrong_extension_on_out_is_refused_naming_fdx() {
    let c = Case::new();
    let out = c.join("Saved.epub");
    let (code, message) = refusal(c.export(&c.opts(Some(&c.fountain), Some(&out), false)));
    assert_eq!(code, "usage");
    assert!(message.contains(".fdx"), "{message}");
    assert!(!exists(&out));
}

#[test]
fn a_write_that_fails_is_export_failed_naming_the_chosen_path_and_not_the_temp_file() {
    let c = Case::new();
    // A directory where the file should go: the rename cannot replace it.
    let out = c.join("Blocked.fdx");
    std::fs::create_dir(&out).unwrap();
    let (code, message) = refusal(c.export(&c.opts(Some(&c.fountain), Some(&out), false)));
    assert_eq!(code, "export-failed");
    assert!(message.contains(&out), "{message}");
    assert!(!message.contains(".tmp"), "{message}");
    assert!(tmp_names(&c.dir).is_empty());
}

#[test]
fn check_builds_nothing_and_says_whether_the_fdx_beside_the_book_is_current() {
    let c = Case::new();
    let first = c
        .export(&c.opts(Some(&c.fountain), None, true))
        .expect("checked");
    assert_eq!(first.path, c.join("Script.fdx"));
    assert_eq!(first.fresh, Some(false));
    assert!(!exists(&first.path));

    c.export(&c.opts(Some(&c.fountain), None, false))
        .expect("written");
    let now = SystemTime::now();
    set_mtime(&c.fountain, now - Duration::from_secs(100));
    set_mtime(&first.path, now);
    let again = c
        .export(&c.opts(Some(&c.fountain), None, true))
        .expect("checked");
    assert_eq!(again.fresh, Some(true));

    // The script changed after the FDX was written.
    set_mtime(&c.fountain, now + Duration::from_secs(100));
    let stale = c
        .export(&c.opts(Some(&c.fountain), None, true))
        .expect("checked");
    assert_eq!(stale.fresh, Some(false));
}

#[test]
fn the_unknown_for_message_names_all_three_formats() {
    let c = Case::new();
    let mut opts = c.opts(None, None, false);
    opts.for_ = Some("pdf".into());
    let (code, message) = refusal(c.export(&opts));
    assert_eq!(code, "usage");
    for word in ["epub", "kindle", "fdx"] {
        assert!(message.contains(word), "{message}");
    }
}

// ---- method-reader export --for fdx (through the CLI) ----

#[test]
fn json_prints_one_object_with_the_path_and_the_file_is_there() {
    let c = Case::new();
    let out = c.join("Saved.fdx");
    let r = c.sb.run(&[
        "export",
        &c.epub,
        "--for",
        "fdx",
        "--fountain",
        &c.fountain,
        "--out",
        &out,
        "--json",
    ]);
    assert_eq!(r.code, 0, "{}", r.stdout);
    let answer = r.json();
    assert_eq!(answer["ok"], Value::Bool(true));
    assert_eq!(answer["path"], Value::String(out.clone()));
    assert_eq!(answer["format"], "fdx");
    assert_eq!(answer["extension"], "fdx");
    assert_eq!(read(&out), fdx_of(FOUNTAIN));
}

#[test]
fn a_refusal_is_one_json_object_with_a_code_and_a_message() {
    let c = Case::new();
    let r = c.sb.run(&["export", &c.epub, "--for", "fdx", "--json"]);
    assert_eq!(r.code, 1);
    let answer = r.json();
    assert_eq!(answer["ok"], Value::Bool(false));
    assert_eq!(answer["error"]["code"], "usage");
    let mut keys: Vec<&String> = answer["error"].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["code", "message"]);
}

#[test]
fn export_help_names_fdx() {
    let r = Sandbox::new().run(&["export", "--help"]);
    assert!(r.stdout.contains("fdx"));
}

// ---- method-reader <script> --fdx <file> ----

#[test]
fn a_pdf_conversion_writes_the_fdx_of_the_fountain_it_wrote_and_names_it_in_json() {
    let c = Case::new();
    let pdf = common::fixture("screenplay.pdf");
    let fdx = c.join("Converted.fdx");
    let r = c.sb.run(&[
        &pdf.to_string_lossy(),
        "-o",
        &c.join("Converted.epub"),
        "--fdx",
        &fdx,
        "--json",
    ]);
    assert_eq!(r.code, 0, "{} {}", r.stdout, r.stderr);
    let answer = r.json();
    assert_eq!(answer["fdxPath"], Value::String(fdx.clone()));
    let fountain = read(answer["fountainPath"].as_str().expect("a fountainPath"));
    assert_eq!(read(&fdx), fdx_of(&fountain));
}

#[test]
fn a_fountain_conversion_writes_the_fdx_of_that_fountain() {
    let c = Case::new();
    let fdx = c.join("FromFountain.fdx");
    let r =
        c.sb.run(&[&c.fountain, "-o", &c.join("Out.epub"), "--fdx", &fdx]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains(&format!("wrote {fdx}")), "{}", r.stdout);
    assert_eq!(read(&fdx), fdx_of(FOUNTAIN));
}

#[test]
fn without_fdx_no_fdx_is_written_and_the_answer_has_no_fdx_path() {
    let c = Case::new();
    let r =
        c.sb.run(&[&c.fountain, "-o", &c.join("Out.epub"), "--json"]);
    let answer = r.json();
    assert!(answer.get("fdxPath").is_none(), "{answer}");
    let fdx: Vec<String> = names(&c.dir)
        .into_iter()
        .filter(|f| f.ends_with(".fdx"))
        .collect();
    assert!(fdx.is_empty(), "{fdx:?}");
}

#[test]
fn help_names_fdx_file() {
    let r = Sandbox::new().run(&["--help"]);
    assert!(r.stdout.contains("--fdx <file>"));
}
