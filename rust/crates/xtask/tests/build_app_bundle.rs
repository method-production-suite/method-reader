//! Port of tests/build-app-bundle.test.ts (72 checks): the bundle matrix,
//! discovery, verification, the argument parser, whole runs against fake
//! outside tools, the updater archive, and Apple silicon only. What the
//! native window's bundles hold is in tests/native_bundle.rs.
//!
//! Retired with the Tauri window at the Rust cutover: the `cargo tauri
//! build` invocation (build, never bundle; the pinned --bundles list; the
//! --target and --config pass-through; the updater overlay), the version
//! gate over package.json, the Tauri Cargo.toml and tauri.conf.json (the
//! native gate over rust/Cargo.toml is in tests/native_bundle.rs), the
//! Tauri bundle floor, and the checks of a signature tauri-cli left beside
//! the archive (the native path signs it itself and checks it against the
//! key the app trusts before writing it: tests/native_bundle.rs). The whole
//! runs that drove a fake `cargo tauri build` drive the native path's fake
//! tools now.

mod packaging_common;

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use packaging_common::*;
use xtask::packaging::bundle::{
    BundleArgs, Env, build_bundles, cargo_package_version, parse_bundle_args,
};
use xtask::packaging::kinds::{
    BUNDLE_KINDS, BundleArch, BundleOs, Kind, MAC_ARCH, MAC_TARGET, MagicAt, NATIVE_FLOOR,
    UPDATER_KINDS, arch_of, bundle_dir_for, discover_artifact, kinds_for_os, platform_keys,
    released_name, updater_kinds_for_os, verify_bundle_file,
};
use xtask::packaging::native::binary_path;
use xtask::packaging::util::{RunResult, parse_checksums, sha256_file};

fn kind(id: &str) -> Kind {
    *BUNDLE_KINDS.iter().find(|k| k.id == id).unwrap()
}

fn app_tar() -> Kind {
    UPDATER_KINDS[0]
}

const ARCHES: [BundleArch; 2] = [BundleArch::X64, BundleArch::Arm64];

/// A file of `size` zero bytes, made sparse where the file system allows.
fn hollow(path: &Path, size: u64) -> File {
    let f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap();
    f.set_len(size).unwrap();
    f
}

fn write_at(f: &mut File, at: u64, bytes: &[u8]) {
    f.seek(SeekFrom::Start(at)).unwrap();
    f.write_all(bytes).unwrap();
}

/// A file that passes `k`'s magic check and clears its floor.
fn plausible(path: &Path, k: &Kind) {
    let size = k.floor_bytes + 1024;
    let mut f = hollow(path, size);
    let at = if k.magic_at == MagicAt::Head {
        0
    } else {
        size - 512
    };
    write_at(&mut f, at, k.magic);
}

fn base_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

// ---- the bundle matrix ----

#[test]
fn it_is_exactly_the_four_kinds_the_design_shipped_and_no_appimage() {
    let mut ids: Vec<&str> = BUNDLE_KINDS.iter().map(|k| k.id).collect();
    ids.sort();
    assert_eq!(ids, ["deb", "dmg", "nsis", "rpm"]);
    assert!(!BUNDLE_KINDS.iter().any(|k| k.id.contains("appimage")));
    assert!(!BUNDLE_KINDS.iter().any(|k| k.ext == ".msi"));
}

#[test]
fn every_os_gets_the_kinds_the_design_assigned_it_and_only_those() {
    let ids = |os| kinds_for_os(os).iter().map(|k| k.id).collect::<Vec<_>>();
    assert_eq!(ids(BundleOs::Linux), ["deb", "rpm"]);
    assert_eq!(ids(BundleOs::Macos), ["dmg"]);
    assert_eq!(ids(BundleOs::Windows), ["nsis"]);
}

#[test]
fn every_row_looks_in_the_bundler_subdirectory_its_own_id_names() {
    for k in BUNDLE_KINDS {
        assert_eq!(k.dir, k.id);
        assert!(
            bundle_dir_for(k.dir, None, Path::new("d")).ends_with(Path::new("bundle").join(k.id))
        );
    }
}

#[test]
fn the_published_names_carry_no_version_and_say_which_machine_they_are_for() {
    assert_eq!(
        released_name(&kind("deb"), "0.6.0", BundleArch::X64),
        "Method-Reader-linux-amd64.deb"
    );
    assert_eq!(
        released_name(&kind("deb"), "0.6.0", BundleArch::Arm64),
        "Method-Reader-linux-arm64.deb"
    );
    assert_eq!(
        released_name(&kind("rpm"), "0.6.0", BundleArch::X64),
        "Method-Reader-linux-x86_64.rpm"
    );
    assert_eq!(
        released_name(&kind("rpm"), "0.6.0", BundleArch::Arm64),
        "Method-Reader-linux-aarch64.rpm"
    );
    assert_eq!(
        released_name(&kind("nsis"), "0.6.0", BundleArch::X64),
        "Method-Reader-windows-x64-setup.exe"
    );
    assert_eq!(
        released_name(&kind("dmg"), "0.6.0", BundleArch::Arm64),
        "Method-Reader-macOS.dmg"
    );
    assert_eq!(MAC_ARCH, BundleArch::Arm64);
}

fn has_version_number(name: &str) -> bool {
    let b = name.as_bytes();
    (0..b.len()).any(|i| {
        let rest = &name[i..];
        let mut parts = rest.splitn(3, '.');
        let digits =
            |s: &str| !s.is_empty() && s.bytes().take_while(u8::is_ascii_digit).count() > 0;
        match (parts.next(), parts.next(), parts.next()) {
            (Some(a), Some(m), Some(c)) => {
                a.bytes().all(|x| x.is_ascii_digit())
                    && !a.is_empty()
                    && m.bytes().all(|x| x.is_ascii_digit())
                    && !m.is_empty()
                    && digits(c)
            }
            _ => false,
        }
    })
}

#[test]
fn no_published_name_changes_when_the_version_does() {
    for k in BUNDLE_KINDS {
        for arch in ARCHES {
            let name = released_name(&k, "0.6.0", arch);
            assert_eq!(released_name(&k, "9.12.3-rc1", arch), name);
            assert!(!has_version_number(&name), "{name}");
        }
    }
}

#[test]
fn every_published_name_ends_in_the_extension_its_own_row_declares() {
    for k in BUNDLE_KINDS {
        for arch in ARCHES {
            assert!(released_name(&k, "0.6.0", arch).ends_with(k.ext));
        }
    }
}

#[test]
fn the_arch_bearing_names_change_when_the_arch_changes() {
    for id in ["deb", "rpm"] {
        assert_ne!(
            released_name(&kind(id), "0.6.0", BundleArch::X64),
            released_name(&kind(id), "0.6.0", BundleArch::Arm64)
        );
    }
    for id in ["dmg", "nsis"] {
        assert_eq!(
            released_name(&kind(id), "0.6.0", BundleArch::X64),
            released_name(&kind(id), "0.6.0", BundleArch::Arm64)
        );
    }
}

#[test]
fn no_two_rows_can_ever_produce_the_same_published_filename() {
    let names: Vec<String> = BUNDLE_KINDS
        .iter()
        .flat_map(|k| ARCHES.map(|a| released_name(k, "0.6.0", a)))
        .collect();
    let unique: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len() - 2);
}

// ---- where the bundler writes ----

#[test]
fn the_bundle_directory_moves_under_the_triple_when_one_is_given() {
    let root = Path::new("d");
    assert!(bundle_dir_for("deb", None, root).ends_with(Path::new("target/release/bundle/deb")));
    let targeted = bundle_dir_for("dmg", Some("aarch64-apple-darwin"), root);
    assert!(targeted.ends_with(Path::new("target/aarch64-apple-darwin/release/bundle/dmg")));
    assert_ne!(targeted, bundle_dir_for("dmg", None, root));
}

// ---- finding the file the bundler wrote ----

#[test]
fn it_returns_the_one_artifact_with_the_right_extension() {
    let s = Scratch::new("bundle-find");
    let dir = s.join("find-one");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Screepub_0.6.0_aarch64.dmg"), "x").unwrap();
    fs::write(dir.join("Screepub_0.6.0_aarch64.dmg.sig"), "x").unwrap();
    assert_eq!(
        discover_artifact(&dir, &kind("dmg")).unwrap(),
        dir.join("Screepub_0.6.0_aarch64.dmg")
    );
}

#[test]
fn it_ignores_the_temporary_image_bundle_dmg_leaves_behind_on_a_crash() {
    let s = Scratch::new("bundle-find");
    let dir = s.join("find-rw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rw.4711.Screepub_0.6.0_x64.dmg"), "x").unwrap();
    fs::write(dir.join("Screepub_0.6.0_x64.dmg"), "x").unwrap();
    assert_eq!(
        discover_artifact(&dir, &kind("dmg")).unwrap(),
        dir.join("Screepub_0.6.0_x64.dmg")
    );
}

#[test]
fn a_directory_holding_only_the_leftover_temporary_image_is_still_a_failure() {
    let s = Scratch::new("bundle-find");
    let dir = s.join("find-only-rw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("rw.4711.Screepub_0.6.0_x64.dmg"), "x").unwrap();
    err_contains(discover_artifact(&dir, &kind("dmg")), "find-only-rw");
}

#[test]
fn two_candidates_is_an_error_that_names_both() {
    let s = Scratch::new("bundle-find");
    let dir = s.join("find-two");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Screepub_0.5.4_arm64.deb"), "x").unwrap();
    fs::write(dir.join("Screepub_0.6.0_arm64.deb"), "x").unwrap();
    let e = err_contains(discover_artifact(&dir, &kind("deb")), "0.5.4");
    assert!(e.contains("0.6.0"));
}

#[test]
fn no_candidate_names_the_directory_it_looked_in() {
    let s = Scratch::new("bundle-find");
    let dir = s.join("find-none");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("notes.txt"), "x").unwrap();
    err_contains(discover_artifact(&dir, &kind("deb")), "find-none");
}

#[test]
fn a_missing_directory_is_a_clear_message_not_an_enoent_stack() {
    let s = Scratch::new("bundle-find");
    err_contains(
        discover_artifact(&s.join("never-made"), &kind("rpm")),
        "never-made",
    );
}

// ---- verifying what came out ----

#[test]
fn a_plausible_artifact_of_every_kind_passes() {
    let s = Scratch::new("bundle-verify");
    for k in BUNDLE_KINDS {
        let path = s.join(&format!("good{}", k.ext));
        plausible(&path, &k);
        verify_bundle_file(&path, &k).unwrap();
    }
}

#[test]
fn an_undersized_artifact_is_rejected_for_every_kind() {
    let s = Scratch::new("bundle-verify");
    for k in BUNDLE_KINDS {
        let path = s.join(&format!("small{}", k.ext));
        let mut f = hollow(&path, 2048);
        write_at(
            &mut f,
            if k.magic_at == MagicAt::Head {
                0
            } else {
                2048 - 512
            },
            k.magic,
        );
        drop(f);
        err_contains(verify_bundle_file(&path, &k), "floor");
    }
}

#[test]
fn an_empty_file_is_rejected_for_every_kind() {
    let s = Scratch::new("bundle-verify");
    for k in BUNDLE_KINDS {
        let path = s.join(&format!("empty{}", k.ext));
        fs::write(&path, b"").unwrap();
        err_contains(verify_bundle_file(&path, &k), "floor");
    }
}

#[test]
fn the_wrong_container_is_rejected_for_every_kind() {
    let s = Scratch::new("bundle-verify");
    for k in BUNDLE_KINDS {
        let path = s.join(&format!("wrong{}", k.ext));
        drop(hollow(&path, k.floor_bytes + 1024));
        err_contains(verify_bundle_file(&path, &k), "magic");
    }
}

#[test]
fn one_kind_rejects_another_kind_of_real_container() {
    let s = Scratch::new("bundle-verify");
    for k in BUNDLE_KINDS {
        let path = s.join(&format!("cross{}", k.ext));
        for other in BUNDLE_KINDS.iter().filter(|o| o.id != k.id) {
            plausible(&path, other);
            err_contains(verify_bundle_file(&path, &k), "magic");
        }
    }
}

#[test]
fn the_dmg_check_reads_the_trailer_and_not_the_head() {
    let s = Scratch::new("bundle-verify");
    let k = kind("dmg");
    assert_eq!(k.magic_at, MagicAt::UdifTrailer);
    let path = s.join("headmagic.dmg");
    let mut f = hollow(&path, k.floor_bytes + 1024);
    write_at(&mut f, 0, k.magic);
    drop(f);
    err_contains(verify_bundle_file(&path, &k), "magic");
}

#[test]
fn the_koly_block_must_start_the_trailer_not_merely_appear_in_it() {
    let s = Scratch::new("bundle-verify");
    let k = kind("dmg");
    let path = s.join("lateloly.dmg");
    let size = k.floor_bytes + 1024;
    let mut f = hollow(&path, size);
    write_at(&mut f, size - 64, k.magic);
    drop(f);
    err_contains(verify_bundle_file(&path, &k), "magic");
}

#[test]
fn a_missing_file_says_which_one() {
    let s = Scratch::new("bundle-verify");
    err_contains(
        verify_bundle_file(&s.join("absent.deb"), &kind("deb")),
        "absent.deb",
    );
}

// ---- the version gate (the native gate itself: tests/native_bundle.rs) ----

#[test]
fn a_dependency_s_version_is_never_mistaken_for_the_crate_s() {
    // What keeps sp-app from carrying a version of its own.
    let text =
        "[package]\nname = \"sp-app\"\n\n[dependencies.egui]\nversion = \"2\"\nfeatures = []\n";
    assert_eq!(cargo_package_version(text), None);
    assert_eq!(
        cargo_package_version(
            "[package]\nname = \"d\"\n\n[dependencies]\ntauri = { version = \"2\" }\n"
        ),
        None
    );
    assert_eq!(
        cargo_package_version("[package]\nversion = \"0.6.0\"\n\n[dependencies]\ntauri = \"2\"\n")
            .as_deref(),
        Some("0.6.0")
    );
}

// ---- the argument parser ----

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn it_derives_the_os_and_architecture_from_the_host_by_default() {
    let a = parse_bundle_args(
        &args(&["--version", "0.6.0", "--out", "o"]),
        "linux",
        "arm64",
    )
    .unwrap();
    assert_eq!(
        (a.version.as_str(), a.os, a.arch),
        ("0.6.0", BundleOs::Linux, BundleArch::Arm64)
    );
    let a =
        parse_bundle_args(&args(&["--version", "0.6.0", "--out", "o"]), "win32", "x64").unwrap();
    assert_eq!((a.os, a.arch), (BundleOs::Windows, BundleArch::X64));
    let a = parse_bundle_args(
        &args(&["--version", "0.6.0", "--out", "o"]),
        "darwin",
        "arm64",
    )
    .unwrap();
    assert_eq!((a.os, a.arch), (BundleOs::Macos, BundleArch::Arm64));
    assert_eq!(a.target.as_deref(), Some("aarch64-apple-darwin"));
    assert_eq!(
        parse_bundle_args(&args(&["--version", "0.6.0", "--out", "o"]), "linux", "x64")
            .unwrap()
            .target,
        None
    );
    assert_eq!(
        parse_bundle_args(&args(&["--version", "0.6.0", "--out", "o"]), "win32", "x64")
            .unwrap()
            .target,
        None
    );
}

#[test]
fn it_strips_a_leading_v_so_a_tag_can_be_passed_straight_through() {
    assert_eq!(
        parse_bundle_args(
            &args(&["--version", "v0.6.0", "--out", "o"]),
            "linux",
            "x64"
        )
        .unwrap()
        .version,
        "0.6.0"
    );
}

#[test]
fn it_refuses_a_branch_name_where_a_version_belongs() {
    err_contains(
        parse_bundle_args(&args(&["--version", "main", "--out", "o"]), "linux", "x64"),
        "MAJOR.MINOR.PATCH",
    );
}

#[test]
fn it_accepts_a_prerelease_suffix_which_release_yml_already_supports() {
    assert_eq!(
        parse_bundle_args(
            &args(&["--version", "v0.6.0-rc1", "--out", "o"]),
            "linux",
            "x64"
        )
        .unwrap()
        .version,
        "0.6.0-rc1"
    );
}

#[test]
fn out_is_required_because_a_bundle_run_writes_90_mb() {
    err_contains(
        parse_bundle_args(&args(&["--version", "0.6.0"]), "linux", "x64"),
        "--out",
    );
}

#[test]
fn it_refuses_a_host_it_has_no_bundles_for() {
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o"]),
            "freebsd",
            "x64",
        ),
        "freebsd",
    );
}

#[test]
fn it_refuses_an_architecture_it_has_no_bundles_for() {
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o"]),
            "linux",
            "ia32",
        ),
        "ia32",
    );
}

#[test]
fn out_is_made_absolute_so_the_tool_can_be_run_from_anywhere() {
    let a = parse_bundle_args(
        &args(&["--version", "0.6.0", "--out", "dist"]),
        "linux",
        "x64",
    )
    .unwrap();
    assert!(a.out_dir.is_absolute());
    assert!(a.out_dir.ends_with("dist"));
}

#[test]
fn target_and_pubkey_reach_the_args_they_are_named_for() {
    let a = parse_bundle_args(
        &args(&[
            "--version",
            "0.6.0",
            "--out",
            "o",
            "--target",
            "aarch64-apple-darwin",
            "--pubkey",
            "k.pub",
        ]),
        "darwin",
        "arm64",
    )
    .unwrap();
    assert_eq!(a.target.as_deref(), Some("aarch64-apple-darwin"));
    assert_eq!(a.pubkey.as_deref(), Some("k.pub"));
    // --config was the Tauri overlay; nothing takes it now.
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o", "--config", "o.json"]),
            "darwin",
            "arm64",
        ),
        "--config",
    );
}

#[test]
fn an_unknown_flag_or_a_positional_is_refused_as_node_s_strict_parser_did() {
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o", "--sign"]),
            "linux",
            "x64",
        ),
        "--sign",
    );
    assert!(parse_bundle_args(&args(&["0.6.0"]), "linux", "x64").is_err());
}

// ---- a whole run, against fake outside tools ----

fn names_of(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| base_name(p)).collect()
}

fn no_env() -> Env {
    Env::new()
}

fn with_key() -> Env {
    env_of(&[
        ("TAURI_SIGNING_PRIVATE_KEY", TAURI_TEST_KEY),
        ("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""),
    ])
}

fn linux_run(out: &Path, arch: BundleArch) -> BundleArgs {
    BundleArgs::new("0.12.0", out, BundleOs::Linux, arch)
}

#[test]
fn it_renames_to_the_published_names_and_writes_sha256sums_app() {
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.join("run-linux");
    let made = build_bundles(
        &linux_run(&out, BundleArch::Arm64),
        &mut |a, c| tools.run(a, c),
        &dirs,
        &no_env(),
    )
    .unwrap();
    assert_eq!(
        names_of(&made),
        [
            "Method-Reader-linux-arm64.deb",
            "Method-Reader-linux-aarch64.rpm"
        ]
    );
    for p in &made {
        assert!(p.is_file());
    }
    // One build, in rust/: the window crate, never `cargo tauri`.
    assert_eq!(tools.calls.len(), 1);
    assert_eq!(tools.calls[0][..2], ["cargo", "build"]);
    assert!(!tools.calls[0].iter().any(|a| a == "tauri"));

    let sums = parse_checksums(&fs::read_to_string(out.join("SHA256SUMS-app")).unwrap());
    assert_eq!(
        sums.keys().collect::<Vec<_>>(),
        [
            "Method-Reader-linux-aarch64.rpm",
            "Method-Reader-linux-arm64.deb"
        ]
    );
    for (name, digest) in &sums {
        assert_eq!(*digest, sha256_file(&out.join(name)).unwrap());
    }
}

#[test]
fn the_version_gate_stops_a_run_before_cargo_is_ever_spawned() {
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.5.4");
    let mut tools = FakeTools::new(&dirs, BundleOs::Linux);
    err_contains(
        build_bundles(
            &linux_run(&s.join("run-gated"), BundleArch::X64),
            &mut |a, c| tools.run(a, c),
            &dirs,
            &no_env(),
        ),
        "0.5.4",
    );
    assert!(tools.calls.is_empty());
}

#[test]
fn the_checksums_file_is_not_called_sha256sums_and_names_the_x64_build() {
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.join("run-names");
    build_bundles(
        &linux_run(&out, BundleArch::X64),
        &mut |a, c| tools.run(a, c),
        &dirs,
        &no_env(),
    )
    .unwrap();
    assert!(out.join("SHA256SUMS-app").is_file());
    assert!(!out.join("SHA256SUMS").exists());
    let sums = parse_checksums(&fs::read_to_string(out.join("SHA256SUMS-app")).unwrap());
    assert_eq!(
        sums.keys().collect::<Vec<_>>(),
        [
            "Method-Reader-linux-amd64.deb",
            "Method-Reader-linux-x86_64.rpm"
        ]
    );
}

#[test]
fn a_failing_cargo_fails_the_run_and_forwards_its_stderr() {
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.12.0");
    let mut fail = |_: &[String], _: Option<&Path>| RunResult {
        exit_code: 101,
        stdout: String::new(),
        stderr: "error: linker `cc` not found".into(),
    };
    err_contains(
        build_bundles(
            &linux_run(&s.join("run-fail"), BundleArch::X64),
            &mut fail,
            &dirs,
            &no_env(),
        ),
        "linker",
    );
}

#[test]
fn a_cargo_that_exits_0_and_writes_nothing_still_fails() {
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.12.0");
    let mut nothing = |_: &[String], _: Option<&Path>| RunResult::ok("", "");
    let out = s.join("run-empty");
    err_contains(
        build_bundles(
            &linux_run(&out, BundleArch::X64),
            &mut nothing,
            &dirs,
            &no_env(),
        ),
        "method-reader-app",
    );
    assert!(!out.join("SHA256SUMS-app").exists());
}

#[test]
fn a_truncated_artifact_fails_the_run_instead_of_being_published() {
    // hdiutil reporting success over a 64-byte image.
    let s = Scratch::new("bundle-run");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    tools.dmg_bytes = Some(64);
    let out = s.join("run-trunc");
    err_contains(
        build_bundles(
            &native_args(&out, BundleOs::Macos, BundleArch::Arm64),
            &mut |a, c| tools.run(a, c),
            &dirs,
            &no_env(),
        ),
        "floor",
    );
    assert!(!out.join("Method-Reader-macOS.dmg").exists());
    assert!(!out.join("SHA256SUMS-app").exists());
}

// ---- the updater archive ----

fn mac_run(out: &Path, updater: bool) -> BundleArgs {
    BundleArgs {
        updater,
        pubkey: updater.then(tauri_test_public_key),
        ..native_args(out, BundleOs::Macos, BundleArch::Arm64)
    }
}

#[test]
fn the_parser_reads_updater_and_it_is_off_by_default() {
    assert!(
        !parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o"]),
            "darwin",
            "arm64"
        )
        .unwrap()
        .updater
    );
    assert!(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "o", "--updater"]),
            "darwin",
            "arm64"
        )
        .unwrap()
        .updater
    );
}

#[test]
fn the_updater_archive_is_a_macos_kind_only_gzip_at_the_head_floor_like_the_rest() {
    assert_eq!(
        UPDATER_KINDS.iter().map(|k| k.id).collect::<Vec<_>>(),
        ["app-tar"]
    );
    assert_eq!(
        updater_kinds_for_os(BundleOs::Macos)
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        ["app-tar"]
    );
    assert!(updater_kinds_for_os(BundleOs::Linux).is_empty());
    assert!(updater_kinds_for_os(BundleOs::Windows).is_empty());
    let k = app_tar();
    assert_eq!(k.dir, "macos");
    assert_eq!(k.ext, ".app.tar.gz");
    assert_eq!(k.magic, [0x1f, 0x8b]);
    assert_eq!(k.magic_at, MagicAt::Head);
    assert_eq!(k.floor_bytes, kind("dmg").floor_bytes);
    // Every kind's floor is the native window's (the Tauri one's 10 MB went
    // with the Bun engine it carried).
    for k in BUNDLE_KINDS.iter().chain(UPDATER_KINDS.iter()) {
        assert_eq!(k.floor_bytes, NATIVE_FLOOR, "{}", k.id);
    }
}

#[test]
fn the_published_name_follows_the_image_s_and_it_serves_apple_silicon_only() {
    let k = app_tar();
    assert_eq!(
        released_name(&k, "0.6.0", MAC_ARCH),
        "Method-Reader-macOS.app.tar.gz"
    );
    assert_eq!(platform_keys(&k, MAC_ARCH), ["darwin-aarch64"]);
}

#[test]
fn the_published_name_can_be_read_back_to_its_arch() {
    let k = app_tar();
    assert_eq!(
        arch_of(&k, &released_name(&k, "0.6.0", MAC_ARCH)),
        Some(BundleArch::Arm64)
    );
    assert_eq!(
        arch_of(&k, "Method-Reader-macOS.app.tar.gz"),
        Some(BundleArch::Arm64)
    );
    for not in [
        "Method-Reader-macOS-x64.app.tar.gz",
        "Method-Reader-macOS-arm64.app.tar.gz",
        "Method-Reader-macOS.dmg",
        "Method-Reader-macOS.app.tar.gz.sig",
        "Method-Reader-macOS-riscv.app.tar.gz",
        "Method-Reader-macOS-universal.app.tar.gz",
        "Screepub-Desktop-macOS-universal.app.tar.gz",
    ] {
        assert_eq!(arch_of(&k, not), None, "{not}");
    }
}

#[test]
fn the_verifier_accepts_a_plausible_archive_and_rejects_a_non_gzip_one() {
    let s = Scratch::new("bundle-updater");
    let k = app_tar();
    let good = s.join("plausible.app.tar.gz");
    plausible(&good, &k);
    verify_bundle_file(&good, &k).unwrap();
    let bad = s.join("not-gzip.app.tar.gz");
    plausible(&bad, &kind("deb"));
    err_contains(verify_bundle_file(&bad, &k), "magic");
}

#[test]
fn with_updater_the_archive_and_its_signature_come_out_under_the_published_names_byte_for_byte() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    let out = s.join("run-updater");
    let made = build_bundles(
        &mac_run(&out, true),
        &mut |a, c| tools.run(a, c),
        &dirs,
        &with_key(),
    )
    .unwrap();
    let mut names = names_of(&made);
    names.sort();
    assert_eq!(
        names,
        [
            "Method-Reader-macOS.app.tar.gz",
            "Method-Reader-macOS.app.tar.gz.sig",
            "Method-Reader-macOS.dmg"
        ]
    );
    let built = bundle_dir_for("macos", Some(MAC_TARGET), &dirs.rust_dir);
    for name in [
        "Method-Reader-macOS.app.tar.gz",
        "Method-Reader-macOS.app.tar.gz.sig",
    ] {
        assert_eq!(
            fs::read(out.join(name)).unwrap(),
            fs::read(built.join(name)).unwrap(),
            "{name}"
        );
    }
    // The archive is proven by its signature, not listed in the checksums.
    let sums = parse_checksums(&fs::read_to_string(out.join("SHA256SUMS-app")).unwrap());
    assert_eq!(sums.keys().collect::<Vec<_>>(), ["Method-Reader-macOS.dmg"]);
}

#[test]
fn without_updater_a_macos_run_publishes_no_archive_and_needs_no_key() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    let out = s.join("run-no-updater");
    let made = build_bundles(
        &mac_run(&out, false),
        &mut |a, c| tools.run(a, c),
        &dirs,
        &no_env(),
    )
    .unwrap();
    assert_eq!(names_of(&made), ["Method-Reader-macOS.dmg"]);
    assert!(!out.join("Method-Reader-macOS.app.tar.gz").exists());
}

#[test]
fn updater_without_the_signing_key_fails_before_cargo_is_spawned() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    err_contains(
        build_bundles(
            &mac_run(&s.join("run-nokey"), true),
            &mut |a, c| tools.run(a, c),
            &dirs,
            &no_env(),
        ),
        "TAURI_SIGNING_PRIVATE_KEY",
    );
    assert!(tools.calls.is_empty());
}

#[test]
fn updater_with_the_key_but_no_password_variable_fails_before_cargo_and_says_empty_is_fine() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    let env = env_of(&[("TAURI_SIGNING_PRIVATE_KEY", "set")]);
    let e = err_contains(
        build_bundles(
            &mac_run(&s.join("run-nopw"), true),
            &mut |a, c| tools.run(a, c),
            &dirs,
            &env,
        ),
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    );
    assert!(e.contains("empty"));
    assert!(tools.calls.is_empty());
}

#[test]
fn the_macos_run_builds_for_the_apple_silicon_triple() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
    let made = build_bundles(
        &mac_run(&s.join("run-macos"), false),
        &mut |a, c| tools.run(a, c),
        &dirs,
        &no_env(),
    )
    .unwrap();
    let call = &tools.calls[0];
    assert_eq!(call[..2], ["cargo", "build"]);
    for want in ["--target", "aarch64-apple-darwin"] {
        assert!(call.iter().any(|a| a == want), "{want}");
    }
    assert!(binary_path(&dirs.rust_dir, Some(MAC_TARGET), BundleOs::Macos).is_file());
    assert_eq!(names_of(&made), ["Method-Reader-macOS.dmg"]);
}

#[test]
fn an_intel_mac_run_is_refused_before_cargo_however_the_args_were_made() {
    let s = Scratch::new("bundle-updater");
    let dirs = native_repo(&s, "0.12.0");
    let cases: [(BundleArch, Option<&str>); 5] = [
        (BundleArch::X64, Some("x86_64-apple-darwin")),
        (BundleArch::X64, None),
        (BundleArch::Arm64, Some("x86_64-apple-darwin")),
        (BundleArch::Arm64, Some("universal-apple-darwin")),
        (BundleArch::Arm64, None),
    ];
    for (arch, target) in cases {
        let mut tools = FakeTools::new(&dirs, BundleOs::Macos);
        let a = BundleArgs {
            target: target.map(str::to_string),
            ..BundleArgs::new("0.12.0", &s.join("run-intel"), BundleOs::Macos, arch)
        };
        err_contains(
            build_bundles(&a, &mut |x, c| tools.run(x, c), &dirs, &no_env()),
            "Apple silicon",
        );
        assert!(tools.calls.is_empty());
    }
}

// ---- a Mac bundle is Apple silicon only ----

#[test]
fn the_arch_and_the_cargo_triple_are_named_once_here() {
    assert_eq!(MAC_ARCH, BundleArch::Arm64);
    assert_eq!(MAC_TARGET, "aarch64-apple-darwin");
}

#[test]
fn a_mac_build_always_names_the_triple_so_the_runner_cannot_choose_it() {
    let a = parse_bundle_args(
        &args(&["--version", "0.6.0", "--out", "/o"]),
        "darwin",
        "arm64",
    )
    .unwrap();
    assert_eq!(a.arch, BundleArch::Arm64);
    assert_eq!(a.target.as_deref(), Some("aarch64-apple-darwin"));
    assert_eq!(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "/o", "--arch", "arm64"]),
            "darwin",
            "arm64"
        )
        .unwrap()
        .target
        .as_deref(),
        Some("aarch64-apple-darwin")
    );
}

#[test]
fn an_intel_mac_is_refused_whether_it_is_the_host_or_asked_for() {
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "/o"]),
            "darwin",
            "x64",
        ),
        "Apple silicon",
    );
    err_contains(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "/o", "--arch", "x64"]),
            "darwin",
            "arm64",
        ),
        "Apple silicon",
    );
    assert_eq!(
        parse_bundle_args(
            &args(&["--version", "0.6.0", "--out", "/o", "--arch", "arm64"]),
            "darwin",
            "x64"
        )
        .unwrap()
        .target
        .as_deref(),
        Some("aarch64-apple-darwin")
    );
}

#[test]
fn a_mac_target_other_than_the_apple_silicon_triple_is_refused() {
    for target in ["x86_64-apple-darwin", "universal-apple-darwin"] {
        err_contains(
            parse_bundle_args(
                &args(&["--version", "0.6.0", "--out", "/o", "--target", target]),
                "darwin",
                "arm64",
            ),
            "Apple silicon",
        );
    }
}

#[test]
fn universal_is_not_an_architecture_any_more_on_any_os() {
    for (platform, host) in [("darwin", "arm64"), ("linux", "x64"), ("win32", "x64")] {
        err_contains(
            parse_bundle_args(
                &args(&["--version", "0.6.0", "--out", "/o", "--arch", "universal"]),
                platform,
                host,
            ),
            "universal",
        );
    }
}
