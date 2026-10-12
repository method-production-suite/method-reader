//! Calibre, ONLY ever isolated: the Rust form of tests/isolate-calibre-config.ts.
//!
//! An ordinary run of ebook-convert or calibre-debug rewrites files in
//! Calibre's settings folder (gui.json, found by the 0.7.3 QA pass), so no
//! test may run Calibre against the developer's real folder. Every run here
//! goes through [`Sandbox::run`], which hands the child an environment whose
//! CALIBRE_CONFIG_DIRECTORY is a scratch COPY of the real folder (without
//! its caches; symlinks copied as what they point at), made fresh for each
//! sandbox and removed with it. A copy and not an empty folder, so Calibre
//! behaves as it would outside. Nothing here runs Calibre any other way, not
//! even `--version`.
//!
//! Which folder is real is Calibre's own rule: CALIBRE_CONFIG_DIRECTORY when
//! the environment sets it (that is then the one to protect: copied, never
//! used), otherwise ~/Library/Preferences/calibre on macOS,
//! %APPDATA%\calibre on Windows and $XDG_CONFIG_HOME/calibre
//! (~/.config/calibre) on Linux.
//!
//! Shared by sp-mobi's and sp-azw3's tests (sp-azw3 includes this file).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What root .env.test gives CALIBRE_CONFIG_DIRECTORY: never a real folder.
pub const CALIBRE_CONFIG_GUARD: &str = "/dev/null/screepub-test-calibre-guard";

/// A scratch folder under the system temp folder, removed when dropped.
pub struct Scratch {
    pub path: PathBuf,
}

impl Scratch {
    pub fn new(label: &str) -> Scratch {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("screepub-rust-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        Scratch { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// The folder Calibre would use, by Calibre's own rule.
pub fn real_calibre_config() -> PathBuf {
    let set = std::env::var("CALIBRE_CONFIG_DIRECTORY").unwrap_or_default();
    let set = set.trim();
    if !set.is_empty() && set != CALIBRE_CONFIG_GUARD {
        return std::path::absolute(set).unwrap_or_else(|_| PathBuf::from(set));
    }
    if cfg!(target_os = "macos") {
        return home().join("Library/Preferences/calibre");
    }
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .map_or_else(|| home().join("AppData/Roaming"), PathBuf::from);
        return appdata.join("calibre");
    }
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_default();
    let base = if xdg.trim().is_empty() {
        home().join(".config")
    } else {
        PathBuf::from(xdg.trim())
    };
    base.join("calibre")
}

fn copy_tree(from: &Path, to: &Path, skip: &Path) {
    std::fs::create_dir_all(to).expect("a copy folder");
    for entry in std::fs::read_dir(from)
        .expect("the real Calibre folder")
        .flatten()
    {
        let src = entry.path();
        if src == skip {
            continue;
        }
        let dst = to.join(entry.file_name());
        // fs::metadata follows symlinks: a link is copied as what it points
        // at, so nothing in the copy can lead Calibre back into the real one.
        match std::fs::metadata(&src) {
            Ok(m) if m.is_dir() => copy_tree(&src, &dst, skip),
            Ok(_) => {
                std::fs::copy(&src, &dst).expect("a copied settings file");
            }
            Err(_) => {} // a dangling link: nothing to copy
        }
    }
}

/// One of Calibre's command-line tools, where Calibre installs them; None
/// when Calibre is absent (the tests then skip).
pub fn calibre_tool(name: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let path_entries = |exe: &str| -> Vec<PathBuf> {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).collect())
            .unwrap_or_default()
    };
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(format!(
            "/Applications/calibre.app/Contents/MacOS/{name}"
        )));
        candidates.push(PathBuf::from(format!("/opt/homebrew/bin/{name}")));
        candidates.push(PathBuf::from(format!("/usr/local/bin/{name}")));
    } else if cfg!(windows) {
        let exe = format!("{name}.exe");
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                candidates.push(PathBuf::from(pf).join("Calibre2").join(&exe));
            }
        }
        candidates.extend(path_entries(&exe));
    } else {
        candidates.push(PathBuf::from(format!("/usr/bin/{name}")));
        candidates.push(PathBuf::from(format!("/usr/local/bin/{name}")));
        candidates.extend(path_entries(name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// A run's exit code and its output, stdout then stderr.
pub struct Run {
    pub code: i32,
    pub out: String,
}

/// A scratch folder holding an isolated copy of Calibre's settings.
pub struct Sandbox {
    pub scratch: Scratch,
    config: PathBuf,
}

impl Sandbox {
    pub fn new(label: &str) -> Sandbox {
        let scratch = Scratch::new(label);
        let config = scratch.path.join("calibre-config");
        let real = real_calibre_config();
        if real.is_dir() {
            copy_tree(&real, &config, &real.join("caches"));
        } else {
            std::fs::create_dir_all(&config).expect("an empty settings copy");
        }
        Sandbox { scratch, config }
    }

    /// Where the book files go.
    pub fn dir(&self) -> &Path {
        &self.scratch.path
    }

    /// Runs a Calibre tool with the isolated settings copy, in `cwd`.
    pub fn run(&self, tool: &Path, args: &[&str], cwd: &Path) -> Run {
        assert!(
            self.config.starts_with(&self.scratch.path),
            "Calibre must only ever see the scratch copy"
        );
        let out = Command::new(tool)
            .args(args)
            .current_dir(cwd)
            .env("CALIBRE_CONFIG_DIRECTORY", &self.config)
            .output()
            .expect("Calibre starts");
        Run {
            code: out.status.code().unwrap_or(-1),
            out: format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        }
    }
}
