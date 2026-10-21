//! `method-reader export <book.epub> --for osf --fountain <f>`: the script
//! as an Open Screenplay Format document, the third editable script. It is
//! the .fdx's route with another writer behind it, so these cases are
//! cli_export_fdx.rs's, held to the same answers: written atomically beside
//! the book or at --out, rebuilt from the .fountain on every call, and
//! refused the same ways. The writer itself is sp-osf's (tests/osf.rs).

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

/// One case's folder: a book (never read for an OSF, only required to
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
            for_: Some("osf".into()),
            fountain: fountain.map(str::to_owned),
            options_json: None,
            out: out.map(str::to_owned),
            check,
        }
    }

    /// Calibre absent; the KFX probe and the ladder must not run.
    fn deps(&self) -> ExportDeps<'_> {
        ExportDeps {
            calibre_available: Some(Box::new(|| false)),
            kfx_status: Some(Box::new(|| panic!("the KFX probe must not run for an OSF"))),
            fresh_kindle_artifact: Some(Box::new(|_, _| {
                self.ladder_calls.set(self.ladder_calls.get() + 1);
                Err(EngineError::plain(
                    "the Kindle ladder must not run for an OSF",
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

fn osf_of(fountain: &str) -> String {
    Books.fountain_to_osf(fountain).expect("an OSF")
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

// ---- export_command --for osf ----

#[test]
fn the_renderer_is_sp_osf_of_the_tokenized_fountain() {
    let tokens = sp_fountain::tokenize(FOUNTAIN).unwrap();
    assert_eq!(osf_of(FOUNTAIN), sp_osf::tokens_to_osf(&tokens));
    assert!(osf_of(FOUNTAIN).contains("<document type=\"Open Screenplay Format document\""));
}

#[test]
fn writes_the_osf_beside_the_book_by_default_built_from_the_fountain() {
    let c = Case::new();
    let result = c
        .export(&c.opts(Some(&c.fountain), None, false))
        .expect("written");
    let want = c.join("Script.osf");
    assert_eq!(result.path, want);
    assert_eq!(result.format.as_str(), "osf");
    assert_eq!(result.extension, "osf");
    assert!(result.label.starts_with("OSF:"), "{}", result.label);
    assert!(result.beta);
    assert!(result.stages.is_empty());
    assert_eq!(read(&want), osf_of(FOUNTAIN));
    assert_eq!(c.ladder_calls.get(), 0);
}

#[test]
fn its_answer_has_the_fdx_answers_shape_with_available_last_and_beta() {
    let c = Case::new();
    let result = c
        .export(&c.opts(Some(&c.fountain), None, false))
        .expect("written");
    let keys: Vec<String> = result
        .to_json()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        [
            "path",
            "format",
            "extension",
            "label",
            "stages",
            "available",
            "beta"
        ]
    );
}

#[test]
fn out_writes_it_there_instead_parents_created_and_nothing_beside_the_book() {
    let c = Case::new();
    let out = c.join("nested/deeper/Saved.osf");
    let result = c
        .export(&c.opts(Some(&c.fountain), Some(&out), false))
        .expect("written");
    assert_eq!(result.path, out);
    assert_eq!(read(&out), osf_of(FOUNTAIN));
    assert!(!exists(&c.join("Script.osf")));
}

#[test]
fn out_replaces_an_existing_file_and_leaves_no_temp_file_behind() {
    let c = Case::new();
    let out = c.join("Saved.osf");
    std::fs::write(&out, "old").unwrap();
    c.export(&c.opts(Some(&c.fountain), Some(&out), false))
        .expect("written");
    assert_eq!(read(&out), osf_of(FOUNTAIN));
    assert!(tmp_names(&c.dir).is_empty());
}

#[test]
fn without_fountain_it_is_a_usage_error_naming_fountain_and_writes_nothing() {
    let c = Case::new();
    let (code, message) = refusal(c.export(&c.opts(None, None, false)));
    assert_eq!(code, "usage");
    assert!(message.contains("--for osf"), "{message}");
    assert!(message.contains("--fountain"), "{message}");
    assert!(!exists(&c.join("Script.osf")));
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
fn the_wrong_extension_on_out_is_refused_naming_osf() {
    let c = Case::new();
    let out = c.join("Saved.fdx");
    let (code, message) = refusal(c.export(&c.opts(Some(&c.fountain), Some(&out), false)));
    assert_eq!(code, "usage");
    assert!(message.contains("an OSF file"), "{message}");
    assert!(message.contains(".osf"), "{message}");
    assert!(!exists(&out));
}

#[test]
fn a_write_that_fails_is_export_failed_naming_the_chosen_path_and_not_the_temp_file() {
    let c = Case::new();
    let out = c.join("Blocked.osf");
    std::fs::create_dir(&out).unwrap();
    let (code, message) = refusal(c.export(&c.opts(Some(&c.fountain), Some(&out), false)));
    assert_eq!(code, "export-failed");
    assert!(message.contains("could not save the OSF"), "{message}");
    assert!(message.contains(&out), "{message}");
    assert!(!message.contains(".tmp"), "{message}");
    assert!(tmp_names(&c.dir).is_empty());
}

#[test]
fn check_builds_nothing_and_says_whether_the_osf_beside_the_book_is_current() {
    let c = Case::new();
    let first = c
        .export(&c.opts(Some(&c.fountain), None, true))
        .expect("checked");
    assert_eq!(first.path, c.join("Script.osf"));
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

    set_mtime(&c.fountain, now + Duration::from_secs(100));
    let stale = c
        .export(&c.opts(Some(&c.fountain), None, true))
        .expect("checked");
    assert_eq!(stale.fresh, Some(false));
}

#[test]
fn the_unknown_for_message_names_osf_too() {
    let c = Case::new();
    let mut opts = c.opts(None, None, false);
    opts.for_ = Some("pdf".into());
    let (code, message) = refusal(c.export(&opts));
    assert_eq!(code, "usage");
    assert!(
        message.starts_with("--for takes epub, kindle, kfx, azw3, mobi, fountain, fdx or osf,"),
        "{message}"
    );
}

#[test]
fn the_fdx_beside_the_book_is_left_alone() {
    let c = Case::new();
    let fdx = c.join("Script.fdx");
    std::fs::write(&fdx, "mine").unwrap();
    c.export(&c.opts(Some(&c.fountain), None, false))
        .expect("written");
    assert_eq!(read(&fdx), "mine");
}

// ---- method-reader export --for osf (through the CLI) ----

#[test]
fn json_prints_one_object_with_the_path_and_the_file_is_there() {
    let c = Case::new();
    let out = c.join("Saved.osf");
    let r = c.sb.run(&[
        "export",
        &c.epub,
        "--for",
        "osf",
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
    assert_eq!(answer["format"], "osf");
    assert_eq!(answer["extension"], "osf");
    assert_eq!(answer["beta"], Value::Bool(true));
    assert_eq!(read(&out), osf_of(FOUNTAIN));
}

#[test]
fn without_json_it_says_where_it_wrote() {
    let c = Case::new();
    let r =
        c.sb.run(&["export", &c.epub, "--for", "osf", "--fountain", &c.fountain]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let said = format!("{}{}", r.stdout, r.stderr);
    assert!(said.contains("OSF: "), "{said}");
    assert!(said.contains(&c.join("Script.osf")), "{said}");
}

#[test]
fn a_refusal_is_one_json_object_with_a_code_and_a_message() {
    let c = Case::new();
    let r = c.sb.run(&["export", &c.epub, "--for", "osf", "--json"]);
    assert_eq!(r.code, 1);
    let answer = r.json();
    assert_eq!(answer["ok"], Value::Bool(false));
    assert_eq!(answer["error"]["code"], "usage");
}

#[test]
fn export_help_names_osf_and_says_it_is_beta() {
    let r = Sandbox::new().run(&["export", "--help"]);
    assert!(
        r.stdout.contains("--for osf --fountain <f>"),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("osf (beta)"), "{}", r.stdout);
    let main = Sandbox::new().run(&["--help"]);
    assert!(main.stdout.contains("fountain|fdx|osf]"), "{}", main.stdout);
}
