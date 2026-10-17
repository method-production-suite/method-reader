//! Port of `src/export/calibre.ts`: finding and running Calibre's
//! command-line tools, for the conversions that still need Calibre (KFX,
//! with Kindle Previewer behind it, and KEPUB). The AZW3 is Method Reader's own.
//!
//! **Every Calibre spawn goes through [`run_with_timeout`], with an explicit
//! environment.** Calibre rewrites files in its settings folder on an
//! ordinary run, so a test must hand it a scratch copy
//! (`CALIBRE_CONFIG_DIRECTORY`); the TypeScript passes `process.env` on
//! purpose for that reason, and here the environment is always a parameter.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use crate::env::{Env, host_platform};
use crate::errors::{EngineError, FsError};
use crate::fsx;
use crate::nodepath::host;

/// Calibre would otherwise undo two Method Reader decisions during the convert:
/// scene-per-page breaks on every h2, and deleting the dialogue column's
/// side margins as "publisher page margins". Device-verified 2026-07-29;
/// change them in one place or not at all.
pub const CALIBRE_FORMAT_GUARDS: [&str; 3] = [
    "--page-breaks-before=/",
    "--chapter-mark=none",
    "--disable-remove-fake-margins",
];

/// `accessSync(path, X_OK)`.
fn is_executable(path: &str) -> bool {
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        Path::new(path).is_file()
    }
}

/// `process.env.PATH` split on the platform's delimiter, each joined to
/// `name`.
fn path_entries(name: &str, env: &Env) -> Vec<String> {
    let delimiter = if cfg!(windows) { ';' } else { ':' };
    env.get("PATH")
        .unwrap_or("")
        .split(delimiter)
        .filter(|d| !d.is_empty())
        .map(|d| host::join(&[d, name]))
        .collect()
}

/// Where each Calibre tool can be, per OS.
fn candidate_paths(name: &str, env: &Env) -> Vec<String> {
    match host_platform() {
        "darwin" => vec![
            format!("/Applications/calibre.app/Contents/MacOS/{name}"),
            format!("/opt/homebrew/bin/{name}"),
            format!("/usr/local/bin/{name}"),
        ],
        "win32" => {
            let exe = format!("{name}.exe");
            let mut out = vec![
                host::join(&[
                    env.get("ProgramFiles").unwrap_or("C:\\Program Files"),
                    "Calibre2",
                    exe.as_str(),
                ]),
                host::join(&[
                    env.get("ProgramFiles(x86)")
                        .unwrap_or("C:\\Program Files (x86)"),
                    "Calibre2",
                    exe.as_str(),
                ]),
            ];
            out.extend(path_entries(&exe, env));
            out
        }
        _ => {
            let mut out = vec![format!("/usr/bin/{name}"), format!("/usr/local/bin/{name}")];
            out.extend(path_entries(name, env));
            out
        }
    }
}

/// `calibreTool(name)`: the first install of the tool that is there and
/// executable. One scanner, so callers never disagree about Calibre.
pub fn calibre_tool(name: &str, env: &Env) -> Option<String> {
    candidate_paths(name, env)
        .into_iter()
        .find(|p| fsx::exists(p) && is_executable(p))
}

/// `isCalibreAvailable()`.
pub fn is_calibre_available(env: &Env) -> bool {
    calibre_tool("ebook-convert", env).is_some()
}

/// `CalibreMissingError`.
pub fn calibre_missing() -> EngineError {
    EngineError::Named {
        name: "CalibreMissingError",
        message: "Calibre's ebook-convert was not found.".into(),
    }
}

/// `CalibreFailedError(detail)`.
pub fn calibre_failed(detail: &str) -> EngineError {
    EngineError::Named {
        name: "CalibreFailedError",
        message: format!("ebook-convert failed: {detail}"),
    }
}

/// How long a conversion may run before it is stopped: ten minutes.
pub const CONVERSION_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// `CalibreTimedOutError(tool, timeoutMs)`: a `CalibreFailedError` with a
/// sentence of its own.
pub fn calibre_timed_out(tool: &str, timeout_ms: u64) -> EngineError {
    EngineError::Named {
        name: "CalibreTimedOutError",
        message: format!(
            "{} was still running after {}, so it was stopped. If Kindle Previewer or Calibre opened a window, close it and try again.",
            host::basename(tool),
            duration_in_words(timeout_ms)
        ),
    }
}

/// Whether an error is a `CalibreFailedError` (a timeout is one too).
pub fn is_calibre_failed(err: &EngineError) -> bool {
    matches!(err.name(), "CalibreFailedError" | "CalibreTimedOutError")
}

/// `durationInWords(ms)`: "10 minutes", "60 seconds", "300 ms".
pub fn duration_in_words(ms: u64) -> String {
    if ms >= 60_000 && ms.is_multiple_of(60_000) {
        let minutes = ms / 60_000;
        return format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" });
    }
    if ms >= 1000 && ms.is_multiple_of(1000) {
        let seconds = ms / 1000;
        return format!("{seconds} second{}", if seconds == 1 { "" } else { "s" });
    }
    format!("{ms} ms")
}

/// `TimedRun`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedRun {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    /// True when the run was stopped for outliving its timeout.
    pub timed_out: bool,
}

/// `runWithTimeout(argv, env, timeoutMs)`: spawn `argv` with exactly `env`
/// (nothing inherited), collect its output, and stop it, children and all,
/// if it is still running after `timeout_ms`. On macOS and Linux the run
/// gets a process group of its own and the whole group is killed; Windows
/// has no process groups to signal, so there only the tool is stopped.
///
/// Fails only when the program cannot be started at all.
pub fn run_with_timeout(
    argv: &[String],
    env: &Env,
    timeout_ms: u64,
) -> Result<TimedRun, EngineError> {
    let (program, args) = argv.split_first().expect("argv names a program");
    let mut command = sp_device::process::quiet_command(program);
    command
        .args(args)
        .env_clear()
        .envs(env.vars())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| EngineError::Fs(FsError::from_io(&e, "posix_spawn", program, None)))?;

    let mut stdout_pipe = child.stdout.take().expect("piped");
    let mut stderr_pipe = child.stderr.take().expect("piped");
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if !timed_out && Instant::now() >= deadline {
            timed_out = true;
            stop_tree(&mut child);
        }
        thread::sleep(Duration::from_millis(5));
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(TimedRun {
        code: status.map_or(1, exit_code),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
    })
}

/// The exit code Bun reports: the code, or 128 + the signal that ended it.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

/// SIGKILL the child's whole process group, or just the child where there
/// are no groups (or the group is already gone).
fn stop_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = rustix::process::Pid::from_child(child);
        if rustix::process::kill_process_group(pid, rustix::process::Signal::KILL).is_ok() {
            return;
        }
    }
    let _ = child.kill();
}

/// `runCalibre(tool, args, env, timeoutMs)`: run one Calibre tool with the
/// given environment, failing with its stderr (or exit code), or with the
/// timeout's own sentence.
pub fn run_calibre(
    tool: &str,
    args: &[String],
    env: &Env,
    timeout_ms: u64,
) -> Result<(), EngineError> {
    let mut argv = vec![tool.to_owned()];
    argv.extend(args.iter().cloned());
    let run = run_with_timeout(&argv, env, timeout_ms)?;
    if run.timed_out {
        return Err(calibre_timed_out(tool, timeout_ms));
    }
    if run.code != 0 {
        let stderr = crate::env::js_trim(&run.stderr);
        let detail = if stderr.is_empty() {
            format!("exit {}", run.code)
        } else {
            stderr.to_owned()
        };
        return Err(calibre_failed(&detail));
    }
    Ok(())
}

/// `epub.replace(/\.epub$/i, '')`.
pub(crate) fn strip_epub(epub: &str) -> &str {
    let n = epub.len();
    if n >= 5 && epub.is_char_boundary(n - 5) && epub[n - 5..].eq_ignore_ascii_case(".epub") {
        &epub[..n - 5]
    } else {
        epub
    }
}

/// `toKepub(epub)`: EPUB to KEPUB for Kobo's own renderer, in two steps
/// (Calibre picks its output format from the extension, so `.kepub`
/// first, then the rename to `.kepub.epub`). Nothing consumes it yet.
pub fn to_kepub(epub: &str, env: &Env) -> Result<String, EngineError> {
    let tool = calibre_tool("ebook-convert", env).ok_or_else(calibre_missing)?;
    let stem = strip_epub(epub);
    let raw = format!("{stem}.kepub");
    let kepub = format!("{stem}.kepub.epub");
    let mut args = vec![epub.to_owned(), raw.clone()];
    args.extend(CALIBRE_FORMAT_GUARDS.iter().map(|s| (*s).to_owned()));
    run_calibre(&tool, &args, env, CONVERSION_TIMEOUT_MS)?;
    if !Path::new(&raw).exists() {
        return Err(calibre_failed(
            "ebook-convert exited cleanly but produced no .kepub",
        ));
    }
    fsx::rename(&raw, &kepub)?;
    Ok(kepub)
}
