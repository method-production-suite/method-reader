//! `cargo xtask check-latest`: ported from tests/check-latest.test.ts.
//!
//! The network is injected: every case is a table of urls and answers, so
//! the judgement meets the failures that matter without a release
//! existing. One more case runs the command itself against a fake GitHub
//! on 127.0.0.1.

mod common;

use std::collections::{BTreeMap, HashMap};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::{FakeServer, text, xtask};
use xtask::check_latest::http::{Bases, Fetched};
use xtask::check_latest::{
    Release, ReleaseAsset, Verdict, check_latest, judge_manifest, parse_public_key_box,
    trusted_pubkey,
};

const REPO: &str = "method-production-suite/method-reader";
const TAR: &str = "Method-Reader-macOS.app.tar.gz";
const INTEL_CLI: &str = "screepub-cli-macos-x64.tar.gz";
const FAKE_KEY_ID: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const OTHER_KEY_ID: [u8; 8] = [0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18];

/// The key id as tauri and minisign print it.
fn key_id_hex(id: [u8; 8]) -> String {
    format!("{:X}", u64::from_le_bytes(id))
}

/// A minisign signature box shaped as tauri-cli writes it (not a real
/// signature: only the container is under test).
fn fake_signature_box(file: &str, key: [u8; 8]) -> String {
    let mut sig = b"ED".to_vec();
    sig.extend_from_slice(&key);
    sig.extend_from_slice(&[7u8; 64]);
    let boxed = format!(
        "untrusted comment: signature from tauri secret key\n{}\ntrusted comment: timestamp:1758470000\tfile:{file}\n{}\n",
        STANDARD.encode(&sig),
        STANDARD.encode([9u8; 64])
    );
    STANDARD.encode(boxed)
}

fn fake_public_key_box(key: [u8; 8]) -> String {
    let mut k = b"Ed".to_vec();
    k.extend_from_slice(&key);
    k.extend_from_slice(&[3u8; 32]);
    let boxed = format!(
        "untrusted comment: minisign public key: {}\n{}\n",
        key_id_hex(key),
        STANDARD.encode(&k)
    );
    STANDARD.encode(boxed)
}

fn url(v: &str, name: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{v}/{name}")
}

fn sig() -> String {
    fake_signature_box("Screepub.app.tar.gz", FAKE_KEY_ID)
}

fn pubkey() -> String {
    fake_public_key_box(FAKE_KEY_ID)
}

const DEFAULT_ASSETS: &[&str] = &[
    TAR,
    "Method-Reader-macOS.app.tar.gz.sig",
    "latest.json",
    "Method-Reader-macOS.dmg",
];

fn release_with(v: &str, assets: &[&str]) -> Release {
    Release {
        tag_name: format!("v{v}"),
        assets: assets
            .iter()
            .map(|n| ReleaseAsset {
                name: n.to_string(),
                url: url(v, n),
            })
            .collect(),
    }
}

fn release(v: &str) -> Release {
    release_with(v, DEFAULT_ASSETS)
}

/// A manifest whose darwin-aarch64 row points at the archive, with other
/// rows added or replaced.
fn manifest(v: &str, over: &[(&str, String, String)]) -> String {
    let mut platforms = serde_json::Map::new();
    platforms.insert(
        "darwin-aarch64".into(),
        serde_json::json!({ "url": url(v, TAR), "signature": sig() }),
    );
    for (k, u, s) in over {
        platforms.insert(
            k.to_string(),
            serde_json::json!({ "url": u, "signature": s }),
        );
    }
    serde_json::json!({
        "version": v,
        "notes": "x",
        "pub_date": "2026-09-21T18:00:00.000Z",
        "platforms": platforms,
    })
    .to_string()
}

fn sigs(text: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(format!("{TAR}.sig"), text.to_string())])
}

fn ok(version: &str, platforms: &[&str]) -> Verdict {
    Verdict::Ok {
        version: version.into(),
        platforms: platforms.iter().map(|p| p.to_string()).collect(),
    }
}

// ── judging a manifest against a release ────────────────────────────────

#[test]
fn a_manifest_naming_the_newest_release_its_assets_and_signatures_is_fine() {
    let v = judge_manifest(
        &manifest("0.6.1", &[]),
        &release("0.6.1"),
        &sigs(&sig()),
        &pubkey(),
    );
    assert_eq!(v, ok("0.6.1", &["darwin-aarch64"]));
}

#[test]
fn a_manifest_a_version_behind_says_both_versions() {
    let v = judge_manifest(
        &manifest("0.6.1", &[]),
        &release("0.6.2"),
        &sigs(&sig()),
        &pubkey(),
    );
    assert!(!v.is_ok());
    let t = v.text();
    assert!(t.contains("0.6.1") && t.contains("0.6.2"), "{t}");
}

#[test]
fn a_manifest_that_is_not_json_or_has_no_platforms_is_a_problem_not_a_crash() {
    assert!(!judge_manifest("<html>", &release("0.6.1"), &BTreeMap::new(), &pubkey()).is_ok());
    let empty = judge_manifest(
        r#"{"version":"0.6.1","platforms":{}}"#,
        &release("0.6.1"),
        &BTreeMap::new(),
        &pubkey(),
    );
    assert!(!empty.is_ok());
    assert!(empty.text().to_lowercase().contains("platform"));
}

#[test]
fn a_url_that_is_not_an_asset_of_that_release_is_a_problem_naming_the_url() {
    let wrong = url("0.6.0", TAR);
    let v = judge_manifest(
        &manifest("0.6.1", &[("darwin-aarch64", wrong.clone(), sig())]),
        &release("0.6.1"),
        &sigs(&sig()),
        &pubkey(),
    );
    assert!(!v.is_ok());
    assert!(v.text().contains(&wrong));
}

#[test]
fn a_signature_that_differs_from_the_published_sig_is_a_problem() {
    let other = fake_signature_box("Screepub.app.tar.gz", OTHER_KEY_ID);
    let v = judge_manifest(
        &manifest("0.6.1", &[]),
        &release("0.6.1"),
        &sigs(&other),
        &pubkey(),
    );
    assert!(!v.is_ok());
    assert!(v.text().to_lowercase().contains("signature"));
}

#[test]
fn a_release_with_no_sig_beside_the_archive_is_a_problem() {
    let v = judge_manifest(
        &manifest("0.6.1", &[]),
        &release_with("0.6.1", &[TAR, "latest.json"]),
        &BTreeMap::new(),
        &pubkey(),
    );
    assert!(!v.is_ok());
    assert!(v.text().contains(&format!("{TAR}.sig")));
}

#[test]
fn a_signature_by_a_key_the_app_does_not_trust_names_both_key_ids() {
    let foreign = fake_signature_box("Screepub.app.tar.gz", OTHER_KEY_ID);
    let v = judge_manifest(
        &manifest(
            "0.6.1",
            &[("darwin-aarch64", url("0.6.1", TAR), foreign.clone())],
        ),
        &release("0.6.1"),
        &sigs(&foreign),
        &pubkey(),
    );
    assert!(!v.is_ok());
    let t = v.text();
    assert!(t.contains(&key_id_hex(FAKE_KEY_ID)), "{t}");
    assert!(t.contains(&key_id_hex(OTHER_KEY_ID)), "{t}");
}

#[test]
fn an_empty_public_key_in_the_config_is_a_problem() {
    let v = judge_manifest(
        &manifest("0.6.1", &[]),
        &release("0.6.1"),
        &sigs(&sig()),
        "",
    );
    assert!(!v.is_ok());
    assert!(v.text().to_lowercase().contains("public key"));
}

#[test]
fn a_manifest_missing_darwin_aarch64_is_a_problem() {
    let intel_only = serde_json::json!({
        "version": "0.6.1",
        "platforms": { "darwin-x86_64": { "url": url("0.6.1", TAR), "signature": sig() } },
    })
    .to_string();
    let v = judge_manifest(&intel_only, &release("0.6.1"), &sigs(&sig()), &pubkey());
    assert!(!v.is_ok());
    assert!(v.text().contains("darwin-aarch64"));
}

#[test]
fn darwin_x86_64_is_no_longer_required_and_a_universal_era_manifest_still_passes() {
    assert!(
        judge_manifest(
            &manifest("0.6.1", &[]),
            &release("0.6.1"),
            &sigs(&sig()),
            &pubkey()
        )
        .is_ok()
    );
    let universal = manifest("0.6.1", &[("darwin-x86_64", url("0.6.1", TAR), sig())]);
    let mut assets = DEFAULT_ASSETS.to_vec();
    assets.push(INTEL_CLI);
    let era = release_with("0.6.1", &assets);
    assert_eq!(
        judge_manifest(&universal, &era, &sigs(&sig()), &pubkey()),
        ok("0.6.1", &["darwin-aarch64", "darwin-x86_64"])
    );
}

#[test]
fn after_intel_was_dropped_darwin_x86_64_on_the_apple_silicon_archive_is_a_problem() {
    let intel_to_arm = manifest("0.6.1", &[("darwin-x86_64", url("0.6.1", TAR), sig())]);
    let v = judge_manifest(&intel_to_arm, &release("0.6.1"), &sigs(&sig()), &pubkey());
    assert!(!v.is_ok());
    let t = v.text();
    assert!(t.contains("darwin-x86_64"));
    assert!(t.contains(&url("0.6.1", TAR)));
    assert!(t.contains("Apple silicon"));
}

#[test]
fn the_same_holds_for_darwin_x86_64_app_which_the_plugin_tries_first() {
    let app_key = manifest("0.6.1", &[("darwin-x86_64-app", url("0.6.1", TAR), sig())]);
    let v = judge_manifest(&app_key, &release("0.6.1"), &sigs(&sig()), &pubkey());
    assert!(!v.is_ok());
    assert!(v.text().contains("darwin-x86_64-app points at"));
    let via_app_arm = manifest(
        "0.6.1",
        &[
            ("darwin-aarch64-app", url("0.6.1", TAR), sig()),
            ("darwin-x86_64-app", url("0.6.1", TAR), sig()),
        ],
    );
    assert!(!judge_manifest(&via_app_arm, &release("0.6.1"), &sigs(&sig()), &pubkey()).is_ok());
    let mut assets = DEFAULT_ASSETS.to_vec();
    assets.push(INTEL_CLI);
    let era = release_with("0.6.1", &assets);
    assert!(judge_manifest(&app_key, &era, &sigs(&sig()), &pubkey()).is_ok());
}

#[test]
fn every_problem_is_reported_not_just_the_first() {
    let v = judge_manifest(
        &manifest("0.6.0", &[("darwin-aarch64", url("0.6.0", TAR), sig())]),
        &release_with("0.6.1", &[TAR, "latest.json"]),
        &BTreeMap::new(),
        "",
    );
    match v {
        Verdict::Problems(p) => assert!(p.len() >= 3, "{p:?}"),
        Verdict::Ok { .. } => panic!("expected problems"),
    }
}

// ── checking the live endpoint, with the network stubbed ────────────────

const LATEST_API: &str =
    "https://api.github.com/repos/method-production-suite/method-reader/releases/latest";
const ENDPOINT: &str =
    "https://github.com/method-production-suite/method-reader/releases/latest/download/latest.json";

fn api(v: &str) -> String {
    let assets: Vec<serde_json::Value> = DEFAULT_ASSETS
        .iter()
        .map(|n| serde_json::json!({ "name": n, "browser_download_url": url(v, n) }))
        .collect();
    serde_json::json!({ "tag_name": format!("v{v}"), "assets": assets }).to_string()
}

/// Runs the check over a url table; anything not in it is a 404, which is
/// what GitHub answers for an asset that does not exist.
fn check_over(answers: &[(&str, String)], version: Option<&str>) -> (Verdict, Vec<String>) {
    let table: HashMap<String, String> = answers
        .iter()
        .map(|(u, t)| (u.to_string(), t.clone()))
        .collect();
    let mut asked = Vec::new();
    let mut fetcher = |u: &str| {
        asked.push(u.to_string());
        Ok(match table.get(u) {
            Some(t) => Fetched {
                status: 200,
                text: t.clone(),
            },
            None => Fetched {
                status: 404,
                text: "Not Found".into(),
            },
        })
    };
    let v = check_latest(&mut fetcher, &Bases::default(), REPO, &pubkey(), version);
    (v, asked)
}

#[test]
fn it_reads_the_newest_release_from_the_api_and_the_manifest_from_the_apps_own_url() {
    let sig_url = url("0.6.1", &format!("{TAR}.sig"));
    let (v, asked) = check_over(
        &[
            (LATEST_API, api("0.6.1")),
            (ENDPOINT, manifest("0.6.1", &[])),
            (&sig_url, sig()),
        ],
        None,
    );
    assert_eq!(v, ok("0.6.1", &["darwin-aarch64"]));
    assert!(asked.iter().any(|a| a == ENDPOINT));
    assert!(asked.iter().any(|a| a == LATEST_API));
}

#[test]
fn a_release_with_no_latest_json_at_all_is_the_failure_this_exists_for() {
    let (v, _) = check_over(&[(LATEST_API, api("0.6.0"))], None);
    assert!(!v.is_ok());
    assert!(v.text().contains("latest.json"));
}

#[test]
fn version_asks_about_that_tag_instead_of_the_newest_release() {
    let tag_api =
        "https://api.github.com/repos/method-production-suite/method-reader/releases/tags/v0.6.1";
    let sig_url = url("0.6.1", &format!("{TAR}.sig"));
    let (v, asked) = check_over(
        &[
            (tag_api, api("0.6.1")),
            (ENDPOINT, manifest("0.6.1", &[])),
            (&sig_url, sig()),
        ],
        Some("v0.6.1"),
    );
    assert!(v.is_ok(), "{}", v.text());
    assert!(asked.iter().any(|a| a == tag_api));
    assert!(!asked.iter().any(|a| a == LATEST_API));
}

#[test]
fn an_api_answer_that_is_not_a_release_is_a_problem_not_a_crash() {
    let (v, _) = check_over(&[(LATEST_API, r#"{"message":"Not Found"}"#.into())], None);
    assert!(!v.is_ok());
}

#[test]
fn the_key_checked_is_the_one_the_shipped_app_trusts() {
    // sp_update::PUBKEY (was: tauri.conf.json's plugins.updater.pubkey).
    assert_eq!(trusted_pubkey(), sp_update::PUBKEY);
    assert_eq!(
        parse_public_key_box(trusted_pubkey()).unwrap(),
        "E4D0668D54F6EB34"
    );
}

#[test]
fn the_command_checks_a_fake_github_and_says_why_it_fails() {
    // End to end through the binary: the bases moved to 127.0.0.1, the
    // key the shipped app trusts, and a manifest a version behind.
    let server = FakeServer::start();
    let hex = parse_public_key_box(trusted_pubkey()).expect("the shipped key reads");
    let key: [u8; 8] = u64::from_str_radix(&hex, 16)
        .expect("hex key id")
        .to_le_bytes();
    let signature = fake_signature_box("Screepub.app.tar.gz", key);
    let dl = |name: &str| format!("{}/dl/{name}", server.base);
    let assets: Vec<serde_json::Value> = [TAR, "Method-Reader-macOS.app.tar.gz.sig"]
        .iter()
        .map(|n| serde_json::json!({ "name": n, "browser_download_url": dl(n) }))
        .collect();
    server.route(
        "/repos/method-production-suite/method-reader/releases/latest",
        200,
        serde_json::json!({ "tag_name": "v0.6.1", "assets": assets }).to_string(),
    );
    server.route(
        "/dl/Method-Reader-macOS.app.tar.gz.sig",
        200,
        signature.clone(),
    );
    let manifest_for = |v: &str| {
        serde_json::json!({
            "version": v,
            "platforms": { "darwin-aarch64": { "url": dl(TAR), "signature": signature } },
        })
        .to_string()
    };
    server.route(
        "/method-production-suite/method-reader/releases/latest/download/latest.json",
        200,
        manifest_for("0.6.1"),
    );
    let env = [
        ("METHOD_READER_GITHUB_API", server.base.as_str()),
        ("METHOD_READER_GITHUB_WEB", server.base.as_str()),
        ("GH_TOKEN", ""),
        ("GITHUB_TOKEN", ""),
    ];
    let good = xtask(&["check-latest"], &env, None, None);
    assert_eq!(good.status.code(), Some(0), "{}", text(&good.stderr));
    assert!(text(&good.stdout).contains("latest.json serves 0.6.1 for darwin-aarch64"));

    // Signed by another key: named, and refused.
    server.route(
        "/dl/Method-Reader-macOS.app.tar.gz.sig",
        200,
        fake_signature_box("Screepub.app.tar.gz", OTHER_KEY_ID),
    );
    let other = xtask(&["check-latest"], &env, None, None);
    assert_eq!(other.status.code(), Some(1));
    server.route(
        "/dl/Method-Reader-macOS.app.tar.gz.sig",
        200,
        signature.clone(),
    );

    server.route(
        "/method-production-suite/method-reader/releases/latest/download/latest.json",
        200,
        manifest_for("0.6.0"),
    );
    let stale = xtask(&["check-latest"], &env, None, None);
    assert_eq!(stale.status.code(), Some(1));
    let err = text(&stale.stderr);
    assert!(
        err.contains("::error::latest.json says version \"0.6.0\" but the newest release is 0.6.1"),
        "{err}"
    );
    assert!(err.contains("::notice::"));
    assert!(server.asked().iter().any(
        |p| p == "/method-production-suite/method-reader/releases/latest/download/latest.json"
    ));
}
