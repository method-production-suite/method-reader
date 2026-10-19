//! Port of tests/torture.test.ts.
//!
//! Source-side assertions against the committed torture fixture. Device-side
//! behavior (keeps, orphan control, transitions never beginning a page) can
//! only be looked at on hardware: see `cargo xtask device-checklist`.
//!
//! The PDF is the oracle's dump of `tests/fixtures/torture.pdf`
//! (`common::extract("torture")`), and, as in the TypeScript, the text is
//! parsed ALONE: `parse_lines(lines, &[])`, never seeing a picture
//! (torture_pictures.rs owns those).
//!
//! The TS `beforeAll` also built `fountain = toFountain(doc)`, because note
//! PLACEMENT is a serializer fact. Those assertions moved:
//!
// MOVED: "a family shift becomes a note glued above its action block": asserts on
//   toFountain output; needs sp-fountain toFountain, ports with that crate.
// MOVED: "all three size steps appear, each on its own block": asserts on toFountain
//   output; needs sp-fountain toFountain, ports with that crate.
// MOVED: "a dialogue shift leads the line instead": asserts on toFountain output; needs
//   sp-fountain toFountain, ports with that crate.
// MOVED: "the body itself carries no notes": counts `[[fmt:` in toFountain output; needs
//   sp-fountain toFountain, ports with that crate.
// All four are in sp-engine/tests/torture.rs (they read the PDF through sp-pdf).

mod common;

use std::sync::OnceLock;

use sp_model::{ElementType, ParsedScreenplay, ScreenplayElement};
use sp_parser::parse_lines;

fn doc() -> &'static ParsedScreenplay {
    static DOC: OnceLock<ParsedScreenplay> = OnceLock::new();
    DOC.get_or_init(|| {
        let extracted = common::extract("torture");
        parse_lines(&extracted.lines, &[])
    })
}

fn styled_all() -> &'static str {
    static STYLED: OnceLock<String> = OnceLock::new();
    STYLED.get_or_init(|| {
        doc()
            .elements
            .iter()
            .map(|e| e.styled_text.as_deref().unwrap_or(&e.text))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn find(pred: impl Fn(&ScreenplayElement) -> bool) -> Option<&'static ScreenplayElement> {
    doc().elements.iter().find(|e| pred(e))
}

mod the_proof_sheet_parses {
    use super::*;

    #[test]
    fn every_content_element_type_the_parser_knows_appears_at_least_once() {
        let seen: Vec<ElementType> = doc().elements.iter().map(|e| e.kind).collect();
        let required = [
            ElementType::Scene,
            ElementType::Character,
            ElementType::Dialogue,
            ElementType::Action,
            ElementType::Parenthetical,
            ElementType::Transition,
            ElementType::MiniSlug,
        ];
        for t in required {
            assert!(seen.contains(&t), "{t:?} missing");
        }
    }

    #[test]
    fn the_saturation_cast_is_the_roster_and_the_title_is_not_in_it() {
        let names: Vec<&str> = doc().characters.iter().map(|c| c.name.as_str()).collect();
        for who in ["BUNNY", "CASSIUS", "ODILE", "WREN"] {
            assert!(names.contains(&who), "{who} missing from {names:?}");
        }
        // A centered title lands in the cue indent band; detectTitlePages flags
        // it and the roster must honor the flag.
        assert!(!names.contains(&"THE PROOF SHEET"));
    }

    #[test]
    fn a_dotted_abbreviation_name_is_a_character_not_action() {
        assert!(
            doc()
                .characters
                .iter()
                .any(|c| c.name == "DR. E. T. MARCHETTI")
        );
    }

    #[test]
    fn the_dual_exchange_marks_its_right_column() {
        assert!(doc().elements.iter().any(|e| e.dual_right == Some(true)));
    }

    // Sheet 9's second exchange (ODILE left, WREN right): the tall one, there
    // to reach the renderer's sequential fallback (registry #10b). Each
    // column's full speech as printed, its lines joined.
    const TALL_LEFT: &str = "This column runs long enough that the taller of the two passes \
        twelve estimated rendered lines, which is the threshold where the side-by-side table \
        gives up and both speeches become ordinary sequential dialogue instead.";
    const TALL_RIGHT: &str = "And this one answers at similar length so neither column is \
        trivially short, because the fallback measures the taller of the pair and a stub \
        would never reach it.";

    #[test]
    fn no_element_mixes_text_from_both_dual_columns() {
        // The first exchange carries explicit markers. The second carries
        // none, so it is held by words that each sit in one column only.
        let mixed: Vec<&ScreenplayElement> = doc()
            .elements
            .iter()
            .filter(|e| {
                (e.text.contains("LEFTMARK") && e.text.contains("RIGHTMARK"))
                    || (["column runs long", "speeches become ordinary"]
                        .iter()
                        .any(|w| e.text.contains(w))
                        && ["answers at", "a stub would"]
                            .iter()
                            .any(|w| e.text.contains(w)))
            })
            .collect();
        assert_eq!(mixed, Vec::<&ScreenplayElement>::new());
    }

    #[test]
    fn the_tall_dual_exchange_is_two_whole_speeches() {
        let els = &doc().elements;
        let at = els
            .iter()
            .position(|e| e.text.starts_with("This column runs long"))
            .expect("the tall exchange's left speech");
        let kinds: Vec<(ElementType, &str, Option<bool>)> = els[at - 1..at + 3]
            .iter()
            .map(|e| (e.kind, e.text.as_str(), e.dual_right))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (ElementType::Character, "ODILE", None),
                (ElementType::Dialogue, TALL_LEFT, None),
                (ElementType::Character, "WREN", Some(true)),
                (ElementType::Dialogue, TALL_RIGHT, None),
            ]
        );
        // Nothing of the exchange is left over as action before the next
        // scene heading (the page break and the keep-test speeches follow).
        let stray: Vec<&ScreenplayElement> = els[at + 3..]
            .iter()
            .take_while(|e| e.kind != ElementType::Scene)
            .filter(|e| e.kind == ElementType::Action)
            .collect();
        assert_eq!(stray, Vec::<&ScreenplayElement>::new());
    }

    #[test]
    fn the_dual_margin_scene_number_is_attached_not_left_in_the_heading() {
        let el = find(|e| e.scene_number.as_deref() == Some("42"));
        assert!(el.is_some());
        let el = el.unwrap();
        assert_eq!(el.kind, ElementType::Scene);
        assert!(!el.text.contains("42"));
    }

    #[test]
    fn mini_slugs_are_micro_headings_not_scenes() {
        let mini: Vec<&str> = doc()
            .elements
            .iter()
            .filter(|e| e.kind == ElementType::MiniSlug)
            .map(|e| e.text.as_str())
            .collect();
        assert!(mini.contains(&"INDEX CARDS"));
        // A mini-slug must NOT become a scene: it serializes as a forced
        // slugline, and a scene would mint a section and a TOC entry.
        assert!(!doc().scenes.iter().any(|s| s.heading == "INDEX CARDS"));
    }

    #[test]
    fn cue_extensions_are_tolerated_with_and_without_their_closing_period() {
        let parens: Vec<&str> = doc()
            .elements
            .iter()
            .filter(|e| e.kind == ElementType::Parenthetical)
            .map(|e| e.text.as_str())
            .collect();
        assert!(parens.contains(&"(O.S.)"));
        assert!(parens.contains(&"(V.O)"));
        assert!(parens.contains(&"(O.C)"));
    }

    #[test]
    fn the_long_speech_really_does_cross_a_page_boundary() {
        // If it stopped fitting on one page, the (MORE)/(CONT'D) rejoin is no
        // longer being exercised and this fixture silently stops proving it.
        let anchor = find(|e| e.text.contains("MORE-ANCHOR"));
        assert!(anchor.is_some());
        let anchor = anchor.unwrap();
        let anchor_page = anchor.page_num;
        let last_of_speech = doc()
            .elements
            .iter()
            .rfind(|e| e.kind == ElementType::Dialogue && e.character == anchor.character);
        assert!(last_of_speech.unwrap().page_num >= anchor_page);
    }
}

mod rich_formatting {
    use super::*;

    #[test]
    fn bold_italic_and_bold_italic_all_reach_styled_text() {
        assert!(styled_all().contains("**DO NOT LAMINATE**"));
        assert!(styled_all().contains("*again*"));
        assert!(styled_all().contains("***both bold and italic***"));
    }

    #[test]
    fn a_styled_run_keeps_its_emphasis_across_a_wrap_boundary() {
        let el = find(|e| e.text.contains("crosses the wrap boundary"));
        assert!(el.is_some());
        assert!(
            el.unwrap()
                .styled_text
                .as_deref()
                .is_some_and(|s| s.contains("**"))
        );
    }

    #[test]
    fn a_punctuation_only_styled_item_never_wraps_alone() {
        // Registry 9d: a lone styled comma must not become "*,*".
        assert!(!styled_all().contains("**,**"));
        assert!(!styled_all().contains("*,*"));
    }

    #[test]
    fn a_drawn_underline_reaches_styled_text_as_underscores() {
        let el = find(|e| e.text.contains("underlined"));
        assert!(el.is_some());
        let el = el.unwrap();
        // The plain text must still be clean: classification never sees markers.
        assert_eq!(el.text, "This word is underlined with drawn vector art.");
        assert_eq!(
            el.styled_text.as_deref(),
            Some("This word is _underlined_ with drawn vector art.")
        );
    }

    // Three rules the detector must REJECT, each caught by a different filter,
    // each drawn beside the real underline on the same page. A loosened
    // threshold shows up here as a stray underscore rather than as silence.
    fn styled_or_text(el: &ScreenplayElement) -> &str {
        el.styled_text.as_deref().unwrap_or(&el.text)
    }

    #[test]
    fn a_strikethrough_is_not_an_underline() {
        let el = find(|e| e.text.contains("struck through"));
        assert!(el.is_some());
        assert!(!styled_or_text(el.unwrap()).contains('_'));
    }

    #[test]
    fn a_table_border_below_the_text_band_is_not_an_underline() {
        let el = find(|e| e.text.contains("below this cell"));
        assert!(el.is_some());
        assert!(!styled_or_text(el.unwrap()).contains('_'));
    }

    #[test]
    fn a_page_wide_rule_is_furniture_not_an_underline() {
        let el = find(|e| e.text.contains("this whole line"));
        assert!(el.is_some());
        assert!(!styled_or_text(el.unwrap()).contains('_'));
    }
}

mod defects_this_fixture_surfaced {
    use super::*;

    // FIXED. This fixture found it: attachSceneNumbers used to promote any
    // page-number element whose text matched SCENE_NUMBER, and a bare "1."
    // matches, so every page OPENING on a scene heading donated its page
    // number to that scene. screenplay.pdf never exposed it because its pages
    // start mid-scene. classifyBlock now marks the shooting-script case at
    // classification time, where the two are still distinguishable.
    #[test]
    fn a_page_number_is_not_absorbed_as_a_scene_number() {
        let openers: Vec<&ScreenplayElement> = doc()
            .elements
            .iter()
            .filter(|e| e.kind == ElementType::Scene && e.text == "INT. ARCHIVE BASEMENT - NIGHT")
            .collect();
        assert!(!openers.is_empty());
        for scene in openers {
            assert_eq!(scene.scene_number, None);
        }
    }

    #[test]
    fn the_genuine_shooting_script_number_still_attaches() {
        let el = find(|e| e.scene_number.as_deref() == Some("42"));
        assert!(el.is_some());
        assert_eq!(el.unwrap().kind, ElementType::Scene);
    }
}

mod block_font_shifts {
    use super::*;
    use sp_model::{FamilyBucket, Fmt};

    // The four toFountain tests of this block moved: see the file header.

    #[test]
    fn a_shifted_block_never_merges_with_the_plain_action_around_it() {
        let chyron = find(|e| e.text.starts_with("CHYRON:"));
        assert!(chyron.is_some());
        let chyron = chyron.unwrap();
        assert_eq!(chyron.kind, ElementType::Action);
        assert_eq!(chyron.text, "CHYRON: EVERY NAME HERE IS INVENTED.");
        assert_eq!(
            chyron.fmt,
            Some(Fmt {
                family: Some(FamilyBucket::Sans),
                size: None
            })
        );
    }
}
