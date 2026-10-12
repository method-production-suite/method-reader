//! Node's `path` functions, posix and win32, ported line for line: the one
//! copy every crate uses (sp-engine re-exports it as `sp_engine::nodepath`).
//!
//! The TypeScript engine built every path it printed with `node:path`, and
//! those strings reach the `--json` answers (the settings file, the library
//! folder, a book's siblings, a volume, a send's destination). `std::path`
//! joins without normalizing and has no win32 rules on a Mac, so the two
//! would print different strings for the same input
//! (`join('/a/b/../c', 'K')` is `/a/c/K` in Node). These are Node's
//! algorithms, checked against Bun's answers in tests/node_path.rs. `posix`
//! and `win32` are always both available, as `path.posix` and `path.win32`
//! are, and `host` is the one this build runs on, as bare `path` is.
//!
//! Paths are `&str`: every separator and dot is ASCII, so slicing at the
//! positions found is always on a character boundary.

/// The host's rules, as `node:path` picks them.
pub mod host {
    #[cfg(windows)]
    pub use super::win32::*;

    #[cfg(not(windows))]
    pub use super::posix::*;
}

/// `process.cwd()`, as a string. Falls back to `/` (what a Finder launch
/// gives) when the working folder cannot be read.
pub fn cwd() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_else(|| "/".into())
}

fn normalize_string(
    path: &str,
    allow_above_root: bool,
    separator: char,
    is_sep: fn(u8) -> bool,
) -> String {
    let bytes = path.as_bytes();
    let mut res = String::new();
    let mut last_segment_length: usize = 0;
    let mut last_slash: isize = -1;
    let mut dots: i32 = 0;
    let mut code: u8 = 0;
    let mut i: usize = 0;
    while i <= bytes.len() {
        if i < bytes.len() {
            code = bytes[i];
        } else if is_sep(code) {
            break;
        } else {
            code = b'/';
        }
        if is_sep(code) {
            if last_slash == i as isize - 1 || dots == 1 {
                // Nothing to add.
            } else if dots == 2 {
                let ends_in_dotdot =
                    res.len() >= 2 && last_segment_length == 2 && res.ends_with("..");
                if !ends_in_dotdot {
                    if res.len() > 2 {
                        match res.rfind(separator) {
                            None => {
                                res.clear();
                                last_segment_length = 0;
                            }
                            Some(index) => {
                                res.truncate(index);
                                // Node: res.length - 1 - res.lastIndexOf(sep), where
                                // lastIndexOf is -1 when there is none.
                                last_segment_length = match res.rfind(separator) {
                                    Some(j) => res.len() - 1 - j,
                                    None => res.len(),
                                };
                            }
                        }
                        last_slash = i as isize;
                        dots = 0;
                        i += 1;
                        continue;
                    } else if !res.is_empty() {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = i as isize;
                        dots = 0;
                        i += 1;
                        continue;
                    }
                }
                if allow_above_root {
                    if res.is_empty() {
                        res.push_str("..");
                    } else {
                        res.push(separator);
                        res.push_str("..");
                    }
                    last_segment_length = 2;
                }
            } else {
                let segment = &path[(last_slash + 1) as usize..i];
                if !res.is_empty() {
                    res.push(separator);
                }
                res.push_str(segment);
                last_segment_length = (i as isize - last_slash - 1) as usize;
            }
            last_slash = i as isize;
            dots = 0;
        } else if code == b'.' && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
        i += 1;
    }
    res
}

/// `node:path`'s posix half.
pub mod posix {
    use super::normalize_string;

    fn is_sep(c: u8) -> bool {
        c == b'/'
    }

    pub fn is_absolute(path: &str) -> bool {
        path.starts_with('/')
    }

    pub fn normalize(path: &str) -> String {
        if path.is_empty() {
            return ".".into();
        }
        let absolute = path.starts_with('/');
        let trailing = path.ends_with('/');
        let mut out = normalize_string(path, !absolute, '/', is_sep);
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
        let joined: Vec<&str> = parts
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !p.is_empty())
            .collect();
        if joined.is_empty() {
            return ".".into();
        }
        normalize(&joined.join("/"))
    }

    /// `path.resolve`, with the current folder as Node's `process.cwd()`.
    pub fn resolve<S: AsRef<str>>(parts: &[S]) -> String {
        resolve_from(&super::cwd(), parts)
    }

    /// `path.posix.resolve(...parts)`, against `cwd` when nothing given is
    /// absolute.
    pub fn resolve_from<S: AsRef<str>>(cwd: &str, parts: &[S]) -> String {
        let mut resolved = String::new();
        let mut absolute = false;
        let mut i = parts.len() as isize - 1;
        while i >= -1 && !absolute {
            let path = if i >= 0 {
                parts[i as usize].as_ref()
            } else {
                cwd
            };
            i -= 1;
            if path.is_empty() {
                continue;
            }
            resolved = format!("{path}/{resolved}");
            absolute = path.starts_with('/');
        }
        let out = normalize_string(&resolved, !absolute, '/', is_sep);
        if absolute {
            format!("/{out}")
        } else if out.is_empty() {
            ".".into()
        } else {
            out
        }
    }

    pub fn basename(path: &str) -> String {
        let b = path.as_bytes();
        let mut start = 0usize;
        let mut end: isize = -1;
        let mut matched_slash = true;
        let mut i = b.len() as isize - 1;
        while i >= 0 {
            if b[i as usize] == b'/' {
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
        path[start..end as usize].to_string()
    }

    /// `path.posix.basename(path, ext)`.
    pub fn basename_ext(path: &str, ext: &str) -> String {
        super::basename_ext_from(path, ext, 0, is_sep)
    }

    pub fn dirname(path: &str) -> String {
        let b = path.as_bytes();
        if b.is_empty() {
            return ".".into();
        }
        let has_root = b[0] == b'/';
        let mut end: isize = -1;
        let mut matched_slash = true;
        let mut i = b.len() as isize - 1;
        while i >= 1 {
            if b[i as usize] == b'/' {
                if !matched_slash {
                    end = i;
                    break;
                }
            } else {
                matched_slash = false;
            }
            i -= 1;
        }
        if end == -1 {
            return if has_root { "/".into() } else { ".".into() };
        }
        if has_root && end == 1 {
            return "//".into();
        }
        path[..end as usize].to_string()
    }

    pub fn extname(path: &str) -> String {
        super::extname_from(path, 0, is_sep)
    }
}

/// `node:path`'s win32 half.
pub mod win32 {
    use super::normalize_string;

    fn is_sep(c: u8) -> bool {
        c == b'/' || c == b'\\'
    }

    fn is_device_root(c: u8) -> bool {
        c.is_ascii_alphabetic()
    }

    fn has_drive(b: &[u8]) -> bool {
        b.len() >= 2 && is_device_root(b[0]) && b[1] == b':'
    }

    pub fn is_absolute(path: &str) -> bool {
        let b = path.as_bytes();
        if b.is_empty() {
            return false;
        }
        is_sep(b[0]) || (b.len() > 2 && has_drive(b) && is_sep(b[2]))
    }

    /// The UNC root's two parts and where the root ends, when `path` starts
    /// with one (`\\server\share`). `whole` is true when nothing follows it.
    fn unc(path: &str) -> Option<(String, String, usize, bool)> {
        let b = path.as_bytes();
        let len = b.len();
        let mut j = 2;
        let mut last = j;
        while j < len && !is_sep(b[j]) {
            j += 1;
        }
        if j < len && j != last {
            let first = path[last..j].to_string();
            last = j;
            while j < len && is_sep(b[j]) {
                j += 1;
            }
            if j < len && j != last {
                last = j;
                while j < len && !is_sep(b[j]) {
                    j += 1;
                }
                if j == len {
                    return Some((first, path[last..].to_string(), j, true));
                }
                if j != last {
                    return Some((first, path[last..j].to_string(), j, false));
                }
            }
        }
        None
    }

    pub fn normalize(path: &str) -> String {
        let b = path.as_bytes();
        let len = b.len();
        if len == 0 {
            return ".".into();
        }
        let mut root_end = 0;
        let mut device: Option<String> = None;
        let mut absolute = false;
        if len == 1 {
            return if b[0] == b'/' {
                "\\".into()
            } else {
                path.to_string()
            };
        }
        if is_sep(b[0]) {
            absolute = true;
            if is_sep(b[1]) {
                if let Some((first, second, end, whole)) = unc(path) {
                    if whole {
                        return format!("\\\\{first}\\{second}\\");
                    }
                    device = Some(format!("\\\\{first}\\{second}"));
                    root_end = end;
                }
            } else {
                root_end = 1;
            }
        } else if has_drive(b) {
            device = Some(path[..2].to_string());
            root_end = 2;
            if len > 2 && is_sep(b[2]) {
                absolute = true;
                root_end = 3;
            }
        }
        let mut tail = if root_end < len {
            normalize_string(&path[root_end..], !absolute, '\\', is_sep)
        } else {
            String::new()
        };
        if tail.is_empty() && !absolute {
            tail = ".".into();
        }
        if !tail.is_empty() && is_sep(b[len - 1]) {
            tail.push('\\');
        }
        match device {
            None => {
                if absolute {
                    format!("\\{tail}")
                } else {
                    tail
                }
            }
            Some(d) => {
                if absolute {
                    format!("{d}\\{tail}")
                } else {
                    format!("{d}{tail}")
                }
            }
        }
    }

    pub fn join<S: AsRef<str>>(parts: &[S]) -> String {
        let kept: Vec<&str> = parts
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !p.is_empty())
            .collect();
        let Some(first) = kept.first().copied() else {
            return ".".into();
        };
        let mut joined = kept.join("\\");
        let fb = first.as_bytes();
        let mut needs_replace = true;
        let mut slashes = 0usize;
        if is_sep(fb[0]) {
            slashes += 1;
            if fb.len() > 1 && is_sep(fb[1]) {
                slashes += 1;
                if fb.len() > 2 {
                    if is_sep(fb[2]) {
                        slashes += 1;
                    } else {
                        needs_replace = false;
                    }
                }
            }
        }
        if needs_replace {
            let jb = joined.as_bytes();
            while slashes < jb.len() && is_sep(jb[slashes]) {
                slashes += 1;
            }
            if slashes >= 2 {
                joined = format!("\\{}", &joined[slashes..]);
            }
        }
        normalize(&joined)
    }

    /// `path.win32.resolve`. Off Windows, Node's cwd has no drive, and so
    /// neither does a resolved rooted path without one; `cwd` is the host's.
    pub fn resolve<S: AsRef<str>>(parts: &[S]) -> String {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        resolve_from(&cwd, parts)
    }

    /// `path.win32.resolve(...parts)`, against `cwd` when nothing given is
    /// absolute (and for a drive with no folder of its own in `=X:`).
    pub fn resolve_from<S: AsRef<str>>(cwd: &str, parts: &[S]) -> String {
        let cwd = cwd.to_owned();
        let mut resolved_device = String::new();
        let mut resolved_tail = String::new();
        let mut resolved_absolute = false;
        let mut i = parts.len() as isize - 1;
        while i >= -1 {
            let path: String = if i >= 0 {
                let p = parts[i as usize].as_ref();
                if p.is_empty() {
                    i -= 1;
                    continue;
                }
                p.to_string()
            } else if resolved_device.is_empty() {
                cwd.clone()
            } else {
                let p =
                    std::env::var(format!("={resolved_device}")).unwrap_or_else(|_| cwd.clone());
                let pb = p.as_bytes();
                if p.get(..2).map(|s| s.to_ascii_lowercase())
                    != Some(resolved_device.to_ascii_lowercase())
                    && pb.get(2) == Some(&b'\\')
                {
                    format!("{resolved_device}\\")
                } else {
                    p
                }
            };
            i -= 1;
            let b = path.as_bytes();
            let len = b.len();
            let mut root_end = 0;
            let mut device = String::new();
            let mut absolute = false;
            if len == 1 {
                if is_sep(b[0]) {
                    root_end = 1;
                    absolute = true;
                }
            } else if len > 1 && is_sep(b[0]) {
                absolute = true;
                if is_sep(b[1]) {
                    if let Some((first, second, end, _)) = unc(&path) {
                        device = format!("\\\\{first}\\{second}");
                        root_end = end;
                    }
                } else {
                    root_end = 1;
                }
            } else if has_drive(b) {
                device = path[..2].to_string();
                root_end = 2;
                if len > 2 && is_sep(b[2]) {
                    absolute = true;
                    root_end = 3;
                }
            }
            if !device.is_empty() {
                if !resolved_device.is_empty() {
                    if !device.eq_ignore_ascii_case(&resolved_device) {
                        continue;
                    }
                } else {
                    resolved_device = device;
                }
            }
            if resolved_absolute {
                if !resolved_device.is_empty() {
                    break;
                }
            } else {
                resolved_tail = format!("{}\\{}", &path[root_end..], resolved_tail);
                resolved_absolute = absolute;
                if absolute && !resolved_device.is_empty() {
                    break;
                }
            }
        }
        let tail = normalize_string(&resolved_tail, !resolved_absolute, '\\', is_sep);
        if resolved_absolute {
            format!("{resolved_device}\\{tail}")
        } else {
            let out = format!("{resolved_device}{tail}");
            if out.is_empty() { ".".into() } else { out }
        }
    }

    pub fn basename(path: &str) -> String {
        let b = path.as_bytes();
        let mut start = 0usize;
        let mut end: isize = -1;
        let mut matched_slash = true;
        if has_drive(b) {
            start = 2;
        }
        let mut i = b.len() as isize - 1;
        while i >= start as isize {
            if is_sep(b[i as usize]) {
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
        path[start..end as usize].to_string()
    }

    pub fn dirname(path: &str) -> String {
        let b = path.as_bytes();
        let len = b.len();
        if len == 0 {
            return ".".into();
        }
        if len == 1 {
            return if is_sep(b[0]) {
                path.to_string()
            } else {
                ".".into()
            };
        }
        let mut root_end: isize = -1;
        let mut offset = 0usize;
        if is_sep(b[0]) {
            root_end = 1;
            offset = 1;
            if is_sep(b[1])
                && let Some((_, _, end, whole)) = unc(path)
            {
                if whole {
                    return path.to_string();
                }
                root_end = end as isize + 1;
                offset = end + 1;
            }
        } else if has_drive(b) {
            root_end = if len > 2 && is_sep(b[2]) { 3 } else { 2 };
            offset = root_end as usize;
        }
        let mut end: isize = -1;
        let mut matched_slash = true;
        let mut i = len as isize - 1;
        while i >= offset as isize {
            if is_sep(b[i as usize]) {
                if !matched_slash {
                    end = i;
                    break;
                }
            } else {
                matched_slash = false;
            }
            i -= 1;
        }
        if end == -1 {
            if root_end == -1 {
                return ".".into();
            }
            end = root_end;
        }
        path[..end as usize].to_string()
    }

    /// `path.win32.basename(path, ext)`.
    pub fn basename_ext(path: &str, ext: &str) -> String {
        let start = if has_drive(path.as_bytes()) { 2 } else { 0 };
        super::basename_ext_from(path, ext, start, is_sep)
    }

    pub fn extname(path: &str) -> String {
        let start = if has_drive(path.as_bytes()) { 2 } else { 0 };
        super::extname_from(path, start, is_sep)
    }
}

/// Node's `basename(path, ext)` loop, shared by both halves (win32 starts
/// past a drive). With an empty `ext`, or one longer than the path, it is
/// plain `basename`.
fn basename_ext_from(path: &str, ext: &str, start0: usize, is_sep: fn(u8) -> bool) -> String {
    let b = path.as_bytes();
    let len = b.len();
    if ext.is_empty() || ext.len() > len {
        let mut start = start0;
        let mut end: isize = -1;
        let mut matched_slash = true;
        let mut i = len as isize - 1;
        while i >= start0 as isize {
            if is_sep(b[i as usize]) {
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
        return String::from_utf8_lossy(&b[start..end as usize]).into_owned();
    }
    if ext == path {
        return String::new();
    }
    let e = ext.as_bytes();
    let mut start = start0;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut ext_idx: isize = e.len() as isize - 1;
    let mut first_non_slash_end: isize = -1;
    let mut i = len as isize - 1;
    while i >= start0 as isize {
        let code = b[i as usize];
        if is_sep(code) {
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
                if code == e[ext_idx as usize] {
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
    String::from_utf8_lossy(&b[start..end as usize]).into_owned()
}

/// Node's extname loop, shared by both halves (win32 starts past a drive).
fn extname_from(path: &str, start: usize, is_sep: fn(u8) -> bool) -> String {
    let b = path.as_bytes();
    let mut start_dot: isize = -1;
    let mut start_part = start as isize;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state = 0;
    let mut i = b.len() as isize - 1;
    while i >= start as isize {
        let code = b[i as usize];
        if is_sep(code) {
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
        if code == b'.' {
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
    path[start_dot as usize..end as usize].to_string()
}
