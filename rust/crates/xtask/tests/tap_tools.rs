//! `cargo xtask bump-tap` and `cargo xtask check-tap`, EXECUTED against a
//! fake tap and a fake GitHub: ported from tests/tap-scripts.test.ts.
//!
//! The formula changed SHAPE at the release after the identifier release:
//! through it, two tarballs from an `if Hardware::CPU.arm?` block; after
//! it, one, and `depends_on arch: :arm64`. bump-tap has to turn the
//! published two-arch formula into that, and check-tap has to judge both
//! shapes, each against the release it describes.
//!
//! No network and no real tap: GitHub is a server on 127.0.0.1 answering
//! from a table (the bases moved by SCREEPUB_GITHUB_API), and the tap is a
//! scratch folder. The formula fixture is the published formula as read
//! from the tap on 2026-10-01 (v0.7.4), version and digests made up.

mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::{FakeServer, Scratch, text, xtask};
use std::process::Output;

const ARM: &str = "screepub-cli-macos-arm64.tar.gz";
const INTEL: &str = "screepub-cli-macos-x64.tar.gz";

fn sha(c: char) -> String {
    c.to_string().repeat(64)
}

/// The formula as the tap served it through the identifier release.
fn two_arch_formula(v: &str, arm: &str, x64: &str) -> String {
    format!(
        r##"class Screepub < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/ssandweiss/screepub"
  # Per-arch tarballs, not one universal binary.
  if Hardware::CPU.arm?
    url "https://github.com/ssandweiss/screepub/releases/download/v{v}/{ARM}"
    sha256 "{arm}"
  else
    url "https://github.com/ssandweiss/screepub/releases/download/v{v}/{INTEL}"
    sha256 "{x64}"
  end
  license "AGPL-3.0-or-later"

  livecheck do
    url "https://github.com/ssandweiss/screepub/releases/latest"
    strategy :github_latest
  end

  depends_on :macos

  def install
    bin.install "screepub"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/screepub --version")
  end
end
"##
    )
}

/// The retired cask, before its retirement notice.
fn bare_cask() -> String {
    format!(
        r##"cask "screepub" do
  version "0.7.3"
  sha256 "{}"

  url "https://github.com/ssandweiss/screepub/releases/download/v#{{version}}/Screepub-macOS.dmg"
  name "Screepub"
  homepage "https://github.com/ssandweiss/screepub"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "Screepub.app"
end
"##,
        sha('c')
    )
}

/// The cask as the identifier release's tap job leaves it: deprecated.
const DEPRECATED_ON: &str = "2026-09-30";

fn cask() -> String {
    bare_cask().replace(
        "  end\n\n  app",
        &format!(
            "  end\n\n  deprecate! date: \"{DEPRECATED_ON}\", because: \"is now downloaded from the releases page and updates itself\"\n\n  app"
        ),
    )
}

/// A fake GitHub for ONE release, with the tap's two files (base64, wrapped
/// at 60 columns, as the contents API sends them). A tag it does not hold
/// gets GitHub's 404. With `flaky`, the release's asset list fails the way
/// a transient API error does.
struct Gh {
    server: FakeServer,
}

impl Gh {
    fn new(
        release: &str,
        assets: &[(&str, String)],
        formula: Option<&str>,
        cask: Option<&str>,
        flaky: bool,
    ) -> Gh {
        let server = FakeServer::start();
        server.route(
            "/repos/ssandweiss/screepub/releases/latest",
            200,
            serde_json::json!({ "tag_name": format!("v{release}") }).to_string(),
        );
        let list: Vec<serde_json::Value> = assets
            .iter()
            .map(|(n, d)| serde_json::json!({ "name": n, "digest": format!("sha256:{d}") }))
            .collect();
        let tag_path = format!("/repos/ssandweiss/screepub/releases/tags/v{release}");
        if flaky {
            server.route(&tag_path, 502, "Bad Gateway");
        } else {
            server.route(
                &tag_path,
                200,
                serde_json::json!({ "tag_name": format!("v{release}"), "assets": list })
                    .to_string(),
            );
        }
        let contents = |t: &str| {
            let b64 = STANDARD.encode(t);
            let wrapped: Vec<String> = b64
                .as_bytes()
                .chunks(60)
                .map(|c| String::from_utf8_lossy(c).into_owned())
                .collect();
            serde_json::json!({ "content": wrapped.join("\n") + "\n", "encoding": "base64" })
                .to_string()
        };
        if let Some(f) = formula {
            server.route(
                "/repos/ssandweiss/homebrew-tap/contents/Formula/screepub.rb",
                200,
                contents(f),
            );
        }
        if let Some(c) = cask {
            server.route(
                "/repos/ssandweiss/homebrew-tap/contents/Casks/screepub.rb",
                200,
                contents(c),
            );
        }
        Gh { server }
    }

    fn run(&self, args: &[&str]) -> Output {
        xtask(
            args,
            &[
                ("SCREEPUB_GITHUB_API", self.server.base.as_str()),
                ("SCREEPUB_GITHUB_WEB", self.server.base.as_str()),
                ("SCREEPUB_REPO", "ssandweiss/screepub"),
                ("SCREEPUB_TAP", "ssandweiss/homebrew-tap"),
                ("GH_TOKEN", ""),
                ("GITHUB_TOKEN", ""),
            ],
            None,
            None,
        )
    }
}

fn tap_checkout(formula: &str, cask: &str) -> Scratch {
    let tap = Scratch::new("tap");
    tap.write("Formula/screepub.rb", formula);
    tap.write("Casks/screepub.rb", cask);
    tap
}

fn bump(gh: &Gh, version: &str, tap: &Scratch) -> Output {
    gh.run(&["bump-tap", version, &tap.path().to_string_lossy()])
}

// ── bump-tap, run against a tap ──────────────────────────────────────────

#[test]
fn it_turns_the_two_arch_formula_into_one_apple_silicon_formula() {
    let arm_sha = sha('a');
    let gh = Gh::new("0.8.0", &[(ARM, arm_sha.clone())], None, None, false);
    let tap = tap_checkout(&two_arch_formula("0.7.4", &sha('1'), &sha('2')), &cask());
    let r = bump(&gh, "v0.8.0", &tap);
    assert_eq!(text(&r.stderr), "");
    assert_eq!(r.status.code(), Some(0));
    let formula = tap.read("Formula/screepub.rb");
    let urls: Vec<&str> = formula
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("url \""))
        .map(|l| l.trim_end_matches('"'))
        .collect();
    assert_eq!(
        urls,
        [
            format!("https://github.com/ssandweiss/screepub/releases/download/v0.8.0/{ARM}"),
            "https://github.com/ssandweiss/screepub/releases/latest".to_string(),
        ]
    );
    assert!(formula.contains(&format!("sha256 \"{arm_sha}\"")));
    assert_eq!(formula.matches("sha256 \"").count(), 1);
    assert!(!formula.contains(INTEL));
    assert!(!formula.contains("Hardware::CPU"));
    let arch = formula.find("  depends_on arch: :arm64\n").expect("arch");
    let macos = formula.find("  depends_on :macos\n").expect("macos");
    assert!(macos > arch);
    assert!(
        !formula
            .lines()
            .any(|l| l.trim_start().starts_with("version \""))
    );
    assert!(text(&r.stdout).contains("tap rewritten to 0.8.0"));
    // The cask: DISABLED, keeping the deprecation's own date, in the same
    // place and nothing else changed.
    let c = tap.read("Casks/screepub.rb");
    assert!(!c.lines().any(|l| l.starts_with("  deprecate! ")));
    assert!(c.lines().any(|l| l
        == format!("  disable! date: \"{DEPRECATED_ON}\", because: \"is now downloaded from the releases page and updates itself\"")));
    let notice = |s: &str, prefix: &str| {
        s.lines()
            .map(|l| if l.starts_with(prefix) { "NOTICE" } else { l })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(notice(&c, "  disable! "), notice(&cask(), "  deprecate! "));
}

#[test]
fn a_cask_with_no_retirement_notice_yet_gets_the_disable_after_its_livecheck_block() {
    let gh = Gh::new("0.8.0", &[(ARM, sha('a'))], None, None, false);
    let tap = tap_checkout(
        &two_arch_formula("0.7.4", &sha('1'), &sha('2')),
        &bare_cask(),
    );
    assert_eq!(bump(&gh, "0.8.0", &tap).status.code(), Some(0));
    let c = tap.read("Casks/screepub.rb");
    assert!(c.contains(
        "    strategy :github_latest\n  end\n\n  disable! date: \"2026-10-01\", because: \"is now downloaded from the releases page and updates itself\"\n\n  app \"Screepub.app\""
    ), "{c}");
}

#[test]
fn a_release_whose_asset_list_cannot_be_read_writes_nothing() {
    let gh = Gh::new(
        "0.7.5",
        &[(ARM, sha('a')), (INTEL, sha('2'))],
        None,
        None,
        true,
    );
    let before = two_arch_formula("0.7.4", &sha('1'), &sha('2'));
    let tap = tap_checkout(&before, &cask());
    let r = bump(&gh, "0.7.5", &tap);
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stderr).contains("v0.7.5"));
    assert_eq!(tap.read("Formula/screepub.rb"), before);
    assert_eq!(tap.read("Casks/screepub.rb"), cask());
}

#[test]
fn a_release_that_still_carries_the_intel_tarball_is_refused_and_nothing_is_written() {
    let gh = Gh::new(
        "0.7.5",
        &[(ARM, sha('a')), (INTEL, sha('2'))],
        None,
        None,
        false,
    );
    let before = two_arch_formula("0.7.4", &sha('1'), &sha('2'));
    let tap = tap_checkout(&before, &cask());
    let r = bump(&gh, "0.7.5", &tap);
    assert_ne!(r.status.code(), Some(0));
    let err = text(&r.stderr);
    assert!(err.contains(INTEL));
    assert!(err.contains("git show v0.7.5:tools/bump-tap.sh"));
    assert_eq!(tap.read("Formula/screepub.rb"), before);
    assert_eq!(tap.read("Casks/screepub.rb"), cask());
}

#[test]
fn a_second_run_at_the_same_version_changes_nothing() {
    let gh = Gh::new("0.8.0", &[(ARM, sha('a'))], None, None, false);
    let tap = tap_checkout(&two_arch_formula("0.7.4", &sha('1'), &sha('2')), &cask());
    assert_eq!(bump(&gh, "0.8.0", &tap).status.code(), Some(0));
    let once = tap.read("Formula/screepub.rb");
    let cask_once = tap.read("Casks/screepub.rb");
    assert_eq!(bump(&gh, "0.8.0", &tap).status.code(), Some(0));
    assert_eq!(tap.read("Formula/screepub.rb"), once);
    assert_eq!(tap.read("Casks/screepub.rb"), cask_once);
}

#[test]
fn a_release_without_the_apple_silicon_tarball_writes_nothing() {
    let gh = Gh::new("0.8.0", &[(INTEL, sha('2'))], None, None, false);
    let before = two_arch_formula("0.7.4", &sha('1'), &sha('2'));
    let tap = tap_checkout(&before, &cask());
    let r = bump(&gh, "0.8.0", &tap);
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stderr).contains(ARM));
    assert_eq!(tap.read("Formula/screepub.rb"), before);
}

#[test]
fn a_tag_with_no_release_writes_nothing_rather_than_a_404_body_as_a_digest() {
    let gh = Gh::new("0.8.0", &[(ARM, sha('a'))], None, None, false);
    let before = two_arch_formula("0.7.4", &sha('1'), &sha('2'));
    let tap = tap_checkout(&before, &cask());
    assert_ne!(bump(&gh, "0.9.9", &tap).status.code(), Some(0));
    assert_eq!(tap.read("Formula/screepub.rb"), before);
}

#[test]
fn a_folder_that_is_not_a_tap_checkout_or_a_missing_argument_is_refused() {
    let gh = Gh::new("0.8.0", &[(ARM, sha('a'))], None, None, false);
    let empty = Scratch::new("not-a-tap");
    let r = bump(&gh, "0.8.0", &empty);
    assert_eq!(r.status.code(), Some(1));
    assert!(text(&r.stderr).contains("not a tap checkout"));
    assert_eq!(gh.run(&["bump-tap", "0.8.0"]).status.code(), Some(2));
}

// ── check-tap, judging the published tap ───────────────────────────────

/// What bump-tap writes, read back from a real run, so the two commands
/// are tested against each other and not against a guess.
fn bumped_tap(v: &str, arm: &str) -> (String, String) {
    let gh = Gh::new(v, &[(ARM, arm.to_string())], None, None, false);
    let tap = tap_checkout(&two_arch_formula("0.7.4", &sha('1'), &sha('2')), &cask());
    assert_eq!(bump(&gh, v, &tap).status.code(), Some(0));
    (
        tap.read("Formula/screepub.rb"),
        tap.read("Casks/screepub.rb"),
    )
}

fn bumped(v: &str, arm: &str) -> String {
    bumped_tap(v, arm).0
}

fn disabled_cask() -> String {
    bumped_tap("0.8.0", &sha('a')).1
}

fn check(release: &str, assets: &[(&str, String)], formula: &str, cask: &str) -> Output {
    Gh::new(release, assets, Some(formula), Some(cask), false).run(&["check-tap"])
}

#[test]
fn an_apple_silicon_release_served_by_the_tap_bump_tap_writes_is_fine() {
    let (formula, c) = bumped_tap("0.8.0", &sha('a'));
    let r = check("0.8.0", &[(ARM, sha('a'))], &formula, &c);
    let out = text(&r.stdout);
    assert!(out.contains("The tap serves 0.8.0"), "{out}");
    assert!(out.contains("retired (disabled)"));
    assert_eq!(r.status.code(), Some(0));
}

#[test]
fn an_apple_silicon_release_whose_cask_is_only_deprecated_is_an_error() {
    let r = check(
        "0.8.0",
        &[(ARM, sha('a'))],
        &bumped("0.8.0", &sha('a')),
        &cask(),
    );
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stdout).contains("disable!"));
}

#[test]
fn a_formula_still_serving_an_intel_tarball_the_release_does_not_carry_is_an_error() {
    let r = check(
        "0.8.0",
        &[(ARM, sha('a'))],
        &two_arch_formula("0.8.0", &sha('a'), &sha('2')),
        &disabled_cask(),
    );
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stdout).contains(INTEL));
}

#[test]
fn an_apple_silicon_formula_that_does_not_say_arm64_is_an_error() {
    let formula = bumped("0.8.0", &sha('a')).replace("  depends_on arch: :arm64\n", "");
    let r = check("0.8.0", &[(ARM, sha('a'))], &formula, &disabled_cask());
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stdout).contains("depends_on arch: :arm64"));
}

#[test]
fn a_wrong_apple_silicon_digest_is_an_error() {
    let r = check(
        "0.8.0",
        &[(ARM, sha('a'))],
        &bumped("0.8.0", &sha('b')),
        &disabled_cask(),
    );
    assert_ne!(r.status.code(), Some(0));
    assert!(text(&r.stdout).contains(ARM));
    // The way out it names is the command that exists, not the deleted script.
    assert!(text(&r.stdout).contains("`cargo xtask bump-tap` does it"));
    assert!(!text(&r.stdout).contains("tools/bump-tap.sh"));
}

#[test]
fn the_identifier_release_which_still_carries_an_intel_tarball_is_judged_as_it_shipped() {
    let formula = two_arch_formula("0.7.5", &sha('a'), &sha('2'));
    let both = [(ARM, sha('a')), (INTEL, sha('2'))];
    let ok = check("0.7.5", &both, &formula, &cask());
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok.stdout));
    assert!(text(&ok.stdout).contains("retired (deprecated)"));
    // Disabled while the identifier release is newest: the tap ran ahead.
    let early = check("0.7.5", &both, &formula, &disabled_cask());
    assert_ne!(early.status.code(), Some(0));
    assert!(text(&early.stdout).contains("deprecate!"));
    let bad_intel = check(
        "0.7.5",
        &[(ARM, sha('a')), (INTEL, sha('3'))],
        &formula,
        &cask(),
    );
    assert_ne!(bad_intel.status.code(), Some(0));
    assert!(text(&bad_intel.stdout).contains(INTEL));
}

#[test]
fn an_apple_silicon_release_whose_cask_says_both_deprecate_and_disable_is_an_error() {
    let disabled = disabled_cask();
    let both: String = disabled
        .lines()
        .map(|l| {
            if l.starts_with("  disable! ") {
                format!("  deprecate! date: \"2026-09-30\", because: \"is now downloaded from the releases page and updates itself\"\n{l}\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    assert!(both.lines().any(|l| l.starts_with("  deprecate! ")));
    assert!(both.lines().any(|l| l.starts_with("  disable! ")));
    let r = check(
        "0.8.0",
        &[(ARM, sha('a'))],
        &bumped("0.8.0", &sha('a')),
        &both,
    );
    assert_ne!(r.status.code(), Some(0));
    let out = text(&r.stdout);
    assert!(
        out.lines()
            .any(|l| l.contains("deprecate!") && l.contains("still installs")),
        "{out}"
    );
}

#[test]
fn a_cask_with_no_retirement_notice_at_all_is_an_error_before_and_after() {
    let after = check(
        "0.8.0",
        &[(ARM, sha('a'))],
        &bumped("0.8.0", &sha('a')),
        &bare_cask(),
    );
    assert_ne!(after.status.code(), Some(0));
    assert!(text(&after.stdout).contains("disable!"));
    let before = check(
        "0.7.5",
        &[(ARM, sha('a')), (INTEL, sha('2'))],
        &two_arch_formula("0.7.5", &sha('a'), &sha('2')),
        &bare_cask(),
    );
    assert_ne!(before.status.code(), Some(0));
    assert!(text(&before.stdout).contains("deprecate!"));
}

/// What tools/bump-tap.sh's heredoc wrote for 0.8.0, expanded, recorded
/// when the Rust cutover deleted the script (SHA_A stands for the digest).
const SHELL_FORMULA_0_8_0: &str = r##"class Screepub < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/ssandweiss/screepub"
  # Written whole by tools/bump-tap.sh in the screepub repository at every
  # release: edit it there, not here. Apple silicon only since the release
  # after 0.9.0; Intel Macs are no longer supported.
  url "https://github.com/ssandweiss/screepub/releases/download/v0.8.0/screepub-cli-macos-arm64.tar.gz"
  sha256 "SHA_A"
  license "AGPL-3.0-or-later"

  livecheck do
    url "https://github.com/ssandweiss/screepub/releases/latest"
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
    assert_match version.to_s, shell_output("#{bin}/screepub --version")
    assert_match "screenplay", shell_output("#{bin}/screepub --help")
  end
end
"##;

#[test]
fn the_formula_is_byte_for_byte_the_one_the_shell_script_wrote() {
    // Was checked against the script itself while it existed; its last
    // output is the pin now, so the tap sees no diff the shell would not
    // have made.
    assert_eq!(
        xtask::bump_tap::formula("ssandweiss/screepub", "0.8.0", &sha('a')),
        SHELL_FORMULA_0_8_0.replace("SHA_A", &sha('a'))
    );
}

#[test]
fn check_tap_names_the_version_it_was_asked_about() {
    // An explicit version is checked instead of the newest release.
    let (formula, c) = bumped_tap("0.8.0", &sha('a'));
    let gh = Gh::new("0.8.0", &[(ARM, sha('a'))], Some(&formula), Some(&c), false);
    let r = gh.run(&["check-tap", "v0.8.0"]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stdout));
    assert!(text(&r.stdout).starts_with("newest release: 0.8.0\n"));
    assert!(
        !gh.server
            .asked()
            .iter()
            .any(|p| p.ends_with("/releases/latest"))
    );
}
