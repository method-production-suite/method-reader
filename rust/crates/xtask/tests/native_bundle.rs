//! The native window's bundles (no TypeScript original: this is the
//! cutover's new half of build-app-bundle). Every outside tool is a fake
//! behind the runner seam; the .deb, .rpm, .app and .app.tar.gz are real,
//! written in Rust and read back. Where `rpm` or `dpkg-deb` is installed
//! the packages are also handed to it; elsewhere that check says it skipped.

mod packaging_common;

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use packaging_common::*;
use xtask::packaging::archive::{bundle_entries, deb_control_field, find_entry, rpm_header_string};
use xtask::packaging::bundle::{
    BundleArgs, BundleDirs, Env, assert_native_versions, build_bundles, parse_bundle_args,
    resolve_app,
};
use xtask::packaging::kinds::{
    AppSource, BUNDLE_KINDS, BundleArch, BundleOs, Kind, MAC_TARGET, MagicAt, NATIVE_FLOOR,
    bundle_dir_for, for_app,
};
use xtask::packaging::manifest::collect_signed_artifacts;
use xtask::packaging::native::{
    NATIVE_APP, NativeTools, binary_path, cargo_argv, info_plist, package_version, plist_string,
};
use xtask::packaging::signature::verify_file;
use xtask::packaging::util::{RunResult, parse_checksums, sha256_file};
use xtask::paths::repo_root;

const VERSION: &str = "0.12.0";

/// A repository with what the native path reads: the workspace version,
/// sp-app inheriting it, the icons and the two resources. No desktop/.
fn native_repo(s: &Scratch, version: &str) -> BundleDirs {
    let repo = s.fresh("repo");
    fs::write(repo.join("LICENSE"), "GNU AFFERO GENERAL PUBLIC LICENSE").unwrap();
    fs::write(repo.join("THIRD-PARTY-NOTICES.md"), "# Notices").unwrap();
    let rust = repo.join("rust");
    fs::create_dir_all(rust.join("crates/sp-app/icons")).unwrap();
    fs::write(
        rust.join("Cargo.toml"),
        format!("[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    fs::write(
        rust.join("crates/sp-app/Cargo.toml"),
        "[package]\nname = \"sp-app\"\nversion.workspace = true\n",
    )
    .unwrap();
    for icon in [
        "icon.icns",
        "icon.ico",
        "32x32.png",
        "128x128.png",
        "128x128@2x.png",
    ] {
        fs::write(
            rust.join("crates/sp-app/icons").join(icon),
            format!("icon bytes of {icon}"),
        )
        .unwrap();
    }
    BundleDirs::for_repo(&repo)
}

fn native_kind(id: &str) -> Kind {
    for_app(
        *BUNDLE_KINDS.iter().find(|k| k.id == id).unwrap(),
        AppSource::Native,
    )
}

/// A file that passes `k`'s magic check and clears its floor.
fn plausible(path: &Path, k: &Kind) {
    let size = k.floor_bytes + 1024;
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap();
    f.set_len(size).unwrap();
    f.seek(SeekFrom::Start(if k.magic_at == MagicAt::Head {
        0
    } else {
        size - 512
    }))
    .unwrap();
    f.write_all(k.magic).unwrap();
}

fn the_binary() -> Vec<u8> {
    noise(NATIVE_FLOOR as usize + 65_536, 0x5EED)
}

/// Every outside tool the native path calls, faked: cargo writes the
/// binary, hdiutil and makensis write their containers, notarytool answers.
struct FakeTools {
    calls: Vec<Vec<String>>,
    rust: PathBuf,
    os: BundleOs,
    notary_status: &'static str,
}

impl FakeTools {
    fn new(dirs: &BundleDirs, os: BundleOs) -> Self {
        FakeTools {
            calls: vec![],
            rust: dirs.rust_dir.clone(),
            os,
            notary_status: "Accepted",
        }
    }

    fn run(&mut self, argv: &[String], _cwd: Option<&Path>) -> RunResult {
        self.calls.push(argv.to_vec());
        let tool = Path::new(&argv[0])
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        match tool.as_str() {
            "cargo" => {
                let target = argv
                    .iter()
                    .position(|a| a == "--target")
                    .map(|i| argv[i + 1].clone());
                let bin = binary_path(&self.rust, target.as_deref(), self.os);
                fs::create_dir_all(bin.parent().unwrap()).unwrap();
                fs::write(&bin, the_binary()).unwrap();
            }
            "hdiutil" => plausible(Path::new(argv.last().unwrap()), &native_kind("dmg")),
            "makensis" => {
                let script = fs::read_to_string(&argv[1]).unwrap();
                let at = script.find("OutFile \"").unwrap() + 9;
                let out = &script[at..at + script[at..].find('"').unwrap()];
                plausible(Path::new(out), &native_kind("nsis"));
            }
            "xcrun" if argv[1] == "notarytool" => {
                return RunResult::ok(
                    &format!("{{\"id\":\"abc\",\"status\":\"{}\"}}", self.notary_status),
                    "",
                );
            }
            _ => {}
        }
        RunResult::ok("", "")
    }

    fn tools(&self) -> Vec<String> {
        self.calls
            .iter()
            .map(|c| {
                let t = Path::new(&c[0])
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if t == "xcrun" {
                    format!("xcrun {}", c[1])
                } else {
                    t
                }
            })
            .collect()
    }
}

fn native_args(out: &Path, os: BundleOs, arch: BundleArch) -> BundleArgs {
    BundleArgs {
        target: (os == BundleOs::Macos).then(|| MAC_TARGET.to_string()),
        ..BundleArgs::new(VERSION, out, os, arch)
    }
}

fn names_of(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn env_of(pairs: &[(&str, &str)]) -> Env {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ---- the handover: the native app is the Tauri app's successor ----

#[test]
fn the_native_app_keeps_the_tauri_window_s_identity() {
    let conf_path = repo_root().join("desktop/src-tauri/tauri.conf.json");
    let Ok(text) = fs::read_to_string(&conf_path) else {
        eprintln!("skipped: tauri.conf.json is gone (cutover)");
        return;
    };
    let conf: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(conf["identifier"], NATIVE_APP.identifier);
    assert_eq!(conf["productName"], NATIVE_APP.product_name);
    assert_eq!(conf["bundle"]["publisher"], NATIVE_APP.publisher);
    assert_eq!(
        conf["bundle"]["shortDescription"],
        NATIVE_APP.short_description
    );
    assert_eq!(
        conf["bundle"]["longDescription"],
        NATIVE_APP.long_description
    );
    assert_eq!(
        conf["bundle"]["macOS"]["minimumSystemVersion"],
        NATIVE_APP.minimum_macos
    );
    let resources: Vec<&str> = conf["bundle"]["resources"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_str().unwrap())
        .collect();
    for (_, name) in NATIVE_APP.resources {
        assert!(resources.contains(name), "{name}");
    }
}

#[test]
fn the_table_names_the_binary_sp_app_actually_builds() {
    let toml = fs::read_to_string(repo_root().join("rust/crates/sp-app/Cargo.toml")).unwrap();
    assert!(toml.contains(&format!("name = \"{}\"", NATIVE_APP.crate_name)));
    assert!(toml.contains(&format!("name = \"{}\"", NATIVE_APP.binary)));
    assert!(toml.contains("version.workspace = true"));
}

#[test]
fn the_native_window_is_chosen_once_tauri_conf_json_is_gone_or_when_asked() {
    let s = Scratch::new("native-choose");
    let dirs = native_repo(&s, VERSION);
    assert_eq!(resolve_app(None, &dirs), AppSource::Native);
    assert_eq!(resolve_app(Some(AppSource::Tauri), &dirs), AppSource::Tauri);
    let a = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        parse_bundle_args(
            &a(&["--version", "0.12.0", "--out", "o", "--app", "native"]),
            "linux",
            "x64"
        )
        .unwrap()
        .app,
        Some(AppSource::Native)
    );
    assert_eq!(
        parse_bundle_args(
            &a(&["--version", "0.12.0", "--out", "o", "--app", "tauri"]),
            "linux",
            "x64"
        )
        .unwrap()
        .app,
        Some(AppSource::Tauri)
    );
    err_contains(
        parse_bundle_args(
            &a(&["--version", "0.12.0", "--out", "o", "--app", "electron"]),
            "linux",
            "x64",
        ),
        "electron",
    );
}

#[test]
fn the_native_version_gate_reads_the_workspace_and_names_it() {
    let s = Scratch::new("native-gate");
    let dirs = native_repo(&s, "0.11.1");
    let e = err_contains(
        assert_native_versions(VERSION, &dirs.rust_dir),
        "rust/Cargo.toml",
    );
    assert!(e.contains("0.11.1") && e.contains(VERSION));
    let dirs = native_repo(&s, VERSION);
    assert_native_versions(VERSION, &dirs.rust_dir).unwrap();
    // sp-app may not carry a version of its own.
    fs::write(
        dirs.rust_dir.join("crates/sp-app/Cargo.toml"),
        "[package]\nname = \"sp-app\"\nversion = \"0.12.0\"\n",
    )
    .unwrap();
    err_contains(
        assert_native_versions(VERSION, &dirs.rust_dir),
        "version.workspace",
    );
}

#[test]
fn the_gate_stops_a_native_run_before_cargo_and_a_tauri_overlay_is_refused() {
    let s = Scratch::new("native-gate");
    let dirs = native_repo(&s, "0.11.1");
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    err_contains(
        build_bundles(
            &native_args(&s.join("o"), BundleOs::Linux, BundleArch::X64),
            &mut |a, c| fake.run(a, c),
            &dirs,
            &Env::new(),
        ),
        "0.11.1",
    );
    let dirs = native_repo(&s, VERSION);
    let a = BundleArgs {
        config: Some("tauri.transition.conf.json".into()),
        ..native_args(&s.join("o"), BundleOs::Linux, BundleArch::X64)
    };
    err_contains(
        build_bundles(&a, &mut |x, c| fake.run(x, c), &dirs, &Env::new()),
        "--config",
    );
    assert!(fake.calls.is_empty());
}

#[test]
fn the_cargo_build_is_the_window_crate_locked_and_targeted() {
    let tools = NativeTools::from_env(&Env::new());
    assert_eq!(
        cargo_argv(&tools, Some(MAC_TARGET)),
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--package",
            "sp-app",
            "--bin",
            "screepub-window",
            "--target",
            MAC_TARGET
        ]
    );
    let tools = NativeTools::from_env(&env_of(&[
        ("SCREEPUB_HDIUTIL", "/fake/hdiutil"),
        ("SCREEPUB_CARGO", "/x/cargo"),
    ]));
    assert_eq!(tools.hdiutil, "/fake/hdiutil");
    assert_eq!(cargo_argv(&tools, None)[0], "/x/cargo");
    assert!(
        binary_path(Path::new("r"), None, BundleOs::Windows)
            .ends_with("target/release/screepub-window.exe")
    );
}

// ---- Linux: the .deb and the .rpm, written here ----

#[test]
fn a_native_linux_run_writes_a_deb_and_an_rpm_that_read_back_whole() {
    let s = Scratch::new("native-linux");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.join("out");
    let env = env_of(&[("SOURCE_DATE_EPOCH", "1767225600")]);
    let made = build_bundles(
        &native_args(&out, BundleOs::Linux, BundleArch::X64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &env,
    )
    .unwrap();
    assert_eq!(
        names_of(&made),
        ["Screepub-linux-amd64.deb", "Screepub-linux-x86_64.rpm"]
    );
    assert_eq!(fake.tools(), ["cargo"]);

    let deb = out.join("Screepub-linux-amd64.deb");
    let rpm = out.join("Screepub-linux-x86_64.rpm");
    for pkg in [&deb, &rpm] {
        let entries = bundle_entries(pkg).unwrap();
        let bin = find_entry(&entries, "usr/bin/screepub-window").unwrap();
        assert_eq!(bin.data, the_binary(), "{}", pkg.display());
        assert_eq!(bin.mode & 0o777, 0o755);
        let desktop = find_entry(&entries, "usr/share/applications/Screepub.desktop").unwrap();
        let text = String::from_utf8_lossy(&desktop.data);
        assert!(
            text.contains("Exec=screepub-window\n") && text.contains("Icon=screepub\n"),
            "{text}"
        );
        assert_eq!(
            find_entry(&entries, "usr/lib/Screepub/LICENSE")
                .unwrap()
                .mode
                & 0o111,
            0
        );
        find_entry(&entries, "usr/lib/Screepub/THIRD-PARTY-NOTICES.md").unwrap();
        for size in ["32x32", "128x128", "256x256@2"] {
            find_entry(
                &entries,
                &format!("usr/share/icons/hicolor/{size}/apps/screepub.png"),
            )
            .unwrap();
        }
        assert_eq!(entries.len(), 7);
    }
    assert_eq!(
        deb_control_field(&deb, "Package").unwrap().as_deref(),
        Some("screepub")
    );
    assert_eq!(
        deb_control_field(&deb, "Version").unwrap().as_deref(),
        Some(VERSION)
    );
    assert_eq!(
        deb_control_field(&deb, "Architecture").unwrap().as_deref(),
        Some("amd64")
    );
    let rpm_bytes = fs::read(&rpm).unwrap();
    assert_eq!(
        rpm_header_string(&rpm_bytes, 1000).unwrap().as_deref(),
        Some("screepub")
    );
    assert_eq!(
        rpm_header_string(&rpm_bytes, 1001).unwrap().as_deref(),
        Some(VERSION)
    );
    assert_eq!(
        rpm_header_string(&rpm_bytes, 1022).unwrap().as_deref(),
        Some("x86_64")
    );
    assert_eq!(
        rpm_header_string(&rpm_bytes, 1125).unwrap().as_deref(),
        Some("gzip")
    );

    let sums = parse_checksums(&fs::read_to_string(out.join("SHA256SUMS-app")).unwrap());
    for (name, digest) in &sums {
        assert_eq!(*digest, sha256_file(&out.join(name)).unwrap());
    }
    assert_eq!(sums.len(), 2);
    if let Some(keep) = std::env::var_os("XTASK_KEEP_PACKAGES") {
        fs::copy(&deb, Path::new(&keep).join("native.deb")).unwrap();
        fs::copy(&rpm, Path::new(&keep).join("native.rpm")).unwrap();
    }
}

#[test]
fn the_rpm_s_signature_header_carries_the_main_header_s_digest_and_sizes() {
    use sha2::{Digest, Sha256};
    let s = Scratch::new("native-rpm");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.join("out");
    build_bundles(
        &native_args(&out, BundleOs::Linux, BundleArch::Arm64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &Env::new(),
    )
    .unwrap();
    let b = fs::read(out.join("Screepub-linux-aarch64.rpm")).unwrap();
    let be = |at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    // The signature header and its 8-byte pad, then the main header.
    let sig_n = be(96 + 8);
    let sig_store = 96 + 16 + sig_n * 16;
    let sig_end = sig_store + be(96 + 12);
    let main = sig_end + (8 - sig_end % 8) % 8;
    let main_end = main + 16 + be(main + 8) * 16 + be(main + 12);
    let tag = |n: u32| -> (u32, usize) {
        (0..sig_n)
            .map(|i| 96 + 16 + i * 16)
            .find(|&e| be(e) as u32 == n)
            .map(|e| (be(e + 4) as u32, sig_store + be(e + 8)))
            .unwrap()
    };
    // Each header opens with its region tag: 62 for the signature, 63 for the main.
    assert_eq!(be(96 + 16), 62);
    assert_eq!(be(main + 16), 63);
    let (kind, at) = tag(273);
    assert_eq!(kind, 6);
    let digest: String = Sha256::digest(&b[main..main_end])
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    assert_eq!(&b[at..at + 64], digest.as_bytes());
    let (_, at) = tag(1000);
    assert_eq!(be(at), b.len() - main);
    assert_eq!(
        rpm_header_string(&b, 1022).unwrap().as_deref(),
        Some("aarch64")
    );
}

#[test]
fn a_prerelease_sorts_before_its_release_in_both_package_formats() {
    assert_eq!(package_version("0.12.0-rc1"), "0.12.0~rc1");
    assert_eq!(package_version("0.12.0"), "0.12.0");
}

fn tool_works(tool: &str, arg: &str) -> bool {
    Command::new(tool)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn the_real_package_tools_accept_them_where_installed() {
    let have_rpm = tool_works("rpm", "--version");
    let have_dpkg = tool_works("dpkg-deb", "--version");
    if !have_rpm && !have_dpkg {
        eprintln!("skipped: neither rpm nor dpkg-deb is installed here");
        return;
    }
    let s = Scratch::new("native-real-tools");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    let out = s.join("out");
    build_bundles(
        &native_args(&out, BundleOs::Linux, BundleArch::X64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &Env::new(),
    )
    .unwrap();
    if have_dpkg {
        let o = Command::new("dpkg-deb")
            .arg("--info")
            .arg(out.join("Screepub-linux-amd64.deb"))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let o = Command::new("dpkg-deb")
            .arg("--contents")
            .arg(out.join("Screepub-linux-amd64.deb"))
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&o.stdout).contains("./usr/bin/screepub-window"));
    } else {
        eprintln!("skipped the dpkg-deb half: not installed");
    }
    if have_rpm {
        let o = Command::new("rpm")
            .args(["-qp", "--queryformat", "%{NAME} %{VERSION} %{ARCH}\\n"])
            .arg(out.join("Screepub-linux-x86_64.rpm"))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(String::from_utf8_lossy(&o.stdout).contains("screepub 0.12.0 x86_64"));
        let o = Command::new("rpm")
            .arg("-qpl")
            .arg(out.join("Screepub-linux-x86_64.rpm"))
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&o.stdout).contains("/usr/bin/screepub-window"));
    } else {
        eprintln!("skipped the rpm half: not installed");
    }
}

#[test]
fn missing_icons_are_named_with_every_folder_searched() {
    let s = Scratch::new("native-icons");
    let dirs = native_repo(&s, VERSION);
    fs::remove_file(dirs.rust_dir.join("crates/sp-app/icons/128x128.png")).unwrap();
    let mut fake = FakeTools::new(&dirs, BundleOs::Linux);
    let e = err_contains(
        build_bundles(
            &native_args(&s.join("o"), BundleOs::Linux, BundleArch::X64),
            &mut |a, c| fake.run(a, c),
            &dirs,
            &Env::new(),
        ),
        "128x128.png",
    );
    assert!(
        e.contains("rust/crates/sp-app/icons") && e.contains("desktop/src-tauri/icons"),
        "{e}"
    );
}

// ---- macOS: the .app, the image and the updater archive ----

#[test]
fn info_plist_says_what_the_tauri_app_said_about_itself() {
    let plist = info_plist(VERSION);
    assert_eq!(
        plist_string(&plist, "CFBundleIdentifier").as_deref(),
        Some("com.darkwell.screepub")
    );
    assert_eq!(
        plist_string(&plist, "CFBundleExecutable").as_deref(),
        Some("Screepub")
    );
    assert_eq!(
        plist_string(&plist, "CFBundleShortVersionString").as_deref(),
        Some(VERSION)
    );
    assert_eq!(
        plist_string(&plist, "CFBundleVersion").as_deref(),
        Some(VERSION)
    );
    assert_eq!(
        plist_string(&plist, "LSMinimumSystemVersion").as_deref(),
        Some("13.0")
    );
    assert_eq!(
        plist_string(&plist, "CFBundlePackageType").as_deref(),
        Some("APPL")
    );
    assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist"));
}

#[test]
fn a_local_mac_build_is_signed_ad_hoc_and_never_notarized() {
    let s = Scratch::new("native-mac");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Macos);
    let out = s.join("out");
    let made = build_bundles(
        &native_args(&out, BundleOs::Macos, BundleArch::Arm64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &Env::new(),
    )
    .unwrap();
    assert_eq!(names_of(&made), ["Screepub-macOS.dmg"]);
    assert_eq!(fake.tools(), ["cargo", "codesign", "hdiutil"]);
    let app = bundle_dir_for("macos", Some(MAC_TARGET), &dirs.rust_dir).join("Screepub.app");
    assert_eq!(
        fake.calls[1],
        ["codesign", "--force", "--sign", "-", app.to_str().unwrap()]
    );
    let hd = &fake.calls[2];
    assert_eq!(
        hd[..5],
        ["hdiutil", "create", "-volname", "Screepub", "-srcfolder"]
    );
    assert!(hd.iter().any(|a| a == "UDZO"));
    let plist = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
    assert_eq!(
        plist_string(&plist, "CFBundleIdentifier").as_deref(),
        Some("com.darkwell.screepub")
    );
    assert_eq!(
        fs::read(app.join("Contents/MacOS/Screepub")).unwrap(),
        the_binary()
    );
    assert!(app.join("Contents/Resources/icon.icns").is_file());
    assert!(app.join("Contents/Resources/LICENSE").is_file());
}

fn release_env() -> Env {
    env_of(&[
        (
            "APPLE_SIGNING_IDENTITY",
            "Developer ID Application: Example (TEAMID1234)",
        ),
        ("APPLE_ID", "release@example.com"),
        ("APPLE_PASSWORD", "app-specific-password"),
        ("APPLE_TEAM_ID", "TEAMID1234"),
        ("TAURI_SIGNING_PRIVATE_KEY", TAURI_TEST_KEY),
        ("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""),
        ("SOURCE_DATE_EPOCH", "1767225600"),
    ])
}

fn mac_release_args(out: &Path, pubkey: &str) -> BundleArgs {
    BundleArgs {
        updater: true,
        pubkey: Some(pubkey.to_string()),
        ..native_args(out, BundleOs::Macos, BundleArch::Arm64)
    }
}

/// Unpacks the archive the way tauri-plugin-updater 2.12.0 installs it on
/// macOS: every path's first component dropped, the rest laid over the app.
fn unpack_like_the_updater(archive: &Path) -> Vec<(String, u32, Vec<u8>)> {
    let gz = flate2::read::GzDecoder::new(fs::File::open(archive).unwrap());
    let mut tar = tar::Archive::new(gz);
    let mut out = Vec::new();
    for entry in tar.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path: PathBuf = entry.path().unwrap().iter().skip(1).collect();
        let mode = entry.header().mode().unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.push((path.to_string_lossy().replace('\\', "/"), mode, data));
    }
    out
}

#[test]
fn a_release_mac_build_signs_notarizes_staples_and_signs_the_updater_archive() {
    let s = Scratch::new("native-mac-release");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Macos);
    let out = s.join("out");
    let pubkey = tauri_test_public_key();
    let made = build_bundles(
        &mac_release_args(&out, &pubkey),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &release_env(),
    )
    .unwrap();
    let mut names = names_of(&made);
    names.sort();
    assert_eq!(
        names,
        [
            "Screepub-macOS.app.tar.gz",
            "Screepub-macOS.app.tar.gz.sig",
            "Screepub-macOS.dmg"
        ]
    );
    assert_eq!(
        fake.tools(),
        [
            "cargo",
            "codesign",
            "ditto",
            "xcrun notarytool",
            "xcrun stapler",
            "hdiutil",
            "codesign"
        ]
    );
    let sign = &fake.calls[1];
    for want in [
        "--options",
        "runtime",
        "--timestamp",
        "Developer ID Application: Example (TEAMID1234)",
    ] {
        assert!(sign.iter().any(|a| a == want), "{want}");
    }
    let submit = &fake.calls[3];
    for want in ["submit", "--apple-id", "--team-id", "--wait"] {
        assert!(submit.iter().any(|a| a == want), "{want}");
    }
    assert!(fake.calls[6].last().unwrap().ends_with(".dmg"));

    // The archive verifies against the key the shipped app trusts, and the
    // manifest builder takes it as the darwin-aarch64 entry.
    let tar = out.join("Screepub-macOS.app.tar.gz");
    let sig = fs::read_to_string(out.join("Screepub-macOS.app.tar.gz.sig")).unwrap();
    verify_file(&tar, &sig, &pubkey).unwrap();
    let found = collect_signed_artifacts(&out).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].platforms, ["darwin-aarch64"]);

    // What a 0.11.x window's updater lays over its own Screepub.app.
    let unpacked = unpack_like_the_updater(&tar);
    let file = |p: &str| {
        unpacked
            .iter()
            .find(|(n, _, _)| n == p)
            .unwrap_or_else(|| panic!("{p} not in the archive"))
    };
    let (_, mode, data) = file("Contents/MacOS/Screepub");
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(*data, the_binary());
    let (_, mode, plist) = file("Contents/Info.plist");
    assert_eq!(mode & 0o777, 0o644);
    let plist = String::from_utf8_lossy(plist);
    assert_eq!(
        plist_string(&plist, "CFBundleIdentifier").as_deref(),
        Some("com.darkwell.screepub")
    );
    assert_eq!(
        plist_string(&plist, "CFBundleExecutable").as_deref(),
        Some("Screepub")
    );
    assert_eq!(
        plist_string(&plist, "CFBundleShortVersionString").as_deref(),
        Some(VERSION)
    );
    // One top-level folder, and it is the app the updater replaces.
    let gz = flate2::read::GzDecoder::new(fs::File::open(&tar).unwrap());
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let first = entry
            .path()
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(first, "Screepub.app");
    }
}

#[test]
fn a_rejected_notarization_stops_the_run_before_any_image_is_made() {
    let s = Scratch::new("native-mac-notary");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Macos);
    fake.notary_status = "Invalid";
    let out = s.join("out");
    err_contains(
        build_bundles(
            &mac_release_args(&out, &tauri_test_public_key()),
            &mut |a, c| fake.run(a, c),
            &dirs,
            &release_env(),
        ),
        "Invalid",
    );
    assert!(!fake.tools().iter().any(|t| t == "hdiutil"));
    assert!(!out.join("Screepub-macOS.dmg").exists());
}

#[test]
fn an_archive_the_shipped_app_s_key_would_not_verify_is_never_published() {
    let s = Scratch::new("native-mac-key");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Macos);
    let out = s.join("out");
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let other_pk = b64(&other.pk.to_box().unwrap().to_string());
    err_contains(
        build_bundles(
            &mac_release_args(&out, &other_pk),
            &mut |a, c| fake.run(a, c),
            &dirs,
            &release_env(),
        ),
        "does not verify",
    );
    assert!(!out.join("Screepub-macOS.app.tar.gz").exists());
}

#[test]
fn updater_with_no_public_key_to_check_against_is_refused() {
    let s = Scratch::new("native-mac-nokey");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Macos);
    let a = BundleArgs {
        updater: true,
        ..native_args(&s.join("out"), BundleOs::Macos, BundleArch::Arm64)
    };
    err_contains(
        build_bundles(&a, &mut |x, c| fake.run(x, c), &dirs, &release_env()),
        "--pubkey",
    );
}

// ---- Windows: the NSIS installer ----

#[test]
fn a_native_windows_run_scripts_a_per_user_installer_over_the_tauri_one() {
    let s = Scratch::new("native-win");
    let dirs = native_repo(&s, VERSION);
    let mut fake = FakeTools::new(&dirs, BundleOs::Windows);
    let out = s.join("out");
    let made = build_bundles(
        &native_args(&out, BundleOs::Windows, BundleArch::X64),
        &mut |a, c| fake.run(a, c),
        &dirs,
        &Env::new(),
    )
    .unwrap();
    assert_eq!(names_of(&made), ["Screepub-windows-x64-setup.exe"]);
    assert_eq!(fake.tools(), ["cargo", "makensis"]);
    let script = fs::read_to_string(&fake.calls[1][1]).unwrap();
    for want in [
        "InstallDir \"$LOCALAPPDATA\\${PRODUCTNAME}\"",
        "RequestExecutionLevel user",
        "!define PRODUCTNAME \"Screepub\"",
        "!define VERSION \"0.12.0\"",
        "VIProductVersion \"0.12.0.0\"",
        "Delete \"$INSTDIR\\screepub-desktop.exe\"",
        "Delete \"$INSTDIR\\screepub-engine.exe\"",
        "Screepub.exe\"",
        "WriteUninstaller",
        "DeleteRegKey HKCU \"${UNINSTKEY}\"",
    ] {
        assert!(script.contains(want), "the script lacks {want}");
    }
    let stage = Path::new(&fake.calls[1][1]).parent().unwrap().to_path_buf();
    assert_eq!(fs::read(stage.join("Screepub.exe")).unwrap(), the_binary());
    assert!(stage.join("icon.ico").is_file());
}
