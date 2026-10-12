//! What the capture server answers, and what it refuses (was
//! tools/capture/server.ts).
//!
//! It serves three folders and three routes, and nothing else:
//!
//! ```text
//! desktop/ui/  site/  tools/capture/      the pages Chrome draws
//! /capture/window.html?shot=<window shot> the window's own page, with the
//!                                         capture scripts inserted
//! /pass-one/<shot>-<theme>.png            this run's first-pass pictures
//! /engine                                 the window's engine calls, run
//!                                         through the gate
//! ```
//!
//! An allow-list and not "the repository minus some things", because the
//! main checkout holds /fixtures/ (real, gitignored screenplays) and .git,
//! and a server that serves the repository serves those too. Every request
//! must also name this server's own host exactly, so a web page that got a
//! name of its own pointed at 127.0.0.1 still cannot read from it, and
//! /engine needs the run's token on top of that.
//!
//! Pure apart from reading files: the engine is passed in, so a test can
//! fake it and drive [`Handler::handle`] with plain [`Request`]s.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::gate::{GateAnswer, GateContext, gate_engine_call};
use super::http::{Request, Response, decode_uri_component};
use super::page::{CaptureConfig, capture_index};
use super::shots::{Kind, SHOTS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineResult {
    /// None when the engine was killed by a signal (or never started).
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// What the run hears from the server: each list grows as requests arrive.
#[derive(Clone, Default)]
pub struct Records {
    /// Each refused engine call, as the gate's sentence.
    pub refused: Arc<Mutex<Vec<String>>>,
    /// Each engine run that exited non-zero or printed nothing.
    pub failed: Arc<Mutex<Vec<String>>>,
    /// Each path answered 404, so a page that never says ready can be
    /// explained by the file it asked for and did not get.
    pub not_found: Arc<Mutex<Vec<String>>>,
}

pub fn snapshot(list: &Mutex<Vec<String>>) -> Vec<String> {
    list.lock().map(|l| l.clone()).unwrap_or_default()
}

fn note(list: &Mutex<Vec<String>>, line: String) {
    if let Ok(mut l) = list.lock() {
        l.push(line);
    }
}

pub type RunEngine = Arc<dyn Fn(&[String]) -> EngineResult + Send + Sync>;

pub struct HandlerOptions {
    pub repo_dir: PathBuf,
    /// Where this run keeps its first-pass pictures.
    pub pass_one: PathBuf,
    /// The per-run secret /engine requires.
    pub token: String,
    pub library: PathBuf,
    pub demo_pdf: PathBuf,
    /// The Host header every request must carry: `127.0.0.1:<port>`.
    pub host: String,
    pub run_engine: RunEngine,
    pub records: Records,
}

/// The only folders served, relative to the repository.
pub const SERVED: &[&str] = &["desktop/ui/", "site/", "tools/capture/"];

fn type_of(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// `^[a-z]+-(light|dark)\.png$`: a bare name this run could have written.
fn pass_one_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".png") else {
        return false;
    };
    let Some((shot, theme)) = stem.rsplit_once('-') else {
        return false;
    };
    !shot.is_empty()
        && shot.bytes().all(|b| b.is_ascii_lowercase())
        && matches!(theme, "light" | "dark")
}

/// Node's posix normalize for an absolute path: `.` and `..` folded, runs
/// of slashes collapsed, a trailing slash kept.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut s = format!("/{}", out.join("/"));
    let trailing = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    if trailing && s != "/" {
        s.push('/');
    }
    s
}

pub struct Handler {
    o: HandlerOptions,
}

impl Handler {
    pub fn new(o: HandlerOptions) -> Handler {
        Handler { o }
    }

    fn missing(&self, path: &str) -> Response {
        note(&self.o.records.not_found, path.to_string());
        Response::say("", 404)
    }

    fn engine(&self, req: &Request) -> Response {
        if req.method != "POST" {
            return Response::say("capture: /engine takes POST", 405);
        }
        if req.header("x-capture-token") != Some(self.o.token.as_str()) {
            return Response::say("capture: missing or wrong token", 403);
        }
        let Ok(body) = serde_json::from_slice::<Value>(&req.body) else {
            return Response::say("capture: the engine call was not JSON", 400);
        };
        let args: Option<Vec<String>> = body.get("args").and_then(Value::as_array).and_then(|a| {
            a.iter()
                .map(|x| x.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        });
        let Some(args) = args else {
            return Response::say("capture: an engine call is a list of strings", 400);
        };
        let ctx = GateContext {
            demo_pdf: &self.o.demo_pdf,
            library: &self.o.library,
        };
        if let GateAnswer::Refuse(reason) = gate_engine_call(&args, &ctx) {
            note(&self.o.records.refused, reason.clone());
            return Response::say(&reason, 403);
        }
        let run = (self.o.run_engine)(&args);
        let said = run.stderr.trim();
        let code = run
            .code
            .map_or_else(|| "null".to_string(), |c| c.to_string());
        if run.code != Some(0) {
            note(
                &self.o.records.failed,
                format!("{}: exited with {code}: {said}", args.join(" ")),
            );
        }
        // Exactly what desktop/src-tauri/src/sidecar.rs does: stdout is the
        // contract and the exit code is not, because an engine error exits 1
        // AND prints a perfectly good error object, which the window then
        // draws. Only an engine that printed nothing is an error, carrying
        // what it said on the way down. (A non-zero exit still fails the
        // run: see `failed`.)
        let answer = run.stdout.trim();
        if answer.is_empty() {
            return Response::say(
                &format!("the Screepub engine exited with {code} and printed nothing: {said}"),
                500,
            );
        }
        Response::say(answer, 200)
    }

    fn file(&self, path: &Path, content_type: &str) -> Result<Response, String> {
        let body =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        Ok(Response::new(200, content_type, body))
    }

    /// One request's answer. An Err is the handler failing (the window page
    /// it could not build, a file it could not read): the server answers 500
    /// and the run fails at once, naming it.
    pub fn handle(&self, req: &Request) -> Result<Response, String> {
        if req.header("host") != Some(self.o.host.as_str()) {
            return Ok(Response::say("capture: wrong host", 403));
        }
        let path = req.path();

        if path == "/engine" {
            return Ok(self.engine(req));
        }

        if path == "/capture/window.html" {
            let shot = req.query("shot").unwrap_or_default();
            if !SHOTS
                .iter()
                .any(|s| s.kind == Kind::Window && s.name == shot)
            {
                return Ok(Response::say(
                    &format!("capture: no window shot named {shot}"),
                    400,
                ));
            }
            let index = self
                .o
                .repo_dir
                .join("desktop")
                .join("ui")
                .join("index.html");
            let html = std::fs::read_to_string(&index)
                .map_err(|e| format!("could not read {}: {e}", index.display()))?;
            let demo = self.o.demo_pdf.to_string_lossy();
            let page = capture_index(
                &html,
                &CaptureConfig {
                    shot: &shot,
                    demo_pdf: &demo,
                    token: &self.o.token,
                },
            )?;
            return Ok(Response::new(
                200,
                "text/html; charset=utf-8",
                page.into_bytes(),
            ));
        }

        if let Some(name) = path.strip_prefix("/pass-one/") {
            // A bare name this run could have written, never a path.
            let file = self.o.pass_one.join(name);
            if !pass_one_name(name) || !file.is_file() {
                return Ok(self.missing(path));
            }
            return self.file(&file, "image/png");
        }

        // Decoded, THEN normalised, THEN checked: an encoded `..` (%2e%2e,
        // or a slash spelled %2f) is only a `..` after decoding, and only
        // resolved after normalising, so checking any earlier checks the
        // wrong string.
        let Ok(decoded) = decode_uri_component(path) else {
            return Ok(Response::say("capture: a malformed path", 400));
        };
        let normal = normalize(&decoded);
        let rel = &normal[1..];
        // A backslash or a drive colon is a separator on Windows: never part
        // of a name these folders hold.
        if rel.contains(['\\', ':', '\0']) || !SERVED.iter().any(|p| rel.starts_with(p)) {
            return Ok(self.missing(path));
        }
        let mut file = self.o.repo_dir.clone();
        for seg in rel.split('/').filter(|s| !s.is_empty()) {
            file.push(seg);
        }
        if !file.is_file() {
            return Ok(self.missing(path));
        }
        self.file(&file, type_of(rel))
    }
}
