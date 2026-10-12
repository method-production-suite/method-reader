//! Ports tests/mobi.test.ts: the MOBI HTML mapping, the container, page
//! markers, emphasis, font shifts, pictures and the EXTH 201 cover.
//!
//! Sources are tokenized by sp-fountain; the EPUB renderer's picture
//! answers come from tests/data/mobi-sources.json (tests/common/sources.rs). The two Calibre
//! round trips are in tests/mobi_calibre.rs, and the images.pdf ones read
//! the oracle's tokens and pictures.

mod common;

use std::collections::HashSet;

use common::sources::{SOURCES, entry, tokens};
use common::*;
use sp_mobi::{MobiArt, MobiError, MobiInput, MobiMeta, build_mobi, mobi_art, tokens_to_mobi_html};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions};

const SAMPLE: &str = "INT. KITCHEN - DAY\n\nJack enters, exhausted.\n\n@JACK\n(tired)\nLong day. 5 < 6 & \"quotes\" too.\n\nEXT. YARD - NIGHT\n\n> FADE OUT.\n";

const OPTS: FormatOptions = DEFAULT_FORMAT_OPTIONS;

fn with(f: impl FnOnce(&mut FormatOptions)) -> FormatOptions {
    let mut o = DEFAULT_FORMAT_OPTIONS;
    f(&mut o);
    o
}

fn meta(title: &str, author: Option<&str>) -> MobiMeta {
    MobiMeta {
        title: title.to_owned(),
        author: author.map(str::to_owned),
    }
}

fn html_of(source: &str, title: &str, author: Option<&str>, format: &FormatOptions) -> String {
    tokens_to_mobi_html(
        &tokens(source),
        &meta(title, author),
        format,
        &MobiArt::default(),
    )
}

fn sample_html() -> String {
    html_of(SAMPLE, "Test Script", Some("Jane Doe"), &OPTS)
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

const PB: &str = "<mbp:pagebreak/>";

/// Enough of a PNG to tell one picture from another.
fn png(tail: &[u8]) -> Vec<u8> {
    [&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a][..], tail].concat()
}

fn input(html: String) -> MobiInput {
    MobiInput {
        title: "Test Script".into(),
        author: Some("Jane Doe".into()),
        html,
        time: Some(1_767_225_600),
        ..Default::default()
    }
}

// ── MOBI HTML mapping ──

#[test]
fn title_page_is_centered_and_followed_by_a_page_break() {
    let html = sample_html();
    assert!(html.contains("<center>"));
    assert!(html.contains("Test Script"));
    assert!(html.contains("Jane Doe"));
    assert!(html.contains(PB));
}

#[test]
fn scene_headings_are_bold_paragraphs() {
    assert!(sample_html().contains("<p><b>INT. KITCHEN - DAY</b></p>"));
}

#[test]
fn dialogue_renders_as_a_blockquote_with_bold_cue_and_italic_parenthetical() {
    assert!(sample_html().contains("<blockquote><b>JACK</b><br/><i>(tired)</i><br/>Long day."));
}

#[test]
fn transitions_are_right_aligned_without_the_forcing_prefix() {
    assert!(sample_html().contains("<p align=\"right\">FADE OUT.</p>"));
}

#[test]
fn text_is_html_escaped() {
    assert!(sample_html().contains("5 &lt; 6 &amp; &quot;quotes&quot; too."));
}

#[test]
fn mini_slugs_read_as_bold_paragraphs_too() {
    let html = html_of(
        "INT. STORE - NIGHT\n\nThe gate rattles.\n\n.LATER\n\nStill on.\n",
        "T",
        None,
        &OPTS,
    );
    assert!(html.contains("<p><b>LATER</b></p>"));
}

#[test]
fn scene_page_breaks_emits_a_break_before_every_scene_heading_except_the_first() {
    let src = "INT. A - DAY\n\nAction.\n\nINT. B - NIGHT\n\nMore.\n";
    let off = html_of(src, "T", None, &OPTS);
    let on = html_of(src, "T", None, &with(|o| o.scene_page_breaks = true));
    // The title page always contributes exactly one pagebreak of its own.
    assert_eq!(count(&off, PB), 1);
    assert_eq!(count(&on, PB), 2);
}

#[test]
fn include_title_page_off_drops_the_title_page_and_its_break() {
    let on = html_of(SAMPLE, "Test Script", Some("Jane Doe"), &OPTS);
    assert!(on.contains("Test Script"));
    assert!(on.contains("Written by"));
    assert_eq!(count(&on, PB), 1);

    let off = html_of(
        SAMPLE,
        "Test Script",
        Some("Jane Doe"),
        &with(|o| o.include_title_page = false),
    );
    assert!(!off.contains("Written by"));
    assert!(!off.contains("<font size=\"+2\">"));
    // No title page means no title-page break either.
    assert_eq!(count(&off, PB), 0);
}

#[test]
fn scene_page_breaks_breaks_before_the_first_scene_when_body_content_precedes_it() {
    let src = "Black screen. A hum builds.\n\nINT. A - DAY\n\nAction.\n";
    let on = html_of(src, "T", None, &with(|o| o.scene_page_breaks = true));
    assert_eq!(count(&on, PB), 2);
    assert!(on.contains("<mbp:pagebreak/>\n<p><b>INT. A - DAY</b></p>"));
}

#[test]
fn scene_page_breaks_leaves_no_blank_leading_page_when_a_scene_opens_the_book() {
    let src = "INT. A - DAY\n\nAction.\n";
    let with_title = html_of(src, "T", None, &with(|o| o.scene_page_breaks = true));
    assert_eq!(count(&with_title, PB), 1);
    let no_title = html_of(
        src,
        "T",
        None,
        &with(|o| {
            o.scene_page_breaks = true;
            o.include_title_page = false;
        }),
    );
    assert_eq!(count(&no_title, PB), 0);
}

#[test]
fn scene_page_breaks_does_not_break_before_a_mini_slug() {
    let src = "INT. A - DAY\n\nAction.\n\n.LATER\n\nMore.\n\nINT. B - NIGHT\n\nEnd.\n";
    let toks = tokens(src);
    assert_eq!(
        toks.iter()
            .find(|t| t.text.as_deref() == Some("LATER"))
            .map(|t| t.kind.as_str()),
        Some("scene_heading")
    );
    let on = html_of(src, "T", None, &with(|o| o.scene_page_breaks = true));
    assert_eq!(count(&on, PB), 2);
    assert!(!on.contains("<mbp:pagebreak/>\n<p><b>LATER</b></p>"));
}

// ── MOBI container ──

fn build() -> Vec<u8> {
    build_mobi(&input(sample_html())).unwrap()
}

#[test]
fn pdb_header_carries_book_mobi_and_consistent_record_offsets() {
    let mobi = build();
    assert_eq!(ascii(&mobi, 60, 8), "BOOKMOBI");
    let offsets = record_offsets(&mobi);
    assert!(offsets.len() >= 5);
    for i in 1..offsets.len() {
        assert!(offsets[i] > offsets[i - 1]);
    }
    assert!(*offsets.last().unwrap() < mobi.len());
}

#[test]
fn record_0_has_uncompressed_palmdoc_and_a_mobi_6_header() {
    let mobi = build();
    let r0 = record_offsets(&mobi)[0];
    assert_eq!(u16_at(&mobi, r0), 1);
    assert_eq!(ascii(&mobi, r0 + 16, 4), "MOBI");
    assert_eq!(u32_at(&mobi, r0 + 24), 2);
    assert_eq!(u32_at(&mobi, r0 + 28), 65001);
    assert_eq!(u32_at(&mobi, r0 + 36), 6);
}

#[test]
fn text_records_reassemble_to_the_exact_source_html() {
    let mobi = build();
    let html = sample_html();
    let r0 = record_offsets(&mobi)[0];
    assert_eq!(u32_at(&mobi, r0 + 4) as usize, html.len());
    assert_eq!(mobi_html(&mobi), html);
}

#[test]
fn full_name_and_exth_metadata_are_present() {
    let mobi = build();
    let r0 = record_offsets(&mobi)[0];
    let name_offset = u32_at(&mobi, r0 + 84) as usize;
    let name_length = u32_at(&mobi, r0 + 88) as usize;
    assert_eq!(ascii(&mobi, r0 + name_offset, name_length), "Test Script");
    assert_eq!(ascii(&mobi, r0 + 248, 4), "EXTH");
    let whole = String::from_utf8_lossy(&mobi);
    assert!(whole.contains("Jane Doe"));
    assert!(whole.contains("EBOK"));
}

#[test]
fn flis_fcis_and_eof_trailer_records_are_in_place() {
    let mobi = build();
    let offsets = record_offsets(&mobi);
    let n = offsets.len();
    assert_eq!(ascii(&mobi, offsets[n - 3], 4), "FLIS");
    assert_eq!(ascii(&mobi, offsets[n - 2], 4), "FCIS");
    assert_eq!(
        &mobi[offsets[n - 1]..offsets[n - 1] + 4],
        &[0xe9, 0x8e, 0x0d, 0x0a]
    );
}

#[test]
fn record_unique_ids_stay_unique_past_128_text_records() {
    let long = (0..20000)
        .map(|i| {
            format!("<p>Scene line {i}, padding the record count well past the byte boundary.</p>")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mobi = build_mobi(&MobiInput {
        title: "Very Long".into(),
        author: Some("A".into()),
        html: long,
        time: Some(0),
        ..Default::default()
    })
    .unwrap();
    let count = u16_at(&mobi, 76);
    assert!(count > 128);
    let uids: HashSet<u32> = (0..count)
        .map(|i| {
            let o = 78 + i * 8 + 5;
            (u32::from(mobi[o]) << 16) | (u32::from(mobi[o + 1]) << 8) | u32::from(mobi[o + 2])
        })
        .collect();
    assert_eq!(uids.len(), count);
}

#[test]
fn multi_record_text_never_splits_a_utf8_codepoint() {
    let long = (0..300)
        .map(|i| format!("<p>Line {i} \u{2014} \u{201c}quoted\u{201d} he said.</p>"))
        .collect::<Vec<_>>()
        .join("\n");
    let mobi = build_mobi(&MobiInput {
        title: "Long".into(),
        author: Some("A".into()),
        html: long,
        time: Some(0),
        ..Default::default()
    })
    .unwrap();
    let offsets = record_offsets(&mobi);
    let text_records = u16_at(&mobi, offsets[0] + 8);
    assert!(text_records > 1);
    for i in 1..=text_records {
        let end = offsets.get(i + 1).copied().unwrap_or(mobi.len());
        assert!(
            std::str::from_utf8(&mobi[offsets[i]..end]).is_ok(),
            "record {i}"
        );
    }
}

// ── page markers ──

fn marker_html(source: &str) -> String {
    html_of(source, "T", Some("A"), &OPTS)
}

#[test]
fn marker_rides_inside_the_following_block() {
    let html = marker_html("INT. KITCHEN - DAY\n\n= pg 47\n\nJack enters, exhausted.\n");
    assert!(!html.contains("<p class=\"page-marker\">"));
    assert!(html.contains("page-marker"));
    assert!(html.contains("<p><span class=\"page-marker\">"));
}

#[test]
fn marker_carries_no_epub3_pagebreak_semantics() {
    let html = marker_html("INT. KITCHEN - DAY\n\n= pg 47\n\nJack enters.\n");
    assert!(!html.contains("epub:type"));
    assert!(!html.contains("doc-pagebreak"));
    assert!(!html.contains("id=\"pg47\""));
}

#[test]
fn a_trailing_marker_with_nothing_after_it_is_dropped() {
    let html = marker_html("INT. KITCHEN - DAY\n\nJack enters.\n\n= pg 48\n");
    assert!(!html.contains("page-marker"));
}

// ── inline emphasis ──

#[test]
fn underscores_become_u() {
    assert!(
        html_of(
            "INT. A - DAY\n\nThe sign reads _DO NOT ENTER_.\n",
            "T",
            None,
            &OPTS
        )
        .contains("<u>DO NOT ENTER</u>")
    );
}

#[test]
fn mixed_marks_unwrap_with_the_underscore_innermost() {
    assert!(
        html_of(
            "INT. A - DAY\n\nThe stamp is **_VOID_** now.\n",
            "T",
            None,
            &OPTS
        )
        .contains("<b><u>VOID</u></b>")
    );
    assert!(
        html_of(
            "INT. A - DAY\n\nAll three: ***_LOUD_*** here.\n",
            "T",
            None,
            &OPTS
        )
        .contains("<b><i><u>LOUD</u></i></b>")
    );
}

#[test]
fn bold_italic_and_bold_italic_keep_their_tags() {
    let out = html_of(
        "INT. A - DAY\n\nA ***b1*** and **c1** and *d1*.\n",
        "T",
        None,
        &OPTS,
    );
    assert!(out.contains("<b><i>b1</i></b>"));
    assert!(out.contains("<b>c1</b>"));
    assert!(out.contains("<i>d1</i>"));
}

// ── font shifts ──

#[test]
fn a_size_step_wraps_the_action_in_a_font_tag() {
    assert!(
        html_of("INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n", "T", None, &OPTS)
            .contains("<p><font size=\"+2\">A SIGN.</font></p>")
    );
}

#[test]
fn all_three_steps_map() {
    assert!(
        html_of("INT. A - DAY\n\n[[fmt: -1]]\nSmall.\n", "T", None, &OPTS)
            .contains("<font size=\"-1\">")
    );
    assert!(
        html_of("INT. A - DAY\n\n[[fmt: +1]]\nMedium.\n", "T", None, &OPTS)
            .contains("<font size=\"+1\">")
    );
}

#[test]
fn dialogue_size_steps_land_too() {
    assert!(
        html_of(
            "INT. A - DAY\n\n@WREN\n[[fmt: +1]] Read it back.\n",
            "T",
            None,
            &OPTS
        )
        .contains("<font size=\"+1\">Read it back.</font>")
    );
}

#[test]
fn family_is_dropped() {
    let out = html_of("INT. A - DAY\n\n[[fmt: sans]]\nCHYRON.\n", "T", None, &OPTS);
    assert!(out.contains("<p>CHYRON.</p>"));
    assert!(!out.contains("face="));
    assert!(!out.contains("fmt-sans"));
}

#[test]
fn a_family_and_size_note_keeps_the_size() {
    assert!(
        html_of(
            "INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n",
            "T",
            None,
            &OPTS
        )
        .contains("<p><font size=\"+2\">A SIGN.</font></p>")
    );
}

#[test]
fn with_the_knob_off_the_note_vanishes_and_no_font_tag_appears() {
    let out = html_of(
        "INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n",
        "T",
        None,
        &with(|o| o.preserve_font_shifts = false),
    );
    assert!(out.contains("<p>A SIGN.</p>"));
    assert!(!out.contains("<font size=\"+2\">A SIGN."));
}

#[test]
fn notes_are_invisible_here_too() {
    let out = html_of(
        "INT. A - DAY\n\nHe leaves [[check this]] quickly.\n",
        "T",
        None,
        &OPTS,
    );
    assert!(out.contains("He leaves quickly."));
    assert!(!out.contains("[["));
}

#[test]
fn an_image_note_renders_nothing_when_no_pictures_are_handed_over() {
    for src in [
        "INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n",
        "INT. A - DAY\n\n@WREN\nHer line.\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n",
        "INT. A - DAY\n\n[[img: p001-01.png w=50]]\nSomeone waits.\n",
    ] {
        let out = html_of(src, "T", None, &OPTS);
        assert!(out.contains("Someone waits."), "{src}");
        assert!(!out.contains("img:"), "{src}");
        assert!(!out.contains("p001-01.png"), "{src}");
    }
}

// ── mobiArt ──

fn have() -> HashSet<String> {
    ["p001-01.png", "p002-01.png"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

fn off() -> FormatOptions {
    with(|o| o.include_images = false)
}

#[test]
fn mobi_art_lists_pictures_in_first_shown_order_each_once() {
    let src = "INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n[[img: p001-01.png w=60]]\n\n[[img: p002-01.png w=42]]\n\nEnd.\n";
    assert_eq!(
        mobi_art(&tokens(src), &OPTS, &have()).order,
        vec!["p002-01.png", "p001-01.png"]
    );
}

#[test]
fn mobi_art_asks_the_epubs_question() {
    let mut have_ab = have();
    have_ab.insert("a/b.png".to_owned());
    let src = "INT. LAB - DAY\n\n[[img: p009-01.png w=50]]\n\n[[img: a/b.png w=50]]\n\n[[img: p001-01.png w=50]]\nGlued.\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n[[img: p001-01.png w=100 full]]\n\nEnd.\n";
    for (key, format) in [("on/haveAb", OPTS), ("off/haveAb", off())] {
        let epub = &entry(src).epub[key].used;
        assert_eq!(
            &mobi_art(&tokens(src), &format, &have_ab).order,
            epub,
            "{key}"
        );
    }
    assert_eq!(
        mobi_art(&tokens(src), &OPTS, &have_ab).order,
        vec!["p002-01.png", "p001-01.png"]
    );
    assert!(mobi_art(&tokens(src), &off(), &have_ab).order.is_empty());
}

const CARD: &str =
    "[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nEnd.\n";

#[test]
fn the_cover_is_the_title_card_named_by_its_place_in_the_order() {
    let art = mobi_art(&tokens(CARD), &OPTS, &have());
    assert_eq!(art.order, vec!["p001-01.png", "p002-01.png"]);
    assert_eq!(art.cover_index, Some(0));
}

#[test]
fn the_cover_asks_the_epubs_cover_pick() {
    let sources = [
        CARD.to_owned(),
        format!("Opening line.\n\n{CARD}"),
        format!("INT. HALL - DAY\n\n{CARD}"),
        format!("[[img: p002-01.png w=42]]\n\n{CARD}"),
        "[[img: p009-01.png w=100 full]]\n\nINT. LAB - DAY\n\nEnd.\n".to_owned(),
    ];
    let formats = [
        ("on", OPTS),
        ("off", off()),
        ("noCover", with(|o| o.promote_cover_image = false)),
    ];
    let mut covers = 0;
    for src in &sources {
        for (key, format) in &formats {
            let art = mobi_art(&tokens(src), format, &have());
            let epub = &entry(src).epub[&format!("{key}/have")].cover;
            let mobi = art.cover_index.map(|i| art.order[i].clone());
            assert_eq!(&mobi, epub, "{key}: {src}");
            if mobi.is_some() {
                covers += 1;
            }
        }
    }
    // The control: exactly one script with both options on has a cover.
    assert_eq!(covers, 1);
}

// ── MOBI figures ──

fn render(src: &str, format: &FormatOptions, have: &HashSet<String>, title: bool) -> String {
    let toks = tokens(src);
    let mut f = *format;
    if !title {
        f.include_title_page = false;
    }
    tokens_to_mobi_html(&toks, &meta("T", None), &f, &mobi_art(&toks, &f, have))
}

fn render_on(src: &str) -> String {
    render(src, &OPTS, &have(), true)
}

fn scene(note: &str) -> String {
    format!("INT. LAB - DAY\n\nBefore.\n\n{note}\n\nAfter.\n")
}

#[test]
fn a_picture_is_a_centred_paragraph_naming_its_record_by_number() {
    assert!(render_on(&scene("[[img: p002-01.png w=42]]")).contains(
        "<p>Before.</p>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<p>After.</p>"
    ));
}

#[test]
fn the_number_is_mobi_arts_and_a_repeat_keeps_its_first_number() {
    let html = render_on(
        "INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n[[img: p001-01.png w=60]]\n\n[[img: p002-01.png w=42]]\n\nEnd.\n",
    );
    let nums: Vec<&str> = html
        .match_indices("recindex=\"")
        .map(|(i, m)| &html[i + m.len()..i + m.len() + 5])
        .collect();
    assert_eq!(nums, vec!["00001", "00002", "00001"]);
}

#[test]
fn no_width_and_no_height() {
    let html = render_on(&scene("[[img: p002-01.png w=42]]"));
    let imgs: Vec<&str> = html
        .match_indices("<img ")
        .map(|(i, _)| &html[i..i + html[i..].find('>').unwrap()])
        .collect();
    assert_eq!(imgs.len(), 1);
    for img in imgs {
        assert!(!img.contains("width") && !img.contains("height") && !img.contains("style"));
    }
}

#[test]
fn a_picture_the_caller_cannot_ship_or_pictures_off_renders_the_picture_free_body() {
    let plain = render_on("INT. LAB - DAY\n\nBefore.\n\nAfter.\n");
    assert_eq!(render_on(&scene("[[img: p009-01.png w=42]]")), plain);
    assert_eq!(
        render(
            &scene("[[img: p002-01.png w=42]]"),
            &OPTS,
            &HashSet::new(),
            true
        ),
        plain
    );
    assert_eq!(
        render(&scene("[[img: p002-01.png w=42]]"), &off(), &have(), true),
        render(
            "INT. LAB - DAY\n\nBefore.\n\nAfter.\n",
            &off(),
            &have(),
            true
        )
    );
}

#[test]
fn a_speech_before_a_picture_is_closed_before_it() {
    assert!(
        render_on("INT. LAB - DAY\n\n@WREN\nHer line.\n\n[[img: p002-01.png w=42]]\n\nAfter.\n")
            .contains("<blockquote><b>WREN</b><br/>Her line.</blockquote>\n<p align=\"center\"><img recindex=\"00001\"")
    );
}

#[test]
fn a_figure_declines_a_pending_page_marker() {
    let html =
        render_on("INT. LAB - DAY\n\nBefore.\n\n= pg 7\n\n[[img: p002-01.png w=42]]\n\nAfter.\n");
    assert!(
        html.contains("<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>")
    );
    assert!(
        html.contains(
            "<p><span class=\"page-marker\"><font size=\"-2\">7.</font></span>After.</p>"
        )
    );
}

#[test]
fn a_plate_ends_the_page_before_it_and_the_page_it_is_on() {
    assert!(render_on(&scene("[[img: p001-01.png w=100 full]]")).contains(
        "<p>Before.</p>\n<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>\n<p>After.</p>"
    ));
}

#[test]
fn an_ordinary_picture_breaks_nothing() {
    assert_eq!(
        count(&render_on(&scene("[[img: p002-01.png w=42]]")), PB),
        count(&render_on(&scene("")), PB)
    );
}

const PLATE_CARD: &str = "[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\nWork.\n";

#[test]
fn a_plate_at_the_start_of_the_book_has_no_blank_page_in_front() {
    let titled = render_on(PLATE_CARD);
    assert!(
        titled.contains("</center>\n<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00001\"")
    );
    assert_eq!(count(&titled, PB), 2);
    assert!(
        render(PLATE_CARD, &OPTS, &have(), false)
            .contains("<body>\n<p align=\"center\"><img recindex=\"00001\"")
    );
}

#[test]
fn a_title_card_that_is_the_cover_stays_on_its_page() {
    let cover = mobi_art(&tokens(PLATE_CARD), &OPTS, &have());
    assert_eq!(cover.cover_index, Some(0));
    let html = render_on(PLATE_CARD);
    assert!(html.contains("</center>\n<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>\n<p><b>INT. LAB - DAY</b></p>"));
    assert_eq!(
        html,
        render(
            PLATE_CARD,
            &with(|o| o.promote_cover_image = false),
            &have(),
            true
        )
    );
    // And the EPUB of the same script does leave it out.
    assert!(entry(PLATE_CARD).epub["on/have"].used.is_empty());
}

#[test]
fn a_plate_at_the_end_of_the_book_has_no_blank_page_after() {
    let html = render_on("INT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n");
    assert!(html.contains("<img recindex=\"00001\" alt=\"Illustration\"/></p>\n</body>"));
}

#[test]
fn never_two_breaks_in_a_row() {
    let src = "Opening.\n\n[[img: p001-01.png w=100 full]]\n\n[[img: p002-01.png w=100 full]]\n\nINT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n\nEXT. YARD - NIGHT\n\nRest.\n";
    let breaks = with(|o| o.scene_page_breaks = true);
    for format in [OPTS, breaks] {
        let html = render(src, &format, &have(), true);
        assert!(!html.contains("<mbp:pagebreak/>\n<mbp:pagebreak/>"));
        assert!(html.contains("<img recindex=\"00001\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00002\""));
        assert_eq!(count(&html, PB), 6);
    }
}

#[test]
fn sp_fountain_tokenizes_every_source_as_fountain_js_did() {
    for s in SOURCES.iter() {
        assert_eq!(
            sp_fountain::tokenize(&s.source).unwrap(),
            s.tokens,
            "{:?}",
            s.source
        );
    }
}

// ── The TypeScript's own answers for every source (the renderer's parity) ──

#[test]
fn every_source_renders_as_the_typescript_renders_it() {
    let formats = [
        ("on", OPTS),
        ("off", off()),
        ("breaks", with(|o| o.scene_page_breaks = true)),
        ("noTitle", with(|o| o.include_title_page = false)),
        ("noShifts", with(|o| o.preserve_font_shifts = false)),
        ("noCover", with(|o| o.promote_cover_image = false)),
    ];
    let mut checked = 0;
    for s in SOURCES.iter() {
        for (key, format) in &formats {
            let want = &s.mobi[*key];
            let toks = tokens(&s.source);
            let art = mobi_art(&toks, format, &have());
            assert_eq!(art.order, want.order, "{key}: {:?}", s.source);
            assert_eq!(art.cover_index, want.cover_index, "{key}: {:?}", s.source);
            let html = tokens_to_mobi_html(&toks, &meta("T", Some("A")), format, &art);
            assert_eq!(html, want.html, "{key}: {:?}", s.source);
            checked += 1;
        }
    }
    assert!(checked >= 200);
}

// ── image records ──

fn pictures() -> Vec<Vec<u8>> {
    vec![png(&[1, 2, 3]), png(&[4, 5])]
}

fn build_with(images: Vec<Vec<u8>>) -> Vec<u8> {
    build_mobi(&MobiInput {
        images,
        ..input(sample_html())
    })
    .unwrap()
}

#[test]
fn each_picture_is_a_record_of_its_own_after_the_text_and_before_flis() {
    let mobi = build_with(pictures());
    let recs = records(&mobi);
    let h = header(&mobi);
    assert_eq!(recs.len(), 1 + h.text_records + 2 + 3);
    assert_eq!(recs[1 + h.text_records], pictures()[0]);
    assert_eq!(recs[2 + h.text_records], pictures()[1]);
    assert_eq!(ascii(&recs[3 + h.text_records], 0, 4), "FLIS");
}

#[test]
fn the_palmdoc_count_still_counts_text_alone_and_the_text_reassembles() {
    let mobi = build_with(pictures());
    assert_eq!(header(&mobi).text_records, header(&build()).text_records);
    assert_eq!(mobi_html(&mobi), sample_html());
}

#[test]
fn field_108_is_an_absolute_record_number() {
    let mobi = build_with(pictures());
    let h = header(&mobi);
    assert_eq!(h.first_image as usize, 1 + h.text_records);
    assert_eq!(records(&mobi)[h.first_image as usize], pictures()[0]);
}

#[test]
fn field_194_reaches_the_last_picture() {
    let mobi = build_with(pictures());
    let h = header(&mobi);
    assert_eq!(h.first_content, 1);
    assert_eq!(h.last_content, h.text_records + 2);
    assert_eq!(records(&mobi)[h.last_content], pictures()[1]);
}

#[test]
fn field_80_is_the_first_record_that_is_not_text() {
    let mobi = build_with(pictures());
    let h = header(&mobi);
    assert_eq!(h.first_non_book as usize, 1 + h.text_records);
    assert_eq!(records(&mobi)[h.first_non_book as usize], pictures()[0]);
}

#[test]
fn flis_and_fcis_move_down_past_the_pictures() {
    let mobi = build_with(pictures());
    let h = header(&mobi);
    let recs = records(&mobi);
    assert_eq!(h.flis as usize, 1 + h.text_records + 2);
    assert_eq!(ascii(&recs[h.flis as usize], 0, 4), "FLIS");
    assert_eq!(h.fcis, h.flis + 1);
    assert_eq!(ascii(&recs[h.fcis as usize], 0, 4), "FCIS");
}

#[test]
fn a_recindex_the_records_cannot_answer_is_a_caller_bug() {
    let showing = |ns: &[&str]| {
        ns.iter()
            .map(|n| {
                format!("<p align=\"center\"><img recindex=\"{n}\" alt=\"Illustration\"/></p>")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let at = |html: String, images: Vec<Vec<u8>>| {
        build_mobi(&MobiInput {
            title: "T".into(),
            html,
            images,
            time: Some(0),
            ..Default::default()
        })
    };
    let err = |r: Result<Vec<u8>, MobiError>| r.unwrap_err().to_string();
    assert!(err(at(showing(&["00001", "00003"]), pictures())).contains("recindex=\"00003\""));
    assert!(err(at(showing(&["00000", "00002"]), pictures())).contains("recindex=\"00000\""));
    assert!(err(at(showing(&["00001"]), Vec::new())).contains("recindex=\"00001\""));
    assert!(at(showing(&["00001", "00002", "00001"]), pictures()).is_ok());
}

#[test]
fn a_large_picture_rides_whole() {
    let big: Vec<u8> = (0..2_000_000u32)
        .map(|i| (i.wrapping_mul(31) & 0xff) as u8)
        .collect();
    let mobi = build_with(vec![big.clone()]);
    let h = header(&mobi);
    assert_eq!(records(&mobi)[h.first_image as usize], big);
}

#[test]
fn without_pictures_108_says_none_and_80_and_194_are_where_the_text_ends() {
    let mobi = build();
    let h = header(&mobi);
    assert_eq!(h.first_image, 0xffff_ffff);
    assert_eq!(h.last_content, h.text_records);
    assert_eq!(h.first_non_book as usize, 1 + h.text_records);
    assert_eq!(h.flis, h.first_non_book);
}

#[test]
fn the_clock_moves_the_pdb_dates_and_nothing_else() {
    let at = |time| {
        build_mobi(&MobiInput {
            images: pictures(),
            time: Some(time),
            ..input(sample_html())
        })
        .unwrap()
    };
    let early = at(1_577_836_800); // 2020-01-01T00:00:00Z
    let late = at(1_790_858_096); // 2026-10-01T12:34:56Z
    assert_eq!(early.len(), late.len());
    let moved: Vec<usize> = (0..early.len()).filter(|&i| early[i] != late[i]).collect();
    assert!(!moved.is_empty());
    assert!(moved.iter().all(|&i| (36..44).contains(&i)));
    assert_eq!(u32_at(&early, 36), 1_577_836_800);
    assert_eq!(u32_at(&early, 40), 1_577_836_800);
}

#[test]
fn a_book_with_no_pictures_is_byte_for_byte_the_book_from_before_image_records() {
    // SHA-256 with the PDB dates zeroed, captured from the TypeScript writer
    // at 0c71095, the last commit before either end could carry a picture.
    const BEFORE: &str = "c77dd2c56516cc252110b3d5bf3cea230f66c42e869c0534fbe8f89e3826d53c";
    assert_eq!(sha256_hex(&mask_pdb_dates(&build())), BEFORE);
    assert_eq!(sha256_hex(&mask_pdb_dates(&build_with(Vec::new()))), BEFORE);
}

// ── the MOBI cover, EXTH 201 ──

fn cover_pictures() -> Vec<Vec<u8>> {
    vec![png(&[1, 2, 3]), png(&[4, 5]), png(&[6])]
}

fn build_cover(cover_index: Option<usize>, images: Vec<Vec<u8>>) -> Result<Vec<u8>, MobiError> {
    build_mobi(&MobiInput {
        images,
        cover_index,
        ..input(sample_html())
    })
}

#[test]
fn exth_201_names_the_cover_by_its_offset_from_the_first_image_record() {
    for i in 0..3 {
        let mobi = build_cover(Some(i), cover_pictures()).unwrap();
        let data = exth(&mobi, 201).unwrap();
        assert_eq!(data, vec![0, 0, 0, u8::try_from(i).unwrap()]);
        let offset = u32_at(&data, 0) as usize;
        assert_eq!(
            records(&mobi)[header(&mobi).first_image as usize + offset],
            cover_pictures()[i]
        );
    }
}

#[test]
fn without_a_cover_there_is_no_exth_201() {
    assert!(exth(&build_cover(None, cover_pictures()).unwrap(), 201).is_none());
    assert!(exth(&build_cover(None, Vec::new()).unwrap(), 201).is_none());
}

#[test]
fn the_exth_block_stays_whole_around_the_cover() {
    let without = build_cover(None, cover_pictures()).unwrap();
    let with_cover = build_cover(Some(1), cover_pictures()).unwrap();
    let r0 = |b: &[u8]| record_offsets(b)[0];
    let exth_at = |b: &[u8]| r0(b) + 16 + u32_at(b, r0(b) + 20) as usize;
    assert_eq!(u32_at(&with_cover, r0(&with_cover) + 20), 232);
    assert_eq!(u32_at(&with_cover, r0(&with_cover) + 128) & 0x40, 0x40);
    assert_eq!(exth_at(&with_cover), exth_at(&without));
    assert_eq!(
        u32_at(&with_cover, exth_at(&with_cover) + 8),
        u32_at(&without, exth_at(&without) + 8) + 1
    );
    assert_eq!(
        u32_at(&with_cover, exth_at(&with_cover) + 4),
        u32_at(&without, exth_at(&without) + 4) + 12
    );
    let kinds = |b: &[u8]| exth_records(b).iter().map(|r| r.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&without), vec![100, 501, 503, 524]);
    assert_eq!(kinds(&with_cover), vec![100, 201, 501, 503, 524]);
    let others: Vec<_> = exth_records(&with_cover)
        .into_iter()
        .filter(|r| r.kind != 201)
        .collect();
    assert_eq!(others, exth_records(&without));
    let name_at = |b: &[u8]| u32_at(b, r0(b) + 84) as usize;
    assert_eq!(name_at(&with_cover), name_at(&without) + 12);
    assert_eq!((name_at(&with_cover) - 248) % 4, 0);
    assert!(name_at(&with_cover) - 248 >= u32_at(&with_cover, exth_at(&with_cover) + 4) as usize);
    assert_eq!(
        u32_at(&with_cover, r0(&with_cover) + 88),
        u32_at(&without, r0(&without) + 88)
    );
    assert_eq!(
        ascii(&with_cover, r0(&with_cover) + name_at(&with_cover), 11),
        "Test Script"
    );
    let rec0 = |b: &[u8]| records(b)[0].clone();
    assert_eq!(rec0(&with_cover).len(), rec0(&without).len() + 12);
    assert_eq!(rec0(&with_cover).len() % 4, 0);
}

#[test]
fn every_other_record_and_header_field_is_the_book_without_a_cover() {
    let without = build_cover(None, cover_pictures()).unwrap();
    let with_cover = build_cover(Some(2), cover_pictures()).unwrap();
    assert_eq!(records(&with_cover)[1..], records(&without)[1..]);
    assert_eq!(header(&with_cover), header(&without));
    let r0 = records(&with_cover)[0].clone();
    let author_len = exth_records(&without)
        .iter()
        .find(|r| r.kind == 100)
        .unwrap()
        .data
        .len();
    let at = 248 + 12 + author_len + 8;
    let mut back = [&r0[..at], &r0[at + 12..]].concat();
    let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_be_bytes());
    put(&mut back, 84, u32_at(&r0, 84) - 12);
    put(&mut back, 248 + 4, u32_at(&r0, 248 + 4) - 12);
    put(&mut back, 248 + 8, u32_at(&r0, 248 + 8) - 1);
    assert_eq!(back, records(&without)[0]);
}

#[test]
fn a_cover_index_the_pictures_cannot_answer_is_a_caller_bug() {
    // usize keeps -1 and 0.5 out by type; the range check is the rest.
    assert!(
        build_cover(Some(3), cover_pictures())
            .unwrap_err()
            .to_string()
            .contains("cover")
    );
    assert!(
        build_cover(Some(0), Vec::new())
            .unwrap_err()
            .to_string()
            .contains("cover")
    );
    assert!(build_cover(Some(2), cover_pictures()).is_ok());
}
