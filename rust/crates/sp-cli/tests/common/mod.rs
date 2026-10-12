//! What the sp-cli tests share: the built `screepub`, run in a scratch
//! folder with every seam pinned, so no test can read or write this
//! machine's settings, library, readers or Calibre.
//!
//! The pins mirror the TypeScript tests' `runCli`: SCREEPUB_CONFIG_DIR and
//! SCREEPUB_LIBRARY in the scratch folder, SCREEPUB_VOLUME_ROOTS at an
//! empty folder, the reMarkable probe at a closed local port, and Calibre
//! at a guard path no Calibre can start against. A test that cares passes
//! its own values over these.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Calibre's guard: a path no Calibre can start against.
pub const CALIBRE_GUARD: &str = "/dev/null/screepub-test-calibre-guard";

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
            .prefix("screepub-cli-test-")
            .tempdir()
            .expect("a scratch folder");
        let root = dir.path().canonicalize().expect("canonical");
        for d in ["config", "library", "empty"] {
            std::fs::create_dir_all(root.join(d)).expect("mkdir");
        }
        let mut env = BTreeMap::new();
        let s = |p: PathBuf| p.to_string_lossy().into_owned();
        env.insert("SCREEPUB_CONFIG_DIR".into(), s(root.join("config")));
        env.insert("SCREEPUB_LIBRARY".into(), s(root.join("library")));
        env.insert("SCREEPUB_VOLUME_ROOTS".into(), s(root.join("empty")));
        env.insert(
            "SCREEPUB_REMARKABLE_ENDPOINT".into(),
            "http://127.0.0.1:9".into(),
        );
        env.insert("CALIBRE_CONFIG_DIRECTORY".into(), CALIBRE_GUARD.into());
        Sandbox { dir, env }
    }

    /// The scratch folder, canonical (macOS hands out /var paths that are
    /// really /private/var).
    pub fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().expect("canonical")
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
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

    /// Run `screepub args` in the scratch folder.
    pub fn run(&self, args: &[&str]) -> Ran {
        self.run_in(&self.root(), args)
    }

    /// Run `screepub args` with `cwd` as the working folder.
    pub fn run_in(&self, cwd: &Path, args: &[&str]) -> Ran {
        let mut c = Command::new(env!("CARGO_BIN_EXE_screepub"));
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
        let out = c.output().expect("screepub runs");
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
