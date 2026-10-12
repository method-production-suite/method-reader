//! Port of tests/epub.test.ts: the body, the stylesheet, the package, the
//! preview. Each `mod` is one of its describe blocks, in its order; the
//! reasons the TypeScript's comments give are kept where a check needs one.
//!
//! The scripts are fountain-js's tokens for the same sources, recorded until
//! sp-fountain has a tokenizer (common::tokens).

mod common;

use common::{body_of, default_body, each_rule, entry_text, matches, rule_for, tokens};
use sp_epub::{
    BodyOptions, BookBody, BookMeta, build_epub, default_css, screenplay_css, tokens_to_body,
    tokens_to_preview_html,
};
use sp_model::{
    CueAlignment, DEFAULT_FORMAT_OPTIONS, FontFamily, FormatOptions, resolve_format_options,
};

const SAMPLE: &str = "INT. KITCHEN - DAY

Jack enters, exhausted.

@JACK
(tired)
Long day. 5 < 6 & \"quotes\" too.

EXT. YARD - NIGHT #2#

The stars come out.

> FADE OUT.
";

fn sample_body() -> BookBody {
    default_body(SAMPLE)
}

fn opts(json: serde_json::Value) -> FormatOptions {
    resolve_format_options(Some(&json), &DEFAULT_FORMAT_OPTIONS)
}

fn css() -> String {
    default_css()
}

fn first_xhtml(src: &str) -> String {
    default_body(src).files[0].xhtml.clone()
}

/// `toEqual` on a toc: (title, href) pairs.
fn toc(body: &BookBody) -> Vec<(String, String)> {
    body.toc
        .iter()
        .map(|e| (e.title.clone(), e.href.clone()))
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

const EPOCH: i64 = 1_767_225_600;

fn meta(title: &str, author: Option<&str>) -> BookMeta {
    BookMeta {
        title: title.to_owned(),
        author: author.map(str::to_owned),
        ..BookMeta::default()
    }
}

// ── tokensToBody ─────────────────────────────────────────

mod tokens_to_body_ {
    use super::*;

    #[test]
    fn scenes_flow_together_in_a_single_body_file() {
        let body = sample_body();
        assert_eq!(body.files.len(), 1);
        assert_eq!(body.files[0].id, "body001");
        assert!(body.files[0].xhtml.contains("INT. KITCHEN - DAY"));
        assert!(body.files[0].xhtml.contains("EXT. YARD - NIGHT"));
    }

    #[test]
    fn each_scene_is_an_anchored_section_for_toc_navigation() {
        let body = sample_body();
        assert!(
            body.files[0]
                .xhtml
                .contains("<section class=\"scene\" id=\"sc-001\">")
        );
        assert!(
            body.files[0]
                .xhtml
                .contains("<section class=\"scene\" id=\"sc-002\">")
        );
        assert_eq!(
            toc(&body),
            pairs(&[
                ("INT. KITCHEN - DAY", "text/body001.xhtml#sc-001"),
                ("EXT. YARD - NIGHT", "text/body001.xhtml#sc-002"),
            ])
        );
    }

    #[test]
    fn oversized_scripts_split_into_multiple_files_at_scene_boundaries() {
        let body = tokens_to_body(
            &tokens(SAMPLE),
            &BodyOptions {
                max_file_bytes: 400,
                ..BodyOptions::default()
            },
        );
        let ids: Vec<&str> = body.files.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["body001", "body002"]);
        assert!(body.toc[0].href.contains("body001.xhtml#sc-001"));
        assert!(body.toc[1].href.contains("body002.xhtml#sc-002"));
    }

    #[test]
    fn scene_heading_leads_its_scene_bare_the_keep_is_the_css_chain() {
        let xhtml = &sample_body().files[0].xhtml;
        assert!(matches(
            xhtml,
            r#"<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. KITCHEN - DAY</h2>\s*<p class="action">Jack enters, exhausted\.</p>"#
        ));
        assert!(!xhtml.contains("keep-together"));
    }

    #[test]
    fn a_dialogue_block_follows_its_heading_directly_no_outer_wrapper() {
        let xhtml = first_xhtml("INT. CAR - DAY\n\n@DEV\nDrive.\n\nThey drive.\n");
        assert!(matches(
            &xhtml,
            r#"<h2 class="scene-heading">INT\. CAR - DAY</h2>\s*<div class="dialogue-block">"#
        ));
        assert!(!xhtml.contains("keep-together"));
    }

    #[test]
    fn heading_only_scene_renders_bare_without_error() {
        let xhtml = first_xhtml("INT. VOID - DAY\n\nEXT. VOID - NIGHT\n\nStars.\n");
        assert!(matches(
            &xhtml,
            r#"<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. VOID - DAY</h2>\s*</section>"#
        ));
    }

    #[test]
    fn opening_content_without_a_heading_renders_directly_in_its_section() {
        let xhtml = first_xhtml("Cold open action.\n\nINT. LAB - DAY\n\nWork.\n");
        assert!(matches(
            &xhtml,
            r#"<section class="scene" id="sc-001">\s*<p class="action">Cold open action\.</p>"#
        ));
    }

    #[test]
    fn dialogue_renders_inside_a_dialogue_block_with_cue_paren_line_classes() {
        let xhtml = &sample_body().files[0].xhtml;
        assert!(xhtml.contains("<div class=\"dialogue-block\">"));
        assert!(xhtml.contains("<p class=\"character\">JACK</p>"));
        assert!(xhtml.contains("<p class=\"parenthetical\">(tired)</p>"));
    }

    #[test]
    fn special_characters_are_xml_escaped() {
        assert!(
            sample_body().files[0]
                .xhtml
                .contains("5 &lt; 6 &amp; &quot;quotes&quot; too.")
        );
    }

    #[test]
    fn transition_strips_the_forcing_prefix() {
        assert!(
            sample_body().files[0]
                .xhtml
                .contains("<p class=\"transition\">FADE OUT.</p>")
        );
    }

    #[test]
    fn content_before_the_first_scene_heading_becomes_an_opening_section() {
        let body = default_body("Some cold-open action.\n\nINT. LAB - DAY\n\nWork.\n");
        assert!(body.files[0].xhtml.contains("Some cold-open action."));
        assert_eq!(body.toc[0].title, "Opening");
        assert_eq!(body.toc[1].title, "INT. LAB - DAY");
    }

    #[test]
    fn a_picture_note_before_the_first_heading_opens_no_section() {
        // A standalone note renders nothing, so a group of only notes is an
        // EMPTY section. The whole body is compared, with and without a
        // title page, because "the book does not change" is the claim.
        let note = "[[img: p001-01.png w=100 full]]\n\n";
        let script = "INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n";
        for head in ["", "Title: T\nAuthor: A\n\n"] {
            let plain = default_body(&format!("{head}{script}"));
            assert_eq!(default_body(&format!("{head}{note}{script}")), plain);
            assert_eq!(default_body(&format!("{head}{note}{note}{script}")), plain);
        }
        let titles: Vec<String> = default_body(&format!("{note}{script}"))
            .toc
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(titles, ["INT. LAB - DAY", "EXT. YARD - NIGHT"]);
    }

    #[test]
    fn a_note_ahead_of_real_opening_content_leaves_that_opening_as_it_was() {
        let open = "A cold open.\n\nINT. LAB - DAY\n\nWork.\n";
        let noted = default_body(&format!("[[img: p001-01.png w=100 full]]\n\n{open}"));
        assert_eq!(noted, default_body(open));
        assert_eq!(noted.toc[0].title, "Opening");
    }

    #[test]
    fn script_with_no_scene_headings_yields_one_section() {
        let body = default_body("Just action.\n\nMore action.\n");
        assert_eq!(body.files.len(), 1);
        assert!(body.files[0].xhtml.contains("Just action."));
        assert_eq!(body.toc.len(), 1);
    }
}

// ── dual dialogue height fallback ────────────────────────

mod dual_dialogue_height_fallback {
    use super::*;

    fn dual_script(line: &str) -> String {
        format!("INT. HALL - DAY\n\n@JACK\n{line}\n\n@JILL ^\nAlso talking here.\n")
    }

    #[test]
    fn a_short_exchange_stays_a_side_by_side_table() {
        assert!(
            first_xhtml(&dual_script("Quick word.")).contains("<table class=\"dual-dialogue\">")
        );
    }

    #[test]
    fn a_tall_exchange_emits_sequential_speeches_instead_of_an_unbreakable_table() {
        let xhtml = first_xhtml(&dual_script("word ".repeat(120).trim()));
        assert!(!xhtml.contains("<table class=\"dual-dialogue\">"));
        assert!(xhtml.contains("<p class=\"character\">JACK</p>"));
        assert!(xhtml.contains("<p class=\"character\">JILL</p>"));
        assert!(matches(
            &xhtml,
            r#"<div class="dialogue-block">\s*<p class="character">JACK</p>[\s\S]*?<div class="dialogue-block">\s*<p class="character">JILL</p>"#
        ));
        assert_eq!(count(&xhtml, "<div class=\"dialogue-block\">"), 2);
    }

    #[test]
    fn the_table_sequential_flip_sits_exactly_where_the_constants_put_it() {
        // The taller column costs one line for the cue plus
        // ceil((N + 1) / 30) for the dialogue, so the flip lands between
        // 329 and 330.
        let render = |n: usize| first_xhtml(&dual_script(&"x".repeat(n)));
        assert!(render(329).contains("<table class=\"dual-dialogue\">"));
        assert!(!render(330).contains("<table class=\"dual-dialogue\">"));
    }
}

// ── screenplay CSS geometry ──────────────────────────────

mod screenplay_css_kindle_safe_geometry {
    use super::*;

    #[test]
    fn dialogue_column_narrows_with_percent_side_margins_never_max_width() {
        let block = rule_for(&css(), ".dialogue-block");
        assert!(matches(&block, r"margin-left:\s*\d+%"));
        assert!(matches(&block, r"margin-right:\s*\d+%"));
        assert!(!css().contains("max-width"));
    }

    #[test]
    fn cue_keeps_with_its_dialogue_and_centers_in_the_column_by_default() {
        let cue = rule_for(&css(), "p.character");
        assert!(cue.contains("break-after: avoid"));
        assert!(cue.contains("text-align: center"));
    }

    #[test]
    fn parenthetical_centers_in_the_column_by_default() {
        assert!(rule_for(&css(), "p.parenthetical").contains("text-align: center"));
    }

    #[test]
    fn vertical_rhythm_is_em_based_a_full_blank_line_between_elements() {
        assert!(matches(
            &rule_for(&css(), "p.action"),
            r"margin:\s*1(\.\d+)?em 0"
        ));
        assert!(matches(
            &rule_for(&css(), ".dialogue-block"),
            r"margin-top:\s*1(\.\d+)?em"
        ));
    }

    #[test]
    fn body_text_keeps_device_default_line_height() {
        assert!(!rule_for(&css(), "body").contains("line-height"));
    }

    #[test]
    fn keep_with_next_chain_covers_heading_cue_and_parenthetical() {
        assert!(rule_for(&css(), "p.parenthetical").contains("break-after: avoid"));
        let heading = rule_for(&css(), "h2.scene-heading");
        // both spellings: the legacy prefixed property AND the standalone
        assert!(heading.contains("page-break-after: avoid"));
        assert!(matches(&heading, r"[^-]break-after: avoid"));
    }

    #[test]
    fn page_markers_float_out_of_the_flow_so_they_cost_no_line() {
        assert!(!matches(&css(), r"p\.page-marker\s*\{"));
        let marker = rule_for(&css(), "span.page-marker");
        assert!(marker.contains("float: right"));
        assert!(matches(&marker, r"margin-left:\s*\d+%"));
    }

    #[test]
    fn transitions_may_end_a_page_but_never_begin_one() {
        let t = rule_for(&css(), "p.transition");
        assert!(t.contains("page-break-before: avoid"));
        assert!(t.contains("break-before: avoid"));
    }

    fn selectors_carrying(pattern: &str) -> Vec<String> {
        let re = regex::Regex::new(pattern).unwrap();
        each_rule(&css())
            .into_iter()
            .filter(|r| re.is_match(&r.body))
            .map(|r| r.selector)
            .collect()
    }

    #[test]
    fn the_avoid_inventory_is_closed_six_entries_twelve_declarations() {
        // Every avoid link grows the chunk a renderer pushes, so the
        // inventory is closed and written out in the css header. Six
        // entries carry both spellings; the last three selectors are the
        // multicol shadows, which add selectors but no declarations.
        let declarations =
            regex::Regex::new(r"(?m)^\s*(?:page-)?break-(?:after|before|inside):\s*avoid;")
                .unwrap()
                .find_iter(&css())
                .count();
        assert_eq!(declarations, 12);
        assert_eq!(
            selectors_carrying(r":\s*avoid;"),
            [
                "h2.scene-heading",
                "p.mini-slug",
                "p.character",
                "p.parenthetical",
                "table.dual-dialogue",
                "p.transition",
                "table.dual-dialogue",
                "h2.scene-heading, p.mini-slug, p.character, p.parenthetical",
                "p.transition",
            ]
        );
    }

    #[test]
    fn widows_orphans_carry_on_exactly_two_selectors() {
        assert_eq!(
            selectors_carrying(r"\b(?:widows|orphans):\s*\d+;"),
            ["p.action", "p.dialogue"]
        );
    }
}

// ── buildEpub ────────────────────────────────────────────

mod build_epub_ {
    use super::*;

    fn build() -> Vec<u8> {
        build_epub(
            &meta("Test Script", Some("Jane Doe")),
            &sample_body(),
            &DEFAULT_FORMAT_OPTIONS,
            &[],
            EPOCH,
        )
        .unwrap()
    }

    #[test]
    fn mimetype_is_first_stored_uncompressed_with_exact_content() {
        let bytes = build();
        let head = String::from_utf8_lossy(&bytes[..100]);
        assert!(head.contains("mimetypeapplication/epub+zip"));
    }

    #[test]
    fn container_xml_points_at_the_package_document() {
        let container = entry_text(&build(), "META-INF/container.xml").unwrap();
        assert!(container.contains("full-path=\"OEBPS/package.opf\""));
    }

    #[test]
    fn package_opf_carries_metadata_manifest_and_spine_in_reading_order() {
        let opf = entry_text(&build(), "OEBPS/package.opf").unwrap();
        assert!(opf.contains("<dc:title>Test Script</dc:title>"));
        assert!(opf.contains("<dc:creator>Jane Doe</dc:creator>"));
        assert!(opf.contains("properties=\"nav\""));
        assert!(matches(
            &opf,
            r#"<itemref idref="titlepage"/>\s*<itemref idref="body001"/>"#
        ));
    }

    #[test]
    fn opf_declares_the_ibooks_prefix_and_specified_fonts_meta() {
        let opf = entry_text(&build(), "OEBPS/package.opf").unwrap();
        assert!(opf.contains(
            "prefix=\"ibooks: http://vocabulary.itunes.apple.com/rdf/ibooks/vocabulary-extensions-1.0/\""
        ));
        assert!(opf.contains("<meta property=\"ibooks:specified-fonts\">true</meta>"));
    }

    #[test]
    fn nav_xhtml_lists_every_scene_with_an_anchored_link() {
        let nav = entry_text(&build(), "OEBPS/nav.xhtml").unwrap();
        assert!(nav.contains("<a href=\"text/body001.xhtml#sc-001\">INT. KITCHEN - DAY</a>"));
        assert!(nav.contains("<a href=\"text/body001.xhtml#sc-002\">EXT. YARD - NIGHT</a>"));
    }

    #[test]
    fn nav_xhtml_carries_a_hidden_page_list_when_the_script_has_markers() {
        let body = default_body("INT. A - DAY\n\n= pg 12\n\nShe waits.\n\n= pg 13\n\nHe leaves.\n");
        let epub = build_epub(
            &meta("Test Script", None),
            &body,
            &DEFAULT_FORMAT_OPTIONS,
            &[],
            EPOCH,
        )
        .unwrap();
        let nav = entry_text(&epub, "OEBPS/nav.xhtml").unwrap();
        assert!(nav.contains("<nav epub:type=\"page-list\" hidden=\"hidden\">"));
        assert!(nav.contains("<a href=\"text/body001.xhtml#pg12\">12</a>"));
        assert!(nav.contains("<a href=\"text/body001.xhtml#pg13\">13</a>"));
        assert!(nav.find("page-list").unwrap() > nav.find("landmarks").unwrap());
    }

    #[test]
    fn no_page_list_nav_at_all_when_the_script_has_no_markers() {
        assert!(
            !entry_text(&build(), "OEBPS/nav.xhtml")
                .unwrap()
                .contains("page-list")
        );
    }

    #[test]
    fn title_page_renders_title_and_author() {
        let tp = entry_text(&build(), "OEBPS/titlepage.xhtml").unwrap();
        assert!(tp.contains("Test Script"));
        assert!(tp.contains("Jane Doe"));
    }

    #[test]
    fn body_file_is_well_formed_xhtml_with_the_stylesheet_linked() {
        let doc = entry_text(&build(), "OEBPS/text/body001.xhtml").unwrap();
        assert!(doc.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(doc.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
        assert!(doc.contains("<link rel=\"stylesheet\""));
    }
}

// ── inline emphasis rendering ────────────────────────────

mod inline_emphasis_in_xhtml {
    use super::*;

    #[test]
    fn fountain_emphasis_renders_as_em_strong_underline() {
        let xhtml = first_xhtml(
            "INT. A - DAY\n\nThe sign reads ***DO NOT ENTER*** and **STOP** and *slow* and _underlined_.\n",
        );
        assert!(xhtml.contains("<strong><em>DO NOT ENTER</em></strong>"));
        assert!(xhtml.contains("<strong>STOP</strong>"));
        assert!(xhtml.contains("<em>slow</em>"));
        assert!(xhtml.contains("<span class=\"underline\">underlined</span>"));
    }

    #[test]
    fn mixed_marks_unwrap_with_the_underscore_innermost() {
        let xhtml =
            first_xhtml("INT. A - DAY\n\nThe stamp is **_VOID_** and ***_LOUD_*** and *_soft_*.\n");
        assert!(xhtml.contains("<strong><span class=\"underline\">VOID</span></strong>"));
        assert!(xhtml.contains("<strong><em><span class=\"underline\">LOUD</span></em></strong>"));
        assert!(xhtml.contains("<em><span class=\"underline\">soft</span></em>"));
    }

    #[test]
    fn escaping_still_applies_inside_emphasized_text() {
        assert!(first_xhtml("INT. A - DAY\n\nA *5 < 6* case.\n").contains("<em>5 &lt; 6</em>"));
    }

    #[test]
    fn stray_asterisk_free_text_is_untouched() {
        assert!(first_xhtml("INT. A - DAY\n\nPlain 2 * 3 math.\n").contains("Plain 2 * 3 math."));
    }
}

// ── page markers render ──────────────────────────────────

mod page_markers_in_xhtml {
    use super::*;

    #[test]
    fn a_marker_rides_inside_the_next_block_not_in_a_paragraph_of_its_own() {
        let xhtml = first_xhtml("INT. A - DAY\n\nAction.\n\n= pg 47\n\nAnne crosses the room.\n");
        assert!(!xhtml.contains("<p class=\"page-marker\">"));
        assert!(matches(
            &xhtml,
            r#"<span[^>]*class="page-marker"[^>]*>47\.</span>"#
        ));
        assert!(matches(
            &xhtml,
            r#"<p class="action"><span[^>]*class="page-marker"[^>]*>47\.</span>Anne crosses the room\."#
        ));
    }

    #[test]
    fn the_marker_span_is_the_epub3_pagination_anchor() {
        let xhtml = first_xhtml("INT. A - DAY\n\n= pg 47\n\nAction.\n");
        assert!(xhtml.contains("epub:type=\"pagebreak\""));
        assert!(xhtml.contains("role=\"doc-pagebreak\""));
        assert!(xhtml.contains("id=\"pg47\""));
        assert!(xhtml.contains("title=\"47\""));
    }

    #[test]
    fn a_trailing_marker_with_no_block_to_ride_in_is_dropped() {
        assert!(
            !first_xhtml("INT. A - DAY\n\nAnne crosses the room.\n\n= pg 48\n")
                .contains("page-marker")
        );
    }

    #[test]
    fn a_marker_at_a_scene_seam_rides_into_the_next_heading_not_lost() {
        let xhtml = first_xhtml("INT. A - DAY\n\nAction.\n\n= pg 12\n\nINT. B - NIGHT\n\nMore.\n");
        assert!(matches(
            &xhtml,
            r#"<h2 class="scene-heading"><span[^>]*class="page-marker"[^>]*>12\.</span>INT\. B - NIGHT</h2>"#
        ));
    }

    #[test]
    fn ordinary_synopsis_lines_stay_invisible() {
        assert!(
            !first_xhtml("INT. A - DAY\n\n= Jack discovers the truth\n\nAction.\n")
                .contains("discovers the truth")
        );
    }

    #[test]
    fn page_list_records_every_marker_in_document_order() {
        let body = default_body("INT. A - DAY\n\n= pg 12\n\nShe waits.\n\n= pg 13\n\nHe leaves.\n");
        let list: Vec<(String, String)> = body
            .page_list
            .iter()
            .map(|p| (p.label.clone(), p.href.clone()))
            .collect();
        assert_eq!(
            list,
            pairs(&[
                ("12", "text/body001.xhtml#pg12"),
                ("13", "text/body001.xhtml#pg13")
            ])
        );
    }

    #[test]
    fn every_page_list_href_resolves_to_an_id_in_the_file_it_names() {
        let body = tokens_to_body(
            &tokens(
                "INT. A - DAY\n\n= pg 12\n\nShe waits a long while by the window.\n\nINT. B - NIGHT\n\n= pg 13\n\nHe leaves without a word to anyone.\n",
            ),
            &BodyOptions {
                max_file_bytes: 400,
                ..BodyOptions::default()
            },
        );
        assert!(body.files.len() > 1);
        for p in &body.page_list {
            let (name, fragment) = p.href.split_once('#').unwrap();
            let file = body
                .files
                .iter()
                .find(|f| name == format!("text/{}.xhtml", f.id))
                .expect("the file it names");
            assert!(file.xhtml.contains(&format!("id=\"{fragment}\"")));
        }
        assert_eq!(body.page_list[1].href, "text/body002.xhtml#pg13");
    }

    #[test]
    fn a_dropped_trailing_marker_never_reaches_the_page_list() {
        assert!(
            default_body("INT. A - DAY\n\nShe waits.\n\n= pg 48\n")
                .page_list
                .is_empty()
        );
    }

    #[test]
    fn a_script_without_markers_has_an_empty_page_list() {
        assert!(sample_body().page_list.is_empty());
    }
}

// ── figures (registry #19) ───────────────────────────────

mod figures {
    use super::*;

    pub const HAVE: &[&str] = &["p001-01.png", "p002-01.png"];

    pub fn off() -> FormatOptions {
        FormatOptions {
            include_images: false,
            ..DEFAULT_FORMAT_OPTIONS
        }
    }

    /// Pictures on and the card left in the flow: the book before covers.
    pub fn no_cover() -> FormatOptions {
        FormatOptions {
            promote_cover_image: false,
            ..DEFAULT_FORMAT_OPTIONS
        }
    }

    pub fn scene(note: &str) -> String {
        format!("INT. LAB - DAY\n\nBefore.\n\n{note}\n\nAfter.\n")
    }

    const ON: FormatOptions = DEFAULT_FORMAT_OPTIONS;

    #[test]
    fn a_picture_note_with_its_picture_present_becomes_a_figure_where_it_sat() {
        let body = body_of(&scene("[[img: p002-01.png w=42]]"), &ON, HAVE);
        assert!(matches(
            &body.files[0].xhtml,
            r#"<p class="action">Before\.</p>\s*<p class="figure w40"><img src="\.\./images/p002-01\.png" alt="Illustration"/></p>\s*<p class="action">After\.</p>"#
        ));
        assert_eq!(body.used_images, ["p002-01.png"]);
    }

    #[test]
    fn a_full_page_picture_says_so_in_its_class() {
        let body = body_of(&scene("[[img: p001-01.png w=100 full]]"), &ON, HAVE);
        assert!(body.files[0].xhtml.contains(
            "<p class=\"figure w100 figure-full\"><img src=\"../images/p001-01.png\" alt=\"Illustration\"/></p>"
        ));
    }

    #[test]
    fn the_width_class_is_the_notes_width_to_the_nearest_five_inside_5_to_100() {
        let class_of = |w: &str| {
            let xhtml = body_of(&scene(&format!("[[img: p002-01.png w={w}]]")), &ON, HAVE).files[0]
                .xhtml
                .clone();
            regex::Regex::new(r#"<p class="figure (w\d+)""#)
                .unwrap()
                .captures(&xhtml)
                .map(|c| c[1].to_owned())
        };
        for (w, class) in [
            ("42", "w40"),
            ("43", "w45"),
            ("97", "w95"),
            ("98", "w100"),
            ("1", "w5"),
            ("7", "w5"),
            ("8", "w10"),
        ] {
            assert_eq!(class_of(w).as_deref(), Some(class), "w={w}");
        }
    }

    #[test]
    fn the_alt_text_is_never_empty() {
        let xhtml = &body_of(&scene("[[img: p002-01.png w=42]]"), &ON, HAVE).files[0].xhtml;
        assert!(xhtml.contains("alt=\"Illustration\""));
        assert!(!xhtml.contains("alt=\"\""));
    }

    #[test]
    fn a_picture_the_caller_cannot_ship_renders_nothing_and_is_not_reported_as_used() {
        let plain = default_body("INT. LAB - DAY\n\nBefore.\n\nAfter.\n");
        let missing = body_of(&scene("[[img: p009-01.png w=42]]"), &ON, HAVE);
        assert_eq!(missing, plain);
        assert!(missing.used_images.is_empty());
    }

    #[test]
    fn pictures_off_renders_nothing_even_with_every_picture_present() {
        let plain = body_of("INT. LAB - DAY\n\nBefore.\n\nAfter.\n", &off(), &[]);
        let off_body = body_of(&scene("[[img: p002-01.png w=42]]"), &off(), HAVE);
        assert_eq!(off_body, plain);
        assert!(off_body.used_images.is_empty());
    }

    #[test]
    fn a_caller_that_passes_no_pictures_renders_exactly_what_it_did_before() {
        let plain = default_body("INT. LAB - DAY\n\nBefore.\n\nAfter.\n");
        assert_eq!(default_body(&scene("[[img: p002-01.png w=42]]")), plain);
        assert_eq!(
            body_of(&scene("[[img: p002-01.png w=42]]"), &ON, &[]),
            plain
        );
    }

    #[test]
    fn used_images_names_each_picture_once_however_often_it_is_shown() {
        let src = "INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n[[img: p002-01.png w=42]]\n\nINT. YARD - DAY\n\n[[img: p001-01.png w=60]]\n";
        let body = body_of(src, &ON, HAVE);
        let mut used = body.used_images.clone();
        used.sort();
        assert_eq!(used, ["p001-01.png", "p002-01.png"]);
        assert_eq!(count(&body.files[0].xhtml, "<p class=\"figure"), 3);
    }

    #[test]
    fn the_src_comes_from_the_callers_resolver_escaped_as_an_attribute() {
        let src = |file: &str| format!("x?f={file}&a=\"b\"");
        let body = tokens_to_body(
            &tokens(&scene("[[img: p002-01.png w=42]]")),
            &BodyOptions {
                images: HAVE.iter().map(|s| (*s).to_owned()).collect(),
                image_src: Some(&src),
                ..BodyOptions::default()
            },
        );
        assert!(
            body.files[0].xhtml.contains(
                "<img src=\"x?f=p002-01.png&amp;a=&quot;b&quot;\" alt=\"Illustration\"/>"
            )
        );
    }

    #[test]
    fn a_figure_declines_a_pending_page_marker_which_lands_on_the_next_text_instead() {
        let body = body_of(
            "INT. LAB - DAY\n\nBefore.\n\n= pg 7\n\n[[img: p002-01.png w=42]]\n\nAfter.\n",
            &ON,
            HAVE,
        );
        let xhtml = &body.files[0].xhtml;
        assert!(matches(xhtml, r#"<p class="figure w40"><img [^>]*/></p>"#));
        assert!(matches(
            xhtml,
            r#"<p class="action"><span[^>]*class="page-marker"[^>]*>7\.</span>After\.</p>"#
        ));
        let labels: Vec<&str> = body.page_list.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["7"]);
    }

    /// toFountain writes a title card's note above the first scene heading.
    /// A section is opened for a figure in the flow, never for a note that
    /// will render nothing there.
    mod a_picture_note_before_the_first_heading {
        use super::*;

        const CARD: &str = "[[img: p001-01.png w=100 full]]\n\n";
        const SCRIPT: &str = "INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n";

        #[test]
        fn a_pictures_off_the_body_is_the_note_free_body_byte_for_byte() {
            for head in ["", "Title: T\nAuthor: A\n\n"] {
                let plain = body_of(&format!("{head}{SCRIPT}"), &off(), HAVE);
                assert_eq!(
                    body_of(&format!("{head}{CARD}{SCRIPT}"), &off(), HAVE),
                    plain
                );
            }
        }

        #[test]
        fn b_pictures_on_and_present_an_opening_holds_the_figure() {
            let body = body_of(&format!("{CARD}{SCRIPT}"), &no_cover(), HAVE);
            assert_eq!(
                toc(&body),
                pairs(&[
                    ("Opening", "text/body001.xhtml#sc-001"),
                    ("INT. LAB - DAY", "text/body001.xhtml#sc-002"),
                    ("EXT. YARD - NIGHT", "text/body001.xhtml#sc-003"),
                ])
            );
            assert!(matches(
                &body.files[0].xhtml,
                r#"<section class="scene" id="sc-001">\s*<p class="figure w100 figure-full"><img src="\.\./images/p001-01\.png" alt="Illustration"/></p>\s*</section>\s*<section class="scene" id="sc-002">\s*<h2 class="scene-heading">INT\. LAB - DAY</h2>"#
            ));
            assert_eq!(body.used_images, ["p001-01.png"]);
            assert_eq!(body.cover_image, None);
        }

        #[test]
        fn c_pictures_on_but_missing_identical_to_a() {
            let plain = body_of(SCRIPT, &off(), HAVE);
            for format in [ON, no_cover()] {
                assert_eq!(
                    body_of(&format!("{CARD}{SCRIPT}"), &format, &["p002-01.png"]),
                    plain
                );
                assert_eq!(body_of(&format!("{CARD}{SCRIPT}"), &format, &[]), plain);
            }
        }

        #[test]
        fn a_note_that_renders_and_one_that_does_not_in_either_order_open_one_opening() {
            let dead = "[[img: p009-01.png w=50]]\n\n";
            for src in [
                format!("{dead}{CARD}{SCRIPT}"),
                format!("{CARD}{dead}{SCRIPT}"),
            ] {
                let body = body_of(&src, &no_cover(), HAVE);
                let titles: Vec<&str> = body.toc.iter().map(|e| e.title.as_str()).collect();
                assert_eq!(titles, ["Opening", "INT. LAB - DAY", "EXT. YARD - NIGHT"]);
                assert_eq!(count(&body.files[0].xhtml, "<p class=\"figure"), 1);
            }
        }
    }

    /// pickCover's choice leaves the flow: the body is the body of the same
    /// script without the card, and names the cover for the packager.
    mod the_title_card_as_the_cover {
        use super::*;

        const CARD: &str = "[[img: p001-01.png w=100 full]]\n\n";
        const SCRIPT: &str = "INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n";

        /// Everything about a body but which picture is its cover.
        fn pages(body: &BookBody) -> BookBody {
            BookBody {
                cover_image: None,
                ..body.clone()
            }
        }

        #[test]
        fn the_card_is_not_drawn_opens_no_opening_and_the_first_scene_is_sc_001() {
            for head in ["", "Title: T\nAuthor: A\n\n"] {
                let body = body_of(&format!("{head}{CARD}{SCRIPT}"), &ON, HAVE);
                assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
                assert_eq!(
                    toc(&body),
                    pairs(&[
                        ("INT. LAB - DAY", "text/body001.xhtml#sc-001"),
                        ("EXT. YARD - NIGHT", "text/body001.xhtml#sc-002"),
                    ])
                );
                assert!(!body.files[0].xhtml.contains("class=\"figure"));
                let plain = body_of(&format!("{head}{SCRIPT}"), &ON, HAVE);
                assert_eq!(pages(&body), pages(&plain));
            }
        }

        #[test]
        fn used_images_stays_what_the_pages_draw_so_the_cover_is_not_in_it() {
            assert!(
                body_of(&format!("{CARD}{SCRIPT}"), &ON, HAVE)
                    .used_images
                    .is_empty()
            );
        }

        #[test]
        fn so_a_card_only_front_reads_the_same_with_pictures_on_or_off() {
            let on = body_of(&format!("{CARD}{SCRIPT}"), &ON, HAVE);
            let off_body = body_of(&format!("{CARD}{SCRIPT}"), &off(), HAVE);
            assert_eq!(pages(&on), pages(&off_body));
        }

        #[test]
        fn pictures_after_the_card_are_drawn_where_they_sit() {
            let body = body_of(
                &format!("{CARD}INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nWork.\n"),
                &ON,
                HAVE,
            );
            assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
            assert!(body.files[0].xhtml.contains(
                "<p class=\"figure w40\"><img src=\"../images/p002-01.png\" alt=\"Illustration\"/></p>"
            ));
            assert!(!body.files[0].xhtml.contains("p001-01.png"));
            assert_eq!(body.used_images, ["p002-01.png"]);
        }

        #[test]
        fn the_same_plate_shown_again_later_is_drawn_there_only_the_card_note_leaves_the_flow() {
            let body = body_of(&format!("{CARD}{SCRIPT}\n{CARD}"), &ON, HAVE);
            assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
            assert_eq!(
                count(
                    &body.files[0].xhtml,
                    "<p class=\"figure w100 figure-full\">"
                ),
                1
            );
            assert!(matches(
                &body.files[0].xhtml,
                r#"Rest\.</p>\s*<p class="figure w100 figure-full">"#
            ));
            assert_eq!(body.used_images, ["p001-01.png"]);
        }

        #[test]
        fn a_note_that_renders_nothing_in_either_place_does_not_stop_the_cover() {
            let dead = "[[img: p009-01.png w=50]]\n\n";
            for src in [
                format!("{dead}{CARD}{SCRIPT}"),
                format!("{CARD}{dead}{SCRIPT}"),
            ] {
                let body = body_of(&src, &ON, HAVE);
                assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
                let titles: Vec<&str> = body.toc.iter().map(|e| e.title.as_str()).collect();
                assert_eq!(titles, ["INT. LAB - DAY", "EXT. YARD - NIGHT"]);
            }
        }

        #[test]
        fn a_cold_open_after_the_card_still_opens_the_opening_which_holds_only_the_cold_open() {
            let body = body_of(&format!("{CARD}A cold open.\n\n{SCRIPT}"), &ON, HAVE);
            assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
            let titles: Vec<&str> = body.toc.iter().map(|e| e.title.as_str()).collect();
            assert_eq!(titles, ["Opening", "INT. LAB - DAY", "EXT. YARD - NIGHT"]);
            assert!(matches(
                &body.files[0].xhtml,
                r#"<section class="scene" id="sc-001">\s*<p class="action">A cold open\.</p>\s*</section>"#
            ));
        }

        #[test]
        fn with_promote_cover_image_off_nothing_is_the_cover_and_the_card_is_the_opening() {
            let body = body_of(&format!("{CARD}{SCRIPT}"), &no_cover(), HAVE);
            assert_eq!(body.cover_image, None);
            assert_eq!(body.toc[0].title, "Opening");
        }
    }
}

// ── figure CSS (registry #19) ────────────────────────────

mod figure_css {
    use super::*;

    /// A spread of option sets, so a figure block that leaned on one knob's
    /// value, or a seam that moved under one, shows up in some row.
    fn formats() -> Vec<FormatOptions> {
        vec![
            DEFAULT_FORMAT_OPTIONS,
            opts(serde_json::json!({ "scenePageBreaks": true })),
            opts(
                serde_json::json!({ "keepSpeechesWhole": true, "keepSceneHeadingWithScene": false }),
            ),
            opts(serde_json::json!({ "elementSpacingEm": 0.6, "justifyText": true })),
            opts(serde_json::json!({ "cueAlignment": "indented", "fontFamily": "serif" })),
            opts(serde_json::json!({ "printSplitMinimums": false, "includeImages": false })),
        ]
    }

    fn widths() -> Vec<u32> {
        (1..=20).map(|i| i * 5).collect()
    }

    fn figure_selectors() -> Vec<String> {
        let mut v = vec![".figure".to_owned(), ".figure img".to_owned()];
        v.extend(widths().iter().map(|n| format!("p.w{n} img")));
        v.push("p.figure-full".to_owned());
        v
    }

    fn with_figures(o: &FormatOptions) -> String {
        screenplay_css(o, true)
    }

    const TITLE_PAGE: &str = "/* Title page */";

    #[test]
    fn without_figures_the_sheet_is_the_one_every_caller_already_gets() {
        // The TypeScript's default argument has no Rust form; the default
        // sheet is the no-figures sheet of the default options.
        assert_eq!(
            screenplay_css(&DEFAULT_FORMAT_OPTIONS, false),
            default_css()
        );
    }

    #[test]
    fn without_figures_the_sheet_styles_no_figure_and_no_image() {
        let re = regex::Regex::new(r"\.figure|\bimg\b|\bp\.w\d").unwrap();
        for o in formats() {
            let css = screenplay_css(&o, false);
            for r in each_rule(&css) {
                assert!(!re.is_match(&r.selector), "{}", r.selector);
            }
            assert!(!css.contains(".figure"));
        }
    }

    #[test]
    fn without_figures_the_seam_is_exactly_one_blank_line_between_p_centered_and_the_title_page() {
        assert!(
            screenplay_css(&DEFAULT_FORMAT_OPTIONS, false).contains(&format!(
                "p.centered {{\n  text-align: center;\n  margin: 1em 0;\n}}\n\n{TITLE_PAGE}\n"
            ))
        );
        assert!(
            screenplay_css(&opts(serde_json::json!({ "elementSpacingEm": 0.6 })), false).contains(
                &format!(
                    "p.centered {{\n  text-align: center;\n  margin: 0.6em 0;\n}}\n\n{TITLE_PAGE}\n"
                )
            )
        );
    }

    #[test]
    fn with_figures_the_sheet_is_that_sheet_plus_one_block_between_p_centered_and_the_title_page() {
        for o in formats() {
            let without = screenplay_css(&o, false);
            let with = with_figures(&o);
            let at = without.find(TITLE_PAGE).unwrap();
            assert!(at > 0);
            assert_eq!(&with[..at], &without[..at]);
            assert!(with.ends_with(&without[at..]));
            let block = &with[at..with.len() - (without.len() - at)];
            assert!(matches(&with[..at], r"p\.centered \{[^}]*\}\n\n$"));
            let selectors: Vec<String> = each_rule(block).into_iter().map(|r| r.selector).collect();
            assert_eq!(selectors, figure_selectors());
            assert!(block.ends_with("}\n\n"));
        }
    }

    #[test]
    fn the_figure_box_element_spacing_above_and_below_centred_in_em() {
        let css = with_figures(&DEFAULT_FORMAT_OPTIONS);
        assert!(rule_for(&css, ".figure").contains("margin: 1em 0;"));
        assert!(rule_for(&css, ".figure").contains("text-align: center;"));
        let tight = with_figures(&opts(serde_json::json!({ "elementSpacingEm": 0.6 })));
        assert!(rule_for(&tight, ".figure").contains("margin: 0.6em 0;"));
    }

    #[test]
    fn the_picture_keeps_its_aspect_the_height_follows_the_width() {
        let img = rule_for(&with_figures(&DEFAULT_FORMAT_OPTIONS), ".figure img");
        assert!(img.contains("height: auto;"));
        assert!(!img.contains("width"));
    }

    #[test]
    fn twenty_width_rules_5_to_100_percent_of_the_column_in_steps_of_five() {
        let css = with_figures(&DEFAULT_FORMAT_OPTIONS);
        for n in widths() {
            assert!(matches(
                &rule_for(&css, &format!("p.w{n} img")),
                &format!(r"\{{\s*width: {n}%;\s*\}}$")
            ));
        }
        let re = regex::Regex::new(r"^p\.w\d+ img$").unwrap();
        assert_eq!(
            each_rule(&css)
                .iter()
                .filter(|r| re.is_match(&r.selector))
                .count(),
            20
        );
    }

    #[test]
    fn a_full_page_plate_ends_the_page_before_it_and_the_page_it_is_on() {
        let full = rule_for(&with_figures(&DEFAULT_FORMAT_OPTIONS), "p.figure-full");
        assert!(full.contains("page-break-before: always;"));
        assert!(full.contains("page-break-after: always;"));
        assert!(!full.contains("avoid"));
    }

    #[test]
    fn a_full_page_plate_has_no_vertical_margin() {
        assert!(matches(
            &rule_for(&with_figures(&DEFAULT_FORMAT_OPTIONS), "p.figure-full"),
            r"(^|[{;\s])margin: 0;"
        ));
    }

    #[test]
    fn figures_add_no_keep_no_multicol_spelling_and_leave_every_existing_pin_as_it_was() {
        let avoids = |css: &str| -> Vec<String> {
            regex::Regex::new(r"(?m)^\s*(?:page-)?break-(?:after|before|inside):\s*avoid;")
                .unwrap()
                .find_iter(css)
                .map(|m| m.as_str().to_owned())
                .collect()
        };
        let carrying = |css: &str, pattern: &str| -> Vec<String> {
            let re = regex::Regex::new(pattern).unwrap();
            each_rule(css)
                .into_iter()
                .filter(|r| re.is_match(&r.body))
                .map(|r| r.selector)
                .collect()
        };
        for o in formats() {
            let without = screenplay_css(&o, false);
            let with = with_figures(&o);
            assert_eq!(avoids(&with), avoids(&without));
            assert_eq!(
                carrying(&with, r":\s*avoid;"),
                carrying(&without, r":\s*avoid;")
            );
            assert_eq!(
                carrying(&with, "-webkit-column-break"),
                carrying(&without, "-webkit-column-break")
            );
            assert_eq!(
                carrying(&with, r"\b(?:widows|orphans):"),
                carrying(&without, r"\b(?:widows|orphans):")
            );
            let mut always = carrying(&without, r":\s*always;");
            always.push("p.figure-full".to_owned());
            assert_eq!(carrying(&with, r":\s*always;"), always);
            assert!(!matches(&rule_for(&with, ".figure"), "break|keep"));
        }
    }

    #[test]
    fn the_figure_sheet_keeps_the_stylesheet_invariants() {
        let selectors = figure_selectors();
        let comment = regex::Regex::new(r"/\*[\s\S]*?\*/").unwrap();
        for o in formats() {
            let css = with_figures(&o);
            assert!(!css.contains("max-width"));
            assert!(!css.contains("max-height"));
            let block = each_rule(&css)
                .into_iter()
                .filter(|r| selectors.contains(&r.selector))
                .map(|r| r.body)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(!block.contains("line-height"));
            assert!(!matches(&block, r"\b(?:min|max|clamp|var|calc)\("));
            assert!(!matches(&block, r"\d(?:px|pt|cm|mm|in)\b"));
            for c in comment.find_iter(&css) {
                assert!(!c.as_str().contains(['{', '}']));
            }
        }
    }

    #[test]
    fn every_class_the_figure_rules_style_is_one_the_renderer_emits_and_every_one_it_emits_is_styled()
     {
        let mut emitted = std::collections::BTreeSet::new();
        let class_re = regex::Regex::new(r#"<p class="([^"]*)"><img "#).unwrap();
        for w in -5..=105 {
            for full in ["", " full"] {
                let xhtml = body_of(
                    &format!("INT. A - DAY\n\n[[img: p001-01.png w={w}{full}]]\n"),
                    &DEFAULT_FORMAT_OPTIONS,
                    &["p001-01.png"],
                )
                .files[0]
                    .xhtml
                    .clone();
                let m = class_re
                    .captures(&xhtml)
                    .unwrap_or_else(|| panic!("w={w}{full}"));
                for cls in m[1].split_whitespace() {
                    emitted.insert(cls.to_owned());
                }
            }
        }
        let selectors = figure_selectors();
        let block = each_rule(&with_figures(&DEFAULT_FORMAT_OPTIONS))
            .into_iter()
            .filter(|r| selectors.contains(&r.selector))
            .map(|r| r.selector)
            .collect::<Vec<_>>()
            .join(" ");
        let styled: std::collections::BTreeSet<String> = regex::Regex::new(r"\.([a-z][a-z0-9-]*)")
            .unwrap()
            .captures_iter(&block)
            .map(|c| c[1].to_owned())
            .collect();
        assert_eq!(styled, emitted);
        assert_eq!(styled.len(), 22);
    }

    #[test]
    fn a_preview_that_shows_no_figure_carries_no_figure_rules() {
        let html = tokens_to_preview_html(
            &tokens("INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nAfter.\n"),
            &DEFAULT_FORMAT_OPTIONS,
            &[],
        );
        assert!(!html.contains(".figure"));
        assert!(html.contains(&default_css()));
    }
}

// ── dual dialogue side-by-side ───────────────────────────

mod dual_dialogue_table_rendering {
    use super::*;

    const DUAL_SRC: &str = "INT. A - DAY\n\n@VERA\n(overlapping)\nRead me the last page--\n\n@INFORMANT ^\n--the last page burned.\n\nThey stare.\n";

    #[test]
    fn dual_speeches_render_as_a_two_cell_table() {
        let xhtml = first_xhtml(DUAL_SRC);
        assert!(matches(
            &xhtml,
            r#"<table class="dual-dialogue">\s*<tr>\s*<td>\s*<p class="character">VERA</p>\s*<p class="parenthetical">\(overlapping\)</p>\s*<p class="dialogue">Read me the last page--</p>\s*</td>\s*<td>\s*<p class="character">INFORMANT</p>\s*<p class="dialogue">--the last page burned\.</p>\s*</td>\s*</tr>\s*</table>"#
        ));
        assert!(xhtml.contains("<p class=\"action\">They stare.</p>"));
    }

    #[test]
    fn table_style_keeps_the_pair_together_and_splits_width_evenly() {
        assert!(rule_for(&css(), "table.dual-dialogue").contains("page-break-inside: avoid"));
        assert!(rule_for(&css(), "table.dual-dialogue td").contains("width: 50%"));
    }
}

// ── mini-slugs (secondary sluglines) ─────────────────────

mod mini_slug_rendering {
    use super::*;

    const SRC: &str =
        "INT. STORE - NIGHT\n\nMargo locks up.\n\n.LATER\n\nThe lights are still on.\n";

    #[test]
    fn a_forced_non_int_ext_slug_renders_as_a_micro_heading_not_a_scene_heading() {
        let xhtml = first_xhtml(SRC);
        assert!(xhtml.contains("<p class=\"mini-slug\">LATER</p>"));
        assert!(!xhtml.contains("<h2 class=\"scene-heading\">LATER</h2>"));
    }

    #[test]
    fn a_mini_slug_stays_inside_its_scene_no_section_no_toc_entry() {
        let body = default_body(SRC);
        assert_eq!(
            toc(&body),
            pairs(&[("INT. STORE - NIGHT", "text/body001.xhtml#sc-001")])
        );
        assert!(!body.files[0].xhtml.contains("sc-002"));
    }

    #[test]
    fn primary_sluglines_are_untouched_int_ext_est_i_e_stay_scene_headings() {
        let body = default_body(
            "INT. A - DAY\n\nOne.\n\nEXT. B - DAY\n\nTwo.\n\nEST. C - DAY\n\nThree.\n\nI/E. D - DAY\n\nFour.\n",
        );
        assert!(!body.files[0].xhtml.contains("mini-slug"));
        let titles: Vec<&str> = body.toc.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "INT. A - DAY",
                "EXT. B - DAY",
                "EST. C - DAY",
                "I/E. D - DAY"
            ]
        );
    }

    #[test]
    fn a_mini_slug_can_open_a_script_without_swallowing_the_opening_section() {
        let body = default_body(".COLD OPEN\n\nBlack.\n");
        assert_eq!(
            toc(&body),
            pairs(&[("Opening", "text/body001.xhtml#sc-001")])
        );
        assert!(
            body.files[0]
                .xhtml
                .contains("<p class=\"mini-slug\">COLD OPEN</p>")
        );
    }

    #[test]
    fn a_numbered_mini_slug_keeps_its_number_like_a_slugline_does() {
        let format = FormatOptions {
            show_scene_numbers: true,
            ..DEFAULT_FORMAT_OPTIONS
        };
        let body = body_of("INT. A - DAY\n\nOne.\n\n.LATER #5#\n\nTwo.\n", &format, &[]);
        assert!(
            body.files[0]
                .xhtml
                .contains("<p class=\"mini-slug\"><span class=\"scene-number\">5</span> LATER</p>")
        );
    }

    #[test]
    fn primary_slug_stays_in_lockstep_with_fountain_js() {
        // Every opener fountain-js promotes UNFORCED must also promote when
        // it arrives forced, and nothing else may.
        let cases = [
            ("INT. HOUSE - DAY", true),
            ("INT HOUSE - DAY", true),
            ("EXT. YARD - NIGHT", true),
            ("EST. THE HOUSE", true),
            ("I/E. CAR - DAY", true),
            ("I/E CAR - DAY", true),
            ("INT./EXT. CAR - DAY", true),
            ("INT/EXT CAR - DAY", true),
            ("LATER", false),
            ("THE BACK ROOM", false),
            ("ESTABLISHING SHOT", false),
            ("INTO THE WOODS", false),
            ("BACK TO:", false),
        ];
        for (text, promotes) in cases {
            let unforced = tokens(&format!("{text}\n\nAction.\n"));
            assert_eq!(
                unforced[0].kind == sp_model::TokenType::SceneHeading,
                promotes,
                "{text}"
            );
            let xhtml = first_xhtml(&format!(".{text}\n\nAction.\n"));
            assert_eq!(
                xhtml.contains("<h2 class=\"scene-heading\">"),
                promotes,
                "{text}"
            );
            assert_eq!(
                xhtml.contains("<p class=\"mini-slug\">"),
                !promotes,
                "{text}"
            );
        }
    }

    // 'the parser excludes from mini-slug exactly what this promotes'
    // compares src/parser/classify.ts's PRIMARY_SLUG with slug.ts's; both
    // copies leave this crate, so the check moves to sp-parser against
    // sp-fountain's (rust/PARITY.md).

    #[test]
    fn the_stylesheet_gives_it_the_bold_uppercase_micro_heading_treatment() {
        let rule = rule_for(&css(), "p.mini-slug");
        assert!(rule.contains("font-weight: bold"));
        assert!(rule.contains("text-transform: uppercase"));
        assert!(matches(&rule, r"margin:\s*1\.4em 0 1em 0"));
    }

    #[test]
    fn every_class_the_stylesheet_styles_is_one_a_renderer_emits() {
        // Two kinds of evidence: a `class="..."` literal in the renderer
        // source, and real rendered output for classes assembled at runtime
        // (the registry #18 fmt-* set).
        let fmt_probe = first_xhtml(
            "INT. A - DAY\n\n[[fmt: mono -1]]\nOne.\n\n[[fmt: serif +1]]\nTwo.\n\n[[fmt: sans +2]]\nThree.\n\n[[fmt: cursive]]\nFour.\n",
        );
        let src = |f: &str| {
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(f))
                .unwrap()
                // The Rust source spells the attribute's quotes \".
                .replace("\\\"", "\"")
        };
        let emitted = format!("{}{}{fmt_probe}", src("src/html.rs"), src("src/build.rs"));
        let sheet = common::strip_comments(&css());
        let selectors: Vec<String> = regex::Regex::new(r"([^{}]+)\{")
            .unwrap()
            .captures_iter(&sheet)
            .map(|c| c[1].to_owned())
            .collect();
        let class_re = regex::Regex::new(r"\.([a-z][a-z0-9-]*)").unwrap();
        let styled: std::collections::BTreeSet<String> = selectors
            .iter()
            .flat_map(|s| {
                class_re
                    .captures_iter(s)
                    .map(|c| c[1].to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(styled.len() > 12);
        // Format placeholders blanked first, so `class="action{fmt}"` reads
        // as `action`. (A placeholder holds no space or quote; a code block
        // always does, so blocks stay.)
        let blanked = regex::Regex::new(r#"\{[^{}\s"]*\}"#)
            .unwrap()
            .replace_all(&emitted, "");
        let emitted_classes: std::collections::BTreeSet<String> =
            regex::Regex::new(r#"class="([^"]*)""#)
                .unwrap()
                .captures_iter(&blanked)
                .flat_map(|c| {
                    c[1].split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect();
        for cls in &styled {
            assert!(emitted_classes.contains(cls), "no renderer emits .{cls}");
        }
    }
}

// ── tokensToPreviewHtml ───────────────────────────────────

mod tokens_to_preview_html_ {
    use super::*;

    const SRC: &str = "INT. KITCHEN - DAY\n\nA kettle screams.\n\nMARGO\nTurn it off.\n";

    #[test]
    fn emits_one_self_contained_document_with_inline_css() {
        let html = tokens_to_preview_html(
            &tokens(SRC),
            &opts(serde_json::json!({ "dialogueSideMarginPct": 25 })),
            &[],
        );
        assert!(html.contains("class=\"dialogue-block\""));
        assert!(html.contains("<style>"));
        assert!(html.contains("margin-left: 25%"));
        assert!(!html.contains("<link"));
    }

    #[test]
    fn defaults_options_when_none_given() {
        assert!(
            tokens_to_preview_html(&tokens(SRC), &DEFAULT_FORMAT_OPTIONS, &[])
                .contains("margin-left: 20%")
        );
    }

    #[test]
    fn empty_tokens_still_fall_back_to_a_styled_document() {
        assert!(tokens_to_preview_html(&[], &DEFAULT_FORMAT_OPTIONS, &[]).contains("<style>"));
    }

    #[test]
    fn a_picture_note_before_the_first_heading_adds_no_section_to_the_preview() {
        let of = |src: &str| tokens_to_preview_html(&tokens(src), &DEFAULT_FORMAT_OPTIONS, &[]);
        let script = "Title: T\n\nINT. KITCHEN - DAY\n\nA kettle screams.\n";
        let noted = of(&script.replacen("INT.", "[[img: p001-01.png w=100 full]]\n\nINT.", 1));
        assert_eq!(noted, of(script));
        assert_eq!(count(&noted, "<section class=\"scene\""), 1);
    }
}

// ── block font shifts (registry #18) ─────────────────────

mod font_shifts {
    use super::*;

    fn render(src: &str, format: &FormatOptions) -> String {
        body_of(src, format, &[]).files[0].xhtml.clone()
    }

    fn no_shifts() -> FormatOptions {
        opts(serde_json::json!({ "preserveFontShifts": false }))
    }

    #[test]
    fn a_leading_fmt_note_becomes_classes_on_the_action_paragraph() {
        assert!(
            render(
                "INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n",
                &DEFAULT_FORMAT_OPTIONS
            )
            .contains("<p class=\"action fmt-sans\">CHYRON: LIVE.</p>")
        );
    }

    #[test]
    fn family_and_size_both_land() {
        assert!(
            render(
                "INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n",
                &DEFAULT_FORMAT_OPTIONS
            )
            .contains("<p class=\"action fmt-sans fmt-plus2\">A SIGN.</p>")
        );
    }

    #[test]
    fn every_dialogue_paragraph_of_the_token_gets_the_classes() {
        let out = render(
            "INT. A - DAY\n\n@WREN\n[[fmt: mono -1]] one\ntwo\n",
            &DEFAULT_FORMAT_OPTIONS,
        );
        assert!(out.contains("<p class=\"dialogue fmt-mono fmt-minus1\">one</p>"));
        assert!(out.contains("<p class=\"dialogue fmt-mono fmt-minus1\">two</p>"));
    }

    #[test]
    fn with_the_knob_off_the_note_still_vanishes_but_adds_no_classes() {
        let out = render(
            "INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n",
            &no_shifts(),
        );
        assert!(out.contains("<p class=\"action\">CHYRON: LIVE.</p>"));
        assert!(!out.contains("fmt-sans"));
    }

    #[test]
    fn notes_are_invisible_everywhere_knob_or_no_knob() {
        for format in [DEFAULT_FORMAT_OPTIONS, no_shifts()] {
            let out = render(
                "INT. A - DAY\n\nHe leaves [[check this]] quickly.\n",
                &format,
            );
            assert!(out.contains("He leaves quickly."));
            assert!(!out.contains("[["));
            assert!(!out.contains("check this"));
        }
    }

    #[test]
    fn an_image_note_renders_nothing_in_every_shape_it_can_arrive_in() {
        for src in [
            "INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n",
            "INT. A - DAY\n\n@WREN\nHer line.\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n",
            "INT. A - DAY\n\n[[img: p001-01.png w=50]]\nSomeone waits.\n",
        ] {
            let out = render(src, &DEFAULT_FORMAT_OPTIONS);
            assert!(out.contains("Someone waits."), "{src}");
            assert!(!out.contains("img:"), "{src}");
            assert!(!out.contains("p001-01.png"), "{src}");
        }
    }

    #[test]
    fn a_malformed_fmt_note_strips_clean_and_adds_nothing() {
        let out = render(
            "INT. A - DAY\n\n[[fmt: chartreuse]]\nStill fine.\n",
            &DEFAULT_FORMAT_OPTIONS,
        );
        assert!(out.contains("<p class=\"action\">Still fine.</p>"));
        assert!(!out.contains("[["));
    }

    #[test]
    fn the_stylesheet_carries_all_seven_classes_in_em_and_css_2_1_syntax() {
        let css = screenplay_css(&DEFAULT_FORMAT_OPTIONS, false);
        for cls in [".fmt-mono", ".fmt-serif", ".fmt-sans", ".fmt-cursive"] {
            assert!(rule_for(&css, cls).contains("font-family"), "{cls}");
        }
        assert!(rule_for(&css, ".fmt-minus1").contains("font-size: 0.85em"));
        assert!(rule_for(&css, ".fmt-plus1").contains("font-size: 1.2em"));
        assert!(rule_for(&css, ".fmt-plus2").contains("font-size: 1.5em"));
        let rules = [
            ".fmt-mono",
            ".fmt-serif",
            ".fmt-sans",
            ".fmt-cursive",
            ".fmt-minus1",
            ".fmt-plus1",
            ".fmt-plus2",
        ]
        .iter()
        .map(|c| rule_for(&css, c))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(!rules.contains("max-width"));
        assert!(!rules.contains("line-height"));
        assert!(!matches(&rules, r"\b(min|max|clamp|var)\("));
    }
}

// ── the forward bind lives on the text, not on a wrapper ─────────────
//
// Device verdict 2026-09-14: cues stranded at page bottoms on a script sent
// via Send-to-Kindle (KFX). The KFX converter neither honors break-after on
// a structural div nor propagates a last child's break-after to its parent,
// so the wrapper went and the chain moved onto the text (#8b).

mod the_cue_chain_no_wrapper {
    use super::*;

    #[test]
    fn the_cue_is_a_direct_child_of_the_dialogue_block() {
        assert!(matches(
            &first_xhtml("INT. A - DAY\n\nHi.\n\n@JACK\nOnly line.\n"),
            r#"<div class="dialogue-block">\s*<p class="character">JACK</p>\s*<p class="dialogue">Only line\.</p>\s*</div>"#
        ));
    }

    #[test]
    fn cue_parenthetical_and_dialogue_are_siblings() {
        assert!(matches(
            &first_xhtml("INT. A - DAY\n\nHi.\n\n@JACK\n(tired)\nFirst line.\nSecond line.\n"),
            r#"<div class="dialogue-block">\s*<p class="character">JACK</p>\s*<p class="parenthetical">\(tired\)</p>\s*<p class="dialogue">First line\.</p>\s*<p class="dialogue">Second line\.</p>\s*</div>"#
        ));
    }

    #[test]
    fn no_keep_together_wrapper_is_emitted_anywhere() {
        assert!(
            !first_xhtml("INT. A - DAY\n\n@JACK\n(tired)\nSpeech.\n").contains("keep-together")
        );
    }

    #[test]
    fn the_stylesheet_no_longer_defines_the_dead_wrapper_class() {
        assert!(!css().contains(".keep-together"));
    }

    #[test]
    fn a_parenthetical_binds_forward_so_cue_plus_paren_cannot_strand() {
        let p = rule_for(&css(), "p.parenthetical");
        assert!(p.contains("page-break-after: avoid"));
        assert!(p.contains("break-after: avoid"));
    }

    #[test]
    fn the_cue_still_binds_forward_itself() {
        let c = rule_for(&css(), "p.character");
        assert!(c.contains("page-break-after: avoid"));
        assert!(c.contains("break-after: avoid"));
    }
}

// ── the column spelling of the FORWARD binds ─────────────────────────

mod the_column_spelling_of_the_forward_binds {
    use super::*;

    fn shadows(css: &str, prop: &str) -> Vec<String> {
        each_rule(css)
            .into_iter()
            .filter(|r| r.body.contains(prop))
            .map(|r| r.selector)
            .collect()
    }

    #[test]
    fn every_forward_bind_is_shadowed_in_the_multicol_spelling() {
        assert_eq!(
            shadows(&css(), "-webkit-column-break-after"),
            ["h2.scene-heading, p.mini-slug, p.character, p.parenthetical"]
        );
    }

    #[test]
    fn the_backward_bind_on_transitions_is_shadowed_too() {
        assert_eq!(
            shadows(&css(), "-webkit-column-break-before"),
            ["p.transition"]
        );
    }

    #[test]
    fn the_shadows_never_share_a_declaration_block_with_the_page_spelling() {
        // iBooks drops BOTH forms when they share one block.
        for r in each_rule(&css()) {
            if r.body.contains("-webkit-column-break") {
                assert!(!matches(&r.body, r"(^|[^-])break-"));
            }
        }
    }

    #[test]
    fn the_heading_shadow_follows_the_heading_gate() {
        let off = screenplay_css(
            &opts(serde_json::json!({ "keepSceneHeadingWithScene": false })),
            false,
        );
        assert_eq!(
            shadows(&off, "-webkit-column-break-after"),
            ["p.mini-slug, p.character, p.parenthetical"]
        );
    }
}

// The option enums are used by name in options.rs; touched here so a rename
// of either fails this file too.
#[test]
fn the_option_enums_the_stylesheet_reads() {
    let o = FormatOptions {
        cue_alignment: CueAlignment::Indented,
        font_family: FontFamily::Sans,
        ..DEFAULT_FORMAT_OPTIONS
    };
    let css = screenplay_css(&o, false);
    assert!(rule_for(&css, "body").contains("font-family: sans-serif;"));
    assert!(rule_for(&css, "p.character").contains("margin-left: 33%;"));
}
