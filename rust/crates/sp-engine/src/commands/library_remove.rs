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
//!
//! A DRIVE WITH NO RECYCLE BIN: the Recycle Bin call Windows offers asks
//! "delete permanently?" when the folder's drive has no bin (a network
//! drive, most memory cards), and a yes deletes. So on Windows the real
//! Trash runs only when the drive's root holds a `$Recycle.Bin`
//! ([`drive_has_bin`]); otherwise the folder is refused with nothing moved.
//! The stand-in skips the check.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::library::script_in;
use crate::env::Env;
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::library::library_root;

/// The folder that stands for the Trash, for tests.
pub const TRASH_ENV: &str = "METHOD_READER_TRASH";

/// What the system Trash is called: the Recycle Bin on Windows, the Trash
/// everywhere else. The one home of the word, for the window and the
/// command line alike.
pub fn bin_name(windows: bool) -> &'static str {
    if windows { "Recycle Bin" } else { "Trash" }
}

/// The command line's plain-words answer to a removal.
pub fn moved_line(removed: &str, windows: bool) -> String {
    format!("moved {removed} to the {}", bin_name(windows))
}

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
        None => into_system_trash(
            cfg!(windows),
            folder,
            &real,
            &script.name,
            |p| fs::symlink_metadata(p).is_ok(),
            into_trash,
        )?,
    }
    Ok(LibraryRemoved {
        removed: folder.to_owned(),
        name: script.name,
    })
}

fn trash_failed(name: &str, why: impl std::fmt::Display) -> EngineError {
    CliError::new(
        ErrorCode::TrashFailed,
        format!(
            "could not move {name} to the {}: {why}",
            bin_name(cfg!(windows))
        ),
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

fn no_bin(folder: &str) -> EngineError {
    CliError::new(
        ErrorCode::TrashFailed,
        format!("The library is on a drive with no Recycle Bin, so nothing was moved: {folder}"),
    )
    .into()
}

/// The root of the drive a canonical Windows path is on, as text: `C:\`,
/// `\\?\C:\`, `\\server\share\` or `\\?\UNC\server\share\`. None for any
/// other shape, which no bin is looked for on. Text, not
/// [`Path::components`], so it reads a Windows path on every system and is
/// tested on every one.
fn drive_root(folder: &str) -> Option<String> {
    let (lead, rest) = match folder.strip_prefix(r"\\?\") {
        Some(rest) => (r"\\?\", rest),
        None => ("", folder),
    };
    let share = if lead.is_empty() {
        folder.strip_prefix(r"\\").map(|r| (r"\\", r))
    } else {
        rest.get(..4)
            .filter(|head| head.eq_ignore_ascii_case(r"UNC\"))
            .map(|_| (r"\\?\UNC\", &rest[4..]))
    };
    if let Some((head, rest)) = share {
        let mut parts = rest.split('\\');
        let server = parts.next().filter(|s| !s.is_empty())?;
        let share = parts.next().filter(|s| !s.is_empty())?;
        return Some(format!("{head}{server}\\{share}\\"));
    }
    let b = rest.as_bytes();
    let letter = b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || b[2] == b'\\');
    letter.then(|| format!("{lead}{}\\", &rest[..2]))
}

/// Whether the drive `folder` is on keeps a Recycle Bin: `exists` says
/// whether `<drive root>$Recycle.Bin` is there. A network share has one
/// only when `exists` finds it at the share's root; a path of no shape
/// [`drive_root`] knows has none.
fn drive_has_bin(folder: &Path, exists: impl Fn(&Path) -> bool) -> bool {
    drive_root(&folder.to_string_lossy())
        .is_some_and(|root| exists(Path::new(&format!("{root}$Recycle.Bin"))))
}

/// The real Trash, through `trash`; on Windows only once
/// [`drive_has_bin`] finds the drive's bin, so nothing is moved (or
/// deleted) when there is none.
fn into_system_trash(
    windows: bool,
    folder: &str,
    real: &Path,
    name: &str,
    exists: impl Fn(&Path) -> bool,
    trash: impl FnOnce(&Path, &str) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    if windows && !drive_has_bin(real, exists) {
        return Err(no_bin(folder));
    }
    trash(real, name)
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

    #[test]
    fn the_bin_is_called_by_its_platforms_name() {
        assert_eq!(bin_name(true), "Recycle Bin");
        assert_eq!(bin_name(false), "Trash");
        assert_eq!(
            moved_line(r"C:\Users\ann\Method Reader\Wren", true),
            r"moved C:\Users\ann\Method Reader\Wren to the Recycle Bin"
        );
        assert_eq!(
            moved_line("/Users/ann/Method Reader/Wren", false),
            "moved /Users/ann/Method Reader/Wren to the Trash"
        );
    }

    #[test]
    fn a_drive_root_is_read_from_every_windows_shape() {
        for (path, root) in [
            (r"\\?\C:\Users\ann\Method Reader\Wren", Some(r"\\?\C:\")),
            (r"C:\Users\ann", Some(r"C:\")),
            (r"d:", Some(r"d:\")),
            (
                r"\\?\UNC\shed\books\Method Reader\Wren",
                Some(r"\\?\UNC\shed\books\"),
            ),
            (r"\\?\unc\shed\books\Wren", Some(r"\\?\UNC\shed\books\")),
            (r"\\shed\books\Wren", Some(r"\\shed\books\")),
            (r"\\shed", None),
            (r"\\?\UNC\shed", None),
            (r"\\?\Volume{0000}\Wren", None),
            ("/Users/ann/Method Reader/Wren", None),
            ("", None),
        ] {
            assert_eq!(drive_root(path).as_deref(), root, "{path}");
        }
    }

    #[test]
    fn the_bin_is_looked_for_at_the_drives_own_root() {
        let asked = std::cell::RefCell::new(Vec::new());
        let probe = |answer: bool| {
            let asked = &asked;
            move |p: &Path| {
                asked.borrow_mut().push(p.to_string_lossy().into_owned());
                answer
            }
        };
        let local = Path::new(r"\\?\C:\Users\ann\Method Reader\Wren");
        assert!(drive_has_bin(local, probe(true)));
        assert!(!drive_has_bin(local, probe(false)));
        let share = Path::new(r"\\?\UNC\shed\books\Method Reader\Wren");
        assert!(!drive_has_bin(share, probe(false)), "a share, no bin");
        assert!(drive_has_bin(share, probe(true)), "a share that keeps one");
        assert_eq!(
            asked.borrow().as_slice(),
            [
                r"\\?\C:\$Recycle.Bin",
                r"\\?\C:\$Recycle.Bin",
                r"\\?\UNC\shed\books\$Recycle.Bin",
                r"\\?\UNC\shed\books\$Recycle.Bin",
            ]
        );
        // A shape with no drive root is never probed, and has no bin.
        assert!(!drive_has_bin(Path::new("relative"), |_| true));
    }

    fn refusal(err: EngineError) -> (String, String) {
        match err {
            EngineError::Cli(e) => (e.code.as_str().to_owned(), e.message),
            other => panic!("not a refusal: {other:?}"),
        }
    }

    #[test]
    fn a_drive_with_no_bin_is_refused_before_anything_moves() {
        let folder = r"C:\Users\ann\Method Reader\Wren";
        let real = Path::new(r"\\?\C:\Users\ann\Method Reader\Wren");
        let moved = std::cell::Cell::new(false);
        let trash = |_: &Path, _: &str| {
            moved.set(true);
            Ok(())
        };
        let err = into_system_trash(true, folder, real, "Wren", |_| false, trash).unwrap_err();
        assert_eq!(
            refusal(err),
            (
                "trash-failed".to_owned(),
                format!(
                    "The library is on a drive with no Recycle Bin, so nothing was moved: {folder}"
                )
            )
        );
        assert!(!moved.get(), "nothing moved");
        // A bin there: the Trash runs.
        into_system_trash(true, folder, real, "Wren", |_| true, trash).unwrap();
        assert!(moved.replace(false));
        // Off Windows there is nothing to check: the Trash runs.
        into_system_trash(
            false,
            "/Users/ann/Wren",
            Path::new("/Users/ann/Wren"),
            "Wren",
            |_| false,
            trash,
        )
        .unwrap();
        assert!(moved.get());
    }
}
