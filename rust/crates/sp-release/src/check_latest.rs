//! Does the PUBLISHED latest.json describe the newest release? Was
//! tools/check-latest.ts, same judgement, same words. Run by release.yml's
//! latest-check job (per release) and tap-freshness.yml (weekly) through
//! `sp-release check-latest`, and by `cargo xtask check-latest`, both on
//! this one implementation. The manifest is read from the exact address
//! the apps read, GitHub's redirect included, never from a checkout or a
//! versioned url: the Homebrew tap served 0.3.0 for five releases because
//! nothing compared the published file to the newest release.
//!
//! The verdict is good when the manifest names the newest release, points
//! every platform at an asset that release carries, pastes the exact
//! contents of each asset's .sig, and is signed by the key the app trusts.
//! Every problem is reported, not only the first.
//!
//! `deep` goes one step further than the TypeScript did: it downloads each
//! archive the manifest names and verifies it the way the Rust app will
//! (signed version required), which also proves every Tauri window's
//! looser check passes.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::boxes::{SignatureBox, parse_public_key_box, parse_signature_box};
use crate::names::{MANIFEST as MANIFEST_NAME, SIGNATURE_EXT, UPDATER_KINDS};

/// The Intel command-line tarball. The identifier release is the last to
/// carry it, and the last whose window archive is universal: the two were
/// dropped together (plan 2026-10-01-f2-handover-amendment.md).
pub const INTEL_CLI_ASSET: &str = "screepub-cli-macos-x64.tar.gz";

/// Where GitHub answers API calls and serves release downloads.
pub const GITHUB_API: &str = "https://api.github.com";
pub const GITHUB_WEB: &str = "https://github.com";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// The `browser_download_url`: the exact string the manifest must carry.
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

/// Every platform the macOS build must answer for (REQUIRED, not
/// exhaustive): the keys of [`UPDATER_KINDS`], darwin-aarch64 alone since
/// the release after 0.9.0 (Apple silicon only).
pub fn required_platforms() -> Vec<&'static str> {
    UPDATER_KINDS
        .iter()
        .flat_map(|k| k.platform_keys.iter().copied())
        .collect()
}

/// Where the app looks: GitHub's redirect to the newest non-prerelease,
/// non-draft release's asset.
pub fn manifest_endpoint(web: &str, repo: &str) -> String {
    format!("{web}/{repo}/releases/latest/download/{MANIFEST_NAME}")
}

fn str_of<'a>(v: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    v?.get(key)?.as_str()
}

/// The judgement, pure (`judgeManifest`). `sig_texts` is the contents of each `.sig` asset
/// the release carries, keyed by asset name; `pubkey` is what the app is
/// configured to trust.
pub fn judge_manifest(
    manifest_text: &str,
    release: &Release,
    sig_texts: &BTreeMap<String, String>,
    pubkey: &str,
) -> Result<(String, Vec<String>), Vec<String>> {
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
            return Err(vec![format!(
                "{MANIFEST_NAME} is not JSON (begins {})",
                serde_json::to_string(&head).expect("string")
            )]);
        }
    };
    if !(manifest.is_object() || manifest.is_array()) {
        return Err(vec![format!("{MANIFEST_NAME} is not a JSON object")]);
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
        Ok(k) => Some(k.key_id),
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
            Ok(SignatureBox { key_id, .. }) => {
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
        Ok((version, keys))
    } else {
        Err(problems)
    }
}

/// One GET: the status and the body. Tests and `cargo xtask` pass their
/// own; [`check_latest`] passes the real one.
pub type Fetch<'a> = dyn FnMut(&str) -> Result<(u16, Vec<u8>), String> + 'a;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent("method-reader-check-latest")
        .build()
        .into()
}

/// A real GET. API calls carry GitHub's Accept header and the token when
/// there is one (GH_TOKEN, else GITHUB_TOKEN), for rate limits.
pub fn fetch(url: &str) -> Result<(u16, Vec<u8>), String> {
    let mut req = agent().get(url);
    if url.starts_with(GITHUB_API) {
        req = req
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        let token = std::env::var("GH_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()));
        if let Some(t) = token {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
    }
    let mut res = req.call().map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let body = res
        .body_mut()
        .with_config()
        .limit(1 << 30)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// Fetch the release, the manifest and every signature through `fetch`,
/// against the given API and web bases, then judge; with `deep`, also
/// download and verify every archive the manifest names.
pub fn check_with(
    fetch: &mut Fetch<'_>,
    api_base: &str,
    web_base: &str,
    repo: &str,
    pubkey: &str,
    version: Option<&str>,
    deep: bool,
) -> Result<(String, Vec<String>), Vec<String>> {
    let api_url = match version {
        Some(v) => format!(
            "{api_base}/repos/{repo}/releases/tags/v{}",
            v.strip_prefix('v').unwrap_or(v)
        ),
        None => format!("{api_base}/repos/{repo}/releases/latest"),
    };
    let unreachable = |url: &str, e: String| vec![format!("{url} could not be fetched: {e}")];
    let (status, body) = fetch(&api_url).map_err(|e| unreachable(&api_url, e))?;
    let text = String::from_utf8_lossy(&body).into_owned();
    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
        return Err(vec![format!(
            "{api_url} did not answer JSON (HTTP {status})"
        )]);
    };
    let tag = parsed.get("tag_name").and_then(|t| t.as_str());
    let assets = parsed.get("assets").and_then(|a| a.as_array());
    let (Some(tag), Some(assets), 200) = (tag, assets, status) else {
        let head: String = text.chars().take(120).collect();
        return Err(vec![format!(
            "{api_url} did not describe a release (HTTP {status}: {head})"
        )]);
    };
    let release = Release {
        tag_name: tag.to_string(),
        assets: assets
            .iter()
            .map(|a| Asset {
                name: str_of(Some(a), "name").unwrap_or_default().to_string(),
                url: str_of(Some(a), "browser_download_url")
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect(),
    };

    let endpoint = manifest_endpoint(web_base, repo);
    let (mstatus, mbody) = fetch(&endpoint).map_err(|e| unreachable(&endpoint, e))?;
    if mstatus != 200 {
        return Err(vec![format!(
            "{endpoint} answered HTTP {mstatus}: release {} publishes no {MANIFEST_NAME}, so the app's update check fails on every machine that makes it.",
            release.tag_name
        )]);
    }
    let mut sigs = BTreeMap::new();
    for a in &release.assets {
        if a.name.ends_with(SIGNATURE_EXT)
            && let Ok((200, b)) = fetch(&a.url)
        {
            sigs.insert(a.name.clone(), String::from_utf8_lossy(&b).into_owned());
        }
    }
    let manifest_text = String::from_utf8_lossy(&mbody).into_owned();
    let verdict = judge_manifest(&manifest_text, &release, &sigs, pubkey)?;
    if deep {
        let manifest: Value =
            serde_json::from_str(&manifest_text).map_err(|e| vec![e.to_string()])?;
        let mut problems = Vec::new();
        if let Some(platforms) = manifest["platforms"].as_object() {
            for (key, p) in platforms {
                let url = p["url"].as_str().unwrap_or("");
                match fetch(url) {
                    Ok((200, bytes)) => {
                        if let Err(e) = sp_update::verify::verify_signature(
                            &bytes,
                            p["signature"].as_str().unwrap_or(""),
                            pubkey,
                            &verdict.0,
                            true,
                        ) {
                            problems.push(format!("{key}: {url} does not verify: {e}"));
                        }
                    }
                    Ok((s, _)) => problems.push(format!("{key}: {url} answered HTTP {s}")),
                    Err(e) => problems.push(format!("{key}: {e}")),
                }
            }
        }
        if !problems.is_empty() {
            return Err(problems);
        }
    }
    Ok(verdict)
}

/// [`check_with`] against GitHub itself, with the real [`fetch`].
pub fn check_latest(
    repo: &str,
    pubkey: &str,
    version: Option<&str>,
    deep: bool,
) -> Result<(String, Vec<String>), Vec<String>> {
    check_with(
        &mut fetch, GITHUB_API, GITHUB_WEB, repo, pubkey, version, deep,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(names: &[&str]) -> Release {
        Release {
            tag_name: "v0.12.0".into(),
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    url: format!(
                        "https://github.com/method-production-suite/method-reader/releases/download/v0.12.0/{n}"
                    ),
                })
                .collect(),
        }
    }

    #[test]
    fn a_good_manifest_passes_and_a_foreign_key_fails() {
        let (sk_text, pk) = crate::sign::keygen("").unwrap();
        let sk = crate::sign::load_secret_key(&sk_text, "").unwrap();
        let sig =
            crate::sign::sign(&sk, b"x", "Method-Reader-macOS.app.tar.gz", "0.12.0", 1).unwrap();
        let r = release(&[
            "Method-Reader-macOS.app.tar.gz",
            "Method-Reader-macOS.app.tar.gz.sig",
        ]);
        let m = crate::manifest::render_manifest(
            "0.12.0",
            &[crate::manifest::SignedArtifact {
                name: "Method-Reader-macOS.app.tar.gz".into(),
                signature: sig.clone(),
                platform_keys: vec!["darwin-aarch64".into()],
            }],
            "method-production-suite/method-reader",
            "2026-10-12T00:00:00.000Z",
        )
        .unwrap();
        let mut sigs = BTreeMap::new();
        sigs.insert("Method-Reader-macOS.app.tar.gz.sig".to_string(), sig);
        let text = m.to_string();
        assert_eq!(
            judge_manifest(&text, &r, &sigs, &pk).unwrap(),
            ("0.12.0".into(), vec!["darwin-aarch64".into()])
        );
        let other = crate::sign::keygen("").unwrap().1;
        let problems = judge_manifest(&text, &r, &sigs, &other).unwrap_err();
        assert!(
            problems[0].contains("but the app trusts key"),
            "{problems:?}"
        );
        // The shipped key reads.
        assert!(parse_public_key_box(sp_update::PUBKEY).is_ok());
    }

    #[test]
    fn stale_or_empty_manifests_say_so() {
        let r = release(&[]);
        let sigs = BTreeMap::new();
        let p = judge_manifest(
            r#"{"version":"0.11.1","platforms":{}}"#,
            &r,
            &sigs,
            sp_update::PUBKEY,
        )
        .unwrap_err();
        assert_eq!(
            p,
            [
                "latest.json says version \"0.11.1\" but the newest release is 0.12.0",
                "latest.json lists no platforms at all",
                "latest.json has no entry for darwin-aarch64, which the macOS build must serve",
            ]
        );
        assert!(
            judge_manifest("<html>", &r, &sigs, sp_update::PUBKEY).unwrap_err()[0]
                .contains("is not JSON")
        );
    }
}
