//! The path on the device, and the temp name a copy is uploaded under.
//!
//! An MTP path is not a file-system path: it is a chain of object names, each
//! looked up among its parent's children. So it is checked here as a list of
//! names, and anything that would mean something else on a disk (`..`, a
//! backslash, an absolute path) is refused rather than interpreted.

use crate::output::{Code, Failure};

/// MTP's own limit on an object name (mtp-rs's NewObjectInfo says 254).
pub const MAX_NAME: usize = 254;

/// What marks a copy still in flight. A Kindle indexes books by extension
/// and skips dotfiles, so `.Title.kfx.method-reader-part` is never shown as a book
/// even if a crash leaves it behind, and the next copy of the same book
/// removes it.
pub const TEMP_SUFFIX: &str = ".method-reader-part";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePath {
    /// Folders from the storage root down, e.g. ["documents"].
    pub folders: Vec<String>,
    /// The file's own name, e.g. "Title.kfx".
    pub name: String,
}

impl DevicePath {
    pub fn display(&self) -> String {
        let mut parts = self.folders.clone();
        parts.push(self.name.clone());
        parts.join("/")
    }
}

fn bad(message: impl Into<String>) -> Failure {
    Failure::new(Code::BadPath, message)
}

fn check_name(part: &str, whole: &str) -> Result<(), Failure> {
    if part.is_empty() {
        return Err(bad(format!("\"{whole}\" has an empty part in it")));
    }
    if part == "." || part == ".." {
        return Err(bad(format!(
            "\"{whole}\" uses \"{part}\", which an MTP device does not understand"
        )));
    }
    if part.contains('\\') || part.contains('\0') {
        return Err(bad(format!(
            "\"{whole}\" has a backslash or a null in a name"
        )));
    }
    if part.chars().count() > MAX_NAME {
        return Err(bad(format!(
            "a name in \"{whole}\" is longer than {MAX_NAME} characters"
        )));
    }
    Ok(())
}

/// `documents/Title.kfx` -> folders ["documents"], name "Title.kfx".
pub fn parse_device_path(raw: &str) -> Result<DevicePath, Failure> {
    if raw.starts_with('/') {
        return Err(bad(format!(
            "\"{raw}\" starts with a slash: give the path from the Kindle's storage, like documents/Title.kfx"
        )));
    }
    let parts: Vec<&str> = raw.split('/').collect();
    for part in &parts {
        check_name(part, raw)?;
    }
    let name = parts.last().copied().unwrap_or_default();
    if name.starts_with('.') && name.ends_with(TEMP_SUFFIX) {
        return Err(bad(format!(
            "\"{raw}\" is a name this helper keeps for copies in flight"
        )));
    }
    Ok(DevicePath {
        folders: parts[..parts.len() - 1]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        name: name.to_string(),
    })
}

/// The name a copy is uploaded under before it replaces `name`. Shortened
/// from the front of `name` when the whole would pass MTP's limit, so the
/// temp name is always one the device accepts; deterministic, so the next
/// copy of the same book finds and removes a leftover.
pub fn temp_name(name: &str) -> String {
    let room = MAX_NAME - 1 - TEMP_SUFFIX.len();
    let chars: Vec<char> = name.chars().collect();
    let kept: String = if chars.len() > room {
        chars[chars.len() - room..].iter().collect()
    } else {
        name.to_string()
    };
    format!(".{kept}{TEMP_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_book_in_documents() {
        let p = parse_device_path("documents/Field Station.kfx").unwrap();
        assert_eq!(p.folders, vec!["documents".to_string()]);
        assert_eq!(p.name, "Field Station.kfx");
        assert_eq!(p.display(), "documents/Field Station.kfx");
    }

    #[test]
    fn a_book_at_the_root_and_one_two_folders_down() {
        assert_eq!(
            parse_device_path("a.kfx").unwrap().folders,
            Vec::<String>::new()
        );
        assert_eq!(
            parse_device_path("documents/mybooks/a.kfx")
                .unwrap()
                .folders,
            vec!["documents", "mybooks"]
        );
    }

    #[test]
    fn paths_that_mean_something_else_on_a_disk_are_refused() {
        for raw in [
            "/documents/a.kfx",
            "documents/../a.kfx",
            "./a.kfx",
            "documents//a.kfx",
            "documents/",
            "",
            "documents\\a.kfx",
            "documents/.a.kfx.method-reader-part",
        ] {
            let err = parse_device_path(raw).unwrap_err();
            assert_eq!(err.code, Code::BadPath, "{raw}");
        }
        let long = "x".repeat(MAX_NAME + 1);
        assert_eq!(
            parse_device_path(&format!("documents/{long}"))
                .unwrap_err()
                .code,
            Code::BadPath
        );
    }

    #[test]
    fn the_temp_name_hides_the_book_and_stays_within_the_limit() {
        assert_eq!(temp_name("Title.kfx"), ".Title.kfx.method-reader-part");
        let long = "y".repeat(MAX_NAME);
        let t = temp_name(&long);
        assert_eq!(t.chars().count(), MAX_NAME);
        assert!(t.starts_with('.') && t.ends_with(TEMP_SUFFIX));
        // Deterministic, so a leftover is found again.
        assert_eq!(temp_name(&long), t);
    }

    #[test]
    fn a_multibyte_name_is_measured_in_characters() {
        let name = "é".repeat(MAX_NAME);
        assert!(parse_device_path(&name).is_ok());
        assert_eq!(temp_name(&name).chars().count(), MAX_NAME);
    }
}
