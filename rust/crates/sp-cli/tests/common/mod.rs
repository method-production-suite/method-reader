//! What the sp-cli tests share: the built `method-reader`, run in a scratch
//! folder with every seam pinned, so no test can read or write this
//! machine's settings, library, readers or Calibre.
//!
//! The pins mirror the TypeScript tests' `runCli`: METHOD_READER_CONFIG_DIR and
//! METHOD_READER_LIBRARY in the scratch folder, METHOD_READER_VOLUME_ROOTS at an
//! empty folder, the reMarkable probe at a closed local port, and Calibre
//! at a guard path no Calibre can start against. A test that cares passes
//! its own values over these.

#![allow(dead_code)]

pub mod seams;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Calibre's guard: a path no Calibre can start against.
pub const CALIBRE_GUARD: &str = "/dev/null/method-reader-test-calibre-guard";

/// The repository root (this crate is rust/crates/sp-cli).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the repo root")
        .to_path_buf()
}

/// A committed, invented fixture: tests/fixtures/<name>.
pub fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

/// `path` with every link resolved (macOS hands out /var paths that are
/// really /private/var), written the way a person or a file dialog writes
/// it. On Windows canonicalize answers the verbatim `\\?\C:\...` form,
/// which turns off the system's own handling of `/` and is never what
/// reaches the CLI, so the plain drive path is answered instead.
pub fn canonical(path: &Path) -> PathBuf {
    let real = path.canonicalize().expect("canonical");
    #[cfg(windows)]
    {
        let text = real.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && !rest.starts_with(r"UNC\")
        {
            return PathBuf::from(rest);
        }
    }
    real
}

/// `rel` (its parts split on `/`) inside the folder `dir`, with the host's
/// separator, the way the engine's own paths come back.
pub fn join(dir: &str, rel: &str) -> String {
    rel.split('/')
        .fold(PathBuf::from(dir), |p, part| p.join(part))
        .to_string_lossy()
        .into_owned()
}

/// One run of the CLI.
#[derive(Debug, Clone)]
pub struct Ran {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Ran {
    /// The --json contract: stdout is EXACTLY one parseable object.
    pub fn json(&self) -> Value {
        let lines: Vec<&str> = self.stdout.trim().lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "stdout is one line: {:?} (stderr {:?})",
            self.stdout,
            self.stderr
        );
        serde_json::from_str(lines[0]).unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", lines[0]))
    }

    /// The `error.code` of a failed --json run.
    pub fn error_code(&self) -> String {
        let v = self.json();
        assert_eq!(v["ok"], Value::Bool(false), "a failure: {v}");
        v["error"]["code"].as_str().unwrap_or_default().to_owned()
    }

    pub fn error_message(&self) -> String {
        self.json()["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

/// A scratch folder with `config/`, `library/` and `empty/` (the default
/// mount parent, scanning nothing), removed when dropped.
pub struct Sandbox {
    pub dir: tempfile::TempDir,
    pub env: BTreeMap<String, String>,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let dir = tempfile::Builder::new()
            .prefix("method-reader-cli-test-")
            .tempdir()
            .expect("a scratch folder");
        let root = canonical(dir.path());
        for d in ["config", "library", "empty"] {
            std::fs::create_dir_all(root.join(d)).expect("mkdir");
        }
        let mut env = BTreeMap::new();
        let s = |p: PathBuf| p.to_string_lossy().into_owned();
        env.insert("METHOD_READER_CONFIG_DIR".into(), s(root.join("config")));
        env.insert("METHOD_READER_LIBRARY".into(), s(root.join("library")));
        env.insert("METHOD_READER_VOLUME_ROOTS".into(), s(root.join("empty")));
        env.insert(
            "METHOD_READER_REMARKABLE_ENDPOINT".into(),
            "http://127.0.0.1:9".into(),
        );
        env.insert("CALIBRE_CONFIG_DIRECTORY".into(), CALIBRE_GUARD.into());
        Sandbox { dir, env }
    }

    /// The scratch folder, canonical (macOS hands out /var paths that are
    /// really /private/var).
    pub fn root(&self) -> PathBuf {
        canonical(self.dir.path())
    }

    /// `rel`, its parts split on `/`, inside the scratch folder, with the
    /// host's separator throughout.
    pub fn path(&self, rel: &str) -> PathBuf {
        PathBuf::from(join(&self.root().to_string_lossy(), rel))
    }

    /// `path` as a string, for argv.
    pub fn p(&self, rel: &str) -> String {
        self.path(rel).to_string_lossy().into_owned()
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn unset(&mut self, key: &str) -> &mut Self {
        self.env.remove(key);
        self
    }

    pub fn write(&self, rel: &str, data: impl AsRef<[u8]>) -> PathBuf {
        let p = self.path(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&p, data).expect("write");
        p
    }

    pub fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.path(rel);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    /// A copy of a committed fixture in the scratch folder.
    pub fn copy_fixture(&self, name: &str, rel: &str) -> PathBuf {
        let p = self.path(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::copy(fixture(name), &p).expect("copy fixture");
        p
    }

    /// Run `method-reader args` in the scratch folder.
    pub fn run(&self, args: &[&str]) -> Ran {
        self.run_in(&self.root(), args)
    }

    /// Run `method-reader args` with `cwd` as the working folder.
    pub fn run_in(&self, cwd: &Path, args: &[&str]) -> Ran {
        self.run_exe(env!("CARGO_BIN_EXE_method-reader"), cwd, args)
    }

    /// Run the old name, `screepub args`, in the scratch folder.
    #[allow(dead_code)]
    pub fn run_old_name(&self, args: &[&str]) -> Ran {
        self.run_exe(env!("CARGO_BIN_EXE_screepub"), &self.root(), args)
    }

    fn run_exe(&self, exe: &str, cwd: &Path, args: &[&str]) -> Ran {
        let mut c = Command::new(exe);
        c.args(args).current_dir(cwd);
        // Nothing this process carries can reach the run but PATH and
        // HOME, and the pins.
        c.env_clear();
        for k in [
            "PATH",
            "HOME",
            "TMPDIR",
            "SystemRoot",
            "USERPROFILE",
            "APPDATA",
        ] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        for (k, v) in &self.env {
            c.env(k, v);
        }
        let out = c.output().expect("method-reader runs");
        Ran {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }
}

/// A file's entries as a zip: name to bytes.
pub fn zip_entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let bytes = std::fs::read(path).expect("a zip");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("reads as a zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("an entry");
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("inflates");
            (f.name().expect("a name").into_owned(), data)
        })
        .collect()
}

/// One entry's text.
pub fn zip_text(path: &Path, name: &str) -> String {
    zip_entries(path)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, d)| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_else(|| panic!("no entry {name}"))
}

/// Every entry whose name ends with `suffix`, concatenated (the chapters).
pub fn zip_texts(path: &Path, suffix: &str) -> String {
    zip_entries(path)
        .into_iter()
        .filter(|(n, _)| n.ends_with(suffix))
        .map(|(_, d)| String::from_utf8_lossy(&d).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A REAL one-pixel PNG, told from another by that pixel's red channel
/// (the TypeScript tests' `encodePng(new Uint8Array([tag, 0, 0]), 1, 1)`).
pub fn png(tag: u8) -> Vec<u8> {
    sp_parser::png::encode_png(&[tag, 0, 0], 1, 1).expect("a one-pixel PNG")
}

/// Standard base64 with padding (Node's `Buffer.toString('base64')`).
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Every `--progress` line on stderr, as its `progress` object.
pub fn progress_events(stderr: &str) -> Vec<Value> {
    stderr
        .trim()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("a stderr line is not JSON ({e}): {l:?}"));
            v["progress"].clone()
        })
        .collect()
}

/// A folder's entry names, sorted.
pub fn list_dir(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Set a path's Unix permission bits.
#[cfg(unix)]
pub fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// The keys of a JSON object, sorted.
pub fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().expect("an object").keys().cloned().collect();
    k.sort();
    k
}

/// A stand-in for the reMarkable's USB web interface on 127.0.0.1: it
/// records `METHOD /path` for every request and answers `get` to a GET and
/// `post` to a POST, each with the body `[]`. `None` takes the request and
/// never answers (the connection is held open while the test runs).
pub struct Tablet {
    pub url: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Tablet {
    pub fn start(get: Option<u16>, post: Option<u16>) -> Tablet {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
        let url = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = std::sync::Arc::clone(&requests);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let Ok(clone) = stream.try_clone() else {
                    continue;
                };
                let mut reader = BufReader::new(clone);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_owned();
                let path = parts.next().unwrap_or("").to_owned();
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
                seen.lock()
                    .expect("the log")
                    .push(format!("{method} {path}"));
                let status = if method == "POST" { post } else { get };
                match status {
                    Some(code) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]"
                        );
                    }
                    None => held.push(stream),
                }
            }
        });
        Tablet { url, requests }
    }

    /// Every request so far, as `METHOD /path`.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("the log").clone()
    }
}

/// A folder of stand-in programs put first on the sandbox's PATH: each one
/// named appends `<name> <args>` to `tools.log` in the scratch folder and
/// exits 1, printing nothing else. So nothing a run asks `open`, `xdg-open`
/// or `defaults` for can open anything on this machine.
pub fn fake_tools(sb: &mut Sandbox, names: &[&str]) {
    let bin = sb.mkdir("fake-bin");
    let log = sb.path("tools.log");
    for name in names {
        let path = bin.join(name);
        script(
            &path,
            &format!("echo \"{name} $*\" >> '{}'\nexit 1", log.display()),
        );
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let joined = format!("{}:{path}", bin.display());
    sb.set("PATH", joined);
}

/// What the stand-ins logged, one line per call.
pub fn tools_log(sb: &Sandbox) -> Vec<String> {
    std::fs::read_to_string(sb.path("tools.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A `#!/bin/sh` script with `body`, executable.
pub fn script(path: &Path, body: &str) -> PathBuf {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("a script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path.to_path_buf()
}

/// A readable book: a few bytes at `rel` in the scratch folder. Nothing
/// reads it as an EPUB.
pub fn book_at(sb: &Sandbox, rel: &str) -> String {
    sb.write(rel, "book-bytes").to_string_lossy().into_owned()
}
