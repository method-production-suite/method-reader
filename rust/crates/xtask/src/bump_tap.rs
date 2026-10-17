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
//! is written. `METHOD_READER_REPO` names the release repository; the
//! network goes through check_latest's `http`, whose bases the environment
//! can move (tests point it at 127.0.0.1).
//!
//! The tap's files are `Casks/method-reader.rb` and
//! `Formula/method-reader.rb`. Up to 0.12.x they were `screepub.rb`; the
//! rename is Homebrew's own tap migration, `cask_renames.json` and
//! `formula_renames.json` at the tap's root mapping `screepub` to
//! `method-reader`, which bump-tap writes whole every time. A tap that
//! still has the old files has them moved to the new names (the cask's
//! token renamed, nothing else in it changed) once the release has been
//! read; one that has both is refused, since Homebrew only migrates a
//! renamed file that is gone.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::check_latest::http::{self, Bases, Fetcher};
use crate::check_tap::{
    ARM, CASK, CASK_RENAMES, FORMULA, FORMULA_RENAMES, INTEL, LEGACY_CASK, LEGACY_FORMULA, is_sha,
    release_repo, renames_json,
};

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

/// The cask under its new token: the retired cask's text with its first
/// line `cask "screepub" do` made `cask "method-reader" do`, and nothing
/// else changed (it is never bumped; the app updates itself). Already
/// renamed, it is left as it is.
pub fn rename_cask(cask: &str) -> String {
    let old = "cask \"screepub\" do\n";
    match cask.strip_prefix(old) {
        Some(rest) => format!("cask \"method-reader\" do\n{rest}"),
        None => cask.to_string(),
    }
}

/// The formula, Apple silicon only, written WHOLE: a pure function of the
/// version and the digest, so a second run is byte-identical. No version
/// stanza (brew reads it from the url), and `depends_on arch:` before
/// `depends_on :macos` (brew style sorts dependencies by name).
pub fn formula(repo: &str, version: &str, arm_sha: &str) -> String {
    format!(
        r##"class MethodReader < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/{repo}"
  # Written whole by `cargo xtask bump-tap` in the method-reader repository
  # at every release: edit it there, not here. Apple silicon only since the
  # release after 0.9.0; Intel Macs are no longer supported. Called
  # screepub up to 0.12.x (formula_renames.json carries installs across).
  url "https://github.com/{repo}/releases/download/v{version}/{ARM}"
  sha256 "{arm_sha}"
  license all_of: [{{ any_of: ["MIT", "Apache-2.0"] }}, "GPL-3.0-only"]

  livecheck do
    url "https://github.com/{repo}/releases/latest"
    strategy :github_latest
  end

  # A signed, notarized Mach-O built by the project's release workflow.
  # There is no source build here, so no bottles.
  depends_on arch: :arm64
  depends_on :macos

  def install
    # The tarball holds the tool and its old name: screepub says once that
    # it is now called method-reader, then does exactly what it does.
    bin.install "method-reader", "screepub"
  end

  test do
    # Pins the download to the version this formula claims: a stale url or a
    # mismatched asset fails here rather than at somebody's prompt.
    assert_match version.to_s, shell_output("#{{bin}}/method-reader --version")
    assert_match "screenplay", shell_output("#{{bin}}/method-reader --help")
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
    if cask_lines.first() != Some(&"cask \"method-reader\" do") {
        return Err("cask is not under the token method-reader".to_string());
    }
    if !formula.starts_with("class MethodReader < Formula\n") {
        return Err("formula is not class MethodReader".to_string());
    }
    if !formula.contains(&format!("/download/v{version}/{ARM}")) {
        return Err("formula arm64 url did not take".to_string());
    }
    if !formula.contains(&format!("\"{arm_sha}\"")) {
        return Err("formula arm64 sha256 did not take".to_string());
    }
    if !formula_lines.contains(&"  depends_on arch: :arm64") {
        return Err("formula does not declare arm64".to_string());
    }
    if formula.contains(INTEL) {
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
    let cask_path = tap.join(CASK);
    let formula_path = tap.join(FORMULA);
    // Each file under its new name, or (a tap from before the rename) its
    // old one, which is moved once the release has been read. Never both:
    // Homebrew only migrates a renamed file that is gone.
    let mut moves: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for (new, old) in [(CASK, LEGACY_CASK), (FORMULA, LEGACY_FORMULA)] {
        let (new_p, old_p) = (tap.join(new), tap.join(old));
        match (new_p.is_file(), old_p.exists()) {
            (true, false) => {}
            (false, true) if old_p.is_file() => moves.push((old_p, new_p)),
            (true, true) => {
                err.push(format!(
                    "bump-tap: {old} is still in the tap beside {new}; Homebrew only migrates a renamed file that is gone. Remove it (git rm) and run again."
                ));
                return (out, err, 1);
            }
            _ => {
                err.push(format!("not a tap checkout: {} missing", new_p.display()));
                return (out, err, 1);
            }
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
    // The rename, for a tap that still has the 0.12.x names.
    for (old, new) in &moves {
        if let Err(e) = std::fs::rename(old, new) {
            err.push(format!(
                "bump-tap: cannot move {} to {}: {e}",
                old.display(),
                new.display()
            ));
            return (out, err, 1);
        }
    }
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
    let new_cask = retire_cask(&rename_cask(&cask));
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
    for renames in [CASK_RENAMES, FORMULA_RENAMES] {
        if let Err(e) = write(&tap.join(renames), &renames_json()) {
            err.push(e);
            return (out, err, 1);
        }
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
        "  renames screepub -> method-reader (cask and formula)".to_string(),
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
