//! `cargo xtask check-latest [--version vX.Y.Z] [--repo owner/name]`: was
//! tools/check-latest.ts. Does the PUBLISHED latest.json describe the
//! newest release?
//!
//! Exit 0 when the manifest the app will fetch names the newest release,
//! points every platform at an asset that release carries, pastes the
//! exact contents of each asset's .sig, and is signed by the key the app
//! is configured to trust. Exit 1 otherwise, having said which part is
//! wrong. Every problem is reported, not only the first.
//!
//! The updater's version of check-tap, for the same reason: the Homebrew
//! tap served 0.3.0 for five releases because nothing compared the
//! published file to the newest release. So the manifest is read from the
//! EXACT url the app reads it from, `releases/latest/download/latest.json`
//! with GitHub's redirect, never from a checkout or a versioned url.
//!
//! Runs in release.yml's latest-check job and in tap-freshness.yml. Needs
//! no secret; GH_TOKEN is honoured for the API call, for rate limits. The
//! network goes through [`http`], whose bases the environment can move.

pub mod http;

use std::collections::BTreeMap;
use std::path::Path;

use http::{Bases, Fetched, Fetcher};

pub const RELEASE_REPO: &str = "ssandweiss/screepub";
pub const MANIFEST_NAME: &str = "latest.json";

/// The Intel command-line tarball. The identifier release is the last to
/// carry it, and the last whose window archive is universal: the two were
/// dropped together (plan 2026-10-01-f2-handover-amendment.md).
pub const INTEL_CLI_ASSET: &str = "screepub-cli-macos-x64.tar.gz";

/// Every platform the macOS build must answer for: darwin-aarch64 alone
/// since the release after 0.9.0 (Apple silicon only). build-app-bundle's
/// UPDATER_KINDS table names it; REQUIRED, not exhaustive.
pub fn required_platforms() -> Vec<&'static str> {
    vec!["darwin-aarch64"]
}

/// Where the app looks: GitHub's redirect to the newest non-prerelease,
/// non-draft release's asset.
pub fn manifest_endpoint(web: &str, repo: &str) -> String {
    format!("{web}/{repo}/releases/latest/download/{MANIFEST_NAME}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    /// The `browser_download_url`: the exact string the manifest must carry.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Ok {
        version: String,
        platforms: Vec<String>,
    },
    Problems(Vec<String>),
}

impl Verdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Verdict::Ok { .. })
    }

    /// The problems, joined one per line ("" when fine).
    pub fn text(&self) -> String {
        match self {
            Verdict::Ok { .. } => String::new(),
            Verdict::Problems(p) => p.join("\n"),
        }
    }
}

// ── the minisign boxes, read but never verified (update-signature.ts) ──

const SIGNATURE_BYTES: usize = 74;
const PUBLIC_KEY_BYTES: usize = 42;
const GLOBAL_SIGNATURE_BYTES: usize = 64;

/// Decode base64 strictly enough that "not base64 at all" fails: the
/// alphabet is checked before decoding.
fn decode_box(what: &str, encoded: &str) -> Result<Vec<u8>, String> {
    let trimmed = encoded.trim();
    if trimmed.is_empty() {
        return Err(format!("{what}: empty; nothing to decode"));
    }
    let body = trimmed.trim_end_matches('=');
    if body.is_empty()
        || !body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return Err(format!(
            "{what}: not base64 (it contains characters outside the base64 alphabet)"
        ));
    }
    http::decode_base64(trimmed)
}

/// The key id as minisign and tauri print it: the 8 bytes after the
/// 2-byte algorithm, read little-endian, in uppercase hex.
fn key_id_of(bytes: &[u8]) -> String {
    let mut id = [0u8; 8];
    id.copy_from_slice(&bytes[2..10]);
    format!("{:X}", u64::from_le_bytes(id))
}

/// Which key a `.sig` file's contents name.
pub fn parse_signature_box(encoded: &str) -> Result<String, String> {
    let text = String::from_utf8_lossy(&decode_box("signature", encoded)?).into_owned();
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    if lines.len() != 4 {
        return Err(format!(
            "signature: expected four lines (untrusted comment, signature, trusted comment, global signature), found {}",
            lines.len()
        ));
    }
    if !lines[0].starts_with("untrusted comment: ") {
        let head: String = lines[0].chars().take(40).collect();
        return Err(format!(
            "signature: does not begin with \"untrusted comment: \" (begins {})",
            serde_json::to_string(&head).expect("string")
        ));
    }
    let sig = decode_box("signature line", lines[1])?;
    if sig.len() != SIGNATURE_BYTES {
        return Err(format!(
            "signature: the signature line decodes to {} bytes, not the {SIGNATURE_BYTES} of a minisign signature",
            sig.len()
        ));
    }
    if !lines[2].starts_with("trusted comment: ") {
        return Err("signature: third line is not a \"trusted comment: \" line".to_string());
    }
    let global = decode_box("global signature line", lines[3])?;
    if global.len() != GLOBAL_SIGNATURE_BYTES {
        return Err(format!(
            "signature: the global signature decodes to {} bytes, not {GLOBAL_SIGNATURE_BYTES}",
            global.len()
        ));
    }
    Ok(key_id_of(&sig))
}

/// Which key a public key (as the app's config carries it) is.
pub fn parse_public_key_box(encoded: &str) -> Result<String, String> {
    if encoded.trim().is_empty() {
        return Err(
            "public key: empty. tauri.conf.json ships with an empty pubkey until the owner generates \
             the key pair and pastes the public half in (docs/release-secrets.md \u{a7}4)."
                .to_string(),
        );
    }
    let text = String::from_utf8_lossy(&decode_box("public key", encoded)?).into_owned();
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    if lines.len() != 2 || !lines[0].starts_with("untrusted comment: ") {
        return Err(format!(
            "public key: expected an \"untrusted comment: \" line and a key line, found {} line(s)",
            lines.len()
        ));
    }
    let key = decode_box("public key line", lines[1])?;
    if key.len() != PUBLIC_KEY_BYTES {
        return Err(format!(
            "public key: the key line decodes to {} bytes, not the {PUBLIC_KEY_BYTES} of a minisign public key",
            key.len()
        ));
    }
    Ok(key_id_of(&key))
}

// ── the judgement ───────────────────────────────────────────────────────

fn str_of<'a>(v: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    v?.get(key)?.as_str()
}

/// The judgement, pure. `sig_texts` is the contents of each `.sig` asset
/// the release carries, keyed by asset name; `pubkey` is what the app is
/// configured to trust.
pub fn judge_manifest(
    manifest_text: &str,
    release: &Release,
    sig_texts: &BTreeMap<String, String>,
    pubkey: &str,
) -> Verdict {
    let mut problems = Vec::new();
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name)
        .to_string();

    let manifest: serde_json::Value = match serde_json::from_str(manifest_text) {
        Ok(v) => v,
        Err(_) => {
            let head: String = manifest_text.chars().take(60).collect();
            return Verdict::Problems(vec![format!(
                "{MANIFEST_NAME} is not JSON (begins {})",
                serde_json::to_string(&head).expect("string")
            )]);
        }
    };
    if !(manifest.is_object() || manifest.is_array()) {
        return Verdict::Problems(vec![format!("{MANIFEST_NAME} is not a JSON object")]);
    }

    let said = manifest.get("version");
    if said.and_then(|v| v.as_str()) != Some(version.as_str()) {
        let shown = said.map_or_else(|| "undefined".to_string(), |v| v.to_string());
        problems.push(format!(
            "{MANIFEST_NAME} says version {shown} but the newest release is {version}"
        ));
    }

    // Platforms, sorted by key (an array's keys are its indices, as
    // Object.keys gives them).
    let mut platforms: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    match manifest.get("platforms") {
        Some(serde_json::Value::Object(o)) => {
            for (k, v) in o {
                platforms.insert(k.clone(), v.clone());
            }
        }
        Some(serde_json::Value::Array(a)) => {
            for (i, v) in a.iter().enumerate() {
                platforms.insert(i.to_string(), v.clone());
            }
        }
        _ => {}
    }
    let keys: Vec<String> = platforms.keys().cloned().collect();
    if keys.is_empty() {
        problems.push(format!("{MANIFEST_NAME} lists no platforms at all"));
    }
    for required in required_platforms() {
        if !keys.iter().any(|k| k == required) {
            problems.push(format!(
                "{MANIFEST_NAME} has no entry for {required}, which the macOS build must serve"
            ));
        }
    }

    let trusted = match parse_public_key_box(pubkey) {
        Ok(id) => Some(id),
        Err(e) => {
            problems.push(format!(
                "the app's configured public key cannot be read: {e}"
            ));
            None
        }
    };

    // After Intel was dropped the one window archive is Apple silicon
    // only: an Intel row pointing at it would hand an Intel copy a build
    // it cannot run. Both Intel keys, because the updater plugin tries
    // `<os>-<arch>-app` before `<os>-<arch>`.
    let intel_era = release.assets.iter().any(|a| a.name == INTEL_CLI_ASSET);
    if !intel_era {
        let arm_urls: Vec<&str> = ["darwin-aarch64", "darwin-aarch64-app"]
            .iter()
            .filter_map(|k| str_of(platforms.get(*k), "url"))
            .collect();
        for key in ["darwin-x86_64-app", "darwin-x86_64"] {
            if let Some(u) = str_of(platforms.get(key), "url")
                && arm_urls.contains(&u)
            {
                problems.push(format!(
                    "{key} points at {u}, the same archive as darwin-aarch64, and release {} is \
                     Apple silicon only (it carries no {INTEL_CLI_ASSET}): an Intel copy would \
                     install a build it cannot run",
                    release.tag_name
                ));
            }
        }
    }

    for key in &keys {
        let entry = platforms.get(key);
        let url = str_of(entry, "url").unwrap_or("");
        let signature = str_of(entry, "signature").unwrap_or("");
        // The LAST asset with a url wins, as a Map built from the list does.
        let Some(asset) = release.assets.iter().rev().find(|a| a.url == url) else {
            problems.push(format!(
                "{key} points at {}, which is not an asset of release {}",
                if url.is_empty() { "<no url>" } else { url },
                release.tag_name
            ));
            continue;
        };
        let sig_name = format!("{}.sig", asset.name);
        match sig_texts.get(&sig_name) {
            None => problems.push(format!(
                "release {} carries no {sig_name} beside {}",
                release.tag_name, asset.name
            )),
            Some(published) if published != signature => problems.push(format!(
                "{key}: the signature in {MANIFEST_NAME} is not the contents of {sig_name}"
            )),
            Some(_) => {}
        }
        match parse_signature_box(signature) {
            Ok(key_id) => {
                if let Some(t) = &trusted
                    && &key_id != t
                {
                    problems.push(format!(
                        "{key}: signed by key {key_id}, but the app trusts key {t}. tauri-cli only \
                         warns about this; every install would fail."
                    ));
                }
            }
            Err(e) => problems.push(format!(
                "{key}: the signature is not a minisign signature: {e}"
            )),
        }
    }

    if problems.is_empty() {
        Verdict::Ok {
            version,
            platforms: keys,
        }
    } else {
        Verdict::Problems(problems)
    }
}

/// Fetch the release, the manifest and every signature, then judge.
pub fn check_latest(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    pubkey: &str,
    version: Option<&str>,
) -> Verdict {
    let api_url = match version {
        Some(v) => format!(
            "{}/repos/{repo}/releases/tags/v{}",
            bases.api,
            v.strip_prefix('v').unwrap_or(v)
        ),
        None => format!("{}/repos/{repo}/releases/latest", bases.api),
    };
    let unreachable =
        |url: &str, e: String| Verdict::Problems(vec![format!("{url} could not be fetched: {e}")]);
    let api: Fetched = match fetcher(&api_url) {
        Ok(f) => f,
        Err(e) => return unreachable(&api_url, e),
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&api.text) else {
        return Verdict::Problems(vec![format!(
            "{api_url} did not answer JSON (HTTP {})",
            api.status
        )]);
    };
    let tag = parsed.get("tag_name").and_then(|t| t.as_str());
    let assets = parsed.get("assets").and_then(|a| a.as_array());
    let (Some(tag), Some(assets), 200) = (tag, assets, api.status) else {
        let head: String = api.text.chars().take(120).collect();
        return Verdict::Problems(vec![format!(
            "{api_url} did not describe a release (HTTP {}: {head})",
            api.status
        )]);
    };
    let release = Release {
        tag_name: tag.to_string(),
        assets: assets
            .iter()
            .map(|a| ReleaseAsset {
                name: str_of(Some(a), "name").unwrap_or_default().to_string(),
                url: str_of(Some(a), "browser_download_url")
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect(),
    };

    let endpoint = manifest_endpoint(&bases.web, repo);
    let manifest = match fetcher(&endpoint) {
        Ok(f) => f,
        Err(e) => return unreachable(&endpoint, e),
    };
    if manifest.status != 200 {
        return Verdict::Problems(vec![format!(
            "{endpoint} answered HTTP {}: release {} publishes no {MANIFEST_NAME}, so the app's \
             update check fails on every machine that makes it.",
            manifest.status, release.tag_name
        )]);
    }

    let mut sig_texts = BTreeMap::new();
    for asset in &release.assets {
        if !asset.name.ends_with(".sig") {
            continue;
        }
        if let Ok(got) = fetcher(&asset.url)
            && got.status == 200
        {
            sig_texts.insert(asset.name.clone(), got.text);
        }
    }

    judge_manifest(&manifest.text, &release, &sig_texts, pubkey)
}

/// What the shipped app trusts: tauri.conf.json's plugins.updater.pubkey
/// while the Tauri window exists. The Rust app has no updater key yet;
/// when it has one, its location is read here. Empty when there is none,
/// which the judgement reports.
pub fn configured_pubkey(repo_dir: &Path) -> String {
    let conf = repo_dir.join("desktop/src-tauri/tauri.conf.json");
    let Ok(text) = std::fs::read_to_string(conf) else {
        return String::new();
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("plugins")?
                .get("updater")?
                .get("pubkey")?
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn parse_args(args: &[String]) -> Result<(Option<String>, String), String> {
    let mut version = None;
    let mut repo = RELEASE_REPO.to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("option '{flag} <value>' argument missing"))
        };
        match flag {
            "--version" => version = Some(value()?),
            "--repo" => repo = value()?,
            other if other.starts_with('-') => return Err(format!("unknown option '{other}'")),
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }
    Ok((version, repo))
}

/// `cargo xtask check-latest`: its exit code.
pub fn main(args: &[String]) -> u8 {
    let (version, repo) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("check-latest: {e}");
            return 1;
        }
    };
    let bases = Bases::from_env();
    let pubkey = configured_pubkey(&crate::paths::repo_root());
    let mut fetcher = http::real_fetcher(&bases);
    match check_latest(&mut fetcher, &bases, &repo, &pubkey, version.as_deref()) {
        Verdict::Ok { version, platforms } => {
            println!(
                "{MANIFEST_NAME} serves {version} for {}, signed by the key the app trusts.",
                platforms.join(", ")
            );
            0
        }
        Verdict::Problems(problems) => {
            for p in problems {
                eprintln!("::error::{p}");
            }
            eprintln!(
                "::notice::A red latest-check means the release published and the app's updater \
                 cannot use it. app-upload writes latest.json from what arrived; see \
                 docs/release-secrets.md \u{a7}4."
            );
            1
        }
    }
}
