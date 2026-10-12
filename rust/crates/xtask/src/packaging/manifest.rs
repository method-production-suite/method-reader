//! latest.json, the static manifest the in-app updater reads: was
//! tools/build-update-manifest.ts.
//!
//! ```text
//! cargo xtask build-update-manifest --dir arrivals --version v0.6.1 --out arrivals/latest.json
//! ```
//!
//! The manifest is written by [`sp_release::manifest`], the one writer
//! release.yml runs (`sp-release manifest`); this command keeps the old
//! name and flags for the /release skill and is a thin caller of it. A
//! `.sig` whose artifact no row of sp-release's UPDATER_KINDS recognises is
//! an ERROR, not a skip.

use std::path::PathBuf;

use serde_json::Value;

use super::util::{FlagSpec, absolute, parse_flags};
pub use sp_release::manifest::{SignedArtifact, collect_signed_artifacts};
pub use sp_release::names::{
    MANIFEST as MANIFEST_NAME, REPO as RELEASE_REPO, asset_url, release_page_url,
};

/// Where the app looks: GitHub's redirect to the newest non-prerelease.
pub fn manifest_endpoint(repo: &str) -> String {
    sp_release::check_latest::manifest_endpoint(sp_release::check_latest::GITHUB_WEB, repo)
}

/// `Date.prototype.toISOString` of a Unix time in milliseconds.
pub fn iso8601(unix_ms: i64) -> String {
    sp_release::manifest::iso_utc(u64::try_from(unix_ms).unwrap_or(0))
}

/// The manifest for `version` over what arrived, dated `now_ms`.
pub fn render_manifest(
    version: &str,
    artifacts: &[SignedArtifact],
    repo: &str,
    now_ms: i64,
) -> Result<Value, String> {
    sp_release::manifest::render_manifest(version, artifacts, repo, &iso8601(now_ms))
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
    if super::util::wants_help(
        args,
        "usage: cargo xtask build-update-manifest --dir <arrivals> --version <tag or version> --out <latest.json> [--repo <owner/name>]",
    ) {
        return 0;
    }
    let run = || -> Result<(), String> {
        let a = parse_manifest_args(args)?;
        let artifacts = collect_signed_artifacts(&a.dir)?;
        let manifest = render_manifest(&a.version, &artifacts, &a.repo, now_ms())?;
        if let Some(parent) = a.out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&a.out, sp_release::manifest::to_text(&manifest))
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
