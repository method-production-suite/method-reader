//! The device verbs' share of the CLI's error contract (src/cli-errors.ts).
//! A failure that already knows its code; sp-cli decides how it reaches the
//! user (`{"ok":false,"error":{"code","message"}}` under --json).

use serde::Serialize;
use std::fmt;

/// The contract codes the device verbs answer with. The words are the
/// TypeScript's, and sp-cli's full list must agree with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Usage,
    Unreadable,
    NoDevices,
    AmbiguousDevice,
    UnknownDevice,
    SendFailed,
    UnsupportedFile,
    RevealFailed,
    EjectFailed,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Usage => "usage",
            ErrorCode::Unreadable => "unreadable",
            ErrorCode::NoDevices => "no-devices",
            ErrorCode::AmbiguousDevice => "ambiguous-device",
            ErrorCode::UnknownDevice => "unknown-device",
            ErrorCode::SendFailed => "send-failed",
            ErrorCode::UnsupportedFile => "unsupported-file",
            ErrorCode::RevealFailed => "reveal-failed",
            ErrorCode::EjectFailed => "eject-failed",
        }
    }
}

/// `CliError`: a contract code and the plain-words message. Serializes as
/// the `error` object of a failed --json answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
}

impl CliError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> CliError {
        CliError {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
