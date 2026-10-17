//! What the export, kfx and update ports add: the guards for an engine
//! handler called in process, a scratch copy of the Calibre folder (the
//! only folder real Calibre may be pointed at), a stand-in AZW3, and file
//! times. Kept in its own module so the ports landing beside it in
//! parallel cannot collide with it on a name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use sp_engine::env::Env;
use sp_engine::errors::EngineError;
use sp_engine::export::renderers::{Books, Renderers};

use super::CALIBRE_GUARD;

/// `TEST_SETTINGS_GUARD` (tests/isolate-app-settings.ts and .env.test).
pub const SETTINGS_GUARD: &str = "/dev/null/method-reader-test-guard";

/// The environment a test hands an engine handler called in process: PATH,
/// so tools resolve as they would, and both guards. Nothing else from this
/// process.
pub fn guarded_env() -> Env {
    let mut env = Env::new();
    if let Some(path) = std::env::var_os("PATH").and_then(|p| p.into_string().ok()) {
        env.set("PATH", path);
    }
    env.set("METHOD_READER_CONFIG_DIR", SETTINGS_GUARD);
    env.set("CALIBRE_CONFIG_DIRECTORY", CALIBRE_GUARD);
    env
}

/// The folder Calibre would use on this machine (sp-engine's
/// tests/common's `real_calibre_config`).
fn real_calibre_config() -> PathBuf {
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
        match std::fs::metadata(&src) {
            Ok(m) if m.is_dir() => copy_tree(&src, &dst, skip),
            Ok(_) => {
                let _ = std::fs::copy(&src, &dst);
            }
            Err(_) => {}
        }
    }
}

/// A scratch COPY of the real Calibre settings folder (caches left
/// behind), made once per test binary: the ONLY folder a test may point
/// real Calibre at (sp-engine's tests/common's `calibre_config_copy`).
/// Hand it to a spawned CLI with `sb.set("CALIBRE_CONFIG_DIRECTORY", ..)`,
/// or to a handler with [`calibre_env`].
pub fn calibre_config_copy() -> &'static Path {
    static COPY: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &COPY
        .get_or_init(|| {
            let scratch = tempfile::Builder::new()
                .prefix("method-reader-calibre-config-")
                .tempdir()
                .expect("scratch");
            let copy = super::canonical(scratch.path()).join("calibre");
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

/// The scratch copy as a string, for `sb.set`.
pub fn calibre_copy_str() -> String {
    calibre_config_copy().to_string_lossy().into_owned()
}

/// [`guarded_env`] with Calibre pointed at the scratch copy.
pub fn calibre_env() -> Env {
    guarded_env().with("CALIBRE_CONFIG_DIRECTORY", &calibre_copy_str())
}

/// tests/helpers/stamped-azw3.ts (sp-engine's tests/common/books.rs): the
/// smallest file the AZW3 staleness rule reads as one of ours, stamped
/// with `revision` (`None`: no revision record, as Calibre's AZW3).
pub fn stamped_azw3(revision: Option<u32>) -> Vec<u8> {
    let mut records: Vec<Vec<u8>> = Vec::new();
    let mut title = Vec::new();
    title.extend(503u32.to_be_bytes());
    title.extend(9u32.to_be_bytes());
    title.push(b'T');
    records.push(title);
    if let Some(rev) = revision {
        let mut r = Vec::new();
        r.extend(0x5350u32.to_be_bytes());
        r.extend(12u32.to_be_bytes());
        r.extend(rev.to_be_bytes());
        records.push(r);
    }
    let size: usize = records.iter().map(Vec::len).sum();
    let mut exth = b"EXTH".to_vec();
    exth.extend(((size + 12) as u32).to_be_bytes());
    exth.extend((records.len() as u32).to_be_bytes());
    for r in &records {
        exth.extend(r);
    }
    exth.extend(vec![0u8; 4 - size % 4]);

    let mut r0 = vec![0u8; 16];
    r0[0..2].copy_from_slice(&2u16.to_be_bytes());
    r0.extend(b"MOBI");
    r0.extend(264u32.to_be_bytes());
    r0.extend(2u32.to_be_bytes());
    r0.extend(65001u32.to_be_bytes());
    r0.extend(1u32.to_be_bytes());
    r0.extend(8u32.to_be_bytes());
    r0.resize(128, 0);
    r0.extend(0x50u32.to_be_bytes());
    r0.resize(16 + 264, 0);
    r0.extend(exth);

    let mut file = vec![0u8; 78];
    file[..1].copy_from_slice(b"T");
    file[60..68].copy_from_slice(b"BOOKMOBI");
    file[76..78].copy_from_slice(&1u16.to_be_bytes());
    let r0_at: u32 = 78 + 8 + 2;
    file.extend(r0_at.to_be_bytes());
    file.extend([0, 0, 0, 0]);
    file.extend([0, 0]);
    file.extend(r0);
    file
}

/// `stampedAzw3()`: [`stamped_azw3`] at the revision Method Reader's own writer
/// stamps today.
pub fn current_azw3() -> Vec<u8> {
    stamped_azw3(Some(Books.azw3_writer_revision()))
}

/// The MOBI header's file version in a PalmDB book (8 for a KF8-only
/// AZW3, `readKf8(..).header.version`): record 0's offset, then the
/// version 36 bytes into it.
pub fn mobi_header_version(book: &[u8]) -> u32 {
    let at = u32::from_be_bytes(book[78..82].try_into().expect("record 0")) as usize;
    assert_eq!(
        &book[at + 16..at + 20],
        b"MOBI",
        "record 0 is a MOBI header"
    );
    u32::from_be_bytes(book[at + 36..at + 40].try_into().expect("version"))
}

/// `utimes(path, at, at)`.
pub fn set_mtime(path: impl AsRef<Path>, at: SystemTime) {
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(path.as_ref())
        .expect("open to set times");
    f.set_times(std::fs::FileTimes::new().set_accessed(at).set_modified(at))
        .expect("set times");
}

/// A file's mtime.
pub fn mtime(path: impl AsRef<Path>) -> SystemTime {
    std::fs::metadata(path.as_ref())
        .and_then(|m| m.modified())
        .expect("mtime")
}

/// `statSync(path).mtimeMs`, as the engine reads it.
pub fn mtime_ms(path: &str) -> f64 {
    sp_engine::fsx::mtime_ms(path).expect("an mtime")
}

/// The names in a folder, sorted.
pub fn names(dir: impl AsRef<Path>) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.as_ref())
        .expect("readdir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// A refusal's `(code, message)`; panics on success. A non-contract error
/// reads as `<Name>`.
pub fn refusal<T: std::fmt::Debug>(result: Result<T, EngineError>) -> (String, String) {
    match result {
        Ok(v) => panic!("expected a refusal, got {v:?}"),
        Err(EngineError::Cli(e)) => (e.code.as_str().to_owned(), e.message),
        Err(other) => (format!("<{}>", other.name()), other.message()),
    }
}

/// Copy a file keeping its mtime (`cpSync(.., { preserveTimestamps })`).
pub fn copy_keeping_time(from: &Path, to: &Path) {
    std::fs::copy(from, to).expect("copy");
    set_mtime(to, mtime(from));
}

/// The text of a source file in the repo, by its path from the root.
pub fn source(rel: &str) -> String {
    std::fs::read_to_string(super::repo_root().join(rel))
        .unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}
