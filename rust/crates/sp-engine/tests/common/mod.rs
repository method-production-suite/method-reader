//! Shared by every test file: scratch folders, the guards that keep tests
//! away from the real settings file and the real Calibre folder, and the
//! TypeScript CLI's frozen answers the parity tests compare against.
//!
//! THE GUARDS, in Rust. The TypeScript suite guards by mutating
//! `process.env` in a preload and by root `.env.test`. Rust tests are
//! threads of one process, where changing the environment is unsafe and
//! racy, so the engine takes its environment as a value instead, and a test
//! never hands it this process's own: [`guarded_env`] is the base every
//! test starts from. It carries the two guard paths from `.env.test`, so a
//! settings read that forgets its scratch path finds nothing and a write
//! fails loudly, and a Calibre spawn that forgets its scratch copy fails on
//! the guard instead of reaching the real folder.
//! `tests/guards.rs` checks the guard values against `.env.test` and that
//! no test file reaches for this process's environment directly.

#![allow(dead_code)]

pub mod books;
pub mod epub;
pub mod frozen;
#[path = "../../../sp-azw3/tests/common/kf8_reader.rs"]
pub mod kf8_reader;
pub mod kfx_stubs;
pub mod machine;
pub mod mobi_records;
#[path = "../../../sp-azw3/tests/common/palmdoc_decode.rs"]
pub mod palmdoc_decode;
pub mod png;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sp_engine::env::Env;

/// `TEST_SETTINGS_GUARD` (tests/isolate-app-settings.ts and .env.test).
pub const SETTINGS_GUARD: &str = "/dev/null/method-reader-test-guard";

/// `CALIBRE_CONFIG_GUARD` (tools/calibre-config.ts and .env.test).
pub const CALIBRE_GUARD: &str = "/dev/null/method-reader-test-calibre-guard";

/// The repository root (rust/crates/sp-engine/../../..).
pub fn repo_root() -> PathBuf {
    canonical(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

/// `path` with every link resolved, written the way a person or a file
/// dialog writes it. On Windows canonicalize answers the verbatim
/// `\\?\C:\...` form, which turns off the system's own handling of `/` and
/// is never what reaches the engine, so the plain drive path is answered.
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

/// A scratch folder, removed when dropped. Canonical, so a path the engine
/// resolves and a path the test built agree (macOS keeps the temp folder
/// behind /var -> /private/var).
pub struct Scratch {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
}

impl Scratch {
    pub fn new(prefix: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("method-reader-{prefix}-"))
            .tempdir()
            .expect("a scratch folder");
        let path = canonical(dir.path());
        Scratch { _dir: dir, path }
    }

    /// `join(scratch, name)` as a string.
    pub fn join(&self, name: &str) -> String {
        self.path.join(name).to_string_lossy().into_owned()
    }

    pub fn str(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    /// A new sub-folder, made.
    pub fn dir(&self, name: &str) -> String {
        let p = self.path.join(name);
        std::fs::create_dir_all(&p).expect("mkdir");
        p.to_string_lossy().into_owned()
    }
}

/// The environment a test hands the engine: PATH, so tools resolve as they
/// would, and both guards. Nothing else from this process.
pub fn guarded_env() -> Env {
    let mut env = Env::new();
    if let Some(path) = std::env::var_os("PATH").and_then(|p| p.into_string().ok()) {
        env.set("PATH", path);
    }
    env.set("METHOD_READER_CONFIG_DIR", SETTINGS_GUARD);
    env.set("CALIBRE_CONFIG_DIRECTORY", CALIBRE_GUARD);
    env
}

/// The folder Calibre would use on this machine, by Calibre's own rule
/// (tools/calibre-config.ts's realCalibreConfig): CALIBRE_CONFIG_DIRECTORY
/// when this shell sets it (and it is not the guard), else the per-OS
/// default.
pub fn real_calibre_config() -> PathBuf {
    let set = std::env::var("CALIBRE_CONFIG_DIRECTORY").unwrap_or_default();
    let set = set.trim();
    if !set.is_empty() && set != CALIBRE_GUARD {
        return PathBuf::from(set);
    }
    let home = std::env::home_dir().unwrap_or_default();
    if cfg!(target_os = "macos") {
        return home.join("Library/Preferences/calibre");
    }
    if cfg!(windows) {
        let app_data = std::env::var_os("APPDATA")
            .map_or_else(|| home.join("AppData").join("Roaming"), PathBuf::from);
        return app_data.join("calibre");
    }
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_default();
    let base = if xdg.trim().is_empty() {
        home.join(".config")
    } else {
        PathBuf::from(xdg.trim())
    };
    base.join("calibre")
}

fn copy_tree(from: &Path, to: &Path, skip: &Path) {
    std::fs::create_dir_all(to).expect("mkdir copy");
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for entry in entries.flatten() {
        let src = entry.path();
        if src == skip {
            continue;
        }
        let dst = to.join(entry.file_name());
        // Dereferenced, as cpSync's dereference: a link in the real folder
        // is copied as what it points at.
        match std::fs::metadata(&src) {
            Ok(m) if m.is_dir() => copy_tree(&src, &dst, skip),
            Ok(_) => {
                let _ = std::fs::copy(&src, &dst);
            }
            Err(_) => {}
        }
    }
}

/// tests/isolate-calibre-config.ts, once per test binary: a scratch COPY of
/// the real Calibre settings folder (caches left behind), so a question like
/// "is the KFX plugin installed?" gets this machine's answer while Calibre
/// writes only into the copy. An empty folder where there is no real one.
pub fn calibre_config_copy() -> &'static Path {
    static COPY: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &COPY
        .get_or_init(|| {
            let scratch = tempfile::Builder::new()
                .prefix("method-reader-calibre-config-")
                .tempdir()
                .expect("scratch");
            let copy = canonical(scratch.path()).join("calibre");
            let real = real_calibre_config();
            if real.exists() {
                copy_tree(&real, &copy, &real.join("caches"));
            } else {
                std::fs::create_dir_all(&copy).expect("mkdir");
            }
            (scratch, copy)
        })
        .1
}

/// [`guarded_env`] with Calibre pointed at the scratch copy: the ONLY
/// environment a test may run real Calibre with.
pub fn calibre_env() -> Env {
    guarded_env().with(
        "CALIBRE_CONFIG_DIRECTORY",
        &calibre_config_copy().to_string_lossy(),
    )
}

/// What one TypeScript CLI run printed (frozen: see [`frozen`]).
#[derive(Debug, Clone)]
pub struct TsRun {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl TsRun {
    /// The one JSON line `--json` prints, parsed.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(self.stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "not one JSON line ({e}): stdout {:?} stderr {:?}",
                self.stdout, self.stderr
            )
        })
    }
}

/// Write a file, making its folder.
pub fn write(path: &str, text: &str) {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, text).expect("write");
}

/// Read a file as text.
pub fn read(path: &str) -> String {
    std::fs::read_to_string(path).expect("read")
}

/// The names in a folder, sorted.
pub fn names(dir: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("readdir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// `JSON.parse` of a file, as a value.
pub fn read_json(path: &str) -> serde_json::Value {
    serde_json::from_str(&read(path)).expect("json")
}

/// An `Env` from pairs.
pub fn env_of(pairs: &[(&str, &str)]) -> Env {
    pairs.iter().map(|(k, v)| (*k, *v)).collect()
}

/// Whether this process runs as root (chmod 000 does not stop root).
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// An executable script at `path`, written by a child `sh` so this test
/// process never holds it open for writing. Linux refuses to run a file
/// that any process has open for writing (ETXTBSY), and a test thread
/// that forks while another thread writes its script carries that handle
/// into its child until the child execs: a script run at once then fails
/// to start, now and then, on a busy CI runner.
#[cfg(unix)]
pub fn executable(path: &str, contents: impl AsRef<str>) {
    use std::io::Write;
    let mut child = std::process::Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh", path])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("sh");
    child
        .stdin
        .take()
        .expect("piped")
        .write_all(contents.as_ref().as_bytes())
        .expect("a script");
    assert!(child.wait().expect("sh").success(), "wrote {path}");
}

/// `chmod(path, mode)`.
#[cfg(unix)]
pub fn chmod(path: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}
