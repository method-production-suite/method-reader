//! What the release files must be to the tools outside Method Reader that read
//! them: tauri-cli's key format, tauri-plugin-updater's unpack, and the real
//! dpkg-deb and rpm. Moved here from the xtask packaging tests when the
//! release tools became one implementation (rust/PARITY.md, Decisions).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sp_release::{archive, linux, mac, sign};

/// tauri-cli 2.11.4's own test key (helpers/updater_signature.rs),
/// published in its source and encrypted with an EMPTY password. Not a
/// credential.
const TAURI_TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5dkpDN09RZm5GeVAzc2RuYlNzWVVJelJRQnNIV2JUcGVXZUplWXZXYXpqUUFBQkFBQUFBQUFBQUFBQUlBQUFBQTZrN2RnWGh5dURxSzZiL1ZQSDdNcktiaHRxczQwMXdQelRHbjRNcGVlY1BLMTBxR2dpa3I3dDE1UTVDRDE4MXR4WlQwa1BQaXdxKy9UU2J2QmVSNXhOQWFDeG1GSVllbUNpTGJQRkhhTnROR3I5RmdUZi90OGtvaGhJS1ZTcjdZU0NyYzhQWlQ5cGM9Cg==";

#[test]
fn the_pinned_minisign_opens_tauri_cli_s_own_key_and_signs_as_it_did() {
    // Later 0.7.x releases of minisign could not open a key encrypted with
    // an empty password, which is what `cargo tauri signer generate` makes
    // when no password is given: the workspace pins =0.7.3 for this.
    let sk = sign::load_secret_key(TAURI_TEST_KEY, "").unwrap();
    let pk = minisign::PublicKey::from_secret_key(&sk).unwrap();
    let pk = STANDARD.encode(pk.to_box().unwrap().to_string());
    let data = b"an update archive";
    let sig = sign::sign(
        &sk,
        data,
        "Method-Reader-macOS.app.tar.gz",
        "0.12.0",
        1_758_470_000,
    )
    .unwrap();
    let text = String::from_utf8(STANDARD.decode(&sig).unwrap()).unwrap();
    assert!(text.starts_with("untrusted comment: signature from tauri secret key\n"));
    assert!(text.contains(
        "trusted comment: timestamp:1758470000\tfile:Method-Reader-macOS.app.tar.gz\tversion:0.12.0\n"
    ));
    // What the readers take, and the key it names is the key's.
    let read = sp_release::boxes::parse_signature_box(&sig).unwrap();
    assert_eq!(
        read.file_name.as_deref(),
        Some("Method-Reader-macOS.app.tar.gz")
    );
    assert_eq!(
        sp_release::boxes::parse_public_key_box(&pk).unwrap().key_id,
        read.key_id
    );
    for required in [false, true] {
        sp_update::verify::verify_signature(data, &sig, &pk, "0.12.0", required).unwrap();
    }
    assert!(sign::load_secret_key(TAURI_TEST_KEY, "not the password").is_err());
}

fn the_window() -> Vec<u8> {
    (0..4096u32).map(|i| (i * 31 % 251) as u8).collect()
}

/// A laid-out Method Reader.app, its window written WITHOUT the executable bit
/// (as a Windows host or a careless copy leaves it).
fn app(dir: &Path) -> PathBuf {
    let bin = dir.join("method-reader-app");
    std::fs::write(&bin, the_window()).unwrap();
    let icon = dir.join("icon.icns");
    std::fs::write(&icon, b"icns").unwrap();
    let lic = dir.join("LICENSE");
    std::fs::write(&lic, b"AGPL").unwrap();
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let bundle = mac::build_app(
        &mac::MacApp {
            version: "0.12.0",
            identifier: sp_update::IDENTIFIER,
            window: &bin,
            helpers: &[],
            icon: &icon,
            resources: &[lic],
        },
        &out,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = bundle.join("Contents/MacOS/method-reader-app");
        std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    bundle
}

/// Unpacks the archive the way tauri-plugin-updater 2.12.0 installs it on
/// macOS: every path's first component dropped, the rest laid over the app.
fn unpack_like_the_plugin(bytes: &[u8]) -> Vec<(String, u32, Vec<u8>)> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
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
fn the_update_archive_unpacks_as_the_plugin_and_the_rust_app_unpack_it() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = archive::app_archive(&app(dir.path()), 1_767_225_600).unwrap();

    let files = unpack_like_the_plugin(&bytes);
    let file = |p: &str| {
        files
            .iter()
            .find(|(n, _, _)| n == p)
            .unwrap_or_else(|| panic!("{p} is not in the archive"))
    };
    let (_, mode, data) = file("Contents/MacOS/method-reader-app");
    assert_eq!(mode & 0o777, 0o755, "the window must run once installed");
    assert_eq!(*data, the_window());
    let (_, mode, plist) = file("Contents/Info.plist");
    assert_eq!(mode & 0o777, 0o644);
    let plist = String::from_utf8_lossy(plist);
    assert!(
        plist
            .contains("<key>CFBundleIdentifier</key>\n\t<string>com.methodreader.desktop</string>")
    );
    // One top folder, the app the updater replaces.
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    for e in tar.entries().unwrap() {
        let e = e.unwrap();
        let first = e.path().unwrap().iter().next().unwrap().to_owned();
        assert_eq!(first, "Method Reader.app");
    }

    // The Rust app's own unpack takes it too, and reads the identifier back,
    // into the old bundle's place as the 0.12 updater installs it.
    let dest = dir.path().join("installed/Screepub.app");
    sp_update::install::unpack_app_archive(&bytes, &dest).unwrap();
    assert_eq!(
        sp_update::install::bundle_identifier(&dest).as_deref(),
        Some(sp_update::IDENTIFIER)
    );
}

fn tool_works(tool: &str, arg: &str) -> bool {
    Command::new(tool)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn the_real_package_tools_accept_the_deb_and_the_rpm_where_installed() {
    let have_rpm = tool_works("rpm", "--version");
    let have_dpkg = tool_works("dpkg-deb", "--version");
    if !have_rpm && !have_dpkg {
        eprintln!("skipped: neither rpm nor dpkg-deb is installed here (CI's Linux leg has both)");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("method-reader-app");
    std::fs::write(&bin, the_window()).unwrap();
    let png = dir.path().join("32x32.png");
    std::fs::write(&png, b"png").unwrap();
    let lic = dir.path().join("LICENSE");
    std::fs::write(&lic, b"AGPL").unwrap();
    let icons = vec![("32x32".to_string(), png)];
    let resources = vec![lic];
    let app = linux::LinuxApp {
        version: "0.12.0-rc1",
        window: &bin,
        icons: &icons,
        resources: &resources,
        depends: "",
    };
    if have_dpkg {
        let deb = dir.path().join("Method-Reader-linux-amd64.deb");
        std::fs::write(&deb, linux::build_deb(&app, "amd64", 7).unwrap()).unwrap();
        let o = Command::new("dpkg-deb")
            .arg("--info")
            .arg(&deb)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(String::from_utf8_lossy(&o.stdout).contains("Version: 0.12.0~rc1"));
        let o = Command::new("dpkg-deb")
            .arg("--contents")
            .arg(&deb)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&o.stdout).contains("./usr/bin/method-reader-app"));
        // dpkg itself reads the Depends: every run-time library's package.
        let o = Command::new("dpkg-deb")
            .args(["--field"])
            .arg(&deb)
            .arg("Depends")
            .output()
            .unwrap();
        let depends = String::from_utf8_lossy(&o.stdout);
        for (_, pkg) in linux::LOADED_AT_RUN_TIME {
            assert!(
                depends.split(',').any(|d| d.trim() == *pkg),
                "{pkg}: {depends}"
            );
        }
    } else {
        eprintln!("skipped the dpkg-deb half: not installed");
    }
    if have_rpm {
        let rpm = dir.path().join("Method-Reader-linux-x86_64.rpm");
        std::fs::write(&rpm, linux::build_rpm(&app, "x86_64", 7).unwrap()).unwrap();
        let o = Command::new("rpm")
            .args(["-qp", "--queryformat", "%{NAME} %{VERSION} %{ARCH}\\n"])
            .arg(&rpm)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(String::from_utf8_lossy(&o.stdout).contains("method-reader 0.12.0~rc1 x86_64"));
        let o = Command::new("rpm").arg("-qpl").arg(&rpm).output().unwrap();
        assert!(String::from_utf8_lossy(&o.stdout).contains("/usr/bin/method-reader-app"));
        // rpm itself reads the Requires: the run-time libraries, by soname.
        let o = Command::new("rpm")
            .arg("-qp")
            .arg("--requires")
            .arg(&rpm)
            .output()
            .unwrap();
        let requires = String::from_utf8_lossy(&o.stdout);
        for (soname, _) in linux::LOADED_AT_RUN_TIME {
            assert!(
                requires
                    .lines()
                    .any(|l| l.trim() == format!("{soname}()(64bit)")),
                "{soname}: {requires}"
            );
        }
    } else {
        eprintln!("skipped the rpm half: not installed");
    }
}
