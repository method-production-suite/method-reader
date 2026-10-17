//! The `node:fs` calls the engine makes, failing the way Node fails: with
//! an [`EngineError::Fs`] carrying Node's code and message, so the plain
//! words (`plain_reason`) and every message that quotes a system error read
//! the same as the TypeScript's.
//!
//! Also `src/atomic-write.ts`: write-then-rename, the one copy of the rule
//! for the files the engine keeps beside a book.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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
    if testing::take_fault(testing::Fault::Write) {
        let _ = fs::write(path, &data[..data.len() / 2]);
        return Err(testing::enospc());
    }
    fs::write(path, data).map_err(|e| fs_err(&e, "open", path, None))
}

/// `renameSync(from, to)`.
pub fn rename(from: &str, to: &str) -> Result<(), EngineError> {
    fs::rename(from, to).map_err(|e| fs_err(&e, "rename", from, Some(to)))
}

/// `copyFileSync(from, to)`.
pub fn copy(from: &str, to: &str) -> Result<(), EngineError> {
    if testing::take_fault(testing::Fault::Copy) {
        if let Ok(bytes) = fs::read(from) {
            let _ = fs::write(to, &bytes[..bytes.len() / 2]);
        }
        return Err(testing::enospc());
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| fs_err(&e, "copyfile", from, Some(to)))
}

/// A seam for tests only: make the NEXT write or copy on this thread put
/// half its bytes down and then fail the way a full disk does, which is
/// what the TypeScript's tests do with `spyOn(fs, ...)`. Thread-local, and
/// Rust runs each test on its own thread, so one test's fault never reaches
/// another's write.
#[doc(hidden)]
pub mod testing {
    use std::cell::Cell;

    use crate::errors::{EngineError, FsError};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Fault {
        Write,
        Copy,
    }

    thread_local! {
        static NEXT: Cell<Option<Fault>> = const { Cell::new(None) };
    }

    /// Arm one fault for the next matching call on this thread.
    pub fn fail_next(fault: Fault) {
        NEXT.with(|n| n.set(Some(fault)));
    }

    pub(crate) fn take_fault(fault: Fault) -> bool {
        NEXT.with(|n| {
            if n.get() == Some(fault) {
                n.set(None);
                true
            } else {
                false
            }
        })
    }

    pub(crate) fn enospc() -> EngineError {
        EngineError::Fs(FsError::with_code(
            "ENOSPC",
            "ENOSPC: no space left on device, write",
        ))
    }
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

/// `statSync(path).isDirectory()`, false when it cannot be stat'ed.
pub fn is_dir(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_dir())
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

/// The writes this process has started, so no two share a temp file.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// `tempPathFor(path)`: a fresh hidden temp sibling for a write to `path`:
/// same folder, a leading dot, the pid AND a per-process count
/// (`.<name>.<pid>-<n>.tmp`). The TypeScript's pid alone was enough when
/// every call was its own process; the window runs every call as a thread
/// of one, where two saves of one file shared a temp file.
pub fn temp_path_for(path: &str) -> String {
    let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    host::join(&[
        host::dirname(path),
        format!(".{}.{}-{n}.tmp", host::basename(path), std::process::id()),
    ])
}

/// `message` with any temp sibling of `path` ([`temp_path_for`], or the
/// TypeScript's pid-only form) spelled as `path`, so a failure names the
/// file the person asked for, never the hidden temp file.
pub fn hide_temp(message: &str, path: &str) -> String {
    let marker = "\u{0}";
    let shape = host::join(&[
        host::dirname(path),
        format!(".{}.{marker}.tmp", host::basename(path)),
    ]);
    let Some((before, after)) = shape.split_once(marker) else {
        return message.to_owned();
    };
    let pattern = format!(
        "{}[0-9]+(?:-[0-9]+)?{}",
        regex::escape(before),
        regex::escape(after)
    );
    match regex::Regex::new(&pattern) {
        Ok(re) => re.replace_all(message, regex::NoExpand(path)).into_owned(),
        Err(_) => message.to_owned(),
    }
}

/// `promoteAtomic`: `fill(tmp)`, then rename over `path`. The temp file
/// reaches the disk before the rename, so a power cut cannot promote an
/// empty file. A failure takes the temp file with it and reports the
/// original error.
fn promote_atomic(
    path: &str,
    fill: impl FnOnce(&str) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    let tmp = temp_path_for(path);
    let result = fill(&tmp)
        .and_then(|()| {
            fs::OpenOptions::new()
                .write(true)
                .open(&tmp)
                .and_then(|f| f.sync_all())
                .map_err(|e| fs_err(&e, "fsync", &tmp, None))
        })
        .and_then(|()| rename(&tmp, path));
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
