//! Where things are.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root (this crate is rust/crates/xtask).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("xtask sits three folders below the repo root")
        .to_path_buf()
}

/// The Cargo workspace (rust/).
pub fn workspace_root() -> PathBuf {
    repo_root().join("rust")
}

/// The committed, invented fixtures: tests/fixtures/*.pdf, sorted.
pub fn committed_fixtures() -> Vec<PathBuf> {
    inputs_in(&repo_root().join("tests/fixtures"), &["pdf"])
}

/// Root fixtures/, where the real scripts live (gitignored). A worktree has
/// none of its own, so the main checkout's is used, found as the parent of
/// the shared .git folder.
pub fn real_fixtures_dir() -> Option<PathBuf> {
    let here = repo_root().join("fixtures");
    if here.is_dir() {
        return Some(here);
    }
    let out = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(repo_root())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let common = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    let main = common.parent()?.join("fixtures");
    main.is_dir().then_some(main)
}

/// The real scripts: *.pdf and *.fountain in [`real_fixtures_dir`], sorted.
pub fn real_fixtures() -> Vec<PathBuf> {
    real_fixtures_dir().map_or_else(Vec::new, |d| inputs_in(&d, &["pdf", "fountain"]))
}

fn inputs_in(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        })
        .collect();
    out.sort();
    out
}

/// A file's stem as a string.
pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
