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

/// Where cargo writes this workspace's builds: cargo metadata's
/// `target_directory`, which follows `CARGO_TARGET_DIR` and any cargo config
/// (the suite builds every checkout into one shared folder outside the
/// repo). Never assume `rust/target`.
pub fn target_dir() -> PathBuf {
    target_dir_of(&workspace_root())
}

/// [`target_dir`] for another workspace (a second checkout, or a proof
/// crate with its own Cargo.toml). A folder with no Cargo.toml, as the tests'
/// scratch trees are, or one cargo cannot read, answers `<workspace>/target`.
pub fn target_dir_of(workspace: &Path) -> PathBuf {
    let fallback = workspace.join("target");
    let manifest = workspace.join("Cargo.toml");
    if !manifest.is_file() {
        return fallback;
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let Ok(out) = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
    else {
        return fallback;
    };
    if !out.status.success() {
        return fallback;
    }
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .ok()
        .and_then(|v| v["target_directory"].as_str().map(PathBuf::from))
        .unwrap_or(fallback)
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
