//! Shared by the release tools' tests: scratch folders, scratch git repos,
//! and a fake GitHub on 127.0.0.1. Not every test file uses every helper.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A folder under the system temp dir, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("screepub-xtask-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        Scratch(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.0.join(p)
    }

    /// Write a file, making its folders.
    pub fn write(&self, rel: &str, text: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(&path, text).expect("write");
        path
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = make_writable(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn make_writable(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            make_writable(&path)?;
        } else {
            let mut p = std::fs::metadata(&path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            std::fs::set_permissions(&path, p)?;
        }
    }
    Ok(())
}

/// Run git in `dir`, failing loudly: ambient config (a global
/// commit.gpgsign, say) must not leave a scratch repo half set up.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A throwaway git repo with one commit tagged v0.1.0, user "Dev".
pub fn git_repo(name: &str) -> Scratch {
    let s = Scratch::new(name);
    git(s.path(), &["init", "-q"]);
    git(s.path(), &["config", "user.email", "dev@example.com"]);
    git(s.path(), &["config", "user.name", "Dev"]);
    git(s.path(), &["config", "commit.gpgsign", "false"]);
    git(s.path(), &["config", "tag.gpgsign", "false"]);
    s.write("a.txt", "one");
    git(s.path(), &["add", "-A"]);
    git(s.path(), &["commit", "-q", "-m", "first"]);
    git(s.path(), &["tag", "v0.1.0"]);
    s
}

/// Run the xtask binary with extra environment and optional stdin.
pub fn xtask(
    args: &[&str],
    env: &[(&str, &str)],
    cwd: Option<&Path>,
    stdin: Option<&str>,
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("xtask runs");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(text.as_bytes())
            .expect("write stdin");
    }
    child.wait_with_output().expect("xtask finishes")
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A fake GitHub: answers each path from a table, 404 for anything else.
/// Plain HTTP on 127.0.0.1, one request per connection.
pub struct FakeServer {
    pub base: String,
    routes: Arc<Mutex<HashMap<String, (u16, String)>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    pub fn start() -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let routes: Arc<Mutex<HashMap<String, (u16, String)>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (r, a) = (routes.clone(), asked.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line == "\r\n" || line == "\n" => break,
                        Ok(_) => {}
                    }
                }
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                a.lock().expect("lock").push(path.clone());
                let (status, body) = r
                    .lock()
                    .expect("lock")
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, "{\"message\":\"Not Found\",\"status\":\"404\"}".into()));
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            }
        });
        FakeServer {
            base,
            routes,
            asked,
        }
    }

    pub fn route(&self, path: &str, status: u16, body: impl Into<String>) {
        self.routes
            .lock()
            .expect("lock")
            .insert(path.to_string(), (status, body.into()));
    }

    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("lock").clone()
    }
}
