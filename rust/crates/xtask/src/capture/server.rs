//! What the capture server answers, and what it refuses (was
//! tools/capture/server.ts).
//!
//! It serves the pages Chrome draws and this run's first-pass pictures,
//! and nothing else:
//!
//! ```text
//! site/  tools/capture/                   the before and after, the terminal, the frame, their faces
//! /pass-one/<shot>-<theme>.png            this run's first-pass pictures
//! ```
//!
//! An allow-list and not "the repository minus some things", because the
//! main checkout holds /fixtures/ (real, gitignored screenplays) and .git,
//! and a server that serves the repository serves those too. Every request
//! must also name this server's own host exactly, so a web page that got a
//! name of its own pointed at 127.0.0.1 still cannot read from it.
//!
//! The window itself is not served: it is drawn by sp-app, not by Chrome
//! (see [`super::backend`]), so the routes that put the old HTML window in
//! front of Chrome and carried its engine calls are gone with it.
//!
//! Pure apart from reading files, so a test can drive [`Handler::handle`]
//! with plain [`Request`]s.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::http::{Request, Response, decode_uri_component};

/// What the run hears from the server: the list grows as requests arrive.
#[derive(Clone, Default)]
pub struct Records {
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

pub struct HandlerOptions {
    pub repo_dir: PathBuf,
    /// Where this run keeps its first-pass pictures.
    pub pass_one: PathBuf,
    /// The Host header every request must carry: `127.0.0.1:<port>`.
    pub host: String,
    pub records: Records,
}

/// The only folders served, relative to the repository.
pub const SERVED: &[&str] = &["site/", "tools/capture/"];

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

    fn file(&self, path: &Path, content_type: &str) -> Result<Response, String> {
        let body =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        Ok(Response::new(200, content_type, body))
    }

    /// One request's answer. An Err is the handler failing (a file it
    /// could not read): the server answers 500 and the run fails at once,
    /// naming it.
    pub fn handle(&self, req: &Request) -> Result<Response, String> {
        if req.header("host") != Some(self.o.host.as_str()) {
            return Ok(Response::say("capture: wrong host", 403));
        }
        let path = req.path();

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
