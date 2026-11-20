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
    // when no password is given, so the workspace pinned =0.7.3; 0.10.0
    // opens it again, and the workspace pins that.
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
        method_update::verify::verify_signature(data, &sig, &pk, "0.12.0", required).unwrap();
    }
    assert!(sign::load_secret_key(TAURI_TEST_KEY, "not the password").is_err());
}

/// What minisign 0.7.3, the version every release up to 1.2.1 signed with,
/// made from tauri-cli's test key above: the update signature of
/// `an update archive` and the plain signature of a one-line SHA256SUMS.
/// Frozen here when the workspace moved to minisign 0.10, so a later
/// version that signs or reads differently fails a test, not an update.
const OLD_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEZFOUJFNDg1NTU4NUZDQUQKUldTdC9JVlZoZVNiL2tVVG1hSFRETjRIZXE0a0F6d3dSY2ViYzdrSFh2MjBGWm1jM0NoWVFqM1YK";
const OLD_UPDATE_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTdC9JVlZoZVNiL2t2TFhQQ3VYazJwMUdPSkhoQi9pNFl3Yk0ray90NnNLMytvakFKSjhxSk5FSmtGdkNpOGQza0ZHTjZWcWU5TUgyZkJ3ZkV5WUoxUklYVGpDNGEwenc4PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzU4NDcwMDAwCWZpbGU6TWV0aG9kLVJlYWRlci1tYWNPUy5hcHAudGFyLmd6CXZlcnNpb246MC4xMi4wCmlURnNzQi9nMmlwM2JZQ2EwNUlDN2lUSTI0RFBLdk1rSksvVVZKcDhZVm9sekV2bTlSdkMxY0hKWE4ybVR2elNuWDNyNW02eUtpNlNPdFVlTlVOdkNBPT0K";
const OLD_PLAIN_SIG_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRoZSBNZXRob2QgUmVhZGVyIHVwZGF0ZSBrZXkKUlVTdC9JVlZoZVNiL2l2ODZrSDlRczQ2bVJsQ1hXc2FyaE00MlgwTlRFekY1UTIxbndZaWpLamhiVUtsL08va1hwY1lpbEdRa1RyVVVudExvakxzUzhHTkh4WS9aZ3lsc1FNPQp0cnVzdGVkIGNvbW1lbnQ6IGZpbGU6U0hBMjU2U1VNUwpESjBjdFR4YTd2RVk3b1ZUU05Tc056UDgwNWxCVWh0VFMxM3dzM01PS1djVTQxMm1ZTk50Ym9LSmZzdklFU1VzZ0dUSWRkMXg2MkV2dDZXRkNUeG5DZz09Cg==";

fn old_plain_sig() -> String {
    String::from_utf8(STANDARD.decode(OLD_PLAIN_SIG_B64).unwrap()).unwrap()
}

#[test]
fn what_minisign_0_7_3_signed_still_verifies_now() {
    // The update signature, under the verifier every installed copy runs.
    for required in [false, true] {
        method_update::verify::verify_signature(
            b"an update archive",
            OLD_UPDATE_SIG,
            OLD_PUBKEY,
            "0.12.0",
            required,
        )
        .unwrap();
    }
    // The plain signature, under this workspace's minisign.
    let data = b"abc  Method-Reader-macOS.dmg\n";
    assert_eq!(
        sign::verify_plain(OLD_PUBKEY, data, &old_plain_sig()).unwrap(),
        "file:SHA256SUMS"
    );
    assert!(sign::verify_plain(OLD_PUBKEY, b"changed", &old_plain_sig()).is_err());
    // And the real thing: 1.2.1's published SHA256SUMS and its signature,
    // made by the release key under 0.7.3, against the key the app ships.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1.2.1");
    let sums = std::fs::read(dir.join("SHA256SUMS")).unwrap();
    let sig = std::fs::read_to_string(dir.join("SHA256SUMS.minisig")).unwrap();
    assert_eq!(
        sign::verify_plain(sp_update_config::PUBKEY, &sums, &sig).unwrap(),
        "file:SHA256SUMS"
    );
}

#[test]
fn minisign_now_signs_in_the_shape_0_7_3_did_and_installed_copies_take_it() {
    // 0.10 mixes randomness into each Ed25519 signature, so the bytes differ
    // from 0.7.3's; everything a reader looks at must not: the algorithm
    // (prehashed, "ED"), the key id, the length and both comments. Installed
    // copies have checked with method-update at 25c80f8 and minisign-verify
    // 0.2.5 since 1.0.0, the verifier this test runs.
    fn parts(sig_text: &str) -> (Vec<u8>, String, String) {
        let lines: Vec<&str> = sig_text.lines().collect();
        assert_eq!(lines.len(), 4, "{sig_text}");
        let raw = STANDARD.decode(lines[1]).unwrap();
        assert_eq!(raw.len(), 74);
        (
            raw[..10].to_vec(),
            lines[0].to_string(),
            lines[2].to_string(),
        )
    }
    let decoded = |b64: &str| String::from_utf8(STANDARD.decode(b64).unwrap()).unwrap();
    let sk = sign::load_secret_key(TAURI_TEST_KEY, "").unwrap();
    let pk = STANDARD.encode(
        minisign::PublicKey::from_secret_key(&sk)
            .unwrap()
            .to_box()
            .unwrap()
            .to_string(),
    );
    assert_eq!(pk, OLD_PUBKEY);
    let upd = sign::sign(
        &sk,
        b"an update archive",
        "Method-Reader-macOS.app.tar.gz",
        "0.12.0",
        1_758_470_000,
    )
    .unwrap();
    assert_eq!(parts(&decoded(&upd)), parts(&decoded(OLD_UPDATE_SIG)));
    assert_eq!(&parts(&decoded(&upd)).0[..2], b"ED");
    let data = b"abc  Method-Reader-macOS.dmg\n";
    let plain = sign::sign_plain(&sk, &pk, data, "SHA256SUMS").unwrap();
    assert_eq!(parts(&plain), parts(&old_plain_sig()));
    assert_eq!(
        sign::verify_plain(&pk, data, &plain).unwrap(),
        "file:SHA256SUMS"
    );
    for required in [false, true] {
        method_update::verify::verify_signature(
            b"an update archive",
            &upd,
            &pk,
            "0.12.0",
            required,
        )
        .unwrap();
    }
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
            identifier: sp_update_config::IDENTIFIER,
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
    method_update::install::unpack_app_archive(&bytes, &dest).unwrap();
    assert_eq!(
        method_update::install::bundle_identifier(&dest).as_deref(),
        Some(sp_update_config::IDENTIFIER)
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
