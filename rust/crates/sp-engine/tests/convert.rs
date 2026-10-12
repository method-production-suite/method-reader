//! Port of tests/convert.test.ts: the conversion, in process, on the
//! committed fixtures (tests/fixtures/*.pdf), through sp-pdf.
//!
//! Where the TypeScript spies on `resolveImageData` and `encodePng` to show
//! a refused document never pays for its pictures, this file counts the
//! picture fetches through `convert_facts_with`, which is how `convert_pdf`
//! reaches sp-pdf's pixels: the facts sp-pdf reads first carry no pixels, so
//! with no fetch there is nothing to encode either.
//!
//! The two real-script blocks (a full-length script, the picture cap) run
//! only where root fixtures/ holds those scripts, and assert counts only.

mod common;

use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use common::epub::{body_of, identifier, image_entries, opf, read_zip, text, toc_of};
use common::mobi_records::{
    exth, exth_records, mask_pdb_dates, mobi_html, mobi_pictures, sha256_hex,
};
use sp_engine::convert::{
    ConvertError, ConvertOptions, ConvertResult, ConvertStage, convert_facts_with,
    convert_fountain, convert_pdf, image_warnings,
};
use sp_epub::{
    BodyOptions, BookMeta, build_epub, screenplay_css, tokens_to_body, tokens_to_preview_html,
};
use sp_mobi::{MOBI_CARRIES_PICTURES, mobi_art};
use sp_model::{ContdMode, DEFAULT_FORMAT_OPTIONS, FormatOptions, Token};
use sp_parser::artwork::DroppedBy;
use sp_parser::images::ImageStats;
use sp_parser::png::encode_png;

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG: [u8; 12] = [
    0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46, 0, 1,
];

fn public(name: &str) -> PathBuf {
    common::repo_root().join("tests/fixtures").join(name)
}

/// Root fixtures/: real scripts, gitignored. Counts only.
/// Root fixtures/ (real scripts, gitignored): this checkout's, else the
/// main checkout's when this is a worktree (found from the shared .git).
fn private(name: &str) -> PathBuf {
    let here = common::repo_root().join("fixtures");
    if here.is_dir() {
        return here.join(name);
    }
    let main = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(common::repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
                .parent()
                .map(|p| p.join("fixtures"))
        });
    main.unwrap_or(here).join(name)
}

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(public(name)).expect("a committed fixture")
}

fn pdf(name: &str) -> ConvertResult {
    convert_pdf(bytes(name), ConvertOptions::default()).expect("converts")
}

fn pdf_with(name: &str, opts: ConvertOptions) -> ConvertResult {
    convert_pdf(bytes(name), opts).expect("converts")
}

fn fountain(text: &str) -> ConvertResult {
    convert_fountain(text, ConvertOptions::default(), None).expect("converts")
}

fn format(f: impl FnOnce(&mut FormatOptions)) -> FormatOptions {
    let mut o = DEFAULT_FORMAT_OPTIONS;
    f(&mut o);
    o
}

fn opts(format: FormatOptions) -> ConvertOptions<'static> {
    ConvertOptions {
        format,
        ..ConvertOptions::default()
    }
}

fn tokens(text: &str) -> Vec<Token> {
    sp_fountain::tokenize(text).expect("tokens")
}

fn names(r: &ConvertResult) -> Vec<String> {
    r.images.iter().map(|i| i.name.clone()).collect()
}

fn png_of(rgb: [u8; 3]) -> Vec<u8> {
    encode_png(&rgb, 1, 1).expect("a PNG")
}

fn map(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(n, b)| ((*n).to_owned(), b.to_vec()))
        .collect()
}

/// The pictures as the preview takes them, first bytes per name.
fn picture_list(r: &ConvertResult) -> Vec<(String, Vec<u8>)> {
    r.images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect()
}

fn image_set(r: &ConvertResult) -> HashSet<String> {
    r.images.iter().map(|i| i.name.clone()).collect()
}

// ── convertPdf ──

#[test]
fn screenplay_pdf_converts_end_to_end() {
    let result = pdf("screenplay.pdf");
    assert_eq!(result.title, "The Last Video Store");
    assert_eq!(result.author.as_deref(), Some("A. N. Placeholder"));
    assert_eq!(result.screenplay.as_ref().unwrap().scenes.len(), 5);
    assert!(
        result
            .fountain_text
            .contains("INT. THE LAST VIDEO STORE - NIGHT")
    );
    assert!(opf(&result.epub).contains("<dc:title>The Last Video Store</dc:title>"));
}

#[test]
fn image_only_pdf_raises_scanned() {
    let err = convert_pdf(bytes("blank-pages.pdf"), ConvertOptions::default()).unwrap_err();
    assert!(matches!(err, ConvertError::Scanned { .. }), "{err:?}");
}

#[test]
fn non_screenplay_pdf_raises_not_a_screenplay() {
    let err = convert_pdf(bytes("prose.pdf"), ConvertOptions::default()).unwrap_err();
    assert_eq!(err, ConvertError::NotAScreenplay);
}

#[test]
fn rebuilding_the_same_script_keeps_the_same_identifier() {
    let src = "Title: Stable Book\n\nINT. ROOM - DAY\n\nAction holds.\n";
    let a = fountain(src);
    let b = fountain(src);
    assert_eq!(identifier(&a.epub), identifier(&b.epub));
    let c = fountain(&format!("{src}\nMore action arrives.\n"));
    assert_ne!(identifier(&c.epub), identifier(&a.epub));
}

#[test]
fn title_and_author_overrides_win_over_detection() {
    let result = pdf_with(
        "screenplay.pdf",
        ConvertOptions {
            title: Some("Override Title".into()),
            author: Some("Override Author".into()),
            ..ConvertOptions::default()
        },
    );
    assert_eq!(result.title, "Override Title");
    assert_eq!(result.author.as_deref(), Some("Override Author"));
}

// ── convertPdf progress ──

fn ticks_of(name: &str) -> Vec<(ConvertStage, f64)> {
    let mut ticks = Vec::new();
    {
        let mut cb = |stage: ConvertStage, fraction: f64, _: Option<(u32, u32)>| {
            ticks.push((stage, fraction));
        };
        convert_pdf(
            bytes(name),
            ConvertOptions {
                on_progress: Some(&mut cb),
                ..ConvertOptions::default()
            },
        )
        .expect("converts");
    }
    ticks
}

fn of(ticks: &[(ConvertStage, f64)], stage: ConvertStage) -> Vec<f64> {
    ticks
        .iter()
        .filter(|(s, _)| *s == stage)
        .map(|(_, f)| *f)
        .collect()
}

fn stages(ticks: &[(ConvertStage, f64)]) -> Vec<ConvertStage> {
    let mut out: Vec<ConvertStage> = Vec::new();
    for (s, _) in ticks {
        if !out.contains(s) {
            out.push(*s);
        }
    }
    out
}

fn in_order(ticks: &[(ConvertStage, f64)]) {
    let fractions: Vec<f64> = ticks.iter().map(|(_, f)| *f).collect();
    let mut sorted = fractions.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(sorted, fractions);
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 5e-11, "{a} is not close to {b}");
}

#[test]
fn a_document_with_pictures_passes_through_all_three_stages_in_order() {
    let ticks = ticks_of("images.pdf");
    assert_eq!(
        stages(&ticks),
        [
            ConvertStage::Parse,
            ConvertStage::Pictures,
            ConvertStage::Render
        ]
    );
    in_order(&ticks);
    close(*of(&ticks, ConvertStage::Parse).last().unwrap(), 0.6);
    close(of(&ticks, ConvertStage::Pictures)[0], 0.6);
    close(*of(&ticks, ConvertStage::Pictures).last().unwrap(), 0.85);
    assert_eq!(of(&ticks, ConvertStage::Render), [0.85, 1.0]);
}

#[test]
fn a_document_without_pictures_goes_straight_from_the_pages_to_the_book() {
    let ticks = ticks_of("screenplay.pdf");
    assert_eq!(stages(&ticks), [ConvertStage::Parse, ConvertStage::Render]);
    in_order(&ticks);
    close(*of(&ticks, ConvertStage::Parse).last().unwrap(), 0.6);
    assert_eq!(of(&ticks, ConvertStage::Render), [0.85, 1.0]);
}

// ── convertPdf on a full-length script (real, counts only) ──

#[test]
fn a_full_length_script_converts_with_a_plausible_scene_count() {
    let path = private("final-draft.pdf");
    if !path.exists() {
        return;
    }
    let result = convert_pdf(std::fs::read(path).unwrap(), ConvertOptions::default()).unwrap();
    assert!(result.screenplay.unwrap().scenes.len() > 50);
    assert_ne!(result.title, "Untitled Screenplay");
    assert_eq!(result.warnings, Vec::<String>::new());
}

// ── convertFountain ──

const SRC: &str =
    "Title: Direct Input\nAuthor: Someone\n\nINT. VOID - DAY\n\nText appears.\n\n@VOICE\nHello.\n";

#[test]
fn fountain_text_converts_with_metadata_from_the_title_block() {
    let result = fountain(SRC);
    assert_eq!(result.title, "Direct Input");
    assert_eq!(result.author.as_deref(), Some("Someone"));
    let ch = text(&result.epub, "OEBPS/text/body001.xhtml");
    assert!(ch.contains("INT. VOID - DAY"));
    assert!(ch.contains("<p class=\"character\">VOICE</p>"));
}

#[test]
fn untitled_fountain_falls_back_to_a_default_title() {
    assert_eq!(
        fountain("INT. SOMEWHERE - DAY\n\nAction.\n").title,
        "Untitled Screenplay"
    );
}

// ── convertFountain warns when a stage-1 option cannot apply ──

const WITH_CONTD: &str = "INT. STORE - NIGHT\n\nAction.\n\n@MARGO\nFirst half.\n\n@MARGO (CONT\u{2019}D)\nSecond half.\n";
const WITHOUT_CONTD: &str = "INT. STORE - NIGHT\n\nAction.\n\n@MARGO\nOnly speech.\n";

fn contd(mode: ContdMode) -> FormatOptions {
    format(|o| o.contd_mode = mode)
}

#[test]
fn strip_requested_but_contd_is_already_baked_into_the_cues() {
    let result = convert_fountain(WITH_CONTD, opts(contd(ContdMode::Strip)), None).unwrap();
    assert_eq!(
        result.warnings,
        [
            "This book still shows (CONT'D): it is written into the script the book was made from. \
             Removing it takes effect when you convert the script's PDF."
        ]
    );
}

#[test]
fn the_warning_carries_no_word_only_the_engine_knows() {
    let result = convert_fountain(WITH_CONTD, opts(contd(ContdMode::Strip)), None).unwrap();
    let said = &result.warnings[0];
    for word in [
        "contdMode",
        "strip",
        ".fountain",
        "Fountain",
        "re-convert",
        "\u{2014}",
    ] {
        assert!(!said.contains(word), "{word}");
    }
}

#[test]
fn the_cues_really_do_keep_their_contd() {
    let result = convert_fountain(WITH_CONTD, opts(contd(ContdMode::Strip)), None).unwrap();
    assert!(text(&result.epub, "OEBPS/text/body001.xhtml").contains("CONT"));
}

#[test]
fn silent_when_there_is_no_contd_to_strip() {
    let result = convert_fountain(WITHOUT_CONTD, opts(contd(ContdMode::Strip)), None).unwrap();
    assert_eq!(result.warnings, Vec::<String>::new());
}

#[test]
fn silent_for_keep_and_auto() {
    for mode in [ContdMode::Keep, ContdMode::Auto] {
        let result = convert_fountain(WITH_CONTD, opts(contd(mode)), None).unwrap();
        assert_eq!(result.warnings, Vec::<String>::new(), "{mode:?}");
    }
}

#[test]
fn pdf_input_never_warns_about_contd() {
    let result = pdf_with("screenplay.pdf", opts(contd(ContdMode::Strip)));
    assert!(!result.warnings.iter().any(|w| w.contains("CONT'D")));
    assert!(!text(&result.epub, "OEBPS/text/body001.xhtml").contains("CONT&apos;D"));
}

// ── convertFountain previewHtml ──

#[test]
fn convert_fountain_returns_the_preview_document() {
    let result = fountain("Title: T\n\nINT. LAB - DAY\n\nBeakers bubble.\n\nELI\nEureka.\n");
    assert!(result.preview_html.contains("class=\"dialogue-block\""));
    assert!(result.preview_html.contains("<style>"));
}

// ── convertPdf carries the pictures ──

#[test]
fn both_kept_pictures_come_back_as_named_png_bytes() {
    let result = pdf("images.pdf");
    assert_eq!(names(&result), ["p001-01.png", "p002-01.png"]);
    for img in &result.images {
        assert_eq!(img.bytes[..8], PNG_MAGIC);
        assert!(img.bytes.len() > 100);
    }
}

#[test]
fn every_name_carried_is_a_name_the_fountain_asks_for() {
    let result = pdf("images.pdf");
    assert!(!result.images.is_empty());
    for img in &result.images {
        assert!(
            result
                .fountain_text
                .contains(&format!("[[img: {}", img.name))
        );
    }
}

#[test]
fn a_script_with_no_pictures_returns_an_empty_array_and_says_nothing() {
    let result = pdf("screenplay.pdf");
    assert!(result.images.is_empty());
    assert_eq!(result.warnings, Vec::<String>::new());
}

fn scene_headings(r: &ConvertResult) -> Vec<String> {
    r.screenplay
        .as_ref()
        .unwrap()
        .scenes
        .iter()
        .map(|s| s.heading.clone())
        .collect()
}

fn toc_titles(epub: &[u8]) -> Vec<String> {
    toc_of(epub).into_iter().map(|(_, t)| t).collect()
}

fn assert_book_carries(result: &ConvertResult) {
    assert_eq!(
        image_entries(&result.epub),
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
    for img in &result.images {
        assert_eq!(
            common::epub::entry(&result.epub, &format!("OEBPS/images/{}", img.name)).unwrap(),
            img.bytes
        );
    }
}

#[test]
fn the_title_card_becomes_the_cover() {
    let result = pdf("images.pdf");
    let f = &result.fountain_text;
    assert!(f.find("[[img: p001-01.png").unwrap() < f.find("INT. ARCHIVE ROOM - DAY").unwrap());
    assert_eq!(toc_titles(&result.epub), scene_headings(&result));
    assert_eq!(toc_of(&result.epub)[0].0, "sc-001");
    let body = body_of(&result.epub);
    assert!(!body.contains("p001-01.png"));
    assert!(body.contains(
        "<p class=\"figure w40\"><img src=\"../images/p002-01.png\" alt=\"Illustration\"/></p>"
    ));
    assert_book_carries(&result);
    let opf = opf(&result.epub);
    assert!(opf.contains("<meta name=\"cover\" content=\"img-p001-01\"/>"));
    assert!(opf.contains(
        "<item id=\"img-p001-01\" href=\"images/p001-01.png\" media-type=\"image/png\" properties=\"cover-image\"/>"
    ));
}

#[test]
fn with_promote_cover_image_off_the_title_card_opens_an_opening() {
    let result = pdf_with(
        "images.pdf",
        opts(format(|o| o.promote_cover_image = false)),
    );
    let mut want = vec!["Opening".to_owned()];
    want.extend(scene_headings(&result));
    assert_eq!(toc_titles(&result.epub), want);
    assert_eq!(toc_of(&result.epub)[1].0, "sc-002");
    let body = body_of(&result.epub);
    let re = regex::Regex::new(
        r#"<section class="scene" id="sc-001">\s*<p class="figure w100 figure-full"><img src="\.\./images/p001-01\.png" alt="Illustration"/></p>\s*</section>"#,
    )
    .unwrap();
    assert!(re.is_match(&body));
    assert!(body.contains(
        "<p class=\"figure w40\"><img src=\"../images/p002-01.png\" alt=\"Illustration\"/></p>"
    ));
    assert_book_carries(&result);
    let opf = opf(&result.epub);
    assert!(!opf.contains("name=\"cover\""));
    assert!(!opf.contains("cover-image"));
}

#[test]
fn with_include_images_off_the_book_is_the_one_a_script_without_pictures_gets() {
    let off = format(|o| o.include_images = false);
    let result = pdf_with("images.pdf", opts(off));
    assert_eq!(names(&result), ["p001-01.png", "p002-01.png"]);
    let toks = tokens(&result.fountain_text);
    let want: String = tokens_to_body(
        &toks,
        &BodyOptions {
            format: off,
            ..BodyOptions::default()
        },
    )
    .files
    .into_iter()
    .map(|f| f.xhtml)
    .collect();
    assert_eq!(body_of(&result.epub), want);
    assert_eq!(toc_titles(&result.epub), scene_headings(&result));
    assert_eq!(toc_of(&result.epub)[0].0, "sc-001");
    assert!(image_entries(&result.epub).is_empty());
    assert!(!opf(&result.epub).contains("image/png"));
    assert_eq!(
        text(&result.epub, "OEBPS/style.css"),
        screenplay_css(&off, false)
    );
}

#[test]
fn for_a_script_without_pictures_convert_pdf_adds_nothing_to_the_renderer_and_packager() {
    let result = pdf("screenplay.pdf");
    let toks = tokens(&result.fountain_text);
    let meta = BookMeta {
        title: result.title.clone(),
        author: result.author.clone(),
        language: None,
        identifier: Some(identifier(&result.epub)),
    };
    let want = build_epub(
        &meta,
        &tokens_to_body(&toks, &BodyOptions::default()),
        &DEFAULT_FORMAT_OPTIONS,
        &[],
        0,
    )
    .unwrap();
    let modified = regex::Regex::new(r#"<meta property="dcterms:modified">[^<]*</meta>"#).unwrap();
    let strip = |b: &[u8]| {
        modified
            .replace(&String::from_utf8_lossy(b), "")
            .into_owned()
    };
    let got = read_zip(&result.epub);
    let want = read_zip(&want);
    assert_eq!(
        got.iter().map(|e| &e.name).collect::<Vec<_>>(),
        want.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    for (g, w) in got.iter().zip(&want) {
        if w.dir {
            continue;
        }
        assert_eq!(strip(&g.data), strip(&w.data), "{}", w.name);
    }
}

#[test]
fn a_healthy_picture_document_warns_about_nothing() {
    assert_eq!(pdf("images.pdf").warnings, Vec::<String>::new());
}

/// images.pdf with its first image XObject's payload clobbered in place.
fn broken_picture() -> Vec<u8> {
    let mut pdf = bytes("images.pdf");
    let subtype = regex::bytes::Regex::new(r"/Subtype\s*/Image").unwrap();
    let at = subtype.find(&pdf).expect("an image XObject").start();
    let find = |needle: &[u8], from: usize, hay: &[u8]| {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
            .expect("found")
    };
    let from = find(b"stream", at, &pdf) + "stream\n".len();
    let to = find(b"endstream", from, &pdf);
    assert!(at > 0);
    assert!(to - from > 100);
    pdf[from + 4..to - 4].fill(0x41);
    pdf
}

#[test]
fn a_picture_the_pdf_layer_cannot_decode_is_reported_not_silently_missing() {
    let result = convert_pdf(broken_picture(), ConvertOptions::default()).unwrap();
    assert_eq!(names(&result), ["p002-01.png"]);
    assert_eq!(
        result.warnings,
        ["1 picture could not be read out of the PDF, so it was left out."]
    );
}

#[test]
fn the_scanned_guard_fires_before_any_picture_is_carried() {
    let err = convert_pdf(bytes("blank-pages.pdf"), ConvertOptions::default()).unwrap_err();
    assert!(matches!(err, ConvertError::Scanned { .. }));
}

#[test]
fn fountain_input_carries_nothing_out() {
    let src = "Title: T\n\nINT. A - DAY\n\n[[img: p001-01.png w=71]]\n";
    assert!(fountain(src).images.is_empty());
    let plate = png_of([0, 0, 0]);
    let images = map(&[("p001-01.png", &plate)]);
    let result = convert_fountain(src, ConvertOptions::default(), Some(&images)).unwrap();
    assert!(result.images.is_empty());
}

// ── convertFountain renders the pictures it is handed ──

const LAB: &str = "Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\nAfter.\n";
const FIGURE: &str =
    "<p class=\"figure w40\"><img src=\"../images/p001-01.png\" alt=\"Illustration\"/></p>";

fn red() -> Vec<u8> {
    png_of([200, 10, 10])
}

#[test]
fn a_note_whose_picture_is_in_the_map_is_a_figure_and_the_book_carries_it() {
    let plate = red();
    let images = map(&[("p001-01.png", &plate)]);
    let epub = convert_fountain(LAB, ConvertOptions::default(), Some(&images))
        .unwrap()
        .epub;
    assert!(body_of(&epub).contains(FIGURE));
    assert_eq!(image_entries(&epub), ["OEBPS/images/p001-01.png"]);
    assert_eq!(
        common::epub::entry(&epub, "OEBPS/images/p001-01.png").unwrap(),
        plate
    );
}

#[test]
fn a_picture_the_map_holds_and_no_note_names_stays_out_of_the_book() {
    let images = map(&[("p001-01.png", &red()), ("p009-01.png", &red())]);
    let epub = convert_fountain(LAB, ConvertOptions::default(), Some(&images))
        .unwrap()
        .epub;
    assert_eq!(image_entries(&epub), ["OEBPS/images/p001-01.png"]);
}

#[test]
fn bytes_that_are_not_a_png_are_no_picture_whatever_the_name_says() {
    let images = map(&[("p001-01.png", &JPEG)]);
    let epub = convert_fountain(LAB, ConvertOptions::default(), Some(&images))
        .unwrap()
        .epub;
    assert!(!body_of(&epub).contains("class=\"figure"));
    assert!(image_entries(&epub).is_empty());
}

#[test]
fn with_include_images_off_the_map_changes_nothing() {
    let off = format(|o| o.include_images = false);
    let without = convert_fountain(LAB, opts(off), None).unwrap().epub;
    let images = map(&[("p001-01.png", &red())]);
    let with_map = convert_fountain(LAB, opts(off), Some(&images))
        .unwrap()
        .epub;
    assert_eq!(body_of(&with_map), body_of(&without));
    assert!(!body_of(&with_map).contains("class=\"figure"));
    assert!(image_entries(&with_map).is_empty());
}

// ── the preview is handed the pictures the book is ──

fn data_srcs(html: &str) -> usize {
    html.matches("<img src=\"data:image/png;base64,").count()
}

#[test]
fn a_pdf_conversions_preview_shows_the_pictures_its_pages_show() {
    for (fmt, shown) in [
        (DEFAULT_FORMAT_OPTIONS, 1),
        (format(|o| o.promote_cover_image = false), 2),
    ] {
        let result = pdf_with("images.pdf", opts(fmt));
        let toks = tokens(&result.fountain_text);
        assert_eq!(data_srcs(&result.preview_html), shown);
        assert_eq!(
            result.preview_html,
            tokens_to_preview_html(&toks, &fmt, &picture_list(&result))
        );
    }
}

#[test]
fn with_include_images_off_a_pdf_conversions_preview_shows_none() {
    let off = format(|o| o.include_images = false);
    let result = pdf_with("images.pdf", opts(off));
    let toks = tokens(&result.fountain_text);
    assert_eq!(data_srcs(&result.preview_html), 0);
    assert_eq!(
        result.preview_html,
        tokens_to_preview_html(&toks, &off, &[])
    );
}

#[test]
fn a_script_without_pictures_gets_the_preview_it_got_before() {
    let result = pdf("screenplay.pdf");
    let toks = tokens(&result.fountain_text);
    assert_eq!(
        result.preview_html,
        tokens_to_preview_html(&toks, &DEFAULT_FORMAT_OPTIONS, &[])
    );
}

#[test]
fn a_fountains_preview_shows_the_pictures_it_is_handed_after_the_same_filter() {
    let src = "Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\n\
               Between.\n\n[[img: p002-01.png w=60]]\n\nAfter.\n";
    let plate = red();
    let images = map(&[("p001-01.png", &plate), ("p002-01.png", &JPEG)]);
    let preview = convert_fountain(src, ConvertOptions::default(), Some(&images))
        .unwrap()
        .preview_html;
    assert!(preview.contains(&format!(
        "<p class=\"figure w40\"><img src=\"data:image/png;base64,{}\" alt=\"Illustration\"/></p>",
        base64(&plate)
    )));
    assert_eq!(data_srcs(&preview), 1);
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in b.chunks(3) {
        let n = chunk.len();
        let v = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= n {
                out.push(A[((v >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ── the MOBI is handed the pictures the book is ──

/// SHA-256 of each book's .mobi with the PDB dates zeroed, pinned in
/// tests/convert.test.ts (made by the code at 0c71095, the last commit
/// before a MOBI could carry a picture).
const BEFORE_PICTURES_SCREENPLAY: &str =
    "7bf143e1fede70bbc73b12ad6a9c43d65aa7fe59aaf6c66cd53cb357951d75e4";
const BEFORE_PICTURES_IMAGES_OFF: &str =
    "4fc4e4d51fd22649c4f99421281bc946ebcf6643576131213e017166741f24d1";

fn masked_sha(b: &[u8]) -> String {
    sha256_hex(&mask_pdb_dates(b))
}

fn mobi_opts(fmt: FormatOptions, pictures: Option<bool>) -> ConvertOptions<'static> {
    ConvertOptions {
        mobi: true,
        mobi_pictures: pictures,
        format: fmt,
        ..ConvertOptions::default()
    }
}

#[test]
fn a_pdf_conversions_mobi_carries_the_epubs_pictures_in_mobi_art_order() {
    let result = pdf_with("images.pdf", mobi_opts(DEFAULT_FORMAT_OPTIONS, Some(true)));
    let toks = tokens(&result.fountain_text);
    let order = mobi_art(&toks, &DEFAULT_FORMAT_OPTIONS, &image_set(&result)).order;
    assert_eq!(order, ["p001-01.png", "p002-01.png"]);
    let shipped: Vec<Vec<u8>> = order
        .iter()
        .map(|n| common::epub::entry(&result.epub, &format!("OEBPS/images/{n}")).unwrap())
        .collect();
    assert_eq!(mobi_pictures(result.mobi.as_ref().unwrap()), shipped);
}

#[test]
fn a_pdf_conversions_mobi_names_the_epubs_cover_and_keeps_that_card_on_its_page() {
    let on = pdf_with("images.pdf", mobi_opts(DEFAULT_FORMAT_OPTIONS, Some(true)));
    let off = pdf_with(
        "images.pdf",
        mobi_opts(format(|o| o.promote_cover_image = false), Some(true)),
    );
    let re =
        regex::Regex::new(r#"<item [^>]*href="images/p001-01\.png"[^>]*properties="cover-image""#)
            .unwrap();
    assert!(re.is_match(&opf(&on.epub)));
    let on_m = on.mobi.as_ref().unwrap();
    let off_m = off.mobi.as_ref().unwrap();
    assert_eq!(exth(on_m, 201), Some(vec![0, 0, 0, 0]));
    assert_eq!(
        mobi_pictures(on_m)[0],
        common::epub::entry(&on.epub, "OEBPS/images/p001-01.png").unwrap()
    );
    assert_eq!(exth(off_m, 201), None);
    assert_eq!(mobi_html(on_m), mobi_html(off_m));
    assert!(
        mobi_html(on_m)
            .contains("<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>")
    );
    assert_eq!(mobi_pictures(on_m), mobi_pictures(off_m));
    let on_rest: Vec<_> = exth_records(on_m)
        .into_iter()
        .filter(|r| r.kind != 201)
        .collect();
    assert_eq!(on_rest, exth_records(off_m));
    assert_eq!(on_m.len(), off_m.len() + 12);
}

#[test]
fn by_default_the_mobi_is_text_only_while_the_epub_keeps_its_pictures() {
    if MOBI_CARRIES_PICTURES {
        return;
    }
    let result = pdf_with("images.pdf", mobi_opts(DEFAULT_FORMAT_OPTIONS, None));
    assert_eq!(result.images.len(), 2);
    let m = result.mobi.as_ref().unwrap();
    assert!(mobi_pictures(m).is_empty());
    assert_eq!(exth(m, 201), None);
    assert!(!mobi_html(m).contains("recindex"));
    assert_eq!(masked_sha(m), BEFORE_PICTURES_IMAGES_OFF);
    assert!(opf(&result.epub).contains("properties=\"cover-image\""));
    assert!(common::epub::entry(&result.epub, "OEBPS/images/p002-01.png").is_some());
}

#[test]
fn and_from_a_fountain_with_its_pictures_handed_over_the_same() {
    if MOBI_CARRIES_PICTURES {
        return;
    }
    let pdf = pdf("images.pdf");
    let images: BTreeMap<String, Vec<u8>> = pdf
        .images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect();
    let result = convert_fountain(
        &pdf.fountain_text,
        mobi_opts(DEFAULT_FORMAT_OPTIONS, None),
        Some(&images),
    )
    .unwrap();
    let m = result.mobi.as_ref().unwrap();
    assert!(mobi_pictures(m).is_empty());
    assert_eq!(exth(m, 201), None);
    let opened = convert_fountain(
        &pdf.fountain_text,
        mobi_opts(DEFAULT_FORMAT_OPTIONS, Some(true)),
        Some(&images),
    )
    .unwrap();
    assert_eq!(mobi_pictures(opened.mobi.as_ref().unwrap()).len(), 2);
}

#[test]
fn with_include_images_off_the_mobi_is_the_book_from_before_pictures() {
    let result = pdf_with(
        "images.pdf",
        mobi_opts(format(|o| o.include_images = false), None),
    );
    assert_eq!(result.images.len(), 2);
    let m = result.mobi.as_ref().unwrap();
    assert!(mobi_pictures(m).is_empty());
    assert_eq!(masked_sha(m), BEFORE_PICTURES_IMAGES_OFF);
}

#[test]
fn a_script_without_pictures_gets_the_mobi_it_got_before() {
    let result = pdf_with("screenplay.pdf", mobi_opts(DEFAULT_FORMAT_OPTIONS, None));
    let m = result.mobi.as_ref().unwrap();
    assert!(mobi_pictures(m).is_empty());
    assert_eq!(masked_sha(m), BEFORE_PICTURES_SCREENPLAY);
}

/// Shown out of name order, one shown twice, a JPEG under a PNG's name, and
/// a picture no note names.
const MOBI_SRC: &str = "Title: T\n\nINT. LAB - DAY\n\n[[img: p002-01.png w=40]]\n\nBetween.\n\n\
                        [[img: p001-01.png w=60]]\n\nAgain.\n\n[[img: p002-01.png w=40]]\n\n\
                        [[img: p003-01.png w=50]]\n\nAfter.\n";

fn mobi_map() -> (Vec<u8>, Vec<u8>, BTreeMap<String, Vec<u8>>) {
    let red = png_of([200, 10, 10]);
    let blue = png_of([10, 10, 200]);
    let green = png_of([10, 200, 10]);
    let m = map(&[
        ("p001-01.png", &blue),
        ("p002-01.png", &red),
        ("p003-01.png", &JPEG),
        ("p009-01.png", &green),
    ]);
    (red, blue, m)
}

#[test]
fn the_mobi_carries_what_the_epub_does_in_the_order_the_script_first_shows_it() {
    let (red, blue, m) = mobi_map();
    let result = convert_fountain(
        MOBI_SRC,
        mobi_opts(DEFAULT_FORMAT_OPTIONS, Some(true)),
        Some(&m),
    )
    .unwrap();
    assert_eq!(mobi_pictures(result.mobi.as_ref().unwrap()), [red, blue]);
    assert_eq!(
        image_entries(&result.epub),
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
}

#[test]
fn with_include_images_off_the_map_changes_nothing_in_the_mobi() {
    let (_, _, m) = mobi_map();
    let off = format(|o| o.include_images = false);
    let without = convert_fountain(MOBI_SRC, mobi_opts(off, None), None).unwrap();
    let with_map = convert_fountain(MOBI_SRC, mobi_opts(off, None), Some(&m)).unwrap();
    let w = with_map.mobi.as_ref().unwrap();
    assert!(mobi_pictures(w).is_empty());
    assert_eq!(
        mask_pdb_dates(w),
        mask_pdb_dates(without.mobi.as_ref().unwrap())
    );
}

// ── a refused document does not pay for its pictures ──

/// images.pdf with every text block run through `edit`, byte for byte. The
/// count is asserted: a regenerated fixture that writes its text
/// differently has to fail here.
fn edited(edit: impl Fn(&mut [u8])) -> Vec<u8> {
    let mut pdf = bytes("images.pdf");
    let re = regex::bytes::Regex::new(r"(?s-u)\nBT\n/F1 12 Tf\n.*?\nET\n").unwrap();
    let blocks: Vec<(usize, usize)> = re
        .find_iter(&pdf)
        .map(|m| (m.start() + 1, m.end() - 1))
        .collect();
    assert_eq!(blocks.len(), 5);
    for (from, to) in blocks {
        edit(&mut pdf[from..to]);
    }
    pdf
}

/// Every word gone: spaces in place of each text block.
fn scan() -> Vec<u8> {
    edited(|b| b.fill(0x20))
}

/// Every capital in every string turned to `x`.
fn disguised() -> Vec<u8> {
    edited(|b| {
        let mut in_string = false;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if in_string && c == 0x5c {
                i += 2;
                continue;
            }
            if c == 0x28 {
                in_string = true;
            } else if c == 0x29 {
                in_string = false;
            } else if in_string && c.is_ascii_uppercase() {
                b[i] = 0x78;
            }
            i += 1;
        }
    })
}

/// `convert_pdf`, with every picture fetch counted, and how many there
/// were: the Rust form of the TypeScript's two spies.
fn counted(pdf: Vec<u8>, force: bool) -> (Result<ConvertResult, ConvertError>, usize) {
    let doc = sp_pdf::Document::load(pdf).expect("a PDF");
    let mut reader = doc.reader();
    let facts = sp_pdf::read_pages(&doc, &mut reader);
    // The pages sp-pdf hands over first carry no pixels: whatever is
    // encoded was fetched.
    assert!(facts.pages.iter().all(|p| p.images.is_empty()));
    let fetches = Cell::new(0);
    let fetch = |id: &str| {
        fetches.set(fetches.get() + 1);
        sp_pdf::image_fact(&reader, id)
    };
    let result = convert_facts_with(
        &facts,
        ConvertOptions {
            force,
            ..ConvertOptions::default()
        },
        Some(&fetch),
    );
    (result, fetches.get())
}

#[test]
fn the_count_sees_the_work_when_a_document_is_converted() {
    let (result, fetches) = counted(bytes("images.pdf"), false);
    let result = result.unwrap();
    assert_eq!(result.images.len(), 2);
    assert!(fetches > 0);
    // And it is convert_pdf's answer.
    assert_eq!(names(&result), names(&pdf("images.pdf")));
}

#[test]
fn a_scan_is_refused_without_fetching_a_single_picture() {
    let (result, fetches) = counted(scan(), false);
    assert!(matches!(result.unwrap_err(), ConvertError::Scanned { .. }));
    assert_eq!(fetches, 0);
    // convert_pdf refuses it the same way.
    assert!(matches!(
        convert_pdf(scan(), ConvertOptions::default()).unwrap_err(),
        ConvertError::Scanned { .. }
    ));
}

#[test]
fn a_document_that_is_not_a_screenplay_is_refused_the_same_way() {
    let (result, fetches) = counted(disguised(), false);
    assert_eq!(result.unwrap_err(), ConvertError::NotAScreenplay);
    assert_eq!(fetches, 0);
    assert_eq!(
        convert_pdf(disguised(), ConvertOptions::default()).unwrap_err(),
        ConvertError::NotAScreenplay
    );
}

#[test]
fn both_edited_documents_still_carry_the_pictures_they_did_not_pay_for() {
    let facts = sp_pdf::read(scan()).expect("a PDF");
    assert!(
        !sp_parser::extract::extract_document(&facts, None, None)
            .images
            .is_empty()
    );
    let forced = convert_pdf(
        disguised(),
        ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        },
    )
    .unwrap();
    assert_eq!(names(&forced), ["p001-01.png", "p002-01.png"]);
}

// ── the cap warning, wired up (real, counts only) ──

#[test]
fn a_document_over_the_cap_says_how_many_it_has_and_how_many_it_kept() {
    let path = private("pitchdeck.pdf");
    // The picture budget (IMAGE_BUDGET_MS, 20 s) is wall-clock time; the
    // workspace builds the pixel crates optimized even in test builds
    // (rust/Cargo.toml), without which a debug run kept only 33.
    if !path.exists() {
        return;
    }
    let result = convert_pdf(
        std::fs::read(path).unwrap(),
        ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result.images.len(), 60);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("Only the first 60 were kept."))
    );
}

// ── the image warnings ──

fn stats(over: u32, failed: u32) -> ImageStats {
    ImageStats {
        failed,
        decoders_unavailable: None,
        over_cap: over,
        dropped_by: DroppedBy::default(),
    }
}

fn carried(n: usize) -> Vec<sp_epub::BookImage> {
    (0..n)
        .map(|i| sp_epub::BookImage {
            name: format!("p00{i}-01.png"),
            bytes: vec![0],
        })
        .collect()
}

fn undecodable(failed: u32) -> ImageStats {
    ImageStats {
        decoders_unavailable: Some(true),
        ..stats(0, failed)
    }
}

#[test]
fn silence_when_every_picture_was_carried() {
    assert!(image_warnings(&stats(0, 0), &carried(12)).is_empty());
}

#[test]
fn the_cap_says_how_many_were_found_and_how_many_made_it() {
    let w = &image_warnings(&stats(13, 0), &carried(60))[0];
    assert!(w.contains("73"));
    assert!(w.contains("60"));
    for word in ["overCap", "MAX_IMAGES", "undefined", "NaN"] {
        assert!(!w.contains(word));
    }
}

#[test]
fn a_lost_picture_is_reported_even_when_others_were_kept() {
    let w = &image_warnings(&stats(0, 3), &carried(5))[0];
    assert!(w.contains('3'));
    assert!(w.contains("pictures"));
}

#[test]
fn one_lost_picture_is_singular() {
    let w = &image_warnings(&stats(0, 1), &[])[0];
    assert!(w.contains("1 picture "));
    assert!(!w.contains("1 picture s"));
}

#[test]
fn both_counts_are_reported_cap_first() {
    let ws = image_warnings(&stats(2, 2), &carried(60));
    assert_eq!(ws.len(), 2);
    assert!(ws[0].contains("62"));
}

#[test]
fn no_em_dashes_and_nothing_a_reader_has_to_be_a_programmer_to_parse() {
    for w in image_warnings(&stats(9, 9), &carried(60)) {
        assert!(!w.contains('\u{2014}'));
        assert!(w.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit()));
        assert!(w.ends_with('.'));
    }
}

#[test]
fn a_decoder_failure_says_so_beside_the_count() {
    let ws = image_warnings(&undecodable(34), &[]);
    assert_eq!(ws.len(), 2);
    assert!(ws[0].contains("34"));
    assert!(ws[1].contains("JPEG2000"));
}

#[test]
fn and_says_nothing_at_all_when_no_picture_was_actually_lost() {
    assert!(image_warnings(&undecodable(0), &carried(3)).is_empty());
}

#[test]
fn the_cause_sentence_is_plain_english_too() {
    let ws = image_warnings(&undecodable(2), &[]);
    let cause = &ws[1];
    assert!(!cause.contains('\u{2014}'));
    for word in ["wasm", "wasmUrl", "pdf.js", "decodersUnavailable"] {
        assert!(!cause.contains(word));
    }
    assert!(cause.ends_with('.'));
}
