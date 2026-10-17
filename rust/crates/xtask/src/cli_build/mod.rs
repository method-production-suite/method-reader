//! Building and checking the method-reader CLI: was tools/build-cli.ts,
//! smoke-cli.ts and verify-signing.ts (with the half of tools/dmg.ts it
//! uses). build-sidecar.ts and sidecar-targets.ts retired with the engine
//! sidecar: the Rust app links the engine, and their Rust triples are
//! [`targets`] now.
//!
//! - [`build`]: `cargo xtask build-cli`, compile (cargo, crate sp-cli), sign,
//!   notarize, package and verify the CLI per target.
//! - [`smoke`]: `cargo xtask smoke-cli`, run a built CLI over the fixtures.
//! - [`signing`]: `cargo xtask verify-signing`, a published DMG against the
//!   requirements the frozen Swift updater pins.
//! - [`targets`], [`binfmt`], [`archive`]: what those share; the DMG half
//!   is [`crate::dmg`], shared with smoke-bundle.
//!
//! Every outside program (cargo, codesign, ditto, xcrun, spctl, hdiutil, the
//! CLI under test) is run through a closure, so the tests hand in fakes.

pub mod archive;
pub mod binfmt;
pub mod build;
pub mod signing;
pub mod smoke;
pub mod targets;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// What a program answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl RunResult {
    /// Exit 0 with this stdout and stderr.
    pub fn ok(stdout: &str, stderr: &str) -> Self {
        Self {
            exit_code: 0,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    /// This exit code with this stderr and no stdout.
    pub fn failed(exit_code: i32, stderr: &str) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }
}

/// Runs an argv (program first) and answers what it printed.
pub type Runner<'a> = &'a dyn Fn(&[String]) -> RunResult;

/// Runs an argv in a working folder.
pub type Spawn<'a> = &'a dyn Fn(&[String], &Path) -> RunResult;

/// The real runner. A program that cannot be started (codesign on Linux,
/// say) answers exit 127 rather than failing, so the caller's message names
/// what it was doing.
pub fn real_run(argv: &[String]) -> RunResult {
    run_with(argv, None)
}

/// [`real_run`] in a working folder.
pub fn real_spawn(argv: &[String], cwd: &Path) -> RunResult {
    run_with(argv, Some(cwd))
}

fn run_with(argv: &[String], cwd: Option<&Path>) -> RunResult {
    let Some((program, rest)) = argv.split_first() else {
        return RunResult::failed(127, "nothing to run");
    };
    let mut command = Command::new(program);
    command.args(rest).stdin(Stdio::null());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    match command.output() {
        Ok(out) => RunResult {
            exit_code: out.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(err) => RunResult::failed(127, &format!("{program}: {err}")),
    }
}

pub use crate::scratch::TempDir;

/// How a flag takes its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagKind {
    /// `--name value` or `--name=value`, the last one wins.
    Value,
    /// `--name value`, repeatable; every value is kept.
    Multiple,
    /// `--name`, no value.
    Switch,
}

/// The flags a command was given, after [`parse_flags`].
#[derive(Debug, Default)]
pub struct Flags {
    values: BTreeMap<String, Vec<String>>,
    switches: Vec<String>,
}

impl Flags {
    /// A value flag's last value.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    /// Every value a repeatable flag was given.
    pub fn values(&self, name: &str) -> &[String] {
        self.values.get(name).map_or(&[], Vec::as_slice)
    }

    /// Whether a switch was given.
    pub fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }
}

/// Strict flag parsing, as node's `parseArgs({ strict: true,
/// allowPositionals: false })` did for the TypeScript tools: an unknown
/// flag, a missing value, a value on a switch or a positional is refused,
/// never ignored.
pub fn parse_flags(
    tool: &str,
    args: &[String],
    spec: &[(&str, FlagKind)],
) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let Some(body) = arg.strip_prefix("--") else {
            return Err(format!("{tool}: unexpected argument '{arg}'"));
        };
        let (name, inline) = match body.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (body, None),
        };
        let Some(&(_, kind)) = spec.iter().find(|(n, _)| *n == name) else {
            return Err(format!("{tool}: unknown option '--{name}'"));
        };
        match kind {
            FlagKind::Switch => {
                if inline.is_some() {
                    return Err(format!("{tool}: option '--{name}' does not take a value"));
                }
                flags.switches.push(name.to_string());
            }
            FlagKind::Value | FlagKind::Multiple => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        args.get(i).cloned().ok_or_else(|| {
                            format!("{tool}: option '--{name} <value>' argument missing")
                        })?
                    }
                };
                let slot = flags.values.entry(name.to_string()).or_default();
                if kind == FlagKind::Value {
                    slot.clear();
                }
                slot.push(value);
            }
        }
        i += 1;
    }
    Ok(flags)
}

/// A `key = "value"` string in one `[section]` of a Cargo.toml, read line by
/// line: enough for the version fields, with no TOML library.
pub fn toml_string(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            current = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_string();
            continue;
        }
        if current != section {
            continue;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim();
        let inner = value.strip_prefix('"')?;
        return inner.split('"').next().map(str::to_string);
    }
    None
}

/// The version every crate of the workspace reports: rust/Cargo.toml's
/// `[workspace.package] version`.
pub fn workspace_version(workspace: &Path) -> Result<String, String> {
    let path = workspace.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    sp_release::version::workspace_version(&text)
        .ok_or_else(|| format!("{} names no [workspace.package] version", path.display()))
}

/// `cargo xtask build-cli`: its exit code.
pub fn build_cli_main(args: &[String]) -> u8 {
    build::main(args)
}

/// `cargo xtask smoke-cli`: its exit code.
pub fn smoke_cli_main(args: &[String]) -> u8 {
    smoke::main(args)
}

/// `cargo xtask verify-signing`: its exit code.
pub fn verify_signing_main(args: &[String]) -> u8 {
    signing::main(args)
}
