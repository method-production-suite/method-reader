//! The whole update, against a local fake of the release endpoint and a
//! throwaway minisign key: check, download, verify, swap, restart command.
//! The manifest and the archive have the shape release.yml publishes and
//! the Tauri windows read.

mod common;

use common::{Answer, Server, TestKey, archive_bundle, make_bundle, tar_gz};
use semver::Version;
use sp_update::install::{InstallError, install_app};
use sp_update::target::{Installer, TargetSet};
use sp_update::{Config, UpdateError, check, download};

const ARCHIVE: &str = "/download/v0.13.0/Method-Reader-macOS.app.tar.gz";

fn config(server: &Server, key: &TestKey) -> Config {
    Config {
        endpoints: vec![server.url("/latest/download/latest.json")],
        pubkey: key.pubkey_b64(),
        allow_insecure: true,
        require_signed_version: true,
        timeout: Some(std::time::Duration::from_secs(10)),
        installer: Some(Installer::App),
        targets: Some(TargetSet::new("darwin", "aarch64", Some(Installer::App))),
    }
}

fn manifest(server: &Server, version: &str, signature: &str) -> String {
    serde_json::json!({
        "version": version,
        "notes": format!("Method Reader {version}. Release notes: https://example.invalid/v{version}"),
        "pub_date": "2026-10-12T09:00:00.000Z",
        "platforms": {
            "darwin-aarch64": { "url": server.url(ARCHIVE), "signature": signature }
        }
    })
    .to_string()
}

fn no_admin(_: &std::path::Path, _: &std::path::Path) -> Result<(), InstallError> {
    panic!("no administrator should be needed in a temp folder")
}

struct World {
    server: Server,
    key: TestKey,
    apps: tempfile::TempDir,
    archive: Vec<u8>,
}

/// A Tauri-era bundle installed under the old name, a Method Reader
/// bundle published as 0.13.0.
fn world() -> World {
    let server = Server::start();
    let key = TestKey::new();
    let apps = tempfile::tempdir().unwrap();
    make_bundle(
        &apps.path().join("Screepub.app"),
        "screepub-desktop",
        "0.11.1",
    );
    std::fs::write(
        apps.path()
            .join("Screepub.app/Contents/MacOS/screepub-engine"),
        "old sidecar",
    )
    .unwrap();
    let build = tempfile::tempdir().unwrap();
    make_bundle(
        &build.path().join("Built.app"),
        "method-reader-app",
        "0.13.0",
    );
    let archive = archive_bundle(&build.path().join("Built.app"));
    World {
        server,
        key,
        apps,
        archive,
    }
}

impl World {
    fn publish(&self, announced: &str, signed_for: Option<&str>, bytes: &[u8]) {
        let sig = self
            .key
            .sign(bytes, "Method-Reader-macOS.app.tar.gz", signed_for);
        self.server.route(
            "/latest/download/latest.json",
            Answer::json(&manifest(&self.server, announced, &sig)),
        );
        self.server
            .route(ARCHIVE, Answer::bytes(self.archive.clone()));
    }

    /// The installed bundle, under its 0.12.x name.
    fn bundle(&self) -> std::path::PathBuf {
        self.apps.path().join("Screepub.app")
    }

    /// Where the update goes: the one-time rename.
    fn renamed(&self) -> std::path::PathBuf {
        self.apps.path().join("Method Reader.app")
    }

    /// The executables of whichever bundle is there.
    fn exe_names(&self) -> Vec<String> {
        let bundle = if self.bundle().exists() {
            self.bundle()
        } else {
            self.renamed()
        };
        let mut v: Vec<String> = std::fs::read_dir(bundle.join("Contents/MacOS"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

#[test]
fn a_tauri_window_s_bundle_becomes_method_reader_under_its_new_name() {
    let w = world();
    w.publish("0.13.0", Some("0.13.0"), &w.archive.clone());
    let c = config(&w.server, &w.key);

    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    assert_eq!(update.version, "0.13.0");
    assert_eq!(update.current_version, "0.11.1");
    assert_eq!(update.target, "darwin-aarch64");
    assert!(
        update
            .body
            .as_deref()
            .unwrap()
            .starts_with("Method Reader 0.13.0.")
    );

    let mut received = 0usize;
    let mut finished = false;
    let bytes = download(
        &c,
        &update,
        |n, total| {
            received += n;
            assert_eq!(total, Some(w.archive.len() as u64));
        },
        || finished = true,
    )
    .unwrap();
    assert!(finished);
    assert_eq!(received, w.archive.len());

    let installed = install_app(&bytes, &w.bundle(), &no_admin).unwrap();
    assert_eq!(installed.bundle, w.renamed());
    assert_eq!(installed.leftover, None);
    // New name, new contents: the old bundle, its executable and its
    // sidecar are gone.
    assert!(!w.bundle().exists());
    assert_eq!(w.exe_names(), ["method-reader-app"]);
    // The bundle carries the new build's identifier, not the old one's:
    // the swap takes the archive's Info.plist whole.
    assert_eq!(
        sp_update::install::bundle_identifier(&w.renamed()).as_deref(),
        Some(sp_update::IDENTIFIER)
    );
    // Nothing left beside it.
    let beside: Vec<_> = std::fs::read_dir(w.apps.path()).unwrap().collect();
    assert_eq!(beside.len(), 1);
    // The restart reads the NEW Info.plist, as tauri's restart_macos_app
    // does, at the path the install reported.
    let cmd = sp_update::restart::relaunch_command(&installed.bundle, &[]).unwrap();
    assert_eq!(
        std::path::Path::new(cmd.get_program()),
        w.renamed().join("Contents/MacOS/method-reader-app")
    );

    let seen = w.server.seen.lock().unwrap().clone();
    assert_eq!(
        seen[0],
        (
            "/latest/download/latest.json".into(),
            "application/json".into()
        )
    );
    assert_eq!(seen[1], (ARCHIVE.into(), "application/octet-stream".into()));
}

#[test]
fn a_tampered_archive_is_refused_and_nothing_moves() {
    let w = world();
    let mut evil = w.archive.clone();
    let sig = w.key.sign(&w.archive, "x", Some("0.13.0"));
    let last = evil.len() - 9;
    evil[last] ^= 1;
    w.server.route(
        "/latest/download/latest.json",
        Answer::json(&manifest(&w.server, "0.13.0", &sig)),
    );
    w.server.route(ARCHIVE, Answer::bytes(evil));
    let c = config(&w.server, &w.key);
    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    let err = download(&c, &update, |_, _| {}, || {}).unwrap_err();
    assert!(matches!(err, UpdateError::Verify(_)), "{err}");
    assert_eq!(w.exe_names(), ["screepub-desktop", "screepub-engine"]);
}

#[test]
fn another_key_is_refused() {
    let w = world();
    w.publish("0.13.0", Some("0.13.0"), &w.archive.clone());
    let mut c = config(&w.server, &w.key);
    c.pubkey = TestKey::new().pubkey_b64();
    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    assert!(download(&c, &update, |_, _| {}, || {}).is_err());
}

#[test]
fn a_manifest_announcing_another_version_than_the_signature_is_refused() {
    let w = world();
    w.publish("0.13.1", Some("0.13.0"), &w.archive.clone());
    let c = config(&w.server, &w.key);
    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    let err = download(&c, &update, |_, _| {}, || {}).unwrap_err();
    assert!(err.to_string().starts_with(
        "The update was signed for version 0.13.0 but the update endpoint announced version 0.13.1."
    ));
}

#[test]
fn a_tauri_cli_signature_without_a_version_passes_only_where_the_window_allowed_it() {
    let w = world();
    w.publish("0.13.0", None, &w.archive.clone());
    let mut c = config(&w.server, &w.key);
    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    // The Rust app requires the signed version...
    assert!(download(&c, &update, |_, _| {}, || {}).is_err());
    // ...every Tauri window (requireSignedVersion unset) does not.
    c.require_signed_version = false;
    assert!(download(&c, &update, |_, _| {}, || {}).is_ok());
}

#[test]
fn nothing_newer_no_content_and_a_missing_manifest() {
    let w = world();
    w.publish("0.11.1", Some("0.11.1"), &w.archive.clone());
    let c = config(&w.server, &w.key);
    assert_eq!(check(&c, &Version::new(0, 11, 1)).unwrap(), None);
    // A dev build past the tag: semver says 0.11.1 > 0.11.1-3-gabc, which is
    // why sp-app applies its own comparator on top, as the window did.
    assert!(
        check(&c, &Version::parse("0.11.1-3-gabc").unwrap())
            .unwrap()
            .is_some()
    );

    w.server
        .route("/latest/download/latest.json", Answer::status(204));
    assert_eq!(check(&c, &Version::new(0, 11, 1)).unwrap(), None);

    w.server
        .route("/latest/download/latest.json", Answer::status(404));
    assert_eq!(
        check(&c, &Version::new(0, 11, 1)).unwrap_err().to_string(),
        "Could not fetch a valid release JSON from the remote"
    );

    w.server
        .route("/latest/download/latest.json", Answer::json("{\"oops\":1}"));
    assert!(matches!(
        check(&c, &Version::new(0, 11, 1)),
        Err(UpdateError::Manifest(_))
    ));
}

#[test]
fn an_intel_mac_hears_why_even_when_nothing_is_newer() {
    let w = world();
    w.publish("0.11.1", Some("0.11.1"), &w.archive.clone());
    let mut c = config(&w.server, &w.key);
    c.targets = Some(TargetSet::new("darwin", "x86_64", Some(Installer::App)));
    let err = check(&c, &Version::new(0, 9, 0)).unwrap_err().to_string();
    assert!(err.contains("darwin-x86_64"));
    assert!(err.contains("`platforms` object"));
}

#[test]
fn a_missing_archive_says_so_with_the_status() {
    let w = world();
    w.publish("0.13.0", Some("0.13.0"), &w.archive.clone());
    w.server.route(ARCHIVE, Answer::status(404));
    let c = config(&w.server, &w.key);
    let update = check(&c, &Version::new(0, 11, 1)).unwrap().unwrap();
    assert_eq!(
        download(&c, &update, |_, _| {}, || {})
            .unwrap_err()
            .to_string(),
        "`Download request failed with status: 404 Not Found`"
    );
}

#[test]
fn an_archive_that_is_not_a_bundle_never_moves_the_old_one() {
    let w = world();
    let no_plist = tar_gz(&[
        ("Method Reader.app/", b"", 0o755),
        ("Method Reader.app/Contents/MacOS/x", b"x", 0o755),
    ]);
    let err = install_app(&no_plist, &w.bundle(), &no_admin).unwrap_err();
    assert!(err.to_string().contains("Info.plist"), "{err}");
    assert_eq!(w.exe_names(), ["screepub-desktop", "screepub-engine"]);

    let escape = tar_gz(&[("Method Reader.app/../../evil", b"x", 0o644)]);
    let err = install_app(&escape, &w.bundle(), &no_admin).unwrap_err();
    assert!(err.to_string().contains("outside its bundle"), "{err}");
    assert!(!w.apps.path().parent().unwrap().join("evil").exists());
    assert_eq!(w.exe_names(), ["screepub-desktop", "screepub-engine"]);
    let beside: Vec<_> = std::fs::read_dir(w.apps.path()).unwrap().collect();
    assert_eq!(beside.len(), 1, "no staging folder left behind");
}

#[cfg(unix)]
#[test]
fn an_old_bundle_that_cannot_be_deleted_is_left_over_not_a_failure() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    let locked = w.bundle().join("Contents/Resources/locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("f"), "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

    let installed = install_app(&w.archive, &w.bundle(), &no_admin).unwrap();
    let leftover = installed.leftover.expect("the old bundle stays behind");
    assert_eq!(w.exe_names(), ["method-reader-app"]);
    std::fs::set_permissions(
        leftover.join("Contents/Resources/locked"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn the_executable_bit_survives_the_round_trip() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    let installed = install_app(&w.archive, &w.bundle(), &no_admin).unwrap();
    let mode = std::fs::metadata(installed.bundle.join("Contents/MacOS/method-reader-app"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
}
