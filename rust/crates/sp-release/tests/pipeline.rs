//! The release's update half, end to end through the `sp-release` binary
//! with a throwaway key: lay out the app, archive it, sign it, write
//! latest.json, verify the folder, then install the archive over a
//! Tauri-shaped bundle the way the plugin would. No network, no real key.

use std::path::{Path, PathBuf};
use std::process::Command;

fn sp_release(args: &[&str], env: &[(&str, &str)]) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_sp-release"));
    c.args(args).env_remove("TAURI_SIGNING_PRIVATE_KEY");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "sp-release {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn file(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn a_release_s_update_files_install_over_a_tauri_bundle() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    let window = file(t, "method-reader-app", "#!/bin/sh\necho rust\n");
    let icon = file(t, "icon.icns", "icns");
    let license = file(t, "LICENSE", "AGPL");
    let notices = file(t, "THIRD-PARTY-NOTICES.md", "notices");
    let s = |p: &Path| p.to_string_lossy().into_owned();

    sp_release(
        &[
            "mac-app",
            "--version",
            "0.12.0",
            "--window",
            &s(&window),
            "--icon",
            &s(&icon),
            "--resource",
            &s(&license),
            "--resource",
            &s(&notices),
            "--out",
            &s(&t.join("build")),
        ],
        &[],
    );
    let arrivals = t.join("arrivals");
    let archive = arrivals.join("Method-Reader-macOS.app.tar.gz");
    sp_release(
        &[
            "update-archive",
            "--app",
            &s(&t.join("build/Method Reader.app")),
            "--out",
            &s(&archive),
        ],
        &[("SOURCE_DATE_EPOCH", "1767225600")],
    );
    let key = t.join("keys/throwaway.key");
    sp_release(&["keygen", "--out", &s(&key)], &[]);
    let key_text = std::fs::read_to_string(&key).unwrap();
    let pubkey = std::fs::read_to_string(t.join("keys/throwaway.key.pub")).unwrap();

    // Without the key, signing refuses rather than publishing nothing.
    let refused = Command::new(env!("CARGO_BIN_EXE_sp-release"))
        .args(["sign-update", "--file", &s(&archive), "--version", "0.12.0"])
        .env_remove("TAURI_SIGNING_PRIVATE_KEY")
        .output()
        .unwrap();
    assert!(!refused.status.success());

    sp_release(
        &["sign-update", "--file", &s(&archive), "--version", "0.12.0"],
        &[
            ("TAURI_SIGNING_PRIVATE_KEY", &key_text),
            ("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""),
        ],
    );
    let manifest_path = arrivals.join("latest.json");
    sp_release(
        &[
            "manifest",
            "--dir",
            &s(&arrivals),
            "--version",
            "v0.12.0",
            "--out",
            &s(&manifest_path),
        ],
        &[],
    );
    let said = sp_release(
        &[
            "verify-update",
            "--dir",
            &s(&arrivals),
            "--manifest",
            &s(&manifest_path),
            "--pubkey",
            &pubkey,
        ],
        &[],
    );
    assert!(said.contains("darwin-aarch64"), "{said}");

    // A second archive of the same bundle is byte-identical.
    let again = t.join("again.tar.gz");
    sp_release(
        &[
            "update-archive",
            "--app",
            &s(&t.join("build/Method Reader.app")),
            "--out",
            &s(&again),
        ],
        &[("SOURCE_DATE_EPOCH", "1767225600")],
    );
    assert_eq!(
        std::fs::read(&archive).unwrap(),
        std::fs::read(&again).unwrap()
    );

    // What a Tauri window or a 0.12.x Screepub does with it: verify (the
    // Tauri rule: version optional), then swap its own bundle, in place,
    // whatever the archive's top folder is called.
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let release = sp_update::Release::from_value(manifest).unwrap();
    let entry = release.platform("darwin-aarch64").unwrap();
    let bytes = std::fs::read(&archive).unwrap();
    sp_update::verify::verify_signature(&bytes, &entry.signature, &pubkey, "0.12.0", false)
        .unwrap();

    let apps = t.join("Applications");
    let old = apps.join("Screepub.app/Contents/MacOS");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("screepub-desktop"), "tauri").unwrap();
    std::fs::write(old.join("screepub-engine"), "bun").unwrap();
    let installed = sp_update::install::install_app(&bytes, &apps.join("Screepub.app"), &|_, _| {
        panic!("no admin")
    })
    .unwrap();
    assert_eq!(installed.leftover, None);
    // A 0.12.x install named Screepub.app: the Rust updater installs the
    // new version beside it as Method Reader.app and removes the old one.
    assert_eq!(installed.bundle, apps.join("Method Reader.app"));
    assert!(!apps.join("Screepub.app").exists());
    let exe = sp_update::install::bundle_executable(&installed.bundle).unwrap();
    assert!(exe.ends_with("Contents/MacOS/method-reader-app"), "{exe:?}");
    assert_eq!(
        std::fs::read_to_string(exe).unwrap(),
        "#!/bin/sh\necho rust\n"
    );
    assert!(
        !installed
            .bundle
            .join("Contents/MacOS/screepub-engine")
            .exists()
    );
    assert_eq!(
        sp_update::install::bundle_identifier(&installed.bundle).as_deref(),
        Some(sp_update::IDENTIFIER)
    );
    assert_eq!(
        std::fs::read_to_string(installed.bundle.join("Contents/Resources/LICENSE")).unwrap(),
        "AGPL"
    );
}

#[test]
fn a_tag_the_workspace_does_not_name_is_refused() {
    let out = Command::new(env!("CARGO_BIN_EXE_sp-release"))
        .args(["version-check", "--version", "99.0.0", "--cargo-toml"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("but the tag says 99.0.0"));
    let ok = Command::new(env!("CARGO_BIN_EXE_sp-release"))
        .args([
            "version-check",
            "--version",
            env!("CARGO_PKG_VERSION"),
            "--cargo-toml",
        ])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
        .output()
        .unwrap();
    assert!(ok.status.success());
}
