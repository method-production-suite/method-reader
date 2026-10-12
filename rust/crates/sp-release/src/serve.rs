//! A static file server on 127.0.0.1, for the handover rehearsal only: it
//! stands in for the GitHub release, serving `latest.json` and the update
//! archive from a scratch folder. GET and HEAD, no listing, nothing
//! outside the folder, every request printed.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};

/// The file a request path names under `root`, or None for anything that
/// would leave it.
pub fn resolve(root: &Path, request_path: &str) -> Option<PathBuf> {
    let path = request_path.split(['?', '#']).next().unwrap_or("");
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.as_os_str().is_empty() {
        return None;
    }
    let mut out = root.to_path_buf();
    for c in rel.components() {
        match c {
            Component::Normal(p) => out.push(p),
            _ => return None,
        }
    }
    out.is_file().then_some(out)
}

/// Serve `root` on `port` (0: any free port) until killed. Calls `ready`
/// with the bound address first.
pub fn serve(root: &Path, port: u16, ready: impl FnOnce(String)) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    ready(format!("http://{}", listener.local_addr()?));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                break;
            }
        }
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let target = parts.next().unwrap_or("/");
        let found = resolve(root, target).and_then(|p| std::fs::read(p).ok());
        let (status, body) = match (method, found) {
            ("GET" | "HEAD", Some(b)) => ("200 OK", b),
            ("GET" | "HEAD", None) => ("404 Not Found", Vec::new()),
            _ => ("405 Method Not Allowed", Vec::new()),
        };
        eprintln!("serve: {method} {target} -> {status}");
        let kind = if target.ends_with(".json") {
            "application/json"
        } else {
            "application/octet-stream"
        };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        if method == "GET" {
            let _ = stream.write_all(&body);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_outside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("latest.json"), "{}").unwrap();
        assert!(resolve(dir.path(), "/latest.json?x=1").is_some());
        assert!(resolve(dir.path(), "/../etc/passwd").is_none());
        assert!(resolve(dir.path(), "/").is_none());
        assert!(resolve(dir.path(), "/missing").is_none());
    }
}
