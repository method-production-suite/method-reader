//! Port of `src/cli-errors.ts`: the CLI's error contract (the codes every
//! `--json` failure carries), the filesystem answers that count as "the
//! disk refused" rather than a bug of ours, and the plain words each is
//! reported in.
//!
//! Also the engine's one error type, [`EngineError`], because the
//! TypeScript's handlers throw three kinds of thing and its callers tell
//! them apart: a [`CliError`] that already knows its code, a filesystem
//! error carrying Node's `code` (`ENOENT`, `EACCES`, ...), and a plain
//! `Error` (sometimes a named subclass, such as `CalibreMissingError`).

use std::fmt;
use std::io;

use serde::Serialize;

/// `JsonError['code']`: every code a `--json` failure can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Scanned,
    NotScreenplay,
    Unreadable,
    Password,
    UnsupportedType,
    Usage,
    BadOptions,
    Internal,
    /// The library folder could not be opened or made.
    Library,
    /// An output we could not write, as opposed to an input we could not read.
    Unwritable,
    NoDevices,
    AmbiguousDevice,
    UnknownDevice,
    SendFailed,
    UnsupportedFile,
    BadSettings,
    ExportFailed,
    KfxInstallFailed,
    RevealFailed,
    RouteUnavailable,
    RouteFailed,
    EjectFailed,
}

impl ErrorCode {
    /// The code as the contract spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Scanned => "scanned",
            ErrorCode::NotScreenplay => "not-screenplay",
            ErrorCode::Unreadable => "unreadable",
            ErrorCode::Password => "password",
            ErrorCode::UnsupportedType => "unsupported-type",
            ErrorCode::Usage => "usage",
            ErrorCode::BadOptions => "bad-options",
            ErrorCode::Internal => "internal",
            ErrorCode::Library => "library",
            ErrorCode::Unwritable => "unwritable",
            ErrorCode::NoDevices => "no-devices",
            ErrorCode::AmbiguousDevice => "ambiguous-device",
            ErrorCode::UnknownDevice => "unknown-device",
            ErrorCode::SendFailed => "send-failed",
            ErrorCode::UnsupportedFile => "unsupported-file",
            ErrorCode::BadSettings => "bad-settings",
            ErrorCode::ExportFailed => "export-failed",
            ErrorCode::KfxInstallFailed => "kfx-install-failed",
            ErrorCode::RevealFailed => "reveal-failed",
            ErrorCode::RouteUnavailable => "route-unavailable",
            ErrorCode::RouteFailed => "route-failed",
            ErrorCode::EjectFailed => "eject-failed",
        }
    }
}

/// `JsonError`: `{ code, message }`, as the CLI prints it under `error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JsonError {
    pub code: ErrorCode,
    pub message: String,
}

/// `CliError`: a failure that already knows its contract code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
}

impl CliError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        CliError {
            code,
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> JsonError {
        JsonError {
            code: self.code,
            message: self.message.clone(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// A filesystem error as Node reports one: its `code` (`ENOENT`) and its
/// message (`ENOENT: no such file or directory, open '/x'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    /// Node's code, or `None` for an error Node would not give one.
    pub code: Option<String>,
    pub message: String,
}

impl FsError {
    /// Node's error for `syscall` on `path` (and `dest`, for the two-path
    /// calls: rename and copyfile).
    pub fn from_io(err: &io::Error, syscall: &str, path: &str, dest: Option<&str>) -> Self {
        match node_code(err) {
            Some(code) => {
                let mut message = format!("{code}: {}, {syscall} '{path}'", describe(code));
                if let Some(dest) = dest {
                    message.push_str(&format!(" -> '{dest}'"));
                }
                FsError {
                    code: Some(code.into()),
                    message,
                }
            }
            None => FsError {
                code: None,
                message: err.to_string(),
            },
        }
    }

    /// An error made up by the engine with a Node code of its own choosing
    /// (`Object.assign(new Error(...), { code: 'ENOTDIR' })`).
    pub fn with_code(code: &str, message: impl Into<String>) -> Self {
        FsError {
            code: Some(code.into()),
            message: message.into(),
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FsError {}

/// libuv's description of a code, as Node's messages carry it.
fn describe(code: &str) -> &'static str {
    match code {
        "ENOENT" => "no such file or directory",
        "EACCES" => "permission denied",
        "EPERM" => "operation not permitted",
        "EEXIST" => "file already exists",
        "ENOTDIR" => "not a directory",
        "EISDIR" => "illegal operation on a directory",
        "ENOSPC" => "no space left on device",
        "EROFS" => "read-only file system",
        "ENAMETOOLONG" => "name too long",
        "ENOTEMPTY" => "directory not empty",
        "EXDEV" => "cross-device link not permitted",
        "EBUSY" => "resource busy or locked",
        "ELOOP" => "too many symbolic links encountered",
        "EINVAL" => "invalid argument",
        "EIO" => "i/o error",
        "EMFILE" => "too many open files",
        _ => "unknown error",
    }
}

/// The Node code for an I/O error, by the error's kind, and by the raw
/// errno where the kind cannot tell two codes apart (EPERM from EACCES).
pub fn node_code(err: &io::Error) -> Option<&'static str> {
    #[cfg(unix)]
    if err.raw_os_error() == Some(1) {
        return Some("EPERM");
    }
    use io::ErrorKind as K;
    Some(match err.kind() {
        K::NotFound => "ENOENT",
        #[cfg(windows)]
        K::PermissionDenied => "EPERM",
        #[cfg(not(windows))]
        K::PermissionDenied => "EACCES",
        K::AlreadyExists => "EEXIST",
        K::NotADirectory => "ENOTDIR",
        K::IsADirectory => "EISDIR",
        K::StorageFull => "ENOSPC",
        K::ReadOnlyFilesystem => "EROFS",
        K::InvalidFilename => "ENAMETOOLONG",
        K::DirectoryNotEmpty => "ENOTEMPTY",
        K::CrossesDevices => "EXDEV",
        K::ResourceBusy => "EBUSY",
        K::InvalidInput => "EINVAL",
        _ => return None,
    })
}

/// Everything an engine call can fail with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// Already knows its contract code.
    Cli(CliError),
    /// The filesystem refused, with Node's code.
    Fs(FsError),
    /// A plain `Error`, or a named subclass of it (`name` is the class's
    /// `name`, `"Error"` for a plain one).
    Named { name: &'static str, message: String },
}

impl EngineError {
    /// `new Error(message)`.
    pub fn plain(message: impl Into<String>) -> Self {
        EngineError::Named {
            name: "Error",
            message: message.into(),
        }
    }

    /// `errorMessage(err)`.
    pub fn message(&self) -> String {
        match self {
            EngineError::Cli(e) => e.message.clone(),
            EngineError::Fs(e) => e.message.clone(),
            EngineError::Named { message, .. } => message.clone(),
        }
    }

    /// `err.code`, for a filesystem error.
    pub fn fs_code(&self) -> Option<&str> {
        match self {
            EngineError::Fs(e) => e.code.as_deref(),
            _ => None,
        }
    }

    /// `err.name`.
    pub fn name(&self) -> &str {
        match self {
            EngineError::Cli(_) => "CliError",
            EngineError::Fs(_) => "Error",
            EngineError::Named { name, .. } => name,
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for EngineError {}

impl From<CliError> for EngineError {
    fn from(e: CliError) -> Self {
        EngineError::Cli(e)
    }
}

impl From<FsError> for EngineError {
    fn from(e: FsError) -> Self {
        EngineError::Fs(e)
    }
}

/// `WRITE_REFUSALS`: a filesystem answer that means "not there, not
/// allowed, or no room", as opposed to a bug in us.
pub const WRITE_REFUSALS: [&str; 8] = [
    "ENOENT", "EACCES", "EPERM", "EEXIST", "ENOSPC", "EROFS", "EISDIR", "ENOTDIR",
];

/// `isWriteRefusal(err)`.
pub fn is_write_refusal(err: &EngineError) -> bool {
    err.fs_code().is_some_and(|c| WRITE_REFUSALS.contains(&c))
}

/// `plainReason(err)`: Node's fs error codes in words a person who has
/// never seen an errno can act on, else the raw message.
pub fn plain_reason(err: &EngineError) -> String {
    match err.fs_code() {
        Some("EACCES" | "EPERM") => "permission denied".into(),
        Some("ENOTDIR") => "part of that path is not a folder".into(),
        Some("EEXIST") => "that is a file, not a folder".into(),
        Some("EISDIR") => "that is a folder, not a file".into(),
        Some("EROFS") => "that location is read-only".into(),
        Some("ENOSPC") => "no space left on the disk".into(),
        Some("ENAMETOOLONG") => "that path is too long".into(),
        _ => err.message(),
    }
}

/// `unwritableError(folder, err, alreadyWritten)`: a failed write of the
/// picture folder as `unwritable`, or `None` for a throw that is not the
/// filesystem refusing.
pub fn unwritable_error(
    folder: &str,
    err: &EngineError,
    already_written: &[String],
) -> Option<JsonError> {
    if !is_write_refusal(err) {
        return None;
    }
    let code = err.fs_code().unwrap_or_default();
    let already = if already_written.is_empty() {
        String::new()
    } else {
        format!(
            " {} {} already written.",
            already_written.join(", "),
            if already_written.len() == 1 {
                "was"
            } else {
                "were"
            }
        )
    };
    Some(JsonError {
        code: ErrorCode::Unwritable,
        message: format!(
            "cannot write the pictures to {folder} ({code}).{already} The .fountain was not updated."
        ),
    })
}

/// `mapConversionError`'s filesystem half: an input that could not be read.
/// (The PDF half, password and invalid-PDF, belongs to the conversion,
/// whose errors are sp-pdf's.)
pub fn map_read_error(err: &EngineError) -> Option<JsonError> {
    match err.fs_code() {
        Some(code @ ("ENOENT" | "EISDIR" | "EACCES")) => Some(JsonError {
            code: ErrorCode::Unreadable,
            message: format!("cannot read the input file ({code})"),
        }),
        _ => None,
    }
}
