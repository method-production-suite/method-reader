//! `cargo xtask bump-tap <version> <tap-checkout>`: was tools/bump-tap.sh.
//!
//! Rewrite a homebrew-tap checkout to serve <version>. Edits only; it does
//! not commit, so it can be run locally to see the diff, and release.yml's
//! `tap` job does the committing. Its most useful property: run it at the
//! version the tap already serves and the diff is EMPTY.
//!
//! SHAs come from the digests GitHub reports for the release assets, so
//! nothing is downloaded and nothing is re-hashed. The asset list is read
//! ONCE, and every decision (the digest, and whether the release still
//! serves Intel) comes from that one answer; if it cannot be read, nothing
//! is written. `SCREEPUB_REPO` names the release repository; the network
//! goes through check_latest's `http`, whose bases the environment can
//! move (tests point it at 127.0.0.1).

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::check_latest::http::{self, Bases, Fetcher};
use crate::check_tap::{ARM, INTEL, is_sha, release_repo};

/// The day Intel was dropped: the disable! date of a cask that had no
/// deprecate! of its own. Fixed, so a second run writes the same bytes,
/// and already past everywhere (Homebrew compares it with the user's
/// LOCAL day: a later date only deprecates, and still installs).
pub const INTEL_DROPPED: &str = "2026-10-01";

/// The cask's retirement line.
pub fn disable_line(date: &str) -> String {
    format!(
        "  disable! date: \"{date}\", because: \"is now downloaded from the releases page and updates itself\""
    )
}

/// Lines as awk reads them: a final newline ends the last line rather
/// than starting an empty one.
fn lines(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        v.pop();
    }
    v
}

/// Lines back to text, every one ending in a newline, as awk prints them.
fn unlines(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").expect("regex"));

/// The cask, retired: DISABLED from the release after the identifier
/// release, because the Swift app's updater would offer an Intel Mac an
/// Apple-silicon-only app. A deprecate! line is replaced where it stands,
/// keeping its own date; with neither, the line goes after the livecheck
/// block (Homebrew's stanza order); a cask already disabled is left alone.
pub fn retire_cask(cask: &str) -> String {
    let ls = lines(cask);
    if ls.iter().any(|l| l.starts_with("  disable! ")) {
        return cask.to_string();
    }
    let deprecated_on = ls
        .iter()
        .find_map(|l| l.strip_prefix("  deprecate! date: \""))
        .map(|rest| rest.split('"').next().unwrap_or("").to_string())
        .filter(|d| DATE.is_match(d));
    let mut out: Vec<String> = Vec::new();
    if ls.iter().any(|l| l.starts_with("  deprecate! ")) {
        let line = disable_line(deprecated_on.as_deref().unwrap_or(INTEL_DROPPED));
        for l in ls {
            out.push(if l.starts_with("  deprecate! ") {
                line.clone()
            } else {
                l.to_string()
            });
        }
    } else {
        let line = disable_line(INTEL_DROPPED);
        let (mut in_live, mut done) = (false, false);
        for l in ls {
            out.push(l.to_string());
            if l.starts_with("  livecheck do") {
                in_live = true;
            }
            if in_live && l == "  end" && !done {
                out.push(String::new());
                out.push(line.clone());
                done = true;
                in_live = false;
            }
        }
    }
    unlines(&out)
}

/// The formula, Apple silicon only, written WHOLE: a pure function of the
/// version and the digest, so a second run is byte-identical. No version
/// stanza (brew reads it from the url), and `depends_on arch:` before
/// `depends_on :macos` (brew style sorts dependencies by name).
pub fn formula(repo: &str, version: &str, arm_sha: &str) -> String {
    format!(
        r##"class Screepub < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/{repo}"
  # Written whole by tools/bump-tap.sh in the screepub repository at every
  # release: edit it there, not here. Apple silicon only since the release
  # after 0.9.0; Intel Macs are no longer supported.
  url "https://github.com/{repo}/releases/download/v{version}/screepub-cli-macos-arm64.tar.gz"
  sha256 "{arm_sha}"
  license "AGPL-3.0-or-later"

  livecheck do
    url "https://github.com/{repo}/releases/latest"
    strategy :github_latest
  end

  # A signed, notarized Mach-O built by the project's release workflow.
  # There is no source build here, so no bottles.
  depends_on arch: :arm64
  depends_on :macos

  def install
    # The tarball holds a single file, already named plain screepub.
    bin.install "screepub"
  end

  test do
    # Pins the download to the version this formula claims: a stale url or a
    # mismatched asset fails here rather than at somebody's prompt.
    assert_match version.to_s, shell_output("#{{bin}}/screepub --version")
    assert_match "screenplay", shell_output("#{{bin}}/screepub --help")
  end
end
"##
    )
}

static DISABLED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^  disable! date: "[0-9-]*", because: "#).expect("regex"));
static ANY_DOWNLOAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/download/v[^/]*/").expect("regex"));

/// Prove the edits landed: the result must fail here rather than commit
/// cleanly and serve the OLD version.
fn verify(cask: &str, formula: &str, version: &str, arm_sha: &str) -> Result<(), String> {
    let cask_lines = lines(cask);
    let formula_lines = lines(formula);
    if !cask_lines.iter().any(|l| DISABLED.is_match(l)) {
        return Err(
            "cask disable! did not take (no livecheck block to insert it after?)".to_string(),
        );
    }
    if cask_lines.iter().any(|l| l.starts_with("  deprecate! ")) {
        return Err("cask still carries deprecate! beside disable!".to_string());
    }
    if !formula.contains(&format!(
        "/download/v{version}/screepub-cli-macos-arm64.tar.gz"
    )) {
        return Err("formula arm64 url did not take".to_string());
    }
    if !formula.contains(&format!("\"{arm_sha}\"")) {
        return Err("formula arm64 sha256 did not take".to_string());
    }
    if !formula_lines.contains(&"  depends_on arch: :arm64") {
        return Err("formula does not declare arm64".to_string());
    }
    if formula.contains("screepub-cli-macos-x64") {
        return Err("formula still names the Intel tarball".to_string());
    }
    let this = format!("/download/v{version}/");
    if formula_lines
        .iter()
        .any(|l| ANY_DOWNLOAD.is_match(l) && !l.contains(&this))
    {
        return Err("formula still carries a url for some other version".to_string());
    }
    Ok(())
}

/// The bump, with the network injected: (stdout lines, stderr lines, exit).
pub fn bump_tap(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    version: &str,
    tap: &Path,
) -> (Vec<String>, Vec<String>, u8) {
    let out = Vec::new();
    let mut err = Vec::new();
    let version = version.strip_prefix('v').unwrap_or(version);
    let cask_path = tap.join("Casks/screepub.rb");
    let formula_path = tap.join("Formula/screepub.rb");
    for f in [&cask_path, &formula_path] {
        if !f.is_file() {
            err.push(format!("not a tap checkout: {} missing", f.display()));
            return (out, err, 1);
        }
    }

    // The release's asset list, read ONCE. Fail closed.
    let Ok(release) = http::release_assets(fetcher, bases, repo, &format!("v{version}")) else {
        err.push(format!(
            "bump-tap: could not read the assets of release v{version}; nothing written"
        ));
        err.push(format!(
            "bump-tap: is release v{version} published? Try again if GitHub was having a moment."
        ));
        return (out, err, 1);
    };
    let arm_sha = release.digest(ARM);
    if !is_sha(&arm_sha) {
        err.push(format!(
            "bump-tap: no usable sha256 for {ARM} at v{version}"
        ));
        err.push(format!(
            "bump-tap: got: {}",
            if arm_sha.is_empty() {
                "<empty>"
            } else {
                &arm_sha
            }
        ));
        err.push(format!(
            "bump-tap: is release v{version} published, with its assets uploaded?"
        ));
        return (out, err, 1);
    }

    // A release that still carries the Intel tarball is served by a
    // two-arch formula, which ITS OWN copy of this tool writes. Refused,
    // before anything is written.
    if release.has(INTEL) {
        err.push(format!(
            "bump-tap: release v{version} still carries {INTEL}, so its tap serves Intel Macs too."
        ));
        err.push(
            "bump-tap: this script writes the Apple-silicon-only tap of the releases after it."
                .to_string(),
        );
        err.push(format!(
            "bump-tap: bump v{version} with its own script: git show v{version}:tools/bump-tap.sh > /tmp/bump-tap.sh"
        ));
        return (out, err, 1);
    }

    let write = |path: &Path, text: &str| -> Result<(), String> {
        std::fs::write(path, text)
            .map_err(|e| format!("bump-tap: cannot write {}: {e}", path.display()))
    };
    let cask = match std::fs::read_to_string(&cask_path) {
        Ok(t) => t,
        Err(e) => {
            err.push(format!(
                "bump-tap: cannot read {}: {e}",
                cask_path.display()
            ));
            return (out, err, 1);
        }
    };
    let new_cask = retire_cask(&cask);
    if new_cask != cask
        && let Err(e) = write(&cask_path, &new_cask)
    {
        err.push(e);
        return (out, err, 1);
    }
    let new_formula = formula(repo, version, &arm_sha);
    if let Err(e) = write(&formula_path, &new_formula) {
        err.push(e);
        return (out, err, 1);
    }

    let cask_now = std::fs::read_to_string(&cask_path).unwrap_or_default();
    let formula_now = std::fs::read_to_string(&formula_path).unwrap_or_default();
    if let Err(e) = verify(&cask_now, &formula_now, version, &arm_sha) {
        err.push(format!("bump-tap: {e}"));
        return (out, err, 1);
    }

    let out = vec![
        format!("tap rewritten to {version}"),
        "  cask    retired (disabled, never bumped)".to_string(),
        format!("  formula arm64 {arm_sha} (Apple silicon only)"),
    ];
    (out, err, 0)
}

/// `cargo xtask bump-tap`: its exit code.
pub fn main(args: &[String]) -> u8 {
    let (Some(version), Some(tap)) = (
        args.first().filter(|a| !a.is_empty()),
        args.get(1).filter(|a| !a.is_empty()),
    ) else {
        eprintln!("usage: cargo xtask bump-tap <version> <tap-checkout>");
        return 2;
    };
    let bases = Bases::from_env();
    let mut fetcher = http::real_fetcher(&bases);
    let (out, err, code) = bump_tap(
        &mut fetcher,
        &bases,
        &release_repo(),
        version,
        Path::new(tap),
    );
    for line in out {
        println!("{line}");
    }
    for line in err {
        eprintln!("{line}");
    }
    code
}
