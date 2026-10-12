//! The `node:fs` calls the engine makes, failing the way Node fails: with
//! an [`EngineError::Fs`] carrying Node's code and message, so the plain
//! words (`plain_reason`) and every message that quotes a system error read
//! the same as the TypeScript's.
//!
//! Also `src/atomic-write.ts`: write-then-rename, the one copy of the rule
//! for the files the engine keeps beside a book.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::errors::{EngineError, FsError};
use crate::nodepath::host;

fn fs_err(err: &std::io::Error, syscall: &str, path: &str, dest: Option<&str>) -> EngineError {
    EngineError::Fs(FsError::from_io(err, syscall, path, dest))
}

/// `readFileSync(path)`.
pub fn read(path: &str) -> Result<Vec<u8>, EngineError> {
    fs::read(path).map_err(|e| fs_err(&e, "open", path, None))
}

/// `readFileSync(path, 'utf8')`.
pub fn read_text(path: &str) -> Result<String, EngineError> {
    crate::jsjson::read_text(Path::new(path)).map_err(|e| fs_err(&e, "open", path, None))
}

/// `writeFileSync(path, data)`.
pub fn write(path: &str, data: &[u8]) -> Result<(), EngineError> {
    fs::write(path, data).map_err(|e| fs_err(&e, "open", path, None))
}

/// `renameSync(from, to)`.
pub fn rename(from: &str, to: &str) -> Result<(), EngineError> {
    fs::rename(from, to).map_err(|e| fs_err(&e, "rename", from, Some(to)))
}

/// `copyFileSync(from, to)`.
pub fn copy(from: &str, to: &str) -> Result<(), EngineError> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| fs_err(&e, "copyfile", from, Some(to)))
}

/// `mkdirSync(path, { recursive: true })`.
pub fn mkdir_all(path: &str) -> Result<(), EngineError> {
    fs::create_dir_all(path).map_err(|e| fs_err(&e, "mkdir", path, None))
}

/// `mkdirSync(path)`: the folder only, never its parents.
pub fn mkdir(path: &str) -> Result<(), EngineError> {
    fs::create_dir(path).map_err(|e| fs_err(&e, "mkdir", path, None))
}

/// `rmSync(path, { force: true })` for a file.
pub fn rm_force(path: &str) {
    let _ = fs::remove_file(path);
}

/// `rmSync(path, { recursive: true, force: true })`.
pub fn rm_tree_force(path: &str) {
    let _ = fs::remove_dir_all(path);
}

/// `existsSync(path)`: follows links, and any failure to ask is false.
pub fn exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

/// `statSync(path).isFile()`, false when it cannot be stat'ed.
pub fn is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// `statSync(path).mtimeMs`, or `None` when it cannot be stat'ed.
///
/// Node reports a fractional millisecond (`1760000000123.4567`); this is
/// the same value, from the same nanoseconds.
pub fn mtime_ms(path: &str) -> Option<f64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => mtime_of(d.as_secs(), d.subsec_nanos()),
        Err(before) => {
            let d = before.duration();
            -mtime_of(d.as_secs(), d.subsec_nanos())
        }
    })
}

/// libuv's `mtimeMs`: seconds * 1000 + nanoseconds / 1e6, in doubles.
fn mtime_of(secs: u64, nanos: u32) -> f64 {
    secs as f64 * 1000.0 + f64::from(nanos) / 1e6
}

/// `statSync(path).size`.
pub fn size(path: &str) -> Result<u64, EngineError> {
    fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| fs_err(&e, "stat", path, None))
}

/// `tempPathFor(path)`: the hidden temp sibling a write to `path` goes
/// through: same folder, a leading dot, and the pid.
pub fn temp_path_for(path: &str) -> String {
    host::join(&[
        host::dirname(path),
        format!(".{}.{}.tmp", host::basename(path), std::process::id()),
    ])
}

/// `promoteAtomic`: `fill(tmp)`, then rename over `path`. A failure takes
/// the temp file with it and reports the original error.
fn promote_atomic(
    path: &str,
    fill: impl FnOnce(&str) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    let tmp = temp_path_for(path);
    let result = fill(&tmp).and_then(|()| rename(&tmp, path));
    if result.is_err() {
        rm_force(&tmp);
    }
    result
}

/// `writeFileAtomicSync(path, data)`.
pub fn write_atomic(path: &str, data: &[u8]) -> Result<(), EngineError> {
    promote_atomic(path, |tmp| write(tmp, data))
}

/// `copyFileAtomicSync(source, destination)`.
pub fn copy_atomic(source: &str, destination: &str) -> Result<(), EngineError> {
    promote_atomic(destination, |tmp| copy(source, tmp))
}
