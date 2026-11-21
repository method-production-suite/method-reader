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

/// The longest path the ordinary form may spell (MAX_PATH less the NUL),
/// in UTF-16 units, which is what Windows counts.
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
    if out.encode_utf16().count() > MAX_PLAIN {
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
    let port = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(|n| {
            let mut c = n.chars();
            // 0 to 9, or the superscripts 1, 2 and 3 Windows also reserves.
            matches!(
                (c.next(), c.next()),
                (Some('0'..='9' | '\u{B9}' | '\u{B2}' | '\u{B3}'), None)
            )
        });
    let reserved = port || matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL");
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
    fn the_length_limit_counts_what_windows_counts_utf16_units() {
        // 130 characters outside the Basic Multilingual Plane: 130 chars,
        // but 260 UTF-16 units, past MAX_PATH. The ordinary form cannot
        // spell it, so the verbatim one stays.
        let wide = format!(r"\\?\C:\{}", "\u{1D11E}".repeat(130));
        assert_eq!(plain(&wide), wide);
        // The same count of characters inside the BMP fits.
        let narrow = format!(r"\\?\C:\{}", "\u{00E9}".repeat(130));
        assert_eq!(plain(&narrow), &narrow[4..]);
    }

    #[test]
    fn the_superscript_ports_are_reserved_too() {
        // Windows reserves COM and LPT with 0 to 9 and with the Latin-1
        // superscripts 1, 2 and 3, whatever follows a dot.
        for kept in [
            "\\\\?\\C:\\Users\\COM\u{00B9}",
            "\\\\?\\C:\\Users\\com\u{00B2}.txt",
            "\\\\?\\C:\\Users\\LPT\u{00B3}.pdf",
            "\\\\?\\C:\\Users\\lpt0",
        ] {
            assert_eq!(plain(kept), kept, "{kept}");
        }
        // Any other superscript is an ordinary name.
        assert_eq!(plain("\\\\?\\C:\\COM\u{2074}.txt"), "C:\\COM\u{2074}.txt");
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
