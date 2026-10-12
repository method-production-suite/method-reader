//! latest.json, the static manifest the in-app updater reads: was
//! tools/build-update-manifest.ts.
//!
//! ```text
//! cargo xtask build-update-manifest --dir arrivals --version v0.6.1 --out arrivals/latest.json
//! ```
//!
//! Written over the directory every bundle leg's artifacts were downloaded
//! into, so it describes the files actually published beside it. Driven by
//! [`UPDATER_KINDS`]: a `.sig` whose artifact no row recognises is an
//! ERROR, not a skip. The same file serves the Tauri window and the native
//! one: the native window's archive keeps the Tauri archive's name and
//! platform key, which is how a 0.11.x window finds it.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::kinds::{SIGNATURE_EXT, UPDATER_KINDS, arch_of, platform_keys};
use super::signature::parse_signature_box;
use super::util::{FlagSpec, absolute, is_release_version, parse_flags, strip_v};

pub const RELEASE_REPO: &str = "ssandweiss/screepub";
pub const MANIFEST_NAME: &str = "latest.json";

/// Where the app looks: GitHub's redirect to the newest non-prerelease.
pub fn manifest_endpoint(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest/download/{MANIFEST_NAME}")
}

pub fn asset_url(repo: &str, version: &str, name: &str) -> String {
    format!("https://github.com/{repo}/releases/download/v{version}/{name}")
}

pub fn release_page_url(repo: &str, version: &str) -> String {
    format!("https://github.com/{repo}/releases/tag/v{version}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedArtifact {
    /// The published filename, also the asset name on the release.
    pub name: String,
    /// The `.sig` file's contents, verbatim.
    pub signature: String,
    pub platforms: Vec<String>,
}

/// Every `<artifact>.sig` in `dir` with its artifact beside it, sorted.
pub fn collect_signed_artifacts(dir: &Path) -> Result<Vec<SignedArtifact>, String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Err(format!(
            "build-update-manifest: no such directory: {}",
            dir.display()
        ));
    };
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
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
        let Some((kind, arch)) = UPDATER_KINDS
            .iter()
            .find_map(|k| arch_of(k, name).map(|a| (k, a)))
        else {
            return Err(format!(
                "build-update-manifest: {sig_name} signs {name}, and no row in UPDATER_KINDS (xtask packaging/kinds.rs) maps that artifact to a platform key. Add the row, or stop signing it; a signed artifact no platform can be offered is a manifest that lies by omission."
            ));
        };
        let signature = std::fs::read_to_string(dir.join(sig_name))
            .map_err(|e| format!("build-update-manifest: cannot read {sig_name}: {e}"))?;
        parse_signature_box(&signature).map_err(|e| {
            format!("build-update-manifest: {sig_name} is not a minisign signature: {e}")
        })?;
        out.push(SignedArtifact {
            name: name.to_string(),
            signature,
            platforms: platform_keys(kind, arch)
                .into_iter()
                .map(str::to_string)
                .collect(),
        });
    }
    Ok(out)
}

/// `Date.prototype.toISOString` of a Unix time in milliseconds.
pub fn iso8601(unix_ms: i64) -> String {
    let days = unix_ms.div_euclid(86_400_000);
    let ms_of_day = unix_ms.rem_euclid(86_400_000);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1000 % 60,
        ms_of_day % 1000
    )
}

/// The manifest for `version` over what arrived. Refuses a prerelease, an
/// empty set and a platform claimed twice.
pub fn render_manifest(
    version: &str,
    artifacts: &[SignedArtifact],
    repo: &str,
    now_ms: i64,
) -> Result<Value, String> {
    let v = strip_v(version);
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
        return Err("build-update-manifest: no signed artifact to describe. Nothing arrived with a .sig beside it, so either the macOS leg did not run with --updater, or its upload failed.".into());
    }
    let mut platforms = Map::new();
    for a in artifacts {
        for key in &a.platforms {
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
        "notes": format!("Screepub {v}. Release notes: {}", release_page_url(repo, v)),
        "pub_date": iso8601(now_ms),
        "platforms": platforms,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestArgs {
    pub dir: PathBuf,
    pub version: String,
    pub out: PathBuf,
    pub repo: String,
}

pub fn parse_manifest_args(args: &[String]) -> Result<ManifestArgs, String> {
    let f = parse_flags(
        args,
        &[
            FlagSpec {
                name: "dir",
                value: true,
            },
            FlagSpec {
                name: "version",
                value: true,
            },
            FlagSpec {
                name: "out",
                value: true,
            },
            FlagSpec {
                name: "repo",
                value: true,
            },
        ],
    )?;
    let dir = f
        .get("dir")
        .ok_or("build-update-manifest: --dir <arrivals> is required")?;
    let version = f
        .get("version")
        .ok_or("build-update-manifest: --version <tag or version> is required")?;
    let out = f
        .get("out")
        .ok_or("build-update-manifest: --out <path to latest.json> is required")?;
    Ok(ManifestArgs {
        dir: absolute(dir),
        version: version.clone(),
        out: absolute(out),
        repo: f
            .get("repo")
            .cloned()
            .unwrap_or_else(|| RELEASE_REPO.to_string()),
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `cargo xtask build-update-manifest`.
pub fn main(args: &[String]) -> u8 {
    let run = || -> Result<(), String> {
        let a = parse_manifest_args(args)?;
        let artifacts = collect_signed_artifacts(&a.dir)?;
        let manifest = render_manifest(&a.version, &artifacts, &a.repo, now_ms())?;
        if let Some(parent) = a.out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(&a.out, format!("{text}\n"))
            .map_err(|e| format!("cannot write {}: {e}", a.out.display()))?;
        println!(
            "wrote {} for {}:",
            a.out.display(),
            manifest["version"].as_str().unwrap_or("")
        );
        if let Some(p) = manifest["platforms"].as_object() {
            for (key, entry) in p {
                let url = entry["url"].as_str().unwrap_or("");
                println!(
                    "   {key:<16} {}",
                    &url[url.rfind('/').map_or(0, |i| i + 1)..]
                );
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
