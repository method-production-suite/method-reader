//! `cargo xtask build-cli`: compile, sign, package and VERIFY the CLI
//! artifacts (was tools/build-cli.ts).
//!
//! The CLI is crate sp-cli's binary `method-reader`, compiled with `cargo
//! build --release --target <triple>` together with its second binary
//! `screepub` (the old name, kept a few releases: it says once that the
//! tool was renamed, then behaves exactly like method-reader). Both ride in
//! every archive. The Mac build signs each binary BEFORE
//! packaging it, so the tarball holds the signed bytes, and can notarize it
//! (`--notarize`; a bare Mach-O cannot be stapled, so notarization is
//! recorded with Apple and checked online).
//!
//! ```text
//! cargo xtask build-cli --version 0.12.0 --out dist/
//! cargo xtask build-cli --version 0.12.0 --out dist/ --only windows-x64
//! cargo xtask build-cli --version 0.12.0 --out dist/ --mac --unsigned
//! cargo xtask build-cli --version 0.12.0 --out dist/ --mac \
//!   --sign "Developer ID Application: ... (TEAMID)" --notarize
//!     # --notarize reads AC_API_KEY_PATH, AC_API_KEY_ID, AC_API_ISSUER_ID
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::archive::archive_entries;
use super::binfmt::{HEAD_BYTES, detect_binary_format, read_binary_format};
use super::targets::{
    MAC_TARGETS, Packaging, TARGETS, Target, alias_path, archive_path, binary_path, build_dir,
    is_mac_target,
};
use super::{FlagKind, RunResult, Spawn, parse_flags, real_spawn, toml_string, workspace_version};
use crate::paths;

/// The crate whose binary is the CLI, the binary's name, and the old
/// name's binary built beside it.
pub const CLI_CRATE: &str = "sp-cli";
pub const CLI_BINARY: &str = "method-reader";
pub const CLI_ALIAS_BINARY: &str = "screepub";

/// A release version, MAJOR.MINOR.PATCH with an optional prerelease: the
/// one rule every release tool checks its --version against, sp-release's.
pub use sp_release::names::is_release_version;

/// How a --mac build is signed. `sign` is the codesign identity, `-` for
/// ad-hoc, or `None` for an explicit --unsigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacSigning {
    pub sign: Option<String>,
    pub notarize: bool,
}

/// What `build-cli` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildArgs {
    pub version: String,
    pub out_dir: PathBuf,
    pub only: Vec<String>,
    /// Present only for a --mac build.
    pub mac: Option<MacSigning>,
}

/// The flags, refused rather than guessed at. Nothing touches the disk here,
/// so a mistyped target never costs a build.
pub fn parse_build_args(argv: &[String]) -> Result<BuildArgs, String> {
    let flags = parse_flags(
        "build-cli",
        argv,
        &[
            ("version", FlagKind::Value),
            ("out", FlagKind::Value),
            ("only", FlagKind::Multiple),
            ("mac", FlagKind::Switch),
            ("sign", FlagKind::Value),
            ("unsigned", FlagKind::Switch),
            ("notarize", FlagKind::Switch),
        ],
    )?;

    let given = flags.value("version");
    let version = given
        .map(|v| v.strip_prefix('v').unwrap_or(v))
        .unwrap_or("")
        .to_string();
    if !is_release_version(&version) {
        return Err(format!(
            "build-cli: --version must be MAJOR.MINOR.PATCH (got {})",
            given.unwrap_or("<missing>")
        ));
    }
    let Some(out) = flags.value("out") else {
        return Err(
            "build-cli: --out <dir> is required. There is no default: a build leaves a binary \
             AND an archive per target, and a default would write them somewhere you did not ask for."
                .to_string(),
        );
    };

    let sign = flags.value("sign").map(str::to_string);
    let unsigned = flags.switch("unsigned");
    let notarize = flags.switch("notarize");
    let mac = if flags.switch("mac") {
        if sign.is_some() && unsigned {
            return Err(
                "build-cli: --sign and --unsigned were both given; a binary is one or the other"
                    .to_string(),
            );
        }
        if sign.is_none() && !unsigned {
            return Err(
                "build-cli: --mac needs --sign <identity> (or `-` for ad-hoc) or an explicit --unsigned. \
                 An unsigned Mach-O is a Gatekeeper warning for everyone who downloads it, so it has \
                 to be asked for by name."
                    .to_string(),
            );
        }
        if notarize && unsigned {
            return Err(
                "build-cli: --notarize needs a signed binary; Apple will not notarize an unsigned one"
                    .to_string(),
            );
        }
        Some(MacSigning { sign, notarize })
    } else {
        if sign.is_some() || unsigned || notarize {
            return Err(
                "build-cli: --sign, --unsigned and --notarize only apply to a --mac build"
                    .to_string(),
            );
        }
        None
    };

    let table = if mac.is_some() { MAC_TARGETS } else { TARGETS };
    let known: Vec<&str> = table.iter().map(|t| t.id).collect();
    let requested: Vec<String> = flags
        .values("only")
        .iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    for id in &requested {
        if mac.is_none() && MAC_TARGETS.iter().any(|t| t.id == id) {
            return Err(format!(
                "build-cli: \"{id}\" is a macOS target; those build only with --mac"
            ));
        }
        if !known.contains(&id.as_str()) {
            return Err(format!(
                "build-cli: unknown target \"{id}\"; known targets are {}",
                known.join(", ")
            ));
        }
    }

    let out = PathBuf::from(out);
    let out_dir = if out.is_absolute() {
        out
    } else {
        std::env::current_dir()
            .map_err(|e| format!("build-cli: no current folder to resolve --out against: {e}"))?
            .join(out)
    };
    Ok(BuildArgs {
        version,
        out_dir,
        only: if requested.is_empty() {
            known.iter().map(|s| s.to_string()).collect()
        } else {
            requested
        },
        mac,
    })
}

/// The version the CLI binary reports: its own Cargo.toml's
/// `[package] version` when that is a literal, else the workspace's (with
/// `version.workspace = true`). Answers the version and the file it came from.
pub fn cli_version(workspace: &Path) -> Result<(String, String), String> {
    let manifest = workspace.join("crates").join(CLI_CRATE).join("Cargo.toml");
    if let Ok(text) = std::fs::read_to_string(&manifest)
        && let Some(v) = toml_string(&text, "package", "version")
    {
        return Ok((v, format!("rust/crates/{CLI_CRATE}/Cargo.toml")));
    }
    Ok((workspace_version(workspace)?, "rust/Cargo.toml".to_string()))
}

/// `method-reader --version` reports the crate version compiled in; --version
/// changes nothing about the binary. So the flag's job is to be CHECKED: a
/// binary that disagrees with its tag lies about itself in every bug report.
pub fn assert_crate_version(version: &str, workspace: &Path) -> Result<(), String> {
    let (have, file) = cli_version(workspace).map_err(|e| format!("build-cli: {e}"))?;
    if have != version {
        return Err(format!(
            "build-cli: {file} says {have} but --version says {version}. `method-reader --version` \
             reports the crate version compiled in, so this binary would misreport itself. Bump \
             the version, or build the version it already names."
        ));
    }
    Ok(())
}

/// The CLI crate has to exist before anything can be built from it.
pub fn assert_cli_crate(workspace: &Path) -> Result<(), String> {
    let manifest = workspace.join("crates").join(CLI_CRATE).join("Cargo.toml");
    if manifest.is_file() {
        return Ok(());
    }
    Err(format!(
        "build-cli: there is no {CLI_CRATE} crate yet ({} is missing), so there is no `{CLI_BINARY}` \
         binary to build. The CLI is crate {CLI_CRATE}'s binary (docs/superpowers/plans/2026-10-11-pure-rust-port.md).",
        manifest.display()
    ))
}

/// The one compile invocation, as data, so it can be asserted element by
/// element. A dropped --target silently builds the HOST architecture.
pub fn compile_argv(target: &Target) -> Vec<String> {
    [
        "cargo",
        "build",
        "--release",
        "--locked",
        "-p",
        CLI_CRATE,
        "--bin",
        CLI_BINARY,
        "--bin",
        CLI_ALIAS_BINARY,
        "--target",
        target.rust_triple,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Where cargo writes the binary for a target: `<target dir>/<triple>/release/`.
pub fn cargo_output_path(target: &Target, cargo_target_dir: &Path) -> PathBuf {
    cargo_target_dir
        .join(target.rust_triple)
        .join("release")
        .join(target.binary_name)
}

/// Where cargo writes the alias (`screepub`) beside it.
pub fn cargo_alias_output_path(target: &Target, cargo_target_dir: &Path) -> PathBuf {
    cargo_target_dir
        .join(target.rust_triple)
        .join("release")
        .join(target.alias_name)
}

/// Every artifact embeds the whole engine. Anything an order of magnitude
/// under a real build is a truncated write, not a lean build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Floors {
    pub binary_bytes: u64,
    pub archive_bytes: u64,
}

/// The production floors. The bun-compiled CLI embedded a whole runtime
/// (64-119 MB, floor 20 MB); a Rust binary carries only the engine, so the
/// floors sit where a truncated write lands, to be re-tuned against the
/// first real sp-cli release build.
pub const RELEASE_FLOORS: Floors = Floors {
    binary_bytes: 1_000_000,
    archive_bytes: 300_000,
};

/// Everything a build reaches outside itself, so tests hand in fakes.
pub struct BuildEnv<'a> {
    /// The cargo workspace (rust/): where cargo runs and the versions live.
    pub workspace: PathBuf,
    /// Where cargo writes (`$CARGO_TARGET_DIR`, else `<workspace>/target`).
    pub cargo_target_dir: PathBuf,
    pub spawn: Spawn<'a>,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub floors: Floors,
}

fn real_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

impl BuildEnv<'static> {
    /// The real tools, this checkout, the production floors.
    pub fn real() -> Self {
        let workspace = paths::workspace_root();
        let cargo_target_dir = match std::env::var_os("CARGO_TARGET_DIR") {
            Some(d) if Path::new(&d).is_absolute() => PathBuf::from(d),
            Some(d) => workspace.join(d),
            None => workspace.join("target"),
        };
        Self {
            workspace,
            cargo_target_dir,
            spawn: &real_spawn,
            env: &real_env,
            floors: RELEASE_FLOORS,
        }
    }
}

/// Compile one target and put the binary and its alias in its build
/// folder. Checks that cargo wrote both files rather than trusting its
/// exit code. Returns where the binary now sits.
pub fn compile_target(target: &Target, out_dir: &Path, env: &BuildEnv) -> Result<PathBuf, String> {
    let dir = build_dir(target, out_dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("build-cli: cannot make {}: {e}", dir.display()))?;
    let result = (env.spawn)(&compile_argv(target), &env.workspace);
    if result.exit_code != 0 {
        return Err(format!(
            "build-cli: {} failed to compile (exit {})\n{}",
            target.id,
            result.exit_code,
            result.stderr.trim()
        ));
    }
    let pairs = [
        (
            cargo_output_path(target, &env.cargo_target_dir),
            binary_path(target, out_dir),
        ),
        (
            cargo_alias_output_path(target, &env.cargo_target_dir),
            alias_path(target, out_dir),
        ),
    ];
    for (built, dest) in &pairs {
        if !built.is_file() {
            return Err(format!(
                "build-cli: {} compiled but cargo wrote nothing at {}",
                target.id,
                built.display()
            ));
        }
        std::fs::copy(built, dest).map_err(|e| {
            format!(
                "build-cli: {}: cannot copy {} to {}: {e}",
                target.id,
                built.display(),
                dest.display()
            )
        })?;
    }
    Ok(binary_path(target, out_dir))
}

/// The exact codesign call the shipped CLI is signed with: hardened runtime
/// and a secure timestamp, both of which notarization requires. `identity`
/// may be `-` for ad-hoc; `--timestamp` is accepted and ignored there.
///
/// No entitlements: the bun-compiled CLI carried allow-jit,
/// allow-unsigned-executable-memory and disable-library-validation from
/// tools/macos/screepub-cli.entitlements because bun's JIT needed them. A
/// Rust binary needs none of the three, so the file went at the Rust
/// cutover and the hardened runtime is now unrelaxed (rust/PARITY.md,
/// Decisions).
pub fn sign_argv(target: &Target, out_dir: &Path, identity: &str) -> Vec<String> {
    sign_argv_for(
        &binary_path(target, out_dir),
        &signing_identifier(CLI_BINARY, target),
        identity,
    )
}

/// The same call for the alias, under the identifier the CLI was signed
/// with up to 0.12.x.
pub fn sign_alias_argv(target: &Target, out_dir: &Path, identity: &str) -> Vec<String> {
    sign_argv_for(
        &alias_path(target, out_dir),
        &signing_identifier(CLI_ALIAS_BINARY, target),
        identity,
    )
}

/// `<name>-cli-<arch>`. codesign takes the identifier from the file name
/// unless told, and the shipped CLI has always been signed as
/// `screepub-cli-<arch>` (the Swift app's release script signed a file of
/// that name). The renamed tool is `method-reader-cli-<arch>`; the alias
/// keeps the old identifier, so its designated requirement is unchanged.
pub fn signing_identifier(name: &str, target: &Target) -> String {
    format!(
        "{name}-cli-{}",
        target.id.strip_prefix("macos-").unwrap_or(target.id)
    )
}

fn sign_argv_for(bin: &Path, identifier: &str, identity: &str) -> Vec<String> {
    vec![
        "codesign".into(),
        "--force".into(),
        "--options".into(),
        "runtime".into(),
        "--timestamp".into(),
        "--identifier".into(),
        identifier.into(),
        "--sign".into(),
        identity.into(),
        bin.display().to_string(),
    ]
}

fn has_hardened_runtime(described: &str) -> bool {
    regex::Regex::new(r"flags=0x[0-9a-f]+\([^)]*\bruntime\b[^)]*\)")
        .is_ok_and(|re| re.is_match(described))
}

fn names_a_team(described: &str) -> bool {
    described.lines().any(|line| {
        line.strip_prefix("TeamIdentifier=").is_some_and(|rest| {
            !rest.starts_with("not set") && rest.chars().next().is_some_and(|c| !c.is_whitespace())
        })
    })
}

/// Sign the tool, then its alias, and check what each signature says rather
/// than trusting codesign's exit code: it must verify strictly, carry the
/// hardened runtime (notarization rejects a binary without it, and only
/// after the upload), and for any identity but ad-hoc, name a team.
pub fn sign_target(
    target: &Target,
    out_dir: &Path,
    identity: &str,
    spawn: Spawn,
    cwd: &Path,
) -> Result<(), String> {
    if !is_mac_target(target) {
        return Err(format!(
            "build-cli: {} is not a macOS target and is never codesigned",
            target.id
        ));
    }
    for (argv, bin) in [
        (
            sign_argv(target, out_dir, identity),
            binary_path(target, out_dir),
        ),
        (
            sign_alias_argv(target, out_dir, identity),
            alias_path(target, out_dir),
        ),
    ] {
        sign_one(
            target,
            argv,
            &bin.display().to_string(),
            identity,
            spawn,
            cwd,
        )?;
    }
    Ok(())
}

fn sign_one(
    target: &Target,
    sign: Vec<String>,
    bin: &str,
    identity: &str,
    spawn: Spawn,
    cwd: &Path,
) -> Result<(), String> {
    let bin = bin.to_string();
    let run = |argv: Vec<String>, what: &str| -> Result<String, String> {
        let r = spawn(&argv, cwd);
        if r.exit_code != 0 {
            return Err(format!(
                "build-cli: {}: {what} failed (exit {})\n{}",
                target.id,
                r.exit_code,
                r.stderr.trim()
            ));
        }
        Ok(r.stderr)
    };
    run(sign, "codesign")?;
    run(
        vec![
            "codesign".into(),
            "--verify".into(),
            "--strict".into(),
            "--verbose=2".into(),
            bin.clone(),
        ],
        "codesign --verify",
    )?;
    // codesign -dv writes its description to stderr.
    let described = run(vec!["codesign".into(), "-dv".into(), bin], "codesign -dv")?;
    if !has_hardened_runtime(&described) {
        return Err(format!(
            "build-cli: {}: signed WITHOUT the hardened runtime; notarization would reject it\n{}",
            target.id,
            described.trim()
        ));
    }
    if identity != "-" && !names_a_team(&described) {
        return Err(format!(
            "build-cli: {}: signed with \"{identity}\" but the binary names no team (an ad-hoc signature?)\n{}",
            target.id,
            described.trim()
        ));
    }
    Ok(())
}

/// The App Store Connect API key the release job writes for notarization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotaryCredentials {
    pub key_path: String,
    pub key_id: String,
    pub issuer: String,
}

/// Read up front, so a missing key fails before any compile.
pub fn notary_credentials(
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<NotaryCredentials, String> {
    let names = ["AC_API_KEY_PATH", "AC_API_KEY_ID", "AC_API_ISSUER_ID"];
    let got: Vec<Option<String>> = names
        .iter()
        .map(|n| env(n).filter(|v| !v.is_empty()))
        .collect();
    let missing: Vec<&str> = names
        .iter()
        .zip(&got)
        .filter(|(_, v)| v.is_none())
        .map(|(n, _)| *n)
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "build-cli: --notarize needs {} in the environment",
            missing.join(" and ")
        ));
    }
    let mut got = got.into_iter().flatten();
    Ok(NotaryCredentials {
        key_path: got.next().unwrap_or_default(),
        key_id: got.next().unwrap_or_default(),
        issuer: got.next().unwrap_or_default(),
    })
}

/// The zip the signed binaries ride in for the submission alone.
pub fn notarize_zip_path(target: &Target, out_dir: &Path) -> PathBuf {
    build_dir(target, out_dir).join("method-reader-notarize.zip")
}

/// The folder the zip is made of: copies of the tool and its alias, so
/// one submission notarizes both.
pub fn notarize_stage_path(target: &Target, out_dir: &Path) -> PathBuf {
    build_dir(target, out_dir).join("notarize")
}

/// notarytool takes an archive, not a bare Mach-O, so the signed binaries
/// (the tool and its alias, copied into one folder) ride in a zip that
/// exists only for the submission, and folder and zip are removed whatever
/// happens. The verdict is READ (`--output-format json`, status
/// "Accepted", as tauri-macos-sign 2.3.4 does for the window), never
/// inferred from the exit code.
pub fn notarize_target(
    target: &Target,
    out_dir: &Path,
    creds: &NotaryCredentials,
    spawn: Spawn,
    cwd: &Path,
) -> Result<(), String> {
    let zip = notarize_zip_path(target, out_dir);
    let stage = notarize_stage_path(target, out_dir);
    let result = stage_both(target, out_dir, &stage)
        .and_then(|()| submit(target, &stage, &zip, creds, spawn, cwd));
    let _ = std::fs::remove_file(&zip);
    let _ = std::fs::remove_dir_all(&stage);
    result
}

fn stage_both(target: &Target, out_dir: &Path, stage: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(stage);
    std::fs::create_dir_all(stage)
        .map_err(|e| format!("build-cli: cannot make {}: {e}", stage.display()))?;
    for (from, name) in [
        (binary_path(target, out_dir), target.binary_name),
        (alias_path(target, out_dir), target.alias_name),
    ] {
        std::fs::copy(&from, stage.join(name))
            .map_err(|e| format!("build-cli: cannot copy {}: {e}", from.display()))?;
    }
    Ok(())
}

fn submit(
    target: &Target,
    stage: &Path,
    zip: &Path,
    creds: &NotaryCredentials,
    spawn: Spawn,
    cwd: &Path,
) -> Result<(), String> {
    let run = |argv: Vec<String>, what: &str| -> Result<String, String> {
        let r: RunResult = spawn(&argv, cwd);
        if r.exit_code != 0 {
            return Err(format!(
                "build-cli: {}: {what} failed (exit {})\n{}",
                target.id,
                r.exit_code,
                r.stderr.trim()
            ));
        }
        Ok(r.stdout)
    };
    let zip_s = zip.display().to_string();
    run(
        vec![
            "ditto".into(),
            "-c".into(),
            "-k".into(),
            stage.display().to_string(),
            zip_s.clone(),
        ],
        "ditto",
    )?;
    let answer = run(
        [
            "xcrun",
            "notarytool",
            "submit",
            &zip_s,
            "--key",
            &creds.key_path,
            "--key-id",
            &creds.key_id,
            "--issuer",
            &creds.issuer,
            "--output-format",
            "json",
            "--wait",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "notarytool submit",
    )?;
    let verdict: serde_json::Value = serde_json::from_str(answer.trim()).map_err(|_| {
        let shown = if answer.trim().is_empty() {
            "<empty>"
        } else {
            answer.trim()
        };
        format!(
            "build-cli: {}: notarytool's answer was not JSON:\n{shown}",
            target.id
        )
    })?;
    let field = |k: &str| verdict.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let status = field("status");
    if status.as_deref() != Some("Accepted") {
        return Err(format!(
            "build-cli: {}: notarization {} ({}); see `xcrun notarytool log {}`",
            target.id,
            status.as_deref().unwrap_or("returned no status"),
            field("message").as_deref().unwrap_or("no message"),
            field("id").as_deref().unwrap_or("<no id>"),
        ));
    }
    println!(
        "   {}: notarized, {}",
        target.id,
        field("id").unwrap_or_default()
    );
    Ok(())
}

/// Pack the binary and its alias for download, named plainly inside so an
/// unpack leaves runnable files in the current folder. The executable bit is SET, not
/// inherited: on Linux it is the difference between a download that runs
/// and one that does not. An existing archive is replaced, never appended.
pub fn package_target(target: &Target, out_dir: &Path) -> Result<PathBuf, String> {
    let bin = binary_path(target, out_dir);
    let alias = alias_path(target, out_dir);
    for b in [&bin, &alias] {
        std::fs::metadata(b).map_err(|e| format!("build-cli: cannot read {}: {e}", b.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(b, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("build-cli: cannot chmod {}: {e}", b.display()))?;
        }
    }
    let out = archive_path(target, out_dir);
    let _ = std::fs::remove_file(&out);
    // sp-release's writer, the one release.yml's `cli-archive` runs:
    // `method-reader` and `screepub` (`.exe` in the zip), mode 0755.
    let packed = match target.packaging {
        Packaging::TarGz => {
            sp_release::archive::cli_tarball(&bin, Some(&alias), sp_release::archive::build_time())
        }
        Packaging::Zip => sp_release::archive::cli_zip(&bin, Some(&alias)),
    }
    .map_err(|e| format!("build-cli: cannot write {}: {e}", out.display()))?;
    std::fs::write(&out, packed)
        .map_err(|e| format!("build-cli: cannot write {}: {e}", out.display()))?;
    Ok(out)
}

/// Check what was PRODUCED, not the compiler's exit code: a build can exit 0
/// and leave a truncated file, and a mis-typed --target exits 0 while
/// producing a perfectly valid binary for the wrong machine.
pub fn verify_artifact(target: &Target, out_dir: &Path, floors: Floors) -> Result<(), String> {
    let id = target.id;
    let bin = binary_path(target, out_dir);
    if !bin.is_file() {
        return Err(format!("build-cli: {id}: no binary at {}", bin.display()));
    }
    let bin_bytes = std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);
    if bin_bytes < floors.binary_bytes {
        return Err(format!(
            "build-cli: {id}: binary is {bin_bytes} bytes, under the {}-byte floor",
            floors.binary_bytes
        ));
    }
    let bin_format = read_binary_format(&bin)
        .map_err(|e| format!("build-cli: {id}: cannot read {}: {e}", bin.display()))?;
    if bin_format != target.format {
        return Err(format!(
            "build-cli: {id}: binary is {bin_format}, expected {}",
            target.format
        ));
    }

    let arc = archive_path(target, out_dir);
    if !arc.is_file() {
        return Err(format!("build-cli: {id}: no archive at {}", arc.display()));
    }
    let arc_bytes = std::fs::metadata(&arc).map(|m| m.len()).unwrap_or(0);
    if arc_bytes < floors.archive_bytes {
        return Err(format!(
            "build-cli: {id}: archive is {arc_bytes} bytes, under the {}-byte floor",
            floors.archive_bytes
        ));
    }
    let entries = archive_entries(&arc).map_err(|e| format!("build-cli: {id}: {e}"))?;
    verify_member(target, &entries, target.binary_name, bin_bytes)?;
    // The alias: on disk, the right format, and in the archive as itself.
    let alias = alias_path(target, out_dir);
    let alias_bytes = std::fs::metadata(&alias)
        .map(|m| m.len())
        .map_err(|_| format!("build-cli: {id}: no alias binary at {}", alias.display()))?;
    let alias_format = read_binary_format(&alias)
        .map_err(|e| format!("build-cli: {id}: cannot read {}: {e}", alias.display()))?;
    if alias_format != target.format {
        return Err(format!(
            "build-cli: {id}: the {} alias is {alias_format}, expected {}",
            target.alias_name, target.format
        ));
    }
    verify_member(target, &entries, target.alias_name, alias_bytes)
}

/// One archive member: present, as big as the file it was packed from, the
/// target's format, executable.
fn verify_member(
    target: &Target,
    entries: &[super::archive::ArchiveEntry],
    name: &str,
    bin_bytes: u64,
) -> Result<(), String> {
    let id = target.id;
    let Some(member) = entries.iter().find(|e| e.name == name) else {
        let held: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        let held = if held.is_empty() {
            "<nothing>".to_string()
        } else {
            held.join(", ")
        };
        return Err(format!(
            "build-cli: {id}: {} does not contain {name} (it holds: {held})",
            target.archive_name
        ));
    };
    if member.size != bin_bytes {
        return Err(format!(
            "build-cli: {id}: {name} inside the archive is {} bytes, the binary on disk is {bin_bytes}",
            member.size
        ));
    }
    let head = &member.data[..member.data.len().min(HEAD_BYTES)];
    let member_format = detect_binary_format(head);
    if member_format != target.format {
        return Err(format!(
            "build-cli: {id}: {name} inside the archive is {member_format}, expected {}",
            target.format
        ));
    }
    if member.mode & 0o111 == 0 {
        return Err(format!(
            "build-cli: {id}: {name} inside the archive is not executable (mode {:o})",
            member.mode
        ));
    }
    Ok(())
}

/// A file's SHA-256, lowercase hex, read in pieces (an app bundle's archive
/// can be large).
pub fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
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

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Name to digest, from SHA256SUMS text.
pub fn parse_checksums(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.split('\n') {
        let Some((digest, name)) = line.split_once("  ") else {
            continue;
        };
        let hex = digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if hex && !name.is_empty() {
            map.insert(name.to_string(), digest.to_string());
        }
    }
    map
}

/// The format `sha256sum -c` and `shasum -a 256 -c` both parse: lowercase
/// hex, two spaces, a bare filename, a trailing newline, sorted.
///
/// MERGES with an existing file: `--only windows-x64` into a folder that
/// already holds a full build used to leave a one-line SHA256SUMS that
/// `sha256sum -c` then passed with two downloads unchecked. Carried-over
/// names are RE-HASHED from the files on disk, never copied out of the old
/// text, and a named file that is gone is dropped, loudly.
pub fn write_checksums(
    out_dir: &Path,
    archive_names: &[String],
    file_name: &str,
) -> Result<String, String> {
    let path = out_dir.join(file_name);
    let mut names: Vec<String> = archive_names.to_vec();
    if let Ok(old) = std::fs::read_to_string(&path) {
        for name in parse_checksums(&old).into_keys() {
            if archive_names.contains(&name) {
                continue;
            }
            if out_dir.join(&name).is_file() {
                names.push(name);
            } else {
                eprintln!(
                    "{name} is named in {file_name} but no longer in {}; dropping it",
                    out_dir.display()
                );
            }
        }
    }
    names.sort();
    let mut text = String::new();
    for name in &names {
        text.push_str(&format!("{}  {name}\n", sha256_file(&out_dir.join(name))?));
    }
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(text)
}

/// Compile, sign, package and verify one target at a time, verifying BEFORE
/// moving on, so a broken target fails at the target that broke. Every
/// refusal (version, a Mac id without --mac, a missing sp-cli, missing
/// notary credentials) comes before the first compile.
pub fn build_all(args: &BuildArgs, env: &BuildEnv) -> Result<Vec<PathBuf>, String> {
    assert_crate_version(&args.version, &env.workspace)?;
    if args.mac.is_none() {
        let stray: Vec<&str> = args
            .only
            .iter()
            .filter(|id| MAC_TARGETS.iter().any(|t| t.id == id.as_str()))
            .map(String::as_str)
            .collect();
        if !stray.is_empty() {
            return Err(format!(
                "build-cli: {} build only with --mac",
                stray.join(", ")
            ));
        }
    }
    assert_cli_crate(&env.workspace)?;
    let creds = match &args.mac {
        Some(m) if m.notarize => Some(notary_credentials(env.env)?),
        _ => None,
    };
    std::fs::create_dir_all(&args.out_dir)
        .map_err(|e| format!("build-cli: cannot make {}: {e}", args.out_dir.display()))?;

    let table = if args.mac.is_some() {
        MAC_TARGETS
    } else {
        TARGETS
    };
    let mut names = Vec::new();
    for target in table
        .iter()
        .filter(|t| args.only.iter().any(|id| id == t.id))
    {
        println!("── {} ({})", target.id, target.rust_triple);
        compile_target(target, &args.out_dir, env)?;
        if let Some(mac) = &args.mac {
            // Signed BEFORE packaging, so the tarball holds the signed bytes.
            match &mac.sign {
                Some(identity) => {
                    sign_target(target, &args.out_dir, identity, env.spawn, &env.workspace)?;
                    if let Some(creds) = &creds {
                        notarize_target(target, &args.out_dir, creds, env.spawn, &env.workspace)?;
                    }
                }
                None => eprintln!(
                    "   {}: UNSIGNED, as asked; Gatekeeper will refuse a downloaded copy",
                    target.id
                ),
            }
        }
        package_target(target, &args.out_dir)?;
        verify_artifact(target, &args.out_dir, env.floors)?;
        let bytes = std::fs::metadata(archive_path(target, &args.out_dir)).map_or(0, |m| m.len());
        println!("   {}  {bytes} bytes  ok", target.archive_name);
        names.push(target.archive_name.to_string());
    }

    write_checksums(&args.out_dir, &names, "SHA256SUMS")?;
    Ok(names.iter().map(|n| args.out_dir.join(n)).collect())
}

/// `cargo xtask build-cli`.
pub fn main(argv: &[String]) -> u8 {
    let result = parse_build_args(argv).and_then(|args| {
        let made = build_all(&args, &BuildEnv::real())?;
        Ok((made.len(), args.out_dir))
    });
    match result {
        Ok((n, out)) => {
            println!("\n{n} artifact(s) + SHA256SUMS in {}", out.display());
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}
