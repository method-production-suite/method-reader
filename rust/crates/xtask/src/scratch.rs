//! Scratch folders for the tools.
//!
//! [`TempDir`] is the one self-removing work folder: build-cli, smoke-cli
//! and verify-signing use it (re-exported as `cli_build::TempDir`), and a
//! new tool that needs a throwaway folder should too.

use std::path::{Path, PathBuf};

/// A fresh folder under the system temp folder (which honours TMPDIR),
/// removed when this is dropped: on the way out of a failure as well as a
/// pass, which is what tests/tool-temp-folders.test.ts exists to check.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// `<temp>/<prefix><unique>`, created empty.
    pub fn new(prefix: &str) -> std::io::Result<Self> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for attempt in 0..1000u32 {
            let path = base.join(format!("{prefix}{pid}-{nanos}-{attempt}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("no free temp folder name"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
