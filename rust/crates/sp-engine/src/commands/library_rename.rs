//! The `library-rename` verb's engine half: a name kept for one script in
//! its folder's source.json ([`crate::library::NAME_KEY`]), with nothing on
//! disk renamed (docs/superpowers/specs/2026-11-03-library-rename-design.md).
//! The folder and its files keep the source's stem, because a conversion
//! finds its folder again by that stem and by the source source.json
//! records; renaming them would make the next conversion of the same PDF a
//! second script. The `.fountain` is not edited either.
//!
//! It takes only what the `library` listing would list, by the same check
//! `library-remove` makes ([`super::library::listed_script`]). The name is
//! trimmed; an empty one clears what is kept, so the script is called by
//! its title page again. One longer than
//! [`crate::library::MAX_NAME_CHARS`] characters, or holding a line break,
//! is refused before anything is written.
//!
//! Rebuilding the script's book with the new name is the command line's
//! half (sp-cli's verbs.rs): it runs the very conversion a reopen does,
//! from the library `.fountain` to `<stem>.epub`, which takes the kept name
//! as the book's title.

use serde::Serialize;

use super::library::{listed_script, script_in};
use crate::env::{Env, js_trim};
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::library::{kept_name_refusal, library_root, save_kept_name};
use crate::nodepath::host;

/// What `library-rename` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryRenamed {
    /// The folder, as it was given.
    pub folder: String,
    /// What the script is called now: the name it was given, or, when the
    /// name was cleared, its title page's Title (else its file name, as
    /// the listing's row shows it).
    pub title: String,
    /// Its book, `<folder>/<stem>.epub`, which the command line rebuilds.
    pub epub: String,
    /// The library `.fountain` the book is rebuilt from. Not in the
    /// answer: the rebuild's input.
    #[serde(skip)]
    pub fountain: String,
    /// The name kept, or `None` when it was cleared. Not in the answer:
    /// the plain-words line's.
    #[serde(skip)]
    pub name: Option<String>,
}

/// The command line's plain-words answer to a rename.
pub fn renamed_line(renamed: &LibraryRenamed) -> String {
    match &renamed.name {
        Some(name) => format!("renamed {} to \"{name}\"", renamed.folder),
        None => format!("{} is called by its title page again", renamed.folder),
    }
}

/// The name a rename keeps: `raw` trimmed, `None` when that leaves
/// nothing (the name is cleared), refused as a usage error when it is too
/// long or holds a line break.
pub fn kept_name_of(raw: &str) -> Result<Option<String>, EngineError> {
    let name = js_trim(raw);
    if name.is_empty() {
        return Ok(None);
    }
    match kept_name_refusal(name) {
        Some(why) => Err(CliError::new(ErrorCode::Usage, why).into()),
        None => Ok(Some(name.to_owned())),
    }
}

/// `library-rename <folder> <name>`: the library a conversion would land
/// in ([`library_root`]), and the name kept for `folder` in it.
pub fn library_rename_command(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
    folder: &str,
    name: &str,
) -> Result<LibraryRenamed, EngineError> {
    rename_in_library(&library_root(platform, env, settings_path), folder, name)
}

/// Keep `name` for the script `folder` is in the library at `root` (or
/// clear it, for an empty name). Writes nothing but that one key of the
/// folder's source.json, and nothing at all on a refusal.
pub fn rename_in_library(
    root: &str,
    folder: &str,
    name: &str,
) -> Result<LibraryRenamed, EngineError> {
    let (real, script) = listed_script("library-rename", root, folder)?;
    let name = kept_name_of(name)?;
    save_kept_name(&script.fountain, name.as_deref())?;
    let title = script_in(&real.to_string_lossy())
        .and_then(|s| s.title)
        .unwrap_or_else(|| script.name.clone());
    let prefix = host::join(&[folder, script.name.as_str()]);
    Ok(LibraryRenamed {
        folder: folder.to_owned(),
        title,
        epub: format!("{prefix}.epub"),
        fountain: format!("{prefix}.fountain"),
        name,
    })
}
