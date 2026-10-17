//! BETA, and off for users until hardware day (see [`mtp_enabled`]).
//!
//! Kindles that connect over MTP (every Kindle sold since October 2024),
//! reached through the MTP helper `method-reader-mtp` (crate sp-mtp). Ported from
//! branch mtp-kindle's src/device/mtp.ts. The helper prints one JSON object
//! per run; this module finds it, runs it, and reads that object. It
//! decides nothing about books or formats.
//!
//! Listing is best-effort by design: a missing helper, a crash, a timeout
//! or output that is not the contract is "no MTP Kindles", never an error.
//! A copy is the opposite: every failure comes back with the helper's own
//! code and sentence, for `send` to report.

use crate::process::{Platform, Runner, SpawnRunner};
use crate::types::{ConnectedDevice, DeviceKind, Transport};
use serde_json::{Map, Value};
use sp_fsutil::nodepath::host;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The helper's file name, without `.exe`. sp-mtp's `[[bin]]` name.
pub const MTP_HELPER_NAME: &str = "method-reader-mtp";

/// Whether MTP Kindles have shipped. They have not: the route stays off for
/// users until a 2024-or-newer Kindle has been tried (the hardware-day
/// checklist on branch mtp-kindle). Until then MTP is on only when
/// `METHOD_READER_MTP_HELPER` names a helper, which is how hardware day turns it
/// on and how no user ever meets it.
pub const MTP_SHIPPED: bool = false;

/// How long `list --kindles` may take. It reads USB descriptors and opens
/// nothing; three seconds is for a slow system, and keeps the two-second
/// poll from piling up behind a stuck one.
pub const MTP_LIST_TIMEOUT: Duration = Duration::from_secs(3);

/// How long one copy may take, all told: a ceiling, not an estimate.
pub const MTP_PUT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Whether the engine looks for MTP Kindles at all: when they have shipped,
/// or when `METHOD_READER_MTP_HELPER` is set (hardware day, and the tests).
pub fn mtp_enabled(env: &dyn Fn(&str) -> Option<String>) -> bool {
    MTP_SHIPPED || env("METHOD_READER_MTP_HELPER").is_some_and(|v| !v.trim().is_empty())
}

/// Where to look for the helper.
pub struct HelperLocation<'a> {
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The running program. The bundled app keeps every binary together.
    pub exec_path: PathBuf,
    pub platform: Platform,
    pub exists: &'a dyn Fn(&str) -> bool,
    /// The workspace's target folder, for a dev checkout's own build.
    pub dev_target_dir: Option<PathBuf>,
}

/// The process's own environment, as the `env` seams take it.
pub fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

impl HelperLocation<'static> {
    /// This process, this machine.
    pub fn host() -> HelperLocation<'static> {
        HelperLocation {
            env: &process_env,
            exec_path: std::env::current_exe().unwrap_or_default(),
            platform: Platform::host(),
            exists: &file_exists,
            dev_target_dir: dev_target_dir(),
        }
    }
}

/// A dev checkout's release folder, in a debug build only: the path is the
/// build machine's, baked in at compile time, and a shipped (release) build
/// must never run a helper from it (on a Windows runner it is D:\a\..., a
/// path a removable drive on someone's machine could carry).
#[cfg(debug_assertions)]
fn dev_target_dir() -> Option<PathBuf> {
    Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("release"),
    )
}

#[cfg(not(debug_assertions))]
fn dev_target_dir() -> Option<PathBuf> {
    None
}

/// Where the helper is, or None when there is none to run.
///
/// 1. `METHOD_READER_MTP_HELPER`, when set: hardware day, tests, and the test
///    guard. Used only if it is there, so a guard under /dev/null/ means none.
/// 2. Beside the running program: the bundled app.
/// 3. A dev checkout's own release build (`cargo build --release`).
pub fn mtp_helper_path(loc: &HelperLocation) -> Option<String> {
    let file = format!(
        "{MTP_HELPER_NAME}{}",
        if loc.platform == Platform::Win32 {
            ".exe"
        } else {
            ""
        }
    );
    let over = (loc.env)("METHOD_READER_MTP_HELPER").unwrap_or_default();
    let over = over.trim();
    if !over.is_empty() {
        return (loc.exists)(over).then(|| over.to_string());
    }
    let exec = loc.exec_path.to_string_lossy();
    let dir = if loc.platform == Platform::Win32 {
        sp_fsutil::nodepath::win32::dirname(&exec)
    } else {
        sp_fsutil::nodepath::posix::dirname(&exec)
    };
    let beside = if loc.platform == Platform::Win32 {
        sp_fsutil::nodepath::win32::join(&[&dir, &file])
    } else {
        sp_fsutil::nodepath::posix::join(&[&dir, &file])
    };
    if (loc.exists)(&beside) {
        return Some(beside);
    }
    let dev = match &loc.dev_target_dir {
        Some(dir) => host::join(&[dir.to_string_lossy().as_ref(), file.as_str()]),
        None => return None,
    };
    if (loc.exists)(&dev) {
        return Some(dev);
    }
    None
}

/// The single JSON object a helper run printed, or None.
fn sole_object(stdout: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(stdout.trim()) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

/// `list --kindles`'s answer as devices: Kindles only, and only those with
/// a serial, because a serial is what a copy is addressed by. Anything that
/// is not the contract is no devices.
pub fn parse_mtp_list(stdout: &str) -> Vec<ConnectedDevice> {
    let Some(answer) = sole_object(stdout) else {
        return Vec::new();
    };
    if answer.get("ok") != Some(&Value::Bool(true)) {
        return Vec::new();
    }
    let Some(Value::Array(entries)) = answer.get("devices") else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut devices = Vec::new();
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        if entry.get("kindle") != Some(&Value::Bool(true)) {
            continue;
        }
        let Some(serial) = text(entry.get("serial")) else {
            continue;
        };
        let serial = serial.trim().to_string();
        if !seen.insert(serial.clone()) {
            continue;
        }
        let name = text(entry.get("model"))
            .map(|m| m.trim().to_string())
            .unwrap_or_else(|| DeviceKind::Kindle.display_name().into());
        devices.push(ConnectedDevice {
            kind: DeviceKind::Kindle,
            name,
            volume: None,
            transport: Some(Transport::Mtp),
            serial: Some(serial),
        });
    }
    devices
}

/// Every MTP Kindle plugged in now, through `helper` (None: none found).
/// Never fails.
pub fn list_mtp_kindles(
    helper: Option<&str>,
    run: &dyn Runner,
    timeout: Duration,
) -> Vec<ConnectedDevice> {
    let Some(helper) = helper else {
        return Vec::new();
    };
    let argv = [helper.to_string(), "list".into(), "--kindles".into()];
    match run.run(&argv, timeout) {
        Ok(out) if out.code == Some(0) => parse_mtp_list(&out.stdout),
        _ => Vec::new(),
    }
}

/// The real listing: the helper this machine has, run for real.
pub fn list_mtp_kindles_here() -> Vec<ConnectedDevice> {
    list_mtp_kindles(
        mtp_helper_path(&HelperLocation::host()).as_deref(),
        &SpawnRunner,
        MTP_LIST_TIMEOUT,
    )
}

/// Where a book goes on an MTP Kindle: `documents/`, as on a mounted one.
/// The helper makes the folder if it is missing.
pub fn kindle_device_path(file: &str) -> String {
    format!("documents/{}", host::basename(file))
}

/// Which way `put` replaced the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutMethod {
    Rename,
    Replace,
}

/// `put`'s answer. A failure carries the helper's code and words; `holder`
/// is set on `busy` when macOS named the program holding the Kindle. Codes
/// the engine adds itself: `helper-missing`, `helper-failed` (no answer in
/// the contract), `timeout`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtpPutAnswer {
    Done {
        path: String,
        bytes: u64,
        method: PutMethod,
    },
    Failed {
        code: String,
        message: String,
        holder: Option<String>,
    },
}

/// Read `put`'s answer.
pub fn parse_put_answer(stdout: &str) -> MtpPutAnswer {
    let answer = sole_object(stdout);
    if let Some(a) = &answer
        && a.get("ok") == Some(&Value::Bool(true))
        && let Some(path) = text(a.get("path"))
    {
        let method = if a.get("method").and_then(Value::as_str) == Some("replace") {
            PutMethod::Replace
        } else {
            PutMethod::Rename
        };
        let bytes = a
            .get("bytes")
            .and_then(Value::as_f64)
            .map(|b| b as u64)
            .unwrap_or(0);
        return MtpPutAnswer::Done {
            path: path.to_string(),
            bytes,
            method,
        };
    }
    if let Some(a) = &answer
        && a.get("ok") == Some(&Value::Bool(false))
        && let Some(Value::Object(error)) = a.get("error")
        && let (Some(code), Some(message)) = (text(error.get("code")), text(error.get("message")))
    {
        return MtpPutAnswer::Failed {
            code: code.into(),
            message: message.into(),
            holder: text(error.get("holder")).map(str::to_string),
        };
    }
    MtpPutAnswer::Failed {
        code: "helper-failed".into(),
        message: "the MTP helper stopped without saying what happened".into(),
        holder: None,
    }
}

/// Copy one file to one MTP Kindle. Never fails: a failure is an answer.
/// `wait` is how long the helper keeps retrying while something else holds
/// the Kindle (the camera-import daemon letting go after a pause).
pub fn put_to_mtp_kindle(
    helper: Option<&str>,
    run: &dyn Runner,
    serial: &str,
    file: &str,
    wait: Option<Duration>,
    timeout: Duration,
) -> MtpPutAnswer {
    let Some(helper) = helper else {
        return MtpPutAnswer::Failed {
            code: "helper-missing".into(),
            message: "Method Reader's MTP helper is not installed beside the engine, so it cannot copy to this Kindle".into(),
            holder: None,
        };
    };
    let mut argv = vec![
        helper.to_string(),
        "put".into(),
        serial.into(),
        file.into(),
        kindle_device_path(file),
    ];
    if let Some(wait) = wait.filter(|w| !w.is_zero()) {
        argv.push("--wait-ms".into());
        argv.push(wait.as_millis().to_string());
    }
    match run.run(&argv, timeout) {
        Ok(out) if out.code.is_none() => MtpPutAnswer::Failed {
            code: "timeout".into(),
            message: "the copy to the Kindle took too long and was stopped".into(),
            holder: None,
        },
        Ok(out) => parse_put_answer(&out.stdout),
        Err(err) => MtpPutAnswer::Failed {
            code: "helper-missing".into(),
            message: format!("Method Reader's MTP helper could not be started: {err}"),
            holder: None,
        },
    }
}
