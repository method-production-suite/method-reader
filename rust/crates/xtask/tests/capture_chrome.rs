//! The capture's Chrome driver, against a fake Chrome (was
//! tests/capture-chrome.test.ts): the capture-fake-chrome binary, which
//! writes DevToolsActivePort, and a fake DevTools server here that
//! answers, or misbehaves, on cue. The real driver runs unchanged, so its
//! failure paths (a Chrome that dies at start, one that lists no page, a
//! connection that drops) are exercised without a real Chrome or
//! /Users/Shared. A few seconds.
//!
//! The last test needs the real Chrome and says so and skips without it.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};
use tungstenite::Message;
use xtask::capture::backend::{Browser, CaptureOptions, DesktopUi};
use xtask::capture::cdp::{self, CHROME, PROFILE_PREFIX, launch};
use xtask::capture::http::{Response, Server};
use xtask::capture::run::{RunOptions, run_capture};
use xtask::capture::shots::{SHOTS, Theme};
use xtask::capture::window_argv as argv;

const PNG: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10];

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "screepub-chrome-driver-test-{tag}-{}",
            xtask::capture::random_token()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Navigate = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Default)]
struct Fake {
    /// /json/list answers with the page (default) or, true, with none.
    unlisted: bool,
    /// The DevTools method on which the fake drops the connection.
    drop_on: Option<&'static str>,
    /// What the page says when asked whether it is ready (default ready).
    working: bool,
    /// What "loading" a page does; a real Chrome fetches it.
    navigate: Option<Navigate>,
}

/// A fake Chrome and its DevTools server.
struct FakeChrome {
    _dir: Scratch,
    dir: PathBuf,
    chrome: PathBuf,
    log: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    port: u16,
}

impl Drop for FakeChrome {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

impl FakeChrome {
    fn new(o: Fake, die: bool) -> FakeChrome {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (log2, stop2) = (Arc::clone(&log), Arc::clone(&stop));
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = conn else { continue };
                let (o, log) = (o.clone(), Arc::clone(&log2));
                std::thread::spawn(move || serve(stream, port, &o, &log));
            }
        });
        let scratch = Scratch::new("fake-chrome");
        let dir = scratch.0.clone();
        let name = if cfg!(windows) {
            "chrome.exe"
        } else {
            "chrome"
        };
        let chrome = dir.join(name);
        std::fs::copy(env!("CARGO_BIN_EXE_capture-fake-chrome"), &chrome).unwrap();
        std::fs::write(dir.join("port"), port.to_string()).unwrap();
        if die {
            std::fs::write(dir.join("die"), "").unwrap();
        }
        FakeChrome {
            _dir: scratch,
            dir,
            chrome,
            log,
            stop,
            port,
        }
    }

    fn logged(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    /// The run removed the profile it launched this fake with: that exact
    /// folder, which the fake wrote down, not "no new profile anywhere".
    fn expect_profile_gone(&self) {
        let profile = std::fs::read_to_string(self.dir.join("profile")).unwrap();
        let profile = Path::new(profile.trim());
        assert!(
            profile.starts_with(std::env::temp_dir()),
            "{}",
            profile.display()
        );
        let name = profile.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with(PROFILE_PREFIX), "{name}");
        assert!(!profile.exists(), "{} is still there", profile.display());
    }
}

/// Peeks the request line, so a websocket upgrade can be handed to
/// tungstenite with its request still unread.
fn request_line(stream: &TcpStream) -> Option<String> {
    let mut buf = [0u8; 2048];
    for _ in 0..200 {
        let n = stream.peek(&mut buf).ok()?;
        let seen = String::from_utf8_lossy(&buf[..n]).into_owned();
        if seen.contains("\r\n\r\n") {
            return seen.lines().next().map(str::to_string);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn answer(mut stream: TcpStream, status: u16, body: &str) {
    let mut buf = [0u8; 4096];
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.read(&mut buf);
    let _ = xtask::capture::http::write_response(
        &mut stream,
        &Response::new(status, "application/json", body.as_bytes().to_vec()),
    );
}

fn serve(stream: TcpStream, port: u16, o: &Fake, log: &Mutex<Vec<String>>) {
    let Some(line) = request_line(&stream) else {
        return;
    };
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let page = json!({ "type": "page", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/1") });
    if path == "/json/list" {
        log.lock().unwrap().push("list".into());
        let list = if o.unlisted { json!([]) } else { json!([page]) };
        return answer(stream, 200, &list.to_string());
    }
    if path.starts_with("/json/new") {
        log.lock().unwrap().push(format!("new {method}"));
        return if method == "PUT" {
            answer(stream, 200, &page.to_string())
        } else {
            answer(stream, 405, "")
        };
    }
    if path != "/devtools/page/1" {
        return answer(stream, 404, "");
    }
    let Ok(mut ws) = tungstenite::accept(stream) else {
        return;
    };
    while let Ok(msg) = ws.read() {
        let Message::Text(t) = msg else { continue };
        let m: Value = serde_json::from_str(t.as_str()).unwrap();
        let method = m["method"].as_str().unwrap_or("").to_string();
        log.lock().unwrap().push(method.clone());
        if o.drop_on == Some(method.as_str()) {
            // Gone, the way a crashed Chrome goes: no close frame.
            let _ = ws.get_mut().shutdown(std::net::Shutdown::Both);
            return;
        }
        let result = match method.as_str() {
            "Browser.getVersion" => json!({ "product": "Chrome/0.0.0.0-fake" }),
            "Page.navigate" => {
                if let Some(nav) = &o.navigate {
                    nav(m["params"]["url"].as_str().unwrap_or(""));
                }
                json!({ "frameId": "fake" })
            }
            "Runtime.evaluate" => {
                if m["params"]["expression"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("JSON.stringify")
                {
                    let state = if o.working { "working" } else { "ready" };
                    json!({ "result": { "type": "string", "value": json!([state]).to_string() } })
                } else {
                    json!({ "result": { "type": "boolean", "value": true } })
                }
            }
            "Page.captureScreenshot" => {
                json!({ "data": base64::engine::general_purpose::STANDARD.encode(PNG) })
            }
            _ => json!({}),
        };
        let reply = json!({ "id": m["id"], "result": result }).to_string();
        if ws.send(Message::text(reply)).is_err() {
            return;
        }
    }
}

fn shot() -> CaptureOptions<'static> {
    let mut o = CaptureOptions::page("http://127.0.0.1:1/page".into(), 10, 10, Theme::Light);
    o.timeout = cdp::CAPTURE_TIMEOUT;
    o
}

#[test]
fn a_chrome_that_lists_no_page_gets_one_opened_and_a_ready_page_is_photographed() {
    let fake = FakeChrome::new(
        Fake {
            unlisted: true,
            ..Fake::default()
        },
        false,
    );
    let mut browser = launch(&fake.chrome).unwrap();
    assert_eq!(browser.version(), "Chrome/0.0.0.0-fake");
    assert!(fake.logged().contains(&"new PUT".to_string()));
    assert_eq!(browser.capture(&shot()).unwrap(), PNG);
    browser.close().unwrap();
    fake.expect_profile_gone();
}

#[test]
fn a_connection_dropped_on_navigate_fails_the_capture_at_once_and_close_still_cleans_up() {
    let fake = FakeChrome::new(
        Fake {
            drop_on: Some("Page.navigate"),
            ..Fake::default()
        },
        false,
    );
    let mut browser = launch(&fake.chrome).unwrap();
    let start = Instant::now();
    let err = browser.capture(&shot()).unwrap_err();
    // Well inside the 15 s a single request may take.
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(err.contains("Page.navigate"), "{err}");
    assert!(err.contains("connection closed"), "{err}");
    browser.close().unwrap();
    fake.expect_profile_gone();
}

#[test]
fn a_chrome_that_exits_at_start_fails_the_launch_at_once_naming_it_and_leaves_no_profile() {
    let fake = FakeChrome::new(Fake::default(), true);
    let start = Instant::now();
    let err = launch(&fake.chrome).err().expect("the launch should fail");
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(
        err.contains("exited (3) before it opened its debugging port"),
        "{err}"
    );
    fake.expect_profile_gone();
}

#[test]
fn a_failure_the_caller_reports_ends_the_wait_at_once_and_a_timeout_names_what_the_server_lacked() {
    let fake = FakeChrome::new(
        Fake {
            working: true,
            ..Fake::default()
        },
        false,
    );
    let mut browser = launch(&fake.chrome).unwrap();
    let start = Instant::now();
    let failure = || Some("the page server failed: boom".to_string());
    let mut o = shot();
    o.failure = Some(&failure);
    let failed = browser.capture(&o).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(failed.contains("the page server failed: boom"), "{failed}");

    let explain = || "not found: /tools/capture/bridge.js".to_string();
    let mut o = shot();
    o.timeout = Duration::from_millis(300);
    o.explain = Some(&explain);
    let late = browser.capture(&o).unwrap_err();
    assert!(late.contains("timed out"), "{late}");
    assert!(
        late.contains("not found: /tools/capture/bridge.js"),
        "{late}"
    );
    browser.close().unwrap();
    fake.expect_profile_gone();
}

#[test]
fn a_page_server_that_fails_fails_the_run_at_once_naming_the_error_not_as_a_timeout() {
    // A window whose index.html no longer loads main.js exactly once: the
    // server's handler fails on it. The run must say so at once, not wait
    // out 150 s and report "timed out".
    let scratch = Scratch::new("run");
    let dir = scratch.0.clone();
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("desktop/ui")).unwrap();
    std::fs::write(
        repo.join("desktop/ui/index.html"),
        "<html><head></head><body>no script</body></html>",
    )
    .unwrap();
    let tmp = dir.join("tmp");
    std::fs::create_dir(&tmp).unwrap();

    // A real Chrome loads the page it is sent to; so does this fake.
    let nav: Navigate = Arc::new(|url: &str| {
        let _ = xtask::capture::http::fetch("GET", url, &[], &[], Duration::from_secs(5));
    });
    let fake = FakeChrome::new(
        Fake {
            working: true,
            navigate: Some(nav),
            ..Fake::default()
        },
        false,
    );
    let chrome = fake.chrome.clone();
    let start = Instant::now();
    let err = run_capture(RunOptions {
        shots: SHOTS.iter().filter(|s| s.name == "drop").copied().collect(),
        demo_pdf: repo.join("demo.pdf"),
        repo_dir: repo,
        library: dir.join("Documents").join("Screepub"),
        launch: Box::new(move || launch(&chrome).map(|c| Box::new(c) as Box<dyn Browser>)),
        window: Box::new(DesktopUi),
        engine: vec!["false".into()],
        out_dir: dir.join("out"),
        tmp: Some(tmp.clone()),
        calibre_config: Some(dir.join("no-calibre")),
        log: Box::new(|_| {}),
        interrupt: None,
    })
    .expect_err("the run should fail");
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(
        err.message.contains("main.js exactly once"),
        "{}",
        err.message
    );
    assert!(!err.message.contains("timed out"), "{}", err.message);
    assert!(std::fs::read_dir(&tmp).unwrap().next().is_none());
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["repo", "tmp"]);
    fake.expect_profile_gone();
}

/// New with the port, and only where Chrome is: app.js's own builders, run
/// in Chrome, answer exactly what the gate's Rust copy of them does.
#[test]
fn with_real_chrome_app_js_builders_answer_what_the_gates_copy_does() {
    if !Path::new(CHROME).exists() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let app = std::fs::read(xtask::paths::repo_root().join("desktop/ui/app.js")).unwrap();
    let mut server = Server::start(move |req| match req.path() {
        "/" => Response::new(
            200,
            "text/html; charset=utf-8",
            b"<!doctype html><title>argv</title>".to_vec(),
        ),
        "/app.js" => Response::new(200, "text/javascript; charset=utf-8", app.clone()),
        _ => Response::say("", 404),
    })
    .unwrap();
    let base = format!("http://127.0.0.1:{}", server.port());
    let mut chrome = launch(Path::new(CHROME)).unwrap();
    let mut o = CaptureOptions::page(format!("{base}/"), 100, 100, Theme::Light);
    o.wait = xtask::capture::backend::Wait::Loaded;
    o.timeout = Duration::from_secs(30);
    chrome.capture(&o).unwrap();
    let js = r#"import('/app.js').then(({ argv: a }) => JSON.stringify([
      a.version(),
      a.convert('/d.pdf'), a.convert('/d.pdf', { force: true, optionsJson: '{"a":1}' }),
      a.convert('/d.pdf', { optionsJson: '' }),
      a.reconvert('/f.fountain', '/e.epub', '{}'),
      a.devices(), a.devices({ quick: true }),
      a.eject('/Volumes/K'), a.readerDevice('kobo'),
      a.send('/e.epub'), a.send('/e.epub', 'dev'),
      a.settings('/f.fountain'), a.settings('/f.fountain', '{"x":2}'),
      a.export('/e.epub', { forFormat: 'kindle' }),
      a.export('/e.epub', { forFormat: 'epub', fountain: '/f', optionsJson: '{}', out: '/o' }),
      a.kindleCheck('/e.epub'), a.kfxStatus(), a.kfxInstall(),
      a.appSettings(), a.appSettings('{"y":3}'),
      a.routes('/e.epub'), a.routes('/e.epub', { quick: true }),
      a.route('save-epub', '/e.epub', { out: '/o', fountain: '/f', optionsJson: '{}' }),
    ]))"#;
    let got = chrome.evaluate(js);
    let _ = chrome.close();
    server.stop(Duration::from_secs(5));
    let got: Value = serde_json::from_str(got.unwrap().as_str().unwrap()).unwrap();
    let want = json!([
        argv::version(),
        argv::convert("/d.pdf", false, None),
        argv::convert("/d.pdf", true, Some(r#"{"a":1}"#)),
        argv::convert("/d.pdf", false, Some("")),
        argv::reconvert("/f.fountain", "/e.epub", "{}"),
        argv::devices(false),
        argv::devices(true),
        argv::eject("/Volumes/K"),
        argv::reader_device("kobo"),
        argv::send("/e.epub", None),
        argv::send("/e.epub", Some("dev")),
        argv::settings("/f.fountain", None),
        argv::settings("/f.fountain", Some(r#"{"x":2}"#)),
        argv::export("/e.epub", "kindle", None, None, None),
        argv::export("/e.epub", "epub", Some("/f"), Some("{}"), Some("/o")),
        argv::kindle_check("/e.epub"),
        argv::kfx_status(),
        argv::kfx_install(),
        argv::app_settings(None),
        argv::app_settings(Some(r#"{"y":3}"#)),
        argv::routes("/e.epub", false),
        argv::routes("/e.epub", true),
        argv::route("save-epub", "/e.epub", Some("/o"), Some("/f"), Some("{}")),
    ]);
    assert_eq!(got, want);
}

/// New with the port, and only where Chrome is: make-og draws the social
/// card at 1200 by 630 (into a scratch file, never site/og.png). The card
/// loads nothing from the network.
#[test]
fn with_real_chrome_make_og_draws_the_card_at_1200_by_630() {
    if !Path::new(CHROME).exists() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let s = Scratch::new("og");
    let out = s.0.join("og.png");
    let card = xtask::paths::repo_root().join("site/og-card.html");
    let size = xtask::capture::make_og(Path::new(CHROME), &card, &out).unwrap();
    assert_eq!(size, (1200, 630));
    let rgba = xtask::capture::picture::decode(&std::fs::read(&out).unwrap()).unwrap();
    // Something was drawn: not one flat colour.
    assert!(rgba.pixels.chunks_exact(4).any(|p| p != &rgba.pixels[..4]));
}
