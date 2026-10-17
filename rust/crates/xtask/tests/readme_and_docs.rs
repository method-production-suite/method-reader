//! The README, the developer page, the ledger, the runbook and the release
//! notes say what the release actually does (the documentation half of
//! tests/release-artifacts.test.ts, and tests/beta-marks.test.ts's README
//! and site checks). Names come from sp_release::names, the one place the
//! release takes them from, so a renamed artifact fails here rather than
//! leaving a page naming a file the release does not carry.

mod repo_common;

use regex::Regex;
use repo_common::*;
use sp_release::names;

/// The text from `heading` to the next heading of the same level.
fn section_of(text: &str, heading: &str) -> String {
    let at = text
        .find(&format!("{heading}\n"))
        .unwrap_or_else(|| panic!("no section {heading:?}"));
    let level = heading.split(' ').next().unwrap();
    let rest = &text[at + heading.len()..];
    let next = Regex::new(&format!(r"(?m)^{level} "))
        .unwrap()
        .find(rest)
        .map_or(rest.len(), |m| m.start());
    rest[..next].to_string()
}

fn readme() -> String {
    read("README.md")
}

fn developers() -> String {
    read("docs/developers.md")
}

fn install() -> String {
    section_of(&readme(), "## Install")
}

/// The installers the release uploads (app-upload's five).
fn released() -> Vec<&'static str> {
    vec![
        names::MAC_DMG,
        names::WINDOWS_SETUP,
        names::LINUX_DEB_AMD64,
        names::LINUX_RPM_X86_64,
        names::LINUX_APPIMAGE_X86_64,
    ]
}

/// Names no leg builds, which no page may offer: the ARM Linux packages
/// and the per-arch Mac images of the universal era.
const UNPUBLISHED: &[&str] = &[
    "Screepub-linux-aarch64.rpm",
    "Screepub-linux-arm64.deb",
    "Screepub-macOS-arm64.dmg",
    "Screepub-macOS-x64.dmg",
];

const VERIFIED: &str = "Verified by a person";
const NOT_YET: &str = "Built and checked automatically. Never installed by a person yet";

/// The README install table's rows for `file`.
fn rows(file: &str) -> Vec<String> {
    install()
        .lines()
        .filter(|l| l.starts_with('|') && l.contains(&format!("`{file}`")))
        .map(str::to_string)
        .collect()
}

fn row(file: &str) -> String {
    let r = rows(file);
    assert_eq!(r.len(), 1, "{file}: {r:?}");
    r[0].clone()
}

fn lower(s: &str) -> String {
    flat(s).to_lowercase()
}

fn unsigned(s: &str) -> bool {
    let l = lower(s);
    l.contains("not signed") || l.contains("unsigned")
}

// ---- the limits, stated where a reader meets them ----------------------

#[test]
fn the_readme_says_the_windows_download_is_unsigned() {
    let l = lower(&readme());
    assert!(l.contains("windows") && l.contains("smartscreen"));
    assert!(unsigned(&readme()));
}

#[test]
fn the_developer_page_names_every_command_line_archive_and_the_readme_links_to_it() {
    let dev = developers();
    for name in [
        names::CLI_MAC_ARM64,
        names::CLI_LINUX_X64,
        names::CLI_LINUX_ARM64,
        names::CLI_WINDOWS_X64,
    ] {
        assert!(dev.contains(name), "{name}");
    }
    assert!(readme().contains("docs/developers.md#install-the-command-line-converter"));
    assert!(dev.contains("\n## Install the command-line converter\n"));
}

#[test]
fn the_developer_page_offers_no_intel_mac_download_and_says_homebrew_is_apple_silicon_only() {
    let install = section_of(&developers(), "## Install the command-line converter");
    assert!(!install.contains("screepub-cli-macos-x64.tar.gz"));
    assert!(!install.contains("| Mac, Intel |"));
    assert!(flat(&install).contains("On a Mac with Apple silicon, Homebrew installs"));
    assert!(install.contains("The Mac archive is signed and notarized"));
}

#[test]
fn the_readme_says_device_support_off_macos_is_unproven_and_names_the_tolino_case() {
    let l = readme().to_lowercase();
    assert!(l.contains("tolino"));
    // On Windows a tolino cannot be detected at all: it is identified by
    // volume name, and a Windows drive root carries none.
    assert!(
        Regex::new(r"(?s)tolino[^.]*windows|windows[^.]*tolino")
            .unwrap()
            .is_match(&l)
    );
}

#[test]
fn the_0_6_0_notes_carry_their_limits_and_name_what_0_6_0_published() {
    // History, pinned as literals on purpose: published notes are never
    // rewritten. These are the four files 0.6.0 put on its page.
    let notes = read("docs/releases/0.6.0.md");
    let l = notes.to_lowercase();
    assert!(l.contains("windows") && unsigned(&notes));
    assert!(l.contains("smartscreen"));
    assert!(l.contains("never been installed"));
    assert!(l.contains("tolino"));
    for name in [
        "Screepub_0.6.0_amd64.deb",
        "Screepub-0.6.0-1.x86_64.rpm",
        "Screepub-0.6.0-setup.exe",
        "Screepub-Desktop-macOS-universal.dmg",
    ] {
        assert!(notes.contains(name), "{name}");
    }
}

// ---- the app downloads, described where a reader meets them ------------

#[test]
fn the_readme_install_table_has_a_row_for_every_file_the_release_uploads() {
    for f in released() {
        row(f);
    }
    assert!(install().contains("SHA256SUMS-app"));
    let words = ["no", "one", "two", "three", "four", "five", "six"];
    assert!(install().contains(&format!("covers these {} files", words[released().len()])));
}

#[test]
fn every_file_the_readme_offers_is_one_the_release_uploads_with_its_count_stated() {
    // The other half of the test above: each row the table has names a
    // file the release uploads, and the sentence counts the rows.
    let offered: Vec<&str> = released()
        .into_iter()
        .filter(|f| !rows(f).is_empty())
        .collect();
    for f in &offered {
        row(f);
    }
    let table_rows = install()
        .lines()
        .filter(|l| l.starts_with("| ") && l.contains("`Method-Reader"))
        .count();
    assert_eq!(
        table_rows,
        offered.len(),
        "a row naming a file the release does not name"
    );
    let words = ["no", "one", "two", "three", "four", "five", "six"];
    assert!(install().contains(&format!("covers these {} files", words[offered.len()])));
}

#[test]
fn no_page_offers_a_bundle_the_release_does_not_carry() {
    for f in [
        "README.md",
        "docs/developers.md",
        "docs/releases/0.6.0.md",
        "site/index.html",
    ] {
        let text = read(f);
        for name in UNPUBLISHED {
            assert!(!text.contains(name), "{f} offers {name}");
        }
    }
}

#[test]
fn the_windows_warning_says_what_to_press_on_both_pages_that_offer_a_windows_download() {
    for text in [
        install(),
        section_of(&developers(), "## Install the command-line converter"),
    ] {
        let l = lower(&text);
        assert!(l.contains("smartscreen"));
        assert!(unsigned(&text));
        assert!(l.contains("more info"));
        assert!(l.contains("run anyway"));
    }
}

#[test]
fn the_readme_install_statuses_agree_with_the_verification_ledger() {
    let ledger = read("docs/verification-ledger.md");
    let person = &ledger[ledger
        .find("**Verified on a real machine, by a person**")
        .unwrap()..ledger.find("**Verified only by CI").unwrap()];
    let nobody = &ledger[ledger.find("**Verified by nobody**").unwrap()..];
    assert!(nobody.contains("Installing the `.deb`, the `.rpm` or the `.exe`"));
    assert!(nobody.contains("The Intel SLICE of 0.9.0's universal `.dmg`"));
    for f in [
        names::WINDOWS_SETUP,
        names::LINUX_DEB_AMD64,
        names::LINUX_RPM_X86_64,
    ] {
        assert!(row(f).contains(NOT_YET), "{f}");
    }
    // The AppImage is run, not installed; the ledger lists that as nobody's.
    assert!(nobody.contains("The Linux AppImage, new with the Rust window, run by a person"));
    assert!(
        row(names::LINUX_APPIMAGE_X86_64)
            .contains("Built and checked automatically. Never run by a person yet")
    );
    assert!(person.contains("/Applications"));
    assert!(person.contains("Apple silicon"));
    let mac = row(names::MAC_DMG);
    assert!(mac.contains(VERIFIED));
    assert!(mac.contains("Apple silicon"));
    assert!(!mac.contains("Intel"));
    assert!(install().contains("docs/verification-ledger.md"));
}

#[test]
fn the_readme_tells_an_intel_mac_which_version_is_its_last_and_where_it_is() {
    let install = install();
    let at = install.find("Intel Mac").expect("an Intel Mac sentence");
    let intel = &install[at..];
    assert!(intel.contains("0.9.0"));
    assert!(
        intel.contains(
            "https://github.com/method-production-suite/method-reader/releases/tag/v0.9.0"
        )
    );
    assert!(intel.contains("keeps working"));
}

#[test]
fn the_download_page_carries_the_unsigned_windows_warning() {
    let site = read("site/index.html");
    assert!(lower(&site).contains("smartscreen"));
    assert!(unsigned(&site));
}

#[test]
fn the_site_and_the_readme_offer_one_mac_download_the_apps_image() {
    let latest_mac = format!(
        "https://github.com/method-production-suite/method-reader/releases/latest/download/{}",
        names::MAC_DMG
    );
    let site = read("site/index.html");
    let buttons: Vec<String> = Regex::new(r#"<a class="btn[^"]*" href="([^"]+)""#)
        .unwrap()
        .captures_iter(&site)
        .map(|c| c[1].to_string())
        .collect();
    assert_eq!(buttons, vec![latest_mac.clone(); 3]);
    let readme = readme();
    let top = &readme[..readme.find("\n## ").unwrap()];
    assert!(top.contains(&format!("href=\"{latest_mac}\"")));
    for (f, text) in [
        ("README.md", readme.clone()),
        ("site", site.clone()),
        ("developers", developers()),
    ] {
        assert!(
            !text.contains("Screepub-Desktop-macOS-universal.dmg"),
            "{f}"
        );
        assert!(!text.contains("Screepub Desktop"), "{f}");
        assert!(!text.contains("macOS 14"), "{f}");
    }
    for text in [install(), site] {
        assert!(flat(&text).contains("macOS 13 or later"));
    }
}

#[test]
fn nothing_anywhere_promises_winget_the_aur_or_the_retired_cask() {
    // Out of scope; a promise in prose is a promise. (The AppImage was on
    // this list until the Rust release began publishing one.)
    for f in [
        "README.md",
        "docs/developers.md",
        "docs/releases/0.6.0.md",
        "site/index.html",
    ] {
        let l = read(f).to_lowercase();
        assert!(!l.contains("winget"), "{f}");
        assert!(!l.contains("aur "), "{f}");
    }
    for f in ["README.md", "docs/developers.md", "site/index.html"] {
        assert!(!read(f).contains("--cask"), "{f}");
    }
    assert!(readme().contains("brew install --formula ssandweiss/tap/method-reader"));
}

// ---- docs/verification-ledger.md, the one ledger ----------------------

fn ledger_parts() -> (String, String, String, String) {
    let doc = read("docs/verification-ledger.md");
    let p = doc
        .find("**Verified on a real machine, by a person**")
        .unwrap();
    let c = doc.find("**Verified only by CI").unwrap();
    let n = doc.find("**Verified by nobody**").unwrap();
    (
        doc.clone(),
        doc[p..c].to_string(),
        doc[c..n].to_string(),
        doc[n..].to_string(),
    )
}

#[test]
fn the_ledger_has_all_three_lists_in_order() {
    let doc = read("docs/verification-ledger.md");
    let p = doc
        .find("**Verified on a real machine, by a person**")
        .unwrap();
    let c = doc.find("**Verified only by CI").unwrap();
    let n = doc.find("**Verified by nobody**").unwrap();
    assert!(p > 0 && c > p && n > c);
}

#[test]
fn the_by_a_person_list_claims_only_the_architecture_that_was_built() {
    let (_, person, _, _) = ledger_parts();
    assert!(person.contains("Screepub_0.6.0_arm64.deb"));
    assert!(person.contains("Screepub-0.6.0-1.aarch64.rpm"));
    assert!(!person.contains("amd64"));
    assert!(person.contains("/Applications"));
    assert!(person.contains("0.7.2 to 0.7.3"));
    let f = flat(&person);
    assert!(f.contains(
        "Each Mac that ran these is Apple silicon, the only kind the Mac download is built for"
    ));
    assert!(!f.contains("ARM half"));
}

#[test]
fn the_nobody_list_keeps_what_nobody_has_done_and_only_that() {
    let (_, _, _, nobody) = ledger_parts();
    assert!(
        !Regex::new(r"`\.dmg` or `\.exe` has been\s+installed")
            .unwrap()
            .is_match(&nobody)
    );
    assert!(nobody.contains(".exe"));
    assert!(!nobody.contains("x86_64-apple-darwin"));
    let intel = &nobody[nobody.find("- The Intel SLICE").unwrap()..];
    let bullet = flat(&intel[..intel.find("\n- ").unwrap_or(intel.len())]);
    assert!(bullet.contains("the last Mac download to carry one"));
    assert!(bullet.contains("never ran"));
    assert!(bullet.contains("Apple silicon only"));
    assert!(!bullet.contains("has never executed"));
}

#[test]
fn the_ci_list_says_what_release_yml_does_and_that_the_mac_image_runs_whole() {
    let (_, _, ci, _) = ledger_parts();
    assert!(ci.contains("release.yml"));
    assert!(ci.contains("notariz"));
    assert!(
        !Regex::new(r"never run|never been run|has not run|never executed")
            .unwrap()
            .is_match(&ci)
    );
    let f = flat(&ci);
    assert!(f.contains("The Mac `.dmg` is built for Apple silicon only"));
    assert!(!f.contains("prints a notice"));
}

#[test]
fn every_relative_link_in_the_ledger_resolves() {
    let (doc, _, _, _) = ledger_parts();
    let links = Regex::new(r"\]\(([^)#]+)(#[^)]*)?\)").unwrap();
    let mut n = 0;
    for c in links.captures_iter(&doc) {
        let target = &c[1];
        if target.starts_with("http:") || target.starts_with("https:") {
            continue;
        }
        n += 1;
        assert!(root().join("docs").join(target).exists(), "{target}");
    }
    assert!(n > 0);
}

// ---- the developer docs: the Apple silicon build ----------------------

#[test]
fn contributing_and_the_device_map_no_longer_speak_of_a_universal_mac_build() {
    let re = Regex::new(r"universal Mac\s+build").unwrap();
    assert!(!re.is_match(&read("CONTRIBUTING.md")));
    assert!(!re.is_match(&read("docs/device-map.md")));
}

#[test]
fn contributing_and_the_ledger_describe_the_retired_cask_the_same_way() {
    let words = "retired (deprecated, and disabled from the release after 0.9.0)";
    for f in ["CONTRIBUTING.md", "docs/verification-ledger.md"] {
        let text = flat(&read(f));
        assert!(text.contains(words), "{f}");
        assert!(!text.contains("cask still says it is deprecated"), "{f}");
    }
}

#[test]
fn contributing_says_an_intel_mac_cannot_run_the_window_where_it_says_to_open_it() {
    let c = read("CONTRIBUTING.md");
    let at = c
        .find("cargo run -p sp-app")
        .expect("CONTRIBUTING says how to open the window");
    let after = flat(&c[at..(at + 800).min(c.len())]);
    assert!(
        after.contains("On an Intel Mac** the window is not offered"),
        "{after}"
    );
}

#[test]
fn the_release_runbook_counts_one_mac_cli_tarball_and_one_sha() {
    let runbook = flat(&read("docs/release-secrets.md"));
    for gone in [
        "Two SHAs are needed",
        "two urls, two SHAs",
        "rewrites the formula's two urls and two SHAs",
        "after the full universal build",
        "`screepub-cli-macos-x64.tar.gz` from the `release` job",
        "that day's date",
    ] {
        assert!(!runbook.contains(gone), "{gone}");
    }
    for want in [
        "One SHA is needed",
        "depends_on arch: :arm64",
        "if/else on the CPU",
        "disable!",
        "keeping its date",
    ] {
        assert!(runbook.contains(want), "{want}");
    }
}

// ---- beta, in the README and on the site ------------------------------

#[test]
fn the_readme_and_the_site_say_what_beta_means_and_mark_the_five() {
    let meaning = "works in our tests, but has not been tried widely on real devices or apps yet";
    for f in ["README.md", "site/index.html"] {
        let text = flat(&read(f));
        assert!(text.contains(meaning), "{f}");
        assert!(!text.contains('\u{2014}'), "{f}");
        for (what, re) in [
            ("FDX", r"(?i)fdx[^.]{0,120}\(beta\)"),
            ("pictures", r"[Pp]ictures[^.]{0,160}\(beta\)"),
            ("eject", r"[Ee]ject[^.]{0,160}\(beta on Windows and Linux\)"),
            ("requests", r"isn.t here[^.]{0,200}\(beta\)"),
            ("AZW3", r"AZW3[^.]{0,120}\(beta"),
        ] {
            assert!(Regex::new(re).unwrap().is_match(&text), "{f}: {what}");
        }
    }
}

#[test]
fn the_readme_and_the_site_tell_a_screepub_user_about_the_new_name() {
    // Someone arriving from an old link, a Homebrew warning or the 0.12
    // app knows it as Screepub: both pages say once, plainly, that this is
    // the same app under a new name and that updating keeps their things.
    let readme = readme();
    assert!(readme.contains("Method Reader is the new name for Screepub"));
    assert!(readme.contains("keeps your settings and your library"));
    let site = read("site/index.html");
    assert!(site.contains("Screepub is now Method Reader."));
    assert!(site.contains("keeps your settings and your library"));
}

#[test]
fn the_readme_says_plainly_what_the_update_check_sends_and_how_to_turn_it_off() {
    let section = flat(&section_of(
        &readme(),
        "## Your script stays on your machine",
    ));
    assert!(section.contains("no network except checking for updates, which you can turn off"));
    assert!(section.contains("two minutes after the window opens and every six hours"));
    assert!(section.contains("`latest.json`"));
    assert!(section.contains("carries the app's name and version"));
    assert!(section.contains("nothing about your scripts"));
    assert!(section.contains("**Help > Check for Updates Automatically**"));
    assert!(section.contains("old once-a-day question, they stay off"));
    // The 0.12 wording, retired with the question.
    assert!(!section.contains("only if you say yes"));
    assert!(!section.contains("Method Reader reaches the network only when you ask"));
}
