//! src/device/paths.ts: the two path predicates every device module needs,
//! in one place because they had drifted (the drive-root regex was written
//! twice with different letter-case handling).

use std::path::Path;

/// A directory, following links; anything unreadable is not one.
pub fn is_directory(path: &str) -> bool {
    std::fs::metadata(Path::new(path))
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// `existsSync`: there, following links.
pub fn exists(path: &str) -> bool {
    std::fs::metadata(Path::new(path)).is_ok()
}

/// A bare Windows drive root, e.g. `D:\` or `d:\`. Case-INSENSITIVE on the
/// letter: Windows accepts either case. Pure string matching, independent
/// of the host, so it is exercised the same way in tests everywhere.
pub fn is_windows_drive_root(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() == 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}
