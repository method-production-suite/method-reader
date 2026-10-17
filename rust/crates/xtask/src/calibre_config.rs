//! A scratch COPY of Calibre's settings folder: was tools/calibre-config.ts.
//!
//! For any tool here that runs the engine's Calibre code but must not touch
//! the real folder (the picture tool, `capture-screens`). A copy and not an
//! empty folder, so a question like "is the KFX Output plugin installed?"
//! gets the answer it would get outside, and the result screen in a picture
//! says what it would say on this machine. `caches` stays behind; Calibre
//! rebuilds it. Where there is no real folder (CI, any machine without
//! Calibre), the copy is an empty folder.
//!
//! The test suites' own isolation is per crate (sp-mobi and sp-azw3's
//! `Sandbox`, and sp-engine's port of tests/isolate-calibre-config.ts);
//! this module is the tools' copy of the same rule.

use std::path::{Path, PathBuf};

/// What root .env.test gives CALIBRE_CONFIG_DIRECTORY: a path under
/// /dev/null, where Calibre can neither read nor make a folder.
pub const CALIBRE_CONFIG_GUARD: &str = "/dev/null/method-reader-test-calibre-guard";

/// The environment and platform [`real_calibre_config`] reads, so a test
/// can ask about another machine.
pub struct Host<'a> {
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// `"macos"`, `"windows"` or anything else (Linux and the rest).
    pub os: &'a str,
    pub home: PathBuf,
}

/// The folder Calibre would use, by Calibre's own rule:
/// CALIBRE_CONFIG_DIRECTORY when the environment already sets it (that is
/// then the folder Calibre uses, so it is the one to protect, and it is
/// copied, never used), otherwise ~/Library/Preferences/calibre on macOS,
/// %APPDATA%\calibre on Windows, and $XDG_CONFIG_HOME/calibre
/// (~/.config/calibre by default) on Linux.
pub fn real_calibre_config(host: &Host) -> PathBuf {
    let get = |k: &str| (host.env)(k).unwrap_or_default();
    let set = get("CALIBRE_CONFIG_DIRECTORY");
    let set = set.trim();
    if !set.is_empty() && set != CALIBRE_CONFIG_GUARD {
        return absolute(Path::new(set));
    }
    match host.os {
        "macos" => host.home.join("Library/Preferences/calibre"),
        "windows" => (host.env)("APPDATA")
            .map_or_else(|| host.home.join("AppData").join("Roaming"), PathBuf::from)
            .join("calibre"),
        _ => {
            let xdg = get("XDG_CONFIG_HOME");
            let base = if xdg.trim().is_empty() {
                host.home.join(".config")
            } else {
                PathBuf::from(xdg.trim())
            };
            base.join("calibre")
        }
    }
}

/// This machine, as [`real_calibre_config`] sees it.
pub fn this_host_real_config() -> PathBuf {
    let env = |k: &str| std::env::var(k).ok();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    real_calibre_config(&Host {
        env: &env,
        os: std::env::consts::OS,
        home,
    })
}

fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| p.to_path_buf(), |d| d.join(p))
    }
}

/// Copies `real` (without its top-level `caches`) to `<scratch>/calibre` and
/// answers that path, or makes it empty when there is no real folder. A
/// symlink inside the real folder is copied as what it points at, so
/// nothing in the copy can lead Calibre back into it.
pub fn copy_calibre_config(real: &Path, scratch: &Path) -> std::io::Result<PathBuf> {
    let copy = scratch.join("calibre");
    if real.exists() {
        copy_tree(real, &copy, Some(&real.join("caches")))?;
    } else {
        std::fs::create_dir(&copy)?;
    }
    Ok(copy)
}

fn copy_tree(from: &Path, to: &Path, skip: Option<&Path>) -> std::io::Result<()> {
    // metadata() follows symlinks: dereference, as cpSync's option does.
    if std::fs::metadata(from)?.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let path = entry?.path();
            if skip.is_some_and(|s| s == path) {
                continue;
            }
            copy_tree(&path, &to.join(path.file_name().unwrap_or_default()), skip)?;
        }
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host<'a>(env: &'a dyn Fn(&str) -> Option<String>, os: &'a str) -> Host<'a> {
        Host {
            env,
            os,
            home: PathBuf::from("/home/someone"),
        }
    }

    // A rooted path with no drive is not absolute on Windows.
    #[cfg(unix)]
    #[test]
    fn an_environment_that_names_the_folder_is_the_folder_to_protect() {
        let env = |k: &str| (k == "CALIBRE_CONFIG_DIRECTORY").then(|| "/x/calibre ".to_string());
        assert_eq!(
            real_calibre_config(&host(&env, "macos")),
            PathBuf::from("/x/calibre")
        );
    }

    #[test]
    fn the_guard_path_is_not_a_folder_and_falls_through_to_the_platforms() {
        let env =
            |k: &str| (k == "CALIBRE_CONFIG_DIRECTORY").then(|| CALIBRE_CONFIG_GUARD.to_string());
        assert_eq!(
            real_calibre_config(&host(&env, "macos")),
            PathBuf::from("/home/someone/Library/Preferences/calibre")
        );
    }

    #[test]
    fn each_platform_has_its_own_default() {
        let none = |_: &str| None;
        assert_eq!(
            real_calibre_config(&host(&none, "linux")),
            PathBuf::from("/home/someone/.config/calibre")
        );
        let xdg = |k: &str| (k == "XDG_CONFIG_HOME").then(|| "/xdg".to_string());
        assert_eq!(
            real_calibre_config(&host(&xdg, "linux")),
            PathBuf::from("/xdg/calibre")
        );
        let app = |k: &str| (k == "APPDATA").then(|| "/appdata".to_string());
        assert_eq!(
            real_calibre_config(&host(&app, "windows")),
            PathBuf::from("/appdata").join("calibre")
        );
    }

    #[test]
    fn the_copy_leaves_caches_behind_and_an_absent_folder_copies_as_empty() {
        // Removed when dropped, on a failing assertion's unwind too.
        let work = crate::scratch::TempDir::new("method-reader-calibre-config-").unwrap();
        let root = work.path();
        let real = root.join("real");
        std::fs::create_dir_all(real.join("caches")).unwrap();
        std::fs::create_dir_all(real.join("plugins")).unwrap();
        std::fs::write(real.join("gui.json"), "{}").unwrap();
        std::fs::write(real.join("plugins/KFX Output.zip"), "zip").unwrap();
        std::fs::write(real.join("caches/x"), "x").unwrap();
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let copy = copy_calibre_config(&real, &scratch).unwrap();
        assert_eq!(copy, scratch.join("calibre"));
        assert_eq!(
            std::fs::read_to_string(copy.join("gui.json")).unwrap(),
            "{}"
        );
        assert!(copy.join("plugins/KFX Output.zip").is_file());
        assert!(!copy.join("caches").exists());

        let empty_scratch = root.join("empty");
        std::fs::create_dir_all(&empty_scratch).unwrap();
        let copy = copy_calibre_config(&root.join("missing"), &empty_scratch).unwrap();
        assert_eq!(std::fs::read_dir(&copy).unwrap().count(), 0);
    }
}
