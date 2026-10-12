//! Paths the way `node:path` writes them.
//!
//! The TypeScript builds every path the engine reports (the settings file,
//! the library folder, a book's siblings) with `node:path`, and those paths
//! reach the `--json` answers as strings. `std::path` joins and normalizes
//! differently (it keeps `..`, never resolves, and has no `win32` flavour on
//! a POSIX host), so this module ports the parts of Node's own algorithms
//! the engine uses, for both flavours: `posix` and `win32` are always both
//! available, as `path.posix` and `path.win32` are, and `host` is the one
//! this build runs on, as bare `path` is.
//!
//! Paths are `&str`: a path that is not UTF-8 cannot reach a JSON answer
//! anyway, and the TypeScript holds every path as a JS string.

/// `path.posix`.
pub mod posix {
    /// `normalizeString(path, allowAboveRoot, '/')`.
    pub(crate) fn normalize_string(path: &str, allow_above_root: bool, sep: char) -> String {
        let mut stack: Vec<&str> = Vec::new();
        for seg in path.split(|c| c == sep || (sep == '\\' && c == '/')) {
            match seg {
                "" | "." => {}
                ".." => {
                    if stack.last().is_some_and(|last| *last != "..") {
                        stack.pop();
                    } else if allow_above_root {
                        stack.push("..");
                    }
                }
                other => stack.push(other),
            }
        }
        stack.join(&sep.to_string())
    }

    pub fn is_absolute(path: &str) -> bool {
        path.starts_with('/')
    }

    pub fn normalize(path: &str) -> String {
        if path.is_empty() {
            return ".".into();
        }
        let absolute = is_absolute(path);
        let trailing = path.ends_with('/');
        let mut out = normalize_string(path, !absolute, '/');
        if out.is_empty() {
            if absolute {
                return "/".into();
            }
            return if trailing { "./".into() } else { ".".into() };
        }
        if trailing {
            out.push('/');
        }
        if absolute { format!("/{out}") } else { out }
    }

    pub fn join<S: AsRef<str>>(parts: &[S]) -> String {
        let joined = parts
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if joined.is_empty() {
            return ".".into();
        }
        normalize(&joined)
    }

    /// `path.posix.resolve(...parts)`, against `cwd` when nothing given is
    /// absolute.
    pub fn resolve_from<S: AsRef<str>>(cwd: &str, parts: &[S]) -> String {
        let mut resolved = String::new();
        let mut absolute = false;
        let mut candidates: Vec<&str> = parts.iter().map(AsRef::as_ref).collect();
        candidates.insert(0, cwd);
        for path in candidates.iter().rev() {
            if path.is_empty() {
                continue;
            }
            resolved = format!("{path}/{resolved}");
            absolute = is_absolute(path);
            if absolute {
                break;
            }
        }
        let out = normalize_string(&resolved, !absolute, '/');
        if absolute {
            format!("/{out}")
        } else if out.is_empty() {
            ".".into()
        } else {
            out
        }
    }

    /// `path.posix.resolve(...parts)` against this process's working folder.
    pub fn resolve<S: AsRef<str>>(parts: &[S]) -> String {
        resolve_from(&super::cwd(), parts)
    }

    pub fn dirname(path: &str) -> String {
        if path.is_empty() {
            return ".".into();
        }
        let bytes = path.as_bytes();
        let has_root = bytes[0] == b'/';
        let mut end: Option<usize> = None;
        let mut matched_slash = true;
        for i in (1..bytes.len()).rev() {
            if bytes[i] == b'/' {
                if !matched_slash {
                    end = Some(i);
                    break;
                }
            } else {
                matched_slash = false;
            }
        }
        match end {
            None => {
                if has_root {
                    "/".into()
                } else {
                    ".".into()
                }
            }
            Some(1) if has_root => "//".into(),
            Some(e) => path[..e].into(),
        }
    }

    pub fn basename(path: &str) -> String {
        super::basename_with(path, None, |c| c == '/', 0)
    }

    /// `path.posix.basename(path, ext)`.
    pub fn basename_ext(path: &str, ext: &str) -> String {
        super::basename_with(path, Some(ext), |c| c == '/', 0)
    }

    pub fn extname(path: &str) -> String {
        super::extname_with(path, |c| c == '/', 0)
    }
}

/// `path.win32`, for the parts the engine uses: enough to compute a
/// Windows answer from any host (the settings file and library folder
/// tests check all three systems), and to run on Windows itself.
pub mod win32 {
    fn is_sep(c: char) -> bool {
        c == '/' || c == '\\'
    }

    /// The root of a win32 path: (device, root length, absolute).
    /// `C:\x` is ("C:", 3, true), `C:x` ("C:", 2, false), `\x` ("", 1,
    /// true), `\\server\share\x` ("\\server\share", 15, true).
    fn root(path: &str) -> (String, usize, bool) {
        let chars: Vec<char> = path.chars().collect();
        if chars.is_empty() {
            return (String::new(), 0, false);
        }
        if is_sep(chars[0]) {
            // A UNC root: two separators, a server, a separator, a share.
            if chars.len() > 1 && is_sep(chars[1]) {
                let mut j = 2;
                let server_start = j;
                while j < chars.len() && !is_sep(chars[j]) {
                    j += 1;
                }
                if j < chars.len() && j != server_start {
                    let server: String = chars[server_start..j].iter().collect();
                    while j < chars.len() && is_sep(chars[j]) {
                        j += 1;
                    }
                    let share_start = j;
                    while j < chars.len() && !is_sep(chars[j]) {
                        j += 1;
                    }
                    if j != share_start {
                        let share: String = chars[share_start..j].iter().collect();
                        let device = format!("\\\\{server}\\{share}");
                        let consumed: usize = chars[..j].iter().map(|c| c.len_utf8()).sum();
                        return (device, consumed, true);
                    }
                }
            }
            return (String::new(), 1, true);
        }
        if chars.len() >= 2 && chars[0].is_ascii_alphabetic() && chars[1] == ':' {
            let device: String = chars[..2].iter().collect();
            if chars.len() > 2 && is_sep(chars[2]) {
                return (device, 3, true);
            }
            return (device, 2, false);
        }
        (String::new(), 0, false)
    }

    pub fn is_absolute(path: &str) -> bool {
        let chars: Vec<char> = path.chars().take(3).collect();
        match chars.as_slice() {
            [] => false,
            [c, ..] if is_sep(*c) => true,
            [d, ':', s] => d.is_ascii_alphabetic() && is_sep(*s),
            _ => false,
        }
    }

    pub fn normalize(path: &str) -> String {
        if path.is_empty() {
            return ".".into();
        }
        let (device, root_len, absolute) = root(path);
        let rest = &path[root_len..];
        let mut tail = super::posix::normalize_string(rest, !absolute, '\\');
        if tail.is_empty() && !absolute {
            tail = ".".into();
        }
        if !tail.is_empty() && path.ends_with(is_sep) {
            tail.push('\\');
        }
        if absolute {
            format!("{device}\\{tail}")
        } else {
            format!("{device}{tail}")
        }
    }

    pub fn join<S: AsRef<str>>(parts: &[S]) -> String {
        let joined = parts
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\\");
        if joined.is_empty() {
            return ".".into();
        }
        normalize(&joined)
    }

    /// `path.win32.resolve(...parts)`, against `cwd` when nothing given is
    /// absolute.
    pub fn resolve_from<S: AsRef<str>>(cwd: &str, parts: &[S]) -> String {
        let mut device = String::new();
        let mut tail = String::new();
        let mut absolute = false;
        let mut candidates: Vec<&str> = parts.iter().map(AsRef::as_ref).collect();
        candidates.insert(0, cwd);
        for path in candidates.iter().rev() {
            if path.is_empty() {
                continue;
            }
            let (this_device, root_len, this_absolute) = root(path);
            if !this_device.is_empty()
                && !device.is_empty()
                && !this_device.eq_ignore_ascii_case(&device)
            {
                continue;
            }
            if device.is_empty() {
                device = this_device;
            }
            if !absolute {
                tail = format!("{}\\{tail}", &path[root_len..]);
                absolute = this_absolute;
            }
            if absolute && !device.is_empty() {
                break;
            }
        }
        let tail = super::posix::normalize_string(&tail, !absolute, '\\');
        let out = if absolute {
            format!("{device}\\{tail}")
        } else {
            format!("{device}{tail}")
        };
        if out.is_empty() { ".".into() } else { out }
    }

    pub fn resolve<S: AsRef<str>>(parts: &[S]) -> String {
        resolve_from(&super::cwd(), parts)
    }

    pub fn dirname(path: &str) -> String {
        let (_, root_len, _) = root(path);
        let chars: Vec<(usize, char)> = path.char_indices().collect();
        let mut end: Option<usize> = None;
        let mut matched_slash = true;
        for &(i, c) in chars.iter().rev() {
            if i < root_len.max(1) {
                break;
            }
            if is_sep(c) {
                if !matched_slash {
                    end = Some(i);
                    break;
                }
            } else {
                matched_slash = false;
            }
        }
        match end {
            Some(e) => path[..e].into(),
            None if root_len > 0 => path[..root_len].into(),
            None => ".".into(),
        }
    }

    pub fn basename(path: &str) -> String {
        let (_, root_len, _) = root(path);
        let start = if root_len == 2 { 2 } else { 0 };
        super::basename_with(path, None, is_sep, start)
    }

    pub fn basename_ext(path: &str, ext: &str) -> String {
        let (_, root_len, _) = root(path);
        let start = if root_len == 2 { 2 } else { 0 };
        super::basename_with(path, Some(ext), is_sep, start)
    }

    pub fn extname(path: &str) -> String {
        let (_, root_len, _) = root(path);
        let start = if root_len == 2 { 2 } else { 0 };
        super::extname_with(path, is_sep, start)
    }
}

/// Bare `path`: the flavour of the system this build runs on.
#[cfg(not(windows))]
pub use posix as host;
#[cfg(windows)]
pub use win32 as host;

/// `process.cwd()`, as a string. Falls back to `/` (what a Finder launch
/// gives) when the working folder cannot be read.
pub fn cwd() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_else(|| "/".into())
}

/// Node's `basename` (both flavours), with `start` skipping a drive.
fn basename_with(path: &str, ext: Option<&str>, is_sep: fn(char) -> bool, start0: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();
    let mut start = start0;
    let mut end: isize = -1;
    let mut matched_slash = true;
    if let Some(ext) = ext.filter(|e| !e.is_empty() && e.chars().count() <= len) {
        if ext == path {
            return String::new();
        }
        let ext: Vec<char> = ext.chars().collect();
        let mut ext_idx: isize = ext.len() as isize - 1;
        let mut first_non_slash_end: isize = -1;
        let mut i = len as isize - 1;
        while i >= start0 as isize {
            let c = chars[i as usize];
            if is_sep(c) {
                if !matched_slash {
                    start = i as usize + 1;
                    break;
                }
            } else {
                if first_non_slash_end == -1 {
                    matched_slash = false;
                    first_non_slash_end = i + 1;
                }
                if ext_idx >= 0 {
                    if c == ext[ext_idx as usize] {
                        ext_idx -= 1;
                        if ext_idx == -1 {
                            end = i;
                        }
                    } else {
                        ext_idx = -1;
                        end = first_non_slash_end;
                    }
                }
            }
            i -= 1;
        }
        if start as isize == end {
            end = first_non_slash_end;
        } else if end == -1 {
            end = len as isize;
        }
        if end < start as isize {
            return String::new();
        }
        return chars[start..end as usize].iter().collect();
    }
    let mut i = len as isize - 1;
    while i >= start0 as isize {
        let c = chars[i as usize];
        if is_sep(c) {
            if !matched_slash {
                start = i as usize + 1;
                break;
            }
        } else if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        i -= 1;
    }
    if end == -1 {
        return String::new();
    }
    chars[start..end as usize].iter().collect()
}

/// Node's `extname` (both flavours).
fn extname_with(path: &str, is_sep: fn(char) -> bool, start0: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    let mut start_dot: isize = -1;
    let mut start_part = start0 as isize;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state = 0;
    let mut i = chars.len() as isize - 1;
    while i >= start0 as isize {
        let c = chars[i as usize];
        if is_sep(c) {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            i -= 1;
            continue;
        }
        if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        if c == '.' {
            if start_dot == -1 {
                start_dot = i;
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot != -1 {
            pre_dot_state = -1;
        }
        i -= 1;
    }
    if start_dot == -1
        || end == -1
        || pre_dot_state == 0
        || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == start_part + 1)
    {
        return String::new();
    }
    chars[start_dot as usize..end as usize].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{posix, win32};

    #[test]
    fn posix_matches_node() {
        assert_eq!(posix::join(&["/a/b", "../c", "d/"]), "/a/c/d/");
        assert_eq!(posix::join(&["", ""]), ".");
        assert_eq!(posix::normalize("a/../.."), "..");
        assert_eq!(posix::resolve_from("/cwd", &["x", "../y"]), "/cwd/y");
        assert_eq!(posix::resolve_from("/cwd", &["/abs/", "z"]), "/abs/z");
        assert_eq!(posix::resolve_from("/cwd", &["/"]), "/");
        assert_eq!(posix::dirname("/a/b/"), "/a");
        assert_eq!(posix::dirname("a"), ".");
        assert_eq!(posix::dirname("/a"), "/");
        assert_eq!(posix::basename("/a/b.txt/"), "b.txt");
        assert_eq!(posix::basename_ext("/a/b.txt", ".txt"), "b");
        assert_eq!(posix::basename_ext(".txt", ".txt"), "");
        assert_eq!(posix::extname(".hidden"), "");
        assert_eq!(posix::extname("a."), ".");
        assert_eq!(posix::extname(".."), "");
        assert_eq!(posix::extname("..."), ".");
        assert_eq!(posix::extname(".a.b"), ".b");
        assert_eq!(posix::extname("/x.y/z"), "");
    }

    #[test]
    fn win32_matches_node() {
        assert!(win32::is_absolute("C:\\Users\\Ada"));
        assert!(win32::is_absolute("\\Users"));
        assert!(!win32::is_absolute("C:Users"));
        assert!(!win32::is_absolute("Users"));
        assert_eq!(
            win32::join(&["C:\\Users\\Ada", "Documents", "Screepub"]),
            "C:\\Users\\Ada\\Documents\\Screepub"
        );
        assert_eq!(win32::normalize("C:/a//b/../c"), "C:\\a\\c");
        assert_eq!(win32::resolve_from("/cwd", &["C:\\a\\..\\b"]), "C:\\b");
        assert_eq!(win32::basename("C:\\a\\b.epub"), "b.epub");
        assert_eq!(win32::dirname("C:\\a\\b.epub"), "C:\\a");
        assert_eq!(win32::dirname("C:\\a"), "C:\\");
        assert_eq!(win32::extname("C:\\a\\b.epub"), ".epub");
    }
}
