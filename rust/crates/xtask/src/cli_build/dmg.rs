//! Mount a macOS .dmg and find the one app on it (the half of tools/dmg.ts
//! verify-signing uses). macOS only; nothing else has hdiutil.
//!
//! tools/dmg.ts was shared by smoke-bundle.ts and verify-signing.ts. This
//! copy is self-contained so the two ports could proceed apart; the
//! `packaging` module carries smoke-bundle's, and the two are to become one.

use std::path::{Path, PathBuf};

use super::Runner;

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
    pub fn detach(&self, run: Runner) {
        detach(&self.mount, run);
    }
}

fn detach(mount: &Path, run: Runner) {
    run(&[
        "hdiutil".to_string(),
        "detach".to_string(),
        mount.display().to_string(),
        "-force".to_string(),
    ]);
}

/// Mount `dmg_path` under `work_dir` and return the one .app on it. `tool`
/// opens every error, so a failure names the tool that hit it.
pub fn mount_dmg_app(
    dmg_path: &Path,
    work_dir: &Path,
    run: Runner,
    tool: &str,
) -> Result<MountedApp, String> {
    let mount = work_dir.join("mnt");
    std::fs::create_dir_all(&mount)
        .map_err(|e| format!("{tool}: cannot make {}: {e}", mount.display()))?;
    // -readonly so verifying an artifact cannot modify it; -nobrowse so no
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
    // until the identifier release and is "Screepub.app" since. Sorted so a
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
