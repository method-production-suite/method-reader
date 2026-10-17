//! Port of tests/cli-routes.test.ts: `method-reader routes` (the ranked list of
//! every way a book can leave) and `method-reader route` (perform one of them).
//!
//! The handlers are driven IN PROCESS through sp-engine with stand-in
//! facts, an opener that records argv, and a settings file in the scratch
//! folder. Nothing here may open an app, a URL or a folder, read the real
//! mail handler, or write the real app settings file.
//!
//! The spawned-CLI tests cover refusals, --help, `routes` (read-only) and
//! the one route that succeeds when spawned, `save-epub`, which opens
//! nothing and writes only in the scratch folder. Every spawn runs in a
//! Sandbox (settings, library and mounts in scratch, the reMarkable at a
//! closed port) with stand-in `open`, `xdg-open` and `defaults` first on
//! PATH: each logs its argv and exits 1. So the mail probe reads "could not
//! tell" instead of this machine's handler, and a refusal that ever stopped
//! refusing would log an open instead of opening anything.

mod common;

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use common::{Sandbox, Tablet, book_at, fake_tools, repo_root, tools_log};
use serde_json::{Value, json};
use sp_device::types::{ConnectedDevice, DeviceKind};
use sp_engine::commands::export::ExportDeps;
use sp_engine::commands::routes::{
    RouteAnswer, RouteDeps, RouteOptions, route_command, routes_command, routes_lines,
};
use sp_engine::errors::EngineError;
use sp_engine::export::kfx::KfxStatus;
use sp_engine::export::renderers::Books;
use sp_engine::export::route_perform::{Opened, email_to_kindle, real_opener, send_via_amazon};
use sp_engine::export::routes::{RouteFacts, routes};

const EM_DASH: char = '\u{2014}';

fn kindle() -> ConnectedDevice {
    ConnectedDevice {
        kind: DeviceKind::Kindle,
        name: "Kindle".into(),
        volume: Some("/Volumes/Kindle".into()),
        transport: None,
        serial: None,
    }
}

/// Darwin with Books, no Amazon app, Apple Mail the default, nothing
/// plugged in, unless a test says otherwise.
fn facts() -> RouteFacts {
    RouteFacts {
        platform: "darwin".into(),
        devices: vec![],
        remarkable_checked: None,
        books_app: true,
        send_to_kindle_app: false,
        apple_mail_default: true,
    }
}

fn on(platform: &str) -> RouteFacts {
    RouteFacts {
        platform: platform.into(),
        books_app: false,
        apple_mail_default: false,
        ..facts()
    }
}

/// A settings file path in its own fresh folder, never created here.
fn settings_file(sb: &Sandbox, folder: &str) -> String {
    sb.mkdir(folder);
    sb.p(&format!("{folder}/settings.json"))
}

fn remembered(path: &str) -> Option<Value> {
    std::fs::read_to_string(path)
        .ok()
        .map(|t| serde_json::from_str(&t).expect("settings JSON"))
}

fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

fn dirname(path: &str) -> String {
    Path::new(path)
        .parent()
        .expect("a folder")
        .to_string_lossy()
        .into_owned()
}

/// The refusal a handler answered: its code and message.
fn refused<T: std::fmt::Debug>(result: Result<T, EngineError>) -> (String, String) {
    match result {
        Err(EngineError::Cli(e)) => (e.code.as_str().to_owned(), e.message),
        other => panic!("expected a contract error, got {other:?}"),
    }
}

// ---- routes: the handler --------------------------------------------------

fn counted(f: RouteFacts) -> (Rc<Cell<usize>>, impl Fn() -> RouteFacts) {
    let calls = Rc::new(Cell::new(0));
    let c = Rc::clone(&calls);
    (calls, move || {
        c.set(c.get() + 1);
        f.clone()
    })
}

#[test]
fn routes_lists_exactly_the_catalog_for_the_probed_facts_in_its_order() {
    let sb = Sandbox::new();
    let f = RouteFacts {
        devices: vec![kindle()],
        ..facts()
    };
    let (_, probe) = counted(f.clone());
    let answer = routes_command(
        &book_at(&sb, "b/Script.epub"),
        &probe,
        Some(&settings_file(&sb, "c")),
    )
    .unwrap();
    assert_eq!(answer.routes, routes(&f));
}

#[test]
fn chosen_follows_a_remembered_key() {
    let sb = Sandbox::new();
    let path = settings_file(&sb, "c");
    std::fs::write(&path, r#"{"lastRoute":"save-kindle"}"#).unwrap();
    let (_, probe) = counted(facts());
    let answer = routes_command(&book_at(&sb, "b/Script.epub"), &probe, Some(&path)).unwrap();
    assert_eq!(answer.chosen, "save-kindle");
}

#[test]
fn chosen_is_the_row_id_not_the_key_a_remembered_kindle_picks_the_connected_one() {
    // A connected device's id carries its volume; its key does not. The
    // window matches rows by id, so answering the key would choose nothing.
    let sb = Sandbox::new();
    let path = settings_file(&sb, "c");
    std::fs::write(&path, r#"{"lastRoute":"device:kindle"}"#).unwrap();
    let (_, probe) = counted(RouteFacts {
        devices: vec![kindle()],
        ..facts()
    });
    let answer = routes_command(&book_at(&sb, "b/Script.epub"), &probe, Some(&path)).unwrap();
    assert_eq!(answer.chosen, "device:kindle#/Volumes/Kindle");
}

#[test]
fn a_remembered_kindle_that_is_unplugged_stays_chosen() {
    let sb = Sandbox::new();
    let path = settings_file(&sb, "c");
    std::fs::write(&path, r#"{"lastRoute":"device:kindle"}"#).unwrap();
    let (_, probe) = counted(facts());
    let answer = routes_command(&book_at(&sb, "b/Script.epub"), &probe, Some(&path)).unwrap();
    assert_eq!(answer.chosen, "device:kindle");
}

#[test]
fn falls_back_to_the_first_available_row_when_nothing_is_remembered() {
    let sb = Sandbox::new();
    let (_, probe) = counted(facts());
    let answer = routes_command(
        &book_at(&sb, "b/Script.epub"),
        &probe,
        Some(&settings_file(&sb, "c")),
    )
    .unwrap();
    assert_eq!(answer.chosen, "apple-books");
}

#[test]
fn a_last_route_that_is_not_a_string_is_treated_as_nothing_remembered() {
    let sb = Sandbox::new();
    let path = settings_file(&sb, "c");
    std::fs::write(&path, r#"{"lastRoute":7}"#).unwrap();
    let (_, probe) = counted(on("linux"));
    let answer = routes_command(&book_at(&sb, "b/Script.epub"), &probe, Some(&path)).unwrap();
    assert_eq!(answer.chosen, "send-to-kindle");
}

#[test]
fn routes_is_read_only_no_settings_file_appears_and_an_existing_one_is_left_byte_for_byte() {
    let sb = Sandbox::new();
    let (_, probe) = counted(facts());
    let absent = settings_file(&sb, "c1");
    routes_command(&book_at(&sb, "b1/Script.epub"), &probe, Some(&absent)).unwrap();
    assert!(!exists(&absent));

    let present = settings_file(&sb, "c2");
    let text = r#"{"lastRoute":"save-epub","libraryPath":"/somewhere"}"#;
    std::fs::write(&present, text).unwrap();
    let before = std::fs::metadata(&present).unwrap().modified().unwrap();
    routes_command(&book_at(&sb, "b2/Script.epub"), &probe, Some(&present)).unwrap();
    assert_eq!(std::fs::read_to_string(&present).unwrap(), text);
    assert_eq!(
        std::fs::metadata(&present).unwrap().modified().unwrap(),
        before
    );
}

#[test]
fn an_unreadable_book_is_refused_first_no_probe_runs() {
    let sb = Sandbox::new();
    let (calls, probe) = counted(facts());
    let (code, message) = refused(routes_command(
        &sb.p("no-such.epub"),
        &probe,
        Some(&settings_file(&sb, "c")),
    ));
    assert_eq!(code, "unreadable");
    assert!(message.contains("no-such.epub"), "{message}");
    assert_eq!(calls.get(), 0);
}

#[test]
fn a_folder_given_as_the_book_is_unreadable_too() {
    let sb = Sandbox::new();
    let (calls, probe) = counted(facts());
    let dir = sb.mkdir("dir");
    let (code, _) = refused(routes_command(
        &dir.to_string_lossy(),
        &probe,
        Some(&settings_file(&sb, "c")),
    ));
    assert_eq!(code, "unreadable");
    assert_eq!(calls.get(), 0);
}

// ---- routes for a person ----------------------------------------------------

#[test]
fn one_line_per_route_a_star_on_the_chosen_one_dimmed_rows_carry_their_fix() {
    let sb = Sandbox::new();
    let (_, probe) = counted(RouteFacts {
        apple_mail_default: false,
        ..facts()
    });
    let answer = routes_command(
        &book_at(&sb, "b/Script.epub"),
        &probe,
        Some(&settings_file(&sb, "c")),
    )
    .unwrap();
    let lines = routes_lines(&answer);
    assert_eq!(lines.len(), answer.routes.len());
    let starred: Vec<&String> = lines.iter().filter(|l| l.starts_with('*')).collect();
    assert_eq!(starred.len(), 1);
    assert!(starred[0].contains("apple-books"), "{}", starred[0]);
    assert!(starred[0].contains("Apple Books"), "{}", starred[0]);
    let email = lines
        .iter()
        .find(|l| l.contains("email-to-kindle"))
        .unwrap();
    assert!(
        email.contains("(dimmed: needs Apple Mail as the default mail app"),
        "{email}"
    );
    let save = lines.iter().find(|l| l.contains("save-epub")).unwrap();
    assert!(!save.contains("dimmed"), "{save}");
    assert!(
        save.contains("for email, Apple Books and most e-readers"),
        "{save}"
    );
    for line in &lines {
        assert!(!line.contains(EM_DASH), "{line}");
    }
}

#[test]
fn the_star_follows_the_row_id_of_two_kindles_only_the_chosen_one_is_starred() {
    // Two connected Kindles share the key device:kindle and differ only by
    // id, so a star matched by key marks neither (chosen is an id) or both.
    let sb = Sandbox::new();
    let path = settings_file(&sb, "c");
    std::fs::write(&path, r#"{"lastRoute":"device:kindle"}"#).unwrap();
    let twin = ConnectedDevice {
        name: "KINDLE2".into(),
        volume: Some("/Volumes/KINDLE2".into()),
        ..kindle()
    };
    let (_, probe) = counted(RouteFacts {
        devices: vec![kindle(), twin],
        ..facts()
    });
    let answer = routes_command(&book_at(&sb, "b/Script.epub"), &probe, Some(&path)).unwrap();
    let lines = routes_lines(&answer);
    let starred: Vec<&String> = lines.iter().filter(|l| l.starts_with('*')).collect();
    assert_eq!(starred.len(), 1);
    assert!(starred[0].contains("Kindle: over USB"), "{}", starred[0]);
    assert!(
        lines
            .iter()
            .find(|l| l.contains("KINDLE2"))
            .unwrap()
            .starts_with(' ')
    );
}

// ---- route: the performer and the handler -----------------------------------

const BOOKS_NOTE: &str =
    "Added to Apple Books. It syncs to your iPhone and iPad when Books uses iCloud.";
const APP_NOTE: &str = "Opened Amazon's Send to Kindle app with the book.";
const PAGE_NOTE: &str =
    "Opened Amazon's Send to Kindle page, and the book's folder so you can drag it in.";
const MAIL_NOTE: &str =
    "Opened a Mail message with the book attached. Address it to your Kindle's email address.";
const SETUP_NOTE: &str = "Opened Amazon's settings page. Find your Kindle's email address there, and add the address you send from to the approved list.";
const STK_URL: &str = "https://www.amazon.com/sendtokindle";
const PDOC_URL: &str = "https://www.amazon.com/hz/mycd/myx#/home/settings/pdoc";
const TO_SEND: &str = "send to a reader with method-reader send; route performs the others";

fn no_toolchain() -> KfxStatus {
    KfxStatus {
        calibre: false,
        previewer: false,
        plugin_installed: false,
        ready: false,
    }
}

type Ladder = Rc<dyn Fn() -> Result<String, EngineError>>;
type CodeFor = Rc<dyn Fn(&[String]) -> i32>;
/// Each ladder run: its fountain and whether scene numbers were on.
type LadderCalls = Rc<RefCell<Vec<(Option<String>, bool)>>>;

/// Everything route_command can reach, stood in and counted: the facts, an
/// opener that records argv and answers `code_for(argv)` (0 unless told),
/// the Kindle ladder (fails the test unless one is given), and a settings
/// file in its own scratch folder that starts absent.
struct Rig {
    sb: Sandbox,
    facts: RouteFacts,
    code_for: CodeFor,
    stderr: String,
    platform: Option<String>,
    ladder: Option<Ladder>,
    calibre: bool,
    calls: Rc<RefCell<Vec<Vec<String>>>>,
    facts_calls: Rc<Cell<usize>>,
    ladder_calls: LadderCalls,
    settings: String,
    books: Cell<usize>,
}

impl Rig {
    fn new() -> Rig {
        let sb = Sandbox::new();
        let settings = settings_file(&sb, "rig-config");
        Rig {
            sb,
            facts: facts(),
            code_for: Rc::new(|_| 0),
            stderr: String::new(),
            platform: None,
            ladder: None,
            calibre: false,
            calls: Rc::default(),
            facts_calls: Rc::default(),
            ladder_calls: Rc::default(),
            settings,
            books: Cell::new(0),
        }
    }

    fn facts(mut self, f: RouteFacts) -> Rig {
        self.facts = f;
        self
    }

    fn codes(mut self, f: impl Fn(&[String]) -> i32 + 'static, stderr: &str) -> Rig {
        self.code_for = Rc::new(f);
        self.stderr = stderr.into();
        self
    }

    fn platform(mut self, p: &str) -> Rig {
        self.platform = Some(p.into());
        self
    }

    fn ladder(mut self, f: impl Fn() -> Result<String, EngineError> + 'static) -> Rig {
        self.ladder = Some(Rc::new(f));
        self
    }

    fn book(&self, name: &str) -> String {
        self.books.set(self.books.get() + 1);
        book_at(&self.sb, &format!("book-{}/{name}", self.books.get()))
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }

    fn deps(&self) -> RouteDeps<'static> {
        let f = self.facts.clone();
        let fc = Rc::clone(&self.facts_calls);
        let calls = Rc::clone(&self.calls);
        let code_for = Rc::clone(&self.code_for);
        let stderr = self.stderr.clone();
        let ladder = self.ladder.clone();
        let ladder_calls = Rc::clone(&self.ladder_calls);
        let has_ladder = ladder.is_some();
        let calibre = self.calibre;
        RouteDeps {
            facts: Some(Box::new(move || {
                fc.set(fc.get() + 1);
                f.clone()
            })),
            settings_path: Some(self.settings.clone()),
            open: Some(Box::new(move |argv: &[String]| {
                calls.borrow_mut().push(argv.to_vec());
                Opened {
                    code: code_for(argv),
                    stderr: stderr.clone(),
                }
            })),
            size_of: None,
            platform: self.platform.clone(),
            export_deps: Some(ExportDeps {
                calibre_available: Some(Box::new(move || has_ladder)),
                kfx_status: Some(Box::new(move || KfxStatus {
                    calibre,
                    ..no_toolchain()
                })),
                fresh_kindle_artifact: Some(Box::new(move |opts, _stage| {
                    ladder_calls
                        .borrow_mut()
                        .push((opts.fountain_path.clone(), opts.format.show_scene_numbers));
                    match &ladder {
                        Some(l) => l(),
                        None => panic!("the Kindle ladder was not expected to run"),
                    }
                })),
                ..Default::default()
            }),
        }
    }

    fn run(&self, options: RouteOptions) -> Result<RouteAnswer, EngineError> {
        route_command(&options, self.deps(), &Books)
    }
}

fn opts(key: &str, epub: Option<&str>) -> RouteOptions {
    RouteOptions {
        key: key.into(),
        epub: epub.map(str::to_owned),
        ..Default::default()
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

// ---- route: Apple Books -----------------------------------------------------

#[test]
fn apple_books_opens_the_book_with_books_answers_the_note_and_remembers_it() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let answer = r.run(opts("apple-books", Some(&epub))).unwrap();
    assert_eq!(r.calls(), vec![argv(&["open", "-a", "Books", &epub])]);
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "apple-books", "note": BOOKS_NOTE })
    );
    assert_eq!(answer.path, None);
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "apple-books" }))
    );
}

#[test]
fn a_book_path_with_spaces_reaches_books_as_one_argument() {
    let r = Rig::new();
    let epub = r.book("My Script (draft 2).epub");
    r.run(opts("apple-books", Some(&epub))).unwrap();
    assert_eq!(r.calls(), vec![argv(&["open", "-a", "Books", &epub])]);
}

#[test]
fn remembering_keeps_the_settings_files_other_keys() {
    let r = Rig::new();
    std::fs::write(
        &r.settings,
        r#"{"lastRoute":"save-epub","libraryPath":"/kept"}"#,
    )
    .unwrap();
    let epub = r.book("Script.epub");
    r.run(opts("apple-books", Some(&epub))).unwrap();
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "apple-books", "libraryPath": "/kept" }))
    );
}

#[test]
fn books_failing_to_open_is_route_failed_naming_apple_books_and_why_and_is_not_remembered() {
    let r = Rig::new().codes(|_| 1, "LSOpenURLsWithRole() failed with error -10810\n");
    std::fs::write(&r.settings, r#"{"lastRoute":"save-epub"}"#).unwrap();
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("apple-books", Some(&epub))));
    assert_eq!(code, "route-failed");
    assert_eq!(
        message,
        "could not open Apple Books: LSOpenURLsWithRole() failed with error -10810"
    );
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "save-epub" }))
    );
}

#[test]
fn a_failure_that_printed_nothing_still_says_why_its_exit_code() {
    let r = Rig::new().codes(|_| 3, "");
    let epub = r.book("Script.epub");
    let (_, message) = refused(r.run(opts("apple-books", Some(&epub))));
    assert_eq!(message, "could not open Apple Books: it exited with code 3");
}

#[test]
fn apple_books_off_a_mac_is_unavailable_with_its_fix_and_nothing_opens() {
    let r = Rig::new().facts(RouteFacts {
        platform: "linux".into(),
        books_app: false,
        ..facts()
    });
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("apple-books", Some(&epub))));
    assert_eq!(code, "route-unavailable");
    assert_eq!(message, "On a Mac only");
    assert!(r.calls().is_empty());
    assert!(!exists(&r.settings));
}

#[test]
fn on_a_mac_without_books_app_the_row_is_hidden_it_is_unavailable_and_nothing_opens() {
    let r = Rig::new().facts(RouteFacts {
        books_app: false,
        ..facts()
    });
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("apple-books", Some(&epub))));
    assert_eq!(code, "route-unavailable");
    assert!(message.contains("Apple Books"), "{message}");
    assert!(r.calls().is_empty());
    assert!(!exists(&r.settings));
}

// ---- route: Send to Kindle --------------------------------------------------

#[test]
fn amazons_app_installed_on_a_mac_one_open_with_the_book() {
    let r = Rig::new().facts(RouteFacts {
        send_to_kindle_app: true,
        ..facts()
    });
    let epub = r.book("Script.epub");
    let answer = r.run(opts("send-to-kindle", Some(&epub))).unwrap();
    assert_eq!(
        r.calls(),
        vec![argv(&["open", "-a", "Send to Kindle", &epub])]
    );
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "send-to-kindle", "note": APP_NOTE })
    );
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "send-to-kindle" }))
    );
}

#[test]
fn no_app_on_a_mac_open_the_page_then_reveal_the_book() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let answer = r.run(opts("send-to-kindle", Some(&epub))).unwrap();
    assert_eq!(
        r.calls(),
        vec![argv(&["open", STK_URL]), argv(&["open", "-R", &epub])]
    );
    assert_eq!(answer.note, PAGE_NOTE);
}

#[test]
fn windows_the_page_opens_then_explorer_opens_the_books_folder_the_app_is_a_mac_thing() {
    // explorer exits 1 even when it did what was asked: not a failure. The
    // folder, not `/select,<book>`: a path with a space or a comma breaks
    // that flag's quoting.
    let r = Rig::new()
        .facts(RouteFacts {
            send_to_kindle_app: true,
            ..on("win32")
        })
        .codes(|a| i32::from(a[0] == "explorer"), "");
    let epub = r.book("My Script, draft 2.epub");
    let answer = r.run(opts("send-to-kindle", Some(&epub))).unwrap();
    assert_eq!(
        r.calls(),
        vec![
            argv(&["rundll32", "url.dll,FileProtocolHandler", STK_URL]),
            argv(&["explorer", &dirname(&epub)]),
        ]
    );
    assert_eq!(answer.note, PAGE_NOTE);
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "send-to-kindle" }))
    );
}

#[test]
fn windows_the_page_failing_to_open_is_a_failure_and_explorer_never_runs() {
    let r = Rig::new()
        .facts(on("win32"))
        .codes(|a| i32::from(a[0] == "rundll32"), "");
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("send-to-kindle", Some(&epub))));
    assert_eq!(code, "route-failed");
    assert!(
        message.contains("could not open Amazon's Send to Kindle page"),
        "{message}"
    );
    assert_eq!(
        r.calls(),
        vec![argv(&["rundll32", "url.dll,FileProtocolHandler", STK_URL])]
    );
    assert!(!exists(&r.settings));
}

#[test]
fn linux_xdg_open_the_page_then_the_books_folder() {
    let r = Rig::new().facts(on("linux"));
    let epub = r.book("Script.epub");
    r.run(opts("send-to-kindle", Some(&epub))).unwrap();
    assert_eq!(
        r.calls(),
        vec![
            argv(&["xdg-open", STK_URL]),
            argv(&["xdg-open", &dirname(&epub)])
        ]
    );
}

#[test]
fn a_mac_reveal_that_fails_is_still_route_failed_but_only_after_the_page_already_opened() {
    let r = Rig::new().codes(|a| i32::from(a[1] == "-R"), "no such file");
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("send-to-kindle", Some(&epub))));
    assert_eq!(code, "route-failed");
    assert_eq!(message, "could not open the book's folder: no such file");
    assert_eq!(
        r.calls(),
        vec![argv(&["open", STK_URL]), argv(&["open", "-R", &epub])]
    );
    assert!(!exists(&r.settings));
}

#[test]
fn the_page_failing_on_a_mac_is_route_failed_too_and_the_folder_is_never_opened() {
    let r = Rig::new().codes(|a| i32::from(a[1] == STK_URL), "");
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("send-to-kindle", Some(&epub))));
    assert_eq!(code, "route-failed");
    assert_eq!(
        message,
        "could not open Amazon's Send to Kindle page: it exited with code 1"
    );
    assert_eq!(r.calls(), vec![argv(&["open", STK_URL])]);
}

#[test]
fn the_pages_argv_is_issued_before_the_folders() {
    // The TypeScript also races a folder opener that never resolves, so a
    // file manager that never returns cannot hold up the page. The Rust
    // opener is synchronous: what carries over is that the page has ALREADY
    // been asked for when the folder's open starts.
    let sb = Sandbox::new();
    let epub = book_at(&sb, "b/Script.epub");
    let calls: RefCell<Vec<Vec<String>>> = RefCell::default();
    let open = |a: &[String]| {
        if a[1] == "-R" {
            assert_eq!(*calls.borrow(), vec![argv(&["open", STK_URL])]);
        }
        calls.borrow_mut().push(a.to_vec());
        Opened {
            code: 0,
            stderr: String::new(),
        }
    };
    send_via_amazon(&epub, "darwin", false, &open).unwrap();
    assert_eq!(
        *calls.borrow(),
        vec![argv(&["open", STK_URL]), argv(&["open", "-R", &epub])]
    );
}

// ---- route: email to Kindle -------------------------------------------------

#[test]
fn apple_mail_the_default_opens_a_mail_message_with_the_book() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let answer = r.run(opts("email-to-kindle", Some(&epub))).unwrap();
    assert_eq!(r.calls(), vec![argv(&["open", "-a", "Mail", &epub])]);
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "email-to-kindle", "note": MAIL_NOTE })
    );
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "email-to-kindle" }))
    );
}

#[test]
fn another_mail_app_the_default_unavailable_with_the_setup_fix_nothing_opened_or_remembered() {
    let r = Rig::new().facts(RouteFacts {
        apple_mail_default: false,
        ..facts()
    });
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("email-to-kindle", Some(&epub))));
    assert_eq!(code, "route-unavailable");
    assert_eq!(
        message,
        "Needs Apple Mail as the default mail app, the one mail app the attachment survives"
    );
    assert_eq!(r.facts_calls.get(), 1);
    assert!(r.calls().is_empty());
    assert!(!exists(&r.settings));
}

#[test]
fn email_off_a_mac_unavailable_with_the_platform_fix() {
    let r = Rig::new().facts(on("win32"));
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("email-to-kindle", Some(&epub))));
    assert_eq!(code, "route-unavailable");
    assert_eq!(
        message,
        "On a Mac only; save the EPUB and attach it yourself"
    );
    assert!(r.calls().is_empty());
}

#[test]
fn mail_failing_to_open_is_route_failed_not_remembered() {
    let r = Rig::new().codes(|_| 1, "Unable to find application named Mail");
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(opts("email-to-kindle", Some(&epub))));
    assert_eq!(code, "route-failed");
    assert_eq!(
        message,
        "could not open Mail: Unable to find application named Mail"
    );
    assert!(!exists(&r.settings));
}

// ---- route email-to-kindle: Amazon's 50 MB limit ----------------------------
// Amazon drops mail over its limit without a word to the sender, so a book
// that is over is refused before Mail opens. Sizes are stood in through the
// seam: no test here writes a 50 MB file.

fn too_large(mb: &str) -> String {
    format!(
        "This book is too large for Kindle email. It is {mb} MB, and Amazon takes up to 50 MB by email and drops anything larger without a word. Send it with the Send to Kindle app or web page instead, which take up to 200 MB."
    )
}

fn with_size(r: &Rig, bytes: u64, sizes: Rc<RefCell<Vec<String>>>) -> RouteDeps<'static> {
    let mut deps = r.deps();
    deps.size_of = Some(Box::new(move |p: &str| {
        sizes.borrow_mut().push(p.to_owned());
        Ok(bytes)
    }));
    deps
}

#[test]
fn a_book_over_the_limit_is_refused_before_mail_opens_naming_send_to_kindle_and_is_not_remembered()
{
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let sizes: Rc<RefCell<Vec<String>>> = Rc::default();
    let deps = with_size(&r, 63_400_000, Rc::clone(&sizes));
    let (code, message) = refused(route_command(
        &opts("email-to-kindle", Some(&epub)),
        deps,
        &Books,
    ));
    assert_eq!(code, "route-unavailable");
    assert_eq!(message, too_large("63.4"));
    assert!(!message.contains(EM_DASH));
    assert_eq!(*sizes.borrow(), vec![epub]);
    assert!(r.calls().is_empty());
    assert!(!exists(&r.settings));
}

#[test]
fn exactly_50_mb_counted_the_way_amazon_counts_it_still_goes() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let deps = with_size(&r, 50_000_000, Rc::default());
    let answer = route_command(&opts("email-to-kindle", Some(&epub)), deps, &Books).unwrap();
    assert_eq!(r.calls(), vec![argv(&["open", "-a", "Mail", &epub])]);
    assert_eq!(answer.note, MAIL_NOTE);
}

#[test]
fn one_byte_over_reads_as_over_never_as_the_50_mb_it_is_refused_for() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let deps = with_size(&r, 50_000_001, Rc::default());
    let (_, message) = refused(route_command(
        &opts("email-to-kindle", Some(&epub)),
        deps,
        &Books,
    ));
    assert_eq!(message, too_large("50.1"));
    assert!(r.calls().is_empty());
}

#[test]
fn a_whole_number_of_megabytes_is_written_without_a_decimal() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let deps = with_size(&r, 120_000_000, Rc::default());
    let (_, message) = refused(route_command(
        &opts("email-to-kindle", Some(&epub)),
        deps,
        &Books,
    ));
    assert_eq!(message, too_large("120"));
}

#[test]
fn the_performer_refuses_on_its_own_so_no_caller_can_attach_a_book_amazon_would_drop() {
    let sb = Sandbox::new();
    let calls: RefCell<Vec<Vec<String>>> = RefCell::default();
    let open = |a: &[String]| {
        calls.borrow_mut().push(a.to_vec());
        Opened {
            code: 0,
            stderr: String::new(),
        }
    };
    let (code, message) = refused(email_to_kindle(
        &book_at(&sb, "b/Script.epub"),
        &open,
        &|_: &str| Ok(75_000_000),
    ));
    assert_eq!(code, "route-unavailable");
    assert_eq!(message, too_large("75"));
    assert!(calls.borrow().is_empty());
}

#[test]
fn with_nothing_stood_in_the_size_is_the_file_on_disk() {
    // The book is a few bytes, far under the limit, so Mail opens.
    let sb = Sandbox::new();
    let epub = book_at(&sb, "b/Script.epub");
    let calls: RefCell<Vec<Vec<String>>> = RefCell::default();
    let open = |a: &[String]| {
        calls.borrow_mut().push(a.to_vec());
        Opened {
            code: 0,
            stderr: String::new(),
        }
    };
    let note = email_to_kindle(
        &epub,
        &open,
        &sp_engine::export::route_perform::real_size_of,
    )
    .unwrap();
    assert_eq!(note, MAIL_NOTE);
    assert_eq!(*calls.borrow(), vec![argv(&["open", "-a", "Mail", &epub])]);
}

// ---- route kindle-email-setup: Amazon's settings page -----------------------

fn setup_opens(platform: &str, expected: &[&str]) {
    let r = Rig::new().platform(platform);
    let answer = r.run(opts("kindle-email-setup", None)).unwrap();
    assert_eq!(r.calls(), vec![argv(expected)]);
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "kindle-email-setup", "note": SETUP_NOTE })
    );
    assert_eq!(r.facts_calls.get(), 0);
}

#[test]
fn darwin_opens_the_page_with_the_platforms_url_opener_no_book_no_probe() {
    setup_opens("darwin", &["open", PDOC_URL]);
}

#[test]
fn win32_opens_the_page_with_the_platforms_url_opener_no_book_no_probe() {
    setup_opens(
        "win32",
        &["rundll32", "url.dll,FileProtocolHandler", PDOC_URL],
    );
}

#[test]
fn linux_opens_the_page_with_the_platforms_url_opener_no_book_no_probe() {
    setup_opens("linux", &["xdg-open", PDOC_URL]);
}

#[test]
fn kindle_email_setup_is_never_remembered_it_is_a_setup_step_not_a_way_out() {
    let r = Rig::new().platform("darwin");
    r.run(opts("kindle-email-setup", None)).unwrap();
    assert!(!exists(&r.settings));
    std::fs::write(&r.settings, r#"{"lastRoute":"email-to-kindle"}"#).unwrap();
    r.run(opts("kindle-email-setup", None)).unwrap();
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "email-to-kindle" }))
    );
}

#[test]
fn kindle_email_setup_is_available_on_every_platform_even_where_email_to_kindle_is_not() {
    let r = Rig::new().platform("linux").facts(on("linux"));
    let answer = r.run(opts("kindle-email-setup", None)).unwrap();
    assert_eq!(answer.note, SETUP_NOTE);
}

#[test]
fn the_settings_page_failing_to_open_is_route_failed() {
    let r = Rig::new().platform("darwin").codes(|_| 1, "");
    let (code, message) = refused(r.run(opts("kindle-email-setup", None)));
    assert_eq!(code, "route-failed");
    assert_eq!(
        message,
        "could not open Amazon's settings page: it exited with code 1"
    );
}

#[test]
fn the_host_platform_is_the_default() {
    let r = Rig::new();
    r.run(opts("kindle-email-setup", None)).unwrap();
    let expected = if cfg!(target_os = "macos") {
        argv(&["open", PDOC_URL])
    } else if cfg!(windows) {
        argv(&["rundll32", "url.dll,FileProtocolHandler", PDOC_URL])
    } else {
        argv(&["xdg-open", PDOC_URL])
    };
    assert_eq!(r.calls(), vec![expected]);
}

// ---- route: save a copy -----------------------------------------------------

#[test]
fn save_epub_writes_the_book_to_out_answers_its_path_and_remembers_it_nothing_opens_or_is_probed() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    r.sb.mkdir("save");
    let out = r.sb.p("save/nested/Saved.epub");
    let answer = r
        .run(RouteOptions {
            out: Some(out.clone()),
            ..opts("save-epub", Some(&epub))
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "save-epub", "path": out, "note": format!("Saved to {out}.") })
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "book-bytes");
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "save-epub" }))
    );
    assert!(r.calls().is_empty());
    assert_eq!(r.facts_calls.get(), 0);
    assert!(r.ladder_calls.borrow().is_empty());
}

/// A book, its .fountain and an already-built AZW3 beside it, for the
/// ladder stand-in to answer.
fn kindle_book(r: &Rig) -> (String, String, String) {
    let epub = r.book("Script.epub");
    let fountain = common::join(&dirname(&epub), "Script.fountain");
    std::fs::write(&fountain, "INT. ROOM - DAY\n").unwrap();
    let built = common::join(&dirname(&epub), "Script.azw3");
    std::fs::write(&built, "azw3-bytes").unwrap();
    (epub, fountain, built)
}

#[test]
fn save_kindle_builds_with_the_scripts_fountain_and_settings_then_copies_the_file_to_out() {
    let mut r = Rig::new();
    r.calibre = true;
    let (epub, fountain, built) = kindle_book(&r);
    let r = r.ladder(move || Ok(built.clone()));
    r.sb.mkdir("save");
    let out = r.sb.p("save/Copy.azw3");
    let answer = r
        .run(RouteOptions {
            out: Some(out.clone()),
            fountain: Some(fountain.clone()),
            options_json: Some(r#"{"showSceneNumbers":true}"#.into()),
            ..opts("save-kindle", Some(&epub))
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({ "key": "save-kindle", "path": out, "note": format!("Saved to {out}.") })
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "azw3-bytes");
    assert_eq!(*r.ladder_calls.borrow(), vec![(Some(fountain), true)]);
    assert_eq!(
        remembered(&r.settings),
        Some(json!({ "lastRoute": "save-kindle" }))
    );
    assert!(r.calls().is_empty());
    assert_eq!(r.facts_calls.get(), 0);
}

#[test]
fn save_kindles_format_defaults_follow_route_deps_settings_path_not_the_production_settings_file() {
    // settings_path is what keeps lastRoute out of a real settings file in
    // every other test here; this proves it also keeps the format defaults
    // out of one, by writing a formatDefaults override to the SAME scratch
    // path and never naming app_settings_path in export_deps at all.
    let mut r = Rig::new();
    r.calibre = true;
    let (epub, fountain, built) = kindle_book(&r);
    let r = r.ladder(move || Ok(built.clone()));
    std::fs::write(
        &r.settings,
        r#"{"formatDefaults":{"showSceneNumbers":true}}"#,
    )
    .unwrap();
    r.sb.mkdir("save");
    let out = r.sb.p("save/Copy.azw3");
    let answer = r
        .run(RouteOptions {
            out: Some(out.clone()),
            fountain: Some(fountain),
            ..opts("save-kindle", Some(&epub))
        })
        .unwrap();
    assert_eq!(answer.path.as_deref(), Some(out.as_str()));
    assert!(r.ladder_calls.borrow()[0].1);
}

#[test]
fn save_kindle_whose_ladder_fails_is_export_failed_and_not_remembered() {
    let r = Rig::new().ladder(|| Err(EngineError::plain("Calibre stopped")));
    let epub = r.book("Script.epub");
    r.sb.mkdir("save");
    let out = r.sb.p("save/Copy.azw3");
    let (code, _) = refused(r.run(RouteOptions {
        out: Some(out.clone()),
        ..opts("save-kindle", Some(&epub))
    }));
    assert_eq!(code, "export-failed");
    assert!(!exists(&out));
    assert!(!exists(&r.settings));
}

#[test]
fn save_epub_to_a_name_that_is_not_epub_is_exports_usage_refusal_nothing_written_or_remembered() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    r.sb.mkdir("save");
    let out = r.sb.p("save/Copy.azw3");
    let (code, message) = refused(r.run(RouteOptions {
        out: Some(out.clone()),
        ..opts("save-epub", Some(&epub))
    }));
    assert_eq!(code, "usage");
    assert!(message.contains(".epub"), "{message}");
    assert!(!exists(&out));
    assert!(!exists(&r.settings));
}

// ---- route: remembering never fails a route that worked ---------------------

#[test]
fn a_settings_path_that_cannot_be_written_the_route_still_answers_its_note() {
    let r = Rig::new();
    let blocker = r.sb.write("block/a-file", "x");
    let epub = r.book("Script.epub");
    let mut deps = r.deps();
    deps.settings_path = Some(format!("{}/config/settings.json", blocker.display()));
    let answer = route_command(&opts("apple-books", Some(&epub)), deps, &Books).unwrap();
    assert_eq!(answer.note, BOOKS_NOTE);
    assert_eq!(r.calls().len(), 1);
}

#[cfg(unix)]
#[test]
fn a_read_only_settings_folder_a_save_still_answers_its_path() {
    use std::os::unix::fs::PermissionsExt;
    let r = Rig::new();
    let folder = r.sb.mkdir("readonly");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let epub = r.book("Script.epub");
    r.sb.mkdir("save");
    let out = r.sb.p("save/Saved.epub");
    let mut deps = r.deps();
    deps.settings_path = Some(format!("{}/settings.json", folder.display()));
    let answer = route_command(
        &RouteOptions {
            out: Some(out.clone()),
            ..opts("save-epub", Some(&epub))
        },
        deps,
        &Books,
    );
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(answer.unwrap().path.as_deref(), Some(out.as_str()));
    assert!(exists(&out));
}

// ---- route refuses before it probes, opens, builds or remembers anything ----
// Every usage refusal, each counted: zero opener calls, zero facts calls,
// zero ladder runs, and no settings file. A refusal that moved below the
// probe, the ladder or an open shows up here as a count, not as a pass.

fn refuses_and_nothing_ran(options: impl Fn(&str) -> RouteOptions, fragment: &str) {
    let r = Rig::new().ladder(|| Ok("/never.azw3".into()));
    let epub = r.book("Script.epub");
    let (code, message) = refused(r.run(options(&epub)));
    assert_eq!(code, "usage");
    assert!(message.contains(fragment), "{message:?} lacks {fragment:?}");
    assert!(!message.contains(EM_DASH));
    assert!(r.calls().is_empty());
    assert_eq!(r.facts_calls.get(), 0);
    assert!(r.ladder_calls.borrow().is_empty());
    assert!(!exists(&r.settings));
}

fn o(
    key: &str,
    epub: Option<&str>,
    out: Option<&str>,
    fountain: Option<&str>,
    json: Option<&str>,
) -> RouteOptions {
    RouteOptions {
        key: key.into(),
        epub: epub.map(str::to_owned),
        out: out.map(str::to_owned),
        fountain: fountain.map(str::to_owned),
        options_json: json.map(str::to_owned),
    }
}

macro_rules! refusal {
    ($name:ident, |$b:ident| $options:expr, $fragment:expr) => {
        #[test]
        fn $name() {
            refuses_and_nothing_ran(|$b: &str| $options, $fragment);
        }
    };
}

refusal!(
    a_device_key_usage_and_nothing_ran,
    |b| o("device:kindle", Some(b), None, None, None),
    TO_SEND
);
refusal!(
    another_device_key_usage_and_nothing_ran,
    |b| o("device:kobo", Some(b), None, None, None),
    TO_SEND
);
refusal!(
    a_bare_device_prefix_usage_and_nothing_ran,
    |b| o("device:", Some(b), None, None, None),
    TO_SEND
);
refusal!(
    remarkable_usage_and_nothing_ran,
    |b| o("remarkable", Some(b), None, None, None),
    TO_SEND
);
refusal!(
    an_unknown_key_usage_and_nothing_ran,
    |b| o("carrier-pigeon", Some(b), None, None, None),
    "no route is called \"carrier-pigeon\""
);
refusal!(
    an_empty_key_usage_and_nothing_ran,
    |b| o("", Some(b), None, None, None),
    "no route is called \"\""
);
refusal!(
    a_key_in_the_wrong_case_usage_and_nothing_ran,
    |b| o("Apple-Books", Some(b), None, None, None),
    "no route is called \"Apple-Books\""
);
refusal!(
    no_book_for_apple_books_usage_and_nothing_ran,
    |_b| o("apple-books", None, None, None, None),
    "route apple-books needs the book"
);
refusal!(
    no_book_for_save_epub_usage_and_nothing_ran,
    |_b| o("save-epub", None, Some("/x/Saved.epub"), None, None),
    "route save-epub needs the book"
);
refusal!(
    a_book_for_kindle_email_setup_usage_and_nothing_ran,
    |b| o("kindle-email-setup", Some(b), None, None, None),
    "kindle-email-setup takes no book"
);
refusal!(
    out_on_kindle_email_setup_usage_and_nothing_ran,
    |_b| o(
        "kindle-email-setup",
        None,
        Some("/x/Saved.epub"),
        None,
        None
    ),
    "kindle-email-setup takes no --out"
);
refusal!(
    fountain_on_kindle_email_setup_usage_and_nothing_ran,
    |_b| o("kindle-email-setup", None, None, Some("/x.fountain"), None),
    "kindle-email-setup takes no --fountain"
);
refusal!(
    options_json_on_kindle_email_setup_usage_and_nothing_ran,
    |_b| o("kindle-email-setup", None, None, None, Some("{}")),
    "kindle-email-setup takes no --options-json"
);
refusal!(
    save_epub_without_out_usage_and_nothing_ran,
    |b| o("save-epub", Some(b), None, None, None),
    "route save-epub needs --out"
);
refusal!(
    save_kindle_without_out_usage_and_nothing_ran,
    |b| o("save-kindle", Some(b), None, None, None),
    "route save-kindle needs --out"
);
refusal!(
    a_relative_out_usage_and_nothing_ran,
    |b| o("save-epub", Some(b), Some("Saved.epub"), None, None),
    "must be absolute"
);
refusal!(
    a_relative_out_on_save_kindle_usage_and_nothing_ran,
    |b| o("save-kindle", Some(b), Some("Saved.azw3"), None, None),
    "must be absolute"
);
refusal!(
    out_on_apple_books_usage_and_nothing_ran,
    |b| o("apple-books", Some(b), Some("/x/Saved.epub"), None, None),
    "route apple-books takes no --out"
);
refusal!(
    out_on_send_to_kindle_usage_and_nothing_ran,
    |b| o("send-to-kindle", Some(b), Some("/x/Saved.epub"), None, None),
    "route send-to-kindle takes no --out"
);
refusal!(
    out_on_email_to_kindle_usage_and_nothing_ran,
    |b| o(
        "email-to-kindle",
        Some(b),
        Some("/x/Saved.epub"),
        None,
        None
    ),
    "route email-to-kindle takes no --out"
);
refusal!(
    fountain_on_apple_books_usage_and_nothing_ran,
    |b| o("apple-books", Some(b), None, Some("/x.fountain"), None),
    "route apple-books takes no --fountain"
);
refusal!(
    options_json_on_send_to_kindle_usage_and_nothing_ran,
    |b| o("send-to-kindle", Some(b), None, None, Some("{}")),
    "route send-to-kindle takes no --options-json"
);
refusal!(
    fountain_on_save_epub_usage_and_nothing_ran,
    |b| o(
        "save-epub",
        Some(b),
        Some("/x/Saved.epub"),
        Some("/x.fountain"),
        None
    ),
    "route save-epub takes no --fountain"
);
refusal!(
    options_json_on_save_epub_usage_and_nothing_ran,
    |b| o(
        "save-epub",
        Some(b),
        Some("/x/Saved.epub"),
        None,
        Some("{}")
    ),
    "route save-epub takes no --options-json"
);

#[test]
fn an_unknown_key_names_every_key_route_takes() {
    let r = Rig::new();
    let epub = r.book("Script.epub");
    let (_, message) = refused(r.run(opts("carrier-pigeon", Some(&epub))));
    for key in [
        "apple-books",
        "send-to-kindle",
        "email-to-kindle",
        "save-epub",
        "save-kindle",
        "kindle-email-setup",
    ] {
        assert!(message.contains(key), "{message} lacks {key}");
    }
}

#[test]
fn a_book_that_is_not_there_is_unreadable_before_any_probe_or_open() {
    for key in ["apple-books", "save-epub"] {
        let r = Rig::new();
        let ghost = r.sb.p("ghost.epub");
        let out = (key == "save-epub").then_some("/x/Saved.epub");
        let (code, message) = refused(r.run(o(key, Some(&ghost), out, None, None)));
        assert_eq!(code, "unreadable");
        assert!(message.contains("ghost.epub"), "{message}");
        assert!(r.calls().is_empty());
        assert_eq!(r.facts_calls.get(), 0);
        assert!(!exists(&r.settings));
    }
}

#[test]
fn the_device_check_comes_before_the_book_check_a_device_key_with_no_book_still_points_at_send() {
    let r = Rig::new();
    let (code, message) = refused(r.run(opts("device:kindle", None)));
    assert_eq!(code, "usage");
    assert_eq!(message, TO_SEND);
}

// ---- the default opener -----------------------------------------------------
// Runs `sh`, never `open`: what is under test is how a real process's exit
// code and stderr come back, and that a missing program is an answer, not a
// panic.

#[cfg(unix)]
#[test]
fn the_default_opener_returns_the_exit_code_and_stderr_of_what_it_ran() {
    assert_eq!(
        real_opener(&argv(&["sh", "-c", "echo nope >&2; exit 3"])),
        Opened {
            code: 3,
            stderr: "nope\n".into()
        }
    );
    assert_eq!(
        real_opener(&argv(&["sh", "-c", "exit 0"])),
        Opened {
            code: 0,
            stderr: String::new()
        }
    );
}

#[test]
fn a_program_that_is_not_there_is_a_non_zero_code_with_a_reason_not_a_panic() {
    let sb = Sandbox::new();
    let answer = real_opener(&[sb.p("no-such-program")]);
    assert_ne!(answer.code, 0);
    assert!(!answer.stderr.is_empty());
}

// ---- the spawned CLI ----------------------------------------------------------

/// A sandbox whose `open`, `xdg-open` and `defaults` are stand-ins.
fn spawn_sandbox() -> Sandbox {
    let mut sb = Sandbox::new();
    fake_tools(&mut sb, &["open", "xdg-open", "defaults"]);
    sb
}

/// No spawned run opened an app, a page or a folder: the only stand-in
/// any run may have asked is `defaults` (the mail probe).
fn stayed_harmless(sb: &Sandbox) {
    let opened: Vec<String> = tools_log(sb)
        .into_iter()
        .filter(|l| !l.starts_with("defaults "))
        .collect();
    assert!(opened.is_empty(), "a spawned run opened {opened:?}");
}

fn settings_made(sb: &Sandbox) -> bool {
    sb.path("config/settings.json").exists()
}

#[test]
fn routes_is_a_verb_and_a_file_of_that_name_still_converts() {
    let sb = spawn_sandbox();
    let epub = book_at(&sb, "b/x.epub");
    assert_eq!(sb.run(&["routes", &epub, "--json"]).json()["ok"], true);
    sb.write("routes", "not a pdf");
    assert_eq!(
        sb.run(&["routes", "--json"]).error_code(),
        "unsupported-type"
    );
    stayed_harmless(&sb);
}

#[test]
fn routes_json_is_one_object_ok_every_route_the_save_rows_available_and_chosen_among_them() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["routes", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0);
    let answer = ran.json();
    assert_eq!(answer["ok"], true);
    let mut keys: Vec<&String> = answer.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["chosen", "ok", "routes"]);
    let rows = answer["routes"].as_array().unwrap();
    let by_key = |k: &str| rows.iter().find(|r| r["key"] == k).unwrap().clone();
    assert_eq!(by_key("save-epub")["available"], true);
    assert_eq!(by_key("save-kindle")["available"], true);
    assert!(rows.iter().any(|r| r["id"] == answer["chosen"]));
    if cfg!(target_os = "macos") {
        // The stand-in `defaults` exits 1, so the mail probe could not
        // tell, which is never Apple Mail: the row is dimmed with its setup
        // fix. That it is proves the real handler was not the one read.
        assert_eq!(by_key("email-to-kindle")["unavailable"], "setup");
        assert!(
            tools_log(&sb)
                .iter()
                .any(|l| l.starts_with("defaults read"))
        );
    }
    // Read-only: no settings file was made.
    assert!(!settings_made(&sb));
    stayed_harmless(&sb);
}

#[test]
fn routes_chosen_follows_the_settings_file_under_method_reader_config_dir() {
    let sb = spawn_sandbox();
    sb.write("config/settings.json", r#"{"lastRoute":"save-kindle"}"#);
    let ran = sb.run(&["routes", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.json()["chosen"], "save-kindle");
}

#[test]
fn a_connected_kindle_under_method_reader_volume_roots_is_listed_first() {
    let mut sb = spawn_sandbox();
    sb.mkdir("vols/Kindle/documents");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let answer = sb
        .run(&["routes", &book_at(&sb, "b/Script.epub"), "--json"])
        .json();
    let first = &answer["routes"][0];
    assert_eq!(first["key"], "device:kindle");
    assert_eq!(first["device"]["volume"], sb.p("vols/Kindle"));
}

#[test]
fn routes_without_json_one_line_per_route_one_star() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["routes", &book_at(&sb, "b/Script.epub")]);
    assert_eq!(ran.code, 0);
    assert!(!ran.stdout.trim().starts_with('{'));
    let lines: Vec<&str> = ran.stdout.trim().lines().collect();
    assert_eq!(lines.iter().filter(|l| l.starts_with('*')).count(), 1);
    assert!(ran.stdout.contains("save-epub"));
    assert!(ran.stdout.contains("(dimmed: "));
    stayed_harmless(&sb);
}

#[test]
fn quick_answers_without_asking_the_remarkable_and_says_the_tablet_is_still_being_looked_for() {
    // A stand-in tablet on this machine that takes every request and never
    // answers, so no real network is involved: a full `routes` waits out the
    // probe's whole timeout on it. The quick answer must not ask it at all.
    let tablet = Tablet::start(None, None);
    let mut sb = spawn_sandbox();
    sb.set("METHOD_READER_REMARKABLE_ENDPOINT", tablet.url.clone());
    let epub = book_at(&sb, "b/Script.epub");
    let row = |answer: &Value| {
        answer["routes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == "remarkable")
            .unwrap()
            .clone()
    };

    let quick = sb.run(&["routes", &epub, "--quick", "--json"]);
    assert_eq!(quick.code, 0);
    assert_eq!(
        row(&quick.json())["detail"],
        "checking whether one is docked\u{2026}"
    );
    assert!(tablet.requests().is_empty());

    // The full answer is the one that looks, and waits for it.
    let full = sb.run(&["routes", &epub, "--json"]);
    assert_eq!(full.code, 0);
    assert_eq!(row(&full.json())["detail"], "dock over USB to send");
    assert_eq!(tablet.requests().len(), 1);
}

#[test]
fn routes_a_missing_book_is_unreadable() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["routes", &sb.p("ghost.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn routes_takes_exactly_one_book() {
    let sb = spawn_sandbox();
    assert_eq!(sb.run(&["routes", "--json"]).error_code(), "usage");
    let two = sb.run(&[
        "routes",
        &book_at(&sb, "b1/Script.epub"),
        &book_at(&sb, "b2/Script.epub"),
        "--json",
    ]);
    assert_eq!(two.code, 1);
    assert_eq!(two.error_code(), "usage");
}

#[test]
fn routes_help_is_its_own_usage_not_the_conversion_flags() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["routes", "--help", "--json"]);
    assert_eq!(ran.code, 0);
    let usage = ran.json()["usage"].as_str().unwrap().to_owned();
    assert!(usage.contains("method-reader routes <file.epub>"));
    assert!(usage.contains("--quick"));
    assert!(!usage.contains("--mobi"));
    assert!(!usage.contains("--out"));
    assert!(!usage.contains(EM_DASH));
}

#[test]
fn the_main_usage_lists_routes() {
    let sb = spawn_sandbox();
    assert!(
        sb.run(&["--help"])
            .stdout
            .contains("method-reader routes <file.epub> [--quick] [--json]")
    );
}

// ---- method-reader route (through the CLI) ---------------------------------------
// Refusals and --help ONLY (the one spawned success, a save, has its own
// block below). No spawned run here names a key that opens anything: what
// those open is proven in process, above, with a stand-in opener. Every run
// below uses a device key, an unknown key, or a save aimed inside the
// scratch folder.

#[test]
fn route_is_a_verb_and_a_file_of_that_name_still_converts() {
    let sb = spawn_sandbox();
    // As a verb: a missing book is the handler's own refusal.
    let ghost = sb.p("x.epub");
    let ran = sb.run(&[
        "route",
        "save-epub",
        &ghost,
        "--out",
        &sb.p("o/x.epub"),
        "--json",
    ]);
    assert_eq!(ran.error_code(), "unreadable");
    sb.write("route", "not a pdf");
    assert_eq!(
        sb.run(&["route", "--json"]).error_code(),
        "unsupported-type"
    );
    stayed_harmless(&sb);
}

#[test]
fn route_help_is_its_own_usage_naming_every_key_kindle_email_setup_included() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["route", "--help", "--json"]);
    assert_eq!(ran.code, 0);
    let usage = ran.json()["usage"].as_str().unwrap().to_owned();
    assert!(usage.contains("method-reader route <key> <file.epub>"));
    assert!(usage.contains("method-reader route kindle-email-setup"));
    for key in [
        "apple-books",
        "send-to-kindle",
        "email-to-kindle",
        "save-epub",
        "save-kindle",
        "kindle-email-setup",
    ] {
        assert!(usage.contains(key), "{key}");
    }
    assert!(usage.contains("--out"));
    assert!(!usage.contains("--mobi"));
    assert!(!usage.contains("--device"));
    assert!(!usage.contains(EM_DASH));
}

#[test]
fn the_main_usage_lists_route() {
    let sb = spawn_sandbox();
    assert!(
        sb.run(&["--help"])
            .stdout
            .contains("method-reader route <key> <file.epub> [--out <path>] [--json]")
    );
}

#[test]
fn a_device_key_and_remarkable_point_at_send() {
    let sb = spawn_sandbox();
    let epub = book_at(&sb, "b/Script.epub");
    for key in ["device:kindle", "remarkable"] {
        let ran = sb.run(&["route", key, &epub, "--json"]);
        assert_eq!(ran.code, 1);
        assert_eq!(
            ran.json()["error"],
            json!({ "code": "usage", "message": TO_SEND })
        );
    }
    assert!(!settings_made(&sb));
    stayed_harmless(&sb);
}

#[test]
fn route_an_unknown_key_is_usage_naming_the_keys() {
    let sb = spawn_sandbox();
    let ran = sb.run(&[
        "route",
        "carrier-pigeon",
        &book_at(&sb, "b/Script.epub"),
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("save-epub"));
    stayed_harmless(&sb);
}

#[test]
fn route_no_key_at_all_is_usage() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["route", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn route_a_second_book_is_usage() {
    let sb = spawn_sandbox();
    let out = sb.p("never/Saved.epub");
    let ran = sb.run(&[
        "route",
        "save-epub",
        &book_at(&sb, "b1/Script.epub"),
        &book_at(&sb, "b2/Script.epub"),
        "--out",
        &out,
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(!Path::new(&out).exists());
}

#[test]
fn a_save_without_out_is_usage_naming_out() {
    let sb = spawn_sandbox();
    let ran = sb.run(&[
        "route",
        "save-epub",
        &book_at(&sb, "b/Script.epub"),
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("--out"));
    assert!(!settings_made(&sb));
}

#[test]
fn out_passes_the_verb_and_reaches_the_handler() {
    // Not refused as a flag route does not take: a missing book is the
    // handler's own unreadable, and a relative --out its own refusal.
    let sb = spawn_sandbox();
    let out = sb.p("never/Saved.epub");
    let ghost = sb.run(&[
        "route",
        "save-epub",
        &sb.p("ghost.epub"),
        "--out",
        &out,
        "--json",
    ]);
    assert_eq!(ghost.error_code(), "unreadable");
    let relative = sb.run(&[
        "route",
        "save-epub",
        &book_at(&sb, "b/Script.epub"),
        "--out",
        "Saved.epub",
        "--json",
    ]);
    assert_eq!(relative.error_code(), "usage");
    assert!(relative.error_message().contains("must be absolute"));
}

#[test]
fn fountain_and_options_json_reach_the_handler_which_refuses_them_for_save_epub() {
    // Refused by route_command, not by the verb: a verb that dropped either
    // flag on the way would let this save go ahead (into scratch).
    let sb = spawn_sandbox();
    let out = sb.p("never/Saved.epub");
    let epub = book_at(&sb, "b/Script.epub");
    for (flag, value) in [("--fountain", "/x.fountain"), ("--options-json", "{}")] {
        let ran = sb.run(&[
            "route",
            "save-epub",
            &epub,
            "--out",
            &out,
            flag,
            value,
            "--json",
        ]);
        assert_eq!(ran.code, 1);
        assert_eq!(
            ran.json()["error"],
            json!({
                "code": "usage",
                "message": format!("route save-epub takes no {flag} (only save-kindle builds a file)"),
            })
        );
    }
    assert!(!Path::new(&out).exists());
}

#[test]
fn fountain_and_options_json_pass_the_verb_for_save_kindle() {
    let sb = spawn_sandbox();
    let ran = sb.run(&[
        "route",
        "save-kindle",
        &sb.p("ghost.epub"),
        "--out",
        &sb.p("never/Saved.azw3"),
        "--fountain",
        &sb.p("ghost.fountain"),
        "--options-json",
        "{}",
        "--json",
    ]);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn without_json_a_refusal_is_one_line_on_stderr_not_json() {
    let sb = spawn_sandbox();
    let ran = sb.run(&["route", "remarkable", &book_at(&sb, "b/Script.epub")]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.stdout, "");
    assert_eq!(ran.stderr.trim(), format!("method-reader: {TO_SEND}"));
}

#[test]
fn every_verb_that_refuses_fountain_and_options_json_names_route_as_an_owner_too() {
    let sb = spawn_sandbox();
    assert_eq!(
        sb.run(&["devices", "--fountain", "x", "--json"])
            .error_message(),
        "devices takes no --fountain (--fountain belongs to export and route)"
    );
    let epub = book_at(&sb, "b/Script.epub");
    assert_eq!(
        sb.run(&["send", &epub, "--options-json", "{}", "--json"])
            .error_message(),
        "send takes no --options-json (--options-json belongs to export and route)"
    );
    assert_eq!(
        sb.run(&["routes", &epub, "--fountain", "x", "--json"])
            .error_message(),
        "routes takes no --fountain (--fountain belongs to export and route)"
    );
}

#[test]
fn no_refusal_remembered_anything_and_no_spawned_run_opened_an_app_a_page_or_a_folder() {
    let sb = spawn_sandbox();
    let epub = book_at(&sb, "b/Script.epub");
    let out = sb.p("never/Saved.epub");
    for args in [
        vec!["route", "device:kindle", &epub, "--json"],
        vec!["route", "remarkable", &epub, "--json"],
        vec!["route", "carrier-pigeon", &epub, "--json"],
        vec!["route", "--json"],
        vec!["route", "save-epub", &epub, "--json"],
        vec!["route", "save-epub", &epub, "--out", "Saved.epub", "--json"],
        vec![
            "route",
            "save-epub",
            &epub,
            "--out",
            &out,
            "--fountain",
            "/x.fountain",
            "--json",
        ],
        vec!["routes", &epub, "--json"],
    ] {
        let ran = sb.run(&args);
        assert!(ran.code == 0 || ran.code == 1, "{args:?}: {ran:?}");
    }
    assert!(!settings_made(&sb));
    assert!(!Path::new(&out).exists());
    stayed_harmless(&sb);
}

// ---- method-reader route save-epub: the one route that works when spawned --------
// A save opens nothing, and --out, the settings folder, the library and the
// mounts are all in the scratch folder. What the verb prints on success is
// proven here, for real.

#[test]
fn save_epub_json_one_object_ok_with_the_key_the_path_written_and_the_note_the_copy_is_there_and_remembered()
 {
    let sb = spawn_sandbox();
    let epub = book_at(&sb, "b/Script.epub");
    let out = sb.p("save/nested dir/Copy.epub");
    let ran = sb.run(&["route", "save-epub", &epub, "--out", &out, "--json"]);
    assert_eq!(ran.stderr, "");
    assert_eq!(ran.code, 0);
    assert_eq!(
        ran.json(),
        json!({ "ok": true, "key": "save-epub", "path": out, "note": format!("Saved to {out}.") })
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "book-bytes");
    assert_eq!(
        remembered(&sb.p("config/settings.json")),
        Some(json!({ "lastRoute": "save-epub" }))
    );
    stayed_harmless(&sb);
}

#[test]
fn save_epub_for_a_person_the_note_alone_one_line_no_json() {
    let sb = spawn_sandbox();
    let epub = book_at(&sb, "b/Script.epub");
    let out = sb.p("save/nested dir/Copy.epub");
    let ran = sb.run(&["route", "save-epub", &epub, "--out", &out]);
    assert_eq!(ran.stderr, "");
    assert_eq!(ran.code, 0);
    assert_eq!(ran.stdout, format!("Saved to {out}.\n"));
    assert!(Path::new(&out).exists());
    assert_eq!(
        remembered(&sb.p("config/settings.json")),
        Some(json!({ "lastRoute": "save-epub" }))
    );
    stayed_harmless(&sb);
}

// ---- the route branch, in the source ------------------------------------------
// It covers keys no spawned run may let through: every refusal the branch
// makes itself comes before the handler is called, so a refusal moved below
// it could not open Books first and refuse after.

#[test]
fn every_refusal_in_the_route_branch_comes_before_the_handler_is_called() {
    let source = std::fs::read_to_string(repo_root().join("rust/crates/sp-cli/src/verbs.rs"))
        .expect("verbs.rs");
    let start = source
        .find("\"route\" => {")
        .expect("the route branch moved");
    let end = start
        + source[start..]
            .find("_ => send(")
            .expect("the route branch lost its end marker");
    let text = &source[start..end];
    let called = text.find("route_command(").expect("the handler call");
    let count = |s: &str| s.matches("Stop::usage(").count();
    // The positional check. The foreign-flag refusal, every verb's, is made
    // once in run_verb above every branch.
    assert_eq!(count(&text[..called]), 1);
    assert_eq!(count(&text[called..]), 0);
    let flag_check = source
        .find("foreign_flag_refusal(verb, &parsed)")
        .expect("the foreign-flag check");
    assert!(flag_check > source.find("pub fn run_verb(").unwrap());
    assert!(flag_check < start);
}
