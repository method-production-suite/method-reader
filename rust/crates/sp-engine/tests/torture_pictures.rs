//! The engine's half of tests/torture-pictures.test.ts: the book (#19a) and
//! the .mobi (#19b) converted from torture.pdf. Which pictures torture.pdf
//! keeps and what each becomes in the element stream are the parser's
//! (sp-parser/tests/torture_pictures.rs).

mod common;

use std::sync::OnceLock;

use common::epub::{body_of, entry, image_entries, opf, text};
use common::mobi_records::{exth, mobi_html, mobi_pictures};
use regex::Regex;
use sp_engine::convert::{ConvertOptions, ConvertResult, convert_pdf};
use sp_model::DEFAULT_FORMAT_OPTIONS;

// Sheet numbers are PDF pages, as the picture filenames count them.
const CARD: &str = "p001-01.png";
const PANEL: &str = "p015-01.png";
const PLATE: &str = "p016-01.png";
const TALL: &str = "p017-01.png";

fn torture(opts: ConvertOptions) -> ConvertResult {
    let pdf =
        std::fs::read(common::repo_root().join("tests/fixtures/torture.pdf")).expect("torture.pdf");
    convert_pdf(pdf, opts).expect("converts")
}

fn book() -> &'static ConvertResult {
    static BOOK: OnceLock<ConvertResult> = OnceLock::new();
    BOOK.get_or_init(|| torture(ConvertOptions::default()))
}

/// Through the safety gate: the .mobi it will be once the probe passes.
fn mobi_book() -> &'static ConvertResult {
    static BOOK: OnceLock<ConvertResult> = OnceLock::new();
    BOOK.get_or_init(|| {
        torture(ConvertOptions {
            mobi: true,
            mobi_pictures: Some(true),
            ..ConvertOptions::default()
        })
    })
}

fn figure(file: &str, cls: &str) -> String {
    format!("<p class=\"{cls}\"><img src=\"../images/{file}\" alt=\"Illustration\"/></p>")
}

fn toc(epub: &[u8]) -> Vec<String> {
    let nav = text(epub, "OEBPS/nav.xhtml");
    let re = Regex::new(r#"<li><a href="[^"]*">([^<]*)</a></li>"#).unwrap();
    re.captures_iter(&nav).map(|c| c[1].to_owned()).collect()
}

// ── the book built from torture.pdf (#19a) ──

#[test]
fn carries_exactly_the_four_pictures_and_no_letterhead() {
    let want: Vec<String> = [CARD, PANEL, PLATE, TALL]
        .iter()
        .map(|f| format!("OEBPS/images/{f}"))
        .collect();
    assert_eq!(image_entries(&book().epub), want);
}

#[test]
fn the_title_card_is_the_cover_on_no_page_and_the_book_opens_on_the_first_scene() {
    let epub = &book().epub;
    let body = body_of(epub);
    let opf = opf(epub);
    assert!(!body.contains(&format!("images/{CARD}")));
    assert!(opf.contains("<meta name=\"cover\" content=\"img-p001-01\"/>"));
    assert!(opf.contains(&format!(
        "href=\"images/{CARD}\" media-type=\"image/png\" properties=\"cover-image\""
    )));
    assert_eq!(toc(epub)[0], "INT. ARCHIVE BASEMENT - NIGHT");
    assert!(
        Regex::new(
            r#"<section class="scene" id="sc-001">\n<h2 class="scene-heading">INT\. ARCHIVE BASEMENT - NIGHT</h2>"#
        )
        .unwrap()
        .is_match(&body)
    );
}

#[test]
fn with_promote_cover_image_off_the_title_card_is_an_opening_section_of_its_own() {
    let mut format = DEFAULT_FORMAT_OPTIONS;
    format.promote_cover_image = false;
    let off = torture(ConvertOptions {
        format,
        ..ConvertOptions::default()
    });
    assert!(body_of(&off.epub).contains(&format!(
        "<section class=\"scene\" id=\"sc-001\">\n{}\n</section>",
        figure(CARD, "figure w100 figure-full")
    )));
    let toc = toc(&off.epub);
    assert_eq!(toc[0], "Opening");
    assert_eq!(toc[1], "INT. ARCHIVE BASEMENT - NIGHT");
    assert!(!opf(&off.epub).contains("cover-image"));
}

#[test]
fn the_panel_and_the_tall_plate_stay_in_the_flow_at_their_own_widths() {
    let body = body_of(&book().epub);
    assert!(body.contains(&figure(PANEL, "figure w40")));
    assert!(body.contains(&figure(TALL, "figure w55")));
}

#[test]
fn a_scene_heading_sits_directly_above_the_full_page_plate() {
    assert!(body_of(&book().epub).contains(&format!(
        "INT. MAP ROOM - NIGHT</h2>\n{}",
        figure(PLATE, "figure w100 figure-full")
    )));
}

// ── the .mobi built from torture.pdf (#19b) ──

#[test]
fn the_mobi_names_the_title_card_as_its_cover_the_record_the_epubs_cover_is() {
    let result = mobi_book();
    let mobi = result.mobi.as_ref().unwrap();
    assert_eq!(exth(mobi, 201), Some(vec![0, 0, 0, 0]));
    let shipped: Vec<Vec<u8>> = [CARD, PANEL, PLATE, TALL]
        .iter()
        .map(|f| entry(&result.epub, &format!("OEBPS/images/{f}")).unwrap())
        .collect();
    assert_eq!(mobi_pictures(mobi), shipped);
}

#[test]
fn and_still_draws_the_card_on_a_page_of_its_own_straight_after_the_title_page() {
    let html = mobi_html(mobi_book().mobi.as_ref().unwrap());
    assert!(html.contains(
        "</center>\n<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>\n"
    ));
    let re = Regex::new(r#"recindex="(\d+)""#).unwrap();
    let found: Vec<String> = re.captures_iter(&html).map(|c| c[1].to_owned()).collect();
    assert_eq!(found, ["00001", "00002", "00003", "00004"]);
}
