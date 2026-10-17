//! Ports tests/build-cli-e2e.test.ts: a real build of the CLI, checked
//! directly (not by calling verify_artifact, which is the code under test),
//! then run over the committed fixtures.
//!
//! Opt-in: a release build of sp-cli is minutes and a few hundred MB, so
//! these skip unless METHOD_READER_REAL_CLI_BUILD=1 and the sp-cli crate exists.
//! They build the HOST target only: cross-compiling to the other triples
//! needs linkers a default machine does not have. The Mac half also signs
//! (ad-hoc, `-`), so it additionally needs METHOD_READER_REAL_CODESIGN=1.

mod cli_build_common;

use std::path::{Path, PathBuf};

use cli_build_common::Scratch;
use sha2::{Digest, Sha256};
use xtask::cli_build::RunResult;
use xtask::cli_build::archive::archive_entries;
use xtask::cli_build::binfmt::{HEAD_BYTES, detect_binary_format, read_binary_format};
use xtask::cli_build::build::{
    BuildArgs, BuildEnv, MacSigning, RELEASE_FLOORS, build_all, parse_checksums, verify_artifact,
};
use xtask::cli_build::smoke::smoke_cli;
use xtask::cli_build::targets::{archive_path, binary_path, host_target, mac_host_target};
use xtask::cli_build::{real_run, workspace_version};
use xtask::paths;

fn skip_reason() -> Option<String> {
    if std::env::var("METHOD_READER_REAL_CLI_BUILD").as_deref() != Ok("1") {
        return Some("set METHOD_READER_REAL_CLI_BUILD=1 to build the real CLI".into());
    }
    if !paths::workspace_root()
        .join("crates/sp-cli/Cargo.toml")
        .is_file()
    {
        return Some("there is no sp-cli crate in this tree yet".into());
    }
    None
}

fn no_env(_: &str) -> Option<String> {
    None
}

/// Real tools, with cargo writing to a folder of its own so it never waits
/// on the lock this test run holds.
fn real_spawn_into(target_dir: PathBuf) -> impl Fn(&[String], &Path) -> RunResult {
    move |argv: &[String], cwd: &Path| {
        let mut command = std::process::Command::new(&argv[0]);
        command.args(&argv[1..]).current_dir(cwd);
        if argv[0] == "cargo" {
            command.env("CARGO_TARGET_DIR", &target_dir);
        }
        let out = command.output().expect("run a tool");
        RunResult {
            exit_code: out.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&out.stdout).into(),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        }
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fixture(name: &str) -> PathBuf {
    paths::repo_root().join("tests").join("fixtures").join(name)
}

#[test]
fn a_real_host_build_is_genuine_runs_the_fixtures_and_its_verification_is_not_vacuous() {
    if let Some(why) = skip_reason() {
        eprintln!("skipping the real CLI build: {why}");
        return;
    }
    let Some(target) = host_target() else {
        eprintln!("skipping: this machine is no cross target's host (a Mac builds with --mac)");
        return;
    };
    let scratch = Scratch::new("e2e");
    let out = scratch.join("dist");
    let cargo = scratch.join("cargo");
    let workspace = paths::workspace_root();
    let version = workspace_version(&workspace).unwrap();
    let spawn = real_spawn_into(cargo.clone());
    let env = BuildEnv {
        workspace,
        cargo_target_dir: cargo,
        spawn: &spawn,
        env: &no_env,
        floors: RELEASE_FLOORS,
    };
    let args = BuildArgs {
        version: version.clone(),
        out_dir: out.clone(),
        only: vec![target.id.to_string()],
        mac: None,
    };
    let made = build_all(&args, &env).unwrap();
    assert_eq!(made, [out.join(target.archive_name)]);

    // Produced genuine binaries, archives and checksums.
    let bin = binary_path(target, &out);
    let bin_bytes = std::fs::read(&bin).unwrap();
    assert_eq!(read_binary_format(&bin).unwrap(), target.format);
    assert!(bin_bytes.len() as u64 > RELEASE_FLOORS.binary_bytes);
    let arc = archive_path(target, &out);
    assert!(std::fs::metadata(&arc).unwrap().len() > RELEASE_FLOORS.archive_bytes);
    let entries = archive_entries(&arc).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, target.binary_name);
    assert_eq!(entries[0].size, bin_bytes.len() as u64);
    assert_ne!(entries[0].mode & 0o111, 0);
    let head = &entries[0].data[..entries[0].data.len().min(HEAD_BYTES)];
    assert_eq!(detect_binary_format(head), target.format);
    let sums = parse_checksums(&std::fs::read_to_string(out.join("SHA256SUMS")).unwrap());
    assert_eq!(sums.len(), 1);
    for (name, digest) in &sums {
        assert_eq!(digest, &sha256(&std::fs::read(out.join(name)).unwrap()));
    }

    // The host binary actually converts the committed fixtures.
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    smoke_cli(
        &bin,
        &fixture("screenplay.pdf"),
        &fixture("jpx.pdf"),
        &work,
        &version,
        &real_run,
    )
    .unwrap();

    // Not vacuous: the PRODUCTION floors reject what the happy path just
    // accepted, once it is truncated.
    assert_eq!(verify_artifact(target, &out, RELEASE_FLOORS), Ok(()));
    std::fs::write(&bin, vec![0u8; 1024]).unwrap();
    let err = verify_artifact(target, &out, RELEASE_FLOORS).unwrap_err();
    assert!(err.contains("floor"), "{err}");
}

#[test]
fn on_a_mac_mac_compiles_signs_with_the_release_flags_packages_the_signed_bytes_and_converts() {
    if let Some(why) = skip_reason() {
        eprintln!("skipping the real Mac build: {why}");
        return;
    }
    let Some(target) = mac_host_target() else {
        eprintln!("skipping: this machine is not an Apple silicon Mac");
        return;
    };
    if std::env::var("METHOD_READER_REAL_CODESIGN").as_deref() != Ok("1") {
        eprintln!("skipping: set METHOD_READER_REAL_CODESIGN=1 to let this run codesign (ad-hoc)");
        return;
    }
    let scratch = Scratch::new("e2e-mac");
    let out = scratch.join("dist");
    let cargo = scratch.join("cargo");
    let workspace = paths::workspace_root();
    let version = workspace_version(&workspace).unwrap();
    let spawn = real_spawn_into(cargo.clone());
    let env = BuildEnv {
        workspace,
        cargo_target_dir: cargo,
        spawn: &spawn,
        env: &no_env,
        floors: RELEASE_FLOORS,
    };
    let args = BuildArgs {
        version: version.clone(),
        out_dir: out.clone(),
        only: vec![target.id.to_string()],
        mac: Some(MacSigning {
            sign: Some("-".into()),
            notarize: false,
        }),
    };
    let made = build_all(&args, &env).unwrap();
    assert_eq!(made, [out.join(target.archive_name)]);

    let bin = binary_path(target, &out);
    assert_eq!(read_binary_format(&bin).unwrap(), target.format);
    let described = real_run(&["codesign".into(), "-dv".into(), bin.display().to_string()]).stderr;
    assert!(
        regex::Regex::new(r"flags=0x[0-9a-f]+\([^)]*runtime[^)]*\)")
            .unwrap()
            .is_match(&described),
        "{described}"
    );
    assert!(
        described.contains("Identifier=screepub-cli-arm64"),
        "{described}"
    );
    let ents = real_run(
        &["codesign", "-d", "--entitlements", "-", "--xml"]
            .iter()
            .map(|s| s.to_string())
            .chain([bin.display().to_string()])
            .collect::<Vec<_>>(),
    )
    .stdout;
    // No entitlements at all: the bun-era three (allow-jit among them)
    // went at the Rust cutover, so the hardened runtime is unrelaxed.
    assert!(!ents.contains("com.apple.security"), "{ents}");

    // The archive holds the SIGNED bytes: packaging came after signing.
    let entries = archive_entries(&archive_path(target, &out)).unwrap();
    assert_eq!(
        sha256(&entries[0].data),
        sha256(&std::fs::read(&bin).unwrap())
    );

    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    smoke_cli(
        &bin,
        &fixture("screenplay.pdf"),
        &fixture("jpx.pdf"),
        &work,
        &version,
        &real_run,
    )
    .unwrap();
}
