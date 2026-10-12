//! Port of the sp-epub blocks of tests/options.test.ts: `screenplayCss with
//! options`, `tokensToBody with options`, `buildEpub with options` and
//! `cueAlignment`. The file's other blocks are ported by sp-model
//! (resolveFormatOptions, keepSpeechesWhole), sp-fountain (the serializer's
//! rejoin, contdMode, showPageMarkers and dualDialogue) and sp-engine
//! (convertFountain); rust/PARITY.md has the split.

mod common;

use common::{each_rule, entry_text, matches, rule_for, tokens};
use sp_epub::{BodyOptions, BookMeta, build_epub, screenplay_css, tokens_to_body};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, resolve_format_options};

const SAMPLE: &str = "INT. KITCHEN - DAY #7#

Jack enters.

@JACK
Hello.

EXT. YARD - NIGHT

Stars.
";

fn opts(json: serde_json::Value) -> FormatOptions {
    resolve_format_options(Some(&json), &DEFAULT_FORMAT_OPTIONS)
}

fn sheet(json: serde_json::Value) -> String {
    screenplay_css(&opts(json), false)
}

fn body_xhtml(format: FormatOptions) -> String {
    tokens_to_body(
        &tokens(SAMPLE),
        &BodyOptions {
            format,
            ..BodyOptions::default()
        },
    )
    .files[0]
        .xhtml
        .clone()
}

mod screenplay_css_with_options {
    use super::*;
    use serde_json::json;

    #[test]
    fn dialogue_geometry_knobs_land_in_the_stylesheet() {
        let css = sheet(json!({
            "cueAlignment": "indented",
            "dialogueSideMarginPct": 25,
            "cueIndentPct": 40,
            "parentheticalIndentPct": 12,
        }));
        assert!(css.contains("margin-left: 25%"));
        assert!(css.contains("margin-right: 25%"));
        assert!(rule_for(&css, "p.character").contains("margin-left: 40%"));
        assert!(rule_for(&css, "p.parenthetical").contains("margin-left: 12%"));
    }

    #[test]
    fn element_spacing_scales_the_vertical_rhythm() {
        assert!(
            rule_for(&sheet(json!({ "elementSpacingEm": 0.6 })), "p.action")
                .contains("margin: 0.6em 0")
        );
    }

    #[test]
    fn scene_page_breaks_add_a_break_rule_only_when_enabled() {
        assert!(!sheet(json!({})).contains("section.scene { page-break-before"));
        assert!(
            sheet(json!({ "scenePageBreaks": true }))
                .contains("section.scene { page-break-before: always; }")
        );
    }

    #[test]
    fn keep_speeches_whole_makes_dialogue_blocks_unbreakable_only_when_enabled() {
        assert!(!rule_for(&sheet(json!({})), ".dialogue-block").contains("break-inside"));
        let block = rule_for(
            &sheet(json!({ "keepSpeechesWhole": true })),
            ".dialogue-block",
        );
        assert!(block.contains("page-break-inside: avoid"));
        assert!(block.contains("break-inside: avoid"));
    }

    #[test]
    fn font_family_option_switches_the_body_stack() {
        assert!(matches(
            &sheet(json!({ "fontFamily": "serif" })),
            r"body\s*\{[^}]*font-family:\s*serif"
        ));
        assert!(sheet(json!({ "fontFamily": "courier" })).contains("\"Courier Prime\""));
    }

    #[test]
    fn body_text_is_ragged_right_by_default_justified_when_enabled() {
        let ragged = sheet(json!({}));
        assert!(rule_for(&ragged, "p.action").contains("text-align: left"));
        assert!(rule_for(&ragged, "p.dialogue").contains("text-align: left"));
        let justified = sheet(json!({ "justifyText": true }));
        assert!(rule_for(&justified, "p.action").contains("text-align: justify"));
        assert!(rule_for(&justified, "p.dialogue").contains("text-align: justify"));
    }

    #[test]
    fn print_split_minimums_controls_widows_orphans_on_dialogue_and_action() {
        // The trailing semicolon keeps `widows: 2` from matching
        // `widows: 20`.
        for (partial, n) in [(json!({}), 2), (json!({ "printSplitMinimums": false }), 1)] {
            let css = sheet(partial);
            for sel in ["p.action", "p.dialogue"] {
                let rule = rule_for(&css, sel);
                assert!(rule.contains(&format!("widows: {n};")), "{sel}");
                assert!(rule.contains(&format!("orphans: {n};")), "{sel}");
            }
        }
    }

    #[test]
    fn wrapper_keeps_carry_the_column_spelling_in_a_separate_rule() {
        let css = screenplay_css(&DEFAULT_FORMAT_OPTIONS, false);
        // The shadow shares its selector with the rule it shadows and is
        // emitted second, so it is addressed by position.
        let both: Vec<_> = each_rule(&css)
            .into_iter()
            .filter(|r| r.selector == "table.dual-dialogue")
            .collect();
        assert_eq!(both.len(), 2);
        assert!(both[1].body.contains("-webkit-column-break-inside: avoid"));
        // iBooks ignores BOTH spellings when they share a block.
        assert!(!both[1].body.contains("page-break-inside"));
    }

    #[test]
    fn the_column_spelling_tracks_the_keep_set_keep_speeches_whole_joins_it() {
        let off = sheet(json!({}));
        assert!(off.contains("table.dual-dialogue { -webkit-column-break-inside: avoid; }"));
        assert!(!off.contains(".dialogue-block { -webkit-column-break-inside"));
        let on = sheet(json!({ "keepSpeechesWhole": true }));
        let rule = rule_for(&on, "table.dual-dialogue, .dialogue-block");
        assert!(rule.contains("-webkit-column-break-inside: avoid"));
        assert!(!rule.contains("page-break-inside"));
    }

    #[test]
    fn page_marker_dims_via_opacity_not_a_hardcoded_gray() {
        let rule = rule_for(
            &screenplay_css(&DEFAULT_FORMAT_OPTIONS, false),
            "span.page-marker",
        );
        assert!(rule.contains("opacity: 0.6"));
        assert!(!rule.contains("#777777"));
    }
}

mod tokens_to_body_with_options {
    use super::*;
    use serde_json::json;

    #[test]
    fn keep_scene_heading_with_scene_gates_the_heading_break_after_rule() {
        let heading = rule_for(&sheet(json!({})), "h2.scene-heading");
        assert!(heading.contains("page-break-after: avoid"));
        assert!(matches(&heading, r"[^-]break-after: avoid"));
        let off = sheet(json!({ "keepSceneHeadingWithScene": false }));
        assert!(!rule_for(&off, "h2.scene-heading").contains("break-after"));
        // The markup carries no wrapper either way.
        for format in [
            opts(json!({})),
            opts(json!({ "keepSceneHeadingWithScene": false })),
        ] {
            assert!(!matches(
                &body_xhtml(format),
                r#"<div class="keep-together">\s*<h2"#
            ));
        }
    }

    #[test]
    fn scene_numbers_render_in_headings_when_enabled() {
        assert!(body_xhtml(opts(json!({ "showSceneNumbers": true }))).contains(
            "<h2 class=\"scene-heading\"><span class=\"scene-number\">7</span> INT. KITCHEN - DAY</h2>"
        ));
        assert!(
            body_xhtml(opts(json!({})))
                .contains("<h2 class=\"scene-heading\">INT. KITCHEN - DAY</h2>")
        );
    }
}

mod build_epub_with_options {
    use super::*;
    use serde_json::json;

    #[test]
    fn title_page_can_be_excluded_from_manifest_spine_and_landmarks() {
        let body = tokens_to_body(&tokens(SAMPLE), &BodyOptions::default());
        let epub = build_epub(
            &BookMeta {
                title: "T".to_owned(),
                ..BookMeta::default()
            },
            &body,
            &opts(json!({ "includeTitlePage": false })),
            &[],
            1_767_225_600,
        )
        .unwrap();
        assert!(entry_text(&epub, "OEBPS/titlepage.xhtml").is_none());
        assert!(
            !entry_text(&epub, "OEBPS/package.opf")
                .unwrap()
                .contains("titlepage")
        );
        assert!(
            !entry_text(&epub, "OEBPS/nav.xhtml")
                .unwrap()
                .contains("titlepage")
        );
    }
}

mod cue_alignment {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_centers_cues_and_parentheticals_in_the_column() {
        let css = sheet(json!({}));
        assert!(rule_for(&css, "p.character").contains("text-align: center"));
        assert!(rule_for(&css, "p.parenthetical").contains("text-align: center"));
    }

    #[test]
    fn indented_mode_uses_print_style_percent_indents_instead() {
        let cue = rule_for(
            &sheet(json!({ "cueAlignment": "indented", "cueIndentPct": 40 })),
            "p.character",
        );
        assert!(cue.contains("margin-left: 40%"));
        assert!(!cue.contains("text-align: center"));
    }
}
