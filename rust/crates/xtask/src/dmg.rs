//! Mount a macOS .dmg and find the one app on it: was tools/dmg.ts, which
//! smoke-bundle.ts and verify-signing.ts shared, and is again the one copy
//! both `cargo xtask smoke-bundle` and `cargo xtask verify-signing` use.
//! macOS only; nothing else has hdiutil.
//!
//! hdiutil is run through the caller's closure, so tests hand in a fake.
//! The two tools' runners differ in shape (verify-signing's takes no
//! working folder), so each passes `&mut |argv| run(...)` adapted to this one.

use std::path::{Path, PathBuf};

use crate::cli_build::RunResult;

/// The runner the DMG helpers take: an argv, program first.
pub type DmgRun<'a> = dyn FnMut(&[String]) -> RunResult + 'a;

/// A mounted image and the one .app on it.
#[derive(Debug)]
pub struct MountedApp {
    pub app_path: PathBuf,
    pub mount: PathBuf,
}

impl MountedApp {
    /// Detach the image. The caller must run this on every way out: an
    /// attached image on a runner outlives the job and the next one inherits
    /// a busy mount point.
    pub fn detach(&self, run: &mut DmgRun<'_>) {
        detach(&self.mount, run);
    }
}

fn detach(mount: &Path, run: &mut DmgRun<'_>) {
    run(&[
        "hdiutil".to_string(),
        "detach".to_string(),
        mount.display().to_string(),
        "-force".to_string(),
    ]);
}

/// Mount `dmg_path` read-only under `work_dir` and return the one .app on
/// it. `tool` opens every error, so a failure names the tool that hit it.
/// The image is detached before any error after the attach.
pub fn mount_dmg_app(
    dmg_path: &Path,
    work_dir: &Path,
    run: &mut DmgRun<'_>,
    tool: &str,
) -> Result<MountedApp, String> {
    let mount = work_dir.join("mnt");
    std::fs::create_dir_all(&mount)
        .map_err(|e| format!("{tool}: cannot make {}: {e}", mount.display()))?;
    // -readonly so checking an artifact cannot modify it; -nobrowse so no
    // volume appears on a desktop the next job inherits.
    let attach = run(&[
        "hdiutil".to_string(),
        "attach".to_string(),
        dmg_path.display().to_string(),
        "-readonly".to_string(),
        "-nobrowse".to_string(),
        "-mountpoint".to_string(),
        mount.display().to_string(),
    ]);
    if attach.exit_code != 0 {
        let why: String = attach.stderr.trim().chars().take(500).collect();
        return Err(format!(
            "{tool}: hdiutil attach failed on {}: {why}",
            dmg_path.display()
        ));
    }
    // Found by suffix, not by name: the bundle was "Screepub Desktop.app"
    // until the identifier release, "Screepub.app" until 0.12.x and is
    // "Method Reader.app" since. Sorted so a
    // two-app error names them in a stable order.
    let mut apps: Vec<String> = std::fs::read_dir(&mount)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".app"))
                .collect()
        })
        .unwrap_or_default();
    apps.sort();
    if apps.len() != 1 {
        detach(&mount, run);
        let listed = if apps.is_empty() {
            String::new()
        } else {
            format!(" ({})", apps.join(", "))
        };
        return Err(format!(
            "{tool}: expected exactly one .app on the mounted image, found {}{listed}",
            apps.len()
        ));
    }
    Ok(MountedApp {
        app_path: mount.join(&apps[0]),
        mount,
    })
}
