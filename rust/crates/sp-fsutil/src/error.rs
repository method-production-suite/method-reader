//! A file error in the words the TypeScript engine reported it.
//!
//! Node and Bun say `CODE: what happened, syscall 'from' -> 'to'`, and those
//! words reach a person as they are: a send's failure line passes the copy's
//! message through (cli-devices.ts), and `plainReason` and `isWriteRefusal`
//! in cli-errors.ts switch on the code. So a Rust failure carries the same
//! code and the same sentence: libuv's code name and its `uv_strerror` text.

use std::fmt;
use std::io;
use std::path::Path;

/// A failed file operation, spelled the way Node spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIoError {
    /// libuv's code name (`ENOENT`, `EISDIR`, ...), or None for an error
    /// that carries none.
    pub code: Option<String>,
    pub message: String,
}

impl fmt::Display for NodeIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NodeIoError {}

impl NodeIoError {
    /// `err` from `syscall` on `path` (and `dest`, for a copy or a rename),
    /// as Node words it. `fsync` names no path, as Node's does not.
    pub fn from_io(
        err: &io::Error,
        syscall: &str,
        path: Option<&Path>,
        dest: Option<&Path>,
    ) -> NodeIoError {
        let found = match posix_code_for(err, syscall, path, dest) {
            Some(code) => named(code),
            None => code_of(err),
        };
        let Some((code, words)) = found else {
            return NodeIoError {
                code: None,
                message: err.to_string(),
            };
        };
        let mut message = format!("{code}: {words}, {syscall}");
        if let Some(p) = path {
            message.push_str(&format!(" '{}'", p.display()));
        }
        if let Some(d) = dest {
            message.push_str(&format!(" -> '{}'", d.display()));
        }
        NodeIoError {
            code: Some(code.to_string()),
            message,
        }
    }
}

/// The code a POSIX system gives for a failure Windows words differently,
/// so the reason a person reads is the same on every system:
///
/// - Windows refuses to open a folder as a file, or to rename or copy a
///   file over a folder, with ACCESS_DENIED, which reads as "permission
///   denied" when the truth is "that is a folder, not a file" (EISDIR).
/// - Making a folder whose parent is a FILE fails on Windows as
///   ALREADY_EXISTS, naming a path that does not exist; a POSIX system says
///   ENOTDIR ("part of that path is not a folder").
///
/// `None` when the system's own code is already the right one.
pub fn posix_code_for(
    err: &io::Error,
    syscall: &str,
    path: Option<&Path>,
    dest: Option<&Path>,
) -> Option<&'static str> {
    const ACCESS_DENIED: i32 = 5;
    if cfg!(windows) && err.raw_os_error() == Some(ACCESS_DENIED) {
        let target = dest.or(path)?;
        return target.is_dir().then_some("EISDIR");
    }
    if syscall == "mkdir" && err.kind() == io::ErrorKind::AlreadyExists {
        let made = path?;
        // Something at the name itself is EEXIST, as everywhere; nothing
        // there means a parent on the way is a file.
        return std::fs::symlink_metadata(made)
            .is_err()
            .then_some("ENOTDIR");
    }
    None
}

/// libuv's name and sentence for an error, by its OS number where there is
/// one, else by its kind.
pub fn code_of(err: &io::Error) -> Option<(&'static str, &'static str)> {
    if let Some(raw) = err.raw_os_error()
        && let Some(found) = by_os_number(raw)
    {
        return Some(found);
    }
    by_kind(err.kind())
}

/// The words libuv's `uv_strerror` gives each code.
pub fn words_for(code: &str) -> Option<&'static str> {
    WORDS.iter().find(|(c, _)| *c == code).map(|(_, w)| *w)
}

const WORDS: &[(&str, &str)] = &[
    ("EACCES", "permission denied"),
    ("EPERM", "operation not permitted"),
    ("ENOENT", "no such file or directory"),
    ("EEXIST", "file already exists"),
    ("EISDIR", "illegal operation on a directory"),
    ("ENOTDIR", "not a directory"),
    ("ENOSPC", "no space left on device"),
    ("EROFS", "read-only file system"),
    ("EIO", "i/o error"),
    ("EBUSY", "resource busy or locked"),
    ("ENOTEMPTY", "directory not empty"),
    ("ENAMETOOLONG", "name too long"),
    ("EXDEV", "cross-device link not permitted"),
    ("ENOTSUP", "operation not supported on socket"),
    ("EINVAL", "invalid argument"),
    ("EMFILE", "too many open files"),
    ("EAGAIN", "resource temporarily unavailable"),
    ("EFBIG", "file too large"),
    ("ELOOP", "too many symbolic links encountered"),
    ("ENXIO", "no such device or address"),
    ("ENODEV", "no such device"),
    ("ETIMEDOUT", "connection timed out"),
    ("EFAULT", "bad address in system call argument"),
    ("EBADF", "bad file descriptor"),
    ("EINTR", "interrupted system call"),
    ("EDQUOT", "disk quota exceeded"),
    ("ECANCELED", "operation canceled"),
];

fn named(code: &'static str) -> Option<(&'static str, &'static str)> {
    words_for(code).map(|w| (code, w))
}

#[cfg(unix)]
fn by_os_number(raw: i32) -> Option<(&'static str, &'static str)> {
    let code = match raw {
        libc::EACCES => "EACCES",
        libc::EPERM => "EPERM",
        libc::ENOENT => "ENOENT",
        libc::EEXIST => "EEXIST",
        libc::EISDIR => "EISDIR",
        libc::ENOTDIR => "ENOTDIR",
        libc::ENOSPC => "ENOSPC",
        libc::EROFS => "EROFS",
        libc::EIO => "EIO",
        libc::EBUSY => "EBUSY",
        libc::ENOTEMPTY => "ENOTEMPTY",
        libc::ENAMETOOLONG => "ENAMETOOLONG",
        libc::EXDEV => "EXDEV",
        libc::ENOTSUP => "ENOTSUP",
        libc::EINVAL => "EINVAL",
        libc::EMFILE => "EMFILE",
        libc::EAGAIN => "EAGAIN",
        libc::EFBIG => "EFBIG",
        libc::ELOOP => "ELOOP",
        libc::ENXIO => "ENXIO",
        libc::ENODEV => "ENODEV",
        libc::ETIMEDOUT => "ETIMEDOUT",
        libc::EFAULT => "EFAULT",
        libc::EBADF => "EBADF",
        libc::EINTR => "EINTR",
        libc::EDQUOT => "EDQUOT",
        libc::ECANCELED => "ECANCELED",
        _ => return None,
    };
    named(code)
}

/// libuv's uv_translate_sys_error, for the Windows errors a file copy meets.
#[cfg(windows)]
fn by_os_number(raw: i32) -> Option<(&'static str, &'static str)> {
    let code = match raw {
        2 | 3 | 15 | 53 | 67 | 123 | 161 => "ENOENT", // FILE/PATH_NOT_FOUND, INVALID_DRIVE, BAD_NETPATH, BAD_NET_NAME, INVALID_NAME, BAD_PATHNAME
        5 => "EPERM",                                 // ACCESS_DENIED
        19 => "EROFS",                                // WRITE_PROTECT
        21 => "EAGAIN",                               // NOT_READY
        32 | 33 | 170 => "EBUSY",                     // SHARING_VIOLATION, LOCK_VIOLATION, BUSY
        39 | 112 | 1100 => "ENOSPC",                  // HANDLE_DISK_FULL, DISK_FULL, END_OF_MEDIA
        80 | 183 => "EEXIST",                         // FILE_EXISTS, ALREADY_EXISTS
        145 => "ENOTEMPTY",                           // DIR_NOT_EMPTY
        206 => "ENAMETOOLONG",                        // FILENAME_EXCED_RANGE
        267 => "ENOTDIR",                             // DIRECTORY
        17 => "EXDEV",                                // NOT_SAME_DEVICE
        23 | 1117 => "EIO",                           // CRC, IO_DEVICE
        87 => "EINVAL",                               // INVALID_PARAMETER
        _ => return None,
    };
    named(code)
}

#[cfg(not(any(unix, windows)))]
fn by_os_number(_raw: i32) -> Option<(&'static str, &'static str)> {
    None
}

fn by_kind(kind: io::ErrorKind) -> Option<(&'static str, &'static str)> {
    use io::ErrorKind as K;
    let code = match kind {
        K::NotFound => "ENOENT",
        K::PermissionDenied => "EACCES",
        K::AlreadyExists => "EEXIST",
        K::IsADirectory => "EISDIR",
        K::NotADirectory => "ENOTDIR",
        K::StorageFull => "ENOSPC",
        K::ReadOnlyFilesystem => "EROFS",
        K::DirectoryNotEmpty => "ENOTEMPTY",
        K::InvalidFilename => "ENAMETOOLONG",
        K::CrossesDevices => "EXDEV",
        K::ResourceBusy => "EBUSY",
        K::TimedOut => "ETIMEDOUT",
        K::Unsupported => "ENOTSUP",
        K::InvalidInput => "EINVAL",
        _ => return None,
    };
    named(code)
}
