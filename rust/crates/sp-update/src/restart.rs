//! The restart into the new bundle: tauri 2.11.5's `process::restart`
//! (`restart_macos_app`), which tauri-plugin-process's `relaunch()` calls
//! in the Tauri window. On a Mac it reads the NEW bundle's Info.plist for
//! `CFBundleExecutable`, so the executable's name may change between
//! versions, spawns it with this process's arguments, and exits.
//!
//! The same function brings a Tauri window up as the Rust app on the
//! handover: the window's relaunch reads the Rust bundle's Info.plist.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::install::{InstallError, bundle_executable};

/// The command that starts the new copy: the bundle's executable on a Mac,
/// the file itself elsewhere (an AppImage, a plain binary).
pub fn relaunch_command(installed: &Path, args: &[OsString]) -> Result<Command, InstallError> {
    let program = if installed.extension().is_some_and(|e| e == "app") {
        bundle_executable(installed)?
    } else {
        installed.to_path_buf()
    };
    let mut cmd = Command::new(program);
    cmd.args(args);
    Ok(cmd)
}

/// Start the new copy with this process's own arguments. The caller exits
/// right after, as tauri's restart does (`exit(0)`), once its window has
/// closed; a spawn that fails is "installed", never "failed" (row U7).
pub fn relaunch(installed: &Path) -> Result<(), InstallError> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    relaunch_command(installed, &args)?
        .spawn()
        .map(|_| ())
        .map_err(|e| InstallError::Io(format!("failed to restart app: {e}")))
}
