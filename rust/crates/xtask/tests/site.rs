//! site/index.html (was tests/site.test.ts). site/ stays HTML after the
//! cutover; only the check moved.
//!
//! "Drag and drop" shows the real window: the capture tool's own site
//! pictures, sized to the files it wrote. "Every download" links each file
//! by its version-free name through /releases/latest/download/, with a
//! status per file that follows docs/verification-ledger.md. The names come
//! from sp_release::names, the one place the release takes them from.

mod repo_common;

use regex::Regex;
use repo_common::*;
use sp_release::names;
use xtask::capture::shots::{SHOTS, Theme, outputs_for};

const LATEST: &str =
    "https://github.com/method-production-suite/method-reader/releases/latest/download/";
const VERIFIED: &str = "Verified by a person";
const NOT_YET: &str = "Built and checked automatically. Never installed by a person yet";

fn site() -> String {
    read("site/index.html")
}

/// The markup of the first `<section class="<cls> ...">`, to its close.
fn section(cls: &str) -> String {
    let site = site();
    let start = Regex::new(&format!(r#"<section class="{cls}[ "]"#))
        .unwrap()
        .find(&site)
        .unwrap_or_else(|| panic!("no section {cls}"))
        .start();
    let end = site[start..].find("</section>").unwrap() + start;
    site[start..end].to_string()
}

/// Every value of attribute `name` in `html`, in order.
fn attrs(html: &str, name: &str) -> Vec<String> {
    Regex::new(&format!(r#"\s{name}="([^"]*)""#))
        .unwrap()
        .captures_iter(html)
        .map(|c| c[1].to_string())
        .collect()
}

/// A PNG's pixel size, from its IHDR chunk.
fn png_size(path: &std::path::Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(&b[1..4], b"PNG", "{}", path.display());
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    (be(16), be(20))
}

fn drop_images() -> Vec<String> {
    Regex::new(r"<img\b[^>]*>")
        .unwrap()
        .find_iter(&section("drop"))
        .map(|m| m.as_str().to_string())
        .collect()
}

#[test]
fn drag_and_drop_shows_the_capture_tools_site_pictures_drop_then_result() {
    let captured: Vec<String> = SHOTS
        .iter()
        .flat_map(|s| outputs_for(s, Theme::Light))
        .filter_map(|p| p.strip_prefix("site/").map(str::to_string))
        .collect();
    assert_eq!(captured, vec!["img/drop-light.png", "img/result-light.png"]);
    let srcs: Vec<String> = drop_images()
        .iter()
        .map(|img| attrs(img, "src")[0].clone())
        .collect();
    assert_eq!(srcs, captured);
}

#[test]
fn each_picture_is_sized_to_its_file_which_the_tool_takes_at_2x() {
    for img in drop_images() {
        let (w, h) = png_size(&root().join("site").join(&attrs(&img, "src")[0]));
        assert_eq!(attrs(&img, "width"), vec![(w / 2).to_string()], "{img}");
        assert_eq!(attrs(&img, "height"), vec![(h / 2).to_string()], "{img}");
    }
}

#[test]
fn each_picture_says_what_it_shows() {
    let imgs = drop_images();
    let alts: Vec<String> = imgs.iter().map(|i| attrs(i, "alt")[0].clone()).collect();
    for a in &alts {
        assert!(a.len() > 30, "{a}");
    }
    let mut unique = alts.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), imgs.len());
}

#[test]
fn the_section_keeps_its_words() {
    let drop = section("drop");
    assert!(drop.contains("INT. THE APP - ONE STEP"));
    assert!(drop.contains("<h2>Drag and drop.</h2>"));
    assert!(drop.contains(
        "It's that simple. Fine tune anything you like afterwards, and see exactly what the device gets."
    ));
}

#[test]
fn the_drawing_and_its_animation_are_gone() {
    let site = site();
    for gone in [
        "id=\"well\"",
        "pdficon",
        "DROP A SCREENPLAY HERE",
        "runDrop",
        ".well{",
        "dropTimer",
    ] {
        assert!(!site.contains(gone), "{gone}");
    }
}

#[test]
fn every_local_src_and_href_resolves_to_a_file_inside_site() {
    // The suite website copies site/ alone, so a picture referenced from
    // assets/ (where the README's copies live) would 404 on methodreader.com.
    let site = site();
    let css: Vec<String> = Regex::new(r"url\(([^)]+)\)")
        .unwrap()
        .captures_iter(&site)
        .map(|c| c[1].trim_matches(|ch| ch == '"' || ch == '\'').to_string())
        .collect();
    let external = Regex::new(r"^(https?:|mailto:|#|data:)").unwrap();
    let local: Vec<String> = attrs(&site, "src")
        .into_iter()
        .chain(attrs(&site, "href"))
        .chain(css)
        .filter(|v| !external.is_match(v))
        .collect();
    for want in ["img/drop-light.png", "img/result-light.png", "favicon.svg"] {
        assert!(local.contains(&want.to_string()), "{want}: {local:?}");
    }
    assert!(local.iter().filter(|v| v.starts_with("fonts/")).count() >= 6);
    let site_dir = root().join("site");
    for r in local {
        let rel = r.split(['?', '#']).next().unwrap();
        assert!(!rel.contains(".."), "{r} leaves site/");
        assert!(site_dir.join(rel).is_file(), "{r} is not a file in site/");
    }
}

/// The installers the release uploads, by the names sp-release gives them.
fn released() -> Vec<&'static str> {
    vec![
        names::MAC_DMG,
        names::WINDOWS_SETUP,
        names::LINUX_DEB_AMD64,
        names::LINUX_RPM_X86_64,
        names::LINUX_APPIMAGE_X86_64,
    ]
}

/// The download table's rows that link `file`.
fn rows_for(file: &str) -> Vec<String> {
    Regex::new(r"<tr\b[\s\S]*?</tr>")
        .unwrap()
        .find_iter(&section("downloads"))
        .map(|m| m.as_str().to_string())
        .filter(|r| r.contains(&format!("href=\"{LATEST}{file}\"")))
        .collect()
}

fn row(file: &str) -> String {
    let rows = rows_for(file);
    assert_eq!(rows.len(), 1, "{file}: {rows:?}");
    rows[0].clone()
}

#[test]
fn every_file_the_release_uploads_is_a_link_to_the_latest_release_by_its_own_name() {
    for file in released() {
        row(file);
    }
}

#[test]
fn every_file_the_page_offers_is_one_the_release_uploads_with_one_row_each() {
    // The other half of the test above: every row links one of the
    // release's own names, once each, and no other file.
    let offered: Vec<&str> = released()
        .into_iter()
        .filter(|f| !rows_for(f).is_empty())
        .collect();
    assert_eq!(
        offered,
        vec![
            names::MAC_DMG,
            names::WINDOWS_SETUP,
            names::LINUX_DEB_AMD64,
            names::LINUX_RPM_X86_64,
            names::LINUX_APPIMAGE_X86_64
        ]
    );
    for f in offered {
        row(f);
    }
    let all = Regex::new(r"<tr\b[\s\S]*?</tr>")
        .unwrap()
        .find_iter(&section("downloads"))
        .filter(|m| m.as_str().contains(LATEST))
        .count();
    assert_eq!(all, 5, "a row linking a file the release does not name");
}

#[test]
fn no_download_link_on_the_page_carries_a_version() {
    // The same file at two versions has the same name, which is what makes
    // /latest/download/ work.
    let links: Vec<String> = attrs(&site(), "href")
        .into_iter()
        .filter(|h| h.contains("/releases/"))
        .collect();
    assert!(links.len() > 4, "{links:?}");
    let version = Regex::new(r"\d+\.\d+\.\d+").unwrap();
    for l in links {
        assert!(!version.is_match(&l), "{l}");
        assert!(!l.contains("/releases/download/"), "{l}");
    }
}

#[test]
fn every_offered_file_has_a_status_and_the_statuses_follow_the_ledger() {
    let ledger = read("docs/verification-ledger.md");
    let nobody_at = ledger
        .find("**Verified by nobody**")
        .expect("a nobody list");
    let nobody = &ledger[nobody_at..];
    // If one of these lines goes, someone did the thing: move that row up.
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
    let person = &ledger[ledger
        .find("**Verified on a real machine, by a person**")
        .unwrap()..ledger.find("**Verified only by CI").unwrap()];
    assert!(person.contains("dragged to `/Applications`"));
    assert!(person.contains("0.7.2 to 0.7.3"));
    assert!(flat(person).contains("Each Mac that ran these is Apple silicon"));
    assert!(section("downloads").contains("docs/verification-ledger.md"));
    let mac = row(names::MAC_DMG);
    assert!(mac.contains(VERIFIED));
    assert!(mac.contains("Apple silicon"));
    assert!(!mac.contains("Intel"));
    assert!(!site().contains("Screepub-Desktop-macOS-universal.dmg"));
}

#[test]
fn an_intel_mac_is_told_0_9_0_is_its_last_version() {
    let downloads = section("downloads");
    let at = downloads.find("Intel Mac").expect("an Intel Mac sentence");
    let intel = &downloads[at..];
    assert!(intel.contains("0.9.0"));
    assert!(intel.contains("keeps working"));
    assert!(
        intel
            .contains("href=\"https://github.com/method-production-suite/method-reader/releases\"")
    );
}

#[test]
fn the_sentence_the_status_column_replaced_is_gone() {
    let downloads = section("downloads");
    assert!(!downloads.contains("anything but Linux"));
    assert!(!downloads.contains("Nobody has installed the new window yet"));
}

#[test]
fn the_page_and_the_share_card_carry_no_em_dash_in_any_spelling() {
    let dash = Regex::new(r"(?i)\u{2014}|&mdash;|&#8212;|&#x2014;").unwrap();
    for f in ["site/index.html", "site/og-card.html"] {
        assert!(!dash.is_match(&read(f)), "{f}");
    }
}
