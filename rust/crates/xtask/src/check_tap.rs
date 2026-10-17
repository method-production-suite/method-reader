//! `cargo xtask check-tap [version]`: was tools/check-tap.sh.
//!
//! Exit 0 if ssandweiss/homebrew-tap serves `version` (default: the newest
//! published non-prerelease) with checksums matching that release's
//! assets. Exit 1 otherwise, having said which part is wrong.
//!
//! Reads the tap FROM GITHUB, never from a checkout. That distinction is
//! the entire point: the tap sat at 0.3.0 for five releases while a local
//! checkout showed 0.4.2, because the edits were never committed.
//!
//! `METHOD_READER_REPO` and `METHOD_READER_TAP` name the two repositories,
//! as the script read them; the network goes through check_latest's `http`,
//! whose bases the environment can move (tests point it at 127.0.0.1).
//!
//! The tap serves `method-reader` (cask and formula); the 0.12.x name,
//! `screepub`, is carried across by `cask_renames.json` and
//! `formula_renames.json`, which must say so.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::check_latest::http::{self, Bases, Fetcher, ReleaseAssets};

pub const ARM: &str = sp_release::names::CLI_MAC_ARM64;
/// The Intel tarball of the releases through the identifier release, which
/// predate the rename and so carry the old name.
pub const INTEL: &str = sp_release::check_latest::INTEL_CLI_ASSET;

/// The tap's files.
pub const CASK: &str = "Casks/method-reader.rb";
pub const FORMULA: &str = "Formula/method-reader.rb";
pub const CASK_RENAMES: &str = "cask_renames.json";
pub const FORMULA_RENAMES: &str = "formula_renames.json";
/// The 0.12.x files, which the rename replaced.
pub const LEGACY_CASK: &str = "Casks/screepub.rb";
pub const LEGACY_FORMULA: &str = "Formula/screepub.rb";

/// What both renames files hold: the old token to the new one.
pub fn renames_json() -> String {
    format!(
        "{{\n  \"{}\": \"{}\"\n}}\n",
        sp_release::names::LEGACY_PACKAGE,
        sp_release::names::PACKAGE
    )
}

/// Whether a renames file maps screepub to method-reader (whatever else
/// it maps, and however it is spaced).
pub fn renames_ok(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| {
        v.get(sp_release::names::LEGACY_PACKAGE)
            .and_then(|n| n.as_str())
            == Some(sp_release::names::PACKAGE)
    })
}

/// METHOD_READER_REPO, else the release repository (sp-release's REPO).
pub fn release_repo() -> String {
    std::env::var("METHOD_READER_REPO")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| sp_release::names::REPO.to_string())
}

fn tap_repo() -> String {
    std::env::var("METHOD_READER_TAP")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "ssandweiss/homebrew-tap".to_string())
}

/// Sixty-four lowercase hex digits: a digest's SHAPE, not merely that
/// something came back (GitHub's 404 body is not a checksum).
pub fn is_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn has_line_starting(text: &str, prefix: &str) -> bool {
    text.split('\n').any(|l| l.starts_with(prefix))
}

static PINNED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.*/releases/download/v([^/]*)/.*$").expect("regex"));

/// The versions a formula's download urls pin, sorted and unique.
pub fn formula_versions(formula: &str) -> BTreeSet<String> {
    formula
        .split('\n')
        .filter_map(|l| PINNED.captures(l).map(|c| c[1].to_string()))
        .collect()
}

/// The check, with the network injected: (stdout lines, exit code).
/// Problems are `::error::` lines on stdout, as the script printed them.
pub fn check_tap(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    tap: &str,
    version: Option<&str>,
) -> (Vec<String>, Vec<String>, u8) {
    let mut out = Vec::new();
    let mut err = Vec::new();

    let version = match version.filter(|v| !v.is_empty()) {
        Some(v) => v.to_string(),
        None => match http::newest_release_tag(fetcher, bases, repo) {
            Ok(t) => t.unwrap_or_default(),
            Err(e) => {
                err.push(format!(
                    "check-tap: could not read the newest release of {repo}: {e}"
                ));
                return (out, err, 1);
            }
        },
    };
    let version = version.strip_prefix('v').unwrap_or(&version).to_string();
    if version.is_empty() {
        err.push("check-tap: no published, non-prerelease release to compare against".to_string());
        return (out, err, 1);
    }

    let mut read = |path: &str| http::repo_file(fetcher, bases, tap, path);
    let cask = match read(CASK) {
        Ok(t) => t,
        Err(e) => {
            err.push(format!("check-tap: could not read {CASK} from {tap}: {e}"));
            return (out, err, 1);
        }
    };
    let formula = match read(FORMULA) {
        Ok(t) => t,
        Err(e) => {
            err.push(format!(
                "check-tap: could not read {FORMULA} from {tap}: {e}"
            ));
            return (out, err, 1);
        }
    };
    // A missing file reads as empty, and so as wrong.
    let renames: Vec<(&str, String)> = [CASK_RENAMES, FORMULA_RENAMES]
        .into_iter()
        .map(|f| (f, read(f).unwrap_or_default()))
        .collect();

    // Checksums, from the digests GitHub reports per asset, so nothing is
    // downloaded. A release that cannot be read has no digests at all.
    let release = http::release_assets(fetcher, bases, repo, &format!("v{version}")).unwrap_or(
        ReleaseAssets {
            tag: String::new(),
            assets: Vec::new(),
        },
    );

    // Intel: keyed on what the RELEASE carries, never on a version number,
    // so each release is judged as it shipped.
    let intel_era = is_sha(&release.digest(INTEL));

    // The CASK was retired at the identifier release: through it
    // DEPRECATED, from the release after it DISABLED.
    let (cask_want, cask_why) = if intel_era {
        (
            "deprecate",
            format!("it still carries {INTEL}, and the cask may still install the last Swift app"),
        )
    } else {
        (
            "disable",
            format!(
                "it carries no {INTEL}, and the last Swift app's updater would offer an Intel Mac an app it cannot open"
            ),
        )
    };
    let pins = formula_versions(&formula);
    let pins_text = pins.iter().cloned().collect::<Vec<_>>().join(" ");

    out.push(format!("newest release: {version}"));
    if has_line_starting(&cask, "  disable! ") {
        out.push("cask:           retired (disabled)".to_string());
    } else if has_line_starting(&cask, "  deprecate! ") {
        out.push("cask:           retired (deprecated)".to_string());
    } else {
        out.push("cask:           NOT retired".to_string());
    }
    out.push(format!("formula pins:   {pins_text}"));

    let mut fail = false;
    for (file, text) in &renames {
        if !renames_ok(text) {
            out.push(format!(
                "::error::{file} does not map screepub to method-reader, so a Mac with the old name installed is not carried across"
            ));
            fail = true;
        }
    }
    if !has_line_starting(&cask, &format!("  {cask_want}! ")) {
        out.push(format!(
            "::error::the frozen cask must say {cask_want}! for release v{version} ({cask_why}), and it does not"
        ));
        fail = true;
    }
    // Homebrew asks deprecated? before disabled?, so a cask carrying both
    // lines still installs.
    if !intel_era && has_line_starting(&cask, "  deprecate! ") {
        out.push(format!(
            "::error::the frozen cask still has a deprecate! line, and a cask with deprecate! still installs whatever else it says; release v{version} wants disable! alone"
        ));
        fail = true;
    }
    if pins.len() != 1 {
        out.push(format!(
            "::error::the formula's download urls disagree: {pins_text}"
        ));
        fail = true;
    } else if pins.first().map(String::as_str) != Some(version.as_str()) {
        out.push(format!(
            "::error::the formula pins {pins_text} but the newest release is {version}"
        ));
        fail = true;
    }

    let mut check_sha = |asset: &str, human: &str, text: &str, out: &mut Vec<String>| {
        let want = release.digest(asset);
        if !is_sha(&want) {
            out.push(format!(
                "::error::release v{version} has no usable digest for {asset}"
            ));
            fail = true;
        } else if !text.contains(&want) {
            out.push(format!(
                "::error::{human} does not carry the SHA-256 of {asset} ({want})"
            ));
            fail = true;
        }
    };
    check_sha(ARM, "the formula", &formula, &mut out);

    // A release that still has the Intel tarball keeps its two-arch formula
    // and both digests; one without it must name no Intel url and say
    // `depends_on arch: :arm64`.
    if intel_era {
        check_sha(INTEL, "the formula", &formula, &mut out);
    } else {
        if formula.contains(INTEL) {
            out.push(format!(
                "::error::the formula still serves {INTEL}, which release v{version} does not carry (Apple silicon only)"
            ));
            fail = true;
        }
        if !formula
            .split('\n')
            .any(|l| l == "  depends_on arch: :arm64")
        {
            out.push(
                "::error::the formula does not say depends_on arch: :arm64, so brew would install an Apple silicon binary on an Intel Mac"
                    .to_string(),
            );
            fail = true;
        }
    }

    if fail {
        out.push(format!(
            "::notice::Bump https://github.com/{tap}: `cargo xtask bump-tap` does it (and disables the retired cask), and the release run's summary carries the SHA. See docs/release-secrets.md."
        ));
        return (out, err, 1);
    }
    out.push(format!(
        "The tap serves {version}, with matching checksums."
    ));
    (out, err, 0)
}

/// `cargo xtask check-tap --help`.
pub const USAGE: &str = "\
usage: cargo xtask check-tap [version]

Does the Homebrew tap serve the latest release (or <version>), with the
release's own checksums, is the old cask still retired, and do the tap's
renames carry screepub to method-reader? Exits 0 when it does, 1 with what
is wrong. METHOD_READER_REPO and METHOD_READER_TAP name the two repositories.
";

/// `cargo xtask check-tap`: its exit code.
pub fn main(args: &[String]) -> u8 {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return 0;
    }
    if let Some(flag) = args.iter().find(|a| a.starts_with('-')) {
        eprint!("check-tap: unknown option {flag}\n\n{USAGE}");
        return 2;
    }
    if args.len() > 1 {
        eprint!("check-tap: one version at most\n\n{USAGE}");
        return 2;
    }
    let bases = Bases::from_env();
    let mut fetcher = http::real_fetcher(&bases);
    let (out, err, code) = check_tap(
        &mut fetcher,
        &bases,
        &release_repo(),
        &tap_repo(),
        args.first().map(String::as_str),
    );
    for line in out {
        println!("{line}");
    }
    for line in err {
        eprintln!("{line}");
    }
    code
}
