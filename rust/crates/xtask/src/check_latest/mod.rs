//! `cargo xtask check-latest [--version vX.Y.Z] [--repo owner/name]`: was
//! tools/check-latest.ts. Does the PUBLISHED latest.json describe the
//! newest release?
//!
//! The judgement and the fetching order are sp-release's
//! ([`sp_release::check_latest`]), the one implementation `sp-release
//! check-latest` runs in release.yml and tap-freshness.yml. This command
//! keeps the old name and flags for the /release skill, checks against the
//! key the shipped app trusts ([`sp_update::PUBKEY`]; until the Rust cutover
//! it read the Tauri window's tauri.conf.json), and fetches through
//! [`http`], whose bases the environment can move so tests point it at a
//! fake GitHub.
//!
//! Exit 0 when the manifest the app will fetch names the newest release,
//! points every platform at an asset that release carries, pastes the
//! exact contents of each asset's .sig, and is signed by the key the app
//! is configured to trust. Exit 1 otherwise, having said which part is
//! wrong. Every problem is reported, not only the first.

pub mod http;

use std::collections::BTreeMap;

use http::{Bases, Fetcher};
pub use sp_release::check_latest::{
    Asset as ReleaseAsset, INTEL_CLI_ASSET, Release, manifest_endpoint, required_platforms,
};
pub use sp_release::names::{MANIFEST as MANIFEST_NAME, REPO as RELEASE_REPO};

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

impl From<Result<(String, Vec<String>), Vec<String>>> for Verdict {
    fn from(r: Result<(String, Vec<String>), Vec<String>>) -> Self {
        match r {
            Ok((version, platforms)) => Verdict::Ok { version, platforms },
            Err(p) => Verdict::Problems(p),
        }
    }
}

/// Which key a `.sig` file's contents name ([`sp_release::boxes`]).
pub fn parse_signature_box(encoded: &str) -> Result<String, String> {
    sp_release::boxes::parse_signature_box(encoded).map(|b| b.key_id)
}

/// Which key a public key (as the app's config carries it) is.
pub fn parse_public_key_box(encoded: &str) -> Result<String, String> {
    sp_release::boxes::parse_public_key_box(encoded).map(|b| b.key_id)
}

/// [`sp_release::check_latest::judge_manifest`], as a [`Verdict`].
pub fn judge_manifest(
    manifest_text: &str,
    release: &Release,
    sig_texts: &BTreeMap<String, String>,
    pubkey: &str,
) -> Verdict {
    sp_release::check_latest::judge_manifest(manifest_text, release, sig_texts, pubkey).into()
}

/// Fetch the release, the manifest and every signature, then judge:
/// [`sp_release::check_latest::check_with`] through this module's fetcher.
pub fn check_latest(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    pubkey: &str,
    version: Option<&str>,
) -> Verdict {
    let mut fetch = |url: &str| fetcher(url).map(|f| (f.status, f.text.into_bytes()));
    sp_release::check_latest::check_with(
        &mut fetch, &bases.api, &bases.web, repo, pubkey, version, false,
    )
    .into()
}

/// What the shipped app trusts: [`sp_update::PUBKEY`], the one home of
/// the updater's public key.
pub fn trusted_pubkey() -> &'static str {
    sp_update::PUBKEY
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
    let pubkey = trusted_pubkey();
    let mut fetcher = http::real_fetcher(&bases);
    match check_latest(&mut fetcher, &bases, &repo, pubkey, version.as_deref()) {
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
