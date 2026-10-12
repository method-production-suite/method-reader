//! Does the PUBLISHED latest.json describe the newest release?
//! tools/check-latest.ts in Rust, same judgement, same words: run by
//! release.yml's latest-check job (per release) and tap-freshness.yml
//! (weekly), reading the manifest from the exact address the apps read,
//! GitHub's redirect included.
//!
//! `--deep` goes one step further than the TypeScript did: it downloads
//! each archive the manifest names and verifies it the way the Rust app
//! will (signed version required), which also proves every Tauri window's
//! looser check passes.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::names::{MANIFEST, SIGNATURE_EXT, UPDATER_KINDS};

/// The Intel command-line tarball: carried by 0.9.0 and earlier only.
pub const INTEL_CLI_ASSET: &str = "screepub-cli-macos-x64.tar.gz";

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

/// The key id a minisign signature or public key box names, as hex.
fn box_key_id(b64_box: &str, sig: bool) -> Result<String, String> {
    let text = sp_update::verify::base64_to_string(b64_box.trim()).map_err(|e| e.to_string())?;
    let line = text
        .lines()
        .nth(1)
        .ok_or("missing the second line of the box")?;
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(line.trim())
        .map_err(|e| e.to_string())?;
    let want = if sig { 74 } else { 42 };
    if raw.len() != want {
        return Err(format!("the box holds {} bytes, not {want}", raw.len()));
    }
    Ok(raw[2..10]
        .iter()
        .rev()
        .map(|b| format!("{b:02X}"))
        .collect())
}

pub fn required_platforms() -> Vec<&'static str> {
    UPDATER_KINDS
        .iter()
        .flat_map(|k| k.platform_keys.iter().copied())
        .collect()
}

/// `judgeManifest`: every problem, not only the first.
pub fn judge_manifest(
    manifest_text: &str,
    release: &Release,
    sig_texts: &BTreeMap<String, String>,
    pubkey: &str,
) -> Result<(String, Vec<String>), Vec<String>> {
    let mut problems = Vec::new();
    let version = release.tag_name.trim_start_matches('v').to_string();
    let manifest: Value = match serde_json::from_str(manifest_text) {
        Ok(v) => v,
        Err(_) => {
            let head: String = manifest_text.chars().take(60).collect();
            return Err(vec![format!("{MANIFEST} is not JSON (begins {head:?})")]);
        }
    };
    if !manifest.is_object() {
        return Err(vec![format!("{MANIFEST} is not a JSON object")]);
    }
    if manifest["version"].as_str() != Some(version.as_str()) {
        problems.push(format!(
            "{MANIFEST} says version {} but the newest release is {version}",
            manifest["version"]
        ));
    }
    let empty = serde_json::Map::new();
    let platforms = manifest["platforms"].as_object().unwrap_or(&empty);
    let keys: Vec<String> = platforms.keys().cloned().collect();
    if keys.is_empty() {
        problems.push(format!("{MANIFEST} lists no platforms at all"));
    }
    for required in required_platforms() {
        if !keys.iter().any(|k| k == required) {
            problems.push(format!(
                "{MANIFEST} has no entry for {required}, which the macOS build must serve"
            ));
        }
    }
    let trusted = match box_key_id(pubkey, false) {
        Ok(id) => Some(id),
        Err(e) => {
            problems.push(format!(
                "the app's configured public key cannot be read: {e}"
            ));
            None
        }
    };
    let intel_era = release.assets.iter().any(|a| a.name == INTEL_CLI_ASSET);
    if !intel_era {
        let arm: Vec<&str> = ["darwin-aarch64", "darwin-aarch64-app"]
            .iter()
            .filter_map(|k| platforms.get(*k).and_then(|p| p["url"].as_str()))
            .collect();
        for key in ["darwin-x86_64-app", "darwin-x86_64"] {
            if let Some(u) = platforms.get(key).and_then(|p| p["url"].as_str())
                && arm.contains(&u)
            {
                problems.push(format!(
                    "{key} points at {u}, the same archive as darwin-aarch64, and release {} is Apple silicon only (it carries no {INTEL_CLI_ASSET}): an Intel copy would install a build it cannot run",
                    release.tag_name
                ));
            }
        }
    }
    for key in &keys {
        let entry = &platforms[key];
        let url = entry["url"].as_str().unwrap_or("");
        let signature = entry["signature"].as_str().unwrap_or("");
        let Some(asset) = release.assets.iter().find(|a| a.url == url) else {
            problems.push(format!(
                "{key} points at {}, which is not an asset of release {}",
                if url.is_empty() { "<no url>" } else { url },
                release.tag_name
            ));
            continue;
        };
        let sig_name = format!("{}{SIGNATURE_EXT}", asset.name);
        match sig_texts.get(&sig_name) {
            None => problems.push(format!(
                "release {} carries no {sig_name} beside {}",
                release.tag_name, asset.name
            )),
            Some(published) if published != signature => problems.push(format!(
                "{key}: the signature in {MANIFEST} is not the contents of {sig_name}"
            )),
            _ => {}
        }
        match box_key_id(signature, true) {
            Ok(id) => {
                if let Some(t) = &trusted
                    && &id != t
                {
                    problems.push(format!(
                        "{key}: signed by key {id}, but the app trusts key {t}. tauri-cli only warns about this; every install would fail."
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

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent("screepub-check-latest")
        .build()
        .into()
}

fn fetch(url: &str) -> Result<(u16, Vec<u8>), String> {
    let mut req = agent().get(url);
    if url.starts_with("https://api.github.com/") {
        req = req.header("Accept", "application/vnd.github+json");
        if let Ok(t) = std::env::var("GH_TOKEN")
            && !t.is_empty()
        {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
    }
    let mut res = req.call().map_err(|e| format!("{url}: {e}"))?;
    let status = res.status().as_u16();
    let body = res
        .body_mut()
        .with_config()
        .limit(1 << 30)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))?;
    Ok((status, body))
}

/// Fetch the release, the manifest and every signature, then judge.
pub fn check_latest(
    repo: &str,
    pubkey: &str,
    version: Option<&str>,
    deep: bool,
) -> Result<(String, Vec<String>), Vec<String>> {
    let api = match version {
        Some(v) => format!(
            "https://api.github.com/repos/{repo}/releases/tags/v{}",
            v.trim_start_matches('v')
        ),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let (status, body) = fetch(&api).map_err(|e| vec![e])?;
    let text = String::from_utf8_lossy(&body);
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|_| vec![format!("{api} did not answer JSON (HTTP {status})")])?;
    let (Some(tag), Some(assets)) = (parsed["tag_name"].as_str(), parsed["assets"].as_array())
    else {
        let head: String = text.chars().take(120).collect();
        return Err(vec![format!(
            "{api} did not describe a release (HTTP {status}: {head})"
        )]);
    };
    if status != 200 {
        return Err(vec![format!(
            "{api} did not describe a release (HTTP {status})"
        )]);
    }
    let release = Release {
        tag_name: tag.to_string(),
        assets: assets
            .iter()
            .map(|a| Asset {
                name: a["name"].as_str().unwrap_or("").to_string(),
                url: a["browser_download_url"].as_str().unwrap_or("").to_string(),
            })
            .collect(),
    };
    let endpoint = format!("https://github.com/{repo}/releases/latest/download/{MANIFEST}");
    let (mstatus, mbody) = fetch(&endpoint).map_err(|e| vec![e])?;
    if mstatus != 200 {
        return Err(vec![format!(
            "{endpoint} answered HTTP {mstatus}: release {} publishes no {MANIFEST}, so the app's update check fails on every machine that makes it.",
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
                        "https://github.com/ssandweiss/screepub/releases/download/v0.12.0/{n}"
                    ),
                })
                .collect(),
        }
    }

    #[test]
    fn a_good_manifest_passes_and_a_foreign_key_fails() {
        let (sk_text, pk) = crate::sign::keygen("").unwrap();
        let sk = crate::sign::load_secret_key(&sk_text, "").unwrap();
        let sig = crate::sign::sign(&sk, b"x", "Screepub-macOS.app.tar.gz", "0.12.0", 1).unwrap();
        let r = release(&["Screepub-macOS.app.tar.gz", "Screepub-macOS.app.tar.gz.sig"]);
        let m = crate::manifest::render_manifest(
            "0.12.0",
            &[crate::manifest::SignedArtifact {
                name: "Screepub-macOS.app.tar.gz".into(),
                signature: sig.clone(),
                platform_keys: vec!["darwin-aarch64".into()],
            }],
            "ssandweiss/screepub",
            "2026-10-12T00:00:00.000Z",
        )
        .unwrap();
        let mut sigs = BTreeMap::new();
        sigs.insert("Screepub-macOS.app.tar.gz.sig".to_string(), sig);
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
        assert!(box_key_id(sp_update::PUBKEY, false).is_ok());
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
