//! Shared by the device tests: scratch "volumes" and a stand-in for the
//! reMarkable's USB web interface. Nothing here reads a real mount, a real
//! USB device or a real tablet.
#![allow(dead_code)]

use sp_device::process::{Opener, OpenerOutput, RunOutput, Runner};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn scratch(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new().prefix(prefix).tempdir().unwrap()
}

/// A uniquely-rooted folder whose own name is the "volume name" under test,
/// holding `subdirs`.
pub fn volume(scratch: &Path, name: &str, subdirs: &[&str]) -> String {
    let parent = tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for sub in subdirs {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    s(&dir)
}

pub fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn join(a: &str, b: &str) -> String {
    s(&PathBuf::from(a).join(b))
}

/// A stand-in for the tablet's interface: records `METHOD /path` for every
/// request and answers `status` (or `documents_status` under /documents).
pub struct Stub {
    pub url: String,
    pub requests: Arc<Mutex<Vec<String>>>,
    pub status: Arc<AtomicU16>,
    pub documents_status: Arc<AtomicU16>,
}

impl Stub {
    pub fn start(status: u16) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let st = Arc::new(AtomicU16::new(status));
        let docs = Arc::new(AtomicU16::new(status));
        let (r, a, d) = (Arc::clone(&requests), Arc::clone(&st), Arc::clone(&docs));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut length = 0usize;
                let mut chunked = false;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                        chunked = true;
                    }
                }
                if chunked {
                    let mut tail = Vec::new();
                    let mut buf = [0u8; 4096];
                    while !tail.ends_with(b"0\r\n\r\n") {
                        match reader.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => tail.extend_from_slice(&buf[..n]),
                        }
                    }
                } else {
                    let mut body = vec![0u8; length];
                    let _ = reader.read_exact(&mut body);
                }
                r.lock().unwrap().push(format!("{method} {path}"));
                let code = if path.starts_with("/documents") {
                    d.load(Ordering::SeqCst)
                } else {
                    a.load(Ordering::SeqCst)
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]"
                );
            }
        });
        Stub {
            url,
            requests,
            status: st,
            documents_status: docs,
        }
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn reset(&self) {
        self.requests.lock().unwrap().clear();
    }
}

type Answer = Box<dyn Fn(&[String]) -> Result<RunOutput, String> + Send + Sync>;

/// A runner that records every argv and never spawns anything.
pub struct Recording {
    pub calls: Mutex<Vec<Vec<String>>>,
    answer: Answer,
}

impl Recording {
    pub fn new(
        answer: impl Fn(&[String]) -> Result<RunOutput, String> + Send + Sync + 'static,
    ) -> Arc<Recording> {
        Arc::new(Recording {
            calls: Mutex::new(Vec::new()),
            answer: Box::new(answer),
        })
    }

    pub fn ok() -> Arc<Recording> {
        Recording::new(|_| Ok(exit(0, "", "")))
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Runner for Recording {
    fn run(&self, argv: &[String], _timeout: Duration) -> Result<RunOutput, String> {
        self.calls.lock().unwrap().push(argv.to_vec());
        (self.answer)(argv)
    }
}

pub fn exit(code: i32, stdout: &str, stderr: &str) -> RunOutput {
    RunOutput {
        code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

/// An opener that records every argv and answers what it is told.
pub struct FakeOpener {
    pub calls: Mutex<Vec<Vec<String>>>,
    pub answer: Result<OpenerOutput, String>,
}

impl FakeOpener {
    pub fn new(answer: Result<OpenerOutput, String>) -> FakeOpener {
        FakeOpener {
            calls: Mutex::new(Vec::new()),
            answer,
        }
    }

    pub fn ok() -> FakeOpener {
        FakeOpener::new(Ok(OpenerOutput {
            code: 0,
            stderr: String::new(),
        }))
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Opener for FakeOpener {
    fn open(&self, argv: &[String]) -> Result<OpenerOutput, String> {
        self.calls.lock().unwrap().push(argv.to_vec());
        self.answer.clone()
    }
}

pub fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}
