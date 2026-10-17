//! The capture tool without Chrome (was tests/capture.test.ts): the shot
//! list, the page server driven with plain requests, the window backend
//! (the orders it gives sp-app's capture program, the picture it reads
//! back, a program that fails, hangs or is interrupted), and whole runs
//! with a fake browser and a fake window, down to what each one leaves
//! behind. The Chrome driver is tested against a fake Chrome in
//! capture_chrome.rs. Real Chrome runs only when someone runs
//! `cargo xtask capture-screens`; the real window program runs here once,
//! for the drop well, where cargo has built it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::Duration;

use xtask::capture::backend::{
    Browser, CaptureOptions, CommandLine, EguiWindow, Exchange, HEAD_LINES, PassOne, Terminal,
    WindowBackend, terminal_query, transcript_of, window_args,
};
use xtask::capture::http::{Request, Response};
#[cfg(unix)]
use xtask::capture::picture::encode_rgba;
use xtask::capture::picture::{decode, size};
use xtask::capture::run::{MARKER, RunError, RunOptions, claim_library, engine_env, run_capture};
use xtask::capture::server::{Handler, HandlerOptions, Records, snapshot};
use xtask::capture::shots::{
    FRAME_PAD, Kind, SHOTS, Shot, Theme, Written, all_outputs, framed_size, write_if_changed,
};
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
        Scratch(dir)
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

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap()
}

fn shot(name: &str) -> &'static Shot {
    SHOTS.iter().find(|x| x.name == name).unwrap()
}

fn demo() -> PathBuf {
    repo_root().join("tests/fixtures/field-station.pdf")
}

// ---------------------------------------------------------------- shots

#[test]
fn shots_are_the_suites_four_in_its_order_every_one_light_and_dark() {
    // SUITE-STANDARDS.md 7.1: (1) the hero, the main view; (2) the
    // signature feature, the before and after; (3) open, with the result
    // beside it; (4) the file you own, the command line.
    let names: Vec<&str> = SHOTS.iter().map(|s| s.name).collect();
    assert_eq!(names, ["read", "compare", "drop", "result", "cli"]);
    let kinds: Vec<Kind> = SHOTS.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            Kind::Window,
            Kind::Site,
            Kind::Window,
            Kind::Window,
            Kind::Terminal
        ]
    );
    for s in SHOTS {
        let words: Vec<&str> = s.themes.iter().map(|t| t.word()).collect();
        assert_eq!(words, ["light", "dark"], "{}", s.name);
        // The suite's one frame size, whatever is in it.
        assert_eq!((s.width, s.height), (1280, 800), "{}", s.name);
    }
}

#[test]
fn window_shots_are_the_windows_own_size() {
    // The size the window opens at (sp-app's viewport, which carried
    // tauri.conf.json's over).
    let app = read("rust/crates/sp-app/src/app/mod.rs");
    for s in SHOTS.iter().filter(|s| s.kind == Kind::Window) {
        let want = format!(".with_inner_size([{}.0, {}.0])", s.width, s.height);
        assert!(app.contains(&want), "sp-app no longer opens at {want}");
    }
}

#[test]
fn every_picture_goes_to_the_readme_and_to_the_site_alike() {
    let all = all_outputs(SHOTS);
    let readme: Vec<&str> = all
        .iter()
        .filter_map(|p| p.strip_prefix("assets/screens/"))
        .collect();
    let site: Vec<&str> = all
        .iter()
        .filter_map(|p| p.strip_prefix("site/img/"))
        .collect();
    assert_eq!(readme.len(), 10);
    assert_eq!(readme, site);
    assert_eq!(all.len(), 20);
}

#[test]
fn the_terminal_shows_the_command_and_the_files_it_wrote_read_off_the_library() {
    let dir = Scratch::new("transcript");
    let book = dir.path().join("Method Reader").join("field-station");
    std::fs::create_dir_all(&book).unwrap();
    let fountain: String = (1..=40).map(|i| format!("line {i}\n")).collect();
    std::fs::write(book.join("field-station.fountain"), &fountain).unwrap();
    std::fs::write(book.join("field-station.epub"), "book").unwrap();
    std::fs::write(book.join(".hidden"), "").unwrap();
    let t = transcript_of("field-station.pdf", "Field Station\n  wrote it\n", &book).unwrap();
    assert_eq!(t[0].command, "method-reader field-station.pdf --library");
    assert_eq!(t[0].output, "Field Station\n  wrote it");
    assert_eq!(t[1].command, format!("ls -1 \"{}\"", book.display()));
    assert_eq!(t[1].output, "field-station.epub\nfield-station.fountain");
    assert!(
        t[2].command
            .starts_with(&format!("head -n {HEAD_LINES} \""))
    );
    let want: Vec<String> = (1..=HEAD_LINES).map(|i| format!("line {i}")).collect();
    assert_eq!(t[2].output, want.join("\n"));
    // Into the page's query whole: JSON, nothing left unescaped.
    let q = terminal_query(&t);
    assert!(
        q.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~%".contains(&b))
    );
    assert!(q.starts_with("%5B%7B%22c%22%3A%22method-reader"));
}

#[test]
fn a_file_is_written_only_when_its_bytes_differ() {
    let dir = Scratch::new("write");
    let path = dir.path().join("x.png");
    assert_eq!(
        write_if_changed(&path, &[1, 2, 3]).unwrap(),
        Written::Written
    );
    assert_eq!(
        write_if_changed(&path, &[1, 2, 3]).unwrap(),
        Written::Unchanged
    );
    assert_eq!(
        write_if_changed(&path, &[1, 2, 4]).unwrap(),
        Written::Written
    );
    assert_eq!(std::fs::read(&path).unwrap(), [1, 2, 4]);
}

#[test]
fn the_capture_command_and_chrome_driver_are_wired() {
    assert!(xtask::COMMANDS.iter().any(|c| c.name == "capture-screens"));
    assert_eq!(
        xtask::capture::cdp::CHROME,
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    );
    assert_eq!(
        xtask::capture::release_dir().join("examples/capture"),
        xtask::paths::workspace_root()
            .join(std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()))
            .join("release/examples/capture")
    );
}

// ---------------------------------------------------------------- the server

const HOST: &str = "127.0.0.1:4321";

struct ServerScene {
    _dir: Scratch,
    handler: Handler,
    records: Records,
}

impl ServerScene {
    /// A throwaway repository with the files a careless static server would
    /// hand out: the gitignored REAL screenplays, git's own files, the
    /// package manifest, a sibling folder whose name starts with the
    /// repo's, and the old HTML window.
    fn new() -> ServerScene {
        let dir = Scratch::new("server");
        let repo = dir.path().join("Screepub");
        let put = |rel: &str, body: &str| {
            let p = repo.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        };
        put(
            "desktop/ui/index.html",
            "<!doctype html><title>old window</title>",
        );
        put("site/index.html", "<!doctype html><title>site</title>");
        put(
            "tools/capture/frame.html",
            "<!doctype html><title>frame</title>",
        );
        put("fixtures/real-script.pdf", "A REAL SCREENPLAY");
        put(".git/HEAD", "ref: refs/heads/main");
        put("package.json", "{}");
        std::fs::create_dir_all(dir.path().join("Screepub-old")).unwrap();
        std::fs::write(dir.path().join("Screepub-old/x"), "not served").unwrap();
        let pass_one = dir.path().join("pass-one");
        std::fs::create_dir_all(&pass_one).unwrap();
        std::fs::write(pass_one.join("drop-light.png"), "PNG").unwrap();
        std::fs::write(pass_one.join("drop-light.rgba"), "RAW").unwrap();
        let records = Records::default();
        let handler = Handler::new(HandlerOptions {
            repo_dir: repo,
            pass_one,
            host: HOST.into(),
            records: records.clone(),
        });
        ServerScene {
            _dir: dir,
            handler,
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
}

#[test]
fn server_serves_the_site_and_the_capture_pages() {
    let s = ServerScene::new();
    for path in ["/site/index.html", "/tools/capture/frame.html"] {
        let r = s.get(path, &[]);
        assert_eq!(r.status, 200, "{path}");
        assert!(r.content_type.contains("text/html"), "{path}");
    }
}

#[test]
fn server_serves_nothing_outside_the_site_and_the_capture_pages() {
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
        // The old HTML window is not served: the window is drawn by sp-app.
        "/desktop/ui/index.html",
        "/capture/window.html?shot=drop",
        // A backslash is a separator on Windows.
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
fn server_has_no_door_for_engine_calls_any_more() {
    let s = ServerScene::new();
    let mut r = Request::get(HOST, "/engine").with_header("content-type", "application/json");
    r.method = "POST".into();
    r.body = br#"{"args":["--version","--json"]}"#.to_vec();
    assert_eq!(s.handler.handle(&r).unwrap().status, 404);
}

#[test]
fn server_refuses_any_other_host_so_a_rebound_name_cannot_reach_it() {
    let s = ServerScene::new();
    assert_eq!(
        s.get("/site/index.html", &[("host", "evil.example:4321")])
            .status,
        403
    );
    assert_eq!(
        s.get("/site/index.html", &[("host", "localhost:4321")])
            .status,
        403
    );
}

#[test]
fn server_serves_pass_one_pictures_by_bare_name_only() {
    let s = ServerScene::new();
    assert_eq!(s.get("/pass-one/drop-light.png", &[]).status, 200);
    // The window's raw pixels are never served, only the PNG made of them.
    assert_eq!(s.get("/pass-one/drop-light.rgba", &[]).status, 404);
    assert_eq!(
        s.get("/pass-one/..%2f..%2fScreepub%2fpackage.json", &[])
            .status,
        404
    );
    assert_eq!(s.get("/pass-one/missing-light.png", &[]).status, 404);
    assert!(snapshot(&s.records.not_found).contains(&"/pass-one/missing-light.png".to_string()));
}

// ---------------------------------------------------------------- the window

#[test]
fn the_window_program_is_told_one_shot_in_one_theme_at_the_shots_size() {
    let args = window_args(
        shot("result"),
        Theme::Dark,
        &abs("repo/tests/fixtures/field-station.pdf"),
        &abs("tmp/pass/result-dark.rgba"),
    );
    assert_eq!(
        args,
        [
            "--shot",
            "result",
            "--theme",
            "dark",
            "--width",
            "1280",
            "--height",
            "772",
            "--demo",
            &s(&abs("repo/tests/fixtures/field-station.pdf")),
            "--out",
            &s(&abs("tmp/pass/result-dark.rgba")),
        ]
    );
}

#[test]
fn every_window_shot_is_one_the_window_program_knows_and_no_other() {
    // sp-app/src/capture.rs names its shots in Shot::word.
    let capture = read("rust/crates/sp-app/src/capture.rs");
    let start = capture.find("pub fn word(self)").unwrap();
    let block = &capture[start..start + capture[start..].find("\n    }").unwrap()];
    let mut known: Vec<&str> = block
        .split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, w)| w)
        .collect();
    known.sort();
    let mut want: Vec<&str> = SHOTS
        .iter()
        .filter(|x| x.kind == Kind::Window)
        .map(|x| x.name)
        .collect();
    want.sort();
    assert_eq!(known, want);
}

fn no_failure() -> Option<String> {
    None
}

/// A stand-in window program: a shell script, with the picture's orders
/// as its arguments ("$@").
#[cfg(unix)]
fn sh_window(script: &str) -> EguiWindow {
    EguiWindow {
        program: vec![
            "/bin/sh".into(),
            "-c".into(),
            script.into(),
            "window".into(),
        ],
        timeout: Duration::from_secs(20),
    }
}

/// What a script finds after --out in its orders.
#[cfg(unix)]
const OUT: &str =
    "out=; prev=; for a in \"$@\"; do [ \"$prev\" = --out ] && out=\"$a\"; prev=\"$a\"; done; ";

#[cfg(unix)]
fn draw(
    w: &mut EguiWindow,
    dir: &Path,
    name: &str,
    failure: &dyn Fn() -> Option<String>,
) -> Result<Vec<u8>, String> {
    w.pass_one(PassOne {
        shot: shot(name),
        theme: Theme::Light,
        demo_pdf: &demo(),
        env: &[],
        scratch: dir,
        failure,
    })
}

#[cfg(unix)]
#[test]
fn the_window_programs_pixels_become_the_same_png_bytes_every_time() {
    let dir = Scratch::new("window-png");
    // 2560 by 1544 pixels of one grey (the window's content under the
    // title bar frame.html draws), written as raw RGBA. tr runs in
    // the C locale: under a UTF-8 one (a CI runner's) it writes byte 128
    // as the two-byte character U+0080 and the picture comes out double.
    let script = format!(
        "{OUT}head -c {} /dev/zero | LC_ALL=C tr '\\0' '\\200' > \"$out\"",
        2560 * 1544 * 4
    );
    let mut w = sh_window(&script);
    let a = draw(&mut w, dir.path(), "drop", &no_failure).unwrap();
    let b = draw(&mut w, dir.path(), "drop", &no_failure).unwrap();
    assert_eq!(a, b, "same pixels, same bytes");
    assert_eq!(size(&a).unwrap(), (2560, 1544));
    let rgba = decode(&a).unwrap();
    assert!(rgba.pixels.iter().all(|&p| p == 128));
    assert_eq!(a, encode_rgba(2560, 1544, &rgba.pixels).unwrap());
    // The raw file does not outlive its picture.
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn a_picture_of_the_wrong_size_fails_rather_than_being_framed() {
    let dir = Scratch::new("window-size");
    let mut w = sh_window(&format!(
        "{OUT}head -c {} /dev/zero > \"$out\"",
        1280 * 800 * 4
    ));
    let e = draw(&mut w, dir.path(), "drop", &no_failure).unwrap_err();
    assert!(e.contains("2560 by 1544"), "{e}");
}

#[cfg(unix)]
#[test]
fn a_window_program_that_fails_fails_the_picture_saying_what_it_said() {
    let dir = Scratch::new("window-fail");
    let mut w =
        sh_window("echo 'capture: the window made engine calls the capture refuses' >&2; exit 1");
    let e = draw(&mut w, dir.path(), "result", &no_failure).unwrap_err();
    assert!(e.contains("the result window (light) exited with 1"), "{e}");
    assert!(e.contains("engine calls the capture refuses"), "{e}");
    let mut silent = sh_window("exit 0");
    let e = draw(&mut silent, dir.path(), "drop", &no_failure).unwrap_err();
    assert!(e.contains("wrote no picture"), "{e}");
    let mut missing = EguiWindow::new(&dir.path().join("no-such-program"));
    let e = draw(&mut missing, dir.path(), "drop", &no_failure).unwrap_err();
    assert!(e.contains("could not start"), "{e}");
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
fn rand6() -> u32 {
    let t = xtask::capture::random_token();
    u32::from_str_radix(&t[..5], 16).unwrap() % 1_000_000
}

/// A window program that runs until it is stopped, named by a duration no
/// other run of this suite uses, so nothing else is found or killed.
#[cfg(unix)]
fn hung_window() -> (EguiWindow, String) {
    let seconds = format!("29.{}{:06}", std::process::id(), rand6());
    let pattern = format!("sleep {}", seconds.replace('.', "\\."));
    (sh_window(&format!("exec sleep {seconds}")), pattern)
}

#[cfg(unix)]
#[test]
fn a_window_program_that_hangs_is_stopped_at_the_time_limit() {
    let dir = Scratch::new("window-hang");
    let (mut w, pattern) = hung_window();
    w.timeout = Duration::from_millis(400);
    let e = draw(&mut w, dir.path(), "read", &no_failure).unwrap_err();
    assert!(e.contains("timed out"), "{e}");
    assert!(pgrep(&pattern).is_empty(), "left running");
}

#[cfg(unix)]
#[test]
fn an_interrupt_stops_the_window_program_at_once() {
    let dir = Scratch::new("window-interrupt");
    let (mut w, pattern) = hung_window();
    let start = std::time::Instant::now();
    let asked = Mutex::new(0);
    let failure = || {
        let mut n = asked.lock().unwrap();
        *n += 1;
        (*n > 5).then(|| "interrupted by SIGINT".to_string())
    };
    let e = draw(&mut w, dir.path(), "result", &failure).unwrap_err();
    assert!(e.contains("interrupted by SIGINT"), "{e}");
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(pgrep(&pattern).is_empty(), "left running");
}

/// The real window program, as `cargo test` built it alongside sp-app's
/// tests (target/debug/examples/capture), if it did.
fn built_window_program() -> Option<PathBuf> {
    let ws = xtask::paths::workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .filter(|t| !t.is_empty())
        .map_or_else(|| ws.join("target"), |t| ws.join(t));
    let exe = if cfg!(windows) {
        "capture.exe"
    } else {
        "capture"
    };
    let p = target.join("debug/examples").join(exe);
    p.is_file().then_some(p)
}

#[test]
fn the_real_window_program_refuses_to_run_without_a_scratch_library() {
    let Some(program) = built_window_program() else {
        eprintln!(
            "skipped: sp-app's capture example is not built (cargo test -p sp-app builds it)"
        );
        return;
    };
    let dir = Scratch::new("window-real-env");
    let out = dir.path().join("drop-light.rgba");
    let ran = std::process::Command::new(&program)
        .args(window_args(shot("drop"), Theme::Light, &demo(), &out))
        .env_remove("METHOD_READER_LIBRARY")
        .env_remove("METHOD_READER_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(ran.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&ran.stderr).contains("scratch folder"));
    assert!(!out.exists());
}

#[test]
fn the_real_window_draws_the_drop_well_at_twice_its_size_where_wgpu_works() {
    let Some(program) = built_window_program() else {
        eprintln!(
            "skipped: sp-app's capture example is not built (cargo test -p sp-app builds it)"
        );
        return;
    };
    let dir = Scratch::new("window-real");
    let library = dir.path().join("Screepub");
    std::fs::create_dir_all(library.join(".no-volumes")).unwrap();
    let calibre = dir.path().join("calibre-copy");
    std::fs::create_dir_all(&calibre).unwrap();
    let env = engine_env(&library, &calibre);
    let mut w = EguiWindow::new(&program);
    let png = match w.pass_one(PassOne {
        shot: shot("drop"),
        theme: Theme::Dark,
        demo_pdf: &demo(),
        env: &env,
        scratch: dir.path(),
        failure: &no_failure,
    }) {
        Ok(p) => p,
        Err(e) if e.contains("wgpu") => {
            eprintln!("skipped: wgpu could not draw here: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    assert_eq!(size(&png).unwrap(), (2560, 1544));
    let rgba = decode(&png).unwrap();
    assert!(
        rgba.pixels.chunks_exact(4).any(|p| p != &rgba.pixels[..4]),
        "a blank picture"
    );
    // The drop well converts nothing: the library holds only what the run
    // made for it, and the settings the window read live under it.
    let mut left: Vec<String> = std::fs::read_dir(&library)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".capture-app-settings")
        .collect();
    left.sort();
    assert_eq!(left, [".no-volumes"]);
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

type DrawFn = Box<dyn FnMut(&PassOne<'_>) -> Result<Vec<u8>, String>>;

/// A window that draws whatever the test says.
struct FakeWindow(DrawFn);

impl WindowBackend for FakeWindow {
    fn pass_one(&mut self, p: PassOne<'_>) -> Result<Vec<u8>, String> {
        (self.0)(&p)
    }
}

/// A command line that answers what the test says.
type TranscriptFn = Box<dyn FnMut(&Terminal<'_>) -> Result<Vec<Exchange>, String>>;
struct FakeCli(TranscriptFn);

impl CommandLine for FakeCli {
    fn transcript(&mut self, t: Terminal<'_>) -> Result<Vec<Exchange>, String> {
        (self.0)(&t)
    }
}

fn some_cli() -> Box<FakeCli> {
    Box::new(FakeCli(Box::new(|_| {
        Ok(vec![Exchange {
            command: "method-reader field-station.pdf --library".into(),
            output: "Field Station".into(),
        }])
    })))
}

fn png_window() -> Box<FakeWindow> {
    Box::new(FakeWindow(Box::new(|_| Ok(PNG.to_vec()))))
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
            window: png_window(),
            cli: some_cli(),
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

fn drop_only(o: &mut RunOptions) {
    o.shots = SHOTS.iter().filter(|s| s.name == "drop").copied().collect();
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
    let b = fake(Box::new(|_| Ok(PNG.to_vec())));
    let events = Arc::clone(&b.events);
    let mut o = s.options(Ok(b));
    o.window = Box::new(FakeWindow(Box::new(|p| {
        if p.shot.name == "read" {
            Err("capture failed at the read shot".into())
        } else {
            Ok(PNG.to_vec())
        }
    })));
    let e = failed(run_capture(o));
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
    let framed: Arc<Mutex<Vec<String>>> = Arc::default();
    let (seen2, lib) = (Arc::clone(&seen), s.library.clone());
    let framed2 = Arc::clone(&framed);
    let b = fake(Box::new(move |url| {
        framed2.lock().unwrap().push(url.to_string());
        Ok(PNG.to_vec())
    }));
    let mut o = s.options(Ok(b));
    o.window = Box::new(FakeWindow(Box::new(move |p| {
        seen2.lock().unwrap().push(entries(&lib));
        // What a conversion leaves behind, which the next shot must not see.
        std::fs::create_dir_all(lib.join("field-station")).unwrap();
        std::fs::write(lib.join("field-station/field-station.epub"), "book").unwrap();
        assert_eq!(p.demo_pdf, demo());
        Ok(PNG.to_vec())
    })));
    let (seen3, lib3) = (Arc::clone(&seen), s.library.clone());
    o.cli = Box::new(FakeCli(Box::new(move |t| {
        seen3.lock().unwrap().push(entries(&lib3));
        assert_eq!(t.demo_pdf, demo());
        assert_eq!(t.library, lib3);
        Ok(vec![Exchange {
            command: "method-reader field-station.pdf --library".into(),
            output: "Field Station".into(),
        }])
    })));
    run_capture(o).unwrap();
    let seen = seen.lock().unwrap();
    // Six window pictures, and the command line once for both its themes.
    assert_eq!(seen.len(), 7);
    for e in seen.iter() {
        let mut want = vec![".no-volumes".to_string(), MARKER.to_string()];
        want.sort();
        assert_eq!(e, &want);
    }
    // Chrome framed each of the window's six pictures, drew the before and
    // after twice, and drew the terminal then framed it, in each theme.
    let framed = framed.lock().unwrap();
    assert_eq!(framed.len(), 12);
    assert!(
        framed[0].contains("/tools/capture/frame.html?img=/pass-one/read-light.png&w=1280&h=800")
    );
    assert!(framed[2].ends_with("/tools/capture/compare.html"));
    assert!(framed[3].ends_with("/tools/capture/compare.html"));
    assert!(
        framed[4].contains("/tools/capture/frame.html?img=/pass-one/drop-light.png&w=1280&h=800")
    );
    assert!(framed[8].contains("/tools/capture/terminal.html?t=%5B%7B%22c%22"));
    assert!(framed[9].contains(
        "/tools/capture/frame.html?img=/pass-one/cli-light.png&w=1280&h=800&title=Terminal"
    ));
    assert!(framed[10].contains("/tools/capture/terminal.html?t="));
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

#[test]
fn run_gives_the_window_the_scratch_environment_and_never_the_real_settings() {
    let s = RunScene::new(false);
    let real = s.dir.path().join("real-calibre");
    std::fs::create_dir_all(real.join("plugins")).unwrap();
    std::fs::write(real.join("plugins/KFX Output.zip"), "plugin").unwrap();
    let got: Arc<Mutex<Vec<(String, PathBuf)>>> = Arc::default();
    let listed: Arc<Mutex<Vec<String>>> = Arc::default();
    let (g2, l2) = (Arc::clone(&got), Arc::clone(&listed));
    let mut o = s.options(Ok(fake(Box::new(|_| Ok(PNG.to_vec())))));
    drop_only(&mut o);
    o.calibre_config = Some(real.clone());
    o.window = Box::new(FakeWindow(Box::new(move |p| {
        for (k, v) in p.env {
            g2.lock().unwrap().push(((*k).to_string(), v.clone()));
            if *k == "CALIBRE_CONFIG_DIRECTORY" {
                *l2.lock().unwrap() = entries(&v.join("plugins"));
            }
        }
        Ok(PNG.to_vec())
    })));
    run_capture(o).unwrap();
    let got = got.lock().unwrap().clone();
    let var = |k: &str| got.iter().find(|(n, _)| n == k).unwrap().1.clone();
    assert_eq!(var("METHOD_READER_LIBRARY"), s.library);
    // Under the run's own scratch library, cleared before every shot.
    assert_eq!(
        var("METHOD_READER_CONFIG_DIR"),
        s.library.join(".capture-app-settings")
    );
    assert_eq!(
        var("METHOD_READER_VOLUME_ROOTS"),
        s.library.join(".no-volumes")
    );
    assert_eq!(
        var("METHOD_READER_REMARKABLE_ENDPOINT"),
        PathBuf::from("http://127.0.0.1:9")
    );
    // A scratch COPY of Calibre's settings, never the real folder.
    let calibre = var("CALIBRE_CONFIG_DIRECTORY");
    assert_ne!(calibre, real);
    assert_ne!(calibre, xtask::capture::run::default_calibre_config());
    assert!(calibre.starts_with(&s.tmp));
    assert_eq!(*listed.lock().unwrap(), ["KFX Output.zip"]);
    // Gone with the run, and the real folder untouched.
    assert!(s.tmp_entries().is_empty());
    assert_eq!(entries(&real.join("plugins")), ["KFX Output.zip"]);
}

#[cfg(unix)]
#[test]
fn run_a_window_program_still_running_when_the_run_is_interrupted_is_stopped() {
    let s = RunScene::new(false);
    let (mut w, pattern) = hung_window();
    w.timeout = Duration::from_secs(30);
    let signal = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut o = s.options(Ok(fake(Box::new(|_| Ok(PNG.to_vec())))));
    drop_only(&mut o);
    o.window = Box::new(w);
    o.interrupt = Some(Arc::clone(&signal));
    let p2 = pattern.clone();
    std::thread::spawn(move || {
        while pgrep(&p2).is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
        signal.store(2, std::sync::atomic::Ordering::SeqCst);
    });
    let e = failed(run_capture(o));
    assert_eq!(e.interrupted, Some(2));
    assert!(e.message.contains("interrupted by SIGINT"), "{}", e.message);
    assert!(pgrep(&pattern).is_empty(), "left running");
    assert!(!s.out.exists());
    assert!(!s.library.exists());
    assert!(s.tmp_entries().is_empty());
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
    assert!(e.message.contains(&format!(
        "capture pid {pid} is using {}",
        s.library.display()
    )));
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
    assert_eq!(
        real_calibre_config(&mac),
        home.join("Library/Preferences/calibre")
    );
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
        (
            FRAME_PAD.top,
            FRAME_PAD.right,
            FRAME_PAD.bottom,
            FRAME_PAD.left
        )
    );
    let drop = shot("drop");
    assert_eq!(framed_size(drop), (1280 + 56 + 56, 800 + 44 + 68));
}

#[test]
fn the_window_still_has_the_stamp_and_the_library_slot_the_picture_mode_hides() {
    // A stamp or a library line drawn outside picture_mode's reach would be
    // in every picture; sp-app's tests check the hiding, this that the two
    // places it hides still exist where the pictures are drawn.
    let frame = read("rust/crates/sp-app/src/app/frame.rs");
    let at = frame.find("if app.picture_mode {").unwrap();
    assert!(frame[at..at + 400].contains("stamp_space"));
    let convert = read("rust/crates/sp-app/src/app/convert.rs");
    let hide = convert.find("if app.picture_mode {").unwrap();
    let line = convert.find("m::library_line(&library)").unwrap();
    assert!(
        hide < line,
        "the library line is drawn before picture mode hides it"
    );
}
