//! The network half: `Updater::check` and `Update::download` from
//! tauri-plugin-updater 2.12.0, over ureq and rustls instead of reqwest.
//!
//! Kept from the plugin, rule for rule: every endpoint is tried in order;
//! `{{current_version}}`, `{{target}}`, `{{arch}}` and `{{bundle_type}}`
//! are filled in; a 204 means "nothing new"; a non-success status is
//! skipped without becoming the error (so a lone 404 reads "Could not
//! fetch a valid release JSON from the remote"); a manifest that does not
//! parse is the error; the platform is looked up BEFORE the version is
//! compared, so a build the manifest has no row for always hears why; and
//! newer means semver-greater. The download is verified before anything
//! sees it.

use std::io::Read;
use std::time::Duration;

use semver::Version;

use crate::manifest::Release;
use crate::target::{Installer, TargetSet};
use crate::verify::{VerifyError, verify_signature};

/// The plugin's User-Agent was `tauri-plugin-updater/2.12.0`; this one
/// says what is asking.
pub const USER_AGENT: &str = concat!("method-reader-update/", env!("CARGO_PKG_VERSION"));

/// No update archive is anywhere near this; a server that keeps sending is
/// refused rather than allowed to fill the disk.
pub const MAX_DOWNLOAD_BYTES: u64 = 1 << 30;

#[derive(Clone, Debug)]
pub struct Config {
    pub endpoints: Vec<String>,
    /// base64 of the minisign public key box, as tauri.conf.json carries it.
    pub pubkey: String,
    /// The plugin's `dangerousInsecureTransportProtocol`: plain http
    /// endpoints allowed. Only a rehearsal or a test sets it.
    pub allow_insecure: bool,
    /// The plugin's `requireSignedVersion`, off in every Tauri window. The
    /// Rust app turns it ON: every release it can be offered is signed by
    /// `sp-release sign-update`, which always records the version.
    pub require_signed_version: bool,
    /// The whole request, connect to last byte. The plugin set none.
    pub timeout: Option<Duration>,
    /// How this copy was installed, for the `<os>-<arch>-<installer>` key.
    pub installer: Option<Installer>,
    /// Overrides the `<os>-<arch>` keys (the plugin's `target`): tests only.
    pub targets: Option<TargetSet>,
}

impl Config {
    /// What a Method Reader build trusts and asks: the baked endpoint and key
    /// (see `crate::baked_endpoint`), signed versions required.
    pub fn method_reader(installer: Option<Installer>) -> Self {
        Self {
            endpoints: vec![crate::baked_endpoint().to_string()],
            pubkey: crate::baked_pubkey().to_string(),
            allow_insecure: crate::baked_insecure(),
            require_signed_version: true,
            timeout: Some(Duration::from_secs(60 * 10)),
            installer,
            targets: None,
        }
    }

    fn target_set(&self) -> Result<TargetSet, UpdateError> {
        match &self.targets {
            Some(t) => Ok(t.clone()),
            None => TargetSet::current(self.installer).map_err(UpdateError::Platform),
        }
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(self.timeout)
            .http_status_as_error(false)
            .https_only(!self.allow_insecure)
            .user_agent(USER_AGENT)
            .build()
            .into()
    }
}

/// Everything that can stop a check or a download, worded for a person.
#[derive(Debug)]
pub enum UpdateError {
    EmptyEndpoints,
    Insecure,
    Platform(String),
    Network(String),
    Manifest(String),
    ReleaseNotFound,
    Download(String),
    TooLarge,
    Verify(VerifyError),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyEndpoints => f.write_str("Updater does not have any endpoints set."),
            Self::Insecure => f.write_str(
                "The configured updater endpoint must use a secure protocol like `https`.",
            ),
            Self::Platform(s) | Self::Network(s) | Self::Manifest(s) => f.write_str(s),
            Self::ReleaseNotFound => {
                f.write_str("Could not fetch a valid release JSON from the remote")
            }
            // The plugin's `Error::Network` is displayed in backticks.
            Self::Download(s) => write!(f, "`{s}`"),
            Self::TooLarge => f.write_str("The update is larger than any Method Reader could be."),
            Self::Verify(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// What a check found: a release newer than this copy, for this platform.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub current_version: String,
    pub version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    /// The key that matched, e.g. `darwin-aarch64`.
    pub target: String,
    pub download_url: String,
    pub signature: String,
    pub raw_json: serde_json::Value,
}

/// Percent-encode controls and `+`, as the plugin does for
/// `{{current_version}}` (build metadata carries a `+`).
fn encode_version(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b < 0x20 || b == 0x7f || b == b'+' || b >= 0x80 {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out
}

/// The endpoint with its placeholders filled in, raw or already encoded.
pub fn fill_endpoint(url: &str, current: &Version, targets: &TargetSet, installer: &str) -> String {
    let version = encode_version(&current.to_string());
    url.replace("%7B%7Bcurrent_version%7D%7D", &version)
        .replace("%7B%7Btarget%7D%7D", &targets.os)
        .replace("%7B%7Barch%7D%7D", &targets.arch)
        .replace("%7B%7Bbundle_type%7D%7D", installer)
        .replace("{{current_version}}", &version)
        .replace("{{target}}", &targets.os)
        .replace("{{arch}}", &targets.arch)
        .replace("{{bundle_type}}", installer)
}

/// `Updater::check`: `Ok(None)` when nothing is newer (or the server said
/// 204), `Ok(Some)` for a newer release this platform has a file in.
pub fn check(config: &Config, current: &Version) -> Result<Option<Update>, UpdateError> {
    if config.endpoints.is_empty() {
        return Err(UpdateError::EmptyEndpoints);
    }
    if !config.allow_insecure
        && config
            .endpoints
            .iter()
            .any(|e| !e.to_ascii_lowercase().starts_with("https://"))
    {
        return Err(UpdateError::Insecure);
    }
    let targets = config.target_set()?;
    let installer = config.installer.map(Installer::name).unwrap_or("unknown");
    let agent = config.agent();

    let mut found: Option<(Release, serde_json::Value)> = None;
    let mut last_error: Option<UpdateError> = None;
    for endpoint in &config.endpoints {
        let url = fill_endpoint(endpoint, current, &targets, installer);
        match agent.get(&url).header("Accept", "application/json").call() {
            Ok(mut res) => {
                let status = res.status().as_u16();
                if !(200..300).contains(&status) {
                    continue;
                }
                if status == 204 {
                    return Ok(None);
                }
                let text = match res.body_mut().read_to_string() {
                    Ok(t) => t,
                    Err(e) => {
                        last_error = Some(UpdateError::Network(e.to_string()));
                        continue;
                    }
                };
                let parsed = serde_json::from_str::<serde_json::Value>(&text)
                    .map_err(|e| UpdateError::Manifest(e.to_string()))
                    .and_then(|raw| {
                        Release::from_value(raw.clone())
                            .map(|r| (r, raw))
                            .map_err(UpdateError::Manifest)
                    });
                match parsed {
                    Ok(pair) => {
                        last_error = None;
                        found = Some(pair);
                        break;
                    }
                    Err(e) => last_error = Some(e),
                }
            }
            Err(e) => last_error = Some(UpdateError::Network(e.to_string())),
        }
    }
    if let Some(e) = last_error {
        return Err(e);
    }
    let (release, raw_json) = found.ok_or(UpdateError::ReleaseNotFound)?;
    let platform = targets.pick(&release).map_err(UpdateError::Platform)?;
    if release.version <= *current {
        return Ok(None);
    }
    let target = targets
        .keys
        .iter()
        .find(|k| release.platform(k).is_ok())
        .cloned()
        .unwrap_or_default();
    Ok(Some(Update {
        current_version: current.to_string(),
        version: release.version.to_string(),
        body: release.notes.clone(),
        date: release.pub_date.clone(),
        target,
        download_url: platform.url.clone(),
        signature: platform.signature.clone(),
        raw_json,
    }))
}

/// `Update::download`: the bytes, reported as they arrive
/// (`on_chunk(chunk_len, content_length)`), then `on_finish`, then
/// verified. Nothing unverified is ever returned.
pub fn download(
    config: &Config,
    update: &Update,
    mut on_chunk: impl FnMut(usize, Option<u64>),
    on_finish: impl FnOnce(),
) -> Result<Vec<u8>, UpdateError> {
    let agent = config.agent();
    let mut res = agent
        .get(&update.download_url)
        .header("Accept", "application/octet-stream")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = res.status();
    if !status.is_success() {
        let reason = status.canonical_reason().unwrap_or("");
        return Err(UpdateError::Download(
            format!(
                "Download request failed with status: {} {reason}",
                status.as_u16()
            )
            .trim_end()
            .to_string(),
        ));
    }
    let length: Option<u64> = res
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok());
    if length.is_some_and(|n| n > MAX_DOWNLOAD_BYTES) {
        return Err(UpdateError::TooLarge);
    }
    let mut reader = res.body_mut().as_reader();
    let mut buffer = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if buffer.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(UpdateError::TooLarge);
        }
        on_chunk(n, length);
    }
    on_finish();
    verify_signature(
        &buffer,
        &update.signature,
        &config.pubkey,
        &update.version,
        config.require_signed_version,
    )
    .map_err(UpdateError::Verify)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_as_the_plugin_fills_them() {
        let t = TargetSet::new("darwin", "aarch64", Some(Installer::App));
        let v = Version::parse("0.11.1+build.7").unwrap();
        assert_eq!(
            fill_endpoint(
                "https://x/{{target}}/{{arch}}/{{current_version}}?b={{bundle_type}}",
                &v,
                &t,
                "app"
            ),
            "https://x/darwin/aarch64/0.11.1%2Bbuild.7?b=app"
        );
        assert_eq!(
            fill_endpoint("https://x/%7B%7Btarget%7D%7D", &v, &t, "app"),
            "https://x/darwin"
        );
    }

    #[test]
    fn a_plain_http_endpoint_is_refused_unless_allowed() {
        let mut c = Config::method_reader(None);
        c.endpoints = vec!["http://127.0.0.1:9/latest.json".into()];
        c.allow_insecure = false;
        let err = check(&c, &Version::new(0, 11, 1)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "The configured updater endpoint must use a secure protocol like `https`."
        );
        c.endpoints.clear();
        assert!(matches!(
            check(&c, &Version::new(0, 11, 1)),
            Err(UpdateError::EmptyEndpoints)
        ));
    }

    #[test]
    fn a_release_build_trusts_the_shipped_key_and_endpoint() {
        // A rehearsal build bakes others in; `cargo test` never does.
        let c = Config::method_reader(Some(Installer::App));
        assert_eq!(c.endpoints, [crate::ENDPOINT]);
        assert_eq!(c.pubkey, crate::PUBKEY);
        assert!(!c.allow_insecure);
        assert!(c.require_signed_version);
    }
}
