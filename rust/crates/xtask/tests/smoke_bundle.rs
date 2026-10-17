//! Port of tests/smoke-bundle.test.ts (35 checks): open a built bundle
//! without installing it and run the window inside it headless, with
//! hdiutil, 7z and the window itself as fakes behind the runner seams.
//!
//! Until the Rust cutover the Tauri bundle's engine sidecar was what ran;
//! the native window took its place (sp-app's src/headless.rs: `--version
//! --json` and `--smoke <pdf> --out <epub>`), so the checks that ran the
//! sidecar run the window now, with a scratch METHOD_READER_CONFIG_DIR,
//! METHOD_READER_LIBRARY and Calibre guard. The one check that read a built
//! Tauri package off this machine's disk became the real-window check at
//! the end, which runs the workspace's own built window.

mod packaging_common;

use std::fs;
use std::path::{Path, PathBuf};

use packaging_common::*;
use xtask::packaging::bundle::{Env, build_bundles};
use xtask::packaging::kinds::{BundleArch, BundleOs, bundle_dir_for, kinds_for_os};
use xtask::packaging::smoke::{
    BuiltSmoke, check_window_version, default_fixture, detach, extract_archive_window,
    extract_dmg_window, extract_exe_window, platform_refusal, smoke_built_bundles, smoke_bundle,
    smoke_window, window_env,
};
use xtask::packaging::util::{RunResult, real_run_env};
use xtask::paths::{repo_root, workspace_root};

fn ok(stdout: &str) -> RunResult {
    RunResult::ok(stdout, "")
}

fn fixture() -> PathBuf {
    default_fixture()
}

const ZIP: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const V: &str = NATIVE_VERSION;

fn version_line(v: &str) -> String {
    format!("{{\"ok\":true,\"version\":\"{v}\"}}\n")
}

// ---- the version assertion ----

#[test]
fn the_version_the_window_reports_must_equal_the_version_being_shipped() {
    check_window_version(&ok(&version_line("0.6.0")), "0.6.0").unwrap();
}

#[test]
fn a_0_5_4_window_inside_a_0_6_0_bundle_is_refused_naming_both_numbers() {
    let e = check_window_version(&ok(&version_line("0.5.4")), "0.6.0").unwrap_err();
    assert!(e.contains("0.5.4") && e.contains("0.6.0"), "{e}");
}

#[test]
fn a_window_that_reports_failure_is_refused_even_with_the_right_version() {
    err_contains(
        check_window_version(
            &ok("{\"ok\":false,\"version\":\"0.6.0\",\"error\":\"broken\"}\n"),
            "0.6.0",
        ),
        "ok",
    );
}

#[test]
fn two_lines_on_stdout_is_refused_the_json_contract_is_exactly_one_object() {
    err_contains(
        check_window_version(
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
        check_window_version(&r, "0.6.0"),
        "cannot execute binary file",
    );
}

#[test]
fn unparseable_output_is_refused_rather_than_coerced() {
    err_contains(
        check_window_version(&ok("not json at all\n"), "0.6.0"),
        "JSON",
    );
}

#[test]
fn a_bundle_with_no_version_field_at_all_is_refused() {
    err_contains(
        check_window_version(&ok("{\"ok\":true}\n"), "0.6.0"),
        "0.6.0",
    );
}

#[test]
fn a_version_that_is_a_prefix_of_the_expected_one_is_refused() {
    err_contains(
        check_window_version(&ok("{\"ok\":true,\"version\":\"0.6\"}\n"), "0.6.0"),
        "0.6.0",
    );
}

// ---- what this machine can and cannot smoke ----

#[test]
fn a_linux_package_is_smokeable_on_linux_and_refused_loudly_anywhere_else() {
    assert_eq!(
        platform_refusal("/x/Method-Reader-linux-arm64.deb", "linux"),
        None
    );
    assert_eq!(
        platform_refusal("/x/Method-Reader-linux-aarch64.rpm", "linux"),
        None
    );
    let refusal = platform_refusal("/x/Method-Reader-linux-arm64.deb", "darwin").unwrap();
    assert!(refusal.contains("NOTHING WAS CHECKED"), "{refusal}");
    assert!(refusal.contains("window"), "{refusal}");
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
fn the_window_comes_out_with_its_bytes_and_an_executable_bit() {
    let s = Scratch::new("smoke-deb");
    let deb = s.join("one.deb");
    let contents = b"#!/bin/sh\necho window\n";
    fake_deb(&deb, "usr/bin/method-reader-app", contents);
    let work = s.fresh("work");
    let window = extract_archive_window(&deb, &work).unwrap();
    assert!(window.starts_with(&work));
    assert_eq!(fs::read(&window).unwrap(), contents);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&window).unwrap().permissions().mode() & 0o111,
            0
        );
    }
}

#[test]
fn a_bundle_with_no_window_inside_says_so_and_lists_what_it_held() {
    // The Tauri package's engine is exactly what a native package must not
    // be mistaken for.
    let s = Scratch::new("smoke-deb");
    let deb = s.join("tauri.deb");
    fake_deb(&deb, "usr/bin/screepub-engine", b"ENGINE");
    let e = err_contains(
        extract_archive_window(&deb, &s.fresh("work")),
        "usr/bin/method-reader-app",
    );
    assert!(e.contains("screepub-engine"), "{e}");
}

#[test]
fn a_near_miss_name_is_not_accepted_as_the_window() {
    let s = Scratch::new("smoke-deb");
    let deb = s.join("nearmiss.deb");
    fake_deb(&deb, "usr/bin/not-method-reader-app", b"X");
    err_contains(
        extract_archive_window(&deb, &s.fresh("work")),
        "not-method-reader-app",
    );
}

#[test]
fn an_empty_or_non_executable_window_is_refused() {
    let s = Scratch::new("smoke-deb");
    let empty = s.join("empty.deb");
    fake_deb(&empty, "usr/bin/method-reader-app", b"");
    err_contains(
        extract_archive_window(&empty, &s.fresh("work")),
        "non-empty executable",
    );
    let plain = s.join("plain.deb");
    let tar = concat(&[
        &tar_member("usr/bin/method-reader-app", b"BIN", 0o644),
        &[0u8; 1024],
    ]);
    fs::write(
        &plain,
        concat(&[b"!<arch>\n", &ar_member("data.tar.gz/    ", &gzip(&tar))]),
    )
    .unwrap();
    err_contains(
        extract_archive_window(&plain, &s.fresh("work")),
        "non-empty executable",
    );
}

// ---- opening a .dmg ----

/// Stands in for hdiutil: `attach` lays out the mount the real one would.
fn fake_hdiutil(
    apps: &'static [&'static str],
    window: bool,
) -> impl FnMut(&[String], Option<&Path>) -> RunResult {
    move |argv: &[String], _| {
        if argv[1] == "attach" {
            let mount =
                PathBuf::from(&argv[argv.iter().position(|a| a == "-mountpoint").unwrap() + 1]);
            fs::create_dir_all(&mount).unwrap();
            for app in apps {
                let macos = mount.join(app).join("Contents/MacOS");
                fs::create_dir_all(&macos).unwrap();
                if window {
                    fs::write(macos.join("method-reader-app"), "WINDOW").unwrap();
                }
            }
        }
        ok("")
    }
}

#[test]
fn it_attaches_read_only_finds_the_app_and_points_at_the_window_inside_it() {
    let s = Scratch::new("smoke-dmg");
    let work = s.fresh("dmg");
    let mut calls: Vec<Vec<String>> = vec![];
    let mut inner = fake_hdiutil(&["Method Reader.app"], true);
    let mut run = |a: &[String], c: Option<&Path>| {
        calls.push(a.to_vec());
        inner(a, c)
    };
    let (window, mounted) = extract_dmg_window(&s.join("x.dmg"), &work, &mut run).unwrap();
    assert!(window.ends_with(Path::new(
        "Method Reader.app/Contents/MacOS/method-reader-app"
    )));
    detach(&mounted, &mut run);
    assert!(calls[0].iter().any(|a| a == "-readonly"));
    assert!(calls[0].iter().any(|a| a == "-nobrowse"));
    assert!(calls.iter().any(|c| c[1] == "detach"));
}

#[test]
fn it_finds_the_app_by_suffix_rather_than_by_a_hardcoded_name() {
    let s = Scratch::new("smoke-dmg");
    let mut run = fake_hdiutil(&["Screepub Desktop.app"], true);
    let (window, _) = extract_dmg_window(&s.join("y.dmg"), &s.fresh("dmg2"), &mut run).unwrap();
    assert!(window.to_string_lossy().contains("Screepub Desktop.app"));
}

#[test]
fn a_mount_holding_two_app_bundles_is_an_error_not_a_coin_flip() {
    let s = Scratch::new("smoke-dmg");
    let mut run = fake_hdiutil(&["A.app", "B.app"], false);
    let e = err_contains(
        extract_dmg_window(&s.join("z.dmg"), &s.fresh("dmg3"), &mut run).map(|_| ()),
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
        extract_dmg_window(&s.join("empty.dmg"), &s.fresh("dmg5"), &mut run).map(|_| ()),
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
        extract_dmg_window(&s.join("bad.dmg"), &s.fresh("dmg4"), &mut run).map(|_| ()),
        "no mountable file systems",
    );
}

// ---- opening an NSIS installer ----

#[test]
fn it_unpacks_with_7z_and_looks_for_the_exe_suffixed_window() {
    let s = Scratch::new("smoke-exe");
    let work = s.fresh("exe");
    let mut calls: Vec<Vec<String>> = vec![];
    let w = work.clone();
    let mut run = |a: &[String], _: Option<&Path>| {
        calls.push(a.to_vec());
        fs::write(w.join("method-reader-app.exe"), "WINDOW").unwrap();
        ok("")
    };
    let window = extract_exe_window(&s.join("setup.exe"), &work, &mut run).unwrap();
    assert_eq!(window, work.join("method-reader-app.exe"));
    assert_eq!(calls[0][0], "7z");
    assert!(calls[0].iter().any(|a| a == "x"));
    assert!(calls[0].contains(&format!("-o{}", work.display())));
}

#[test]
fn an_unpack_that_produces_no_window_names_the_directory_it_searched() {
    let s = Scratch::new("smoke-exe");
    let mut run = |_: &[String], _: Option<&Path>| ok("");
    err_contains(
        extract_exe_window(&s.join("setup.exe"), &s.fresh("exe2"), &mut run),
        "method-reader-app",
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
        extract_exe_window(&s.join("setup.exe"), &s.fresh("exe3"), &mut run),
        "Cannot open the file",
    );
}

// ---- a whole smoke run over the native packages ----

/// The native .deb and .rpm, built by build-app-bundle from a fake cargo:
/// real packages around a window that is only bytes.
fn native_packages(s: &Scratch) -> (PathBuf, PathBuf, PathBuf) {
    let dirs = native_repo(s, V);
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.fresh("out");
    build_bundles(
        &native_args(&out, BundleOs::Linux, BundleArch::X64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &Env::new(),
    )
    .unwrap();
    (
        out.join("Method-Reader-linux-amd64.deb"),
        out.join("Method-Reader-linux-x86_64.rpm"),
        dirs.rust_dir,
    )
}

type Seen = Vec<(Vec<String>, Vec<(String, String)>)>;

/// A window that answers `--version` and, for `--smoke`, writes
/// `epub_bytes` (or nothing) where `--out` says, reporting `pages`.
fn window_run<'a>(
    calls: &'a mut Seen,
    version: &'static str,
    epub_bytes: Option<&'static [u8]>,
    pages: u32,
) -> impl FnMut(&[String], &[(String, String)]) -> RunResult + 'a {
    move |argv: &[String], env: &[(String, String)]| {
        calls.push((argv.to_vec(), env.to_vec()));
        if argv.iter().any(|a| a == "--version") {
            return ok(&version_line(version));
        }
        let epub = argv[argv.iter().position(|a| a == "--out").unwrap() + 1].clone();
        if let Some(bytes) = epub_bytes {
            fs::write(&epub, bytes).unwrap();
        }
        ok(&format!(
            "{{\"ok\":true,\"epubPath\":{},\"pages\":{pages}}}\n",
            serde_json::to_string(&epub).unwrap()
        ))
    }
}

fn no_tools() -> impl FnMut(&[String], Option<&Path>) -> RunResult {
    |_: &[String], _: Option<&Path>| ok("")
}

#[test]
fn it_runs_version_and_then_a_real_conversion_through_the_same_window() {
    let s = Scratch::new("smoke-run");
    let (deb, _, _) = native_packages(&s);
    let work = s.fresh("full");
    let mut calls = vec![];
    smoke_bundle(
        &deb,
        &fixture(),
        &work,
        V,
        &mut no_tools(),
        &mut window_run(&mut calls, V, Some(&ZIP), 18),
    )
    .unwrap();
    assert_eq!(calls.len(), 2);
    let window = work.join("usr/bin/method-reader-app");
    assert_eq!(fs::read(&window).unwrap(), the_binary());
    assert_eq!(
        calls[0].0,
        [
            window.to_string_lossy().into_owned(),
            "--version".into(),
            "--json".into()
        ]
    );
    assert_eq!(
        calls[1].0,
        [
            window.to_string_lossy().into_owned(),
            "--smoke".into(),
            fixture().to_string_lossy().into_owned(),
            "--out".into(),
            work.join("smoke.epub").to_string_lossy().into_owned(),
        ]
    );
    assert!(fixture().ends_with("tests/fixtures/field-station.pdf"));
    assert!(work.join("smoke.epub").is_file());
}

#[test]
fn every_window_run_gets_scratch_settings_a_scratch_library_and_no_calibre() {
    let s = Scratch::new("smoke-env");
    let (deb, _, _) = native_packages(&s);
    let work = s.fresh("env");
    let mut calls = vec![];
    smoke_bundle(
        &deb,
        &fixture(),
        &work,
        V,
        &mut no_tools(),
        &mut window_run(&mut calls, V, Some(&ZIP), 18),
    )
    .unwrap();
    for (_, env) in &calls {
        assert_eq!(env, &window_env(&work).unwrap());
        let get = |k: &str| {
            PathBuf::from(
                &env.iter()
                    .find(|(name, _)| name == k)
                    .unwrap_or_else(|| panic!("{k} is not set"))
                    .1,
            )
        };
        assert_eq!(get("METHOD_READER_CONFIG_DIR"), work.join("config"));
        assert_eq!(get("METHOD_READER_LIBRARY"), work.join("library"));
        // Calibre's folder sits under a FILE, so nothing can be made there.
        let calibre = get("CALIBRE_CONFIG_DIRECTORY");
        assert!(calibre.starts_with(&work));
        assert!(calibre.parent().unwrap().is_file());
    }
}

#[test]
fn a_wrong_version_fails_the_run_before_any_conversion_is_attempted() {
    let s = Scratch::new("smoke-run");
    let (deb, _, _) = native_packages(&s);
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("wv"),
            V,
            &mut no_tools(),
            &mut window_run(&mut calls, "0.11.9", Some(&ZIP), 18),
        ),
        "0.11.9",
    );
    assert_eq!(calls.len(), 1);
}

#[test]
fn a_package_that_declares_another_version_fails_before_the_window_runs() {
    let s = Scratch::new("smoke-run");
    let (deb, rpm, _) = native_packages(&s);
    for pkg in [&deb, &rpm] {
        let mut calls = vec![];
        let e = err_contains(
            smoke_bundle(
                pkg,
                &fixture(),
                &s.fresh("pv"),
                "0.13.0",
                &mut no_tools(),
                &mut window_run(&mut calls, "0.13.0", Some(&ZIP), 18),
            ),
            "0.13.0",
        );
        assert!(e.contains(V), "{e}");
        assert!(calls.is_empty());
    }
}

#[test]
fn a_conversion_that_reports_zero_pages_is_a_failure_not_a_success() {
    let s = Scratch::new("smoke-run");
    let (deb, _, _) = native_packages(&s);
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("zp"),
            V,
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(&ZIP), 0),
        ),
        "pages",
    );
}

#[test]
fn a_conversion_that_reports_success_and_writes_nothing_is_a_failure() {
    let s = Scratch::new("smoke-run");
    let (deb, _, _) = native_packages(&s);
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("nofile"),
            V,
            &mut no_tools(),
            &mut window_run(&mut calls, V, None, 18),
        ),
        "is not there",
    );
}

#[test]
fn an_output_that_is_not_a_zip_container_is_a_failure() {
    let s = Scratch::new("smoke-run");
    let (deb, _, _) = native_packages(&s);
    let mut calls = vec![];
    err_contains(
        smoke_bundle(
            &deb,
            &fixture(),
            &s.fresh("notzip"),
            V,
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(b"this is not an epub"), 18),
        ),
        "zip container",
    );
}

#[test]
fn a_missing_bundle_says_which_path_before_anything_is_spawned() {
    let s = Scratch::new("smoke-run");
    let mut calls = vec![];
    let mut tools = 0;
    let mut run = |_: &[String], _: Option<&Path>| {
        tools += 1;
        ok("")
    };
    err_contains(
        smoke_bundle(
            &s.join("nope.deb"),
            &fixture(),
            &s.fresh("missing"),
            V,
            &mut run,
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "nope.deb",
    );
    assert_eq!(tools, 0);
    assert!(calls.is_empty());
}

#[test]
fn a_missing_fixture_is_named_before_the_window_runs() {
    let s = Scratch::new("smoke-run");
    let mut calls = vec![];
    err_contains(
        smoke_window(
            Path::new("/no/window"),
            &s.join("nowhere.pdf"),
            &s.fresh("nofixture"),
            V,
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "nowhere.pdf",
    );
    assert!(calls.is_empty());
}

// ---- a native image ----

#[test]
fn a_native_image_must_carry_the_identifier_and_the_version_and_its_window_runs() {
    use xtask::packaging::native::info_plist;
    let s = Scratch::new("smoke-native-dmg");
    let dmg = s.join("Method-Reader-macOS.dmg");
    fs::write(&dmg, "image").unwrap();
    let plist_for = |version: &'static str, identifier: &'static str| {
        move |argv: &[String], _: Option<&Path>| {
            if argv[1] == "attach" {
                let mount =
                    PathBuf::from(&argv[argv.iter().position(|a| a == "-mountpoint").unwrap() + 1]);
                let contents = mount.join("Method Reader.app/Contents");
                fs::create_dir_all(contents.join("MacOS")).unwrap();
                fs::write(contents.join("MacOS/method-reader-app"), "BIN").unwrap();
                fs::write(
                    contents.join("Info.plist"),
                    info_plist(version).replace(sp_update::IDENTIFIER, identifier),
                )
                .unwrap();
            }
            ok("")
        }
    };
    let mut calls = vec![];
    let work = s.fresh("a");
    smoke_bundle(
        &dmg,
        &fixture(),
        &work,
        V,
        &mut plist_for(V, "com.methodreader.desktop"),
        &mut window_run(&mut calls, V, Some(&ZIP), 18),
    )
    .unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        Path::new(&calls[0].0[0]).ends_with("Method Reader.app/Contents/MacOS/method-reader-app")
    );
    for (version, identifier, said) in [
        ("0.11.9", "com.methodreader.desktop", "0.11.9"),
        (V, "com.example.other", "com.methodreader.desktop"),
        // The 0.12.x identifier is another app's now.
        (V, "com.darkwell.screepub", "com.methodreader.desktop"),
    ] {
        let mut calls = vec![];
        err_contains(
            smoke_bundle(
                &dmg,
                &fixture(),
                &s.fresh("b"),
                V,
                &mut plist_for(version, identifier),
                &mut window_run(&mut calls, V, Some(&ZIP), 18),
            ),
            said,
        );
        assert!(calls.is_empty());
    }
}

// ---- finding and smoking whatever this runner just built ----

fn built<'a>(
    os: BundleOs,
    work: &'a Path,
    root: &'a Path,
    host: &'a str,
    fixture: &'a Path,
) -> BuiltSmoke<'a> {
    BuiltSmoke {
        os,
        expected: V,
        fixture,
        work_root: work,
        root,
        host,
    }
}

#[test]
fn it_refuses_an_os_whose_bundles_are_not_on_disk_naming_the_directory() {
    let s = Scratch::new("smoke-built");
    let root = s.fresh("nonsis");
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Windows, s.path(), &root, "win32", &f),
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "nsis",
    );
}

#[test]
fn it_smokes_every_kind_the_os_defines_each_with_its_own_window_run() {
    assert_eq!(kinds_for_os(BundleOs::Linux).len(), 2);
    let s = Scratch::new("smoke-built");
    let (_, _, rust) = native_packages(&s);
    let work = s.fresh("bothwork");
    let f = fixture();
    let mut calls = vec![];
    let smoked = smoke_built_bundles(
        &built(BundleOs::Linux, &work, &rust, "linux", &f),
        &mut no_tools(),
        &mut window_run(&mut calls, V, Some(&ZIP), 18),
    )
    .unwrap();
    assert_eq!(smoked.len(), 2);
    assert!(smoked.iter().any(|p| p.to_string_lossy().ends_with(".deb")));
    assert!(smoked.iter().any(|p| p.to_string_lossy().ends_with(".rpm")));
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .filter(|(c, _)| c.iter().any(|a| a == "--version"))
            .count(),
        2
    );
}

#[test]
fn one_kind_missing_fails_the_run_even_though_the_other_one_smoked_clean() {
    let s = Scratch::new("smoke-built");
    let (_, _, rust) = native_packages(&s);
    fs::remove_dir_all(bundle_dir_for("rpm", None, &rust)).unwrap();
    let work = s.fresh("debonlywork");
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, &work, &rust, "linux", &f),
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "rpm",
    );
    assert_eq!(calls.len(), 2);
}

#[test]
fn the_container_is_verified_before_any_window_inside_it_is_run() {
    let s = Scratch::new("smoke-built");
    let (_, _, rust) = native_packages(&s);
    let dir = bundle_dir_for("deb", None, &rust);
    let deb = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
    fs::write(
        &deb,
        noise(native_kind("deb").floor_bytes as usize + 1024, 3),
    )
    .unwrap();
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, s.path(), &rust, "linux", &f),
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "magic",
    );
    assert!(calls.is_empty());
}

#[test]
fn a_bundle_under_the_size_floor_is_refused_rather_than_smoked() {
    let s = Scratch::new("smoke-built");
    let (_, _, rust) = native_packages(&s);
    let dir = bundle_dir_for("deb", None, &rust);
    let deb = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
    fs::write(&deb, b"!<arch>\n").unwrap();
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, s.path(), &rust, "linux", &f),
            &mut no_tools(),
            &mut window_run(&mut calls, V, Some(&ZIP), 18),
        ),
        "floor",
    );
    assert!(calls.is_empty());
}

#[test]
fn a_wrong_version_window_inside_a_built_bundle_fails_the_whole_run() {
    let s = Scratch::new("smoke-built");
    let (_, _, rust) = native_packages(&s);
    let work = s.fresh("badverwork");
    let f = fixture();
    let mut calls = vec![];
    err_contains(
        smoke_built_bundles(
            &built(BundleOs::Linux, &work, &rust, "linux", &f),
            &mut no_tools(),
            &mut window_run(&mut calls, "0.5.4", Some(&ZIP), 18),
        ),
        "0.5.4",
    );
}

// ---- the real window ----

/// The workspace's own built window, if this machine has built one.
fn built_window() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "method-reader-app.exe"
    } else {
        "method-reader-app"
    };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"));
    ["release", "debug"]
        .iter()
        .map(|profile| target.join(profile).join(exe))
        .find(|p| p.is_file())
}

#[test]
fn the_real_window_runs_headless_and_converts_the_fixture() {
    // What a real smoke runs inside a bundle: the window built from this
    // checkout (`cargo build -p sp-app` first), with the real runner.
    let Some(window) = built_window() else {
        eprintln!(
            "skipped: no method-reader-app built in this workspace (cargo build -p sp-app --bin method-reader-app)"
        );
        return;
    };
    let version = xtask::cli_build::workspace_version(&workspace_root()).unwrap();
    let s = Scratch::new("smoke-real");
    let work = s.fresh("real");
    smoke_window(
        &window,
        &fixture(),
        &work,
        &version,
        &mut |argv: &[String], env: &[(String, String)]| real_run_env(argv, env),
    )
    .unwrap();
    assert!(work.join("smoke.epub").is_file());
    // A version it is not is refused by the real window's own answer.
    let other = s.fresh("other");
    err_contains(
        smoke_window(
            &window,
            &fixture(),
            &other,
            "0.0.1",
            &mut |argv: &[String], env: &[(String, String)]| real_run_env(argv, env),
        ),
        "0.0.1",
    );
    assert!(
        repo_root()
            .join("tests/fixtures/field-station.pdf")
            .is_file()
    );
}

// ---- the command ----

#[test]
fn the_command_needs_an_expected_version_and_a_bundle_and_knows_one_window() {
    let (code, _, err) = xtask(&["smoke-bundle"]);
    assert_eq!(code, 1);
    assert!(err.contains("--expect-version"), "{err}");
    let (code, _, err) = xtask(&["smoke-bundle", "--expect-version", "0.6.0"]);
    assert_eq!(code, 1);
    assert!(err.contains("--bundle"), "{err}");
    let (code, _, err) = xtask(&[
        "smoke-bundle",
        "--expect-version",
        "0.6.0",
        "--app",
        "tauri",
    ]);
    assert_eq!(code, 1);
    assert!(err.contains("Tauri window is gone"), "{err}");
}
