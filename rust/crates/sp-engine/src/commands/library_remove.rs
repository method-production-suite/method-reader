//! The `library-remove` verb: one script's folder off the library, to the
//! system Trash (the Recycle Bin on Windows, the freedesktop trash on
//! Linux), never deleted outright
//! (docs/superpowers/specs/2026-11-03-library-remove-design.md). A script's
//! folder can hold the only copy of its `.fountain` and its own settings,
//! so it goes where the person can put it back.
//!
//! It takes only what the `library` listing would list, by the listing's
//! own rule ([`super::library::script_in`]): a folder directly under the
//! library root, compared with links resolved on both sides so `..` and a
//! linked root cannot reach outside it; not a link itself; not the root.
//! Anything else is refused with its reason and nothing is moved.
//!
//! THE SEAM: [`TRASH_ENV`], when set, names a folder (already there) that
//! stands for the Trash, and the script's folder is renamed into it (under "<name> 2",
//! "<name> 3", ... when its name is taken there). Only when it is unset
//! does the system Trash run. Every test sets it, and sp-engine's test
//! guard sets it under /dev/null, so a test that forgets fails instead of
//! filling the owner's Trash. The recent list is left alone: it names the
//! PDF chosen, which is still where it was.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::library::script_in;
use crate::env::Env;
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::library::library_root;

/// The folder that stands for the Trash, for tests.
pub const TRASH_ENV: &str = "METHOD_READER_TRASH";

/// What `library-remove` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryRemoved {
    /// The folder, as it was given.
    pub removed: String,
    /// The stem its files were named by.
    pub name: String,
}

fn not_a_script(why: &str, folder: &str) -> EngineError {
    CliError::new(
        ErrorCode::NotAScript,
        format!("not a script in the library ({why}): {folder}"),
    )
    .into()
}

/// `library-remove <folder>`: the library a conversion would land in
/// ([`library_root`]), and `folder` off it.
pub fn library_remove_command(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
    folder: &str,
) -> Result<LibraryRemoved, EngineError> {
    remove_from_library(&library_root(platform, env, settings_path), folder, env)
}

/// `folder` off the library at `root`, to the Trash (or to the stand-in
/// [`TRASH_ENV`] names, when `env` sets it).
pub fn remove_from_library(
    root: &str,
    folder: &str,
    env: &Env,
) -> Result<LibraryRemoved, EngineError> {
    if !Path::new(folder).is_absolute() {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("library-remove needs a script folder's full path, not \"{folder}\""),
        )
        .into());
    }
    let meta = fs::symlink_metadata(folder).map_err(|_| not_a_script("it is not there", folder))?;
    if meta.file_type().is_symlink() {
        return Err(not_a_script("it is a link", folder));
    }
    if !meta.is_dir() {
        return Err(not_a_script("it is not a folder", folder));
    }
    let real_root = fs::canonicalize(root)
        .map_err(|_| not_a_script("the library folder is not there", folder))?;
    let real = fs::canonicalize(folder).map_err(|_| not_a_script("it is not there", folder))?;
    if real == real_root {
        return Err(not_a_script("it is the library folder itself", folder));
    }
    if real.parent() != Some(real_root.as_path()) {
        return Err(not_a_script(
            &format!("it is not directly inside {root}"),
            folder,
        ));
    }
    let script = script_in(&real.to_string_lossy()).ok_or_else(|| {
        not_a_script(
            "it has no source.json naming its source, or no .fountain of that name",
            folder,
        )
    })?;
    match env.get(TRASH_ENV) {
        Some(stand_in) => into_stand_in(stand_in, &real, &script.name)?,
        None => into_trash(&real, &script.name)?,
    }
    Ok(LibraryRemoved {
        removed: folder.to_owned(),
        name: script.name,
    })
}

fn trash_failed(name: &str, why: impl std::fmt::Display) -> EngineError {
    CliError::new(
        ErrorCode::TrashFailed,
        format!("could not move {name} to the Trash: {why}"),
    )
    .into()
}

/// The first of `<base>`, `<base> 2`, `<base> 3`, ... with nothing there.
fn free_name(trash: &Path, base: &str) -> PathBuf {
    (1u64..)
        .map(|n| match n {
            1 => trash.join(base),
            n => trash.join(format!("{base} {n}")),
        })
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("a free name")
}

/// The stand-in Trash: `folder` renamed into `trash`, under a name nothing
/// there has. The folder must already be there: it is never made, so a
/// guard path cannot become a real folder on a system that would let it.
fn into_stand_in(trash: &str, folder: &Path, name: &str) -> Result<(), EngineError> {
    if !fs::metadata(trash).is_ok_and(|m| m.is_dir()) {
        return Err(trash_failed(
            name,
            format!("{TRASH_ENV} is not a folder: \"{trash}\""),
        ));
    }
    let base = folder
        .file_name()
        .map_or_else(|| name.to_owned(), |n| n.to_string_lossy().into_owned());
    fs::rename(folder, free_name(Path::new(trash), &base))
        .map_err(|e| trash_failed(name, format!("{e} ({trash})")))
}

/// The system's own Trash. On a Mac through the file manager's own call,
/// not by asking the Finder: that needs no permission to drive another
/// app, which the signed app does not hold, and makes no sound.
fn into_trash(folder: &Path, name: &str) -> Result<(), EngineError> {
    #[cfg(target_os = "macos")]
    let moved = {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        context.delete(folder)
    };
    #[cfg(not(target_os = "macos"))]
    let moved = trash::delete(folder);
    moved.map_err(|e| trash_failed(name, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_is_numbered_from_two() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().to_path_buf();
        fs::create_dir_all(dir.join("Moth Hour")).unwrap();
        assert_eq!(free_name(&dir, "Moth Hour"), dir.join("Moth Hour 2"));
        fs::create_dir_all(dir.join("Moth Hour 2")).unwrap();
        assert_eq!(free_name(&dir, "Moth Hour"), dir.join("Moth Hour 3"));
        assert_eq!(free_name(&dir, "Wren"), dir.join("Wren"));
    }
}
