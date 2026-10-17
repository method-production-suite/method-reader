//! Port of tests/build-update-manifest.test.ts (20 checks): latest.json,
//! built from what ARRIVED.

mod packaging_common;

use std::fs;
use std::path::PathBuf;

use packaging_common::*;
use xtask::packaging::manifest::{
    MANIFEST_NAME, RELEASE_REPO, SignedArtifact, asset_url, collect_signed_artifacts, iso8601,
    manifest_endpoint, parse_manifest_args, render_manifest,
};

const TAR: &str = "Method-Reader-macOS.app.tar.gz";
/// 2026-09-21T18:00:00Z
const WHEN: i64 = 1_790_013_600_000;

struct Arrivals {
    sig: Option<String>,
    tar: bool,
    extra: Vec<(String, String)>,
}

impl Default for Arrivals {
    fn default() -> Self {
        Arrivals {
            sig: Some(fake_signature_box("Screepub.app.tar.gz", FAKE_KEY_ID)),
            tar: true,
            extra: Vec::new(),
        }
    }
}

/// A folder shaped like app-upload's `arrivals`.
fn arrivals(s: &Scratch, tag: &str, a: Arrivals) -> PathBuf {
    let dir = s.join(tag);
    fs::create_dir_all(&dir).unwrap();
    for n in [
        "Screepub_0.6.0_amd64.deb",
        "Screepub-0.6.0-1.x86_64.rpm",
        "Method-Reader-macOS.dmg",
        "Screepub-0.6.0-setup.exe",
        "SHA256SUMS-app",
    ] {
        fs::write(dir.join(n), format!("contents of {n}")).unwrap();
    }
    if a.tar {
        fs::write(dir.join(TAR), "gzip bytes").unwrap();
    }
    if let Some(sig) = a.sig {
        fs::write(dir.join(format!("{TAR}.sig")), sig).unwrap();
    }
    for (n, c) in a.extra {
        fs::write(dir.join(n), c).unwrap();
    }
    dir
}

fn one() -> SignedArtifact {
    SignedArtifact {
        name: TAR.into(),
        signature: fake_signature_box("Screepub.app.tar.gz", FAKE_KEY_ID),
        platform_keys: vec!["darwin-aarch64".into()],
    }
}

// ---- the addresses ----

#[test]
fn the_endpoint_is_the_one_url_the_app_is_configured_with() {
    // The app's one endpoint lives in sp-update (it was tauri.conf.json's
    // plugins.updater.endpoints until the Rust cutover).
    assert_eq!(manifest_endpoint(RELEASE_REPO), sp_update::ENDPOINT);
    assert_eq!(
        manifest_endpoint(RELEASE_REPO),
        "https://github.com/method-production-suite/method-reader/releases/latest/download/latest.json"
    );
    assert_eq!(RELEASE_REPO, "method-production-suite/method-reader");
    assert_eq!(MANIFEST_NAME, "latest.json");
    // What every 0.12.x copy asks, which GitHub redirects once the
    // repository is moved: the same file name under the old repository.
    assert_eq!(
        manifest_endpoint(sp_release::names::LEGACY_REPO),
        sp_update::LEGACY_ENDPOINT
    );
}

#[test]
fn an_asset_url_names_the_tag_and_the_published_filename_over_tls() {
    assert_eq!(
        asset_url("method-production-suite/method-reader", "0.6.1", TAR),
        format!(
            "https://github.com/method-production-suite/method-reader/releases/download/v0.6.1/{TAR}"
        )
    );
}

// ---- collecting what arrived ----

#[test]
fn the_signed_archive_becomes_one_artifact_for_apple_silicon_with_its_signature_verbatim() {
    let s = Scratch::new("manifest");
    let found = collect_signed_artifacts(&arrivals(&s, "ok", Arrivals::default())).unwrap();
    assert_eq!(found, vec![one()]);
}

#[test]
fn an_intel_archive_arriving_is_an_error_not_a_darwin_x86_64_row() {
    let s = Scratch::new("manifest");
    let dir = arrivals(
        &s,
        "intel",
        Arrivals {
            extra: vec![
                ("Screepub-macOS-x64.app.tar.gz".into(), "gzip bytes".into()),
                (
                    "Screepub-macOS-x64.app.tar.gz.sig".into(),
                    fake_signature_box("Screepub-macOS-x64.app.tar.gz", FAKE_KEY_ID),
                ),
            ],
            ..Arrivals::default()
        },
    );
    err_contains(
        collect_signed_artifacts(&dir),
        "Screepub-macOS-x64.app.tar.gz",
    );
}

#[test]
fn installers_and_checksums_are_not_artifacts_because_nothing_signed_them() {
    let s = Scratch::new("manifest");
    let dir = arrivals(
        &s,
        "unsigned",
        Arrivals {
            sig: None,
            tar: false,
            extra: vec![],
        },
    );
    assert_eq!(collect_signed_artifacts(&dir).unwrap(), vec![]);
}

#[test]
fn a_signature_with_no_artifact_beside_it_is_an_error_that_names_both() {
    let s = Scratch::new("manifest");
    let dir = arrivals(
        &s,
        "orphan",
        Arrivals {
            tar: false,
            ..Arrivals::default()
        },
    );
    let e = err_contains(collect_signed_artifacts(&dir), &format!("{TAR}.sig"));
    assert!(e.contains("no such artifact"), "{e}");
}

#[test]
fn a_signature_for_an_artifact_no_row_knows_is_an_error_not_a_skip() {
    let s = Scratch::new("manifest");
    let dir = arrivals(
        &s,
        "unknown",
        Arrivals {
            extra: vec![(
                "Screepub_0.6.0_amd64.deb.sig".into(),
                fake_signature_box("Screepub_0.6.0_amd64.deb", FAKE_KEY_ID),
            )],
            ..Arrivals::default()
        },
    );
    let e = err_contains(collect_signed_artifacts(&dir), "Screepub_0.6.0_amd64.deb");
    assert!(e.contains("UPDATER_KINDS"), "{e}");
}

#[test]
fn a_signature_that_is_not_a_minisign_box_is_refused_naming_the_file() {
    let s = Scratch::new("manifest");
    let dir = arrivals(
        &s,
        "badsig",
        Arrivals {
            sig: Some("<html>Not Found</html>".into()),
            ..Arrivals::default()
        },
    );
    err_contains(collect_signed_artifacts(&dir), &format!("{TAR}.sig"));
}

#[test]
fn a_missing_directory_is_a_clear_message() {
    let s = Scratch::new("manifest");
    err_contains(collect_signed_artifacts(&s.join("nowhere")), "nowhere");
}

// ---- rendering the manifest ----

#[test]
fn it_is_the_shape_the_plugin_reads_version_notes_pub_date_platforms() {
    let m = render_manifest("0.6.1", &[one()], RELEASE_REPO, WHEN).unwrap();
    assert_eq!(m["version"], "0.6.1");
    assert_eq!(m["pub_date"], "2026-09-21T18:00:00.000Z");
    let keys: Vec<&String> = m["platforms"].as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["darwin-aarch64"]);
    assert_eq!(
        m["platforms"]["darwin-aarch64"],
        serde_json::json!({
            "url": format!("https://github.com/method-production-suite/method-reader/releases/download/v0.6.1/{TAR}"),
            "signature": one().signature,
        })
    );
    // The key order JSON.stringify wrote.
    let order: Vec<&String> = m.as_object().unwrap().keys().collect();
    assert_eq!(order, vec!["version", "notes", "pub_date", "platforms"]);
}

#[test]
fn the_notes_point_a_person_at_the_release_page_for_that_version() {
    let m = render_manifest("0.6.1", &[one()], RELEASE_REPO, WHEN).unwrap();
    let notes = m["notes"].as_str().unwrap();
    assert!(
        notes.contains(
            "https://github.com/method-production-suite/method-reader/releases/tag/v0.6.1"
        )
    );
    assert!(notes.contains("0.6.1"));
}

#[test]
fn the_version_is_the_plain_tag_version_whatever_it_was_handed() {
    assert_eq!(
        render_manifest("v0.6.1", &[one()], RELEASE_REPO, WHEN).unwrap()["version"],
        "0.6.1"
    );
    err_contains(
        render_manifest("main", &[one()], RELEASE_REPO, WHEN),
        "MAJOR.MINOR.PATCH",
    );
}

#[test]
fn a_prerelease_version_is_refused_releases_latest_never_points_at_one() {
    err_contains(
        render_manifest("0.7.0-rc1", &[one()], RELEASE_REPO, WHEN),
        "prerelease",
    );
}

#[test]
fn no_artifacts_is_an_error_an_empty_manifest_is_a_stale_forever_endpoint() {
    err_contains(
        render_manifest("0.6.1", &[], RELEASE_REPO, WHEN),
        "no signed",
    );
}

#[test]
fn a_repeated_key_is_an_error() {
    let other = SignedArtifact {
        name: "Screepub-other.app.tar.gz".into(),
        signature: "a".into(),
        platform_keys: vec!["darwin-aarch64".into()],
    };
    err_contains(
        render_manifest("0.6.1", &[one(), other], RELEASE_REPO, WHEN),
        "darwin-aarch64",
    );
}

#[test]
fn the_repository_can_be_overridden_for_a_fork_or_a_dry_run() {
    let m = render_manifest("0.6.1", &[one()], "someone/fork", WHEN).unwrap();
    assert!(
        m["platforms"]["darwin-aarch64"]["url"]
            .as_str()
            .unwrap()
            .contains("someone/fork")
    );
    assert!(m["notes"].as_str().unwrap().contains("someone/fork"));
}

#[test]
fn pub_date_is_to_iso_string_across_the_calendar() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso8601(951_782_400_123), "2000-02-29T00:00:00.123Z");
    assert_eq!(iso8601(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
}

// ---- the arguments ----

#[test]
fn dir_version_and_out_are_required() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    err_contains(
        parse_manifest_args(&a(&["--version", "0.6.1", "--out", "x"])),
        "--dir",
    );
    err_contains(
        parse_manifest_args(&a(&["--dir", "d", "--out", "x"])),
        "--version",
    );
    err_contains(
        parse_manifest_args(&a(&["--dir", "d", "--version", "0.6.1"])),
        "--out",
    );
}

#[test]
fn repo_defaults_to_this_repository() {
    let args: Vec<String> = ["--dir", "d", "--version", "v0.6.1", "--out", "x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let parsed = parse_manifest_args(&args).unwrap();
    assert_eq!(parsed.version, "v0.6.1");
    assert_eq!(parsed.repo, RELEASE_REPO);
}

// ---- end to end, through the real tool ----

#[test]
fn it_writes_latest_json_describing_the_signed_archive_that_arrived() {
    let s = Scratch::new("manifest-e2e");
    let dir = arrivals(&s, "e2e", Arrivals::default());
    let out = dir.join(MANIFEST_NAME);
    let (code, stdout, stderr) = xtask(&[
        "build-update-manifest",
        "--dir",
        dir.to_str().unwrap(),
        "--version",
        "v0.6.1",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let text = fs::read_to_string(&out).unwrap();
    assert!(text.ends_with("}\n"));
    let m: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(m["version"], "0.6.1");
    let keys: Vec<&String> = m["platforms"].as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["darwin-aarch64"]);
    assert!(m["platforms"].get("darwin-x86_64").is_none());
    assert_eq!(
        m["platforms"]["darwin-aarch64"]["signature"],
        fake_signature_box("Screepub.app.tar.gz", FAKE_KEY_ID)
    );
    assert_eq!(
        m["platforms"]["darwin-aarch64"]["url"],
        format!(
            "https://github.com/method-production-suite/method-reader/releases/download/v0.6.1/{TAR}"
        )
    );
    assert!(stdout.contains("darwin-aarch64"));
    assert!(stdout.contains(TAR));
}

#[test]
fn nothing_signed_is_a_non_zero_exit_and_no_file() {
    let s = Scratch::new("manifest-e2e");
    let dir = arrivals(
        &s,
        "e2e-empty",
        Arrivals {
            sig: None,
            tar: false,
            extra: vec![],
        },
    );
    let out = dir.join(MANIFEST_NAME);
    let (code, _, stderr) = xtask(&[
        "build-update-manifest",
        "--dir",
        dir.to_str().unwrap(),
        "--version",
        "0.6.1",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert!(stderr.to_lowercase().contains("no signed"), "{stderr}");
    assert!(!out.exists());
}
