//! What the packaging tools share: running outside programs through a seam
//! a test can replace, checksums, the release-version rule, and a strict
//! flag parser in the manner of node's `parseArgs`.
//!
//! These were imported from tools/build-cli.ts and tools/smoke-cli.ts. The
//! cli_build module ports those files separately; the copies here keep this
//! module self-contained until the owner unifies them.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

/// What one run of an outside program answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl RunResult {
    pub fn ok(stdout: &str) -> Self {
        RunResult {
            exit_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }
}

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

/// A release version, MAJOR.MINOR.PATCH with an optional prerelease: the
/// one rule every release tool checks its --version against
/// (`/^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$/`).
pub fn is_release_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let core_ok = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let pre_ok = pre
        .is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'));
    core_ok && pre_ok
}

/// `--version`'s leading `v`, which a tag carries, removed once.
pub fn strip_v(v: &str) -> &str {
    v.strip_prefix('v').unwrap_or(v)
}

/// Lowercase hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f =
        std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The format `sha256sum -c` and `shasum -a 256 -c` both parse: lowercase
/// hex, two spaces, a bare filename, a newline. Sorted. Names already in
/// the file that are not being written now are carried over, RE-HASHED
/// from disk; one whose file is gone is dropped with a warning.
pub fn write_checksums(
    out_dir: &Path,
    names: &[String],
    file_name: &str,
) -> Result<String, String> {
    let path = out_dir.join(file_name);
    let mut all: Vec<String> = names.to_vec();
    if let Ok(old) = std::fs::read_to_string(&path) {
        for name in parse_checksums(&old).into_keys() {
            if names.contains(&name) {
                continue;
            }
            if out_dir.join(&name).exists() {
                all.push(name);
            } else {
                eprintln!(
                    "build-cli: {name} is named in {file_name} but no longer in {}; dropping it",
                    out_dir.display()
                );
            }
        }
    }
    all.sort();
    let mut text = String::new();
    for name in &all {
        text.push_str(&format!("{}  {name}\n", sha256_file(&out_dir.join(name))?));
    }
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(text)
}

/// name -> digest, for every well-formed line.
pub fn parse_checksums(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.split('\n') {
        if line.len() > 66
            && line.as_bytes()[..64]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            && &line[64..66] == "  "
        {
            map.insert(line[66..].to_string(), line[..64].to_string());
        }
    }
    map
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
