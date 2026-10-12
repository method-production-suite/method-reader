//! The capture tool without Chrome (was tests/capture.test.ts): the pure
//! parts (the gate, the page, the shot list), the page server driven with
//! plain requests, and whole runs with a fake browser and a stand-in
//! engine, down to what each one leaves behind. The Chrome driver is tested
//! against a fake Chrome in capture_chrome.rs. Real Chrome runs only when
//! someone runs `cargo xtask capture-screens`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use xtask::capture::backend::{Browser, CaptureOptions, DesktopUi};
use xtask::capture::gate::{GateAnswer, GateContext, gate_engine_call};
use xtask::capture::http::{Request, Response, fetch};
use xtask::capture::page::{CaptureConfig, capture_index, config_json};
use xtask::capture::run::{MARKER, RunError, RunOptions, claim_library, run_capture};
use xtask::capture::server::{EngineResult, Handler, HandlerOptions, Records, snapshot};
use xtask::capture::shots::{FRAME_PAD, Kind, SHOTS, Written, all_outputs, framed_size, write_if_changed};
use xtask::capture::window_argv as argv;
use xtask::paths::repo_root;

/// A fresh scratch folder for one test, removed when it is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "screepub-capture-test-{tag}-{}",
            xtask::capture::random_token()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // Canonical, so a path the run builds compares equal to one built here
        // (macOS's temp folder sits behind a symlink).
        Scratch(dir.canonicalize().unwrap())
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An absolute path on this platform: `/a/b` here, `C:\a\b` on Windows.
fn abs(rel: &str) -> PathBuf {
    let root = if cfg!(windows) { "C:\\" } else { "/" };
    let mut p = PathBuf::from(root);
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_string()).collect()
}

struct Ctx {
    demo: PathBuf,
    library: PathBuf,
}

impl Ctx {
    fn new() -> Ctx {
        Ctx {
            demo: abs("repo/tests/fixtures/field-station.pdf"),
            library: abs("Users/Shared/Documents/Screepub"),
        }
    }
    fn gate(&self) -> GateContext<'_> {
        GateContext {
            demo_pdf: &self.demo,
            library: &self.library,
        }
    }
    fn in_lib(&self, p: &str) -> String {
        s(&self.library.join("field-station").join(p))
    }
    fn ask(&self, args: &[String]) -> GateAnswer {
        gate_engine_call(args, &self.gate())
    }
}

// ---------------------------------------------------------------- the gate

#[test]
fn gate_allows_exactly_what_the_window_asks_for_on_the_way_to_the_pictures() {
    let c = Ctx::new();
    let d = s(&c.demo);
    assert_eq!(c.ask(&argv::version()), GateAnswer::Allow);
    assert_eq!(c.ask(&argv::convert(&d, false, None)), GateAnswer::Allow);
    assert_eq!(
        c.ask(&argv::settings(&c.in_lib("field-station.fountain"), None)),
        GateAnswer::Allow
    );
    assert_eq!(
        c.ask(&argv::reconvert(
            &c.in_lib("field-station.fountain"),
            &c.in_lib("field-station.epub"),
            "{}"
        )),
        GateAnswer::Allow
    );
    // The Convert page's library line reads the app settings every time the
    // drop area draws. A read: it writes nothing.
    assert_eq!(c.ask(&argv::app_settings(None)), GateAnswer::Allow);
}

#[test]
fn gate_allows_the_app_settings_read_only() {
    let c = Ctx::new();
    let mut with_set = argv::app_settings(None);
    with_set.extend(strings(&["--set", "{}"]));
    let mut with_debug = argv::app_settings(None);
    with_debug.push("--debug".into());
    for args in [
        argv::app_settings(Some(r#"{"libraryPath":"/Users/me/Books"}"#)),
        argv::app_settings(Some(r#"{"keepScriptSettings":false}"#)),
        argv::app_settings(Some(r#"{"formatDefaults":null}"#)),
        with_set,
        with_debug,
        strings(&["app-settings"]),
    ] {
        assert!(!c.ask(&args).allowed(), "{}", args.join(" "));
    }
}

#[test]
fn gate_refuses_copies_sends_builds_and_ejects_naming_each() {
    let c = Ctx::new();
    let e = c.in_lib("field-station.epub");
    for args in [
        argv::send(&e, None),
        argv::export(&e, "kindle", None, None, None),
        argv::eject("/Volumes/Kindle"),
        argv::route("send-to-kindle", &e, None, None, None),
    ] {
        match c.ask(&args) {
            GateAnswer::Allow => panic!("allowed {}", args.join(" ")),
            GateAnswer::Refuse(reason) => assert!(reason.contains(&args[0])),
        }
    }
}

#[test]
fn gate_allows_the_result_screens_reads() {
    let c = Ctx::new();
    let e = c.in_lib("field-station.epub");
    let mut all = vec![
        argv::devices(false),
        argv::devices(true),
        argv::routes(&e, false),
        argv::routes(&e, true),
        argv::kindle_check(&e),
    ];
    all.extend(
        ["kindle", "apple", "kobo", "tolino", "remarkable"]
            .iter()
            .map(|k| argv::reader_device(k)),
    );
    for args in all {
        assert!(c.ask(&args).allowed(), "{}", args.join(" "));
    }
}

#[test]
fn gate_refuses_a_book_outside_the_library_and_any_other_app_settings_write() {
    let c = Ctx::new();
    let evil = format!("{}-evil", s(&c.library));
    for args in [
        argv::routes(&s(&abs("Users/someone/elsewhere.epub")), false),
        argv::kindle_check(&s(&Path::new(&evil).join("x.epub"))),
        argv::reader_device("nook"),
        argv::app_settings(Some(r#"{"readerDevice":"kobo","libraryPath":"/x"}"#)),
        argv::export(
            &c.in_lib("field-station.epub"),
            "epub",
            None,
            None,
            Some(&c.in_lib("copy.epub")),
        ),
    ] {
        assert!(!c.ask(&args).allowed(), "{}", args.join(" "));
    }
}

#[test]
fn gate_refuses_a_conversion_of_any_file_but_the_demo_script() {
    let c = Ctx::new();
    match c.ask(&argv::convert(&s(&abs("Users/someone/real-script.pdf")), false, None)) {
        GateAnswer::Allow => panic!("allowed"),
        GateAnswer::Refuse(r) => assert!(r.contains("real-script.pdf")),
    }
}

#[test]
fn gate_refuses_a_rerender_that_would_write_outside_the_library() {
    let c = Ctx::new();
    let a = c.ask(&argv::reconvert(
        &c.in_lib("field-station.fountain"),
        &s(&abs("tmp/elsewhere.epub")),
        "{}",
    ));
    assert!(!a.allowed());
    // A path that merely STARTS with the library's name is not inside it.
    let evil = format!("{}-evil", s(&c.library));
    let b = c.ask(&argv::settings(&s(&Path::new(&evil).join("x.fountain")), None));
    assert!(!b.allowed());
}

#[test]
fn gate_refuses_an_empty_call_rather_than_allow_by_default() {
    assert!(!Ctx::new().ask(&[]).allowed());
}

#[test]
fn gate_refuses_any_call_whose_whole_shape_the_window_would_not_produce() {
    // Each is CLOSE to a real call but not one the window's own argv
    // builders can produce. A gate that only checks the first argument, or
    // only checks that -o lands in the library, allows every one of these.
    let c = Ctx::new();
    let d = s(&c.demo);
    let f = c.in_lib("field-station.fountain");
    let e = c.in_lib("field-station.epub");
    let plus = |mut a: Vec<String>, more: &[&str]| {
        a.extend(strings(more));
        a
    };
    let up = format!(
        "{}/../../../Users/me/x.fountain",
        s(&c.library).replace('\\', "/")
    );
    let refusals: Vec<(&str, Vec<String>)> = vec![
        (
            "a flag smuggled onto a convert after --library",
            plus(argv::convert(&d, false, None), &["--fountain", "/Users/me/real.fountain"]),
        ),
        (
            "a second flag smuggled onto a convert after --library",
            plus(argv::convert(&d, false, None), &["--preview-html", "/anywhere.html"]),
        ),
        ("--output instead of -o", plus(argv::convert(&d, false, None), &["--output", "/x.epub"])),
        ("--output= form", plus(argv::convert(&d, false, None), &["--output=/x.epub"])),
        ("-o glued to its value", plus(argv::convert(&d, false, None), &["-o/x.epub"])),
        (
            "a repeated -o",
            vec![
                f.clone(),
                "--json".into(),
                "--preview-inline".into(),
                "-o".into(),
                e.clone(),
                "-o".into(),
                "/x.epub".into(),
                "--options-json".into(),
                "{}".into(),
            ],
        ),
        // Missing --progress/--preview-inline/--library, on purpose.
        (
            "a convert without --library, which would write into tests/fixtures",
            vec![d.clone(), "--json".into()],
        ),
        ("an unknown flag after --library", plus(argv::convert(&d, false, None), &["--options", "/any/file"])),
        (
            "a flag smuggled onto a reconvert after --options-json",
            plus(argv::reconvert(&f, &e, "{}"), &["--output=/x.epub"]),
        ),
        ("a .. escape out of the library", argv::settings(&up, None)),
        ("a relative demo path", argv::convert("tests/fixtures/field-station.pdf", false, None)),
        ("a relative settings path", argv::settings("field-station/x.fountain", None)),
        ("flag-first", vec!["--json".into(), "settings".into(), f.clone()]),
        ("update-decision", strings(&["update-decision", "--offered", "1", "--current", "0"])),
        ("update-should-check", strings(&["update-should-check", "--json"])),
        ("extra args on version", strings(&["--version", "--json", "--debug"])),
        ("kfx-status: the KFX checklist is never captured", argv::kfx_status()),
        ("kfx-install: a capture must never install anything", argv::kfx_install()),
    ];
    for (label, args) in refusals {
        assert!(!c.ask(&args).allowed(), "{label}");
    }
}

#[test]
fn gate_allows_every_shape_the_window_really_sends_on_the_way_to_a_picture() {
    let c = Ctx::new();
    let d = s(&c.demo);
    let f = c.in_lib("field-station.fountain");
    for args in [
        argv::convert(&d, true, None),
        argv::convert(&d, false, Some(r#"{"a":1}"#)),
        argv::convert(&d, true, Some("{}")),
        argv::settings(&f, Some(r#"{"a":1}"#)),
    ] {
        assert_eq!(c.ask(&args), GateAnswer::Allow, "{}", args.join(" "));
    }
}

/// New with the port: the gate's builders are a copy of app.js's, so this
/// holds the copy to the original (window_argv.rs explains).
#[test]
fn the_gates_builders_are_still_the_ones_app_js_spells() {
    let app = std::fs::read_to_string(repo_root().join("desktop/ui/app.js")).unwrap();
    assert!(app.contains("export const FORCE_FLAG = '--force';"));
    for (name, copied) in argv::SOURCES {
        let now = argv::source_in(&app, name)
            .unwrap_or_else(|| panic!("app.js has no argv.{name} any more"));
        assert_eq!(
            &now, copied,
            "argv.{name} changed in desktop/ui/app.js; update capture/window_argv.rs to match"
        );
    }
}

// ---------------------------------------------------------------- the page

fn real_index() -> String {
    std::fs::read_to_string(repo_root().join("desktop/ui/index.html")).unwrap()
}

const CFG: CaptureConfig<'static> = CaptureConfig {
    shot: "result",
    demo_pdf: "/Users/Shared/demo.pdf",
    token: "t",
};

#[test]
fn page_is_the_windows_own_index_with_the_capture_scripts_before_main() {
    let real = real_index();
    let out = capture_index(&real, &CFG).unwrap();
    // Every stylesheet the window loads, still loaded.
    for part in real.split(r#"<link rel="stylesheet" href=""#).skip(1) {
        let href = part.split('"').next().unwrap();
        assert!(out.contains(&format!("href=\"{href}\"")), "{href}");
    }
    let bridge = out.find("/tools/capture/bridge.js").unwrap();
    let main = out.find(r#"src="main.js""#).unwrap();
    let steps = out.find("/tools/capture/steps.js").unwrap();
    assert!(bridge > 0 && bridge < main && steps > main);
    assert!(out.contains(r#"<base href="/desktop/ui/">"#));
    assert!(out.contains(r#"{"shot":"result","demoPdf":"/Users/Shared/demo.pdf","token":"t"}"#));
    assert!(out.contains(&config_json(&CFG)));
}

#[test]
fn page_fails_loudly_for_a_window_page_it_does_not_recognise() {
    let err = capture_index("<html><body>nothing</body></html>", &CFG).unwrap_err();
    assert!(err.contains("main.js"));
    let twice = real_index().replace(
        "</body>",
        r#"<script type="module" src="main.js"></script></body>"#,
    );
    assert!(capture_index(&twice, &CFG).unwrap_err().contains("exactly once"));
}

#[test]
fn page_value_cannot_close_the_script_early_or_be_read_as_a_pattern() {
    let evil = CaptureConfig {
        shot: "</script><script>alert(1)</script>",
        demo_pdf: "/Users/a$&b/demo.pdf",
        token: "t",
    };
    let out = capture_index(&real_index(), &evil).unwrap();
    assert!(out.contains("/Users/a$&b/demo.pdf"));
    assert!(!out.contains("</script><script>alert"));
}

#[test]
fn page_head_with_attributes_is_not_recognised_and_fails_loudly() {
    let html = r#"<html><head lang="en"></head><body><script type="module" src="main.js"></script></body></html>"#;
    assert!(capture_index(html, &CFG).unwrap_err().contains("no <head>"));
}

#[test]
fn no_file_in_desktop_ui_knows_the_capture_tool_exists() {
    let ui = repo_root().join("desktop/ui");
    for e in std::fs::read_dir(&ui).unwrap() {
        let p = e.unwrap().path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !matches!(ext, "js" | "html" | "css") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let knows = ["__CAPTURE__", "__captureState", "tools/capture"]
            .iter()
            .any(|w| text.contains(w));
        assert!(!knows, "{}", p.display());
    }
}

// ---------------------------------------------------------------- shots

#[test]
fn shots_are_the_four_readme_pictures_with_light_and_dark_for_the_window() {
    let names: Vec<&str> = SHOTS.iter().map(|s| s.name).collect();
    assert_eq!(names, ["hero", "drop", "result", "read"]);
    for s in SHOTS {
        let words: Vec<&str> = s.themes.iter().map(|t| t.word()).collect();
        let want: &[&str] = if s.kind == Kind::Window { &["light", "dark"] } else { &["light"] };
        assert_eq!(words, want);
    }
}

#[test]
fn window_shots_are_the_windows_own_size() {
    let conf: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("desktop/src-tauri/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    let w = &conf["app"]["windows"][0];
    for s in SHOTS.iter().filter(|s| s.kind == Kind::Window) {
        assert_eq!(json!([s.width, s.height]), json!([w["width"], w["height"]]));
    }
}

#[test]
fn readme_gets_every_picture_and_the_site_the_light_drop_and_result() {
    let all = all_outputs(SHOTS);
    assert_eq!(all.iter().filter(|p| p.starts_with("assets/screens/")).count(), 7);
    let mut site: Vec<&String> = all.iter().filter(|p| p.starts_with("site/img/")).collect();
    site.sort();
    assert_eq!(site, ["site/img/drop-light.png", "site/img/result-light.png"]);
}

#[test]
fn a_file_is_written_only_when_its_bytes_differ() {
    let dir = Scratch::new("write");
    let path = dir.path().join("x.png");
    assert_eq!(write_if_changed(&path, &[1, 2, 3]).unwrap(), Written::Written);
    assert_eq!(write_if_changed(&path, &[1, 2, 3]).unwrap(), Written::Unchanged);
    assert_eq!(write_if_changed(&path, &[1, 2, 4]).unwrap(), Written::Written);
    assert_eq!(std::fs::read(&path).unwrap(), [1, 2, 4]);
}

#[test]
fn the_capture_command_and_chrome_driver_are_wired() {
    // Was "they import without running, so tsc covers them": the compiler
    // covers that now; what is left is the command row and the path.
    assert!(
        xtask::COMMANDS
            .iter()
            .any(|c| c.name == "capture-screens")
    );
    assert_eq!(
        xtask::capture::cdp::CHROME,
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    );
}

// ---------------------------------------------------------------- the server

const HOST: &str = "127.0.0.1:4321";

struct ServerScene {
    _dir: Scratch,
    handler: Handler,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    answer: Arc<Mutex<EngineResult>>,
    records: Records,
}

impl ServerScene {
    /// A throwaway repository with the files a careless static server would
    /// hand out: the gitignored REAL screenplays, git's own files, the
    /// package manifest, and a sibling folder whose name starts with the
    /// repo's.
    fn new() -> ServerScene {
        let dir = Scratch::new("server");
        let repo = dir.path().join("Screepub");
        let put = |rel: &str, body: &str| {
            let p = repo.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        };
        put("desktop/ui/index.html", &real_index());
        put("desktop/ui/main.js", "// main");
        put("site/index.html", "<!doctype html><title>site</title>");
        put("tools/capture/steps.js", "// steps");
        put("fixtures/real-script.pdf", "A REAL SCREENPLAY");
        put(".git/HEAD", "ref: refs/heads/main");
        put("package.json", "{}");
        std::fs::create_dir_all(dir.path().join("Screepub-old")).unwrap();
        std::fs::write(dir.path().join("Screepub-old/x"), "not served").unwrap();
        let pass_one = dir.path().join("pass-one");
        std::fs::create_dir_all(&pass_one).unwrap();
        std::fs::write(pass_one.join("drop-light.png"), "PNG").unwrap();
        let calls: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
        let answer = Arc::new(Mutex::new(EngineResult {
            code: Some(0),
            stdout: "{\"ok\":true}\n".into(),
            stderr: String::new(),
        }));
        let records = Records::default();
        let (c2, a2) = (Arc::clone(&calls), Arc::clone(&answer));
        let handler = Handler::new(HandlerOptions {
            demo_pdf: repo.join("tests/fixtures/field-station.pdf"),
            repo_dir: repo,
            pass_one,
            token: "secret".into(),
            library: abs("Users/Shared/Documents/Screepub"),
            host: HOST.into(),
            run_engine: Arc::new(move |args: &[String]| {
                c2.lock().unwrap().push(args.to_vec());
                a2.lock().unwrap().clone()
            }),
            records: records.clone(),
        });
        ServerScene {
            _dir: dir,
            handler,
            calls,
            answer,
            records,
        }
    }

    fn get(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut r = Request::get(HOST, path);
        for (n, v) in headers {
            r = r.with_header(n, v);
        }
        self.handler.handle(&r).unwrap()
    }

    fn engine_raw(&self, body: &str, headers: &[(&str, &str)]) -> Response {
        let mut r = Request::get(HOST, "/engine").with_header("content-type", "application/json");
        r.method = "POST".into();
        r.body = body.as_bytes().to_vec();
        for (n, v) in headers {
            r = r.with_header(n, v);
        }
        self.handler.handle(&r).unwrap()
    }

    fn engine(&self, args: &[String]) -> Response {
        self.engine_raw(&json!({ "args": args }).to_string(), &[("x-capture-token", "secret")])
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[test]
fn server_serves_the_window_the_site_and_the_capture_scripts() {
    let s = ServerScene::new();
    for (path, kind) in [
        ("/desktop/ui/main.js", "text/javascript"),
        ("/site/index.html", "text/html"),
        ("/tools/capture/steps.js", "text/javascript"),
    ] {
        let r = s.get(path, &[]);
        assert_eq!(r.status, 200, "{path}");
        assert!(r.content_type.contains(kind), "{path}");
    }
}

#[test]
fn server_serves_nothing_outside_the_window_the_site_and_the_capture_scripts() {
    let s = ServerScene::new();
    // Every one of these exists on disk, so a 404 is the rule, not luck.
    for path in [
        "/fixtures/real-script.pdf",
        "/.git/HEAD",
        "/package.json",
        "/..%2fScreepub-old%2fx",
        "/site/..%2f..%2ffixtures%2freal-script.pdf",
        "/site/%2e%2e/package.json",
        "/site/",
        "/site",
        "/desktop/ui",
        // New with the port: a backslash is a separator on Windows.
        "/site/..%5c..%5cpackage.json",
    ] {
        assert_eq!(s.get(path, &[]).status, 404, "{path}");
    }
    assert_eq!(s.get("/site/%E0%A4%A", &[]).status, 400);
    // Every 404 is written down, so a capture that times out can say which
    // file the page asked for and did not get.
    let nf = snapshot(&s.records.not_found);
    assert!(nf.contains(&"/.git/HEAD".to_string()));
    assert!(nf.contains(&"/..%2fScreepub-old%2fx".to_string()));
}

#[test]
fn server_refuses_any_other_host_so_a_rebound_name_cannot_reach_it() {
    let s = ServerScene::new();
    assert_eq!(s.get("/site/index.html", &[("host", "evil.example:4321")]).status, 403);
    assert_eq!(s.get("/site/index.html", &[("host", "localhost:4321")]).status, 403);
    let r = s.engine_raw(
        &json!({ "args": argv::version() }).to_string(),
        &[("host", "evil.example:4321"), ("x-capture-token", "secret")],
    );
    assert_eq!(r.status, 403);
    assert!(s.calls().is_empty());
}

#[test]
fn server_engine_needs_the_run_token_and_refuses_what_the_gate_refuses() {
    let s = ServerScene::new();
    let body = json!({ "args": argv::version() }).to_string();
    assert_eq!(s.engine_raw(&body, &[]).status, 403);
    assert_eq!(s.engine_raw(&body, &[("x-capture-token", "guess")]).status, 403);
    assert!(s.calls().is_empty());
    let r = s.engine(&argv::eject("/Volumes/Kindle"));
    assert_eq!(r.status, 403);
    assert!(r.text().contains("eject /Volumes/Kindle --json"));
    let refused = snapshot(&s.records.refused);
    assert_eq!(refused.len(), 1);
    assert!(refused[0].contains("eject /Volumes/Kindle --json"));
    assert!(s.calls().is_empty());
    assert_eq!(s.engine_raw("not json", &[("x-capture-token", "secret")]).status, 400);
    assert_eq!(
        s.engine_raw(r#"{"args":[1,2]}"#, &[("x-capture-token", "secret")]).status,
        400
    );
    assert_eq!(s.get("/engine", &[("x-capture-token", "secret")]).status, 405);
}

#[test]
fn server_runs_an_allowed_call_with_exactly_its_arguments() {
    let s = ServerScene::new();
    let r = s.engine(&argv::version());
    assert_eq!(r.status, 200);
    assert_eq!(r.text(), r#"{"ok":true}"#);
    assert_eq!(s.calls(), vec![argv::version()]);
    assert!(snapshot(&s.records.failed).is_empty());
}

#[test]
fn server_answers_500_for_an_engine_that_prints_nothing_as_sidecar_rs_does() {
    let s = ServerScene::new();
    *s.answer.lock().unwrap() = EngineResult {
        code: Some(1),
        stdout: String::new(),
        stderr: "TypeError: boom\n".into(),
    };
    let r = s.engine(&argv::version());
    assert_eq!(r.status, 500);
    assert!(r.text().contains("TypeError: boom"));
    let failed = snapshot(&s.records.failed);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].contains("TypeError: boom"));
}

#[test]
fn server_passes_through_a_nonzero_exit_with_an_answer_and_records_it() {
    // sidecar.rs: an engine error exits 1 and prints a perfectly good error
    // object, which the window draws as its refusal. The run still fails.
    let s = ServerScene::new();
    *s.answer.lock().unwrap() = EngineResult {
        code: Some(1),
        stdout: r#"{"ok":false,"error":{"code":"scanned"}}"#.into(),
        stderr: "no text\n".into(),
    };
    let r = s.engine(&argv::version());
    assert_eq!(r.status, 200);
    assert!(r.text().contains("\"scanned\""));
    let failed = snapshot(&s.records.failed);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].contains("exited with 1"));
}

#[test]
fn server_serves_the_window_page_only_for_a_window_shot_with_the_token() {
    let s = ServerScene::new();
    assert_eq!(s.get("/capture/window.html?shot=hero", &[]).status, 400);
    assert_eq!(s.get("/capture/window.html?shot=nope", &[]).status, 400);
    assert_eq!(s.get("/capture/window.html", &[]).status, 400);
    let r = s.get("/capture/window.html?shot=result", &[]);
    assert_eq!(r.status, 200);
    let html = r.text();
    assert!(html.contains(r#""token":"secret""#));
    assert!(html.contains("/tools/capture/bridge.js"));
}

#[test]
fn server_serves_pass_one_pictures_by_bare_name_only() {
    let s = ServerScene::new();
    assert_eq!(s.get("/pass-one/drop-light.png", &[]).status, 200);
    assert_eq!(
        s.get("/pass-one/..%2f..%2fScreepub%2fpackage.json", &[]).status,
        404
    );
    assert_eq!(s.get("/pass-one/missing-light.png", &[]).status, 404);
    assert!(snapshot(&s.records.not_found).contains(&"/pass-one/missing-light.png".to_string()));
}

// ---------------------------------------------------------------- runs

const PNG: &[u8] = &[137, 80, 78, 71];

type CaptureFn = Box<dyn FnMut(&str) -> Result<Vec<u8>, String>>;
type CloseFn = Box<dyn FnMut() -> Result<(), String>>;

struct FakeBrowser {
    capture: CaptureFn,
    close: CloseFn,
    events: Arc<Mutex<Vec<String>>>,
}

impl Browser for FakeBrowser {
    fn version(&self) -> String {
        "FakeChrome/1.0".into()
    }
    fn capture(&mut self, o: &CaptureOptions<'_>) -> Result<Vec<u8>, String> {
        (self.capture)(&o.url)
    }
    fn close(&mut self) -> Result<(), String> {
        self.events.lock().unwrap().push("closed".into());
        (self.close)()
    }
}

fn fake(capture: CaptureFn) -> FakeBrowser {
    FakeBrowser {
        capture,
        close: Box::new(|| Ok(())),
        events: Arc::default(),
    }
}

/// Every path is under a scratch folder. The tests never touch /Users/Shared.
struct RunScene {
    dir: Scratch,
    documents: PathBuf,
    library: PathBuf,
    tmp: PathBuf,
    out: PathBuf,
    lines: Arc<Mutex<Vec<String>>>,
}

impl RunScene {
    fn new(documents_exist: bool) -> RunScene {
        let dir = Scratch::new("run");
        let documents = dir.path().join("Documents");
        if documents_exist {
            std::fs::create_dir(&documents).unwrap();
        }
        let tmp = dir.path().join("tmp");
        std::fs::create_dir(&tmp).unwrap();
        RunScene {
            library: documents.join("Screepub"),
            out: dir.path().join("out"),
            documents,
            tmp,
            dir,
            lines: Arc::default(),
        }
    }

    fn options(&self, browser: Result<FakeBrowser, String>) -> RunOptions {
        let root = repo_root();
        let lines = Arc::clone(&self.lines);
        RunOptions {
            shots: SHOTS.to_vec(),
            demo_pdf: root.join("tests/fixtures/field-station.pdf"),
            repo_dir: root,
            library: self.library.clone(),
            launch: Box::new(move || browser.map(|b| Box::new(b) as Box<dyn Browser>)),
            window: Box::new(DesktopUi),
            engine: vec!["false".into()],
            out_dir: self.out.clone(),
            tmp: Some(self.tmp.clone()),
            calibre_config: Some(self.dir.path().join("no-calibre-here")),
            log: Box::new(move |l| lines.lock().unwrap().push(l.to_string())),
            interrupt: None,
        }
    }

    fn tmp_entries(&self) -> Vec<String> {
        entries(&self.tmp)
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn failed(r: Result<(), RunError>) -> RunError {
    r.expect_err("expected the run to fail")
}

#[test]
fn run_chrome_failing_to_start_leaves_nothing_behind_and_its_error_is_the_one_returned() {
    let s = RunScene::new(false);
    let e = failed(run_capture(s.options(Err("no Chrome today".into()))));
    assert_eq!(e.message, "no Chrome today");
    assert!(!s.library.exists());
    // The run created Documents, so the run removes it.
    assert!(!s.documents.exists());
    assert!(s.tmp_entries().is_empty());
}

#[test]
fn run_a_documents_folder_that_was_there_before_is_still_there_after() {
    let s = RunScene::new(true);
    let e = failed(run_capture(s.options(Err("no Chrome today".into()))));
    assert_eq!(e.message, "no Chrome today");
    assert!(!s.library.exists());
    assert!(s.documents.exists());
}

#[test]
fn run_that_fails_part_way_writes_no_picture_closes_chrome_and_returns_the_capture_error() {
    let s = RunScene::new(false);
    let mut n = 0;
    let b = fake(Box::new(move |_| {
        n += 1;
        if n == 6 {
            Err("capture failed at the read shot".into())
        } else {
            Ok(PNG.to_vec())
        }
    }));
    let events = Arc::clone(&b.events);
    let e = failed(run_capture(s.options(Ok(b))));
    assert_eq!(e.message, "capture failed at the read shot");
    assert_eq!(*events.lock().unwrap(), ["closed"]);
    assert!(!s.out.exists());
    assert!(!s.library.exists());
    assert!(!s.documents.exists());
    assert!(s.tmp_entries().is_empty());
}

#[test]
fn run_a_cleanup_failure_is_reported_after_the_original_error_never_instead() {
    let s = RunScene::new(false);
    let mut b = fake(Box::new(|_| Err("capture failed".into())));
    b.close = Box::new(|| Err("Chrome would not die".into()));
    let e = failed(run_capture(s.options(Ok(b))));
    assert_eq!(e.message, "capture failed");
    assert_eq!(e.cleanup.len(), 1);
    assert!(e.cleanup[0].contains("Chrome would not die"));
    // The steps after the one that failed still ran.
    assert!(!s.library.exists());
    assert!(s.tmp_entries().is_empty());
}

#[test]
fn run_writes_every_picture_and_each_window_shot_starts_from_an_empty_library() {
    let s = RunScene::new(false);
    let seen: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let (seen2, lib) = (Arc::clone(&seen), s.library.clone());
    let b = fake(Box::new(move |url| {
        if url.contains("/capture/window.html") {
            seen2.lock().unwrap().push(entries(&lib));
            // What a conversion leaves behind, which the next shot must not see.
            std::fs::create_dir_all(lib.join("field-station")).unwrap();
            std::fs::write(lib.join("field-station/field-station.epub"), "book").unwrap();
        }
        Ok(PNG.to_vec())
    }));
    run_capture(s.options(Ok(b))).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 6);
    for e in seen.iter() {
        assert_eq!(e, &[MARKER]);
    }
    let all = all_outputs(SHOTS);
    for out in &all {
        assert_eq!(std::fs::read(s.out.join(out)).unwrap(), PNG);
    }
    let lines = s.lines.lock().unwrap();
    assert!(lines[0].contains("FakeChrome/1.0"));
    let mut rest: Vec<String> = lines[1..].to_vec();
    rest.sort();
    let mut want: Vec<String> = all.iter().map(|o| format!("written  {o}")).collect();
    want.sort();
    assert_eq!(rest, want);
    assert!(!s.library.exists());
    assert!(!s.documents.exists());
}

/// A pid that belonged to a process a moment ago and to nothing now.
fn dead_pid() -> u32 {
    let mut c = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "exit"]);
        c
    } else {
        std::process::Command::new("true")
    };
    let mut child = c.spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn run_clears_a_library_left_by_an_interrupted_run_and_what_it_created() {
    let s = RunScene::new(false);
    // An interrupted run whose process is gone: it created Documents and the
    // library, marked the library, and never got to clean up.
    claim_library(&s.library, dead_pid()).unwrap();
    std::fs::write(s.library.join("half-a-book.epub"), "x").unwrap();
    let e = failed(run_capture(s.options(Err("no Chrome today".into()))));
    assert_eq!(e.message, "no Chrome today");
    assert!(!s.library.exists());
    assert!(!s.documents.exists());
}

#[test]
fn run_refuses_a_library_a_running_capture_is_using_naming_its_pid() {
    let s = RunScene::new(false);
    // Marked by this very process, which is certainly alive.
    let pid = std::process::id();
    claim_library(&s.library, pid).unwrap();
    std::fs::write(s.library.join("book-in-progress.epub"), "x").unwrap();
    let launched = Arc::new(Mutex::new(false));
    let mut o = s.options(Ok(fake(Box::new(|_| Ok(PNG.to_vec())))));
    let l2 = Arc::clone(&launched);
    o.launch = Box::new(move || {
        *l2.lock().unwrap() = true;
        Err("launched".into())
    });
    let e = failed(run_capture(o));
    assert!(e.expected);
    assert!(
        e.message
            .contains(&format!("capture pid {pid} is using {}", s.library.display()))
    );
    assert!(!*launched.lock().unwrap());
    assert_eq!(entries(&s.library), [MARKER, "book-in-progress.epub"]);
}

#[test]
fn run_refuses_a_marker_that_does_not_say_whose_it_is() {
    let s = RunScene::new(false);
    std::fs::create_dir_all(&s.library).unwrap();
    std::fs::write(s.library.join(MARKER), "{\"made\":\"/\"}\n").unwrap();
    let e = failed(run_capture(s.options(Err("not reached".into()))));
    assert!(e.expected);
    assert!(e.message.contains("pid"));
    assert_eq!(entries(&s.library), [MARKER]);
}

/// The window's boot: read the token off the window page, post an engine call.
fn post_version(url: &str) -> Result<(u16, String), String> {
    let (_, html) = fetch("GET", url, &[], &[], Duration::from_secs(10))?;
    let html = String::from_utf8_lossy(&html).into_owned();
    let token = html
        .split("\"token\":\"")
        .nth(1)
        .and_then(|t| t.split('"').next())
        .ok_or("no token on the window page")?
        .to_string();
    let base = &url[..url.find("/capture/").unwrap()];
    let body = json!({ "args": argv::version() }).to_string();
    let (status, said) = fetch(
        "POST",
        &format!("{base}/engine"),
        &[("content-type", "application/json"), ("x-capture-token", &token)],
        body.as_bytes(),
        Duration::from_secs(40),
    )?;
    Ok((status, String::from_utf8_lossy(&said).into_owned()))
}

fn drop_only(o: &mut RunOptions) {
    o.shots = SHOTS.iter().filter(|s| s.name == "drop").copied().collect();
}

#[cfg(unix)]
fn pgrep(pattern: &str) -> String {
    let out = std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(unix)]
#[test]
fn run_cleanup_refuses_an_engine_call_made_while_it_runs_instead_of_waiting() {
    let s = RunScene::new(false);
    // A stand-in engine that runs until it is stopped, named by a duration
    // no other run of this suite uses, so nothing else is found or killed.
    let seconds = format!("29.{}{:06}", std::process::id(), rand6());
    let pattern = format!("sleep {}$", seconds.replace('.', "\\."));
    let url_seen: Arc<Mutex<Option<String>>> = Arc::default();
    let during: Arc<Mutex<Option<(u16, String)>>> = Arc::default();
    let (u2, p2) = (Arc::clone(&url_seen), pattern.clone());
    let mut b = fake(Box::new(move |url| {
        if !url.contains("/capture/window.html") {
            return Ok(PNG.to_vec());
        }
        *u2.lock().unwrap() = Some(url.to_string());
        let url = url.to_string();
        std::thread::spawn(move || post_version(&url));
        while pgrep(&p2).is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("capture failed".into())
    }));
    // Chrome closes AFTER the engines are stopped, so a page still alive at
    // this point can post a new call. It must be refused, not run.
    let (u3, d3) = (Arc::clone(&url_seen), Arc::clone(&during));
    b.close = Box::new(move || {
        let url = u3.lock().unwrap().clone().unwrap();
        *d3.lock().unwrap() = Some(post_version(&url)?);
        Ok(())
    });
    let mut o = s.options(Ok(b));
    drop_only(&mut o);
    o.engine = vec!["/bin/sh".into(), "-c".into(), format!("exec sleep {seconds}")];
    let e = failed(run_capture(o));
    assert_eq!(e.message, "capture failed");
    let (status, body) = during.lock().unwrap().clone().unwrap();
    assert_eq!(status, 500);
    assert!(body.contains("cleaning up"), "{body}");
    assert!(pgrep(&pattern).is_empty());
    assert!(!s.library.exists());
}

#[cfg(unix)]
fn rand6() -> u32 {
    let t = xtask::capture::random_token();
    u32::from_str_radix(&t[..5], 16).unwrap() % 1_000_000
}

/// One engine call is enough: runs `script` as the engine and answers what
/// the window's call got back.
#[cfg(unix)]
fn one_engine_call(s: &RunScene, script: &str, calibre: Option<PathBuf>) -> String {
    let said: Arc<Mutex<String>> = Arc::default();
    let s2 = Arc::clone(&said);
    let b = fake(Box::new(move |url| {
        if !url.contains("/capture/window.html") {
            return Ok(PNG.to_vec());
        }
        *s2.lock().unwrap() = post_version(url)?.1;
        Err("one engine call is enough for this test".into())
    }));
    let mut o = s.options(Ok(b));
    drop_only(&mut o);
    o.engine = vec!["/bin/sh".into(), "-c".into(), script.into()];
    if calibre.is_some() {
        o.calibre_config = calibre;
    }
    let e = failed(run_capture(o));
    assert_eq!(e.message, "one engine call is enough for this test");
    said.lock().unwrap().clone()
}

#[cfg(unix)]
#[test]
fn run_gives_the_engine_its_own_screepub_config_dir_never_the_real_settings() {
    let s = RunScene::new(false);
    let said = one_engine_call(&s, "printf %s \"$SCREEPUB_CONFIG_DIR\"", None);
    // Under the run's own scratch library, a folder the run never writes into.
    assert_eq!(said, s.library.join(".capture-app-settings").to_string_lossy());
}

#[cfg(unix)]
#[test]
fn run_gives_the_engine_a_scratch_copy_of_calibres_settings_never_the_real_folder() {
    let s = RunScene::new(false);
    let real = s.dir.path().join("real-calibre");
    std::fs::create_dir_all(real.join("plugins")).unwrap();
    std::fs::write(real.join("plugins/KFX Output.zip"), "plugin").unwrap();
    let said = one_engine_call(
        &s,
        "printf \"%s|\" \"$CALIBRE_CONFIG_DIRECTORY\"; ls \"$CALIBRE_CONFIG_DIRECTORY/plugins\"",
        Some(real.clone()),
    );
    let (dir, listed) = said.split_once('|').unwrap();
    assert!(!dir.is_empty());
    assert_ne!(Path::new(dir), real);
    assert_ne!(Path::new(dir), xtask::capture::run::default_calibre_config());
    assert!(Path::new(dir).starts_with(&s.tmp));
    assert_eq!(listed.trim(), "KFX Output.zip");
    // Gone with the run, and the real folder untouched.
    assert!(s.tmp_entries().is_empty());
    assert_eq!(entries(&real.join("plugins")), ["KFX Output.zip"]);
}

#[test]
fn untold_the_folder_copied_is_the_one_calibre_itself_would_use() {
    use xtask::calibre_config::{Host, real_calibre_config};
    let home = PathBuf::from("/home/someone");
    let none = |_: &str| None;
    let mac = Host {
        env: &none,
        os: "macos",
        home: home.clone(),
    };
    assert_eq!(real_calibre_config(&mac), home.join("Library/Preferences/calibre"));
    let set = |k: &str| (k == "CALIBRE_CONFIG_DIRECTORY").then(|| s(&abs("x/cal")));
    let linux = Host {
        env: &set,
        os: "linux",
        home,
    };
    assert_eq!(real_calibre_config(&linux), abs("x/cal"));
    assert_eq!(
        xtask::capture::run::default_calibre_config(),
        xtask::calibre_config::this_host_real_config()
    );
}

#[test]
fn run_refuses_a_library_the_tool_did_not_make_and_leaves_it_exactly_as_it_was() {
    let s = RunScene::new(false);
    std::fs::create_dir_all(&s.library).unwrap();
    std::fs::write(s.library.join("my-real-book.epub"), "mine").unwrap();
    let launched = Arc::new(Mutex::new(false));
    let mut o = s.options(Err("unused".into()));
    let l2 = Arc::clone(&launched);
    o.launch = Box::new(move || {
        *l2.lock().unwrap() = true;
        Err("launched".into())
    });
    let e = failed(run_capture(o));
    assert!(e.expected);
    assert!(e.message.contains("already exists"));
    assert!(!*launched.lock().unwrap());
    assert_eq!(entries(&s.library), ["my-real-book.epub"]);
    assert!(s.tmp_entries().is_empty());
}

// ---------------------------------------------------------------- agreement

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap()
}

#[test]
fn the_frame_page_pads_the_window_by_exactly_what_pass_two_is_sized_for() {
    let html = read("tools/capture/frame.html");
    let rule = html.split(".pad").nth(1).unwrap();
    let padding = rule
        .split("padding:")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let px: Vec<u32> = padding
        .split_whitespace()
        .map(|p| p.trim_end_matches("px").parse().unwrap())
        .collect();
    assert_eq!(px.len(), 3);
    assert_eq!(
        (px[0], px[1], px[2], px[1]),
        (FRAME_PAD.top, FRAME_PAD.right, FRAME_PAD.bottom, FRAME_PAD.left)
    );
    let drop = SHOTS.iter().find(|x| x.name == "drop").unwrap();
    assert_eq!(framed_size(drop), (1280 + 56 + 56, 800 + 44 + 68));
}

#[test]
fn the_step_runner_has_a_step_for_every_window_shot_and_nothing_else() {
    let steps = read("tools/capture/steps.js");
    let start = steps.find("const SHOTS = {").unwrap();
    let block = &steps[start..];
    let block = &block[..block.find("\n};").unwrap()];
    let mut names: Vec<&str> = block
        .lines()
        .filter_map(|l| l.strip_prefix("  async "))
        .filter_map(|l| l.split_once("()").map(|(n, _)| n))
        .collect();
    names.sort();
    let mut want: Vec<&str> = SHOTS
        .iter()
        .filter(|x| x.kind == Kind::Window)
        .map(|x| x.name)
        .collect();
    want.sort();
    assert_eq!(names, want);
}

#[test]
fn the_window_still_draws_the_library_slot_the_step_runner_hides() {
    let convert = read("desktop/ui/convert.js");
    let a = convert.find("function buildLibrarySlot").unwrap();
    let b = convert.find("async function probeLibrary").unwrap();
    assert_eq!(convert[a..b].matches("class: 'well-ask well-library'").count(), 2);
    let steps = read("tools/capture/steps.js");
    assert!(steps.contains(".well-library"));
    assert!(steps.contains(".well-library { display: none !important; }"));
}

#[test]
fn the_window_still_draws_the_version_stamp_the_step_runner_hides() {
    assert!(read("desktop/ui/frame.js").contains("class: 'rev-stamp'"));
    assert!(read("tools/capture/steps.js").contains(".rev-stamp"));
}
