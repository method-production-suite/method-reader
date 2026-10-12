//! Shared by every test file: scratch folders, the guards that keep tests
//! away from the real settings file and the real Calibre folder, and the
//! runner for the TypeScript CLI the parity tests compare against.
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
use std::process::Command;
use std::sync::OnceLock;

use sp_engine::env::Env;

/// `TEST_SETTINGS_GUARD` (tests/isolate-app-settings.ts and .env.test).
pub const SETTINGS_GUARD: &str = "/dev/null/screepub-test-guard";

/// `CALIBRE_CONFIG_GUARD` (tools/calibre-config.ts and .env.test).
pub const CALIBRE_GUARD: &str = "/dev/null/screepub-test-calibre-guard";

/// The repository root (rust/crates/sp-engine/../../..).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repo root exists")
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
            .prefix(&format!("screepub-{prefix}-"))
            .tempdir()
            .expect("a scratch folder");
        let path = dir.path().canonicalize().expect("canonical");
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
    env.set("SCREEPUB_CONFIG_DIR", SETTINGS_GUARD);
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
                .prefix("screepub-calibre-config-")
                .tempdir()
                .expect("scratch");
            let copy = scratch
                .path()
                .canonicalize()
                .expect("canonical")
                .join("calibre");
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

/// Whether the TypeScript engine can be run here: bun on PATH and the
/// repo's node_modules installed. CI's rust job has neither, so the parity
/// tests skip there, saying so.
pub fn bun_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        repo_root().join("node_modules").exists()
            && repo_root().join("src/cli.ts").exists()
            && Command::new("bun")
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
    })
}

/// What one TypeScript CLI run printed.
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

/// `bun src/cli.ts <args>` from the repo root, with exactly `env` (plus
/// HOME, which bun needs for its own cache, unless `env` names one). The
/// settings and Calibre folders are whatever `env` says: a parity test
/// always passes its own scratch folder and either the Calibre guard or
/// [`calibre_env`]'s copy.
pub fn ts_cli(args: &[&str], env: &Env) -> TsRun {
    let mut command = Command::new("bun");
    command
        .arg("src/cli.ts")
        .args(args)
        .current_dir(repo_root())
        .env_clear()
        .envs(env.vars());
    if env.get("HOME").is_none()
        && let Some(home) = std::env::var_os("HOME")
    {
        command.env("HOME", home);
    }
    let out = command.output().expect("bun runs");
    TsRun {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Skip a parity test where the TypeScript cannot run, saying so.
#[macro_export]
macro_rules! require_bun {
    () => {
        if !$crate::common::bun_available() {
            eprintln!("skipped: bun or node_modules is not available here");
            return;
        }
    };
}

/// Whether this run records the TypeScript's answers instead of comparing
/// against the frozen ones (SCREEPUB_RECORD_FROZEN=1, with bun and
/// node_modules present). Recording rewrites tests/data/parity-cli.json.
pub fn recording() -> bool {
    std::env::var_os("SCREEPUB_RECORD_FROZEN").is_some()
}

/// Add one recorded answer to tests/data/parity-cli.json, with the facts of
/// the machine it was recorded on.
pub fn record(key: &str, value: serde_json::Value) {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _held = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/parity-cli.json");
    let mut all: std::collections::BTreeMap<String, serde_json::Value> =
        std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
    all.insert("$machine".into(), machine::facts(None));
    all.insert(key.to_owned(), value);
    std::fs::create_dir_all(file.parent().unwrap()).expect("mkdir");
    let mut text = serde_json::to_string_pretty(&all).expect("json");
    text.push('\n');
    std::fs::write(&file, text).expect("write the frozen answers");
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
        Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// `chmod(path, mode)`.
#[cfg(unix)]
pub fn chmod(path: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}
