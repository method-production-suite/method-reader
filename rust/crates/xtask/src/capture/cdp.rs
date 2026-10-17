//! Headless Chrome, driven over the DevTools protocol (was
//! tools/capture/cdp.ts): a plain ws://127.0.0.1 websocket (tungstenite,
//! no TLS), serde_json for the messages, nothing of Node's.
//!
//! Why not `chrome --screenshot`: given a --virtual-time-budget it does wait
//! for a fetch in flight, but it photographs whatever is on screen when the
//! budget runs out, a failed state included. This waits for the page to say
//! `ready` or `failed` first. Dark mode goes through the protocol as well
//! (Emulation.setEmulatedMedia), because the --force-prefers-color-scheme=dark
//! flag does nothing. Both measured 2026-09-22, Chrome 153.
//!
//! Every way this can go wrong ends with Chrome stopped and its profile
//! folder removed: a launch that fails part-way cleans up before it
//! returns, a request Chrome never answers times out, and a connection that
//! drops fails the request waiting on it at once.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use super::backend::{Browser, CaptureOptions, Wait};
use super::http;

pub const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// sRGB, so a picture's colours do not follow the display profile of the
/// Mac that took it; no first-run, background networking or component
/// updates, so nothing runs but the page.
pub const CHROME_FLAGS: &[&str] = &[
    "--headless",
    "--disable-gpu",
    "--hide-scrollbars",
    "--force-color-profile=srgb",
    "--no-first-run",
    "--disable-background-networking",
    "--disable-component-update",
    "--remote-debugging-port=0",
];

/// How long Chrome has to answer one protocol request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a page has to say `ready` or `failed`. Longer than the step
/// runner's own budget (the read shot waits on four steps of up to 30 s
/// each), so a page that gives up says why before this gives up on it.
pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(150);

/// The prefix of every profile folder this makes in the temp folder.
pub const PROFILE_PREFIX: &str = "method-reader-capture-chrome-";

/// A fresh, empty folder under `parent` whose name starts with `prefix`.
pub fn make_temp_dir(parent: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    for _ in 0..100 {
        let dir = parent.join(format!("{prefix}{}", &super::random_token()[..12]));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("could not make a fresh temp folder"))
}

fn exit_word(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return code.to_string();
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return format!("signal {sig}");
        }
    }
    "unknown".into()
}

fn debugging_port(profile: &Path, proc: &mut Child, chrome: &Path) -> Result<String, String> {
    for _ in 0..150 {
        if let Ok(Some(status)) = proc.try_wait() {
            return Err(format!(
                "capture: Chrome at {} exited ({}) before it opened its debugging port",
                chrome.display(),
                exit_word(status)
            ));
        }
        if let Ok(text) = std::fs::read_to_string(profile.join("DevToolsActivePort"))
            && let Some(port) = text.lines().next().map(str::trim).filter(|p| !p.is_empty())
        {
            return Ok(port.to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "capture: Chrome at {} did not open its debugging port within 15 s",
        chrome.display()
    ))
}

/// The page to drive: the one Chrome opened, or a new one if it lists none
/// (the list can be empty for a moment after the port opens).
fn page_target(port: &str) -> Result<String, String> {
    let base = format!("http://127.0.0.1:{port}");
    let ws_of = |v: &Value| {
        v.get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    for _ in 0..20 {
        if let Ok((200, body)) = http::fetch(
            "GET",
            &format!("{base}/json/list"),
            &[],
            &[],
            REQUEST_TIMEOUT,
        ) && let Ok(Value::Array(list)) = serde_json::from_slice::<Value>(&body)
            && let Some(url) = list
                .iter()
                .filter(|t| t.get("type").and_then(Value::as_str) == Some("page"))
                .find_map(ws_of)
        {
            return Ok(url);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (_, body) = http::fetch(
        "PUT",
        &format!("{base}/json/new?about:blank"),
        &[],
        &[],
        REQUEST_TIMEOUT,
    )
    .map_err(|e| format!("capture: Chrome has no page to drive and would not open one: {e}"))?;
    serde_json::from_slice::<Value>(&body)
        .ok()
        .as_ref()
        .and_then(ws_of)
        .ok_or_else(|| "capture: Chrome has no page to drive and would not open one".to_string())
}

/// One DevTools connection, used from one thread: a request is sent and
/// its answer read before the next is sent.
struct Session {
    ws: WebSocket<TcpStream>,
    next: u64,
    gone: Option<String>,
}

const CLOSED: &str = "Chrome's debugging connection closed";

impl Session {
    fn connect(url: &str) -> Result<Session, String> {
        let rest = url
            .strip_prefix("ws://")
            .ok_or_else(|| format!("capture: not a ws:// debugging URL: {url}"))?;
        let host = rest.split('/').next().unwrap_or("");
        let addr = host
            .parse()
            .map_err(|_| format!("capture: not a local debugging address: {host}"))?;
        let stream = TcpStream::connect_timeout(&addr, REQUEST_TIMEOUT)
            .map_err(|e| format!("capture: could not open Chrome's debugging connection: {e}"))?;
        let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
        let _ = stream.set_write_timeout(Some(REQUEST_TIMEOUT));
        let (ws, _) = tungstenite::client::client(url, stream).map_err(|e| {
            format!(
                "capture: Chrome's debugging connection did not open within {} s: {e}",
                REQUEST_TIMEOUT.as_secs()
            )
        })?;
        Ok(Session {
            ws,
            next: 0,
            gone: None,
        })
    }

    fn closed(&self) -> bool {
        self.gone.is_some()
    }

    fn send(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if let Some(gone) = &self.gone {
            return Err(format!("capture: {method}: {gone}"));
        }
        self.next += 1;
        let id = self.next;
        let text = json!({ "id": id, "method": method, "params": params }).to_string();
        if self.ws.send(Message::text(text)).is_err() {
            return Err(self.drop_with(method, CLOSED));
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!(
                    "capture: {method}: no answer from Chrome within {} s",
                    REQUEST_TIMEOUT.as_secs()
                ));
            }
            let _ = self.ws.get_mut().set_read_timeout(Some(left));
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    let Ok(m) = serde_json::from_str::<Value>(t.as_str()) else {
                        continue;
                    };
                    if m.get("id").and_then(Value::as_u64) != Some(id) {
                        continue;
                    }
                    if let Some(err) = m.get("error") {
                        let msg = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("an error");
                        return Err(format!("capture: {method}: {msg}"));
                    }
                    return Ok(m.get("result").cloned().unwrap_or(Value::Null));
                }
                Ok(Message::Close(_)) => return Err(self.drop_with(method, CLOSED)),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return Err(self.drop_with(method, CLOSED)),
            }
        }
    }

    fn drop_with(&mut self, method: &str, why: &str) -> String {
        let gone = self.gone.get_or_insert_with(|| why.to_string());
        format!("capture: {method}: {gone}")
    }

    fn close(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

/// SIGTERM, then SIGKILL if the process is still there after five seconds.
/// (Windows has no SIGTERM: there it is killed at once.)
pub fn stop(proc: &mut Child) {
    if let Ok(Some(_)) = proc.try_wait() {
        return;
    }
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;
        if let Ok(pid) = i32::try_from(proc.id()) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                if let Ok(Some(_)) = proc.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let _ = proc.kill();
    let _ = proc.wait();
}

/// Chrome's helper processes can still be writing into the profile for a
/// moment after the browser exits, so a failed removal is tried again.
fn remove_profile(profile: &Path) -> Result<(), String> {
    let mut attempt = 1;
    loop {
        match std::fs::remove_dir_all(profile) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) if attempt >= 5 => {
                return Err(format!("could not remove {}: {e}", profile.display()));
            }
            Err(_) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Chrome, launched and connected.
pub struct Chrome {
    version: String,
    proc: Child,
    profile: PathBuf,
    session: Option<Session>,
    closed: Option<Result<(), String>>,
}

pub fn launch(chrome: &Path) -> Result<Chrome, String> {
    let profile = make_temp_dir(&std::env::temp_dir(), PROFILE_PREFIX)
        .map_err(|e| format!("capture: could not make Chrome's profile folder: {e}"))?;
    let mut user_data = std::ffi::OsString::from("--user-data-dir=");
    user_data.push(profile.as_os_str());
    // Spawned, never run to completion here: the process that serves the
    // pages is this one, and Chrome must not be waited on while it asks for
    // them. (The server answers on threads of its own besides.)
    let proc = Command::new(chrome)
        .args(CHROME_FLAGS)
        .arg(user_data)
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let proc = match proc {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&profile);
            return Err(format!(
                "capture: could not start Chrome at {}: {e}",
                chrome.display()
            ));
        }
    };
    let mut browser = Chrome {
        version: String::new(),
        proc,
        profile,
        session: None,
        closed: None,
    };
    match browser.connect(chrome) {
        Ok(()) => Ok(browser),
        Err(e) => {
            let _ = browser.close();
            Err(e)
        }
    }
}

impl Chrome {
    /// Runs `expression` in the page last captured, awaiting a promise, and
    /// answers its value: for checks that ask the page a question.
    pub fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        let cdp = self.session.as_mut().ok_or("capture: Chrome is closed")?;
        let r = cdp.send(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": true, "returnByValue": true }),
        )?;
        if let Some(ex) = r.get("exceptionDetails") {
            return Err(format!("capture: the page threw: {ex}"));
        }
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    fn connect(&mut self, chrome: &Path) -> Result<(), String> {
        let port = debugging_port(&self.profile, &mut self.proc, chrome)?;
        let session = self.session.insert(Session::connect(&page_target(&port)?)?);
        let v = session.send("Browser.getVersion", json!({}))?;
        self.version = v
            .get("product")
            .and_then(Value::as_str)
            .unwrap_or("Chrome/unknown")
            .to_string();
        Ok(())
    }
}

const STATE_EXPR: &str = "JSON.stringify([window.__captureState, window.__captureError])";

/// For a page that says nothing: ready once it and its fonts have loaded,
/// and it is not the page being left (which carries __captureOld).
const LOADED_EXPR: &str = "JSON.stringify([!window.__captureOld && document.readyState === 'complete' \
     && document.fonts.status === 'loaded' ? 'ready' : 'working'])";

impl Browser for Chrome {
    fn version(&self) -> String {
        self.version.clone()
    }

    fn capture(&mut self, o: &CaptureOptions<'_>) -> Result<Vec<u8>, String> {
        let cdp = self.session.as_mut().ok_or("capture: Chrome is closed")?;
        cdp.send(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": o.width, "height": o.height, "deviceScaleFactor": o.scale, "mobile": false }),
        )?;
        cdp.send(
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-color-scheme", "value": o.theme.word() }] }),
        )?;
        cdp.send(
            "Emulation.setDefaultBackgroundColorOverride",
            if o.transparent {
                json!({ "color": { "r": 0, "g": 0, "b": 0, "a": 0 } })
            } else {
                json!({})
            },
        )?;
        // The page being left may still say `ready`. Cleared first, so a
        // poll that lands before the new page takes over reads "not yet"
        // rather than photographing the old page.
        cdp.send(
            "Runtime.evaluate",
            json!({ "expression":
                "delete window.__captureState; delete window.__captureError; window.__captureOld = true" }),
        )?;
        let nav = cdp.send("Page.navigate", json!({ "url": o.url }))?;
        if let Some(err) = nav
            .get("errorText")
            .and_then(Value::as_str)
            .filter(|e| !e.is_empty())
        {
            return Err(format!("capture: Chrome could not load {}: {err}", o.url));
        }

        let expression = match o.wait {
            Wait::PageSaysReady => STATE_EXPR,
            Wait::Loaded => LOADED_EXPR,
        };
        let start = Instant::now();
        let mut state = String::new();
        while start.elapsed() < o.timeout {
            if let Some(failed) = o.failure.and_then(|f| f()) {
                return Err(format!("capture failed at {}: {failed}", o.url));
            }
            let r = match cdp.send(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true }),
            ) {
                Ok(r) => r,
                // A page mid-navigation can refuse one evaluation; a dead
                // connection refuses them all.
                Err(e) if cdp.closed() => return Err(e),
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let said = r
                .pointer("/result/value")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .unwrap_or(Value::Null);
            state = said
                .get(0)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if state == "failed" {
                let err = said.get(1).and_then(Value::as_str).unwrap_or("undefined");
                return Err(format!("capture failed at {}: {err}", o.url));
            }
            if state == "ready" {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if state != "ready" {
            let why = o.explain.map(|f| f()).unwrap_or_default();
            return Err(format!(
                "capture timed out at {} after {} s (last state: {}){}",
                o.url,
                o.timeout.as_secs_f64(),
                if state.is_empty() { "none" } else { &state },
                if why.is_empty() {
                    String::new()
                } else {
                    format!("; {why}")
                }
            ));
        }
        let shot = cdp.send("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = shot.get("data").and_then(Value::as_str).unwrap_or("");
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("capture: Chrome's screenshot was not base64: {e}"))
    }

    fn close(&mut self) -> Result<(), String> {
        if let Some(done) = &self.closed {
            return done.clone();
        }
        if let Some(s) = self.session.as_mut() {
            s.close();
        }
        self.session = None;
        stop(&mut self.proc);
        let done = remove_profile(&self.profile);
        self.closed = Some(done.clone());
        done
    }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
