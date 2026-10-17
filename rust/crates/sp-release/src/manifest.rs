//! latest.json: tools/build-update-manifest.ts in Rust, same rules and same
//! words. Built in release.yml's app-upload job over the folder every leg's
//! files arrived in, so it describes exactly what is published beside it,
//! and uploaded LAST.
//!
//! Before it goes out, [`verify_dir`] checks every signature it names
//! against the file it points at, with the public key every Method Reader
//! trusts and the version rule the Rust app requires (the 0.11.x windows'
//! rule is the same check with the version optional, so passing this
//! passes theirs).

use std::path::Path;

use serde_json::{Map, Value, json};

use crate::boxes::parse_signature_box;
use crate::names::{SIGNATURE_EXT, UPDATER_KINDS, asset_url, is_release_version, release_page_url};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedArtifact {
    pub name: String,
    /// The `.sig` file's content, verbatim.
    pub signature: String,
    pub platform_keys: Vec<String>,
}

/// Every `<artifact>.sig` in `dir` with its artifact beside it, sorted.
pub fn collect_signed_artifacts(dir: &Path) -> Result<Vec<SignedArtifact>, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|_| {
            format!(
                "build-update-manifest: no such directory: {}",
                dir.display()
            )
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut out = Vec::new();
    for sig_name in &names {
        let Some(name) = sig_name.strip_suffix(SIGNATURE_EXT) else {
            continue;
        };
        if !names.iter().any(|n| n == name) {
            return Err(format!(
                "build-update-manifest: {sig_name} has no such artifact beside it ({name} is missing from {}). A signature for a file that did not arrive must not be published.",
                dir.display()
            ));
        }
        let kind = UPDATER_KINDS.iter().find(|k| k.name == name).ok_or_else(|| {
            format!(
                "build-update-manifest: {sig_name} signs {name}, and no row in UPDATER_KINDS (sp-release names.rs) maps that artifact to a platform key. Add the row, or stop signing it; a signed artifact no platform can be offered is a manifest that lies by omission."
            )
        })?;
        let signature = std::fs::read_to_string(dir.join(sig_name))
            .map_err(|e| format!("build-update-manifest: cannot read {sig_name}: {e}"))?;
        parse_signature_box(&signature).map_err(|e| {
            format!("build-update-manifest: {sig_name} is not a minisign signature: {e}")
        })?;
        out.push(SignedArtifact {
            name: name.to_string(),
            signature,
            platform_keys: kind.platform_keys.iter().map(|k| k.to_string()).collect(),
        });
    }
    Ok(out)
}

/// The manifest for `version` over what arrived, as the TypeScript wrote
/// it (`version`, `notes`, `pub_date`, `platforms`, in that order).
pub fn render_manifest(
    version: &str,
    artifacts: &[SignedArtifact],
    repo: &str,
    now_iso: &str,
) -> Result<Value, String> {
    let v = version.trim_start_matches('v');
    if !is_release_version(v) {
        return Err(format!(
            "build-update-manifest: --version must be MAJOR.MINOR.PATCH (got {}). The plugin parses it with semver, and a branch name here is a manifest no app can read.",
            serde_json::to_string(version).unwrap_or_default()
        ));
    }
    if v.contains('-') {
        return Err(format!(
            "build-update-manifest: {v} is a prerelease. releases/latest/download never resolves to a prerelease, so no manifest is written for one."
        ));
    }
    if artifacts.is_empty() {
        return Err("build-update-manifest: no signed artifact to describe. Nothing arrived with a .sig beside it, so either the macOS leg did not sign its archive, or its upload failed.".into());
    }
    let mut platforms = Map::new();
    for a in artifacts {
        for key in &a.platform_keys {
            if let Some(existing) = platforms.get(key) {
                return Err(format!(
                    "build-update-manifest: {key} is claimed by two artifacts ({} and the one already at {}). One platform, one file.",
                    a.name,
                    existing["url"].as_str().unwrap_or("")
                ));
            }
            platforms.insert(
                key.clone(),
                json!({ "url": asset_url(repo, v, &a.name), "signature": a.signature }),
            );
        }
    }
    Ok(json!({
        "version": v,
        "notes": format!("Method Reader {v}. Release notes: {}", release_page_url(repo, v)),
        "pub_date": now_iso,
        "platforms": platforms,
    }))
}

/// Point every platform at `<base>/<file name>` instead of the release:
/// the handover rehearsal serves the archive from a local folder. Never
/// used by release.yml.
pub fn rebase(manifest: &mut Value, base: &str) {
    if let Some(platforms) = manifest["platforms"].as_object_mut() {
        for p in platforms.values_mut() {
            let name = p["url"]
                .as_str()
                .and_then(|u| u.rsplit('/').next())
                .unwrap_or_default()
                .to_string();
            p["url"] = Value::String(format!("{}/{name}", base.trim_end_matches('/')));
        }
    }
}

/// `JSON.stringify(manifest, null, 2)` plus a newline.
pub fn to_text(manifest: &Value) -> String {
    let mut s = serde_json::to_string_pretty(manifest).unwrap_or_default();
    s.push('\n');
    s
}

/// Every platform the manifest names must point at a file in `dir` whose
/// signature verifies with `pubkey`, for the manifest's own version, as
/// the Rust app will check it (signed version required).
pub fn verify_dir(manifest: &Value, dir: &Path, pubkey: &str) -> Result<Vec<String>, String> {
    let release = sp_update::Release::from_value(manifest.clone())?;
    let version = release.version.to_string();
    let Some(platforms) = manifest["platforms"].as_object() else {
        return Err("the manifest has no platforms".into());
    };
    let mut ok = Vec::new();
    for (key, p) in platforms {
        let url = p["url"].as_str().unwrap_or_default();
        let name = url.rsplit('/').next().unwrap_or_default();
        let bytes = std::fs::read(dir.join(name))
            .map_err(|e| format!("{key}: {name} is not in {}: {e}", dir.display()))?;
        let sig = p["signature"].as_str().unwrap_or_default();
        sp_update::verify::verify_signature(&bytes, sig, pubkey, &version, true)
            .map_err(|e| format!("{key}: {name}: {e}"))?;
        ok.push(key.clone());
    }
    if ok.is_empty() {
        return Err("the manifest names no platform".into());
    }
    Ok(ok)
}

/// An ISO 8601 UTC time with milliseconds, as JavaScript's toISOString.
pub fn iso_utc(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let ms = unix_ms % 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::{MAC_UPDATE, REPO};

    fn artifact() -> SignedArtifact {
        SignedArtifact {
            name: MAC_UPDATE.into(),
            signature: "c2ln".into(),
            platform_keys: vec!["darwin-aarch64".into()],
        }
    }

    #[test]
    fn the_shape_the_typescript_wrote() {
        let m =
            render_manifest("v0.12.0", &[artifact()], REPO, "2026-10-12T09:00:00.000Z").unwrap();
        assert_eq!(
            to_text(&m),
            r#"{
  "version": "0.12.0",
  "notes": "Method Reader 0.12.0. Release notes: https://github.com/method-production-suite/method-reader/releases/tag/v0.12.0",
  "pub_date": "2026-10-12T09:00:00.000Z",
  "platforms": {
    "darwin-aarch64": {
      "url": "https://github.com/method-production-suite/method-reader/releases/download/v0.12.0/Method-Reader-macOS.app.tar.gz",
      "signature": "c2ln"
    }
  }
}
"#
        );
    }

    #[test]
    fn refusals() {
        let a = artifact();
        assert!(
            render_manifest("main", std::slice::from_ref(&a), REPO, "x")
                .unwrap_err()
                .contains("MAJOR.MINOR.PATCH")
        );
        assert!(
            render_manifest("0.12.0-rc1", std::slice::from_ref(&a), REPO, "x")
                .unwrap_err()
                .contains("prerelease")
        );
        assert!(
            render_manifest("0.12.0", &[], REPO, "x")
                .unwrap_err()
                .contains("no signed artifact")
        );
        assert!(
            render_manifest("0.12.0", &[a.clone(), a], REPO, "x")
                .unwrap_err()
                .contains("claimed by two")
        );
    }

    #[test]
    fn a_folder_with_a_stray_or_orphan_signature_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Method-Reader-macOS.dmg.sig"), "x").unwrap();
        assert!(
            collect_signed_artifacts(dir.path())
                .unwrap_err()
                .contains("no such artifact")
        );
        std::fs::write(dir.path().join("Method-Reader-macOS.dmg"), "x").unwrap();
        assert!(
            collect_signed_artifacts(dir.path())
                .unwrap_err()
                .contains("no row in UPDATER_KINDS")
        );
    }

    #[test]
    fn times_as_javascript_writes_them() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_utc(1_767_225_600_123), "2026-01-01T00:00:00.123Z");
        assert_eq!(iso_utc(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
