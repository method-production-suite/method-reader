//! A Windows path as a person writes it, not as `canonicalize` returns it.
//!
//! On Windows, `std::fs::canonicalize` answers in the verbatim form,
//! `\\?\C:\Users\...` or `\\?\UNC\server\share\...`. That form reaches a
//! person wherever a canonical path is printed (the MCP server's answers,
//! a library verb's report), and many programs it is pasted into refuse
//! it. [`plain`] gives the ordinary form back, but only where the ordinary
//! form names the same file: a verbatim path may hold what the ordinary
//! form cannot (a name ending in a dot or a space, a reserved device name
//! such as `CON`, a character Windows forbids, more than 259 characters),
//! and then it is kept as it is.
//!
//! Text, not [`std::path::Path::components`], so it reads a Windows path on
//! every system and is tested on every one; [`plain_path`] applies it on
//! Windows only.

use std::path::{Path, PathBuf};

/// The longest path the ordinary form may spell (MAX_PATH less the NUL).
const MAX_PLAIN: usize = 259;

/// `s` without its verbatim prefix when the ordinary form names the same
/// file, or `s` itself.
pub fn plain(s: &str) -> String {
    plain_of(s).unwrap_or_else(|| s.to_owned())
}

fn plain_of(s: &str) -> Option<String> {
    let rest = s.strip_prefix(r"\\?\")?;
    let (out, names) = if rest
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case(r"UNC\"))
    {
        let share = &rest[4..];
        (format!(r"\\{share}"), share)
    } else {
        let b = rest.as_bytes();
        let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
        if !drive {
            return None;
        }
        (rest.to_owned(), &rest[3..])
    };
    if out.chars().count() > MAX_PLAIN {
        return None;
    }
    let mut parts: Vec<&str> = names.split('\\').collect();
    // A trailing separator leaves one empty name; it means the folder.
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if names.is_empty() {
        parts.clear();
    }
    parts.iter().all(|n| ordinary(n)).then_some(out)
}

/// A name the ordinary form reads as the same name.
fn ordinary(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    if name
        .chars()
        .any(|c| (c as u32) < 32 || matches!(c, '<' | '>' | ':' | '"' | '/' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit());
    !reserved
}

/// [`plain`] for a path, on Windows; elsewhere the path as it is.
pub fn plain_path(p: &Path) -> PathBuf {
    if cfg!(windows)
        && let Some(s) = p.to_str()
    {
        return PathBuf::from(plain(s));
    }
    p.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn a_drive_path_and_a_share_lose_the_prefix() {
        for (from, to) in [
            (
                r"\\?\C:\Users\ann\Field Station.pdf",
                r"C:\Users\ann\Field Station.pdf",
            ),
            (r"\\?\d:\scripts\", r"d:\scripts\"),
            (r"\\?\C:\", r"C:\"),
            (r"\\?\UNC\shed\books\Wren", r"\\shed\books\Wren"),
            (r"\\?\unc\shed\books", r"\\shed\books"),
        ] {
            assert_eq!(plain(from), to, "{from}");
        }
    }

    #[test]
    fn what_the_ordinary_form_would_read_otherwise_keeps_it() {
        let long = format!(r"\\?\C:\{}", "a".repeat(260));
        for kept in [
            r"\\?\C:\Users\ann\notes.",
            r"\\?\C:\Users\ann\trailing ",
            r"\\?\C:\Users\CON\x.pdf",
            r"\\?\C:\Users\com1.txt",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"\\?\C:\a\\b",
            r"\\?\C:\a?b",
            r"\\?\Volume{0000}\Wren",
            r"\\?\GLOBALROOT\Device\x",
            long.as_str(),
        ] {
            assert_eq!(plain(kept), kept, "{kept}");
        }
    }

    #[test]
    fn a_path_with_no_prefix_is_left_alone() {
        for p in [
            r"C:\Users\ann",
            r"\\shed\books",
            "/Users/ann/Field Station.pdf",
            "",
        ] {
            assert_eq!(plain(p), p);
        }
    }

    #[test]
    fn a_name_merely_starting_like_a_device_is_ordinary() {
        assert_eq!(
            plain(r"\\?\C:\CONTRACTS\com10.pdf"),
            r"C:\CONTRACTS\com10.pdf"
        );
        assert_eq!(plain(r"\\?\C:\console\x"), r"C:\console\x");
    }
}
