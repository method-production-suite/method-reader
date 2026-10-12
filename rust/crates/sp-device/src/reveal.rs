//! The `reveal` verb: src/reveal.ts (the performer) and src/cli-reveal.ts
//! (the handler). Shows an existing file in the system's file manager. The
//! handler's two checks run BEFORE anything opens, and every test hands in
//! a fake [`Opener`], so no test opens a real Finder or Explorer window.

use crate::error::{CliError, ErrorCode};
use crate::process::{Opener, Platform, SpawnOpener};
use serde::Serialize;
use sp_fsutil::nodepath::{host, posix, win32};

/// `{ "revealed": <path> }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevealResult {
    pub revealed: String,
}

/// The argv that shows `path`, per platform.
///
/// darwin: `open -R <path>` selects the file itself in Finder.
///
/// win32: `explorer <folder>`, opening the CONTAINING folder rather than
/// `explorer /select,<path>`, which breaks on a name with a space (the
/// spawn layer quotes the whole element and explorer reads the quotes as
/// part of the path) or a comma (explorer ends the `/select,` token at the
/// first comma). The path is resolved before dirname so a trailing `\.`
/// does not hand back the file itself.
///
/// Everything else: `xdg-open <folder>`; there is no "select this file"
/// convention across Linux file managers.
pub fn reveal_argv(path: &str, platform: Platform) -> Vec<String> {
    match platform {
        Platform::Darwin => vec!["open".into(), "-R".into(), path.into()],
        Platform::Win32 => vec!["explorer".into(), win32::dirname(&win32::resolve(&[path]))],
        Platform::Linux => vec!["xdg-open".into(), posix::dirname(path)],
    }
}

/// Show `path` in the system's file manager. explorer exits 1 on plenty of
/// successful runs, so its code is never checked; every other opener's
/// non-zero exit is a real failure. A tool that cannot start at all is
/// reveal-failed too.
pub fn reveal_file(path: &str, platform: Platform, opener: &dyn Opener) -> Result<(), CliError> {
    let argv = reveal_argv(path, platform);
    let result = opener.open(&argv).map_err(|err| {
        CliError::new(
            ErrorCode::RevealFailed,
            format!("the file manager could not be opened to show {path}: {err}"),
        )
    })?;
    if platform == Platform::Win32 {
        return Ok(());
    }
    if result.code != 0 {
        let detail = result.stderr.trim();
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        return Err(CliError::new(
            ErrorCode::RevealFailed,
            format!("could not show {path} in the file manager{tail}"),
        ));
    }
    Ok(())
}

/// The handler: a relative path is usage, and an absolute one that is not a
/// file (missing, or a folder) is unreadable, both before anything opens.
pub fn reveal_command(
    path: &str,
    platform: Platform,
    opener: &dyn Opener,
) -> Result<RevealResult, CliError> {
    if !host::is_absolute(path) {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("reveal needs a full path, not \"{path}\""),
        ));
    }
    let is_file = std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false);
    if !is_file {
        return Err(CliError::new(
            ErrorCode::Unreadable,
            format!("cannot read the file to reveal: {path}"),
        ));
    }
    reveal_file(path, platform, opener)?;
    Ok(RevealResult {
        revealed: path.to_string(),
    })
}

/// [`reveal_command`] on this machine, for real.
pub fn reveal_here(path: &str) -> Result<RevealResult, CliError> {
    reveal_command(path, Platform::host(), &SpawnOpener)
}
