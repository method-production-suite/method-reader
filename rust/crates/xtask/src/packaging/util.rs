//! What the packaging tools share: running outside programs through a seam
//! a test can replace, checksums, the release-version rule, and a strict
//! flag parser in the manner of node's `parseArgs`.
//!
//! What these share with build-cli (the run result, the release-version
//! rule, the checksum file) is cli_build's, re-exported here. The flag
//! parser stays separate: it reproduces node's strict `parseArgs` messages,
//! which build-cli's own flags never had.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use crate::cli_build::RunResult;
pub use crate::cli_build::build::{
    hex, is_release_version, parse_checksums, sha256_file, write_checksums,
};

/// The seam every outside program goes through: the argv and the folder
/// to run it in. A test passes a closure; the tools pass [`real_run`].
pub type Runner<'a> = dyn FnMut(&[String], Option<&Path>) -> RunResult + 'a;

fn spawn(argv: &[String], cwd: Option<&Path>, inherit_stdout: bool) -> RunResult {
    let Some((program, rest)) = argv.split_first() else {
        return RunResult {
            exit_code: 127,
            ..RunResult::default()
        };
    };
    let mut cmd = Command::new(program);
    cmd.args(rest).stdin(Stdio::null()).stderr(Stdio::piped());
    cmd.stdout(if inherit_stdout {
        Stdio::inherit()
    } else {
        Stdio::piped()
    });
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // A missing program (codesign on Linux, say) is answered as a failed
    // run, so the caller's message names what it was doing.
    match cmd.output() {
        Ok(out) => RunResult {
            exit_code: out.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => RunResult {
            exit_code: 127,
            stdout: String::new(),
            stderr: format!("{program}: {e}"),
        },
    }
}

/// Runs a program, collecting both streams.
pub fn real_run(argv: &[String], cwd: Option<&Path>) -> RunResult {
    spawn(argv, cwd, false)
}

/// Runs a long build: its stdout goes to the terminal, stderr is kept for
/// the failure message.
pub fn real_build(argv: &[String], cwd: Option<&Path>) -> RunResult {
    spawn(argv, cwd, true)
}

/// argv from string slices.
pub fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// `--version`'s leading `v`, which a tag carries, removed once.
pub fn strip_v(v: &str) -> &str {
    v.strip_prefix('v').unwrap_or(v)
}

/// The flags a command takes: name -> takes a value (false for a switch).
pub struct FlagSpec<'a> {
    pub name: &'a str,
    pub value: bool,
}

/// What node's strict `parseArgs` gave: `--name value`, `--name=value` and
/// switches; an unknown flag or a positional is an error.
pub fn parse_flags(args: &[String], spec: &[FlagSpec]) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let Some(body) = arg.strip_prefix("--") else {
            return Err(format!(
                "Unexpected argument '{arg}'. This command does not take positional arguments"
            ));
        };
        let (name, inline) = match body.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (body, None),
        };
        let Some(flag) = spec.iter().find(|f| f.name == name) else {
            return Err(format!("Unknown option '--{name}'"));
        };
        if flag.value {
            let value = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    match args.get(i) {
                        Some(v) if !v.starts_with("--") => v.clone(),
                        _ => {
                            return Err(format!("Option '--{name} <value>' argument missing"));
                        }
                    }
                }
            };
            out.insert(name.to_string(), value);
        } else {
            if inline.is_some() {
                return Err(format!("Option '--{name}' does not take an argument"));
            }
            out.insert(name.to_string(), "true".to_string());
        }
        i += 1;
    }
    Ok(out)
}

/// `-h` or `--help` anywhere: print `usage` and stop.
pub fn wants_help(args: &[String], usage: &str) -> bool {
    let asked = args.iter().any(|a| a == "-h" || a == "--help");
    if asked {
        println!("{usage}");
    }
    asked
}

/// A path made absolute against the current folder.
pub fn absolute(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path))
            .unwrap_or(path)
    }
}

/// The host as node names it: `linux`, `darwin` or `win32` (others as Rust
/// names them).
pub fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

/// The host architecture as node names it: `x64` or `arm64`.
pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        other => other,
    }
}
