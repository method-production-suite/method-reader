//! Ports tests/build-cli.test.ts: `cargo xtask build-cli`. Every outside
//! program is a fake closure; the two checks that need a real cargo build of
//! sp-cli skip unless METHOD_READER_REAL_CLI_BUILD=1 and the crate exists.

mod cli_build_common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use cli_build_common::{
    Scratch, elf_header, elf_payload, fat_header, macho_header, payload_for, pe_header, strings,
    which,
};
use sha2::{Digest, Sha256};
use xtask::cli_build::RunResult;
use xtask::cli_build::archive::{archive_entries, tar_gz_entries, write_tar_gz, zip_entries};
use xtask::cli_build::binfmt::{BinaryFormat as F, detect_binary_format, read_binary_format};
use xtask::cli_build::build::{
    BuildArgs, BuildEnv, Floors, MacSigning, NotaryCredentials, RELEASE_FLOORS,
    assert_crate_version, build_all, cargo_alias_output_path, cargo_output_path, compile_argv,
    compile_target, notarize_target, notary_credentials, package_target, parse_build_args,
    parse_checksums, sign_alias_argv, sign_argv, sign_target, verify_artifact, write_checksums,
};
use xtask::cli_build::targets::{
    MAC_TARGETS, TARGETS, Target, alias_path, archive_path, binary_path, build_dir, find_target,
    host_target, mac_target_id_for_host, target_id_for_host,
};
use xtask::cli_build::workspace_version;
use xtask::paths;

const TINY: Floors = Floors {
    binary_bytes: 512,
    archive_bytes: 32,
};

fn t(id: &str) -> &'static Target {
    find_target(id).expect("a known target")
}

fn args(parts: &[&str]) -> Vec<String> {
    strings(parts)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn no_env(_: &str) -> Option<String> {
    None
}

/// A cargo workspace with this version, with or without an sp-cli crate.
fn workspace_with(version: &str, with_cli: bool) -> Scratch {
    let ws = Scratch::new("ws");
    std::fs::write(
        ws.join("Cargo.toml"),
        format!("[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    if with_cli {
        std::fs::create_dir_all(ws.join("crates/sp-cli")).unwrap();
        std::fs::write(
            ws.join("crates/sp-cli/Cargo.toml"),
            "[package]\nname = \"sp-cli\"\nversion.workspace = true\n",
        )
        .unwrap();
    }
    ws
}

fn test_env<'a>(
    workspace: &Path,
    cargo_target_dir: &Path,
    spawn: &'a dyn Fn(&[String], &Path) -> RunResult,
    env: &'a dyn Fn(&str) -> Option<String>,
    floors: Floors,
) -> BuildEnv<'a> {
    BuildEnv {
        workspace: workspace.to_path_buf(),
        cargo_target_dir: cargo_target_dir.to_path_buf(),
        spawn,
        env,
        floors,
    }
}

// ── detect_binary_format ────────────────────────────────────────────────

#[test]
fn names_each_format_this_project_can_produce() {
    assert_eq!(detect_binary_format(&elf_header(0x3e)), F::ElfX8664);
    assert_eq!(detect_binary_format(&elf_header(0xb7)), F::ElfAarch64);
    assert_eq!(detect_binary_format(&pe_header(0x8664, 0x78)), F::PeX8664);
    assert_eq!(
        detect_binary_format(&macho_header(0x0100_000c)),
        F::MachoArm64
    );
    assert_eq!(
        detect_binary_format(&macho_header(0x0100_0007)),
        F::MachoX8664
    );
}

#[test]
fn a_universal_binary_is_named_as_such_not_mistaken_for_one_slice() {
    let both = [0x0100_0007, 0x0100_000c];
    assert_eq!(
        detect_binary_format(&fat_header(&both, 0xcafe_babe)),
        F::MachoUniversal
    );
    assert_eq!(
        detect_binary_format(&fat_header(&both, 0xcafe_babf)),
        F::MachoUniversal
    );
}

#[test]
fn a_fat_binary_missing_one_of_the_two_arches_is_not_universal() {
    assert_eq!(
        detect_binary_format(&fat_header(&[0x0100_000c], 0xcafe_babe)),
        F::Unknown
    );
    assert_eq!(
        detect_binary_format(&fat_header(&[0x0100_0007], 0xcafe_babe)),
        F::Unknown
    );
}

#[test]
fn the_two_elf_architectures_are_never_confused_for_each_other() {
    assert_ne!(detect_binary_format(&elf_header(0x3e)), F::ElfAarch64);
    assert_ne!(detect_binary_format(&elf_header(0xb7)), F::ElfX8664);
    assert_eq!(detect_binary_format(&elf_header(0x28)), F::Unknown); // 32-bit ARM
}

#[test]
fn a_32_bit_or_big_endian_elf_is_not_accepted_as_one_of_ours() {
    let mut b32 = elf_header(0x3e);
    b32[4] = 1;
    assert_eq!(detect_binary_format(&b32), F::Unknown);
    let mut be = elf_header(0x3e);
    be[5] = 2;
    assert_eq!(detect_binary_format(&be), F::Unknown);
}

#[test]
fn an_mz_stub_that_is_not_a_pe_or_is_the_wrong_machine_is_unknown() {
    let mut not_pe = pe_header(0x8664, 0x78);
    not_pe[0x78] = 0;
    assert_eq!(detect_binary_format(&not_pe), F::Unknown);
    assert_eq!(detect_binary_format(&pe_header(0x014c, 0x78)), F::Unknown); // i386
    assert_eq!(detect_binary_format(&pe_header(0xaa64, 0x78)), F::Unknown); // arm64
}

#[test]
fn empty_short_and_garbage_input_is_unknown_never_a_crash() {
    assert_eq!(detect_binary_format(&[]), F::Unknown);
    assert_eq!(detect_binary_format(&[0x7f, 0x45]), F::Unknown);
    assert_eq!(detect_binary_format(&[0u8; 64]), F::Unknown);
    assert_eq!(detect_binary_format(b"#!/bin/sh\nexit 0\n"), F::Unknown);
    // A PE whose header offset points past the bytes read.
    assert_eq!(
        detect_binary_format(&pe_header(0x8664, 0x00ff_fff0)),
        F::Unknown
    );
}

#[test]
fn reads_the_format_off_disk_including_a_0_byte_file() {
    let dir = Scratch::new("fmt");
    std::fs::write(dir.join("arm"), elf_header(0xb7)).unwrap();
    std::fs::write(dir.join("empty"), []).unwrap();
    assert_eq!(read_binary_format(&dir.join("arm")).unwrap(), F::ElfAarch64);
    assert_eq!(read_binary_format(&dir.join("empty")).unwrap(), F::Unknown);
}

// ── the target table ────────────────────────────────────────────────────

#[test]
fn the_table_is_exactly_the_three_non_macos_targets_and_nothing_darwin() {
    let ids: Vec<_> = TARGETS.iter().map(|t| t.id).collect();
    assert_eq!(ids, ["linux-x64", "linux-arm64", "windows-x64"]);
    let triples: Vec<_> = TARGETS.iter().map(|t| t.rust_triple).collect();
    assert_eq!(
        triples,
        [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc"
        ]
    );
    let formats: Vec<_> = TARGETS.iter().map(|t| t.format).collect();
    assert_eq!(formats, [F::ElfX8664, F::ElfAarch64, F::PeX8664]);
    let archives: Vec<_> = TARGETS.iter().map(|t| t.archive_name).collect();
    assert_eq!(
        archives,
        [
            "method-reader-cli-linux-x64.tar.gz",
            "method-reader-cli-linux-arm64.tar.gz",
            "method-reader-cli-windows-x64.zip"
        ]
    );
}

#[test]
fn every_archive_name_is_a_bare_filename_with_no_path() {
    for t in TARGETS.iter().chain(MAC_TARGETS) {
        assert!(!t.archive_name.contains('/') && !t.archive_name.contains('\\'));
    }
}

// ── paths around a target ───────────────────────────────────────────────

#[test]
fn each_target_builds_in_its_own_folder_and_archives_land_flat() {
    let out = Path::new("/tmp/method-reader-out");
    let dirs: Vec<PathBuf> = TARGETS.iter().map(|t| build_dir(t, out)).collect();
    assert_eq!(
        dirs,
        ["linux-x64", "linux-arm64", "windows-x64"].map(|d| out.join(d))
    );
    for t in TARGETS {
        assert_eq!(archive_path(t, out), out.join(t.archive_name));
    }
}

#[test]
fn cargo_names_the_windows_binary_with_exe_and_the_others_without() {
    // Was: the outfile passed to bun never carries .exe. cargo names the
    // file itself; what matters now is where it lands and what it is called.
    let out = Path::new("/tmp/method-reader-out");
    let target_dir = Path::new("/tmp/cargo-target");
    assert_eq!(
        binary_path(t("windows-x64"), out),
        out.join("windows-x64").join("method-reader.exe")
    );
    assert_eq!(
        binary_path(t("linux-x64"), out),
        out.join("linux-x64").join("method-reader")
    );
    assert_eq!(
        cargo_output_path(t("windows-x64"), target_dir),
        target_dir.join("x86_64-pc-windows-msvc/release/method-reader.exe")
    );
    assert_eq!(
        alias_path(t("windows-x64"), out),
        out.join("windows-x64").join("screepub.exe")
    );
    assert_eq!(
        cargo_alias_output_path(t("linux-arm64"), target_dir),
        target_dir.join("aarch64-unknown-linux-gnu/release/screepub")
    );
    assert_eq!(
        cargo_output_path(t("linux-arm64"), target_dir),
        target_dir.join("aarch64-unknown-linux-gnu/release/method-reader")
    );
}

// ── the host ────────────────────────────────────────────────────────────

#[test]
fn maps_the_os_and_arch_pairs_that_matter() {
    assert_eq!(target_id_for_host("linux", "x86_64"), Some("linux-x64"));
    assert_eq!(target_id_for_host("linux", "aarch64"), Some("linux-arm64"));
    assert_eq!(target_id_for_host("windows", "x86_64"), Some("windows-x64"));
    // macOS is not a target of the cross table: "nothing", not a fallback.
    assert_eq!(target_id_for_host("macos", "aarch64"), None);
    assert_eq!(target_id_for_host("macos", "x86_64"), None);
    assert_eq!(target_id_for_host("linux", "x86"), None);
    assert_eq!(target_id_for_host("freebsd", "x86_64"), None);
}

#[test]
fn host_target_agrees_with_the_running_process() {
    let expected = target_id_for_host(std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(host_target().map(|t| t.id), expected);
    if let Some(h) = host_target() {
        assert!(TARGETS.contains(h));
    }
}

// ── parse_build_args ────────────────────────────────────────────────────

#[test]
fn takes_a_version_and_an_output_folder() {
    // A full path by this host's rules ("/tmp/x" has no drive on Windows,
    // so it is resolved against the current one).
    let scratch = Scratch::new("args");
    let out = scratch.join("x");
    let out_arg = out.to_string_lossy().into_owned();
    let a = parse_build_args(&args(&["--version", "0.6.0", "--out", &out_arg])).unwrap();
    assert_eq!(a.version, "0.6.0");
    assert_eq!(a.out_dir, out);
    assert_eq!(a.only, ["linux-x64", "linux-arm64", "windows-x64"]);
}

#[test]
fn accepts_a_tag_spelling_and_normalises_it() {
    let v = |s| {
        parse_build_args(&args(&["--version", s, "--out", "/tmp/x"]))
            .unwrap()
            .version
    };
    assert_eq!(v("v0.6.0"), "0.6.0");
    assert_eq!(v("v0.6.0-rc1"), "0.6.0-rc1");
}

#[test]
fn refuses_a_version_that_is_not_major_minor_patch() {
    for bad in [
        args(&["--out", "/tmp/x"]),
        args(&["--version", "0.6", "--out", "/tmp/x"]),
        args(&["--version", "main", "--out", "/tmp/x"]),
        args(&["--version", "", "--out", "/tmp/x"]),
    ] {
        assert!(parse_build_args(&bad).unwrap_err().contains("--version"));
    }
}

#[test]
fn refuses_to_invent_an_output_folder() {
    let err = parse_build_args(&args(&["--version", "0.6.0"])).unwrap_err();
    assert!(err.contains("--out"), "{err}");
    // The bun build named its real size (450 MB); a Rust build's size is
    // not known until sp-cli exists, so the message says what is left
    // behind instead of a number that would be wrong.
    assert!(err.contains("a binary AND an archive per target"), "{err}");
}

#[test]
fn resolves_the_output_folder_to_an_absolute_path() {
    let a = parse_build_args(&args(&["--version", "0.6.0", "--out", "dist"])).unwrap();
    assert!(a.out_dir.is_absolute());
    assert!(a.out_dir.ends_with("dist"));
}

#[test]
fn only_narrows_the_matrix_and_rejects_a_target_that_does_not_exist() {
    let only = |o| {
        parse_build_args(&args(&[
            "--version",
            "0.6.0",
            "--out",
            "/tmp/x",
            "--only",
            o,
        ]))
        .map(|a| a.only)
    };
    assert_eq!(only("windows-x64").unwrap(), ["windows-x64"]);
    assert_eq!(
        only("linux-x64,windows-x64").unwrap(),
        ["linux-x64", "windows-x64"]
    );
    assert!(only("darwin-arm64").unwrap_err().contains("darwin-arm64"));
    assert!(
        only("linux_x64")
            .unwrap_err()
            .contains("linux-x64, linux-arm64, windows-x64")
    );
}

#[test]
fn rejects_an_unknown_flag_rather_than_ignoring_it() {
    assert!(
        parse_build_args(&args(&[
            "--version",
            "0.6.0",
            "--out",
            "/tmp/x",
            "--sing",
            "x"
        ]))
        .is_err()
    );
}

#[test]
fn a_plain_build_has_no_macos_half() {
    let a = parse_build_args(&args(&["--version", "0.6.0", "--out", "/tmp/x"])).unwrap();
    assert_eq!(a.mac, None);
}

// ── the version check ───────────────────────────────────────────────────

#[test]
fn passes_when_the_workspace_agrees_with_the_requested_version() {
    let ws = workspace_with("0.6.0", true);
    assert_eq!(assert_crate_version("0.6.0", ws.path()), Ok(()));
}

#[test]
fn refuses_to_build_a_binary_that_would_misreport_itself() {
    let ws = workspace_with("0.5.4", true);
    let err = assert_crate_version("0.6.0", ws.path()).unwrap_err();
    // Both numbers, and the file: whoever hits this knows what to change.
    assert!(err.contains("0.5.4") && err.contains("0.6.0"), "{err}");
    assert!(err.contains("Cargo.toml"), "{err}");
}

#[test]
fn a_literal_version_in_sp_clis_own_manifest_is_the_one_checked() {
    let ws = workspace_with("0.6.0", true);
    std::fs::write(
        ws.join("crates/sp-cli/Cargo.toml"),
        "[package]\nname = \"sp-cli\"\nversion = \"0.7.0\"\n",
    )
    .unwrap();
    assert_eq!(assert_crate_version("0.7.0", ws.path()), Ok(()));
    let err = assert_crate_version("0.6.0", ws.path()).unwrap_err();
    assert!(err.contains("rust/crates/sp-cli/Cargo.toml"), "{err}");
}

#[test]
fn the_real_workspace_agrees_with_itself() {
    let ws = paths::workspace_root();
    let version = workspace_version(&ws).unwrap();
    assert_eq!(assert_crate_version(&version, &ws), Ok(()));
}

// ── compile ─────────────────────────────────────────────────────────────

#[test]
fn compile_argv_is_the_exact_cargo_invocation_per_target() {
    assert_eq!(
        compile_argv(t("linux-x64")),
        strings(&[
            "cargo",
            "build",
            "--release",
            "--locked",
            "-p",
            "sp-cli",
            "--bin",
            "method-reader",
            "--bin",
            "screepub",
            "--target",
            "x86_64-unknown-linux-gnu"
        ])
    );
    assert_eq!(
        compile_argv(t("windows-x64")),
        strings(&[
            "cargo",
            "build",
            "--release",
            "--locked",
            "-p",
            "sp-cli",
            "--bin",
            "method-reader",
            "--bin",
            "screepub",
            "--target",
            "x86_64-pc-windows-msvc"
        ])
    );
}

#[test]
fn every_target_names_its_own_triple_and_nothing_else() {
    for target in TARGETS.iter().chain(MAC_TARGETS) {
        let argv = compile_argv(target);
        let at = argv.iter().position(|a| a == "--target").unwrap();
        assert_eq!(argv[at + 1], target.rust_triple);
        assert_eq!(argv.iter().filter(|a| *a == "--target").count(), 1);
        assert!(argv.windows(2).any(|w| w[0] == "-p" && w[1] == "sp-cli"));
    }
}

#[test]
fn runs_cargo_in_the_workspace_and_returns_where_the_binary_was_put() {
    let out = Scratch::new("compile");
    let cargo = Scratch::new("cargo-target");
    let seen: RefCell<Vec<(Vec<String>, PathBuf)>> = RefCell::new(Vec::new());
    let win = t("windows-x64");
    let built = cargo_output_path(win, cargo.path());
    let alias = cargo_alias_output_path(win, cargo.path());
    let spawn = |argv: &[String], cwd: &Path| {
        seen.borrow_mut().push((argv.to_vec(), cwd.to_path_buf()));
        std::fs::create_dir_all(built.parent().unwrap()).unwrap();
        std::fs::write(&built, b"MZ").unwrap();
        std::fs::write(&alias, b"MZ alias").unwrap();
        RunResult::ok("", "")
    };
    let env = test_env(Path::new("/repo/rust"), cargo.path(), &spawn, &no_env, TINY);
    let got = compile_target(win, out.path(), &env).unwrap();
    let seen = seen.borrow().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, PathBuf::from("/repo/rust"));
    assert_eq!(seen[0].0, compile_argv(win));
    assert_eq!(
        got,
        out.path().join("windows-x64").join("method-reader.exe")
    );
    assert_eq!(std::fs::read(&got).unwrap(), b"MZ");
    assert_eq!(
        std::fs::read(out.path().join("windows-x64").join("screepub.exe")).unwrap(),
        b"MZ alias"
    );
}

#[test]
fn a_failed_compile_fails_naming_the_target_and_cargos_stderr() {
    let out = Scratch::new("compile");
    let spawn = |_: &[String], _: &Path| RunResult::failed(101, "error: no such target\n");
    let env = test_env(Path::new("/repo/rust"), out.path(), &spawn, &no_env, TINY);
    let err = compile_target(t("linux-arm64"), out.path(), &env).unwrap_err();
    assert!(
        err.contains("linux-arm64") && err.contains("no such target"),
        "{err}"
    );
}

#[test]
fn a_compile_that_exits_0_without_writing_the_binary_is_still_a_failure() {
    // Ported from tests/build-sidecar.test.ts: trusting the exit code.
    let out = Scratch::new("compile");
    let cargo = Scratch::new("cargo-target");
    let spawn = |_: &[String], _: &Path| RunResult::ok("", "");
    let env = test_env(Path::new("/repo/rust"), cargo.path(), &spawn, &no_env, TINY);
    let err = compile_target(t("linux-x64"), out.path(), &env).unwrap_err();
    assert!(
        err.contains("wrote nothing") && err.contains("x86_64-unknown-linux-gnu"),
        "{err}"
    );
}

/// Why a real build cannot run here, or None when it can.
fn real_build_skip_reason() -> Option<String> {
    if std::env::var("METHOD_READER_REAL_CLI_BUILD").as_deref() != Ok("1") {
        return Some(
            "set METHOD_READER_REAL_CLI_BUILD=1 to run a real cargo build of sp-cli".into(),
        );
    }
    if !paths::workspace_root()
        .join("crates/sp-cli/Cargo.toml")
        .is_file()
    {
        return Some("there is no sp-cli crate in this tree yet".into());
    }
    None
}

/// Runs cargo for real, with its own target folder so it never waits on
/// the lock this test run holds.
fn real_spawn_into(target_dir: PathBuf) -> impl Fn(&[String], &Path) -> RunResult {
    move |argv: &[String], cwd: &Path| {
        let out = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("run cargo");
        RunResult {
            exit_code: out.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&out.stdout).into(),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        }
    }
}

#[test]
fn a_real_build_of_the_host_target_is_the_format_the_table_predicts() {
    if let Some(why) = real_build_skip_reason() {
        eprintln!("skipping a real build: {why}");
        return;
    }
    let Some(target) = host_target().or_else(xtask::cli_build::targets::mac_host_target) else {
        eprintln!("skipping a real build: this machine is no target's host");
        return;
    };
    let out = Scratch::new("realbuild");
    let cargo = out.join("cargo");
    let spawn = real_spawn_into(cargo.clone());
    let env = test_env(
        &paths::workspace_root(),
        &cargo,
        &spawn,
        &no_env,
        RELEASE_FLOORS,
    );
    let bin = compile_target(target, out.path(), &env).unwrap();
    assert_eq!(read_binary_format(&bin).unwrap(), target.format);
}

#[test]
fn a_real_windows_build_writes_method_reader_exe_and_it_is_a_pe() {
    if let Some(why) = real_build_skip_reason() {
        eprintln!("skipping a real build: {why}");
        return;
    }
    if std::env::consts::OS != "windows" {
        eprintln!("skipping a real Windows build: it needs the msvc toolchain of a Windows host");
        return;
    }
    let out = Scratch::new("realbuild");
    let cargo = out.join("cargo");
    let spawn = real_spawn_into(cargo.clone());
    let env = test_env(
        &paths::workspace_root(),
        &cargo,
        &spawn,
        &no_env,
        RELEASE_FLOORS,
    );
    let bin = compile_target(t("windows-x64"), out.path(), &env).unwrap();
    assert!(bin.to_string_lossy().ends_with("method-reader.exe"));
    assert_eq!(read_binary_format(&bin).unwrap(), F::PeX8664);
}

// ── reading an archive back ─────────────────────────────────────────────

/// A tar.gz built by the system tar from files with known contents and
/// modes: two files, one longer than a tar block, so the 512-byte walk is
/// genuinely exercised. None (and a note) where there is no tar.
fn system_tar_gz(dir: &Scratch) -> Option<(PathBuf, Vec<u8>)> {
    let Some(tar) = which("tar") else {
        eprintln!("skipping: no tar on PATH");
        return None;
    };
    let stage = dir.join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    let big: Vec<u8> = (0..1500).map(|i| (i % 251) as u8).collect();
    std::fs::write(stage.join("screepub"), &big).unwrap();
    std::fs::write(stage.join("NOTES"), "plain\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: PathBuf, m| std::fs::set_permissions(p, std::fs::Permissions::from_mode(m));
        mode(stage.join("screepub"), 0o755).unwrap();
        mode(stage.join("NOTES"), 0o644).unwrap();
    }
    let archive = dir.join("a.tar.gz");
    // COPYFILE_DISABLE: macOS's bsdtar would add a hidden ._name member.
    // Relative names from the scratch folder: Windows' tar reads a drive
    // letter in -f as a remote host.
    let status = std::process::Command::new(tar)
        .args(["-czf", "a.tar.gz", "-C", "stage", "screepub", "NOTES"])
        .current_dir(dir.path())
        .env("COPYFILE_DISABLE", "1")
        .status()
        .unwrap();
    assert!(status.success(), "test setup: tar failed");
    Some((archive, big))
}

#[test]
fn tar_gz_entries_reports_every_member_with_its_size_mode_and_bytes() {
    let dir = Scratch::new("tar");
    let Some((archive, big)) = system_tar_gz(&dir) else {
        return;
    };
    let entries = tar_gz_entries(&archive).unwrap();
    let mut names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["NOTES", "screepub"]);
    let bin = entries.iter().find(|e| e.name == "screepub").unwrap();
    assert_eq!(bin.size, 1500);
    assert_eq!(bin.data, big);
    let notes = entries.iter().find(|e| e.name == "NOTES").unwrap();
    assert_eq!(notes.data, b"plain\n");
    if cfg!(unix) {
        assert_ne!(bin.mode & 0o111, 0);
        assert_eq!(notes.mode & 0o111, 0);
    }
}

#[test]
fn archive_entries_dispatches_on_the_extension_and_refuses_anything_else() {
    let dir = Scratch::new("tar");
    write_tar_gz(
        &dir.join("a.tar.gz"),
        &[("a", b"1", 0o644), ("b", b"2", 0o644)],
    )
    .unwrap();
    assert_eq!(archive_entries(&dir.join("a.tar.gz")).unwrap().len(), 2);
    let err = archive_entries(&dir.join("a.rar")).unwrap_err();
    assert!(err.contains(".tar.gz") && err.contains(".zip"), "{err}");
}

#[test]
fn zip_entries_round_trips_a_name_bytes_and_the_executable_bit() {
    use std::io::Write;
    let dir = Scratch::new("zip");
    let payload: Vec<u8> = (0..1500).map(|i| ((i * 7) % 253) as u8).collect();
    let archive = dir.join("a.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755)
        .system(zip::System::Unix);
    zip.start_file("screepub.exe", options).unwrap();
    zip.write_all(&payload).unwrap();
    zip.finish().unwrap();
    let entries = zip_entries(&archive).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "screepub.exe");
    assert_eq!(entries[0].size, 1500);
    assert_eq!(entries[0].data, payload);
    assert_ne!(entries[0].mode & 0o111, 0);
}

// ── package_target ──────────────────────────────────────────────────────

/// A stand-in binary and alias exactly where compile_target leaves them.
fn stage(target: &Target, bytes: &[u8]) -> Scratch {
    let dir = Scratch::new("pack");
    std::fs::create_dir_all(build_dir(target, dir.path())).unwrap();
    std::fs::write(binary_path(target, dir.path()), bytes).unwrap();
    std::fs::write(alias_path(target, dir.path()), bytes).unwrap();
    dir
}

#[test]
fn a_linux_target_becomes_a_tar_gz_holding_the_executable_tool_and_its_alias() {
    let target = t("linux-arm64");
    let dir = stage(target, &elf_payload(3000, 0xb7));
    #[cfg(unix)]
    {
        // NOT executable on disk first: packaging sets the bit.
        use std::os::unix::fs::PermissionsExt;
        let bin = binary_path(target, dir.path());
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let archive = package_target(target, dir.path()).unwrap();
    assert_eq!(archive, dir.join("method-reader-cli-linux-arm64.tar.gz"));
    let entries = archive_entries(&archive).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "method-reader"); // no path prefix
    assert_eq!(entries[1].name, "screepub");
    for e in &entries {
        assert_eq!(e.size, 3000);
        assert_ne!(e.mode & 0o111, 0);
        assert_eq!(detect_binary_format(&e.data), F::ElfAarch64);
    }
}

#[test]
fn the_windows_target_becomes_a_zip_holding_the_tool_and_its_alias() {
    let target = t("windows-x64");
    let dir = stage(target, &payload_for(F::PeX8664, 3000));
    let archive = package_target(target, dir.path()).unwrap();
    assert_eq!(archive, dir.join("method-reader-cli-windows-x64.zip"));
    let entries = archive_entries(&archive).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "method-reader.exe");
    assert_eq!(entries[1].name, "screepub.exe");
    for e in &entries {
        assert_eq!(e.size, 3000);
        assert_eq!(detect_binary_format(&e.data), F::PeX8664);
        assert_ne!(e.mode & 0o111, 0);
    }
}

#[test]
fn re_packaging_replaces_the_archive_instead_of_appending_to_it() {
    let target = t("linux-x64");
    let dir = stage(target, &[0u8; 3000]);
    package_target(target, dir.path()).unwrap();
    package_target(target, dir.path()).unwrap();
    assert_eq!(
        archive_entries(&archive_path(target, dir.path()))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_system_tar_unpacks_the_archive_to_a_runnable_tool_and_alias() {
    // New beside the port: the tar is written here now rather than by a tar
    // program, so a real tar has to agree it is a tar.
    let Some(tar) = which("tar") else {
        eprintln!("skipping: no tar on PATH");
        return;
    };
    let target = t("linux-x64");
    let payload = elf_payload(3000, 0x3e);
    let dir = stage(target, &payload);
    let archive = package_target(target, dir.path()).unwrap();
    assert_eq!(archive, dir.join(target.archive_name));
    let unpack = dir.join("unpack");
    std::fs::create_dir_all(&unpack).unwrap();
    let status = std::process::Command::new(tar)
        .args(["-xzf", target.archive_name, "-C", "unpack"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    let mut names: Vec<_> = std::fs::read_dir(&unpack)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["method-reader", "screepub"]);
    for name in &names {
        assert_eq!(std::fs::read(unpack.join(name)).unwrap(), payload);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(unpack.join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0);
        }
    }
}

// ── verify_artifact ─────────────────────────────────────────────────────

fn built(payload: &[u8]) -> Scratch {
    let target = t("linux-x64");
    let dir = stage(target, payload);
    package_target(target, dir.path()).unwrap();
    dir
}

#[test]
fn a_good_artifact_passes() {
    let dir = built(&elf_payload(3000, 0x3e));
    assert_eq!(verify_artifact(t("linux-x64"), dir.path(), TINY), Ok(()));
}

#[test]
fn a_missing_binary_fails() {
    let dir = built(&elf_payload(3000, 0x3e));
    std::fs::remove_file(binary_path(t("linux-x64"), dir.path())).unwrap();
    let err = verify_artifact(t("linux-x64"), dir.path(), TINY).unwrap_err();
    assert!(err.contains("no binary"), "{err}");
}

#[test]
fn a_0_byte_binary_fails_and_so_does_a_merely_small_one() {
    for payload in [Vec::new(), elf_payload(100, 0x3e)] {
        let dir = built(&elf_payload(3000, 0x3e));
        std::fs::write(binary_path(t("linux-x64"), dir.path()), payload).unwrap();
        let err = verify_artifact(t("linux-x64"), dir.path(), TINY).unwrap_err();
        assert!(err.contains("floor"), "{err}");
    }
}

#[test]
fn the_right_size_but_the_wrong_architecture_fails() {
    let dir = built(&elf_payload(3000, 0xb7)); // aarch64 in the x64 slot
    let err = verify_artifact(t("linux-x64"), dir.path(), TINY).unwrap_err();
    assert!(
        err.contains("elf-aarch64") && err.contains("elf-x86-64"),
        "{err}"
    );
}

#[test]
fn a_shell_script_where_a_binary_should_be_fails() {
    let dir = built(&elf_payload(3000, 0x3e));
    std::fs::write(
        binary_path(t("linux-x64"), dir.path()),
        "#!/bin/sh\n".repeat(200),
    )
    .unwrap();
    let err = verify_artifact(t("linux-x64"), dir.path(), TINY).unwrap_err();
    assert!(err.contains("unknown"), "{err}");
}

#[test]
fn a_missing_or_tiny_archive_fails() {
    let target = t("linux-x64");
    let dir = built(&elf_payload(3000, 0x3e));
    std::fs::remove_file(archive_path(target, dir.path())).unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("no archive"), "{err}");
    std::fs::write(archive_path(target, dir.path()), [0u8; 4]).unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("floor"), "{err}");
}

#[test]
fn an_archive_that_does_not_contain_the_binary_fails_and_says_what_it_holds() {
    let target = t("linux-x64");
    let dir = built(&elf_payload(3000, 0x3e));
    write_tar_gz(
        &archive_path(target, dir.path()),
        &[("READ.ME", &[b'x'; 400], 0o644)],
    )
    .unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("READ.ME"), "{err}");
}

#[test]
fn an_archive_whose_member_is_not_executable_fails() {
    let target = t("linux-x64");
    let payload = elf_payload(3000, 0x3e);
    let dir = built(&payload);
    write_tar_gz(
        &archive_path(target, dir.path()),
        &[
            ("method-reader", &payload, 0o644),
            ("screepub", &payload, 0o755),
        ],
    )
    .unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("not executable"), "{err}");
}

#[test]
fn an_archive_without_the_alias_or_a_missing_alias_fails() {
    let target = t("linux-x64");
    let payload = elf_payload(3000, 0x3e);
    let dir = built(&payload);
    write_tar_gz(
        &archive_path(target, dir.path()),
        &[("method-reader", &payload, 0o755)],
    )
    .unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("does not contain screepub"), "{err}");
    let dir = built(&payload);
    std::fs::remove_file(alias_path(target, dir.path())).unwrap();
    let err = verify_artifact(target, dir.path(), TINY).unwrap_err();
    assert!(err.contains("no alias binary"), "{err}");
}

#[test]
fn the_production_floors_are_set_and_reject_a_toy_binary() {
    // Re-tuned for a Rust binary (see RELEASE_FLOORS): the bun runtime that
    // made every artifact 64-119 MB is gone.
    assert_eq!(RELEASE_FLOORS.binary_bytes, 1_000_000);
    assert_eq!(RELEASE_FLOORS.archive_bytes, 300_000);
    let dir = built(&elf_payload(3000, 0x3e));
    let err = verify_artifact(t("linux-x64"), dir.path(), RELEASE_FLOORS).unwrap_err();
    assert!(err.contains("floor"), "{err}");
}

// ── SHA256SUMS ──────────────────────────────────────────────────────────

fn with_files() -> (Scratch, Vec<String>) {
    let dir = Scratch::new("sums");
    let names = strings(&[
        "method-reader-cli-windows-x64.zip",
        "method-reader-cli-linux-x64.tar.gz",
    ]);
    std::fs::write(dir.join(&names[0]), "windows bytes").unwrap();
    std::fs::write(dir.join(&names[1]), "linux bytes").unwrap();
    (dir, names)
}

#[test]
fn the_digests_are_the_real_ones() {
    let (dir, names) = with_files();
    write_checksums(dir.path(), &names, "SHA256SUMS").unwrap();
    let map = parse_checksums(&std::fs::read_to_string(dir.join("SHA256SUMS")).unwrap());
    for name in &names {
        assert_eq!(map[name], sha256(&std::fs::read(dir.join(name)).unwrap()));
    }
}

#[test]
fn the_format_is_the_one_sha256sum_c_actually_parses() {
    let (dir, names) = with_files();
    let text = write_checksums(dir.path(), &names, "SHA256SUMS").unwrap();
    assert!(text.ends_with('\n'));
    let body: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    assert_eq!(body.len(), 2);
    for line in &body {
        let (hex, name) = line.split_at(64);
        assert!(
            hex.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        let name = name.strip_prefix("  ").expect("two spaces");
        assert!(!name.starts_with(' ') && !name.contains('/') && !name.contains('\\'));
    }
    let mut sorted = names.clone();
    sorted.sort();
    let listed: Vec<&str> = body.iter().map(|l| &l[66..]).collect();
    assert_eq!(listed, sorted);
}

#[test]
fn a_changed_byte_changes_the_digest() {
    let (dir, names) = with_files();
    let before = parse_checksums(&write_checksums(dir.path(), &names, "SHA256SUMS").unwrap());
    std::fs::write(dir.join(&names[0]), "windows bytez").unwrap();
    let after = parse_checksums(&write_checksums(dir.path(), &names, "SHA256SUMS").unwrap());
    assert_ne!(after[&names[0]], before[&names[0]]);
    assert_eq!(after[&names[1]], before[&names[1]]);
}

#[test]
fn a_partial_rebuild_keeps_the_entries_it_did_not_rebuild() {
    let (dir, names) = with_files();
    write_checksums(dir.path(), &names, "SHA256SUMS").unwrap();
    std::fs::write(dir.join(&names[0]), "rebuilt windows bytes").unwrap();
    let map = parse_checksums(&write_checksums(dir.path(), &names[..1], "SHA256SUMS").unwrap());
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut want: Vec<&String> = names.iter().collect();
    want.sort();
    assert_eq!(keys, want);
    // Both re-hashed from the files on disk, never copied out of the old text.
    for name in &names {
        assert_eq!(map[name], sha256(&std::fs::read(dir.join(name)).unwrap()));
    }
}

#[test]
fn an_entry_whose_file_is_gone_is_dropped_rather_than_carried() {
    let (dir, names) = with_files();
    write_checksums(dir.path(), &names, "SHA256SUMS").unwrap();
    std::fs::remove_file(dir.join(&names[1])).unwrap();
    let map = parse_checksums(&write_checksums(dir.path(), &names[..1], "SHA256SUMS").unwrap());
    assert_eq!(map.keys().collect::<Vec<_>>(), [&names[0]]);
}

#[test]
fn the_system_checker_accepts_it_and_rejects_a_tampered_file() {
    let (program, check): (PathBuf, Vec<&str>) = if let Some(p) = which("sha256sum") {
        (p, vec!["-c", "SHA256SUMS"])
    } else if let Some(p) = which("shasum") {
        (p, vec!["-a", "256", "-c", "SHA256SUMS"])
    } else {
        eprintln!("skipping: no sha256sum/shasum on PATH");
        return;
    };
    let (dir, names) = with_files();
    write_checksums(dir.path(), &names, "SHA256SUMS").unwrap();
    let run = || {
        std::process::Command::new(&program)
            .args(&check)
            .current_dir(dir.path())
            .output()
            .unwrap()
            .status
            .success()
    };
    assert!(run());
    std::fs::write(dir.join(&names[0]), "tampered").unwrap();
    assert!(!run());
}

// ── build_all ───────────────────────────────────────────────────────────

/// A cargo that writes a binary of the right format for whatever --target
/// it is given, and a codesign that appends a signature to what it signs
/// and describes it as signed with the hardened runtime by a team.
fn fake_tools(
    cargo_target_dir: PathBuf,
    seen: &RefCell<Vec<Vec<String>>>,
) -> impl Fn(&[String], &Path) -> RunResult + '_ {
    move |argv: &[String], _cwd: &Path| {
        seen.borrow_mut().push(argv.to_vec());
        match argv[0].as_str() {
            "cargo" => {
                let triple = &argv[argv.iter().position(|a| a == "--target").unwrap() + 1];
                let target = TARGETS
                    .iter()
                    .chain(MAC_TARGETS)
                    .find(|t| t.rust_triple == triple)
                    .unwrap();
                let path = cargo_output_path(target, &cargo_target_dir);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, payload_for(target.format, 4000)).unwrap();
                let alias = cargo_alias_output_path(target, &cargo_target_dir);
                std::fs::write(&alias, payload_for(target.format, 4000)).unwrap();
                RunResult::ok("", "")
            }
            "codesign" if argv[1] == "--force" => {
                let bin = argv.last().unwrap();
                let mut bytes = std::fs::read(bin).unwrap();
                bytes.extend_from_slice(b"SIGNATURE");
                std::fs::write(bin, bytes).unwrap();
                RunResult::ok("", "")
            }
            "codesign" if argv[1] == "-dv" => RunResult::ok(
                "",
                "Identifier=method-reader-cli-arm64\nCodeDirectory v=20500 size=1 flags=0x10000(runtime) hashes=1+2\nTeamIdentifier=XSRB3D643J\n",
            ),
            _ => RunResult::ok("", ""),
        }
    }
}

#[test]
fn refuses_to_build_a_version_the_workspace_does_not_name() {
    let ws = workspace_with("0.6.0", true);
    let out = Scratch::new("all");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let a = BuildArgs {
        version: "99.0.0".into(),
        out_dir: out.join("dist"),
        only: strings(&["linux-x64"]),
        mac: None,
    };
    let err = build_all(&a, &env).unwrap_err();
    assert!(err.contains("Cargo.toml"), "{err}");
    // Nothing was compiled: the guard runs before any build.
    assert!(seen.borrow().is_empty());
    assert!(!out.join("dist").exists());
}

#[test]
fn a_missing_sp_cli_crate_is_refused_before_anything_is_compiled() {
    let ws = workspace_with("0.6.0", false);
    let out = Scratch::new("all");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let a = parse_build_args(&args(&[
        "--version",
        "0.6.0",
        "--out",
        &out.join("dist").display().to_string(),
    ]))
    .unwrap();
    let err = build_all(&a, &env).unwrap_err();
    assert!(err.contains("no sp-cli crate"), "{err}");
    assert!(seen.borrow().is_empty());
    assert!(!out.join("dist").exists());
}

#[test]
fn a_whole_cross_build_with_fake_tools_leaves_verified_archives_and_their_sums() {
    let ws = workspace_with("0.6.0", true);
    let out = Scratch::new("all");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let dist = out.join("dist");
    let a = parse_build_args(&args(&[
        "--version",
        "v0.6.0",
        "--out",
        &dist.display().to_string(),
    ]))
    .unwrap();
    let made = build_all(&a, &env).unwrap();
    assert_eq!(
        made,
        TARGETS
            .iter()
            .map(|t| dist.join(t.archive_name))
            .collect::<Vec<_>>()
    );
    // Only cargo ran: a cross build never reaches codesign.
    assert!(seen.borrow().iter().all(|argv| argv[0] == "cargo"));
    let sums = parse_checksums(&std::fs::read_to_string(dist.join("SHA256SUMS")).unwrap());
    assert_eq!(sums.len(), 3);
    for (name, digest) in &sums {
        assert_eq!(digest, &sha256(&std::fs::read(dist.join(name)).unwrap()));
    }
}

// ── the command line ────────────────────────────────────────────────────

#[test]
fn a_missing_version_fails_with_a_message_and_a_non_zero_exit() {
    let scratch = Scratch::new("cmd");
    let never = scratch.join("never");
    let code = xtask::cli_build::build_cli_main(&args(&["--out", &never.display().to_string()]));
    assert_ne!(code, 0);
    assert!(!never.exists());
}

#[test]
fn an_unknown_target_is_refused_before_anything_is_compiled() {
    let scratch = Scratch::new("cmd");
    let never = scratch.join("never");
    let argv = args(&[
        "--version",
        "0.0.1",
        "--out",
        &never.display().to_string(),
        "--only",
        "darwin-arm64",
    ]);
    assert!(
        parse_build_args(&argv)
            .unwrap_err()
            .contains("darwin-arm64")
    );
    assert_ne!(xtask::cli_build::build_cli_main(&argv), 0);
    assert!(!never.exists());
}

#[test]
fn gitignore_covers_the_obvious_hand_run_output_folders() {
    if which("git").is_none() {
        eprintln!("skipping: no git on PATH");
        return;
    }
    for path in [
        "dist/method-reader-cli-linux-x64.tar.gz",
        "dist/SHA256SUMS",
        "build/method-reader-cli-windows-x64.zip",
    ] {
        let ignored = std::process::Command::new("git")
            .args(["check-ignore", "-q", path])
            .current_dir(paths::repo_root())
            .status()
            .unwrap()
            .success();
        assert!(ignored, "{path} is not ignored");
    }
}

// ── the macOS half ──────────────────────────────────────────────────────

#[test]
fn the_mac_table_is_exactly_one_row_apple_silicon_in_the_formulas_own_name() {
    assert_eq!(MAC_TARGETS.len(), 1);
    let m = &MAC_TARGETS[0];
    assert_eq!(
        (
            m.id,
            m.rust_triple,
            m.format,
            m.binary_name,
            m.alias_name,
            m.archive_name
        ),
        (
            "macos-arm64",
            "aarch64-apple-darwin",
            F::MachoArm64,
            "method-reader",
            "screepub",
            "method-reader-cli-macos-arm64.tar.gz"
        )
    );
    assert_eq!(m.packaging, xtask::cli_build::targets::Packaging::TarGz);
    // The name is the tap's: its bump tool (the bump_tap module, which
    // was tools/bump-tap.sh) writes it into the formula.
    assert_eq!(xtask::check_tap::ARM, m.archive_name);
    assert!(xtask::bump_tap::formula("o/r", "1.0.0", &"a".repeat(64)).contains(m.archive_name));
}

#[test]
fn an_intel_binary_is_named_for_what_it_is_and_refused_by_the_mac_row() {
    let intel = macho_header(0x0100_0007);
    assert_eq!(detect_binary_format(&intel), F::MachoX8664);
    assert_ne!(detect_binary_format(&intel), MAC_TARGETS[0].format);
}

#[test]
fn shares_no_id_folder_or_archive_with_the_cross_targets() {
    let o = Path::new("/o");
    for m in MAC_TARGETS {
        for c in TARGETS {
            assert_ne!(m.id, c.id);
            assert_ne!(m.archive_name, c.archive_name);
            assert_ne!(build_dir(m, o), build_dir(c, o));
            assert_ne!(m.rust_triple, c.rust_triple);
        }
    }
}

#[test]
fn mac_target_id_for_host_names_the_mac_this_is_and_nothing_else() {
    assert_eq!(
        mac_target_id_for_host("macos", "aarch64"),
        Some("macos-arm64")
    );
    assert_eq!(mac_target_id_for_host("macos", "x86_64"), None);
    assert_eq!(mac_target_id_for_host("linux", "aarch64"), None);
    assert_eq!(mac_target_id_for_host("windows", "x86_64"), None);
    assert_eq!(target_id_for_host("macos", "aarch64"), None);
}

const ID: &str = "Developer ID Application: Darkwell Entertainment LLC (XSRB3D643J)";

fn mac_args(extra: &[&str]) -> Result<BuildArgs, String> {
    let mut a = args(&["--version", "0.6.0", "--out", "/tmp/x"]);
    a.extend(strings(extra));
    parse_build_args(&a)
}

#[test]
fn mac_sign_builds_the_one_mac_target_signed_not_notarized() {
    let a = mac_args(&["--mac", "--sign", ID]).unwrap();
    assert_eq!(a.only, ["macos-arm64"]);
    assert_eq!(
        a.mac,
        Some(MacSigning {
            sign: Some(ID.into()),
            notarize: false
        })
    );
}

#[test]
fn notarize_rides_on_sign() {
    let a = mac_args(&["--mac", "--sign", ID, "--notarize"]).unwrap();
    assert_eq!(
        a.mac,
        Some(MacSigning {
            sign: Some(ID.into()),
            notarize: true
        })
    );
}

#[test]
fn mac_unsigned_is_allowed_and_says_so_in_the_args() {
    let a = mac_args(&["--mac", "--unsigned"]).unwrap();
    assert_eq!(
        a.mac,
        Some(MacSigning {
            sign: None,
            notarize: false
        })
    );
}

#[test]
fn mac_must_say_how_it_is_signed_neither_or_both_is_refused() {
    let err = mac_args(&["--mac"]).unwrap_err();
    assert!(
        err.contains("--sign") && err.contains("--unsigned"),
        "{err}"
    );
    assert!(
        mac_args(&["--mac", "--sign", ID, "--unsigned"])
            .unwrap_err()
            .contains("both")
    );
}

#[test]
fn an_unsigned_binary_cannot_be_notarized_so_the_pair_is_refused() {
    assert!(
        mac_args(&["--mac", "--unsigned", "--notarize"])
            .unwrap_err()
            .contains("notariz")
    );
}

#[test]
fn the_signing_flags_mean_nothing_without_mac_and_are_refused() {
    for extra in [&["--sign", ID][..], &["--unsigned"], &["--notarize"]] {
        assert!(mac_args(extra).unwrap_err().contains("--mac"));
    }
}

#[test]
fn only_stays_inside_its_own_table_in_both_directions() {
    assert_eq!(
        mac_args(&["--mac", "--unsigned", "--only", "macos-arm64"])
            .unwrap()
            .only,
        ["macos-arm64"]
    );
    assert!(
        mac_args(&["--only", "macos-arm64"])
            .unwrap_err()
            .contains("--mac")
    );
    assert!(
        mac_args(&["--mac", "--unsigned", "--only", "linux-x64"])
            .unwrap_err()
            .ends_with("known targets are macos-arm64")
    );
}

#[test]
fn the_intel_mac_target_is_gone_with_or_without_mac() {
    let err = mac_args(&["--mac", "--unsigned", "--only", "macos-x64"]).unwrap_err();
    assert!(err.contains("unknown target \"macos-x64\""), "{err}");
    assert!(
        mac_args(&["--only", "macos-x64"])
            .unwrap_err()
            .contains("macos-x64")
    );
}

#[test]
fn the_rust_cli_is_signed_with_no_entitlements_and_the_bun_era_file_is_gone() {
    // Was: the entitlements are exactly the three the shipped CLI has
    // always carried (allow-jit, allow-unsigned-executable-memory,
    // disable-library-validation). bun's JIT needed them; a Rust binary
    // needs none, so the hardened runtime is no longer relaxed at all.
    let argv = sign_argv(arm(), Path::new("/tmp/out"), ID);
    assert!(!argv.iter().any(|a| a.contains("entitlements")), "{argv:?}");
    assert!(
        !paths::repo_root()
            .join("tools/macos/screepub-cli.entitlements")
            .exists(),
        "the bun-era entitlements file is back; nothing signs with it"
    );
}

// ── signing ─────────────────────────────────────────────────────────────

const GOOD_DESCRIBE: &str = "Executable=/tmp/out/macos-arm64/method-reader\nIdentifier=method-reader-cli-arm64\nCodeDirectory v=20500 size=1 flags=0x10000(runtime) hashes=1+2 location=embedded\nTeamIdentifier=XSRB3D643J\n";

/// A codesign that answers by its first two words, recording every call.
fn recorder<'a>(
    seen: &'a RefCell<Vec<Vec<String>>>,
    answers: &'a [(&'a str, RunResult)],
) -> impl Fn(&[String], &Path) -> RunResult + 'a {
    move |argv: &[String], _: &Path| {
        seen.borrow_mut().push(argv.to_vec());
        let key = argv[..2.min(argv.len())].join(" ");
        if let Some((_, r)) = answers.iter().find(|(k, _)| *k == key) {
            return r.clone();
        }
        RunResult::ok(
            "",
            if key == "codesign -dv" {
                GOOD_DESCRIBE
            } else {
                ""
            },
        )
    }
}

fn arm() -> &'static Target {
    t("macos-arm64")
}

fn sign(identity: &str, spawn: &dyn Fn(&[String], &Path) -> RunResult) -> Result<(), String> {
    sign_target(
        arm(),
        Path::new("/tmp/out"),
        identity,
        spawn,
        Path::new("/repo"),
    )
}

#[test]
fn sign_argv_is_the_exact_codesign_invocation_the_shipped_cli_was_signed_with() {
    assert_eq!(
        sign_argv(arm(), Path::new("/tmp/out"), ID),
        strings(&[
            "codesign",
            "--force",
            "--options",
            "runtime",
            "--timestamp",
            "--identifier",
            "method-reader-cli-arm64",
            "--sign",
            ID,
            &Path::new("/tmp/out")
                .join("macos-arm64")
                .join("method-reader")
                .display()
                .to_string(),
        ])
    );
    // The alias keeps the identifier the CLI was signed with up to 0.12.x.
    assert_eq!(
        sign_alias_argv(arm(), Path::new("/tmp/out"), ID),
        strings(&[
            "codesign",
            "--force",
            "--options",
            "runtime",
            "--timestamp",
            "--identifier",
            "screepub-cli-arm64",
            "--sign",
            ID,
            &Path::new("/tmp/out")
                .join("macos-arm64")
                .join("screepub")
                .display()
                .to_string(),
        ])
    );
}

#[test]
fn signs_then_verifies_then_reads_back_the_flags_in_that_order() {
    let seen = RefCell::new(Vec::new());
    let spawn = recorder(&seen, &[]);
    sign(ID, &spawn).unwrap();
    let seen = seen.borrow().clone();
    let path = |name: &str| {
        Path::new("/tmp/out")
            .join("macos-arm64")
            .join(name)
            .display()
            .to_string()
    };
    // The tool, then its alias, each signed, verified and read back.
    assert_eq!(seen.len(), 6);
    for (at, argv, bin) in [
        (
            0,
            sign_argv(arm(), Path::new("/tmp/out"), ID),
            path("method-reader"),
        ),
        (
            3,
            sign_alias_argv(arm(), Path::new("/tmp/out"), ID),
            path("screepub"),
        ),
    ] {
        assert_eq!(seen[at], argv);
        assert_eq!(
            seen[at + 1],
            strings(&["codesign", "--verify", "--strict", "--verbose=2", &bin])
        );
        assert_eq!(seen[at + 2], strings(&["codesign", "-dv", &bin]));
    }
}

#[test]
fn a_failed_sign_fails_naming_the_target_and_codesigns_words() {
    let seen = RefCell::new(Vec::new());
    let answers = [(
        "codesign --force",
        RunResult::failed(1, "no identity found\n"),
    )];
    let spawn = recorder(&seen, &answers);
    let err = sign(ID, &spawn).unwrap_err();
    assert!(
        err.contains("macos-arm64") && err.contains("no identity found"),
        "{err}"
    );
}

#[test]
fn a_signature_that_does_not_verify_fails() {
    let seen = RefCell::new(Vec::new());
    let answers = [(
        "codesign --verify",
        RunResult::failed(3, "invalid signature\n"),
    )];
    let spawn = recorder(&seen, &answers);
    assert!(sign(ID, &spawn).unwrap_err().contains("invalid signature"));
}

#[test]
fn a_signature_without_the_hardened_runtime_is_refused() {
    let seen = RefCell::new(Vec::new());
    let no_runtime = GOOD_DESCRIBE.replace("0x10000(runtime)", "0x0(none)");
    let answers = [("codesign -dv", RunResult::ok("", &no_runtime))];
    let spawn = recorder(&seen, &answers);
    assert!(sign(ID, &spawn).unwrap_err().contains("hardened runtime"));
}

#[test]
fn a_real_identity_must_leave_a_team_on_the_binary_ad_hoc_may_not() {
    let seen = RefCell::new(Vec::new());
    let adhoc = GOOD_DESCRIBE.replace("TeamIdentifier=XSRB3D643J", "TeamIdentifier=not set");
    let answers = [("codesign -dv", RunResult::ok("", &adhoc))];
    let spawn = recorder(&seen, &answers);
    assert!(
        sign(ID, &spawn)
            .unwrap_err()
            .to_lowercase()
            .contains("team")
    );
    assert_eq!(sign("-", &spawn), Ok(()));
}

#[test]
fn refuses_to_sign_anything_that_is_not_a_mac_target() {
    let seen = RefCell::new(Vec::new());
    let spawn = recorder(&seen, &[]);
    let err = sign_target(
        t("linux-x64"),
        Path::new("/tmp/out"),
        ID,
        &spawn,
        Path::new("/repo"),
    )
    .unwrap_err();
    assert!(err.contains("linux-x64"), "{err}");
    assert!(seen.borrow().is_empty());
}

// ── notarizing ──────────────────────────────────────────────────────────

fn creds() -> NotaryCredentials {
    NotaryCredentials {
        key_path: "/k/ac.p8".into(),
        key_id: "KEYID".into(),
        issuer: "ISSUER".into(),
    }
}

#[test]
fn notary_credentials_reads_the_three_variables_the_release_job_sets() {
    let env = |n: &str| match n {
        "AC_API_KEY_PATH" => Some("/k/ac.p8".to_string()),
        "AC_API_KEY_ID" => Some("KEYID".to_string()),
        "AC_API_ISSUER_ID" => Some("ISSUER".to_string()),
        _ => None,
    };
    assert_eq!(notary_credentials(&env), Ok(creds()));
}

#[test]
fn and_names_exactly_the_ones_that_are_missing() {
    let only_id = |n: &str| (n == "AC_API_KEY_ID").then(|| "KEYID".to_string());
    let err = notary_credentials(&only_id).unwrap_err();
    assert!(
        err.contains("AC_API_KEY_PATH") && err.contains("AC_API_ISSUER_ID"),
        "{err}"
    );
    assert!(!err.contains("AC_API_KEY_ID"), "{err}");
    let empty_path = |n: &str| {
        Some(if n == "AC_API_KEY_PATH" {
            String::new()
        } else {
            "x".into()
        })
    };
    assert!(
        notary_credentials(&empty_path)
            .unwrap_err()
            .contains("AC_API_KEY_PATH")
    );
}

/// ditto makes the zip it would have made; xcrun answers `answer`.
fn notary_tools<'a>(
    seen: &'a RefCell<Vec<Vec<String>>>,
    answer: RunResult,
) -> impl Fn(&[String], &Path) -> RunResult + 'a {
    move |argv: &[String], _: &Path| {
        seen.borrow_mut().push(argv.to_vec());
        if argv[0] == "ditto" {
            std::fs::write(argv.last().unwrap(), "zip").unwrap();
            return RunResult::ok("", "");
        }
        answer.clone()
    }
}

#[test]
fn zips_the_binary_submits_the_zip_and_waits_then_removes_the_zip() {
    let dir = stage(arm(), b"signed");
    let seen = RefCell::new(Vec::new());
    let accepted = RunResult::ok(
        r#"{"id":"2efe2717-52ef-43a5-96dc-0797e4ca1041","message":"Processing complete","status":"Accepted"}"#,
        "",
    );
    let spawn = notary_tools(&seen, accepted);
    notarize_target(arm(), dir.path(), &creds(), &spawn, Path::new("/repo")).unwrap();
    let zip = dir
        .path()
        .join("macos-arm64")
        .join("method-reader-notarize.zip");
    let zip_s = zip.display().to_string();
    // One folder holding the tool and its alias: one submission for both.
    let folder = dir.path().join("macos-arm64").join("notarize");
    assert_eq!(
        seen.borrow().clone(),
        vec![
            strings(&["ditto", "-c", "-k", &folder.display().to_string(), &zip_s]),
            strings(&[
                "xcrun",
                "notarytool",
                "submit",
                &zip_s,
                "--key",
                "/k/ac.p8",
                "--key-id",
                "KEYID",
                "--issuer",
                "ISSUER",
                "--output-format",
                "json",
                "--wait"
            ]),
        ]
    );
    assert!(!zip.exists());
    assert!(!folder.exists());
}

#[test]
fn an_exit_of_0_with_a_status_other_than_accepted_is_a_failure_with_the_id_to_look_up() {
    let dir = stage(arm(), b"signed");
    let seen = RefCell::new(Vec::new());
    let invalid = notary_tools(
        &seen,
        RunResult::ok(
            r#"{"id":"abc-123","message":"Processing complete","status":"Invalid"}"#,
            "",
        ),
    );
    let err =
        notarize_target(arm(), dir.path(), &creds(), &invalid, Path::new("/repo")).unwrap_err();
    assert!(
        err.contains("Invalid") && err.contains("abc-123") && err.contains("notarytool log"),
        "{err}"
    );
    for answer in ["Processing complete", ""] {
        let spawn = notary_tools(&seen, RunResult::ok(answer, ""));
        let err =
            notarize_target(arm(), dir.path(), &creds(), &spawn, Path::new("/repo")).unwrap_err();
        assert!(err.contains("macos-arm64"), "{err}");
    }
    assert!(
        !dir.path()
            .join("macos-arm64/method-reader-notarize.zip")
            .exists()
    );
}

#[test]
fn a_rejected_submission_fails_names_the_target_and_still_removes_the_zip() {
    let dir = stage(arm(), b"signed");
    let seen = RefCell::new(Vec::new());
    let spawn = notary_tools(&seen, RunResult::failed(1, "status: Invalid\n"));
    let err = notarize_target(arm(), dir.path(), &creds(), &spawn, Path::new("/repo")).unwrap_err();
    assert!(
        err.contains("macos-arm64") && err.contains("Invalid"),
        "{err}"
    );
    assert!(
        !dir.path()
            .join("macos-arm64/method-reader-notarize.zip")
            .exists()
    );
}

// ── build_all with --mac ────────────────────────────────────────────────

#[test]
fn a_notarizing_build_checks_its_credentials_before_compiling_anything() {
    let ws = workspace_with("0.6.0", true);
    let out = Scratch::new("all-mac");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let a = BuildArgs {
        version: "0.6.0".into(),
        out_dir: out.join("dist"),
        only: strings(&["macos-arm64"]),
        mac: Some(MacSigning {
            sign: Some("Developer ID Application: X (T)".into()),
            notarize: true,
        }),
    };
    assert!(build_all(&a, &env).unwrap_err().contains("AC_API_KEY_PATH"));
    assert!(seen.borrow().is_empty());
    assert!(!build_dir(arm(), &out.join("dist")).exists());
}

#[test]
fn a_mac_id_handed_to_build_all_without_mac_is_refused_not_silently_skipped() {
    let ws = workspace_with("0.6.0", true);
    let out = Scratch::new("all-mac");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let a = BuildArgs {
        version: "0.6.0".into(),
        out_dir: out.join("dist"),
        only: strings(&["macos-arm64"]),
        mac: None,
    };
    assert!(build_all(&a, &env).unwrap_err().contains("--mac"));
}

#[test]
fn a_mac_build_signs_before_packaging_so_the_archive_holds_the_signed_bytes() {
    let ws = workspace_with("0.6.0", true);
    let out = Scratch::new("all-mac");
    let seen = RefCell::new(Vec::new());
    let spawn = fake_tools(out.join("cargo"), &seen);
    let env = test_env(ws.path(), &out.join("cargo"), &spawn, &no_env, TINY);
    let dist = out.join("dist");
    let a = BuildArgs {
        version: "0.6.0".into(),
        out_dir: dist.clone(),
        only: strings(&["macos-arm64"]),
        mac: Some(MacSigning {
            sign: Some(ID.into()),
            notarize: false,
        }),
    };
    build_all(&a, &env).unwrap();
    let bin = std::fs::read(binary_path(arm(), &dist)).unwrap();
    assert!(bin.ends_with(b"SIGNATURE"));
    let alias = std::fs::read(alias_path(arm(), &dist)).unwrap();
    assert!(alias.ends_with(b"SIGNATURE"));
    let entries = archive_entries(&archive_path(arm(), &dist)).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(sha256(&entries[0].data), sha256(&bin));
    assert_eq!(sha256(&entries[1].data), sha256(&alias));
    let order: Vec<String> = seen.borrow().iter().map(|a| a[..2].join(" ")).collect();
    assert_eq!(
        order,
        [
            "cargo build",
            "codesign --force",
            "codesign --verify",
            "codesign -dv",
            "codesign --force",
            "codesign --verify",
            "codesign -dv"
        ]
    );
}
