//! Write-then-rename: the one copy of the rule for the files the engine
//! keeps beside a book (settings, sidecars, a library folder's source.json,
//! a rebuilt Kindle file, the pictures in a script's picture folder).
//! Ported from src/atomic-write.ts. [`crate::replace`] is the variant for a
//! reader's USB copy, which also flushes to the device.

use crate::error::NodeIoError;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The writes this process has started, so no two share a temp file.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// A fresh hidden temp sibling for a write to `path`: same folder, so it
/// shares a volume and the promote is a rename, not a copy; a leading dot so
/// nothing lists an unfinished file; the pid AND a per-process count, so
/// neither two engines nor two threads of one engine (the window runs every
/// call in one process) ever write through the same temp file. Each call
/// names a new one: `.<name>.<pid>-<n>.tmp`.
pub fn temp_path_for(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = format!(".{name}.{}-{n}.tmp", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(temp),
        None => PathBuf::from(temp),
    }
}

/// Put `fill(tmp)`'s result at `path` in one step, so `path` only ever holds
/// a complete file: a write that dies part way (disk full, the process
/// killed) leaves whatever was at `path` before, never half of the new one.
/// The temp file reaches the disk before the rename, so a power cut cannot
/// promote an empty file. A failure takes the temp file with it, and the
/// original error is what the caller sees.
fn promote_atomic(
    path: &Path,
    fill: impl FnOnce(&Path) -> Result<(), NodeIoError>,
) -> Result<(), NodeIoError> {
    let tmp = temp_path_for(path);
    let result = fill(&tmp)
        .and_then(|()| {
            fs::OpenOptions::new()
                .write(true)
                .open(&tmp)
                .and_then(|f| f.sync_all())
                .map_err(|e| NodeIoError::from_io(&e, "fsync", Some(&tmp), None))
        })
        .and_then(|()| {
            fs::rename(&tmp, path)
                .map_err(|e| NodeIoError::from_io(&e, "rename", Some(&tmp), Some(path)))
        });
    if result.is_err() {
        // Nothing more to do if this fails too: the write's own error is
        // the one reported.
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// `writeFileAtomicSync`: `data` at `path`, whole or not at all.
pub fn write_file_atomic(path: &Path, data: impl AsRef<[u8]>) -> Result<(), NodeIoError> {
    promote_atomic(path, |tmp| {
        // create_new: the name is fresh, so anything already there (a link
        // planted beside the file) is refused rather than written through.
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp)
            .map_err(|e| NodeIoError::from_io(&e, "open", Some(tmp), None))?;
        f.write_all(data.as_ref())
            .map_err(|e| NodeIoError::from_io(&e, "write", Some(tmp), None))
    })
}

/// `copyFileAtomicSync`: a copy of `source` at `destination`, whole or not
/// at all.
pub fn copy_file_atomic(source: &Path, destination: &Path) -> Result<(), NodeIoError> {
    promote_atomic(destination, |tmp| {
        fs::copy(source, tmp)
            .map(|_| ())
            .map_err(|e| NodeIoError::from_io(&e, "copyfile", Some(source), Some(tmp)))
    })
}
