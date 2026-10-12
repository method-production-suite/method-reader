//! Port of tests/smoke-bundle.test.ts (35 checks): open a built bundle
//! without installing it and run the engine from inside, with hdiutil, 7z
//! and the engine itself as fakes behind the runner seam. Then the native
//! window's half, which checks the binary and the declared version.

mod packaging_common;

use std::fs;
use std::path::{Path, PathBuf};

use packaging_common::*;
use xtask::packaging::kinds::{AppSource, BundleOs, kinds_for_os};
use xtask::packaging::smoke::{
    BuiltSmoke, check_engine_version, detach, extract_archive_engine, extract_dmg_engine,
    extract_exe_engine, platform_refusal, smoke_built_bundles, smoke_bundle, smoke_native_bundle,
};
use xtask::packaging::util::RunResult;
use xtask::paths::repo_root;

fn ok(stdout: &str) -> RunResult {
    RunResult::ok(stdout, "")
}

fn fixture() -> PathBuf {
    repo_root().join("tests/fixtures/screenplay.pdf")
}

const ZIP: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

// ---- the version assertion ----

#[test]
fn the_version_the_engine_reports_must_equal_the_version_being_shipped() {
    check_engine_version(&ok("{\"ok\":true,\"version\":\"0.6.0\"}\n"), "0.6.0").unwrap();
}

#[test]
fn a_0_5_4_engine_inside_a_0_6_0_bundle_is_refused_naming_both_numbers() {
    let e =
        check_engine_version(&ok("{\"ok\":true,\"version\":\"0.5.4\"}\n"), "0.6.0").unwrap_err();
    assert!(e.contains("0.5.4") && e.contains("0.6.0"), "{e}");
}

#[test]
fn an_engine_that_reports_failure_is_refused_even_with_the_right_version() {
    err_contains(
        check_engine_version(
            &ok("{\"ok\":false,\"version\":\"0.6.0\",\"error\":\"broken\"}\n"),
            "0.6.0",
        ),
        "ok",
    );
}

#[test]
fn two_lines_on_stdout_is_refused_the_json_contract_is_exactly_one_object() {
    err_contains(
        check_engine_version(
            &ok("starting\n{\"ok\":true,\"version\":\"0.6.0\"}\n"),
            "0.6.0",
        ),
        "one",
    );
}

#[test]
fn a_non_zero_exit_is_refused_and_the_stderr_is_forwarded() {
    let r = RunResult {
        exit_code: 127,
        stdout: String::new(),
        stderr: "cannot execute binary file".into(),
    };
    err_contains(
        check_engine_version(&r, "0.6.0"),
        "cannot execute binary file",
    );
}

#[test]
fn unparseable_output_is_refused_rather_than_coerced() {
    err_contains(
        check_engine_version(&ok("not json at all\n"), "0.6.0"),
        "JSON",
    );
}

#[test]
fn a_bundle_with_no_version_field_at_all_is_refused() {
    err_contains(
        check_engine_version(&ok("{\"ok\":true}\n"), "0.6.0"),
        "0.6.0",
    );
}

#[test]
fn a_version_that_is_a_prefix_of_the_expected_one_is_refused() {
    err_contains(
        check_engine_version(&ok("{\"ok\":true,\"version\":\"0.6\"}\n"), "0.6.0"),
        "0.6.0",
    );
}

// ---- what this machine can and cannot smoke ----

#[test]
fn a_linux_package_is_smokeable_on_linux_and_refused_loudly_anywhere_else() {
    assert_eq!(
        platform_refusal("/x/Screepub_0.6.0_arm64.deb", "linux"),
        None
    );
    assert_eq!(
        platform_refusal("/x/Screepub-0.6.0-1.aarch64.rpm", "linux"),
        None
    );
    assert!(
        platform_refusal("/x/Screepub_0.6.0_arm64.deb", "darwin")
            .unwrap()
            .contains("NOTHING WAS CHECKED")
    );
}

#[test]
fn a_dmg_is_refused_off_macos_and_an_exe_off_windows_each_naming_its_os() {
    assert!(
        platform_refusal("/x/Screepub.dmg", "linux")
            .unwrap()
            .contains("macOS")
    );
    assert_eq!(platform_refusal("/x/Screepub.dmg", "darwin"), None);
    assert!(
        platform_refusal("/x/setup.exe", "linux")
            .unwrap()
            .contains("Windows")
    );
    assert_eq!(platform_refusal("/x/setup.exe", "win32"), None);
}

#[test]
fn an_unrecognised_extension_is_refused_rather_than_waved_through() {
    assert!(
        platform_refusal("/x/Screepub.AppImage", "linux")
            .unwrap()
            .contains("not a .deb")
    );
}

// ---- opening a .deb without installing it ----

#[test]
fn the_engine_comes_out_with_its_bytes_and_an_executable_bit() {
    let s = Scratch::new("smoke-deb");
    let deb = s.join("one.deb");
    let contents = b"#!/bin/sh\necho engine\n";
    fake_deb(&deb, "usr/bin/screepub-engine", contents);
    let work = s.fresh("work");
    let engine = extract_archive_engine(&deb, &work).unwrap();
    assert!(engine.starts_with(&work));
    assert_eq!(fs::read(&engine).unwrap(), contents);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&engine).unwrap().permissions().mode() & 0o111,
            0
        );
    }
}

#[test]
fn a_bundle_with_no_engine_inside_says_so_and_lists_what_it_held() {
    let s = Scratch::new("smoke-deb");
    let deb = s.join("noengine.deb");
    let tar = concat(&[
        &tar_member("usr/bin/screepub-desktop", b"", 0o755),
        &[0u8; 1024],
    ]);
    fs::write(
        &deb,
        concat(&[b"!<arch>\n", &ar_member("data.tar.gz/    ", &gzip(&tar))]),
    )
    .unwrap();
    err_contains(
        extract_archive_engine(&deb, &s.fresh("work")),
        "screepub-desktop",
    );
}

#[test]
fn a_near_miss_name_is_not_accepted_as_the_engine() {
    let s = Scratch::new("smoke-deb");
    let deb = s.join("nearmiss.deb");
    fake_deb(&deb, "usr/bin/not-screepub-engine", b"X");
    err_contains(
        extract_archive_engine(&deb, &s.fresh("work")),
        "not-screepub-engine",
    );
}

// ---- opening a .dmg ----

/// Stands in for hdiutil: `attach` lays out the mount the real one would.
fn fake_hdiutil(
    apps: &'static [&'static str],
    engine: bool,
) -> impl FnMut(&[String], Option<&Path>) -> RunResult {
    move |argv: &[String], _| {
        if argv[1] == "attach" {
            let mount =
                PathBuf::from(&argv[argv.iter().position(|a| a == "-mountpoint").unwrap() + 1]);
            fs::create_dir_all(&mount).unwrap();
            for app in apps {
                let macos = mount.join(app).join("Contents/MacOS");
                fs::create_dir_all(&macos).unwrap();
                if engine {
                    fs::write(macos.join("screepub-engine"), "ENGINE").unwrap();
                }
            }
        }
        ok("")
    }
}

#[test]
fn it_attaches_read_only_finds_the_app_and_points_at_the_engine_inside_it() {
    let s = Scratch::new("smoke-dmg");
    let work = s.fresh("dmg");
    let mut calls: Vec<Vec<String>> = vec![];
    let mut inner = fake_hdiutil(&["Screepub.app"], true);
    let mut run = |a: &[String], c: Option<&Path>| {
        calls.push(a.to_vec());
        inner(a, c)
    };
    let (engine, mounted) = extract_dmg_engine(&s.join("x.dmg"), &work, &mut run).unwrap();
    assert!(engine.ends_with(Path::new("Screepub.app/Contents/MacOS/screepub-engine")));
    detach(&mounted, &mut run);
    assert!(calls[0].iter().any(|a| a == "-readonly"));
    assert!(calls[0].iter().any(|a| a == "-nobrowse"));
    assert!(calls.iter().any(|c| c[1] == "detach"));
}

#[test]
fn it_finds_the_app_by_suffix_rather_than_by_a_hardcoded_name() {
    let s = Scratch::new("smoke-dmg");
    let mut run = fake_hdiutil(&["Screepub Desktop.app"], true);
    let (engine, _) = extract_dmg_engine(&s.join("y.dmg"), &s.fresh("dmg2"), &mut run).unwrap();
    assert!(engine.to_string_lossy().contains("Screepub Desktop.app"));
}

#[test]
fn a_mount_holding_two_app_bundles_is_an_error_not_a_coin_flip() {
    let s = Scratch::new("smoke-dmg");
    let mut run = fake_hdiutil(&["A.app", "B.app"], false);
    let e = err_contains(
        extract_dmg_engine(&s.join("z.dmg"), &s.fresh("dmg3"), &mut run).map(|_| ()),
        "A.app",
    );
    assert!(e.find("A.app") < e.find("B.app"));
}

#[test]
fn an_image_with_no_app_on_it_is_detached_before_the_error_is_thrown() {
    let s = Scratch::new("smoke-dmg");
    let mut calls: Vec<Vec<String>> = vec![];
    let mut inner = fake_hdiutil(&[], false);
    let mut run = |a: &[String], c: Option<&Path>| {
        calls.push(a.to_vec());
        inner(a, c)
    };
    err_contains(
        extract_dmg_engine(&s.join("empty.dmg"), &s.fresh("dmg5"), &mut run).map(|_| ()),
        "found 0",
    );
    assert!(calls.iter().any(|c| c[1] == "detach"));
}

#[test]
fn a_failed_attach_is_reported_with_hdiutil_s_own_stderr() {
    let s = Scratch::new("smoke-dmg");
    let mut run = |_: &[String], _: Option<&Path>| RunResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: "no mountable file systems".into(),
    };
    err_contains(
        extract_dmg_engine(&s.join("bad.dmg"), &s.fresh("dmg4"), &mut run).map(|_| ()),
        "no mountable file systems",
    );
}

// ---- opening an NSIS installer ----

#[test]
fn it_unpacks_with_7z_and_looks_for_the_exe_suffixed_engine() {
    let s = Scratch::new("smoke-exe");
    let work = s.fresh("exe");
    let mut calls: Vec<Vec<String>> = vec![];
    let w = work.clone();
    let mut run = |a: &[String], _: Option<&Path>| {
        calls.push(a.to_vec());
        fs::write(w.join("screepub-engine.exe"), "ENGINE").unwrap();
        ok("")
    };
    let engine = extract_exe_engine(&s.join("setup.exe"), &work, &mut run).unwrap();
    assert_eq!(engine, work.join("screepub-engine.exe"));
    assert_eq!(calls[0][0], "7z");
    assert!(calls[0].iter().any(|a| a == "x"));
    assert!(calls[0].contains(&format!("-o{}", work.display())));
}

#[test]
fn an_unpack_that_produces_no_engine_names_the_directory_it_searched() {
    let s = Scratch::new("smoke-exe");
    let mut run = |_: &[String], _: Option<&Path>| ok("");
    err_contains(
        extract_exe_engine(&s.join("setup.exe"), &s.fresh("exe2"), &mut run),
        "screepub-engine",
    );
}

#[test]
fn a_failing_7z_is_reported_rather_than_swallowed() {
    let s = Scratch::new("smoke-exe");
    let mut run = |_: &[String], _: Option<&Path>| RunResult {
        exit_code: 2,
        stdout: String::new(),
        stderr: "Cannot open the file".into(),
    };
    err_contains(
        extract_exe_engine(&s.join("setup.exe"), &s.fresh("exe3"), &mut run),
        "Cannot open the file",
    );
}

// ---- a whole smoke run over a .deb ----

/// An engine that answers `--version` and writes `epub_bytes` (or nothing)
/// where it is told, reporting `pages`.
fn engine_run<'a>(
    calls: &'a mut Vec<Vec<String>>,
    epub_bytes: Option<&'static [u8]>,
    pages: u32,
) -> impl FnMut(&[String], Option<&Path>) -> RunResult + 'a {
    move |argv: &[String], _| {
        calls.push(argv.to_vec());
        if argv.iter().any(|a| a == "--version") {
            return ok("{\"ok\":true,\"version\":\"0.6.0\"}\n");
        }
        let epub = argv[argv.iter().position(|a| a == "-o").unwrap() + 1].clone();
        if let Some(bytes) = epub_bytes {
            fs::write(&epub, bytes).unwrap();
        }
        ok(&format!(
            "{{\"ok\":true,\"epubPath\":{},\"pages\":{pages}}}\n",
            serde_json::to_string(&epub).unwrap()
        ))
    }
}

#[test]
fn it_runs_version_and_then_a_real_conversion_through_the_same_engine() {
    let s = Scratch::new("smoke-run");
    let deb = s.join("full.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let work = s.fresh("full");
    let mut calls = vec![];
    smoke_bundle(
        &deb,
        &fixture(),
        &work,
        "0.6.0",
        &mut engine_run(&mut calls, Some(&ZIP), 12),
    )
    .unwrap();
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert!(Path::new(&call[0]).starts_with(&work));
    }
    assert_eq!(
        calls[0],
        [
            work.join("usr/bin/screepub-engine")
                .to_string_lossy()
                .into_owned(),
            "--version".into(),
            "--json".into()
        ]
    );
    assert!(calls[1].contains(&fixture().to_string_lossy().into_owned()));
    assert!(calls[1].iter().any(|a| a == "--json"));
    assert!(work.join("smoke.epub").is_file());
}

#[test]
fn a_wrong_version_fails_the_run_before_any_conversion_is_attempted() {
    let s = Scratch::new("smoke-run");
    let deb = s.join("wrongver.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let mut calls = 0;
    let mut run = |_: &[String], _: Option<&Path>| {
        calls += 1;
        ok("{\"ok\":true,\"version\":\"0.5.4\"}\n")
    };
    err_contains(
        smoke_bundle(&deb, &fixture(), &s.fresh("wv"), "0.6.0", &mut run),
        "0.5.4",
    );
    assert_eq!(calls, 1);
}

#[test]
fn a_conversion_that_reports_zero_pages_is_a_failure_not_a_success() {
    let s = Scratch::new("smoke-run");
    let deb = s.join("zeropages.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("zp"),
            "0.6.0",
            &mut engine_run(&mut calls, Some(&ZIP), 0),
        ),
        "pages",
    );
}

#[test]
fn a_conversion_that_reports_success_and_writes_nothing_is_a_failure() {
    let s = Scratch::new("smoke-run");
    let deb = s.join("nofile.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("nofile"),
            "0.6.0",
            &mut engine_run(&mut calls, None, 12),
        ),
        "is not there",
    );
}

#[test]
fn an_output_that_is_not_a_zip_container_is_a_failure() {
    let s = Scratch::new("smoke-run");
    let deb = s.join("notzip.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("notzip"),
            "0.6.0",
            &mut engine_run(&mut calls, Some(b"this is not an epub"), 12),
        ),
        "zip container",
    );
}

#[test]
fn a_missing_bundle_says_which_path_before_anything_is_spawned() {
    let s = Scratch::new("smoke-run");
    let mut calls = 0;
    let mut run = |_: &[String], _: Option<&Path>| {
        calls += 1;
        ok("")
    };
    err_contains(
        smoke_bundle(
            &s.join("nope.deb"),
            &fixture(),
            &s.fresh("missing"),
            "0.6.0",
            &mut run,
        ),
        "nope.deb",
    );
    assert_eq!(calls, 0);
}

// ---- finding and smoking whatever this runner just built ----

const PAD: usize = 11_000_000;

fn big_deb(path: &Path, contents: &[u8]) {
    let tar = concat(&[
        &tar_member("usr/bin/screepub-engine", contents, 0o755),
        &[0u8; 1024],
    ]);
    fs::write(
        path,
        concat(&[
            b"!<arch>\n",
            &ar_member("debian-binary/  ", b"2.0\n"),
            &ar_member("control.tar.gz/ ", &gzip(&[0u8; 1024])),
            &ar_member("data.tar.gz/    ", &gzip(&tar)),
            &ar_member("_padding/       ", &vec![0u8; PAD]),
        ]),
    )
    .unwrap();
}

fn big_rpm(path: &Path, contents: &[u8]) {
    let cpio = concat(&[
        &cpio_member("./usr/bin/screepub-engine", contents, 0o100755),
        &cpio_member("./usr/lib/Screepub/_padding", &noise(PAD, 7), 0o100644),
        &cpio_trailer(),
    ]);
    fs::write(path, fake_rpm(&gzip(&cpio))).unwrap();
}

/// A stand-in for desktop/src-tauri with the named kinds' bundles in it.
fn desktop_tree(s: &Scratch, kinds: &[&str], tag: &str) -> PathBuf {
    let root = s.fresh(tag);
    for kind in kinds {
        let dir = root.join("target/release/bundle").join(kind);
        fs::create_dir_all(&dir).unwrap();
        if *kind == "deb" {
            big_deb(&dir.join("Screepub_0.6.0_arm64.deb"), b"ENGINE");
        } else {
            big_rpm(&dir.join("Screepub-0.6.0-1.aarch64.rpm"), b"ENGINE");
        }
    }
    root
}

fn built<'a>(
    os: BundleOs,
    work: &'a Path,
    root: &'a Path,
    host: &'a str,
    fixture: &'a Path,
) -> BuiltSmoke<'a> {
    BuiltSmoke {
        os,
        expected: "0.6.0",
        fixture,
        work_root: work,
        root,
        host,
        app: AppSource::Tauri,
    }
}

#[test]
fn it_refuses_an_os_whose_bundles_are_not_on_disk_naming_the_directory() {
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &[], "nonsis");
    let f = fixture();
    let mut run = |_: &[String], _: Option<&Path>| ok("");
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Windows, s.path(), &root, "win32", &f),
            &mut run,
        ),
        "nsis",
    );
}

#[test]
fn it_smokes_every_kind_the_os_defines_each_with_its_own_engine_run() {
    assert_eq!(kinds_for_os(BundleOs::Linux).len(), 2);
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &["deb", "rpm"], "both");
    let work = s.fresh("bothwork");
    let f = fixture();
    let mut calls = vec![];
    let smoked = smoke_built_bundles(
        &built(BundleOs::Linux, &work, &root, "linux", &f),
        &mut engine_run(&mut calls, Some(&ZIP), 12),
    )
    .unwrap();
    assert_eq!(smoked.len(), 2);
    assert!(smoked.iter().any(|p| p.to_string_lossy().ends_with(".deb")));
    assert!(smoked.iter().any(|p| p.to_string_lossy().ends_with(".rpm")));
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.iter().any(|a| a == "--version"))
            .count(),
        2
    );
}

#[test]
fn one_kind_missing_fails_the_run_even_though_the_other_one_smoked_clean() {
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &["deb"], "debonly");
    let work = s.fresh("debonlywork");
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, &work, &root, "linux", &f),
            &mut engine_run(&mut calls, Some(&ZIP), 12),
        ),
        "rpm",
    );
    assert_eq!(calls.len(), 2);
}

#[test]
fn the_container_is_verified_before_any_engine_inside_it_is_run() {
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &[], "wrongmagic");
    let dir = root.join("target/release/bundle/deb");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Screepub_0.6.0_arm64.deb"), noise(PAD, 3)).unwrap();
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, s.path(), &root, "linux", &f),
            &mut engine_run(&mut calls, Some(&ZIP), 12),
        ),
        "magic",
    );
    assert!(calls.is_empty());
}

#[test]
fn a_bundle_under_the_size_floor_is_refused_rather_than_smoked() {
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &[], "tiny");
    let dir = root.join("target/release/bundle/deb");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Screepub_0.6.0_arm64.deb"), b"!<arch>\n").unwrap();
    let f = fixture();
    let mut run = |_: &[String], _: Option<&Path>| ok("");
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, s.path(), &root, "linux", &f),
            &mut run,
        ),
        "floor",
    );
}

#[test]
fn a_wrong_version_engine_inside_a_built_bundle_fails_the_whole_run() {
    let s = Scratch::new("smoke-built");
    let root = desktop_tree(&s, &["deb", "rpm"], "badver");
    let work = s.fresh("badverwork");
    let f = fixture();
    let mut run = |_: &[String], _: Option<&Path>| ok("{\"ok\":true,\"version\":\"0.5.4\"}\n");
    err_contains(
        smoke_built_bundles(&built(BundleOs::Linux, &work, &root, "linux", &f), &mut run),
        "0.5.4",
    );
}

#[test]
fn every_built_linux_package_runs_its_own_engine_if_one_has_been_built() {
    let bundle_dir = repo_root().join("desktop/src-tauri/target/release/bundle");
    let real: Vec<PathBuf> = ["deb", "rpm"]
        .iter()
        .filter_map(|kind| {
            fs::read_dir(bundle_dir.join(kind))
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.to_string_lossy().ends_with(&format!(".{kind}")))
        })
        .collect();
    if real.is_empty() || !cfg!(target_os = "linux") {
        eprintln!("skipped: no Tauri Linux bundle has been built on this machine");
        return;
    }
    let version = serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(repo_root().join("package.json")).unwrap(),
    )
    .unwrap()["version"]
        .as_str()
        .unwrap()
        .to_string();
    let s = Scratch::new("smoke-real");
    for bundle in real {
        let work = s.fresh("real");
        smoke_bundle(
            &bundle,
            &fixture(),
            &work,
            &version,
            &mut xtask::packaging::util::real_run,
        )
        .unwrap();
        assert!(work.join("smoke.epub").is_file());
    }
}

// ---- the native window: its binary and its declared version ----

#[test]
fn a_native_package_is_checked_for_its_window_and_its_version() {
    use xtask::packaging::bundle::{BundleArgs, BundleDirs, Env, build_bundles};
    use xtask::packaging::kinds::BundleArch;
    use xtask::packaging::native::binary_path;

    let s = Scratch::new("smoke-native");
    let repo = s.fresh("repo");
    fs::write(repo.join("LICENSE"), "L").unwrap();
    fs::write(repo.join("THIRD-PARTY-NOTICES.md"), "N").unwrap();
    fs::create_dir_all(repo.join("rust/crates/sp-app/icons")).unwrap();
    fs::write(
        repo.join("rust/Cargo.toml"),
        "[workspace.package]\nversion = \"0.12.0\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("rust/crates/sp-app/Cargo.toml"),
        "[package]\nversion.workspace = true\n",
    )
    .unwrap();
    for icon in [
        "icon.icns",
        "icon.ico",
        "32x32.png",
        "128x128.png",
        "128x128@2x.png",
    ] {
        fs::write(repo.join("rust/crates/sp-app/icons").join(icon), "i").unwrap();
    }
    let dirs = BundleDirs::for_repo(&repo);
    let rust = dirs.rust_dir.clone();
    let mut cargo = |_: &[String], _: Option<&Path>| {
        let bin = binary_path(&rust, None, BundleOs::Linux);
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, noise(2_100_000, 11)).unwrap();
        ok("")
    };
    let out = s.join("out");
    build_bundles(
        &BundleArgs::new("0.12.0", &out, BundleOs::Linux, BundleArch::X64),
        &mut cargo,
        &dirs,
        &Env::new(),
    )
    .unwrap();

    let mut none = |_: &[String], _: Option<&Path>| ok("");
    for name in ["Screepub-linux-amd64.deb", "Screepub-linux-x86_64.rpm"] {
        smoke_native_bundle(&out.join(name), &s.fresh("w"), "0.12.0", &mut none).unwrap();
        let e = err_contains(
            smoke_native_bundle(&out.join(name), &s.fresh("w"), "0.13.0", &mut none),
            "0.13.0",
        );
        assert!(e.contains("0.12.0"), "{e}");
    }
    // The whole built tree, as `--built` finds it under rust/.
    let f = fixture();
    let smoked = smoke_built_bundles(
        &BuiltSmoke {
            os: BundleOs::Linux,
            expected: "0.12.0",
            fixture: &f,
            work_root: &s.fresh("built"),
            root: &dirs.rust_dir,
            host: "linux",
            app: AppSource::Native,
        },
        &mut none,
    )
    .unwrap();
    assert_eq!(smoked.len(), 2);
    // A package without the window binary is refused, naming where it looked.
    let deb = s.join("tauri.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    err_contains(
        smoke_native_bundle(&deb, &s.fresh("w"), "0.12.0", &mut none),
        "usr/bin/screepub-desktop",
    );
}

#[test]
fn a_native_image_must_carry_the_tauri_identifier_and_the_version() {
    use xtask::packaging::native::info_plist;
    let s = Scratch::new("smoke-native-dmg");
    let dmg = s.join("Screepub-macOS.dmg");
    fs::write(&dmg, "image").unwrap();
    let plist_for = |version: &'static str, identifier: &'static str| {
        move |argv: &[String], _: Option<&Path>| {
            if argv[1] == "attach" {
                let mount =
                    PathBuf::from(&argv[argv.iter().position(|a| a == "-mountpoint").unwrap() + 1]);
                let contents = mount.join("Screepub.app/Contents");
                fs::create_dir_all(contents.join("MacOS")).unwrap();
                fs::write(contents.join("MacOS/screepub-desktop"), "BIN").unwrap();
                fs::write(
                    contents.join("Info.plist"),
                    info_plist(version).replace("com.darkwell.screepub", identifier),
                )
                .unwrap();
            }
            ok("")
        }
    };
    smoke_native_bundle(
        &dmg,
        &s.fresh("a"),
        "0.12.0",
        &mut plist_for("0.12.0", "com.darkwell.screepub"),
    )
    .unwrap();
    err_contains(
        smoke_native_bundle(
            &dmg,
            &s.fresh("b"),
            "0.12.0",
            &mut plist_for("0.11.9", "com.darkwell.screepub"),
        ),
        "0.11.9",
    );
    err_contains(
        smoke_native_bundle(
            &dmg,
            &s.fresh("c"),
            "0.12.0",
            &mut plist_for("0.12.0", "com.example.other"),
        ),
        "com.darkwell.screepub",
    );
}

#[test]
fn the_command_needs_an_expected_version_and_a_bundle() {
    let (code, _, err) = xtask(&["smoke-bundle"]);
    assert_eq!(code, 1);
    assert!(err.contains("--expect-version"), "{err}");
    let (code, _, err) = xtask(&["smoke-bundle", "--expect-version", "0.6.0"]);
    assert_eq!(code, 1);
    assert!(err.contains("--bundle"), "{err}");
}
